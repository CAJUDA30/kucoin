use anyhow::{Result, Context};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, RwLock, Semaphore};
use tokio::time;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, error, info, warn};

use super::metrics::PerformanceMetrics;

#[derive(Debug, Clone)]
pub struct ConnectionConfig {
    pub url: String,
    pub max_reconnect_attempts: u32,
    pub reconnect_delay_ms: u64,
    pub ping_interval_secs: u64,
    pub message_buffer_size: usize,
    pub connection_timeout_secs: u64,
    pub max_concurrent_connections: usize,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            url: "wss://ws-api-futures.kucoin.com".to_string(),
            max_reconnect_attempts: 10,
            reconnect_delay_ms: 1000,
            ping_interval_secs: 30,
            message_buffer_size: 10000,
            connection_timeout_secs: 10,
            max_concurrent_connections: 100,
        }
    }
}

#[derive(Debug, Clone)]
pub struct StreamMessage {
    pub symbol: String,
    pub data_type: String,
    pub timestamp: i64,
    pub data: serde_json::Value,
    pub receive_time: Instant,
}

impl StreamMessage {
    pub fn latency_ms(&self) -> u128 {
        self.receive_time.elapsed().as_millis()
    }
}

pub struct WebSocketManager {
    config: ConnectionConfig,
    metrics: Arc<PerformanceMetrics>,
    active_connections: Arc<RwLock<HashMap<String, ConnectionState>>>,
    connection_semaphore: Arc<Semaphore>,
    message_tx: mpsc::UnboundedSender<StreamMessage>,
}

#[derive(Debug)]
struct ConnectionState {
    topic: String,
    connected: bool,
    last_message: Instant,
    reconnect_count: u32,
}

impl WebSocketManager {
    pub fn new(config: ConnectionConfig) -> Self {
        let (message_tx, _message_rx) = mpsc::unbounded_channel();
        let metrics = Arc::new(PerformanceMetrics::new());
        let max_conns = config.max_concurrent_connections;
        
        Self {
            config,
            metrics,
            active_connections: Arc::new(RwLock::new(HashMap::new())),
            connection_semaphore: Arc::new(Semaphore::new(max_conns)),
            message_tx,
        }
    }

    pub async fn subscribe(&self, topic: String, symbols: Vec<String>) -> Result<()> {
        info!("📡 Subscribing to topic: {} for {} symbols", topic, symbols.len());
        
        // Check connection limit
        let _permit = self.connection_semaphore.acquire().await?;
        
        // Register connection state
        {
            let mut connections = self.active_connections.write().await;
            connections.insert(topic.clone(), ConnectionState {
                topic: topic.clone(),
                connected: false,
                last_message: Instant::now(),
                reconnect_count: 0,
            });
        }

        // Spawn connection handler
        let config = self.config.clone();
        let metrics = self.metrics.clone();
        let message_tx = self.message_tx.clone();
        let connections = self.active_connections.clone();
        
        tokio::spawn(async move {
            Self::connection_handler(
                config,
                topic.clone(),
                symbols,
                metrics,
                message_tx,
                connections,
            ).await;
        });

        Ok(())
    }

    async fn connection_handler(
        config: ConnectionConfig,
        topic: String,
        symbols: Vec<String>,
        metrics: Arc<PerformanceMetrics>,
        message_tx: mpsc::UnboundedSender<StreamMessage>,
        connections: Arc<RwLock<HashMap<String, ConnectionState>>>,
    ) {
        let mut reconnect_count = 0;

        loop {
            match Self::establish_connection(&config, &topic, &symbols, &metrics, &message_tx).await {
                Ok(_) => {
                    info!("✅ Connection established for topic: {}", topic);
                    reconnect_count = 0;
                    
                    // Update connection state
                    {
                        let mut conns = connections.write().await;
                        if let Some(state) = conns.get_mut(&topic) {
                            state.connected = true;
                            state.last_message = Instant::now();
                        }
                    }
                }
                Err(e) => {
                    error!("❌ Connection failed for {}: {}", topic, e);
                    reconnect_count += 1;
                    
                    if reconnect_count >= config.max_reconnect_attempts {
                        error!("🛑 Max reconnect attempts reached for {}", topic);
                        break;
                    }
                    
                    // Update connection state
                    {
                        let mut conns = connections.write().await;
                        if let Some(state) = conns.get_mut(&topic) {
                            state.connected = false;
                            state.reconnect_count = reconnect_count;
                        }
                    }
                    
                    // Exponential backoff
                    let delay = config.reconnect_delay_ms * (2_u64.pow(reconnect_count.min(5)));
                    warn!("⏳ Reconnecting in {}ms (attempt {}/{})", 
                        delay, reconnect_count, config.max_reconnect_attempts);
                    time::sleep(Duration::from_millis(delay)).await;
                }
            }
        }
    }

    async fn establish_connection(
        config: &ConnectionConfig,
        topic: &str,
        symbols: &[String],
        metrics: &Arc<PerformanceMetrics>,
        message_tx: &mpsc::UnboundedSender<StreamMessage>,
    ) -> Result<()> {
        let connect_start = Instant::now();
        
        // Connect to WebSocket
        let (ws_stream, _) = tokio::time::timeout(
            Duration::from_secs(config.connection_timeout_secs),
            connect_async(&config.url)
        )
        .await
        .context("Connection timeout")?
        .context("Failed to connect")?;

        metrics.record_connection_latency(connect_start.elapsed());
        info!("🔗 WebSocket connected in {:?}", connect_start.elapsed());

        let (mut write, mut read) = ws_stream.split();

        // Send subscription message
        let subscribe_msg = Self::create_subscription_message(topic, symbols)?;
        write.send(Message::Text(subscribe_msg)).await?;
        debug!("📤 Subscription message sent for {}", topic);

        // Note: Ping/pong handled by underlying WebSocket implementation

        // Process incoming messages
        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    let receive_time = Instant::now();
                    metrics.increment_messages_received();
                    
                    // Parse and forward message
                    if let Ok(parsed) = Self::parse_message(&text, receive_time) {
                        let latency = parsed.latency_ms();
                        metrics.record_message_latency(Duration::from_millis(latency as u64));
                        
                        if latency > 100 {
                            warn!("⚠️  High latency detected: {}ms for {}", latency, parsed.symbol);
                        }
                        
                        if message_tx.send(parsed).is_err() {
                            error!("Failed to forward message");
                            break;
                        }
                    }
                }
                Ok(Message::Pong(_)) => {
                    debug!("🏓 Pong received");
                }
                Ok(Message::Close(_)) => {
                    warn!("🔌 WebSocket closed by server");
                    break;
                }
                Err(e) => {
                    error!("❌ WebSocket error: {}", e);
                    metrics.increment_errors();
                    break;
                }
                _ => {}
            }
        }

        Ok(())
    }

    fn create_subscription_message(topic: &str, symbols: &[String]) -> Result<String> {
        let msg = serde_json::json!({
            "type": "subscribe",
            "topic": topic,
            "symbols": symbols,
            "response": true
        });
        Ok(serde_json::to_string(&msg)?)
    }

    fn parse_message(text: &str, receive_time: Instant) -> Result<StreamMessage> {
        let value: serde_json::Value = serde_json::from_str(text)?;
        
        Ok(StreamMessage {
            symbol: value["symbol"].as_str().unwrap_or("UNKNOWN").to_string(),
            data_type: value["type"].as_str().unwrap_or("unknown").to_string(),
            timestamp: value["timestamp"].as_i64().unwrap_or(0),
            data: value,
            receive_time,
        })
    }

    pub async fn get_metrics(&self) -> HashMap<String, serde_json::Value> {
        self.metrics.get_snapshot().await
    }

    pub async fn get_connection_status(&self) -> HashMap<String, bool> {
        let connections = self.active_connections.read().await;
        connections.iter()
            .map(|(topic, state)| (topic.clone(), state.connected))
            .collect()
    }

    pub fn get_metrics_handle(&self) -> Arc<PerformanceMetrics> {
        self.metrics.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_connection_config_defaults() {
        let config = ConnectionConfig::default();
        assert_eq!(config.max_reconnect_attempts, 10);
        assert_eq!(config.ping_interval_secs, 30);
    }

    #[tokio::test]
    async fn test_websocket_manager_creation() {
        let config = ConnectionConfig::default();
        let manager = WebSocketManager::new(config);
        
        let status = manager.get_connection_status().await;
        assert!(status.is_empty());
    }
}

