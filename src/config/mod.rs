use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub bybit: BybitConfig,
    pub trading: TradingConfig,
    pub risk: RiskConfig,
    pub performance: PerformanceConfig,
    pub telegram: TelegramConfig,
    #[serde(default)]
    pub strategy: StrategyConfig,
    #[serde(default)]
    pub validation: ValidationConfig,
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
    // Phase 3B: Dynamic risk management
    #[serde(default = "default_base_sl_pct")]
    pub base_sl_pct: f64,
    #[serde(default = "default_base_tp_pct")]
    pub base_tp_pct: f64,
    #[serde(default = "default_volatility_multiplier")]
    pub volatility_multiplier: f64,
    #[serde(default = "default_atr_period")]
    pub atr_period: usize,
    // Native Bybit SL/TP Orders
    #[serde(default = "default_use_native_sltp")]
    pub use_native_sltp: bool,
    #[serde(default = "default_sltp_order_type")]
    pub sltp_order_type: String,              // "Market" or "Limit"
    #[serde(default = "default_sltp_trigger_by")]
    pub sltp_trigger_by: String,              // "LastPrice", "MarkPrice", "IndexPrice"
    #[serde(default = "default_keep_software_monitoring")]
    pub keep_software_monitoring: bool,       // Keep software monitoring as backup
}

fn default_base_sl_pct() -> f64 { 0.01 }
fn default_base_tp_pct() -> f64 { 0.02 }
fn default_volatility_multiplier() -> f64 { 0.5 }
fn default_atr_period() -> usize { 14 }
fn default_use_native_sltp() -> bool { true }
fn default_sltp_order_type() -> String { "Market".to_string() }
fn default_sltp_trigger_by() -> String { "LastPrice".to_string() }
fn default_keep_software_monitoring() -> bool { true }

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PerformanceConfig {
    pub enable_metrics: bool,
    pub metrics_port: u16,
    pub log_level: String,
    pub orderbook_depth: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TelegramConfig {
    pub enabled: bool,
    pub bot_token: Option<String>,
    pub chat_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StrategyConfig {
    // Scoring weights (should sum to ~1.0)
    #[serde(default = "default_imbalance_weight")]
    pub imbalance_weight: f64,
    #[serde(default = "default_volume_delta_weight")]
    pub volume_delta_weight: f64,
    #[serde(default = "default_whale_weight")]
    pub whale_weight: f64,
    #[serde(default = "default_pressure_weight")]
    pub pressure_weight: f64,
    #[serde(default = "default_depth_consistency_weight")]
    pub depth_consistency_weight: f64,

    // Analysis parameters
    #[serde(default = "default_depth_levels")]
    pub depth_levels: Vec<usize>,
    #[serde(default = "default_whale_threshold")]
    pub whale_threshold_multiplier: f64,
    #[serde(default = "default_min_whale_size")]
    pub min_whale_size_btc: f64,
    #[serde(default = "default_delta_windows")]
    pub delta_windows: Vec<u64>,
}

impl Default for StrategyConfig {
    fn default() -> Self {
        Self {
            imbalance_weight: default_imbalance_weight(),
            volume_delta_weight: default_volume_delta_weight(),
            whale_weight: default_whale_weight(),
            pressure_weight: default_pressure_weight(),
            depth_consistency_weight: default_depth_consistency_weight(),
            depth_levels: default_depth_levels(),
            whale_threshold_multiplier: default_whale_threshold(),
            min_whale_size_btc: default_min_whale_size(),
            delta_windows: default_delta_windows(),
        }
    }
}

fn default_imbalance_weight() -> f64 { 0.30 }
fn default_volume_delta_weight() -> f64 { 0.25 }
fn default_whale_weight() -> f64 { 0.20 }
fn default_pressure_weight() -> f64 { 0.15 }
fn default_depth_consistency_weight() -> f64 { 0.10 }
fn default_depth_levels() -> Vec<usize> { vec![5, 10, 20] }
fn default_whale_threshold() -> f64 { 3.0 }
fn default_min_whale_size() -> f64 { 0.5 }
fn default_delta_windows() -> Vec<u64> { vec![1000, 5000, 30000] }

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ValidationConfig {
    #[serde(default = "default_validation_enabled")]
    pub enable_validation: bool,
    #[serde(default = "default_max_spread_multiplier")]
    pub max_spread_multiplier: f64,
    #[serde(default = "default_min_liquidity_multiplier")]
    pub min_liquidity_multiplier: f64,
    #[serde(default = "default_max_data_age_ms")]
    pub max_data_age_ms: u64,
    #[serde(default = "default_min_depth_levels")]
    pub min_depth_levels: usize,
}

impl Default for ValidationConfig {
    fn default() -> Self {
        Self {
            enable_validation: default_validation_enabled(),
            max_spread_multiplier: default_max_spread_multiplier(),
            min_liquidity_multiplier: default_min_liquidity_multiplier(),
            max_data_age_ms: default_max_data_age_ms(),
            min_depth_levels: default_min_depth_levels(),
        }
    }
}

fn default_validation_enabled() -> bool { true }
fn default_max_spread_multiplier() -> f64 { 3.0 }
fn default_min_liquidity_multiplier() -> f64 { 0.25 }
fn default_max_data_age_ms() -> u64 { 5000 }
fn default_min_depth_levels() -> usize { 5 }

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
