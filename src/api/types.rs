use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KuCoinResponse<T> {
    pub code: String,
    pub data: T,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountInfo {
    pub account_equity: f64,
    pub unrealised_pnl: f64,
    pub margin_balance: f64,
    pub position_margin: f64,
    pub order_margin: f64,
    pub frozen_funds: f64,
    pub available_balance: f64,
    pub currency: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Position {
    pub symbol: String,
    pub current_qty: f64,
    pub current_cost: f64,
    pub unrealised_pnl: f64,
    pub unrealised_pnl_pcnt: f64,
    pub mark_price: f64,
    pub liquidation_price: f64,
    pub leverage: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ticker {
    pub symbol: String,
    pub sequence: i64,
    pub side: String,
    pub price: String,
    pub size: i64,
    pub trade_id: String,
    pub best_bid_size: i64,
    pub best_bid_price: String,
    pub best_ask_size: i64,
    pub best_ask_price: String,
    pub ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketData {
    pub symbol: String,
    pub price: f64,
    pub volume: f64,
    pub timestamp: i64,
    pub bid: f64,
    pub ask: f64,
}

