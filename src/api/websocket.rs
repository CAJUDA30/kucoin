use anyhow::Result;
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};

use super::types::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsMessage {
    pub r#type: String,
    pub topic: String,
    pub subject: String,
    pub data: serde_json::Value,
}

pub struct WebSocketManager {
    sender: mpsc::UnboundedSender<MarketData>,
}

impl WebSocketManager {
    pub fn new() -> (Self, mpsc::UnboundedReceiver<MarketData>) {
        let (sender, receiver) = mpsc::unbounded_channel();
        (Self { sender }, receiver)
    }

    pub async fn connect(&self, url: &str, symbol: &str) -> Result<()> {
        tracing::info!("Connecting to WebSocket: {}", url);

        let (ws_stream, _) = connect_async(url).await?;
        let (mut write, mut read) = ws_stream.split();

        // Subscribe to ticker
        let subscribe_msg = serde_json::json!({
            "id": uuid::Uuid::new_v4().to_string(),
            "type": "subscribe",
            "topic": format!("/contractMarket/ticker:{}", symbol),
            "privateChannel": false,
            "response": true
        });

        write.send(Message::Text(subscribe_msg.to_string())).await?;
        tracing::info!("Subscribed to ticker for {}", symbol);

        let sender = self.sender.clone();

        tokio::spawn(async move {
            while let Some(msg) = read.next().await {
                match msg {
                    Ok(Message::Text(text)) => {
                        if let Ok(ws_msg) = serde_json::from_str::<WsMessage>(&text) {
                            if ws_msg.r#type == "message" {
                                if let Ok(market_data) = Self::parse_market_data(&ws_msg) {
                                    let _ = sender.send(market_data);
                                }
                            } else if ws_msg.r#type == "ack" {
                                tracing::info!("WebSocket subscription acknowledged");
                            }
                        }
                    }
                    Ok(Message::Ping(data)) => {
                        let _ = write.send(Message::Pong(data)).await;
                    }
                    Err(e) => {
                        tracing::error!("WebSocket error: {}", e);
                        break;
                    }
                    _ => {}
                }
            }
            tracing::warn!("WebSocket connection closed");
        });

        Ok(())
    }

    fn parse_market_data(msg: &WsMessage) -> Result<MarketData> {
        let data = &msg.data;
        Ok(MarketData {
            symbol: data["symbol"].as_str().unwrap_or("").to_string(),
            price: data["price"].as_str().unwrap_or("0").parse()?,
            volume: data["size"].as_i64().unwrap_or(0) as f64,
            timestamp: data["ts"].as_i64().unwrap_or(0),
            bid: data["bestBidPrice"].as_str().unwrap_or("0").parse()?,
            ask: data["bestAskPrice"].as_str().unwrap_or("0").parse()?,
        })
    }
}
