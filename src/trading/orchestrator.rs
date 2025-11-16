use anyhow::Result;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::api::KuCoinClient;
use crate::core::Config;
use crate::monitoring::TokenRegistry; // ← ADD THIS
use crate::scanner::MarketScanner;
use crate::strategy::AIStrategyEngine;
use super::order_manager::OrderManager;
use super::risk_manager::{RiskManager, RiskConfig};

pub struct TradingOrchestrator {
    scanner: Arc<MarketScanner>,
    order_manager: Arc<OrderManager>,
    risk_manager: Arc<RwLock<RiskManager>>,
    ai_engine: Arc<RwLock<AIStrategyEngine>>,
    token_registry: Arc<TokenRegistry>, // ← ADD THIS
    config: Config,
    enabled: bool,
}

impl TradingOrchestrator {
    pub fn new(
        client: Arc<KuCoinClient>,
        token_registry: Arc<TokenRegistry>, // ← ADD THIS PARAMETER
        config: Config,
        paper_trading: bool,
    ) -> Self {
        let scanner = Arc::new(MarketScanner::new(
            client.clone(),
            token_registry.clone(), // ← PASS IT HERE
        ));
        let risk_manager = Arc::new(RwLock::new(RiskManager::new(RiskConfig::default())));
        let order_manager = Arc::new(OrderManager::new(
            client.clone(),
            config.clone(),
            paper_trading,
            risk_manager.clone(),
        ));
        let ai_engine = Arc::new(RwLock::new(AIStrategyEngine::new()));

        Self {
            scanner,
            order_manager,
            risk_manager,
            ai_engine,
            token_registry, // ← ADD THIS
            config,
            enabled: false, // Start disabled for safety
        }
    }

    pub async fn start(&mut self) -> Result<()> {
        tracing::info!("🚀 Trading Orchestrator starting...");
        
        // Start market scanner
        self.scanner.start().await?;
        
        tracing::info!("✅ Market scanner started");
        
        // Start trading loop
        self.start_trading_loop().await?;
        
        Ok(())
    }

    async fn start_trading_loop(&self) -> Result<()> {
        let scanner = self.scanner.clone();
        let order_manager = self.order_manager.clone();
        let ai_engine = self.ai_engine.clone();
        let risk_manager = self.risk_manager.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(
                tokio::time::Duration::from_secs(30) // Analyze every 30 seconds
            );
            
            loop {
                interval.tick().await;
                
                // Get top opportunities from scanner
                match scanner.get_top_opportunities(5).await {
                    Ok(opportunities) => {
                        if opportunities.is_empty() {
                            tracing::debug!("📊 No trading opportunities found");
                            continue;
                        }
                        
                        tracing::info!(
                            "📊 Found {} opportunities to analyze",
                            opportunities.len()
                        );
                        
                        for opp in opportunities {
                            // Get detailed snapshot
                            if let Some(snapshot) = scanner.get_snapshot(&opp.symbol).await {
                                // Run AI analysis
                                let mut engine = ai_engine.write().await;
                                match engine.analyze(&snapshot) {
                                    Ok(Some(signal)) => {
                                        tracing::info!(
                                            "🎯 SIGNAL: {} {} @ ${:.2} (confidence: {:.2})",
                                            signal.symbol,
                                            match signal.signal_type {
                                                crate::strategy::SignalType::Long => "LONG",
                                                crate::strategy::SignalType::Short => "SHORT",
                                                _ => "HOLD",
                                            },
                                            signal.entry_price,
                                            signal.confidence
                                        );
                                        
                                        // Only execute if confidence is high enough
                                        if signal.confidence >= 0.75 {
                                            let side = match signal.signal_type {
                                                crate::strategy::SignalType::Long => "buy",
                                                crate::strategy::SignalType::Short => "sell",
                                                _ => continue,
                                            };
                                            
                                            // Calculate position size
                                            let risk_mgr = risk_manager.read().await;
                                            let _position_value = risk_mgr.calculate_position_size(
                                                snapshot.price, // Using price as proxy for balance
                                                signal.confidence,
                                                signal.recommended_leverage,
                                            );
                                            drop(risk_mgr);
                                            
                                            // Place order (paper or real based on mode)
                                            match order_manager.place_limit_order(
                                                &signal.symbol,
                                                side,
                                                signal.entry_price,
                                                1, // 1 lot for now
                                                signal.recommended_leverage,
                                            ).await {
                                                Ok(order_id) => {
                                                    tracing::info!(
                                                        "✅ Order executed: {}",
                                                        &order_id[..8]
                                                    );
                                                    
                                                    // Place stop-loss
                                                    let sl_side = if side == "buy" { "sell" } else { "buy" };
                                                    if let Err(e) = order_manager.place_stop_loss(
                                                        &signal.symbol,
                                                        sl_side,
                                                        signal.stop_loss,
                                                        1,
                                                        signal.recommended_leverage,
                                                    ).await {
                                                        tracing::error!(
                                                            "❌ Failed to place stop-loss: {}",
                                                            e
                                                        );
                                                    }
                                                }
                                                Err(e) => {
                                                    tracing::error!(
                                                        "❌ Failed to execute order: {}",
                                                        e
                                                    );
                                                }
                                            }
                                        } else {
                                            tracing::debug!(
                                                "⏸️  Signal confidence too low: {:.2}",
                                                signal.confidence
                                            );
                                        }
                                    }
                                    Ok(None) => {
                                        tracing::debug!("📊 No signal for {}", snapshot.symbol);
                                    }
                                    Err(e) => {
                                        tracing::error!(
                                            "❌ AI analysis error for {}: {}",
                                            snapshot.symbol,
                                            e
                                        );
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("❌ Failed to get opportunities: {}", e);
                    }
                }
                
                // Sync orders periodically
                if let Err(e) = order_manager.sync_orders().await {
                    tracing::error!("❌ Failed to sync orders: {}", e);
                }
            }
        });
        
        Ok(())
    }

    pub fn enable_trading(&mut self) {
        self.enabled = true;
        tracing::warn!("⚠️  TRADING ENABLED - Bot will execute trades automatically!");
    }

    pub fn disable_trading(&mut self) {
        self.enabled = false;
        tracing::info!("✅ Trading disabled - Bot in monitoring mode only");
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

