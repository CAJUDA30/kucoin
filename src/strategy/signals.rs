use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum SignalType {
    Long,
    Short,
    CloseLong,
    CloseShort,
    Hold,
}

impl fmt::Display for SignalType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SignalType::Long => write!(f, "LONG"),
            SignalType::Short => write!(f, "SHORT"),
            SignalType::CloseLong => write!(f, "CLOSE_LONG"),
            SignalType::CloseShort => write!(f, "CLOSE_SHORT"),
            SignalType::Hold => write!(f, "HOLD"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeSignal {
    pub symbol: String,
    pub signal_type: SignalType,
    pub confidence: f64, // 0.0 to 1.0
    pub entry_price: f64,
    pub stop_loss: f64,
    pub take_profit: f64,
    pub recommended_size: f64, // Position size as percentage
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
        let (stop_loss, take_profit) =
            Self::calculate_targets(signal_type, confidence, entry_price);

        let recommended_leverage = Self::calculate_leverage(confidence);
        let recommended_size = Self::calculate_position_size(confidence);

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

    fn calculate_targets(signal_type: SignalType, confidence: f64, entry: f64) -> (f64, f64) {
        let risk_percent = 0.02; // 2% risk
        let reward_ratio = 2.0 + confidence; // 2-3x reward

        match signal_type {
            SignalType::Long => {
                let stop_loss = entry * (1.0 - risk_percent);
                let take_profit = entry * (1.0 + (risk_percent * reward_ratio));
                (stop_loss, take_profit)
            }
            SignalType::Short => {
                let stop_loss = entry * (1.0 + risk_percent);
                let take_profit = entry * (1.0 - (risk_percent * reward_ratio));
                (stop_loss, take_profit)
            }
            _ => (entry, entry),
        }
    }

    fn calculate_leverage(confidence: f64) -> u32 {
        if confidence > 0.9 {
            10
        } else if confidence > 0.8 {
            7
        } else if confidence > 0.7 {
            5
        } else {
            3
        }
    }

    fn calculate_position_size(confidence: f64) -> f64 {
        if confidence > 0.9 {
            0.15 // 15%
        } else if confidence > 0.8 {
            0.10 // 10%
        } else if confidence > 0.7 {
            0.07 // 7%
        } else {
            0.05 // 5%
        }
    }

    pub fn with_reason(mut self, reason: String) -> Self {
        self.reason = reason;
        self
    }
}

