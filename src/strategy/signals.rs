use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum SignalType {
    Long,
    Short,
    Hold,
}

impl fmt::Display for SignalType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SignalType::Long => write!(f, "LONG"),
            SignalType::Short => write!(f, "SHORT"),
            SignalType::Hold => write!(f, "HOLD"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeSignal {
    pub symbol: String,
    pub signal_type: SignalType,
    pub confidence: f64,
    pub entry_price: f64,
    pub stop_loss: f64,
    pub take_profit: f64,
    pub recommended_size: f64,
    pub recommended_leverage: u32,
    pub reason: String,
    pub timestamp: DateTime<Utc>,
}

impl TradeSignal {
    pub fn new(
        symbol: String,
        signal_type: SignalType,
        confidence: f64,
        entry_price: f64,
    ) -> Self {
        let risk_percent = 0.02;
        let reward_ratio = 2.0 + confidence;
        
        let (stop_loss, take_profit) = match signal_type {
            SignalType::Long => {
                (entry_price * (1.0 - risk_percent), entry_price * (1.0 + risk_percent * reward_ratio))
            }
            SignalType::Short => {
                (entry_price * (1.0 + risk_percent), entry_price * (1.0 - risk_percent * reward_ratio))
            }
            SignalType::Hold => (entry_price, entry_price),
        };
        
        let recommended_leverage = if confidence > 0.9 { 10 }
            else if confidence > 0.8 { 7 }
            else if confidence > 0.7 { 5 }
            else { 3 };
        
        let recommended_size = if confidence > 0.9 { 0.15 }
            else if confidence > 0.8 { 0.10 }
            else { 0.05 };
        
        Self {
            symbol,
            signal_type,
            confidence,
            entry_price,
            stop_loss,
            take_profit,
            recommended_size,
            recommended_leverage,
            reason: String::new(),
            timestamp: Utc::now(),
        }
    }

    pub fn with_reason(mut self, reason: String) -> Self {
        self.reason = reason;
        self
    }
}
