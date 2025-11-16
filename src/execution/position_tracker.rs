use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct Position {
    pub symbol: String,
    pub side: PositionSide,
    pub entry_price: f64,
    pub current_price: f64,
    pub size: f64,
    pub leverage: u32,
    pub stop_loss: f64,
    pub take_profit: f64,
    pub unrealized_pnl: f64,
    pub opened_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PositionSide {
    Long,
    Short,
}

impl Position {
    pub fn new(
        symbol: String,
        side: PositionSide,
        entry_price: f64,
        size: f64,
        leverage: u32,
        stop_loss: f64,
        take_profit: f64,
    ) -> Self {
        Self {
            symbol,
            side,
            entry_price,
            current_price: entry_price,
            size,
            leverage,
            stop_loss,
            take_profit,
            unrealized_pnl: 0.0,
            opened_at: Utc::now(),
        }
    }

    pub fn update_price(&mut self, price: f64) {
        self.current_price = price;
        self.unrealized_pnl = self.calculate_pnl();
    }

    fn calculate_pnl(&self) -> f64 {
        let price_change = match self.side {
            PositionSide::Long => self.current_price - self.entry_price,
            PositionSide::Short => self.entry_price - self.current_price,
        };
        (price_change / self.entry_price) * self.size * self.leverage as f64
    }

    pub fn should_close(&self) -> bool {
        match self.side {
            PositionSide::Long => {
                self.current_price <= self.stop_loss || self.current_price >= self.take_profit
            }
            PositionSide::Short => {
                self.current_price >= self.stop_loss || self.current_price <= self.take_profit
            }
        }
    }

    pub fn close_reason(&self) -> &str {
        match self.side {
            PositionSide::Long => {
                if self.current_price <= self.stop_loss {
                    "Stop Loss"
                } else if self.current_price >= self.take_profit {
                    "Take Profit"
                } else {
                    "Unknown"
                }
            }
            PositionSide::Short => {
                if self.current_price >= self.stop_loss {
                    "Stop Loss"
                } else if self.current_price <= self.take_profit {
                    "Take Profit"
                } else {
                    "Unknown"
                }
            }
        }
    }
}

pub struct PositionTracker {
    positions: Arc<RwLock<HashMap<String, Position>>>,
}

impl PositionTracker {
    pub fn new() -> Self {
        Self {
            positions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn add_position(&self, position: Position) {
        let symbol = position.symbol.clone();
        self.positions.write().await.insert(symbol, position);
    }

    pub async fn remove_position(&self, symbol: &str) -> Option<Position> {
        self.positions.write().await.remove(symbol)
    }

    pub async fn get_position(&self, symbol: &str) -> Option<Position> {
        self.positions.read().await.get(symbol).cloned()
    }

    pub async fn update_price(&self, symbol: &str, price: f64) {
        if let Some(position) = self.positions.write().await.get_mut(symbol) {
            position.update_price(price);
        }
    }

    pub async fn get_all_positions(&self) -> Vec<Position> {
        self.positions.read().await.values().cloned().collect()
    }

    pub async fn has_position(&self, symbol: &str) -> bool {
        self.positions.read().await.contains_key(symbol)
    }

    pub async fn position_count(&self) -> usize {
        self.positions.read().await.len()
    }

    pub async fn total_pnl(&self) -> f64 {
        self.positions
            .read()
            .await
            .values()
            .map(|p| p.unrealized_pnl)
            .sum()
    }
}

