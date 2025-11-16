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
        // Fetch all symbols from API
        let symbols = match client.get_all_symbols().await {
            Ok(syms) => syms,
            Err(e) => {
                tracing::warn!("Failed to fetch symbols from API: {}", e);
                return Err(e);
            }
        };

        let now = Utc::now();
        let mut updated_count = 0;
        let mut new_symbols = Vec::new();

        for symbol in &symbols {
            // Check if this is a new token
            let existing = database.get_token(&symbol.symbol).await?;
            let is_new = existing.is_none();

            let token_record = TokenRecord {
                symbol: symbol.symbol.clone(),
                base_currency: symbol.base_currency.clone(),
                quote_currency: symbol.quote_currency.clone(),
                first_seen: existing.as_ref().map(|t| t.first_seen).unwrap_or(now),
                last_seen: now,
                status: if symbol.status == "Open" {
                    "active".to_string()
                } else {
                    "suspended".to_string()
                },
                is_new: is_new, // Set based on whether token existed before
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

            // Save to database
            database.upsert_token(&token_record).await?;

            // Update cache
            cache.write().await.insert(symbol.symbol.clone(), token_record.clone());

            if is_new {
                new_symbols.push(token_record);
                database.add_history_event(&symbol.symbol, "new_listing", None).await?;
            }

            updated_count += 1;
        }

        // Log new listings with visual badges
        if !new_symbols.is_empty() {
            tracing::info!("🆕 NEW LISTINGS DETECTED: {} tokens", new_symbols.len());
            for token in &new_symbols {
                tracing::info!(
                    "  {} {} ({}/{}) - {}",
                    token.get_badge(),
                    token.symbol,
                    token.base_currency,
                    token.quote_currency,
                    token.get_colored_status()
                );
            }
        }

        // Check for delisted tokens
        let cached_symbols: Vec<String> = cache.read().await.keys().cloned().collect();
        let api_symbols: std::collections::HashSet<String> = symbols
            .iter()
            .map(|s| s.symbol.clone())
            .collect();

        for cached_symbol in cached_symbols {
            if !api_symbols.contains(&cached_symbol) {
                tracing::warn!("⚠️  Token possibly delisted: {}", cached_symbol);
                database.mark_as_delisted(&cached_symbol).await?;
                cache.write().await.remove(&cached_symbol);
            }
        }

        Ok(updated_count)
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_token_registry() {
        // Test will be added when integrated
    }
}

