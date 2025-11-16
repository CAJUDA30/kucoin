use anyhow::Result;
use super::{TradeContext, DataQualityManager};

#[derive(Debug, Clone)]
pub enum ValidationLayer {
    DataQuality,
    MarketConditions,
    RiskLimits,
    Regulatory,
    Confidence,
}

#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub layer: ValidationLayer,
    pub passed: bool,
    pub reason: String,
    pub score: f64,
}

pub struct PreTradeValidator {
    data_quality_mgr: DataQualityManager,
    min_confidence: f64,
    max_spread_bps: f64,
    min_liquidity_usd: f64,
}

impl PreTradeValidator {
    pub fn new() -> Self {
        Self {
            data_quality_mgr: DataQualityManager::new(),
            min_confidence: 0.75,
            max_spread_bps: 50.0,
            min_liquidity_usd: 10000.0,
        }
    }

    pub async fn validate(&self, context: &TradeContext) -> Result<Vec<ValidationResult>> {
        let mut results = Vec::new();

        // Layer 1: Data Quality
        results.push(self.validate_data_quality(context).await?);

        // Layer 2: Market Conditions
        results.push(self.validate_market_conditions(context).await?);

        // Layer 3: Risk Limits
        results.push(self.validate_risk_limits(context).await?);

        // Layer 4: Regulatory
        results.push(self.validate_regulatory(context).await?);

        // Layer 5: Confidence
        results.push(self.validate_confidence(context).await?);

        Ok(results)
    }

    pub fn can_trade(&self, results: &[ValidationResult]) -> bool {
        // ALL layers must pass
        results.iter().all(|r| r.passed)
    }

    async fn validate_data_quality(&self, context: &TradeContext) -> Result<ValidationResult> {
        let checks = self.data_quality_mgr.validate(&context.market_data)?;
        let passed = self.data_quality_mgr.is_valid(&checks);
        let score = self.data_quality_mgr.get_overall_score(&checks);

        Ok(ValidationResult {
            layer: ValidationLayer::DataQuality,
            passed,
            reason: if passed {
                format!("Data quality excellent: {:.1}%", score * 100.0)
            } else {
                "Data quality insufficient".to_string()
            },
            score,
        })
    }

    async fn validate_market_conditions(&self, context: &TradeContext) -> Result<ValidationResult> {
        let data = &context.market_data;
        
        let spread_ok = data.spread_bps() < self.max_spread_bps;
        let liquidity_ok = data.bid_volume + data.ask_volume > self.min_liquidity_usd;
        let volume_ok = data.volume_24h > 1000.0;

        let passed = spread_ok && liquidity_ok && volume_ok;
        
        let score = if passed { 1.0 } else { 0.5 };

        Ok(ValidationResult {
            layer: ValidationLayer::MarketConditions,
            passed,
            reason: format!(
                "Spread: {:.1}bps, Liquidity: ${:.0}, Volume: ${:.0}",
                data.spread_bps(),
                data.bid_volume + data.ask_volume,
                data.volume_24h
            ),
            score,
        })
    }

    async fn validate_risk_limits(&self, context: &TradeContext) -> Result<ValidationResult> {
        let balance_ok = context.account_balance > 10.0;
        let positions_ok = context.open_positions.len() < 3;
        let daily_loss_ok = context.daily_pnl > -(context.account_balance * 0.05);

        let passed = balance_ok && positions_ok && daily_loss_ok;
        
        let score = if passed { 1.0 } else { 0.0 };

        Ok(ValidationResult {
            layer: ValidationLayer::RiskLimits,
            passed,
            reason: if passed {
                format!(
                    "Balance: ${:.2}, Positions: {}/3, Daily P&L: ${:.2}",
                    context.account_balance,
                    context.open_positions.len(),
                    context.daily_pnl
                )
            } else {
                "Risk limits exceeded".to_string()
            },
            score,
        })
    }

    async fn validate_regulatory(&self, context: &TradeContext) -> Result<ValidationResult> {
        let not_delisted = !context.market_data.is_delisted;
        let trading_hours = true; // Crypto 24/7
        
        let passed = not_delisted && trading_hours;
        
        let score = if passed { 1.0 } else { 0.0 };

        Ok(ValidationResult {
            layer: ValidationLayer::Regulatory,
            passed,
            reason: if passed {
                "All regulatory checks passed".to_string()
            } else if !not_delisted {
                "⚠️ TOKEN DELISTED - CANNOT TRADE".to_string()
            } else {
                "Regulatory check failed".to_string()
            },
            score,
        })
    }

    async fn validate_confidence(&self, context: &TradeContext) -> Result<ValidationResult> {
        let passed = context.confidence_score >= self.min_confidence;
        
        Ok(ValidationResult {
            layer: ValidationLayer::Confidence,
            passed,
            reason: format!(
                "AI Confidence: {:.1}% (min: {:.1}%)",
                context.confidence_score * 100.0,
                self.min_confidence * 100.0
            ),
            score: context.confidence_score,
        })
    }
}

