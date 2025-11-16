use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, info, warn};

use super::websocket_manager::StreamMessage;
use super::metrics::StreamMetrics;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UpdateType {
    Ticker,
    OrderBook,
    Trade,
    Kline,
    MarkPrice,
    FundingRate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamUpdate {
    pub symbol: String,
    pub update_type: UpdateType,
    pub timestamp: i64,
    pub latency_ms: u128,
    pub data: serde_json::Value,
}

impl StreamUpdate {
    pub fn from_message(msg: &StreamMessage, update_type: UpdateType) -> Self {
        Self {
            symbol: msg.symbol.clone(),
            update_type,
            timestamp: msg.timestamp,
            latency_ms: msg.latency_ms(),
            data: msg.data.clone(),
        }
    }
}

use crate::api::KuCoinClient;

pub struct DataFeed {
    client: Arc<KuCoinClient>,
    update_tx: mpsc::UnboundedSender<StreamUpdate>,
    stream_metrics: Arc<RwLock<HashMap<String, StreamMetrics>>>,
    cache: Arc<RwLock<HashMap<String, StreamUpdate>>>,
}

impl DataFeed {
    pub fn new(client: Arc<KuCoinClient>) -> Self {
        let (update_tx, _update_rx) = mpsc::unbounded_channel();
        
        Self {
            client,
            update_tx,
            stream_metrics: Arc::new(RwLock::new(HashMap::new())),
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn start(&self, mut message_rx: mpsc::UnboundedReceiver<StreamMessage>) -> Result<()> {
        info!("🚀 Starting data feed processor...");
        
        let update_tx = self.update_tx.clone();
        let metrics = self.stream_metrics.clone();
        let cache = self.cache.clone();
        
        tokio::spawn(async move {
            while let Some(msg) = message_rx.recv().await {
                let latency = msg.latency_ms();
                
                // Process message based on type
                let update_type = match msg.data_type.as_str() {
                    "ticker" => UpdateType::Ticker,
                    "level2" => UpdateType::OrderBook,
                    "match" => UpdateType::Trade,
                    "candle" => UpdateType::Kline,
                    "markPrice" => UpdateType::MarkPrice,
                    "fundingRate" => UpdateType::FundingRate,
                    _ => {
                        debug!("Unknown message type: {}", msg.data_type);
                        continue;
                    }
                };

                let update = StreamUpdate::from_message(&msg, update_type);
                
                // Update metrics
                {
                    let mut m = metrics.write().await;
                    let metric = m.entry(msg.symbol.clone())
                        .or_insert_with(|| StreamMetrics::new(msg.symbol.clone()));
                    metric.update(latency as u64);
                }

                // Update cache for delta updates
                {
                    let mut c = cache.write().await;
                    c.insert(msg.symbol.clone(), update.clone());
                }

                // Forward update
                if update_tx.send(update).is_err() {
                    warn!("Failed to forward update, receiver dropped");
                    break;
                }
            }
        });

        Ok(())
    }

    pub async fn subscribe_ticker(&self, _symbols: Vec<String>) -> Result<()> {
        info!("📈 Ticker subscription (WebSocket integration pending)");
        Ok(())
    }

    pub async fn subscribe_orderbook(&self, _symbols: Vec<String>, depth: u32) -> Result<()> {
        info!("📊 OrderBook subscription depth:{} (WebSocket integration pending)", depth);
        Ok(())
    }

    pub async fn subscribe_trades(&self, _symbols: Vec<String>) -> Result<()> {
        info!("💹 Trade feed subscription (WebSocket integration pending)");
        Ok(())
    }

    pub async fn subscribe_mark_price(&self, _symbols: Vec<String>) -> Result<()> {
        info!("🎯 Mark price subscription (WebSocket integration pending)");
        Ok(())
    }

    pub async fn get_cached_update(&self, symbol: &str) -> Option<StreamUpdate> {
        self.cache.read().await.get(symbol).cloned()
    }

    pub async fn get_stream_metrics(&self, symbol: &str) -> Option<StreamMetrics> {
        self.stream_metrics.read().await.get(symbol).cloned()
    }

    pub async fn get_all_metrics(&self) -> HashMap<String, StreamMetrics> {
        self.stream_metrics.read().await.clone()
    }

    pub async fn check_stale_streams(&self, timeout_secs: u64) -> Vec<String> {
        let metrics = self.stream_metrics.read().await;
        metrics.iter()
            .filter(|(_, m)| m.is_stale(timeout_secs))
            .map(|(symbol, _)| symbol.clone())
            .collect()
    }

    pub async fn print_metrics_report(&self) {
        let metrics = self.get_all_metrics().await;
        
        info!("
╔══════════════════════════════════════════════════════════════════════╗
║                  DATA FEED METRICS REPORT                           ║
╚══════════════════════════════════════════════════════════════════════╝

📊 Active Streams: {}

Top 10 by Update Count:
", metrics.len());

        let mut sorted: Vec<_> = metrics.iter().collect();
        sorted.sort_by(|a, b| b.1.update_count.cmp(&a.1.update_count));

        for (i, (symbol, metric)) in sorted.iter().take(10).enumerate() {
            info!("  {}. {} - {} updates, {:.2}ms avg latency",
                i + 1, symbol, metric.update_count, metric.average_latency_ms);
        }

        let stale = self.check_stale_streams(60).await;
        if !stale.is_empty() {
            warn!("\n⚠️  Stale Streams (no updates in 60s): {}", stale.len());
            for symbol in stale.iter().take(5) {
                warn!("  • {}", symbol);
            }
        }

        info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_stream_update_creation() {
        let msg = StreamMessage {
            symbol: "XBTUSDTM".to_string(),
            data_type: "ticker".to_string(),
            timestamp: 1234567890,
            data: serde_json::json!({"price": 50000}),
            receive_time: Instant::now(),
        };

        let update = StreamUpdate::from_message(&msg, UpdateType::Ticker);
        assert_eq!(update.symbol, "XBTUSDTM");
    }
}

