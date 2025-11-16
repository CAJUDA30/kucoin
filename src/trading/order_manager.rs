use anyhow::Result;
use std::sync::Arc;
use tokio::sync::RwLock;
use std::collections::HashMap;
use uuid::Uuid;

use crate::api::{KuCoinClient, OrderRequest, Order};
use crate::core::Config;
use super::risk_manager::RiskManager;

#[derive(Clone)]
pub struct OrderManager {
    client: Arc<KuCoinClient>,
    config: Config,
    active_orders: Arc<RwLock<HashMap<String, Order>>>,
    paper_trading: bool,
    risk_manager: Arc<RwLock<RiskManager>>,
}

impl OrderManager {
    pub fn new(
        client: Arc<KuCoinClient>,
        config: Config,
        paper_trading: bool,
        risk_manager: Arc<RwLock<RiskManager>>,
    ) -> Self {
        Self {
            client,
            config,
            active_orders: Arc::new(RwLock::new(HashMap::new())),
            paper_trading,
            risk_manager,
        }
    }

    pub async fn place_market_order(
        &self,
        symbol: &str,
        side: &str, // "buy" or "sell"
        size: i64,
        leverage: u32,
    ) -> Result<String> {
        if self.paper_trading {
            return self.place_paper_order(symbol, side, "market", None, size, leverage).await;
        }

        // Check risk limits
        let account = self.client.get_account_overview_currency("USDT").await?;
        let positions = self.client.get_positions().await?;
        let position_value = size as f64; // Simplified
        
        let risk_mgr = self.risk_manager.read().await;
        if !risk_mgr.can_open_position(
            account.available_balance,
            positions.len(),
            position_value,
        )? {
            return Err(anyhow::anyhow!("Risk check failed - position blocked"));
        }
        drop(risk_mgr);

        let order = OrderRequest {
            client_oid: Uuid::new_v4().to_string(),
            side: side.to_string(),
            symbol: symbol.to_string(),
            order_type: "market".to_string(),
            leverage: leverage.to_string(),
            price: None,
            size: Some(size),
            time_in_force: Some("GTC".to_string()),
            post_only: Some(false),
            reduce_only: Some(false),
            stop: None,
            stop_price: None,
        };

        tracing::info!(
            "🎯 Placing REAL {} market order: {} {} lots @ {}x leverage",
            side.to_uppercase(),
            symbol,
            size,
            leverage
        );

        let response = self.client.place_order(order).await?;
        
        tracing::info!("✅ Order placed: {}", response.order_id);
        
        Ok(response.order_id)
    }

    pub async fn place_limit_order(
        &self,
        symbol: &str,
        side: &str,
        price: f64,
        size: i64,
        leverage: u32,
    ) -> Result<String> {
        if self.paper_trading {
            return self.place_paper_order(symbol, side, "limit", Some(price), size, leverage).await;
        }

        // Check risk limits
        let account = self.client.get_account_overview_currency("USDT").await?;
        let positions = self.client.get_positions().await?;
        let position_value = price * size as f64;
        
        let risk_mgr = self.risk_manager.read().await;
        if !risk_mgr.can_open_position(
            account.available_balance,
            positions.len(),
            position_value,
        )? {
            return Err(anyhow::anyhow!("Risk check failed - position blocked"));
        }
        drop(risk_mgr);

        let order = OrderRequest {
            client_oid: Uuid::new_v4().to_string(),
            side: side.to_string(),
            symbol: symbol.to_string(),
            order_type: "limit".to_string(),
            leverage: leverage.to_string(),
            price: Some(price.to_string()),
            size: Some(size),
            time_in_force: Some("GTC".to_string()),
            post_only: Some(true),
            reduce_only: Some(false),
            stop: None,
            stop_price: None,
        };

        tracing::info!(
            "🎯 Placing REAL {} limit order: {} {} lots @ ${:.2} ({}x leverage)",
            side.to_uppercase(),
            symbol,
            size,
            price,
            leverage
        );

        let response = self.client.place_order(order).await?;
        
        tracing::info!("✅ Order placed: {}", response.order_id);
        
        Ok(response.order_id)
    }

    async fn place_paper_order(
        &self,
        symbol: &str,
        side: &str,
        order_type: &str,
        price: Option<f64>,
        size: i64,
        leverage: u32,
    ) -> Result<String> {
        let order_id = Uuid::new_v4().to_string();
        
        if order_type == "market" {
            tracing::info!(
                "📝 PAPER TRADE: {} MARKET {} - {} lots @ {}x leverage [{}]",
                side.to_uppercase(),
                symbol,
                size,
                leverage,
                &order_id[..8]
            );
        } else {
            tracing::info!(
                "📝 PAPER TRADE: {} LIMIT {} - {} lots @ ${:.2} ({}x leverage) [{}]",
                side.to_uppercase(),
                symbol,
                size,
                price.unwrap_or(0.0),
                leverage,
                &order_id[..8]
            );
        }
        
        Ok(order_id)
    }

    pub async fn place_stop_loss(
        &self,
        symbol: &str,
        side: &str, // "buy" or "sell" (opposite of main position)
        stop_price: f64,
        size: i64,
        leverage: u32,
    ) -> Result<String> {
        if self.paper_trading {
            tracing::info!(
                "📝 PAPER STOP-LOSS: {} {} @ ${:.2} ({} lots)",
                side.to_uppercase(),
                symbol,
                stop_price,
                size
            );
            return Ok(Uuid::new_v4().to_string());
        }

        let order = OrderRequest {
            client_oid: Uuid::new_v4().to_string(),
            side: side.to_string(),
            symbol: symbol.to_string(),
            order_type: "market".to_string(),
            leverage: leverage.to_string(),
            price: None,
            size: Some(size),
            time_in_force: Some("GTC".to_string()),
            post_only: Some(false),
            reduce_only: Some(true),
            stop: Some(if side == "buy" { "up" } else { "down" }.to_string()),
            stop_price: Some(stop_price.to_string()),
        };

        tracing::info!(
            "🛑 Placing REAL stop-loss: {} {} @ ${:.2}",
            side.to_uppercase(),
            symbol,
            stop_price
        );

        let response = self.client.place_order(order).await?;
        
        tracing::info!("✅ Stop-loss placed: {}", response.order_id);
        
        Ok(response.order_id)
    }

    pub async fn cancel_order(&self, order_id: &str) -> Result<()> {
        if self.paper_trading {
            tracing::info!("📝 PAPER: Would cancel order {}", &order_id[..8]);
            return Ok(());
        }

        self.client.cancel_order(order_id).await?;
        
        tracing::info!("❌ Order cancelled: {}", order_id);
        
        // Remove from active orders
        self.active_orders.write().await.remove(order_id);
        
        Ok(())
    }

    pub async fn sync_orders(&self) -> Result<()> {
        if self.paper_trading {
            return Ok(());
        }

        let orders = self.client.get_orders(None, Some("active")).await?;
        
        let mut active = self.active_orders.write().await;
        active.clear();
        
        for order in orders {
            active.insert(order.id.clone(), order);
        }
        
        tracing::debug!("Synced {} active orders", active.len());
        
        Ok(())
    }

    pub async fn get_active_orders(&self) -> Vec<Order> {
        self.active_orders.read().await.values().cloned().collect()
    }

    pub fn is_paper_trading(&self) -> bool {
        self.paper_trading
    }

    pub fn set_paper_trading(&mut self, enabled: bool) {
        self.paper_trading = enabled;
        tracing::info!(
            "🔄 Trading mode changed to: {}",
            if enabled { "PAPER" } else { "LIVE" }
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_paper_trading_mode() {
        // Test will be added when we integrate
    }
}

