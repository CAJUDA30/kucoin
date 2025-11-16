use tokio::sync::broadcast;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TradingEvent {
    NewListingDetected {
        symbol: String,
        timestamp: DateTime<Utc>,
    },
    DelistingDetected {
        symbol: String,
        timestamp: DateTime<Utc>,
    },
    DataQualityIssue {
        symbol: String,
        severity: String,
        message: String,
    },
    RiskLimitHit {
        limit_type: String,
        current_value: f64,
        limit_value: f64,
    },
    HighConfidenceSignal {
        symbol: String,
        signal_type: String,
        confidence: f64,
    },
    OrderPlaced {
        order_id: String,
        symbol: String,
        side: String,
        price: f64,
    },
    OrderFilled {
        order_id: String,
        symbol: String,
        filled_price: f64,
    },
    StopLossTriggered {
        symbol: String,
        trigger_price: f64,
    },
    EmergencyStop {
        reason: String,
        timestamp: DateTime<Utc>,
    },
}

pub struct EventBus {
    sender: broadcast::Sender<TradingEvent>,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    pub fn publish(&self, event: TradingEvent) {
        match self.sender.send(event.clone()) {
            Ok(receivers) => {
                tracing::debug!("📡 Event published to {} receivers: {:?}", receivers, event);
            }
            Err(e) => {
                tracing::warn!("Failed to publish event: {}", e);
            }
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<TradingEvent> {
        self.sender.subscribe()
    }
}

