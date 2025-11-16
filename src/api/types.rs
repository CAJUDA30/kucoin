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
    #[serde(rename = "unrealisedPNL")]
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

// === PHASE 4: NEW TYPES ===

// Symbol information
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Symbol {
    pub symbol: String,
    pub root_symbol: String,
    #[serde(rename = "type")]
    pub type_field: String,
    pub base_currency: String,
    pub quote_currency: String,
    pub settle_currency: String,
    pub max_order_qty: i64,
    pub max_price: f64,
    pub lot_size: i64,
    pub tick_size: f64,
    pub multiplier: f64,
    pub initial_margin: f64,
    pub maintain_margin: f64,
    pub max_risk_limit: i64,
    pub maker_fee_rate: f64,
    pub taker_fee_rate: f64,
    pub status: String,
}

// Candlestick/Kline data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Kline {
    pub time: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

// Mark price
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarkPrice {
    pub symbol: String,
    pub granularity: i64,
    pub time_point: i64,
    pub value: f64,
    pub index_price: f64,
}

// Funding rate
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FundingRate {
    pub symbol: String,
    pub granularity: i64,
    pub time_point: i64,
    pub value: f64,
    pub predict_value: f64,
}

// Order request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderRequest {
    pub client_oid: String,
    pub side: String, // "buy" or "sell"
    pub symbol: String,
    #[serde(rename = "type")]
    pub order_type: String, // "limit" or "market"
    pub leverage: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_in_force: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reduce_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_price: Option<String>,
}

// Order response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderResponse {
    pub order_id: String,
}

// Order details
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Order {
    pub id: String,
    pub symbol: String,
    #[serde(rename = "type")]
    pub order_type: String,
    pub side: String,
    pub price: String,
    pub size: i64,
    pub filled_size: i64,
    pub value: String,
    pub deal_value: String,
    pub status: String,
    pub time_in_force: String,
    pub post_only: bool,
    pub leverage: String,
    pub client_oid: String,
    pub reduce_only: bool,
    pub created_at: i64,
}

// Max open size
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaxOpenSize {
    pub symbol: String,
    pub max_buy_open_size: i64,
    pub max_sell_open_size: i64,
}
