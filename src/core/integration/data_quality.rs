use anyhow::Result;
use super::UnifiedMarketData;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum QualityLevel {
    Critical,   // MUST pass
    Important,  // SHOULD pass
    Optional,   // CAN fail
}

#[derive(Debug, Clone)]
pub struct QualityCheck {
    pub name: String,
    pub level: QualityLevel,
    pub passed: bool,
    pub message: String,
    pub score: f64,
}

pub struct DataQualityManager {
    min_quality_score: f64,
    max_staleness_ms: u64,
}

impl DataQualityManager {
    pub fn new() -> Self {
        Self {
            min_quality_score: 0.95,
            max_staleness_ms: 5000,
        }
    }

    pub fn validate(&self, data: &UnifiedMarketData) -> Result<Vec<QualityCheck>> {
        let mut checks = Vec::new();

        // CRITICAL CHECKS
        checks.push(self.check_price_validity(data));
        checks.push(self.check_data_freshness(data));
        checks.push(self.check_completeness(data));
        checks.push(self.check_not_delisted(data));

        // IMPORTANT CHECKS
        checks.push(self.check_spread_reasonable(data));
        checks.push(self.check_volume_present(data));
        checks.push(self.check_liquidity_adequate(data));

        // OPTIONAL CHECKS
        checks.push(self.check_funding_rate_present(data));
        checks.push(self.check_mark_price_present(data));

        Ok(checks)
    }

    pub fn is_valid(&self, checks: &[QualityCheck]) -> bool {
        // All critical checks must pass
        let critical_pass = checks
            .iter()
            .filter(|c| c.level == QualityLevel::Critical)
            .all(|c| c.passed);

        if !critical_pass {
            return false;
        }

        // At least 80% of important checks must pass
        let important_checks: Vec<_> = checks
            .iter()
            .filter(|c| c.level == QualityLevel::Important)
            .collect();

        if !important_checks.is_empty() {
            let important_pass_rate = important_checks.iter()
                .filter(|c| c.passed)
                .count() as f64 / important_checks.len() as f64;

            if important_pass_rate < 0.8 {
                return false;
            }
        }

        true
    }

    fn check_price_validity(&self, data: &UnifiedMarketData) -> QualityCheck {
        let passed = data.price > 0.0 && data.price.is_finite();
        
        QualityCheck {
            name: "Price Validity".to_string(),
            level: QualityLevel::Critical,
            passed,
            message: if passed {
                format!("Price valid: ${:.2}", data.price)
            } else {
                "Invalid price".to_string()
            },
            score: if passed { 1.0 } else { 0.0 },
        }
    }

    fn check_data_freshness(&self, data: &UnifiedMarketData) -> QualityCheck {
        let passed = data.data_freshness_ms < self.max_staleness_ms;
        
        QualityCheck {
            name: "Data Freshness".to_string(),
            level: QualityLevel::Critical,
            passed,
            message: if passed {
                format!("Data fresh: {}ms old", data.data_freshness_ms)
            } else {
                format!("Data stale: {}ms old (max: {}ms)", 
                    data.data_freshness_ms, self.max_staleness_ms)
            },
            score: if passed { 1.0 } else { 0.0 },
        }
    }

    fn check_completeness(&self, data: &UnifiedMarketData) -> QualityCheck {
        let passed = data.completeness > 0.99;
        
        QualityCheck {
            name: "Data Completeness".to_string(),
            level: QualityLevel::Critical,
            passed,
            message: format!("Completeness: {:.1}%", data.completeness * 100.0),
            score: data.completeness,
        }
    }

    fn check_not_delisted(&self, data: &UnifiedMarketData) -> QualityCheck {
        let passed = !data.is_delisted;
        
        QualityCheck {
            name: "Not Delisted".to_string(),
            level: QualityLevel::Critical,
            passed,
            message: if passed {
                "Token active".to_string()
            } else {
                "⚠️ TOKEN DELISTED - DO NOT TRADE".to_string()
            },
            score: if passed { 1.0 } else { 0.0 },
        }
    }

    fn check_spread_reasonable(&self, data: &UnifiedMarketData) -> QualityCheck {
        let spread_bps = data.spread_bps();
        let passed = spread_bps < 50.0; // 0.5% max spread
        
        QualityCheck {
            name: "Spread Reasonable".to_string(),
            level: QualityLevel::Important,
            passed,
            message: format!("Spread: {:.1} bps", spread_bps),
            score: if passed { 1.0 } else { 0.5 },
        }
    }

    fn check_volume_present(&self, data: &UnifiedMarketData) -> QualityCheck {
        let passed = data.volume_24h > 0.0;
        
        QualityCheck {
            name: "Volume Present".to_string(),
            level: QualityLevel::Important,
            passed,
            message: format!("24h volume: ${:.0}", data.volume_24h),
            score: if passed { 1.0 } else { 0.0 },
        }
    }

    fn check_liquidity_adequate(&self, data: &UnifiedMarketData) -> QualityCheck {
        let passed = data.liquidity_adequate();
        
        QualityCheck {
            name: "Liquidity Adequate".to_string(),
            level: QualityLevel::Important,
            passed,
            message: format!("Liquidity score: {:.2}", data.liquidity_score),
            score: data.liquidity_score,
        }
    }

    fn check_funding_rate_present(&self, data: &UnifiedMarketData) -> QualityCheck {
        let passed = data.funding_rate != 0.0;
        
        QualityCheck {
            name: "Funding Rate".to_string(),
            level: QualityLevel::Optional,
            passed,
            message: format!("Funding: {:.4}%", data.funding_rate * 100.0),
            score: if passed { 1.0 } else { 0.5 },
        }
    }

    fn check_mark_price_present(&self, data: &UnifiedMarketData) -> QualityCheck {
        let passed = data.mark_price > 0.0;
        
        QualityCheck {
            name: "Mark Price".to_string(),
            level: QualityLevel::Optional,
            passed,
            message: format!("Mark: ${:.2}", data.mark_price),
            score: if passed { 1.0 } else { 0.5 },
        }
    }

    pub fn get_overall_score(&self, checks: &[QualityCheck]) -> f64 {
        if checks.is_empty() {
            return 0.0;
        }

        let total_score: f64 = checks.iter().map(|c| c.score).sum();
        total_score / checks.len() as f64
    }
}

