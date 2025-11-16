use anyhow::Result;

#[derive(Debug, Clone)]
pub struct RiskConfig {
    pub max_position_size_pct: f64,      // Max 20% of account per position
    pub max_daily_loss_pct: f64,         // Max 5% daily loss
    pub max_total_leverage: f64,         // Max combined leverage
    pub min_account_balance: f64,        // Don't trade below this
    pub max_concurrent_positions: usize, // Max open positions
}

impl Default for RiskConfig {
    fn default() -> Self {
        Self {
            max_position_size_pct: 0.20,    // 20%
            max_daily_loss_pct: 0.05,       // 5%
            max_total_leverage: 50.0,       // 50x total
            min_account_balance: 10.0,      // $10 minimum
            max_concurrent_positions: 3,     // 3 positions max
        }
    }
}

#[derive(Debug)]
pub struct RiskManager {
    config: RiskConfig,
    daily_pnl: f64,
}

impl RiskManager {
    pub fn new(config: RiskConfig) -> Self {
        Self {
            config,
            daily_pnl: 0.0,
        }
    }

    pub fn can_open_position(
        &self,
        account_balance: f64,
        current_positions: usize,
        position_size: f64,
    ) -> Result<bool> {
        // Check minimum balance
        if account_balance < self.config.min_account_balance {
            tracing::warn!(
                "❌ Account balance too low: ${:.2} < ${:.2}",
                account_balance,
                self.config.min_account_balance
            );
            return Ok(false);
        }

        // Check daily loss limit
        if self.daily_pnl < -(account_balance * self.config.max_daily_loss_pct) {
            tracing::warn!(
                "❌ Daily loss limit reached: ${:.2}",
                self.daily_pnl
            );
            return Ok(false);
        }

        // Check max concurrent positions
        if current_positions >= self.config.max_concurrent_positions {
            tracing::warn!(
                "❌ Max concurrent positions reached: {}",
                current_positions
            );
            return Ok(false);
        }

        // Check position size
        let position_pct = position_size / account_balance;
        if position_pct > self.config.max_position_size_pct {
            tracing::warn!(
                "❌ Position too large: {:.1}% > {:.1}%",
                position_pct * 100.0,
                self.config.max_position_size_pct * 100.0
            );
            return Ok(false);
        }

        Ok(true)
    }

    pub fn calculate_position_size(
        &self,
        account_balance: f64,
        confidence: f64,
        leverage: u32,
    ) -> f64 {
        // Base size on confidence
        let base_pct: f64 = if confidence > 0.9 {
            0.15 // 15% for very high confidence
        } else if confidence > 0.8 {
            0.10 // 10% for high confidence
        } else if confidence > 0.7 {
            0.07 // 7% for medium confidence
        } else {
            0.05 // 5% for low confidence
        };

        // Cap at max position size
        let size_pct = base_pct.min(self.config.max_position_size_pct);
        
        // Calculate actual size
        let position_value = account_balance * size_pct;
        
        tracing::debug!(
            "📊 Position sizing: balance=${:.2}, confidence={:.2}, leverage={}x, size={:.2}%",
            account_balance, confidence, leverage, size_pct * 100.0
        );
        
        position_value
    }

    pub fn update_daily_pnl(&mut self, pnl: f64) {
        self.daily_pnl += pnl;
        tracing::info!("💰 Daily P&L updated: ${:.2}", self.daily_pnl);
    }

    pub fn reset_daily_pnl(&mut self) {
        tracing::info!("🔄 Resetting daily P&L (was: ${:.2})", self.daily_pnl);
        self.daily_pnl = 0.0;
    }

    pub fn get_daily_pnl(&self) -> f64 {
        self.daily_pnl
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_position_sizing() {
        let risk_mgr = RiskManager::new(RiskConfig::default());
        
        let size = risk_mgr.calculate_position_size(1000.0, 0.85, 5);
        assert!(size > 0.0);
        assert!(size <= 200.0); // Max 20% of 1000
    }

    #[test]
    fn test_risk_checks() {
        let risk_mgr = RiskManager::new(RiskConfig::default());
        
        // Should allow normal position
        assert!(risk_mgr.can_open_position(1000.0, 1, 150.0).unwrap());
        
        // Should block too many positions
        assert!(!risk_mgr.can_open_position(1000.0, 5, 150.0).unwrap());
        
        // Should block low balance
        assert!(!risk_mgr.can_open_position(5.0, 0, 150.0).unwrap());
    }
}

