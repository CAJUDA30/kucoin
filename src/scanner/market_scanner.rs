use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use chrono::{DateTime, Utc};

use crate::api::KuCoinClient;

#[derive(Debug, Clone)]
pub struct MarketSnapshot {
    pub symbol: String,
    pub price: f64,
    pub volume_24h: f64,
    pub price_change_24h: f64,
    pub volatility: f64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct ScanResult {
    pub symbol: String,
    pub score: f64,
    pub signals: Vec<String>,
    pub recommended_leverage: u32,
}

pub struct MarketScanner {
    client: Arc<KuCoinClient>,
    snapshots: Arc<RwLock<HashMap<String, MarketSnapshot>>>,
}

impl MarketScanner {
    pub fn new(client: Arc<KuCoinClient>) -> Self {
        Self {
            client,
            snapshots: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn start(&self) -> Result<()> {
        tracing::info!("🔍 Market scanner starting...");
        
        let client = self.client.clone();
        let snapshots = self.snapshots.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(
                tokio::time::Duration::from_secs(10)
            );
            
            loop {
                interval.tick().await;
                
                // Scan major symbols
                let symbols = vec![
                    "XBTUSDTM",
                    "ETHUSDTM",
                    "SOLUSDTM",
                ];
                
                for symbol in symbols {
                    match client.get_ticker(symbol).await {
                        Ok(ticker) => {
                            let price = ticker.price.parse::<f64>().unwrap_or(0.0);
                            let snapshot = MarketSnapshot {
                                symbol: symbol.to_string(),
                                price,
                                volume_24h: ticker.size as f64,
                                price_change_24h: 0.0,
                                volatility: 0.02,
                                timestamp: Utc::now(),
                            };
                            snapshots.write().await.insert(symbol.to_string(), snapshot);
                        }
                        Err(e) => {
                            tracing::warn!("Failed to get ticker for {}: {}", symbol, e);
                        }
                    }
                }
            }
        });
        
        Ok(())
    }

    pub async fn get_top_opportunities(&self, limit: usize) -> Result<Vec<ScanResult>> {
        let snapshots = self.snapshots.read().await;
        let mut opportunities = Vec::new();
        
        for (symbol, snapshot) in snapshots.iter() {
            let score = snapshot.volatility * 10.0;
            
            if score > 0.1 {
                opportunities.push(ScanResult {
                    symbol: symbol.clone(),
                    score,
                    signals: vec!["High volatility".to_string()],
                    recommended_leverage: 5,
                });
            }
        }
        
        opportunities.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        opportunities.truncate(limit);
        
        Ok(opportunities)
    }

    pub async fn get_snapshot(&self, symbol: &str) -> Option<MarketSnapshot> {
        self.snapshots.read().await.get(symbol).cloned()
    }
}
