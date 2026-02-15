use std::collections::VecDeque;
use std::time::{SystemTime, UNIX_EPOCH};

/// Price point for ATR calculation
#[derive(Debug, Clone)]
pub struct PricePoint {
    pub timestamp_ms: u64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
}

/// Volatility calculator using Average True Range (ATR)
pub struct VolatilityCalculator {
    /// Historical price points
    price_history: VecDeque<PricePoint>,

    /// ATR period (default: 14)
    atr_period: usize,

    /// Current ATR value
    current_atr: f64,

    /// Simple moving average for ATR smoothing
    atr_history: VecDeque<f64>,
}

impl VolatilityCalculator {
    pub fn new(atr_period: usize) -> Self {
        Self {
            price_history: VecDeque::new(),
            atr_period,
            current_atr: 0.0,
            atr_history: VecDeque::new(),
        }
    }

    /// Add a price point (using bid/ask to simulate high/low)
    pub fn add_price(&mut self, bid: f64, ask: f64) {
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let mid = (bid + ask) / 2.0;

        let price_point = PricePoint {
            timestamp_ms,
            high: ask,
            low: bid,
            close: mid,
        };

        self.price_history.push_back(price_point);

        // Keep only what we need
        while self.price_history.len() > self.atr_period + 1 {
            self.price_history.pop_front();
        }

        // Recalculate ATR if we have enough data
        if self.price_history.len() >= 2 {
            self.current_atr = self.calculate_atr();
        }
    }

    /// Calculate Average True Range
    pub fn calculate_atr(&mut self) -> f64 {
        if self.price_history.len() < 2 {
            return 0.0;
        }

        let mut true_ranges = Vec::new();

        for i in 1..self.price_history.len() {
            let current = &self.price_history[i];
            let previous = &self.price_history[i - 1];

            // True Range is the greatest of:
            // 1. Current High - Current Low
            // 2. |Current High - Previous Close|
            // 3. |Current Low - Previous Close|
            let tr1 = current.high - current.low;
            let tr2 = (current.high - previous.close).abs();
            let tr3 = (current.low - previous.close).abs();

            let true_range = tr1.max(tr2).max(tr3);
            true_ranges.push(true_range);
        }

        if true_ranges.is_empty() {
            return 0.0;
        }

        // Take last N true ranges (up to atr_period)
        let period = self.atr_period.min(true_ranges.len());
        let recent_trs: Vec<f64> = true_ranges.iter().rev().take(period).copied().collect();

        // Calculate average
        let atr = recent_trs.iter().sum::<f64>() / recent_trs.len() as f64;

        // Store in history for smoothing
        self.atr_history.push_back(atr);
        while self.atr_history.len() > 3 {
            self.atr_history.pop_front();
        }

        // Return smoothed ATR (SMA of last 3 ATR values)
        self.atr_history.iter().sum::<f64>() / self.atr_history.len() as f64
    }

    /// Get current ATR value
    pub fn get_atr(&self) -> f64 {
        self.current_atr
    }

    /// Calculate ATR as percentage of price
    pub fn get_atr_pct(&self, current_price: f64) -> f64 {
        if current_price == 0.0 {
            return 0.0;
        }
        self.current_atr / current_price
    }

    /// Suggest dynamic stop loss based on ATR
    /// Formula: base_pct + (ATR% * multiplier)
    /// Clamped between min and max percentages
    pub fn suggest_stop_loss(
        &self,
        base_pct: f64,
        multiplier: f64,
        entry_price: f64,
        min_pct: f64,
        max_pct: f64,
    ) -> f64 {
        let atr_pct = self.get_atr_pct(entry_price);
        let sl_pct = base_pct + (atr_pct * multiplier);
        sl_pct.clamp(min_pct, max_pct)
    }

    /// Suggest dynamic take profit based on ATR
    /// Formula: base_pct + (ATR% * multiplier)
    /// Clamped between min and max percentages
    pub fn suggest_take_profit(
        &self,
        base_pct: f64,
        multiplier: f64,
        entry_price: f64,
        min_pct: f64,
        max_pct: f64,
    ) -> f64 {
        let atr_pct = self.get_atr_pct(entry_price);
        let tp_pct = base_pct + (atr_pct * multiplier);
        tp_pct.clamp(min_pct, max_pct)
    }

    /// Get volatility regime (Low, Medium, High)
    pub fn get_volatility_regime(&self, entry_price: f64) -> VolatilityRegime {
        let atr_pct = self.get_atr_pct(entry_price);

        // For crypto, typical ranges:
        // Low: < 0.5%
        // Medium: 0.5% - 2%
        // High: > 2%
        if atr_pct < 0.005 {
            VolatilityRegime::Low
        } else if atr_pct < 0.02 {
            VolatilityRegime::Medium
        } else {
            VolatilityRegime::High
        }
    }

    /// Calculate position size adjustment based on volatility
    /// Returns a multiplier (0.5 - 1.5) to adjust base position size
    pub fn position_size_multiplier(&self, entry_price: f64) -> f64 {
        let regime = self.get_volatility_regime(entry_price);

        match regime {
            VolatilityRegime::Low => 1.5,    // Increase size in low vol
            VolatilityRegime::Medium => 1.0, // Standard size
            VolatilityRegime::High => 0.5,   // Reduce size in high vol
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VolatilityRegime {
    Low,
    Medium,
    High,
}

impl Default for VolatilityCalculator {
    fn default() -> Self {
        Self::new(14)
    }
}

/// Dynamic risk parameters for a position
#[derive(Debug, Clone)]
pub struct DynamicRiskParams {
    pub stop_loss_pct: f64,
    pub take_profit_pct: f64,
    pub stop_loss_price: f64,
    pub take_profit_price: f64,
    pub position_size_multiplier: f64,
    pub atr_value: f64,
    pub volatility_regime: VolatilityRegime,
}

impl DynamicRiskParams {
    /// Calculate dynamic risk parameters
    pub fn calculate(
        volatility_calc: &VolatilityCalculator,
        entry_price: f64,
        side: crate::strategy::TradingSide,
        base_sl_pct: f64,
        base_tp_pct: f64,
        volatility_multiplier: f64,
    ) -> Self {
        // Calculate dynamic percentages
        let stop_loss_pct = volatility_calc.suggest_stop_loss(
            base_sl_pct,
            volatility_multiplier,
            entry_price,
            0.005, // Min 0.5%
            0.05,  // Max 5%
        );

        let take_profit_pct = volatility_calc.suggest_take_profit(
            base_tp_pct,
            volatility_multiplier * 1.5, // TP multiplier is 1.5x SL multiplier
            entry_price,
            0.01, // Min 1%
            0.10, // Max 10%
        );

        // Calculate actual prices
        let (stop_loss_price, take_profit_price) = match side {
            crate::strategy::TradingSide::Buy => (
                entry_price * (1.0 - stop_loss_pct),
                entry_price * (1.0 + take_profit_pct),
            ),
            crate::strategy::TradingSide::Sell => (
                entry_price * (1.0 + stop_loss_pct),
                entry_price * (1.0 - take_profit_pct),
            ),
        };

        let position_size_multiplier = volatility_calc.position_size_multiplier(entry_price);
        let volatility_regime = volatility_calc.get_volatility_regime(entry_price);

        Self {
            stop_loss_pct,
            take_profit_pct,
            stop_loss_price,
            take_profit_price,
            position_size_multiplier,
            atr_value: volatility_calc.get_atr(),
            volatility_regime,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_atr_calculation() {
        let mut calc = VolatilityCalculator::new(3);

        // Add some price points
        calc.add_price(50000.0, 50010.0);
        calc.add_price(50100.0, 50110.0);
        calc.add_price(50050.0, 50060.0);
        calc.add_price(50200.0, 50210.0);

        let atr = calc.calculate_atr();
        assert!(atr > 0.0);
    }

    #[test]
    fn test_dynamic_stop_loss() {
        let mut calc = VolatilityCalculator::new(3);

        calc.add_price(50000.0, 50100.0); // High volatility
        calc.add_price(50500.0, 50600.0);
        calc.add_price(49500.0, 49600.0);

        calc.calculate_atr();

        let sl_pct = calc.suggest_stop_loss(0.01, 0.5, 50000.0, 0.005, 0.05);

        // Should be higher than base due to volatility
        assert!(sl_pct > 0.01);
    }

    #[test]
    fn test_volatility_regime() {
        let mut calc = VolatilityCalculator::new(3);

        // Low volatility
        calc.add_price(50000.0, 50010.0);
        calc.add_price(50005.0, 50015.0);
        calc.calculate_atr();

        let regime = calc.get_volatility_regime(50000.0);
        assert_eq!(regime, VolatilityRegime::Low);
    }

    #[test]
    fn test_position_size_adjustment() {
        let mut calc = VolatilityCalculator::new(3);

        // Low volatility - should increase size
        calc.add_price(50000.0, 50010.0);
        calc.add_price(50005.0, 50015.0);
        calc.calculate_atr();

        let multiplier = calc.position_size_multiplier(50000.0);
        assert!(multiplier > 1.0);
    }
}
