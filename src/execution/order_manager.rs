use anyhow::Result;
use std::sync::Arc;

use crate::api::KuCoinClient;
use crate::core::Config;
use crate::strategy::TradeSignal;

use super::position_tracker::{Position, PositionSide, PositionTracker};

pub struct OrderManager {
    client: Arc<KuCoinClient>,
    config: Config,
    position_tracker: Arc<PositionTracker>,
    dry_run: bool, // Safety: true = simulate orders, false = real orders
}

impl OrderManager {
    pub fn new(
        client: Arc<KuCoinClient>,
        config: Config,
        position_tracker: Arc<PositionTracker>,
    ) -> Self {
        Self {
            client,
            config,
            position_tracker,
            dry_run: true, // Start in dry run mode for safety
        }
    }

    pub fn enable_live_trading(&mut self) {
        tracing::warn!("⚠️  LIVE TRADING ENABLED - Real orders will be placed!");
        self.dry_run = false;
    }

    pub fn enable_dry_run(&mut self) {
        tracing::info!("✅ Dry run mode enabled - Orders will be simulated");
        self.dry_run = true;
    }

    pub async fn execute_signal(&self, signal: &TradeSignal) -> Result<bool> {
        // Check if we already have a position in this symbol
        if self.position_tracker.has_position(&signal.symbol).await {
            tracing::debug!("Already have position in {}, skipping", signal.symbol);
            return Ok(false);
        }

        // Check position limits
        let position_count = self.position_tracker.position_count().await;
        if position_count >= 3 {
            // Max 3 concurrent positions
            tracing::warn!(
                "Max position limit reached ({}/3), skipping {}",
                position_count,
                signal.symbol
            );
            return Ok(false);
        }

        // Validate signal
        if !self.validate_signal(signal)? {
            return Ok(false);
        }

        // Calculate position size
        let position_size = self.calculate_position_size(signal)?;

        tracing::info!(
            "📊 Signal: {} {} @ {:.2} | SL: {:.2} | TP: {:.2} | Size: {:.4} | Lev: {}x | Conf: {:.2}",
            if signal.signal_type == crate::strategy::SignalType::Long {
                "LONG"
            } else {
                "SHORT"
            },
            signal.symbol,
            signal.entry_price,
            signal.stop_loss,
            signal.take_profit,
            position_size,
            signal.recommended_leverage,
            signal.confidence
        );

        if self.dry_run {
            tracing::info!("🔸 DRY RUN: Order simulated (not executed)");
            self.simulate_order(signal, position_size).await?;
        } else {
            tracing::warn!("🔴 LIVE: Placing real order on KuCoin");
            self.place_real_order(signal, position_size).await?;
        }

        Ok(true)
    }

    fn validate_signal(&self, signal: &TradeSignal) -> Result<bool> {
        // Check confidence threshold
        if signal.confidence < 0.7 {
            tracing::debug!(
                "Signal confidence too low: {:.2} < 0.7",
                signal.confidence
            );
            return Ok(false);
        }

        // Check leverage limits
        if signal.recommended_leverage > self.config.trading.max_leverage {
            tracing::warn!(
                "Signal leverage {} > max {}, capping",
                signal.recommended_leverage,
                self.config.trading.max_leverage
            );
        }

        // Check position size limits
        if signal.recommended_size > self.config.trading.max_position_size {
            tracing::warn!(
                "Signal size {:.2} > max {:.2}, capping",
                signal.recommended_size,
                self.config.trading.max_position_size
            );
        }

        Ok(true)
    }

    fn calculate_position_size(&self, signal: &TradeSignal) -> Result<f64> {
        // Get account balance (simplified - in production, fetch from API)
        let account_balance = 1000.0; // Placeholder

        // Cap position size to config limits
        let size_percent = signal
            .recommended_size
            .min(self.config.trading.max_position_size);

        // Calculate actual position size
        let position_size = account_balance * size_percent;

        Ok(position_size)
    }

    async fn simulate_order(&self, signal: &TradeSignal, size: f64) -> Result<()> {
        let side = match signal.signal_type {
            crate::strategy::SignalType::Long => PositionSide::Long,
            crate::strategy::SignalType::Short => PositionSide::Short,
            _ => return Ok(()),
        };

        let leverage = signal
            .recommended_leverage
            .min(self.config.trading.max_leverage);

        let position = Position::new(
            signal.symbol.clone(),
            side,
            signal.entry_price,
            size,
            leverage,
            signal.stop_loss,
            signal.take_profit,
        );

        self.position_tracker.add_position(position).await;

        tracing::info!(
            "✅ Simulated position opened: {} {} @ {:.2}",
            match side {
                PositionSide::Long => "LONG",
                PositionSide::Short => "SHORT",
            },
            signal.symbol,
            signal.entry_price
        );

        Ok(())
    }

    async fn place_real_order(&self, signal: &TradeSignal, size: f64) -> Result<()> {
        // This would call KuCoin API to place actual order
        // For safety, we'll add extensive validation and logging

        tracing::warn!("🚨 REAL ORDER PLACEMENT NOT IMPLEMENTED YET");
        tracing::warn!("🚨 Enable this only after thorough testing!");
        tracing::warn!("🚨 Would place: {} {} @ {:.2} size {:.4}", 
            signal.signal_type,
            signal.symbol,
            signal.entry_price,
            size
        );

        // TODO: Implement actual KuCoin order placement
        // Steps would be:
        // 1. Set leverage: client.set_leverage(symbol, leverage)
        // 2. Place market order: client.place_order(symbol, side, size, leverage)
        // 3. Set stop loss: client.set_stop_loss(...)
        // 4. Set take profit: client.set_take_profit(...)
        // 5. Track position

        Ok(())
    }

    pub async fn check_positions(&self) -> Result<()> {
        let positions = self.position_tracker.get_all_positions().await;

        for position in positions {
            // Update current price from market
            // In production, fetch real price from API
            // For now, we'll keep the position as-is

            if position.should_close() {
                let reason = position.close_reason();
                tracing::info!(
                    "🔔 Position {} hit {} at {:.2} | PnL: {:.2}",
                    position.symbol,
                    reason,
                    position.current_price,
                    position.unrealized_pnl
                );

                if self.dry_run {
                    self.close_position_simulated(&position).await?;
                } else {
                    self.close_position_real(&position).await?;
                }
            }
        }

        Ok(())
    }

    async fn close_position_simulated(&self, position: &Position) -> Result<()> {
        self.position_tracker
            .remove_position(&position.symbol)
            .await;

        tracing::info!(
            "✅ Simulated position closed: {} | PnL: {:.2} ({:.2}%)",
            position.symbol,
            position.unrealized_pnl,
            (position.unrealized_pnl / position.size) * 100.0
        );

        Ok(())
    }

    async fn close_position_real(&self, position: &Position) -> Result<()> {
        tracing::warn!("🚨 REAL POSITION CLOSE NOT IMPLEMENTED YET");
        tracing::warn!(
            "🚨 Would close: {} at {:.2}",
            position.symbol,
            position.current_price
        );

        // TODO: Implement actual position closing
        // client.close_position(symbol)

        Ok(())
    }

    pub async fn get_position_summary(&self) -> String {
        let positions = self.position_tracker.get_all_positions().await;
        let total_pnl = self.position_tracker.total_pnl().await;

        if positions.is_empty() {
            return "No open positions".to_string();
        }

        let mut summary = format!("Open Positions ({}):\n", positions.len());
        for pos in positions {
            summary.push_str(&format!(
                "  • {} {} @ {:.2} | Current: {:.2} | PnL: {:.2}\n",
                pos.symbol,
                match pos.side {
                    PositionSide::Long => "LONG",
                    PositionSide::Short => "SHORT",
                },
                pos.entry_price,
                pos.current_price,
                pos.unrealized_pnl
            ));
        }
        summary.push_str(&format!("Total PnL: {:.2}", total_pnl));

        summary
    }
}

