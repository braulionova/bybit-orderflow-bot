use anyhow::Result;
use std::sync::Arc;
use tracing::{info, warn, Level};
use tracing_subscriber::FmtSubscriber;

use bybit_orderflow_bot::config::Config;
use bybit_orderflow_bot::bybit::{BybitWebSocket, OrderbookData};
use bybit_orderflow_bot::orderbook::{Orderbook, OrderbookValidator};
use bybit_orderflow_bot::TelegramNotifier;
use bybit_orderflow_bot::strategy::{Strategy, PositionManager, TradingSide};
use bybit_orderflow_bot::execution::BybitClient;
use bybit_orderflow_bot::bybit::auth::BybitAuth;
use bybit_orderflow_bot::risk::VolatilityCalculator;

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
    
    // Initialize Telegram notifier
    let tg = if config.telegram.enabled {
        if let (Some(token), Some(chat_id)) = (&config.telegram.bot_token, &config.telegram.chat_id) {
            if !token.is_empty() {
                info!("📱 Telegram notifications enabled");
                let notifier = TelegramNotifier::new(token.clone(), chat_id.clone());
                let symbol = config.trading.symbol.clone();
                let testnet = config.bybit.testnet;
                
                // Send startup notification and wait for it
                match notifier.notify_startup(&symbol, testnet).await {
                    Ok(true) => info!("📱 Startup notification sent"),
                    Ok(false) => info!("📱 Startup notification blocked (cooldown)"),
                    Err(e) => warn!("Failed to send startup notification: {}", e),
                }
                
                Some(notifier)
            } else {
                info!("📱 Telegram bot_token is empty, notifications disabled");
                None
            }
        } else {
            info!("📱 Telegram chat_id not configured, notifications disabled");
            None
        }
    } else {
        info!("📱 Telegram notifications disabled in config");
        None
    };
    
    // Store notifier for shutdown notification
    let tg_shutdown = tg.clone();
    
    // Initialize Bybit REST client for trading
    let auth = if let (Some(api_key), Some(api_secret)) = (&config.bybit.api_key, &config.bybit.api_secret) {
        Some(BybitAuth::new(api_key.clone(), api_secret.clone()))
    } else {
        None
    };
    
    let rest_client = BybitClient::new(config.bybit.rest_url.clone(), auth);
    info!("✅ REST client initialized");
    
    // Initialize strategy with custom weights from config
    let strategy = Strategy::with_weights(
        20,                                      // min_score (was 40) - easier to trigger
        30.0,                                    // min_confidence (was 50.0)
        config.risk.max_spread_pct * 100.0,     // max_spread_pct
        config.risk.min_liquidity_btc,          // min_liquidity_btc
        config.risk.max_latency_ms,             // max_latency_ms
        config.strategy.imbalance_weight,
        config.strategy.volume_delta_weight,
        config.strategy.whale_weight,
        config.strategy.pressure_weight,
        config.strategy.depth_consistency_weight,
    );
    info!("✅ Strategy initialized (multi-dimensional scoring)");
    
    // Initialize position manager
    let position_manager = PositionManager::new();
    info!("✅ Position manager initialized");

    // Phase 3C: Initialize orderbook validator
    let validator_config = bybit_orderflow_bot::orderbook::validation::ValidationConfig {
        enabled: config.validation.enable_validation,
        max_spread_multiplier: config.validation.max_spread_multiplier,
        min_liquidity_multiplier: config.validation.min_liquidity_multiplier,
        max_data_age_ms: config.validation.max_data_age_ms,
        min_depth_levels: config.validation.min_depth_levels,
    };
    let validator = OrderbookValidator::new(validator_config);
    info!("✅ Orderbook validator initialized");

    // Phase 3B: Initialize volatility calculator
    let volatility_calc = VolatilityCalculator::new(config.risk.atr_period);
    info!("✅ Volatility calculator initialized (ATR period: {})", config.risk.atr_period);
    
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
        let tg = tg.clone();

        tokio::spawn(async move {
            monitor_orderbook(ob, cfg, tg, strategy, position_manager, rest_client, validator, volatility_calc).await
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
    
    // Send shutdown notification
    if let Some(tg) = tg_shutdown {
        let symbol = config.trading.symbol.clone();
        if let Err(e) = tg.notify_shutdown(&symbol).await {
            warn!("Failed to send shutdown notification: {}", e);
        }
    }
    
    info!("👋 Bot stopped");
    Ok(())
}

async fn monitor_orderbook(
    orderbook: Arc<Orderbook>,
    config: Arc<Config>,
    tg: Option<TelegramNotifier>,
    strategy: Strategy,
    position_manager: PositionManager,
    rest_client: BybitClient,
    mut validator: OrderbookValidator,
    mut volatility_calc: VolatilityCalculator,
) -> Result<()> {
    let mut interval = tokio::time::interval(
        tokio::time::Duration::from_secs(5)
    );
    
    let mut summary_counter: u32 = 0;
    let summary_interval: u32 = 60; // 5 minutes (60 * 5 seconds)
    let mut trade_cooldown: u64 = 0;
    let min_time_between_trades = config.trading.min_time_between_trades_ms / 1000 / 5;
    
    loop {
        interval.tick().await;
        
        let (bid, ask) = orderbook.best_bid_ask();

        // Skip if orderbook not initialized
        if bid == 0.0 || ask == 0.0 || ask == f64::from_bits(u64::MAX) {
            continue;
        }

        // Phase 3B: Update volatility calculator
        volatility_calc.add_price(bid, ask);
        let atr = volatility_calc.get_atr();
        let atr_pct = volatility_calc.get_atr_pct((bid + ask) / 2.0);
        let vol_regime = volatility_calc.get_volatility_regime((bid + ask) / 2.0);

        // Phase 2: Update orderbook metrics
        orderbook.update_metrics(
            &config.strategy.depth_levels,
            config.strategy.whale_threshold_multiplier,
        );

        // Phase 3C: Validate orderbook before trading
        let validation = validator.validate(&orderbook);
        if !validation.is_valid() {
            warn!("⚠️  Orderbook validation failed: {} - skipping tick", validation.to_string());
            continue;
        }

        let mid = orderbook.mid_price();
        let spread_pct = orderbook.spread_pct() * 100.0;
        let imbalance = orderbook.imbalance(10);
        let liquidity = orderbook.liquidity_depth(10);
        let latency = orderbook.latency_ms();
        let updates = orderbook.update_count();

        // Phase 2: Get advanced metrics (in scope to release lock before await)
        let (volume_delta_1s, volume_delta_5s, whale_score, pressure_score, depth_consistency) = {
            let metrics = orderbook.get_metrics();
            let vd1 = metrics.calculate_volume_delta(1000);
            let vd5 = metrics.calculate_volume_delta(5000);
            let ws = metrics.whale_score(10000); // Last 10s
            let ps = metrics.pressure_score();
            let dc = metrics.depth_consistency();
            (vd1, vd5, ws, ps, dc)
        }; // Lock released here

        info!(
            "📊 {} | Bid: ${:.2} | Ask: ${:.2} | Mid: ${:.2} | Spread: {:.4}% | Imb: {:.3} | Liq: {:.2} BTC | Lat: {}ms",
            config.trading.symbol, bid, ask, mid, spread_pct, imbalance, liquidity, latency
        );

        info!(
            "📈 Advanced | VolΔ1s: {:.2} | VolΔ5s: {:.2} | Whale: {:.0} | Pressure: {:.0} | DepthCons: {:.2} | ATR: ${:.2} ({:.3}%) | Vol: {:?}",
            volume_delta_1s, volume_delta_5s, whale_score, pressure_score, depth_consistency, atr, atr_pct * 100.0, vol_regime
        );

        // Phase 3A: Analyze with enhanced multi-dimensional scoring
        let signal = strategy.analyze_enhanced(
            imbalance,
            spread_pct,
            liquidity,
            latency,
            volume_delta_1s,
            volume_delta_5s,
            whale_score,
            pressure_score,
            depth_consistency,
        );
        
        // Check cooldown
        if trade_cooldown > 0 {
            trade_cooldown -= 1;
        }
        
        // Check if we should trade
        let has_position = position_manager.has_position().await;
        
        if !has_position && trade_cooldown == 0 && strategy.should_trade(&signal, config.trading.min_time_between_trades_ms as u64) {
            if let Some(side) = signal.bias.side() {
                info!("🎯 SIGNAL | {:?} | Score: {} | Conf: {:.1}% | Momentum: {:.2} | Whale: {:.0} | Depth: {:.2}",
                    signal.bias, signal.score, signal.confidence, signal.momentum_score, signal.whale_score, signal.depth_consistency);

                // Fixed $1000 USDT per order
                let price = mid;
                let usd_amount = 1000.0;
                let qty = usd_amount / price; // BTC quantity for $1000
                let qty = (qty * 1000.0).round() / 1000.0; // Round to 3 decimal places (Bybit step size)

                // Phase 3B: Apply volatility-based position sizing
                let vol_multiplier = volatility_calc.position_size_multiplier(price);
                let qty = qty * vol_multiplier;
                let qty = qty.max(0.001).min(1.0); // Min 0.001, Max 1 BTC

                info!("💰 Placing order: {} {} @ ${:.2} (qty: {:.3}, vol_adj: {:.2}x)",
                    if side == TradingSide::Buy { "BUY" } else { "SELL" },
                    config.trading.symbol, price, qty, vol_multiplier);

                // Phase 3B: Calculate risk params for native SL/TP (synchronous calculation)
                let risk_params_for_order = bybit_orderflow_bot::risk::DynamicRiskParams::calculate(
                    &volatility_calc,
                    price,
                    side,
                    config.risk.base_sl_pct,
                    config.risk.base_tp_pct,
                    config.risk.volatility_multiplier,
                );

                // Prepare SL/TP for native API or software-only fallback
                let (stop_loss, take_profit) = if config.risk.use_native_sltp {
                    (Some(risk_params_for_order.stop_loss_price), Some(risk_params_for_order.take_profit_price))
                } else {
                    (None, None)
                };

                // Place order
                let order_request = bybit_orderflow_bot::execution::OrderRequest {
                    symbol: config.trading.symbol.clone(),
                    side: if side == TradingSide::Buy {
                        bybit_orderflow_bot::execution::OrderSide::Buy
                    } else {
                        bybit_orderflow_bot::execution::OrderSide::Sell
                    },
                    order_type: bybit_orderflow_bot::execution::OrderType::Market,
                    qty,
                    price: None,
                    reduce_only: false,
                    close_on_trigger: false,
                    // Native SL/TP parameters
                    stop_loss,
                    take_profit,
                    tpsl_mode: Some("Full".to_string()),
                    tp_order_type: Some(config.risk.sltp_order_type.clone()),
                    sl_order_type: Some(config.risk.sltp_order_type.clone()),
                    tp_trigger_by: Some(config.risk.sltp_trigger_by.clone()),
                    sl_trigger_by: Some(config.risk.sltp_trigger_by.clone()),
                };

                match rest_client.place_order(order_request).await {
                    Ok(order) => {
                        info!("📤 Order placed: {} | {} @ ${:.2}", order.order_id, order.side, order.price);

                        // Phase 3B: Open position with dynamic risk management
                        let risk_params = position_manager.open_position_dynamic(
                            side,
                            price,
                            qty,
                            &volatility_calc,
                            config.risk.base_sl_pct,
                            config.risk.base_tp_pct,
                            config.risk.volatility_multiplier,
                        ).await;

                        info!("🛡️  Dynamic Risk | SL: {:.2}% (${:.2}) | TP: {:.2}% (${:.2}) | ATR: ${:.2} | Vol: {:?}",
                            risk_params.stop_loss_pct * 100.0,
                            risk_params.stop_loss_price,
                            risk_params.take_profit_pct * 100.0,
                            risk_params.take_profit_price,
                            risk_params.atr_value,
                            risk_params.volatility_regime
                        );

                        // Log native SL/TP if enabled
                        if config.risk.use_native_sltp {
                            info!("🔗 Native SL/TP | SL @ ${:.2} | TP @ ${:.2} | Type: {} | Trigger: {}",
                                risk_params.stop_loss_price,
                                risk_params.take_profit_price,
                                config.risk.sltp_order_type,
                                config.risk.sltp_trigger_by
                            );
                        }

                        // Set cooldown
                        trade_cooldown = min_time_between_trades;

                        // Send notification
                        if let Some(ref notifier) = tg {
                            let side_str = if side == TradingSide::Buy { "Buy" } else { "Sell" };
                            let _ = notifier.notify_order_placed(&config.trading.symbol, side_str, "Market", qty, price, &order.order_id).await;
                            let _ = notifier.notify_position_opened(
                                &config.trading.symbol,
                                side_str,
                                price,
                                qty,
                                risk_params.stop_loss_price,
                                risk_params.take_profit_price
                            ).await;
                        }
                    }
                    Err(e) => {
                        warn!("❌ Order failed: {}", e);
                        if let Some(ref notifier) = tg {
                            let side_str = if side == TradingSide::Buy { "Buy" } else { "Sell" };
                            let _ = notifier.notify_order_error(&config.trading.symbol, side_str, &e.to_string()).await;
                        }
                    }
                }
            }
        }
        
        // Check exit conditions (software monitoring)
        if has_position && config.risk.keep_software_monitoring {
            if let Some(exit_reason) = position_manager.check_exit(mid).await {
                let exit_reason_str = match exit_reason {
                    bybit_orderflow_bot::strategy::ExitReason::StopLoss => "Stop Loss",
                    bybit_orderflow_bot::strategy::ExitReason::TakeProfit => "Take Profit",
                    bybit_orderflow_bot::strategy::ExitReason::SignalReversal => "Signal Reversal",
                    bybit_orderflow_bot::strategy::ExitReason::Manual => "Manual",
                };

                // Log warning if native SL/TP is enabled (it should have triggered first)
                if config.risk.use_native_sltp {
                    warn!("⚠️  Software monitoring triggered (Native SL/TP should have executed): {:?}", exit_reason);
                }

                info!("🔄 Exiting position: {:?} at ${:.2}", exit_reason, mid);
                
                // Get position details before closing
                if let Some(pos) = position_manager.get_position_details().await {
                    let pnl_pct = match pos.side {
                        TradingSide::Buy => (mid - pos.entry_price) / pos.entry_price * 100.0,
                        TradingSide::Sell => (pos.entry_price - mid) / pos.entry_price * 100.0,
                    };
                    let pnl = pos.size * (mid - pos.entry_price);
                    let side_str = if pos.side == TradingSide::Buy { "Buy" } else { "Sell" };
                    
                    // Send notification
                    if let Some(ref notifier) = tg {
                        let _ = notifier.notify_position_closed(&config.trading.symbol, side_str, pos.entry_price, mid, pos.size, pnl, pnl_pct, exit_reason_str).await;
                    }
                }
                
                position_manager.close_position().await;
            }
        }
        
        // Send summary every 5 minutes with wallet info
        summary_counter += 1;
        if summary_counter >= summary_interval {
            summary_counter = 0;
            if let Some(ref notifier) = tg {
                // Get wallet balance
                match rest_client.get_wallet().await {
                    Ok(wallet) => {
                        let _ = notifier.notify_wallet(wallet.total_margin_balance, wallet.total_available_balance, wallet.total_perpetual_unrealised_pnl).await;
                    }
                    Err(_) => {}
                }
                let symbol = config.trading.symbol.clone();
                if let Err(e) = notifier.notify_summary(&symbol, bid, ask, spread_pct, imbalance, liquidity, latency, updates).await {
                    warn!("Failed to send summary notification: {}", e);
                }
            }
        }
        
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
