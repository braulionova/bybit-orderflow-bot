use serde::{Deserialize, Serialize};
use std::sync::Arc;
use anyhow::Result;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub bybit: BybitConfig,
    pub trading: TradingConfig,
    pub risk: RiskConfig,
    pub performance: PerformanceConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BybitConfig {
    pub api_key: Option<String>,
    pub api_secret: Option<String>,
    pub testnet: bool,
    pub ws_url: String,
    pub rest_url: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TradingConfig {
    pub symbol: String,
    pub risk_per_trade_pct: f64,
    pub max_leverage: u8,
    pub target_maker_ratio: f64,
    pub min_time_between_trades_ms: u64,
    pub max_trades_per_hour: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RiskConfig {
    pub max_daily_drawdown_pct: f64,
    pub max_consecutive_losses: u8,
    pub max_latency_ms: u64,
    pub max_spread_pct: f64,
    pub min_liquidity_btc: f64,
    pub kill_switch_enabled: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PerformanceConfig {
    pub enable_metrics: bool,
    pub metrics_port: u16,
    pub log_level: String,
    pub orderbook_depth: usize,
}

impl Config {
    pub fn load() -> Result<Arc<Self>> {
        dotenv::dotenv().ok();
        
        let mut builder = config::Config::builder()
            .add_source(config::File::with_name("config/default").required(false))
            .add_source(config::Environment::with_prefix("BOT").separator("_"));
        
        // Load API keys from environment
        if let Ok(api_key) = std::env::var("BYBIT_API_KEY") {
            builder = builder.set_override("bybit.api_key", api_key)?;
        }
        
        if let Ok(api_secret) = std::env::var("BYBIT_API_SECRET") {
            builder = builder.set_override("bybit.api_secret", api_secret)?;
        }
        
        let config = builder.build()?;
        Ok(Arc::new(config.try_deserialize()?))
    }
}
