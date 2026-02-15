use std::collections::{HashMap, VecDeque};
use std::time::{SystemTime, UNIX_EPOCH};

/// Snapshot of orderbook volume at a specific time
#[derive(Debug, Clone)]
pub struct VolumeSnapshot {
    pub timestamp_ms: u64,
    pub bid_volume: f64,
    pub ask_volume: f64,
    pub total_volume: f64,
}

/// Large order detected in the orderbook
#[derive(Debug, Clone)]
pub struct LargeOrder {
    pub price: f64,
    pub size: f64,
    pub side: OrderSide,
    pub timestamp_ms: u64,
    pub size_multiplier: f64, // How many times larger than average
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderSide {
    Bid,
    Ask,
}

/// Advanced metrics calculator for orderbook analysis
pub struct OrderbookMetrics {
    /// Historical volume snapshots (last 60 seconds)
    volume_history: VecDeque<VolumeSnapshot>,

    /// Maximum history size (60 snapshots = 60 seconds at 1Hz)
    max_history_size: usize,

    /// Imbalance calculated at different depths
    imbalance_by_depth: HashMap<usize, f64>,

    /// Average order size across all levels
    avg_order_size: f64,

    /// Recently detected large orders
    large_orders: VecDeque<LargeOrder>,

    /// Maximum large orders to track
    max_large_orders: usize,

    /// Bid pressure (rate of change in bid volume)
    bid_pressure: f64,

    /// Ask pressure (rate of change in ask volume)
    ask_pressure: f64,

    /// Previous best bid for tracking pressure
    prev_best_bid: f64,

    /// Previous best ask for tracking pressure
    prev_best_ask: f64,

    /// Previous timestamp for pressure calculation
    prev_timestamp_ms: u64,
}

impl OrderbookMetrics {
    pub fn new() -> Self {
        Self {
            volume_history: VecDeque::new(),
            max_history_size: 60, // 60 seconds of history
            imbalance_by_depth: HashMap::new(),
            avg_order_size: 0.0,
            large_orders: VecDeque::new(),
            max_large_orders: 20,
            bid_pressure: 0.0,
            ask_pressure: 0.0,
            prev_best_bid: 0.0,
            prev_best_ask: 0.0,
            prev_timestamp_ms: 0,
        }
    }

    /// Add a volume snapshot to history
    pub fn add_snapshot(&mut self, bid_volume: f64, ask_volume: f64) {
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let snapshot = VolumeSnapshot {
            timestamp_ms,
            bid_volume,
            ask_volume,
            total_volume: bid_volume + ask_volume,
        };

        self.volume_history.push_back(snapshot);

        // Remove old snapshots
        while self.volume_history.len() > self.max_history_size {
            self.volume_history.pop_front();
        }
    }

    /// Calculate volume delta over a time window
    /// Returns the change in total volume over the specified window
    pub fn calculate_volume_delta(&self, window_ms: u64) -> f64 {
        if self.volume_history.len() < 2 {
            return 0.0;
        }

        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let cutoff_time = now_ms.saturating_sub(window_ms);

        // Get current volume
        let current = self.volume_history.back().unwrap();

        // Find oldest snapshot within window
        let old_snapshot = self.volume_history.iter()
            .find(|s| s.timestamp_ms >= cutoff_time)
            .or_else(|| self.volume_history.front());

        if let Some(old) = old_snapshot {
            let delta = current.total_volume - old.total_volume;
            let time_diff = (current.timestamp_ms - old.timestamp_ms) as f64 / 1000.0;

            if time_diff > 0.0 {
                // Return delta per second
                delta / time_diff
            } else {
                0.0
            }
        } else {
            0.0
        }
    }

    /// Calculate bid and ask volume deltas separately
    pub fn calculate_side_volume_deltas(&self, window_ms: u64) -> (f64, f64) {
        if self.volume_history.len() < 2 {
            return (0.0, 0.0);
        }

        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let cutoff_time = now_ms.saturating_sub(window_ms);

        let current = self.volume_history.back().unwrap();
        let old_snapshot = self.volume_history.iter()
            .find(|s| s.timestamp_ms >= cutoff_time)
            .or_else(|| self.volume_history.front());

        if let Some(old) = old_snapshot {
            let bid_delta = current.bid_volume - old.bid_volume;
            let ask_delta = current.ask_volume - old.ask_volume;
            let time_diff = (current.timestamp_ms - old.timestamp_ms) as f64 / 1000.0;

            if time_diff > 0.0 {
                (bid_delta / time_diff, ask_delta / time_diff)
            } else {
                (0.0, 0.0)
            }
        } else {
            (0.0, 0.0)
        }
    }

    /// Update average order size based on current orderbook levels
    pub fn update_avg_order_size(&mut self, levels: &[(f64, f64)]) {
        if levels.is_empty() {
            return;
        }

        let total: f64 = levels.iter().map(|(_, qty)| qty).sum();
        self.avg_order_size = total / levels.len() as f64;
    }

    /// Detect whale orders (orders significantly larger than average)
    pub fn detect_whales(
        &mut self,
        bid_levels: &[(f64, f64)],
        ask_levels: &[(f64, f64)],
        threshold_multiplier: f64,
    ) -> Vec<LargeOrder> {
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let mut whales = Vec::new();

        if self.avg_order_size == 0.0 {
            return whales;
        }

        let threshold = self.avg_order_size * threshold_multiplier;

        // Check bid side
        for (price, qty) in bid_levels {
            if *qty > threshold {
                let whale = LargeOrder {
                    price: *price,
                    size: *qty,
                    side: OrderSide::Bid,
                    timestamp_ms,
                    size_multiplier: qty / self.avg_order_size,
                };
                whales.push(whale.clone());
                self.large_orders.push_back(whale);
            }
        }

        // Check ask side
        for (price, qty) in ask_levels {
            if *qty > threshold {
                let whale = LargeOrder {
                    price: *price,
                    size: *qty,
                    side: OrderSide::Ask,
                    timestamp_ms,
                    size_multiplier: qty / self.avg_order_size,
                };
                whales.push(whale.clone());
                self.large_orders.push_back(whale);
            }
        }

        // Trim old large orders
        while self.large_orders.len() > self.max_large_orders {
            self.large_orders.pop_front();
        }

        whales
    }

    /// Calculate imbalance at multiple depth levels
    pub fn calculate_multi_level_imbalance(
        &mut self,
        bid_levels: &[(f64, f64)],
        ask_levels: &[(f64, f64)],
        depths: &[usize],
    ) -> HashMap<usize, f64> {
        let mut imbalances = HashMap::new();

        for &depth in depths {
            let bid_volume: f64 = bid_levels.iter()
                .take(depth)
                .map(|(_, qty)| qty)
                .sum();

            let ask_volume: f64 = ask_levels.iter()
                .take(depth)
                .map(|(_, qty)| qty)
                .sum();

            let total = bid_volume + ask_volume;
            let imbalance = if total > 0.0 {
                (bid_volume - ask_volume) / total
            } else {
                0.0
            };

            imbalances.insert(depth, imbalance);
        }

        self.imbalance_by_depth = imbalances.clone();
        imbalances
    }

    /// Calculate bid/ask pressure (velocity of price changes)
    pub fn calculate_pressure(&mut self, best_bid: f64, best_ask: f64) -> (f64, f64) {
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        if self.prev_timestamp_ms == 0 {
            self.prev_best_bid = best_bid;
            self.prev_best_ask = best_ask;
            self.prev_timestamp_ms = timestamp_ms;
            return (0.0, 0.0);
        }

        let time_diff = (timestamp_ms - self.prev_timestamp_ms) as f64 / 1000.0;

        if time_diff > 0.0 {
            // Calculate price velocity
            let bid_change = best_bid - self.prev_best_bid;
            let ask_change = best_ask - self.prev_best_ask;

            self.bid_pressure = bid_change / time_diff;
            self.ask_pressure = ask_change / time_diff;

            self.prev_best_bid = best_bid;
            self.prev_best_ask = best_ask;
            self.prev_timestamp_ms = timestamp_ms;
        }

        (self.bid_pressure, self.ask_pressure)
    }

    /// Calculate depth consistency
    /// Measures how consistent the imbalance is across different depth levels
    /// Returns a score from 0.0 (inconsistent) to 1.0 (highly consistent)
    pub fn depth_consistency(&self) -> f64 {
        if self.imbalance_by_depth.len() < 2 {
            return 0.0;
        }

        let imbalances: Vec<f64> = self.imbalance_by_depth.values().copied().collect();

        // Calculate standard deviation
        let mean = imbalances.iter().sum::<f64>() / imbalances.len() as f64;
        let variance = imbalances.iter()
            .map(|x| (x - mean).powi(2))
            .sum::<f64>() / imbalances.len() as f64;

        let std_dev = variance.sqrt();

        // Convert std dev to consistency score (lower std dev = higher consistency)
        // Using exponential decay: consistency = e^(-k * std_dev)
        let k = 2.0; // Decay rate
        let consistency = (-k * std_dev).exp();

        consistency.clamp(0.0, 1.0)
    }

    /// Get recent whale score (0-100)
    /// Higher score means more recent whale activity
    pub fn whale_score(&self, max_age_ms: u64) -> f64 {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let cutoff = now_ms.saturating_sub(max_age_ms);

        let recent_whales: Vec<_> = self.large_orders.iter()
            .filter(|w| w.timestamp_ms >= cutoff)
            .collect();

        if recent_whales.is_empty() {
            return 0.0;
        }

        // Score based on:
        // 1. Number of whales
        // 2. Size multiplier
        // 3. Recency
        let mut score = 0.0;

        for whale in recent_whales {
            let age_factor = 1.0 - ((now_ms - whale.timestamp_ms) as f64 / max_age_ms as f64);
            let size_factor = (whale.size_multiplier - 3.0).max(0.0); // Subtract threshold
            score += age_factor * size_factor * 10.0;
        }

        score.min(100.0)
    }

    /// Get pressure score (-100 to 100)
    /// Positive means more bid pressure, negative means more ask pressure
    pub fn pressure_score(&self) -> f64 {
        let diff = self.bid_pressure - self.ask_pressure;
        // Normalize to -100 to 100 range
        (diff * 100.0).clamp(-100.0, 100.0)
    }

    /// Get volume delta for multiple windows
    pub fn get_volume_deltas(&self, windows: &[u64]) -> HashMap<u64, f64> {
        windows.iter()
            .map(|&window| (window, self.calculate_volume_delta(window)))
            .collect()
    }
}

impl Default for OrderbookMetrics {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_volume_snapshot() {
        let mut metrics = OrderbookMetrics::new();
        metrics.add_snapshot(10.0, 8.0);
        metrics.add_snapshot(12.0, 9.0);

        assert_eq!(metrics.volume_history.len(), 2);
    }

    #[test]
    fn test_whale_detection() {
        let mut metrics = OrderbookMetrics::new();

        let bid_levels = vec![(50000.0, 1.0), (49999.0, 1.0)];
        let ask_levels = vec![(50001.0, 1.0), (50002.0, 5.0)]; // Whale here

        metrics.update_avg_order_size(&bid_levels);
        metrics.update_avg_order_size(&ask_levels);

        let whales = metrics.detect_whales(&bid_levels, &ask_levels, 3.0);

        assert!(whales.len() > 0);
        assert_eq!(whales[0].side, OrderSide::Ask);
    }

    #[test]
    fn test_multi_level_imbalance() {
        let mut metrics = OrderbookMetrics::new();

        let bids = vec![(50000.0, 10.0), (49999.0, 5.0), (49998.0, 3.0)];
        let asks = vec![(50001.0, 2.0), (50002.0, 1.0), (50003.0, 1.0)];

        let depths = vec![2, 3];
        let imbalances = metrics.calculate_multi_level_imbalance(&bids, &asks, &depths);

        assert!(imbalances.contains_key(&2));
        assert!(imbalances.contains_key(&3));
        assert!(imbalances[&2] > 0.0); // More bids
    }

    #[test]
    fn test_depth_consistency() {
        let mut metrics = OrderbookMetrics::new();

        // Consistent imbalance across depths
        metrics.imbalance_by_depth.insert(5, 0.5);
        metrics.imbalance_by_depth.insert(10, 0.52);
        metrics.imbalance_by_depth.insert(20, 0.48);

        let consistency = metrics.depth_consistency();
        assert!(consistency > 0.8); // Should be highly consistent
    }
}
