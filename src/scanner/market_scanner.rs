use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use chrono::{DateTime, Utc};

use crate::api::KuCoinClient;
use crate::monitoring::TokenRegistry; // ← Using token monitor!

#[derive(Debug, Clone)]
pub struct MarketSnapshot {
    pub symbol: String,
    pub price: f64,
    pub volume_24h: f64,
    pub price_change_24h: f64,
    pub volatility: f64,
    pub is_new_listing: bool, // ← From token monitor!
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct ScanResult {
    pub symbol: String,
    pub score: f64,
    pub signals: Vec<String>,
    pub recommended_leverage: u32,
    pub is_new_listing: bool, // ← NEW listings get extra attention!
}

pub struct MarketScanner {
    client: Arc<KuCoinClient>,
    token_registry: Arc<TokenRegistry>, // ← Token monitor!
    snapshots: Arc<RwLock<HashMap<String, MarketSnapshot>>>,
}

impl MarketScanner {
    pub fn new(client: Arc<KuCoinClient>, token_registry: Arc<TokenRegistry>) -> Self {
        Self {
            client,
            token_registry,
            snapshots: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn start(&self) -> Result<()> {
        tracing::info!("🔍 Market scanner starting with token monitor integration...");
        
        let client = self.client.clone();
        let snapshots = self.snapshots.clone();
        let token_registry = self.token_registry.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(
                tokio::time::Duration::from_secs(10)
            );
            
            loop {
                interval.tick().await;
                
                // Get all ACTIVE tokens from monitoring system
                match token_registry.get_all_active_symbols().await {
                    Ok(symbols) => {
                        tracing::debug!("📊 Scanning {} active symbols", symbols.len());
                        
                        for symbol in symbols {
                            match client.get_ticker(&symbol).await {
                                Ok(ticker) => {
                                    let price = ticker.price.parse::<f64>().unwrap_or(0.0);
                                    
                                    // Check if this is a NEW listing (< 24h)
                                    let is_new = token_registry.is_new_listing(&symbol).await;
                                    
                                    let snapshot = MarketSnapshot {
                                        symbol: symbol.clone(),
                                        price,
                                        volume_24h: ticker.size as f64,
                                        price_change_24h: 0.0,
                                        volatility: 0.02,
                                        is_new_listing: is_new,
                                        timestamp: Utc::now(),
                                    };
                                    
                                    snapshots.write().await.insert(symbol.clone(), snapshot);
                                    
                                    // Special logging for NEW listings
                                    if is_new {
                                        tracing::debug!(
                                            "🆕 Tracking NEW listing: {} @ ${:.2}",
                                            symbol, price
                                        );
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!("Failed to get ticker for {}: {}", symbol, e);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("❌ Failed to get active symbols: {}", e);
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
            let mut score = snapshot.volatility * 10.0;
            let mut signals = vec![];
            
            // Boost score for NEW listings (priority!)
            if snapshot.is_new_listing {
                score += 0.3; // 30% boost for new listings
                signals.push("🆕 NEW LISTING".to_string());
            }
            
            if snapshot.volume_24h > 100000.0 {
                score += 0.2;
                signals.push("High volume".to_string());
            }
            
            if snapshot.volatility > 0.03 {
                signals.push("High volatility".to_string());
            }
            
            if score > 0.1 {
                // Lower leverage for NEW listings (safer)
                let recommended_leverage = if snapshot.is_new_listing {
                    3 // Conservative for new listings
                } else {
                    5 // Standard for established tokens
                };
                
                opportunities.push(ScanResult {
                    symbol: symbol.clone(),
                    score,
                    signals,
                    recommended_leverage,
                    is_new_listing: snapshot.is_new_listing,
                });
            }
        }
        
        // Sort by score, NEW listings naturally rank higher
        opportunities.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        opportunities.truncate(limit);
        
        Ok(opportunities)
    }

    pub async fn get_snapshot(&self, symbol: &str) -> Option<MarketSnapshot> {
        self.snapshots.read().await.get(symbol).cloned()
    }

    pub async fn get_new_listings_only(&self) -> Result<Vec<ScanResult>> {
        let all_opps = self.get_top_opportunities(100).await?;
        Ok(all_opps.into_iter()
            .filter(|opp| opp.is_new_listing)
            .collect())
    }
}
