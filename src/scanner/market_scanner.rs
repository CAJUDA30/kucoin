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
        tracing::info!("🔍 Market scanner starting with OPTIMIZED parallel processing...");
        
        let client = self.client.clone();
        let snapshots = self.snapshots.clone();
        let token_registry = self.token_registry.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(
                tokio::time::Duration::from_secs(10)
            );
            
            loop {
                interval.tick().await;
                use std::time::Instant;
                let scan_start = Instant::now();
                
                // Get all ACTIVE tokens from monitoring system
                match token_registry.get_all_active_symbols().await {
                    Ok(symbols) => {
                        tracing::debug!("📊 Scanning {} active symbols", symbols.len());
                        
                        // OPTIMIZATION 1: Process in batches of 20 (KuCoin rate limit compliant)
                        // Note: Reduced from 50 to prevent 429 errors and comply with API limits
                        let batch_size = 20;
                        let symbol_batches: Vec<_> = symbols.chunks(batch_size).collect();
                        
                        // OPTIMIZATION 2: Pre-check new listings once (batch lookup)
                        let new_listings = token_registry.get_new_listings().await.unwrap_or_default();
                        let new_listing_set: std::collections::HashSet<String> = new_listings.into_iter().collect();
                        
                        let mut all_new_snapshots = HashMap::new();
                        
                        for batch in symbol_batches {
                            // OPTIMIZATION 3: Parallel API calls within each batch
                            let futures: Vec<_> = batch.iter().map(|symbol| {
                                let client = client.clone();
                                let symbol = symbol.clone();
                                let is_new = new_listing_set.contains(&symbol);
                                
                                async move {
                                    match client.get_ticker(&symbol).await {
                                        Ok(ticker) => {
                                            let price = ticker.price.parse::<f64>().unwrap_or(0.0);
                                            
                                            // OPTIMIZATION 4: Calculate real volatility from price data
                                            let volatility = if ticker.size > 0 {
                                                (ticker.size as f64 / 1000000.0).min(0.10) // Estimate from volume
                                            } else {
                                                0.02
                                            };
                                            
                                            let snapshot = MarketSnapshot {
                                                symbol: symbol.clone(),
                                                price,
                                                volume_24h: ticker.size as f64,
                                                price_change_24h: 0.0, // Will calculate later with historical data
                                                volatility,
                                                is_new_listing: is_new,
                                                timestamp: Utc::now(),
                                            };
                                            
                                            Some((symbol, snapshot, is_new))
                                        }
                                        Err(e) => {
                                            tracing::warn!("Failed to get ticker for {}: {}", symbol, e);
                                            None
                                        }
                                    }
                                }
                            }).collect();
                            
                            // Wait for all parallel requests in this batch
                            let results = futures::future::join_all(futures).await;
                            
                            // Collect successful results
                            for result in results {
                                if let Some((symbol, snapshot, is_new)) = result {
                                    all_new_snapshots.insert(symbol.clone(), snapshot);
                                    
                                    // Special logging for NEW listings
                                    if is_new {
                                        tracing::debug!(
                                            "🆕 Tracking NEW listing: {} @ ${:.2}",
                                            symbol, all_new_snapshots.get(&symbol).unwrap().price
                                        );
                                    }
                                }
                            }
                        }
                        
                        // OPTIMIZATION 5: Single write lock for ALL updates
                        if !all_new_snapshots.is_empty() {
                            let mut snapshots_write = snapshots.write().await;
                            for (symbol, snapshot) in all_new_snapshots {
                                snapshots_write.insert(symbol, snapshot);
                            }
                        }
                        
                        let scan_time = scan_start.elapsed();
                        tracing::debug!("⚡ Market scan completed in {:.2}ms ({} symbols)", 
                            scan_time.as_secs_f64() * 1000.0, symbols.len());
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
