pub mod data_aggregator;
pub mod data_quality;
pub mod pre_trade_validator;
pub mod market_intelligence;
pub mod event_bus;

pub use data_aggregator::DataAggregator;
pub use data_quality::{DataQualityManager, QualityCheck, QualityLevel};
pub use pre_trade_validator::{PreTradeValidator, ValidationResult, ValidationLayer};
pub use market_intelligence::{MarketIntelligence, MarketSignal, SignalType};
pub use event_bus::{EventBus, TradingEvent};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Unified market data structure combining all sources
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedMarketData {
    pub symbol: String,
    pub timestamp: DateTime<Utc>,
    
    // Price data
    pub price: f64,
    pub mark_price: f64,
    pub index_price: f64,
    
    // Volume & liquidity
    pub volume_24h: f64,
    pub volume_1h: f64,
    pub liquidity_score: f64,
    
    // Order book
    pub best_bid: f64,
    pub best_ask: f64,
    pub bid_volume: f64,
    pub ask_volume: f64,
    pub order_book_imbalance: f64,
    
    // Funding & fees
    pub funding_rate: f64,
    pub next_funding_time: DateTime<Utc>,
    
    // Metadata
    pub is_new_listing: bool,
    pub is_delisted: bool,
    pub data_freshness_ms: u64,
    pub source_count: u32,
    
    // Quality metrics
    pub data_quality_score: f64,
    pub completeness: f64,
}

impl UnifiedMarketData {
    pub fn is_valid(&self) -> bool {
        self.data_quality_score > 0.95 && 
        self.completeness > 0.99 &&
        self.data_freshness_ms < 5000 &&
        !self.is_delisted
    }
    
    pub fn spread_bps(&self) -> f64 {
        if self.price == 0.0 {
            return 9999.0;
        }
        ((self.best_ask - self.best_bid) / self.price) * 10000.0
    }
    
    pub fn liquidity_adequate(&self) -> bool {
        self.liquidity_score > 0.5 && 
        self.bid_volume + self.ask_volume > 10000.0
    }
}

impl Default for UnifiedMarketData {
    fn default() -> Self {
        Self {
            symbol: String::new(),
            timestamp: Utc::now(),
            price: 0.0,
            mark_price: 0.0,
            index_price: 0.0,
            volume_24h: 0.0,
            volume_1h: 0.0,
            liquidity_score: 0.0,
            best_bid: 0.0,
            best_ask: 0.0,
            bid_volume: 0.0,
            ask_volume: 0.0,
            order_book_imbalance: 0.0,
            funding_rate: 0.0,
            next_funding_time: Utc::now(),
            is_new_listing: false,
            is_delisted: false,
            data_freshness_ms: 0,
            source_count: 0,
            data_quality_score: 0.0,
            completeness: 0.0,
        }
    }
}

/// Pre-trade validation context
#[derive(Debug, Clone)]
pub struct TradeContext {
    pub market_data: UnifiedMarketData,
    pub account_balance: f64,
    pub open_positions: Vec<String>,
    pub daily_pnl: f64,
    pub confidence_score: f64,
}

