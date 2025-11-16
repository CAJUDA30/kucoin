use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use chrono::Utc;

use crate::api::KuCoinClient;
use crate::streaming::WebSocketManager;
use crate::monitoring::TokenRegistry;
use super::UnifiedMarketData;

pub struct DataAggregator {
    client: Arc<KuCoinClient>,
    ws_manager: Arc<WebSocketManager>,
    token_registry: Arc<TokenRegistry>,
    unified_data: Arc<RwLock<HashMap<String, UnifiedMarketData>>>,
}

impl DataAggregator {
    pub fn new(
        client: Arc<KuCoinClient>,
        ws_manager: Arc<WebSocketManager>,
        token_registry: Arc<TokenRegistry>,
    ) -> Self {
        Self {
            client,
            ws_manager,
            token_registry,
            unified_data: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn start(&self) -> Result<()> {
        tracing::info!("🔄 Data Aggregator starting...");
        
        // Start ticker stream processing
        self.process_ticker_stream().await?;
        
        // Start periodic full sync
        self.start_periodic_sync().await?;
        
        tracing::info!("✅ Data Aggregator active - processing all streams");
        
        Ok(())
    }

    async fn process_ticker_stream(&self) -> Result<()> {
        let unified_data = self.unified_data.clone();
        let token_registry = self.token_registry.clone();
        let _ws_manager = self.ws_manager.clone();
        
        // Note: WebSocket stream processing is integrated via periodic sync
        // In production, this would be replaced with actual WebSocket message handling
        tokio::spawn(async move {
            tracing::debug!("📡 Ticker stream processor ready (using REST backup)");
            
            // This is a placeholder for WebSocket integration
            // The actual data comes from periodic_sync for now
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(3600)).await;
                tracing::debug!("Ticker stream processor alive");
            }
        });
        
        Ok(())
    }

    async fn start_periodic_sync(&self) -> Result<()> {
        let unified_data = self.unified_data.clone();
        let client = self.client.clone();
        let token_registry = self.token_registry.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(
                tokio::time::Duration::from_secs(10)
            );
            
            loop {
                interval.tick().await;
                
                // Update data freshness and sync fallback data
                let mut data = unified_data.write().await;
                
                // Get active symbols
                if let Ok(symbols) = token_registry.get_all_active_symbols().await {
                    for symbol in symbols.iter().take(10) {
                        // Update existing or create new entry
                        let entry = data.entry(symbol.clone()).or_insert_with(|| {
                            UnifiedMarketData {
                                symbol: symbol.clone(),
                                timestamp: Utc::now(),
                                ..Default::default()
                            }
                        });
                        
                        // Fetch fresh data if stale
                        let age_ms = (Utc::now() - entry.timestamp).num_milliseconds() as u64;
                        if age_ms > 5000 {
                            // Fetch from REST API as backup
                            if let Ok(ticker) = client.get_ticker(symbol).await {
                                if let Ok(price) = ticker.price.parse::<f64>() {
                                    entry.price = price;
                                    entry.volume_24h = ticker.size as f64;
                                    entry.timestamp = Utc::now();
                                    entry.data_freshness_ms = 0;
                                }
                            }
                        } else {
                            entry.data_freshness_ms = age_ms;
                        }
                        
                        // Update metadata
                        entry.is_new_listing = token_registry.is_new_listing(symbol).await;
                        entry.is_delisted = token_registry.is_delisted(symbol).await;
                        
                        // Recalculate quality score
                        entry.data_quality_score = Self::calculate_quality_score(entry);
                    }
                }
            }
        });
        
        Ok(())
    }

    fn extract_symbol(topic: &str) -> Option<String> {
        topic.split(':').nth(1).map(|s| s.to_string())
    }

    fn calculate_completeness(data: &UnifiedMarketData) -> f64 {
        let mut score = 0.0;
        let mut total = 0.0;
        
        // Critical fields (weight: 2.0)
        if data.price > 0.0 { score += 2.0; }
        total += 2.0;
        
        if data.best_bid > 0.0 { score += 2.0; }
        total += 2.0;
        
        if data.best_ask > 0.0 { score += 2.0; }
        total += 2.0;
        
        // Important fields (weight: 1.0)
        if data.volume_24h > 0.0 { score += 1.0; }
        total += 1.0;
        
        if data.mark_price > 0.0 { score += 1.0; }
        total += 1.0;
        
        score / total
    }

    fn calculate_quality_score(data: &UnifiedMarketData) -> f64 {
        let mut score = 1.0;
        
        // Penalize stale data
        if data.data_freshness_ms > 5000 {
            score *= 0.5;
        } else if data.data_freshness_ms > 1000 {
            score *= 0.9;
        }
        
        // Boost for multiple sources
        if data.source_count > 2 {
            score *= 1.1;
        }
        
        // Completeness factor
        score *= data.completeness;
        
        score.min(1.0)
    }

    pub async fn get_unified_data(&self, symbol: &str) -> Option<UnifiedMarketData> {
        self.unified_data.read().await.get(symbol).cloned()
    }

    pub async fn get_all_valid_data(&self) -> Vec<UnifiedMarketData> {
        self.unified_data
            .read()
            .await
            .values()
            .filter(|d| d.is_valid())
            .cloned()
            .collect()
    }
}

