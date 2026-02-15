use anyhow::Result;
use std::sync::Arc;
use tracing::{info, warn, Level};
use tracing_subscriber::FmtSubscriber;

use bybit_orderflow_bot::config::Config;
use bybit_orderflow_bot::bybit::{BybitWebSocket, OrderbookData};
use bybit_orderflow_bot::orderbook::Orderbook;

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() -> Result<()> {
    // Initialize logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_target(false)
        .with_thread_ids(true)
        .finish();
    
    tracing::subscriber::set_global_default(subscriber)?;
    
    info!("🚀 Bybit Order Flow Bot - Phase 1 - Starting...");
    
    // Load configuration
    let config = Config::load()?;
    info!("✅ Configuration loaded");
    info!("   Symbol: {}", config.trading.symbol);
    info!("   Testnet: {}", config.bybit.testnet);
    info!("   WebSocket URL: {}", config.bybit.ws_url);
    
    // Initialize orderbook
    let orderbook = Arc::new(Orderbook::new(config.trading.symbol.clone()));
    info!("✅ Orderbook initialized");
    
    // Setup WebSocket
    let mut ws = BybitWebSocket::new(config.bybit.ws_url.clone());
    
    // Subscribe to orderbook updates
    let orderbook_clone = orderbook.clone();
    let symbol = config.trading.symbol.clone();
    
    ws.subscribe(
        format!("orderbook.50.{}", symbol),
        Arc::new(move |data| {
            // Parse orderbook data
            match serde_json::from_value::<OrderbookData>(data) {
                Ok(ob_data) => {
                    let (bids, asks) = ob_data.parse_levels();
                    
                    // Apply snapshot or delta based on message type
                    if ob_data.update_id > 0 {
                        orderbook_clone.apply_snapshot(bids, asks);
                    } else {
                        orderbook_clone.apply_delta(bids, asks);
                    }
                    
                    Ok(())
                }
                Err(e) => {
                    warn!("Failed to parse orderbook data: {}", e);
                    Ok(())
                }
            }
        })
    );
    
    info!("✅ WebSocket subscriptions configured");
    
    // Start WebSocket in background
    let ws = Arc::new(ws);
    let ws_task = {
        let ws_clone = ws.clone();
        tokio::spawn(async move {
            ws_clone.run().await
        })
    };
    
    // Start monitoring loop
    let monitor_task = {
        let ob = orderbook.clone();
        let cfg = config.clone();
        
        tokio::spawn(async move {
            monitor_orderbook(ob, cfg).await
        })
    };
    
    info!("✅ All tasks started");
    info!("📊 Monitoring orderbook for {}...", config.trading.symbol);
    
    // Wait for tasks
    tokio::select! {
        result = ws_task => {
            if let Err(e) = result {
                warn!("WebSocket task error: {}", e);
            }
        }
        result = monitor_task => {
            if let Err(e) = result {
                warn!("Monitor task error: {}", e);
            }
        }
        _ = tokio::signal::ctrl_c() => {
            info!("Shutdown signal received");
        }
    }
    
    info!("👋 Bot stopped");
    Ok(())
}

async fn monitor_orderbook(orderbook: Arc<Orderbook>, config: Arc<Config>) -> Result<()> {
    let mut interval = tokio::time::interval(
        tokio::time::Duration::from_secs(5)
    );
    
    loop {
        interval.tick().await;
        
        let (bid, ask) = orderbook.best_bid_ask();
        
        // Skip if orderbook not initialized
        if bid == 0.0 || ask == 0.0 || ask == f64::from_bits(u64::MAX) {
            continue;
        }
        
        let mid = orderbook.mid_price();
        let spread_pct = orderbook.spread_pct() * 100.0;
        let imbalance = orderbook.imbalance(10);
        let liquidity = orderbook.liquidity_depth(10);
        let latency = orderbook.latency_ms();
        let updates = orderbook.update_count();
        
        info!(
            "📊 {} | Bid: ${:.2} | Ask: ${:.2} | Mid: ${:.2} | Spread: {:.4}% | Imbalance: {:.3} | Liquidity: {:.2} BTC | Latency: {}ms | Updates: {}",
            config.trading.symbol,
            bid,
            ask,
            mid,
            spread_pct,
            imbalance,
            liquidity,
            latency,
            updates
        );
        
        // Alert on wide spread
        if spread_pct > config.risk.max_spread_pct * 100.0 {
            warn!("⚠️  Wide spread detected: {:.4}%", spread_pct);
        }
        
        // Alert on high latency
        if latency > config.risk.max_latency_ms {
            warn!("⚠️  High latency detected: {}ms", latency);
        }
        
        // Alert on low liquidity
        if liquidity < config.risk.min_liquidity_btc {
            warn!("⚠️  Low liquidity detected: {:.2} BTC", liquidity);
        }
    }
}
