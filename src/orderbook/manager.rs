use dashmap::DashMap;
use ordered_float::OrderedFloat;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use parking_lot::RwLock;

pub type Price = OrderedFloat<f64>;
pub type Quantity = OrderedFloat<f64>;

#[derive(Debug, Clone)]
pub struct OrderbookLevel {
    pub price: Price,
    pub quantity: Quantity,
    pub timestamp: u64,
}

pub struct Orderbook {
    pub symbol: String,
    
    // Lock-free concurrent hashmaps
    bids: Arc<DashMap<Price, Quantity>>,
    asks: Arc<DashMap<Price, Quantity>>,
    
    // Sorted views (updated periodically)
    sorted_bids: Arc<RwLock<Vec<OrderbookLevel>>>,
    sorted_asks: Arc<RwLock<Vec<OrderbookLevel>>>,
    
    // Best bid/ask (atomic)
    best_bid: Arc<AtomicU64>,
    best_ask: Arc<AtomicU64>,
    
    // Metrics
    last_update_time: Arc<AtomicU64>,
    update_count: Arc<AtomicU64>,
}

impl Orderbook {
    pub fn new(symbol: String) -> Self {
        Self {
            symbol,
            bids: Arc::new(DashMap::new()),
            asks: Arc::new(DashMap::new()),
            sorted_bids: Arc::new(RwLock::new(Vec::new())),
            sorted_asks: Arc::new(RwLock::new(Vec::new())),
            best_bid: Arc::new(AtomicU64::new(0)),
            best_ask: Arc::new(AtomicU64::new(u64::MAX)),
            last_update_time: Arc::new(AtomicU64::new(0)),
            update_count: Arc::new(AtomicU64::new(0)),
        }
    }
    
    /// Process orderbook snapshot (full replace)
    pub fn apply_snapshot(&self, bids: Vec<(f64, f64)>, asks: Vec<(f64, f64)>) {
        let start = std::time::Instant::now();
        
        // Clear existing
        self.bids.clear();
        self.asks.clear();
        
        // Insert bids
        for (price, qty) in bids {
            if qty > 0.0 {
                self.bids.insert(Price::from(price), Quantity::from(qty));
            }
        }
        
        // Insert asks
        for (price, qty) in asks {
            if qty > 0.0 {
                self.asks.insert(Price::from(price), Quantity::from(qty));
            }
        }
        
        // Update sorted views and best prices
        self.update_sorted_views();
        
        let elapsed = start.elapsed().as_micros();
        if elapsed > 100 {
            tracing::warn!("Slow snapshot processing: {}μs", elapsed);
        }
        
        self.last_update_time.store(
            chrono::Utc::now().timestamp_millis() as u64,
            Ordering::Relaxed
        );
        self.update_count.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Process orderbook delta (incremental update)
    pub fn apply_delta(&self, bids: Vec<(f64, f64)>, asks: Vec<(f64, f64)>) {
        let start = std::time::Instant::now();
        
        let mut best_bid_changed = false;
        let mut best_ask_changed = false;
        
        // Update bids
        for (price, qty) in bids {
            let price = Price::from(price);
            
            if qty == 0.0 {
                self.bids.remove(&price);
                best_bid_changed = true;
            } else {
                self.bids.insert(price, Quantity::from(qty));
                
                // Check if this is new best bid
                let current_best = f64::from_bits(
                    self.best_bid.load(Ordering::Relaxed)
                );
                if price.0 > current_best {
                    best_bid_changed = true;
                }
            }
        }
        
        // Update asks
        for (price, qty) in asks {
            let price = Price::from(price);
            
            if qty == 0.0 {
                self.asks.remove(&price);
                best_ask_changed = true;
            } else {
                self.asks.insert(price, Quantity::from(qty));
                
                let current_best = f64::from_bits(
                    self.best_ask.load(Ordering::Relaxed)
                );
                if price.0 < current_best {
                    best_ask_changed = true;
                }
            }
        }
        
        // Only update sorted views if best prices changed
        if best_bid_changed || best_ask_changed {
            self.update_sorted_views();
        }
        
        let elapsed = start.elapsed().as_micros();
        if elapsed > 50 {
            tracing::warn!("Slow delta processing: {}μs", elapsed);
        }
        
        self.last_update_time.store(
            chrono::Utc::now().timestamp_millis() as u64,
            Ordering::Relaxed
        );
    }
    
    fn update_sorted_views(&self) {
        let now = chrono::Utc::now().timestamp_millis() as u64;
        
        // Bids (highest to lowest)
        let mut bids: Vec<_> = self.bids.iter()
            .map(|entry| OrderbookLevel {
                price: *entry.key(),
                quantity: *entry.value(),
                timestamp: now,
            })
            .collect();
        
        bids.sort_by(|a, b| b.price.cmp(&a.price));
        
        // Update best bid (atomic)
        if let Some(best) = bids.first() {
            self.best_bid.store(best.price.0.to_bits(), Ordering::Relaxed);
        }
        
        *self.sorted_bids.write() = bids;
        
        // Asks (lowest to highest)
        let mut asks: Vec<_> = self.asks.iter()
            .map(|entry| OrderbookLevel {
                price: *entry.key(),
                quantity: *entry.value(),
                timestamp: now,
            })
            .collect();
        
        asks.sort_by(|a, b| a.price.cmp(&b.price));
        
        // Update best ask (atomic)
        if let Some(best) = asks.first() {
            self.best_ask.store(best.price.0.to_bits(), Ordering::Relaxed);
        }
        
        *self.sorted_asks.write() = asks;
    }
    
    /// Get best bid/ask (lock-free read)
    pub fn best_bid_ask(&self) -> (f64, f64) {
        let bid = f64::from_bits(self.best_bid.load(Ordering::Relaxed));
        let ask = f64::from_bits(self.best_ask.load(Ordering::Relaxed));
        (bid, ask)
    }
    
    /// Get mid price
    #[inline]
    pub fn mid_price(&self) -> f64 {
        let (bid, ask) = self.best_bid_ask();
        (bid + ask) / 2.0
    }
    
    /// Get spread percentage
    #[inline]
    pub fn spread_pct(&self) -> f64 {
        let (bid, ask) = self.best_bid_ask();
        if bid == 0.0 { return 1.0; }
        (ask - bid) / self.mid_price()
    }
    
    /// Calculate imbalance (top N levels)
    pub fn imbalance(&self, depth: usize) -> f64 {
        let bids = self.sorted_bids.read();
        let asks = self.sorted_asks.read();
        
        let bid_volume: f64 = bids.iter()
            .take(depth)
            .map(|level| level.quantity.0)
            .sum();
        
        let ask_volume: f64 = asks.iter()
            .take(depth)
            .map(|level| level.quantity.0)
            .sum();
        
        if bid_volume + ask_volume == 0.0 {
            return 0.0;
        }
        
        (bid_volume - ask_volume) / (bid_volume + ask_volume)
    }
    
    /// Get total liquidity in top N levels
    pub fn liquidity_depth(&self, depth: usize) -> f64 {
        let bids = self.sorted_bids.read();
        let asks = self.sorted_asks.read();
        
        let bid_volume: f64 = bids.iter()
            .take(depth)
            .map(|level| level.quantity.0)
            .sum();
        
        let ask_volume: f64 = asks.iter()
            .take(depth)
            .map(|level| level.quantity.0)
            .sum();
        
        bid_volume + ask_volume
    }
    
    /// Get latency since last update
    pub fn latency_ms(&self) -> u64 {
        let last_update = self.last_update_time.load(Ordering::Relaxed);
        let now = chrono::Utc::now().timestamp_millis() as u64;
        now.saturating_sub(last_update)
    }
    
    /// Get update count
    pub fn update_count(&self) -> u64 {
        self.update_count.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_orderbook_snapshot() {
        let ob = Orderbook::new("BTCUSDT".to_string());
        
        let bids = vec![(50000.0, 1.5), (49999.0, 2.0)];
        let asks = vec![(50001.0, 1.3), (50002.0, 1.8)];
        
        ob.apply_snapshot(bids, asks);
        
        let (bid, ask) = ob.best_bid_ask();
        assert_eq!(bid, 50000.0);
        assert_eq!(ask, 50001.0);
    }
    
    #[test]
    fn test_imbalance_calculation() {
        let ob = Orderbook::new("BTCUSDT".to_string());
        
        let bids = vec![(50000.0, 10.0), (49999.0, 5.0)];
        let asks = vec![(50001.0, 2.0), (50002.0, 3.0)];
        
        ob.apply_snapshot(bids, asks);
        
        let imbalance = ob.imbalance(2);
        assert!(imbalance > 0.0); // More bids than asks
    }
}
