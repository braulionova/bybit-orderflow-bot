use std::collections::VecDeque;
use std::time::{SystemTime, UNIX_EPOCH};
use super::Orderbook;

/// Result of orderbook validation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationResult {
    Valid,
    WideSpread,         // Spread > threshold times normal
    LowLiquidity,       // Liquidity < threshold times normal
    StaleData,          // Data older than max age
    PriceAnomaly,       // Bid >= Ask (crossed book)
    InsufficientDepth,  // Less than minimum required levels
}

/// Historical measurement for calculating normal ranges
#[derive(Debug, Clone)]
struct Measurement {
    timestamp_ms: u64,
    spread_pct: f64,
    liquidity: f64,
}

/// Validator to prevent trading in anomalous orderbook conditions
pub struct OrderbookValidator {
    /// Historical measurements for calculating normal ranges
    measurements: VecDeque<Measurement>,

    /// Maximum history size
    max_history_size: usize,

    /// Normal spread range (min, max)
    normal_spread_range: (f64, f64),

    /// Normal liquidity range (min, max)
    normal_liquidity_range: (f64, f64),

    /// Configuration
    config: ValidationConfig,
}

#[derive(Debug, Clone)]
pub struct ValidationConfig {
    /// Maximum spread multiplier (e.g., 3.0 = spread > 3x normal is invalid)
    pub max_spread_multiplier: f64,

    /// Minimum liquidity multiplier (e.g., 0.25 = liquidity < 25% normal is invalid)
    pub min_liquidity_multiplier: f64,

    /// Maximum data age in milliseconds
    pub max_data_age_ms: u64,

    /// Minimum number of depth levels required
    pub min_depth_levels: usize,

    /// Enable validation
    pub enabled: bool,
}

impl Default for ValidationConfig {
    fn default() -> Self {
        Self {
            max_spread_multiplier: 3.0,
            min_liquidity_multiplier: 0.25,
            max_data_age_ms: 5000, // 5 seconds
            min_depth_levels: 5,
            enabled: true,
        }
    }
}

impl OrderbookValidator {
    pub fn new(config: ValidationConfig) -> Self {
        Self {
            measurements: VecDeque::new(),
            max_history_size: 100, // Keep last 100 measurements
            normal_spread_range: (0.0, 1.0), // Will be updated
            normal_liquidity_range: (0.0, 100.0), // Will be updated
            config,
        }
    }

    /// Validate orderbook before generating signals
    pub fn validate(&mut self, orderbook: &Orderbook) -> ValidationResult {
        if !self.config.enabled {
            return ValidationResult::Valid;
        }

        let (bid, ask) = orderbook.best_bid_ask();

        // 1. Check for price anomaly (crossed book)
        if bid >= ask && bid != 0.0 && ask != f64::from_bits(u64::MAX) {
            return ValidationResult::PriceAnomaly;
        }

        // 2. Check if orderbook is initialized
        if bid == 0.0 || ask == 0.0 || ask == f64::from_bits(u64::MAX) {
            return ValidationResult::InsufficientDepth;
        }

        // 3. Check data staleness
        let latency_ms = orderbook.latency_ms();
        if latency_ms > self.config.max_data_age_ms {
            return ValidationResult::StaleData;
        }

        // 4. Check depth levels
        let (bid_levels, ask_levels) = orderbook.get_sorted_levels(self.config.min_depth_levels);
        if bid_levels.len() < self.config.min_depth_levels
            || ask_levels.len() < self.config.min_depth_levels {
            return ValidationResult::InsufficientDepth;
        }

        // 5. Check spread
        let spread_pct = orderbook.spread_pct();
        if self.measurements.len() >= 10 { // Need history to validate
            let max_normal_spread = self.normal_spread_range.1;
            if spread_pct > max_normal_spread * self.config.max_spread_multiplier {
                return ValidationResult::WideSpread;
            }
        }

        // 6. Check liquidity
        let liquidity = orderbook.liquidity_depth(10);
        if self.measurements.len() >= 10 { // Need history to validate
            let min_normal_liquidity = self.normal_liquidity_range.0;
            if liquidity < min_normal_liquidity * self.config.min_liquidity_multiplier {
                return ValidationResult::LowLiquidity;
            }
        }

        // Update normal ranges with current measurement
        self.update_normal_ranges(spread_pct, liquidity);

        ValidationResult::Valid
    }

    /// Update normal ranges based on historical measurements
    fn update_normal_ranges(&mut self, spread_pct: f64, liquidity: f64) {
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let measurement = Measurement {
            timestamp_ms,
            spread_pct,
            liquidity,
        };

        self.measurements.push_back(measurement);

        // Remove old measurements
        while self.measurements.len() > self.max_history_size {
            self.measurements.pop_front();
        }

        // Calculate percentiles for spread
        if !self.measurements.is_empty() {
            let mut spreads: Vec<f64> = self.measurements.iter()
                .map(|m| m.spread_pct)
                .collect();
            spreads.sort_by(|a, b| a.partial_cmp(b).unwrap());

            // Use 10th and 90th percentiles for normal range
            let p10_idx = (spreads.len() as f64 * 0.10) as usize;
            let p90_idx = (spreads.len() as f64 * 0.90) as usize;

            self.normal_spread_range = (
                spreads[p10_idx.min(spreads.len() - 1)],
                spreads[p90_idx.min(spreads.len() - 1)],
            );

            // Calculate percentiles for liquidity
            let mut liquidities: Vec<f64> = self.measurements.iter()
                .map(|m| m.liquidity)
                .collect();
            liquidities.sort_by(|a, b| a.partial_cmp(b).unwrap());

            self.normal_liquidity_range = (
                liquidities[p10_idx.min(liquidities.len() - 1)],
                liquidities[p90_idx.min(liquidities.len() - 1)],
            );
        }
    }

    /// Get current normal ranges (for logging/debugging)
    pub fn get_normal_ranges(&self) -> ((f64, f64), (f64, f64)) {
        (self.normal_spread_range, self.normal_liquidity_range)
    }

    /// Check if validator has enough history
    pub fn is_calibrated(&self) -> bool {
        self.measurements.len() >= 10
    }
}

impl ValidationResult {
    /// Convert validation result to user-friendly message
    pub fn to_string(&self) -> &str {
        match self {
            ValidationResult::Valid => "Valid",
            ValidationResult::WideSpread => "Wide Spread",
            ValidationResult::LowLiquidity => "Low Liquidity",
            ValidationResult::StaleData => "Stale Data",
            ValidationResult::PriceAnomaly => "Price Anomaly",
            ValidationResult::InsufficientDepth => "Insufficient Depth",
        }
    }

    /// Check if trading should be allowed
    pub fn is_valid(&self) -> bool {
        matches!(self, ValidationResult::Valid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_orderbook() -> Orderbook {
        let ob = Orderbook::new("BTCUSDT".to_string());

        let bids = vec![
            (50000.0, 1.0),
            (49999.0, 1.0),
            (49998.0, 1.0),
            (49997.0, 1.0),
            (49996.0, 1.0),
        ];

        let asks = vec![
            (50001.0, 1.0),
            (50002.0, 1.0),
            (50003.0, 1.0),
            (50004.0, 1.0),
            (50005.0, 1.0),
        ];

        ob.apply_snapshot(bids, asks);
        ob
    }

    #[test]
    fn test_valid_orderbook() {
        let mut validator = OrderbookValidator::new(ValidationConfig::default());
        let ob = create_test_orderbook();

        let result = validator.validate(&ob);
        assert_eq!(result, ValidationResult::Valid);
    }

    #[test]
    fn test_crossed_book() {
        let mut validator = OrderbookValidator::new(ValidationConfig::default());
        let ob = Orderbook::new("BTCUSDT".to_string());

        // Crossed book: bid > ask
        let bids = vec![(50002.0, 1.0)];
        let asks = vec![(50001.0, 1.0)];

        ob.apply_snapshot(bids, asks);

        let result = validator.validate(&ob);
        assert_eq!(result, ValidationResult::PriceAnomaly);
    }

    #[test]
    fn test_insufficient_depth() {
        let mut validator = OrderbookValidator::new(ValidationConfig::default());
        let ob = Orderbook::new("BTCUSDT".to_string());

        // Only 2 levels
        let bids = vec![(50000.0, 1.0), (49999.0, 1.0)];
        let asks = vec![(50001.0, 1.0), (50002.0, 1.0)];

        ob.apply_snapshot(bids, asks);

        let result = validator.validate(&ob);
        assert_eq!(result, ValidationResult::InsufficientDepth);
    }

    #[test]
    fn test_normal_range_calculation() {
        let mut validator = OrderbookValidator::new(ValidationConfig::default());
        let ob = create_test_orderbook();

        // Add multiple measurements
        for _ in 0..20 {
            validator.validate(&ob);
        }

        assert!(validator.is_calibrated());

        let (spread_range, liquidity_range) = validator.get_normal_ranges();
        assert!(spread_range.0 > 0.0);
        assert!(spread_range.1 > spread_range.0);
        assert!(liquidity_range.0 > 0.0);
        assert!(liquidity_range.1 > liquidity_range.0);
    }
}
