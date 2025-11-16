use anyhow::Result;
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::api::KuCoinClient;
use super::database::{TokenDatabase, TokenRecord, TokenMetadata};

pub struct TokenRegistry {
    client: Arc<KuCoinClient>,
    database: Arc<TokenDatabase>,
    cache: Arc<RwLock<HashMap<String, TokenRecord>>>,
    refresh_interval_secs: u64,
}

impl TokenRegistry {
    pub fn new(
        client: Arc<KuCoinClient>,
        database: Arc<TokenDatabase>,
        refresh_interval_secs: u64,
    ) -> Self {
        Self {
            client,
            database,
            cache: Arc::new(RwLock::new(HashMap::new())),
            refresh_interval_secs,
        }
    }

    pub async fn start(&self) -> Result<()> {
        tracing::info!("🗂️  Token Registry starting...");

        // Initial sync
        self.sync_all_tokens().await?;

        // Start background sync task
        let client = self.client.clone();
        let database = self.database.clone();
        let cache = self.cache.clone();
        let interval_secs = self.refresh_interval_secs;

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(
                tokio::time::Duration::from_secs(interval_secs)
            );

            loop {
                interval.tick().await;

                match Self::fetch_and_update(&client, &database, &cache).await {
                    Ok(count) => {
                        tracing::debug!("✅ Token registry synced: {} tokens", count);
                    }
                    Err(e) => {
                        tracing::error!("❌ Token registry sync failed: {}", e);
                    }
                }
            }
        });

        // Start daily cleanup task for NEW badge updates
        let database_cleanup = self.database.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(
                tokio::time::Duration::from_secs(3600) // Check every hour
            );

            loop {
                interval.tick().await;

                match database_cleanup.update_new_status().await {
                    Ok(count) if count > 0 => {
                        tracing::info!("🔄 Removed NEW badge from {} tokens (older than 24h)", count);
                    }
                    Ok(_) => {}, // No updates needed
                    Err(e) => {
                        tracing::error!("❌ Failed to update NEW status: {}", e);
                    }
                }
            }
        });

        tracing::info!("✅ Token Registry started (refresh every {}s)", self.refresh_interval_secs);

        Ok(())
    }

    async fn fetch_and_update(
        client: &Arc<KuCoinClient>,
        database: &Arc<TokenDatabase>,
        cache: &Arc<RwLock<HashMap<String, TokenRecord>>>,
    ) -> Result<usize> {
        use std::time::Instant;
        let start = Instant::now();

        // Fetch all symbols from API
        let symbols = match client.get_all_symbols().await {
            Ok(syms) => syms,
            Err(e) => {
                tracing::warn!("Failed to fetch symbols from API: {}", e);
                return Err(e);
            }
        };

        let api_fetch_time = start.elapsed();
        tracing::debug!("⚡ API fetch completed in {:.2}ms", api_fetch_time.as_secs_f64() * 1000.0);

        // OPTIMIZATION 1: Batch load existing tokens from database (single query)
        let db_start = Instant::now();
        let existing_tokens = database.get_all_tokens().await?;
        let existing_map: HashMap<String, TokenRecord> = existing_tokens
            .into_iter()
            .map(|t| (t.symbol.clone(), t))
            .collect();
        let db_load_time = db_start.elapsed();
        tracing::debug!("⚡ Database load completed in {:.2}ms ({} tokens)", 
            db_load_time.as_secs_f64() * 1000.0, existing_map.len());

        let now = Utc::now();
        let mut new_symbols = Vec::new();
        let mut records_to_upsert = Vec::new();
        let mut history_events = Vec::new();

        // OPTIMIZATION 2: Build all records in memory first (no I/O in loop)
        let process_start = Instant::now();
        for symbol in &symbols {
            let existing = existing_map.get(&symbol.symbol);
            let is_new = existing.is_none();

            let token_record = TokenRecord {
                symbol: symbol.symbol.clone(),
                base_currency: symbol.base_currency.clone(),
                quote_currency: symbol.quote_currency.clone(),
                first_seen: existing.map(|t| t.first_seen).unwrap_or(now),
                last_seen: now,
                status: if symbol.status == "Open" {
                    "active".to_string()
                } else {
                    "suspended".to_string()
                },
                is_new,
                delisted_at: None,
                lot_size: Some(symbol.lot_size as f64),
                tick_size: Some(symbol.tick_size),
                multiplier: Some(symbol.multiplier),
                max_leverage: Some((1.0 / symbol.initial_margin) as i32),
                funding_rate_symbol: Some(symbol.symbol.clone()),
                metadata: serde_json::to_string(&TokenMetadata {
                    display_name: Some(symbol.root_symbol.clone()),
                    description: Some(format!("{} Futures Contract", symbol.root_symbol)),
                    listing_price: None,
                    initial_volume: None,
                    market_cap: None,
                }).unwrap_or_default(),
            };

            records_to_upsert.push(token_record.clone());

            if is_new {
                new_symbols.push(token_record.clone());
                history_events.push((symbol.symbol.clone(), "new_listing".to_string()));
            }
        }
        let process_time = process_start.elapsed();
        tracing::debug!("⚡ Record processing completed in {:.2}ms", process_time.as_secs_f64() * 1000.0);

        // OPTIMIZATION 3: Batch database operations
        let batch_start = Instant::now();
        database.batch_upsert_tokens(&records_to_upsert).await?;
        let batch_time = batch_start.elapsed();
        tracing::debug!("⚡ Batch upsert completed in {:.2}ms ({} records)", 
            batch_time.as_secs_f64() * 1000.0, records_to_upsert.len());

        // OPTIMIZATION 4: Batch history events
        if !history_events.is_empty() {
            let history_start = Instant::now();
            database.batch_add_history_events(&history_events).await?;
            let history_time = history_start.elapsed();
            tracing::debug!("⚡ History events completed in {:.2}ms ({} events)", 
                history_time.as_secs_f64() * 1000.0, history_events.len());
        }

        // OPTIMIZATION 5: Single cache update with write lock
        let cache_start = Instant::now();
        {
            let mut cache_write = cache.write().await;
            for record in records_to_upsert.iter() {
                cache_write.insert(record.symbol.clone(), record.clone());
            }
        }
        let cache_time = cache_start.elapsed();
        tracing::debug!("⚡ Cache update completed in {:.2}ms", cache_time.as_secs_f64() * 1000.0);

        // Log new listings (only show first 10 for performance)
        if !new_symbols.is_empty() {
            tracing::info!("🆕 NEW LISTINGS DETECTED: {} tokens", new_symbols.len());
            for token in new_symbols.iter().take(10) {
                tracing::info!(
                    "  {} {} ({}/{}) - {}",
                    token.get_badge(),
                    token.symbol,
                    token.base_currency,
                    token.quote_currency,
                    token.get_colored_status()
                );
            }
            if new_symbols.len() > 10 {
                tracing::info!("  ... and {} more", new_symbols.len() - 10);
            }
        }

        // OPTIMIZATION 6: Efficient delisting check using HashSet
        let delist_start = Instant::now();
        let api_symbols: std::collections::HashSet<String> = symbols
            .iter()
            .map(|s| s.symbol.clone())
            .collect();

        let cached_symbols: Vec<String> = cache.read().await.keys().cloned().collect();
        let mut delisted = Vec::new();

        for cached_symbol in cached_symbols {
            if !api_symbols.contains(&cached_symbol) {
                delisted.push(cached_symbol);
            }
        }

        if !delisted.is_empty() {
            tracing::warn!("⚠️  {} tokens possibly delisted", delisted.len());
            database.batch_mark_as_delisted(&delisted).await?;
            let mut cache_write = cache.write().await;
            for symbol in &delisted {
                cache_write.remove(symbol);
            }
        }
        let delist_time = delist_start.elapsed();
        tracing::debug!("⚡ Delisting check completed in {:.2}ms", delist_time.as_secs_f64() * 1000.0);

        let total_time = start.elapsed();
        tracing::info!("⚡ PERFORMANCE: Total sync time: {:.2}ms (API: {:.2}ms, DB: {:.2}ms, Process: {:.2}ms, Batch: {:.2}ms, Cache: {:.2}ms)", 
            total_time.as_secs_f64() * 1000.0,
            api_fetch_time.as_secs_f64() * 1000.0,
            db_load_time.as_secs_f64() * 1000.0,
            process_time.as_secs_f64() * 1000.0,
            batch_time.as_secs_f64() * 1000.0,
            cache_time.as_secs_f64() * 1000.0
        );

        Ok(records_to_upsert.len())
    }

    async fn sync_all_tokens(&self) -> Result<()> {
        tracing::info!("🔄 Performing initial token sync...");

        let count = Self::fetch_and_update(&self.client, &self.database, &self.cache).await?;

        tracing::info!("✅ Initial sync complete: {} tokens loaded", count);

        Ok(())
    }

    pub async fn get_token(&self, symbol: &str) -> Option<TokenRecord> {
        self.cache.read().await.get(symbol).cloned()
    }

    pub async fn get_all_tokens(&self) -> Vec<TokenRecord> {
        self.cache.read().await.values().cloned().collect()
    }

    pub async fn get_active_tokens(&self) -> Vec<TokenRecord> {
        self.cache
            .read()
            .await
            .values()
            .filter(|t| t.status == "active")
            .cloned()
            .collect()
    }

    pub async fn get_token_count(&self) -> usize {
        self.cache.read().await.len()
    }

    // === HELPER METHODS FOR TRADING INTEGRATION ===

    pub async fn get_all_active_symbols(&self) -> Result<Vec<String>> {
        let cache = self.cache.read().await;
        Ok(cache
            .values()
            .filter(|t| t.status == "active")
            .map(|t| t.symbol.clone())
            .collect())
    }

    pub async fn is_new_listing(&self, symbol: &str) -> bool {
        if let Some(token) = self.cache.read().await.get(symbol) {
            token.is_still_new()
        } else {
            false
        }
    }

    pub async fn get_new_listings(&self) -> Result<Vec<String>> {
        let cache = self.cache.read().await;
        Ok(cache
            .values()
            .filter(|t| t.status == "active" && t.is_still_new())
            .map(|t| t.symbol.clone())
            .collect())
    }

    pub async fn is_delisted(&self, symbol: &str) -> bool {
        if let Some(token) = self.cache.read().await.get(symbol) {
            token.status == "delisted"
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_token_registry() {
        // Test will be added when integrated
    }
}

