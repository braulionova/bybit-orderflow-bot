use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum MarketBias {
    StrongLong,
    WeakLong,
    Neutral,
    WeakShort,
    StrongShort,
}

impl MarketBias {
    pub fn score(&self) -> i32 {
        match self {
            MarketBias::StrongLong => 80,
            MarketBias::WeakLong => 40,
            MarketBias::Neutral => 0,
            MarketBias::WeakShort => -40,
            MarketBias::StrongShort => -80,
        }
    }
    
    pub fn side(&self) -> Option<TradingSide> {
        match self {
            MarketBias::StrongLong | MarketBias::WeakLong => Some(TradingSide::Buy),
            MarketBias::WeakShort | MarketBias::StrongShort => Some(TradingSide::Sell),
            MarketBias::Neutral => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum TradingSide {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingSignal {
    // Existing fields
    pub bias: MarketBias,
    pub score: i32,
    pub confidence: f64,
    pub imbalance: f64,
    pub spread_pct: f64,
    pub liquidity: f64,
    pub latency_ms: u64,

    // Phase 2: Advanced metrics
    pub volume_delta_1s: f64,      // Short-term momentum
    pub volume_delta_5s: f64,      // Medium-term trend
    pub whale_score: f64,          // Large order influence (0-100)
    pub pressure_score: f64,       // Bid/ask pressure difference
    pub depth_consistency: f64,    // Consistency across levels
    pub momentum_score: f64,       // Combined momentum indicator
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum SignalStrength {
    Strong,
    Moderate,
    Weak,
    None,
}

pub struct Strategy {
    min_score: i32,
    min_confidence: f64,
    max_spread_pct: f64,
    min_liquidity_btc: f64,
    max_latency_ms: u64,
    last_signal_time: Arc<RwLock<u64>>,

    // Phase 2: Scoring weights (should sum to ~1.0)
    imbalance_weight: f64,
    volume_delta_weight: f64,
    whale_weight: f64,
    pressure_weight: f64,
    depth_consistency_weight: f64,
}

impl Strategy {
    pub fn new(
        min_score: i32,
        min_confidence: f64,
        max_spread_pct: f64,
        min_liquidity_btc: f64,
        max_latency_ms: u64,
    ) -> Self {
        Self {
            min_score,
            min_confidence,
            max_spread_pct,
            min_liquidity_btc,
            max_latency_ms,
            last_signal_time: Arc::new(RwLock::new(0)),
            // Default weights optimized for Bybit orderflow
            imbalance_weight: 0.30,
            volume_delta_weight: 0.25,
            whale_weight: 0.20,
            pressure_weight: 0.15,
            depth_consistency_weight: 0.10,
        }
    }

    /// Create strategy with custom weights
    pub fn with_weights(
        min_score: i32,
        min_confidence: f64,
        max_spread_pct: f64,
        min_liquidity_btc: f64,
        max_latency_ms: u64,
        imbalance_weight: f64,
        volume_delta_weight: f64,
        whale_weight: f64,
        pressure_weight: f64,
        depth_consistency_weight: f64,
    ) -> Self {
        Self {
            min_score,
            min_confidence,
            max_spread_pct,
            min_liquidity_btc,
            max_latency_ms,
            last_signal_time: Arc::new(RwLock::new(0)),
            imbalance_weight,
            volume_delta_weight,
            whale_weight,
            pressure_weight,
            depth_consistency_weight,
        }
    }

    pub fn analyze(
        &self,
        imbalance: f64,
        spread_pct: f64,
        liquidity: f64,
        latency_ms: u64,
    ) -> TradingSignal {
        let bias = self.calculate_bias(imbalance);
        let score = self.calculate_score(imbalance, spread_pct, liquidity, latency_ms);
        let confidence = self.calculate_confidence(imbalance, spread_pct, liquidity);

        TradingSignal {
            bias,
            score,
            confidence,
            imbalance,
            spread_pct,
            liquidity,
            latency_ms,
            // Phase 2 fields (default to 0)
            volume_delta_1s: 0.0,
            volume_delta_5s: 0.0,
            whale_score: 0.0,
            pressure_score: 0.0,
            depth_consistency: 0.0,
            momentum_score: 0.0,
        }
    }

    /// Enhanced analysis with advanced orderbook metrics (Phase 2)
    pub fn analyze_enhanced(
        &self,
        imbalance: f64,
        spread_pct: f64,
        liquidity: f64,
        latency_ms: u64,
        volume_delta_1s: f64,
        volume_delta_5s: f64,
        whale_score: f64,
        pressure_score: f64,
        depth_consistency: f64,
    ) -> TradingSignal {
        let bias = self.calculate_bias(imbalance);

        // Calculate momentum score from volume deltas
        let momentum_score = self.calculate_momentum_score(volume_delta_1s, volume_delta_5s, imbalance);

        // Multi-dimensional score calculation
        let score = self.calculate_enhanced_score(
            imbalance,
            spread_pct,
            liquidity,
            latency_ms,
            volume_delta_5s,
            whale_score,
            pressure_score,
            depth_consistency,
        );

        // Enhanced confidence calculation
        let confidence = self.calculate_enhanced_confidence(
            imbalance,
            spread_pct,
            liquidity,
            depth_consistency,
            whale_score,
        );

        TradingSignal {
            bias,
            score,
            confidence,
            imbalance,
            spread_pct,
            liquidity,
            latency_ms,
            volume_delta_1s,
            volume_delta_5s,
            whale_score,
            pressure_score,
            depth_consistency,
            momentum_score,
        }
    }

    fn calculate_bias(&self, imbalance: f64) -> MarketBias {
        if imbalance > 0.7 {
            MarketBias::StrongLong
        } else if imbalance > 0.3 {
            MarketBias::WeakLong
        } else if imbalance < -0.7 {
            MarketBias::StrongShort
        } else if imbalance < -0.3 {
            MarketBias::WeakShort
        } else {
            MarketBias::Neutral
        }
    }

    fn calculate_score(&self, imbalance: f64, spread_pct: f64, liquidity: f64, latency_ms: u64) -> i32 {
        let mut score = 0;
        
        let imbalance_score = (imbalance.abs() * 100.0) as i32;
        score += imbalance_score;
        
        if spread_pct > self.max_spread_pct {
            score -= 30;
        }
        
        if liquidity < self.min_liquidity_btc * 0.5 {
            score -= 20;
        } else if liquidity < self.min_liquidity_btc {
            score -= 10;
        }
        
        if latency_ms > self.max_latency_ms {
            score -= 20;
        } else if latency_ms > self.max_latency_ms / 2 {
            score -= 10;
        }
        
        score.clamp(-100, 100)
    }

    fn calculate_confidence(&self, imbalance: f64, spread_pct: f64, liquidity: f64) -> f64 {
        let imbalance_conf = imbalance.abs().min(1.0);

        let spread_conf = if spread_pct <= self.max_spread_pct * 0.5 {
            1.0
        } else if spread_pct <= self.max_spread_pct {
            0.7
        } else {
            0.3
        };

        let liquidity_conf = (liquidity / self.min_liquidity_btc).min(1.0);

        (imbalance_conf * 0.5 + spread_conf * 0.3 + liquidity_conf * 0.2) * 100.0
    }

    /// Phase 2: Multi-dimensional score calculation
    fn calculate_enhanced_score(
        &self,
        imbalance: f64,
        spread_pct: f64,
        liquidity: f64,
        latency_ms: u64,
        volume_delta_5s: f64,
        whale_score: f64,
        pressure_score: f64,
        depth_consistency: f64,
    ) -> i32 {
        let mut score = 0;

        // 1. Imbalance base (30% weight) - Bybit orderflow specific
        let imbalance_component = (imbalance.abs() * 100.0 * self.imbalance_weight) as i32;
        score += imbalance_component;

        // 2. Volume delta momentum (25% weight)
        let delta_score = (volume_delta_5s.abs() * 100.0 * self.volume_delta_weight) as i32;
        if volume_delta_5s.signum() == imbalance.signum() || imbalance == 0.0 {
            // Momentum aligned with imbalance
            score += delta_score;
        } else {
            // Divergence - penalize
            score -= delta_score / 2;
        }

        // 3. Whale detection (20% weight) - Critical in Bybit
        score += (whale_score * self.whale_weight) as i32;

        // 4. Pressure consistency (15% weight)
        let pressure_component = (pressure_score.abs() * self.pressure_weight) as i32;
        if pressure_score.signum() == imbalance.signum() || imbalance == 0.0 {
            score += pressure_component;
        } else {
            score -= pressure_component / 2;
        }

        // 5. Depth consistency (10% weight)
        score += (depth_consistency * 100.0 * self.depth_consistency_weight) as i32;

        // Standard penalties
        if spread_pct > self.max_spread_pct {
            score -= 30;
        }
        if liquidity < self.min_liquidity_btc * 0.5 {
            score -= 20;
        }
        if latency_ms > self.max_latency_ms {
            score -= 20;
        }

        score.clamp(-100, 100)
    }

    /// Calculate momentum score from volume deltas
    fn calculate_momentum_score(&self, volume_delta_1s: f64, volume_delta_5s: f64, imbalance: f64) -> f64 {
        // Short-term momentum (1s) weighted 40%
        let short_term = volume_delta_1s * 0.4;

        // Medium-term momentum (5s) weighted 60%
        let medium_term = volume_delta_5s * 0.6;

        let momentum = short_term + medium_term;

        // Amplify if aligned with imbalance
        if momentum.signum() == imbalance.signum() {
            momentum * 1.2
        } else {
            momentum * 0.8
        }
    }

    /// Enhanced confidence calculation with advanced metrics
    fn calculate_enhanced_confidence(
        &self,
        imbalance: f64,
        spread_pct: f64,
        liquidity: f64,
        depth_consistency: f64,
        whale_score: f64,
    ) -> f64 {
        let imbalance_conf = imbalance.abs().min(1.0);

        let spread_conf = if spread_pct <= self.max_spread_pct * 0.5 {
            1.0
        } else if spread_pct <= self.max_spread_pct {
            0.7
        } else {
            0.3
        };

        let liquidity_conf = (liquidity / self.min_liquidity_btc).min(1.0);

        // New factors
        let consistency_conf = depth_consistency;
        let whale_conf = (whale_score / 100.0).min(1.0);

        // Weighted average
        (imbalance_conf * 0.30
            + spread_conf * 0.20
            + liquidity_conf * 0.15
            + consistency_conf * 0.20
            + whale_conf * 0.15)
            * 100.0
    }

    pub fn should_trade(&self, signal: &TradingSignal, min_time_between_ms: u64) -> bool {
        if signal.score < self.min_score {
            return false;
        }
        
        if signal.confidence < self.min_confidence {
            return false;
        }
        
        if signal.spread_pct > self.max_spread_pct {
            return false;
        }
        
        if signal.liquidity < self.min_liquidity_btc {
            return false;
        }
        
        if signal.latency_ms > self.max_latency_ms {
            return false;
        }
        
        true
    }

    pub fn get_signal_strength(signal: &TradingSignal) -> SignalStrength {
        if signal.score >= 70 && signal.confidence >= 70.0 {
            SignalStrength::Strong
        } else if signal.score >= 40 && signal.confidence >= 50.0 {
            SignalStrength::Moderate
        } else if signal.score >= 20 && signal.confidence >= 30.0 {
            SignalStrength::Weak
        } else {
            SignalStrength::None
        }
    }
}

pub struct PositionManager {
    position: Arc<RwLock<Option<Position>>>,
    entry_price: f64,
    side: TradingSide,
    size: f64,
    stop_loss: f64,
    take_profit: f64,
}

#[derive(Debug, Clone)]
pub struct Position {
    pub side: TradingSide,
    pub entry_price: f64,
    pub size: f64,
    pub stop_loss: f64,
    pub take_profit: f64,
    pub opened_at: u64,
}

impl PositionManager {
    pub fn new() -> Self {
        Self {
            position: Arc::new(RwLock::new(None)),
            entry_price: 0.0,
            side: TradingSide::Buy,
            size: 0.0,
            stop_loss: 0.0,
            take_profit: 0.0,
        }
    }

    pub async fn has_position(&self) -> bool {
        self.position.read().await.is_some()
    }

    pub async fn open_position(&self, side: TradingSide, entry_price: f64, size: f64, stop_loss_pct: f64, take_profit_pct: f64) {
        let stop_loss = match side {
            TradingSide::Buy => entry_price * (1.0 - stop_loss_pct),
            TradingSide::Sell => entry_price * (1.0 + stop_loss_pct),
        };

        let take_profit = match side {
            TradingSide::Buy => entry_price * (1.0 + take_profit_pct),
            TradingSide::Sell => entry_price * (1.0 - take_profit_pct),
        };

        let position = Position {
            side,
            entry_price,
            size,
            stop_loss,
            take_profit,
            opened_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };

        *self.position.write().await = Some(position);
    }

    /// Phase 3B: Open position with dynamic risk parameters based on ATR
    pub async fn open_position_dynamic(
        &self,
        side: TradingSide,
        entry_price: f64,
        size: f64,
        volatility_calc: &crate::risk::VolatilityCalculator,
        base_sl_pct: f64,
        base_tp_pct: f64,
        volatility_multiplier: f64,
    ) -> crate::risk::DynamicRiskParams {
        use crate::risk::DynamicRiskParams;

        // Calculate dynamic risk parameters
        let risk_params = DynamicRiskParams::calculate(
            volatility_calc,
            entry_price,
            side,
            base_sl_pct,
            base_tp_pct,
            volatility_multiplier,
        );

        let position = Position {
            side,
            entry_price,
            size,
            stop_loss: risk_params.stop_loss_price,
            take_profit: risk_params.take_profit_price,
            opened_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };

        *self.position.write().await = Some(position);

        risk_params
    }

    pub async fn close_position(&self) {
        *self.position.write().await = None;
    }

    pub async fn get_position_details(&self) -> Option<Position> {
        self.position.read().await.clone()
    }

    pub async fn check_exit(&self, current_price: f64) -> Option<ExitReason> {
        let position = self.position.read().await;
        
        if let Some(pos) = position.as_ref() {
            match pos.side {
                TradingSide::Buy => {
                    if current_price <= pos.stop_loss {
                        return Some(ExitReason::StopLoss);
                    }
                    if current_price >= pos.take_profit {
                        return Some(ExitReason::TakeProfit);
                    }
                }
                TradingSide::Sell => {
                    if current_price >= pos.stop_loss {
                        return Some(ExitReason::StopLoss);
                    }
                    if current_price <= pos.take_profit {
                        return Some(ExitReason::TakeProfit);
                    }
                }
            }
        }
        
        None
    }

    pub async fn get_pnl(&self, current_price: f64) -> Option<f64> {
        let position = self.position.read().await;
        
        if let Some(pos) = position.as_ref() {
            let pnl = match pos.side {
                TradingSide::Buy => (current_price - pos.entry_price) / pos.entry_price * 100.0,
                TradingSide::Sell => (pos.entry_price - current_price) / pos.entry_price * 100.0,
            };
            return Some(pnl);
        }
        
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExitReason {
    StopLoss,
    TakeProfit,
    SignalReversal,
    Manual,
}

impl Default for PositionManager {
    fn default() -> Self {
        Self::new()
    }
}
