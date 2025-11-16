use anyhow::Result;
use chrono::{DateTime, Utc};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::api::KuCoinClient;

#[derive(Debug, Clone)]
pub struct NewListing {
    pub symbol: String,
    pub listing_time: DateTime<Utc>,
    pub initial_price: f64,
    pub detected_at: DateTime<Utc>,
}

pub struct NewListingDetector {
    client: Arc<KuCoinClient>,
    known_symbols: Arc<RwLock<HashSet<String>>>,
    new_listings: Arc<RwLock<Vec<NewListing>>>,
}

impl NewListingDetector {
    pub fn new(client: Arc<KuCoinClient>) -> Self {
        Self {
            client,
            known_symbols: Arc::new(RwLock::new(HashSet::new())),
            new_listings: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn start(&self) -> Result<()> {
        tracing::info!("🆕 New listing detector starting...");

        // Initial scan to populate known symbols
        self.update_known_symbols().await?;

        let client = self.client.clone();
        let known_symbols = self.known_symbols.clone();
        let new_listings = self.new_listings.clone();

        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(tokio::time::Duration::from_secs(60)); // Check every minute

            loop {
                interval.tick().await;

                if let Err(e) =
                    Self::detect_new_listings(&client, &known_symbols, &new_listings).await
                {
                    tracing::error!("New listing detection error: {}", e);
                }
            }
        });

        Ok(())
    }

    async fn update_known_symbols(&self) -> Result<()> {
        // Get current symbols from API
        let symbols = self.fetch_all_symbols().await?;

        let mut known = self.known_symbols.write().await;
        for symbol in symbols {
            known.insert(symbol);
        }

        tracing::info!("Initialized with {} known symbols", known.len());
        Ok(())
    }

    async fn detect_new_listings(
        client: &Arc<KuCoinClient>,
        known_symbols: &Arc<RwLock<HashSet<String>>>,
        new_listings: &Arc<RwLock<Vec<NewListing>>>,
    ) -> Result<()> {
        let current_symbols = Self::fetch_all_symbols_static(client).await?;
        let mut known = known_symbols.write().await;

        for symbol in current_symbols {
            if !known.contains(&symbol) {
                tracing::info!("🆕 NEW LISTING DETECTED: {}", symbol);

                // Get initial price
                let initial_price = match client.get_ticker(&symbol).await {
                    Ok(ticker) => ticker.price.parse::<f64>().unwrap_or(0.0),
                    Err(_) => 0.0,
                };

                let listing = NewListing {
                    symbol: symbol.clone(),
                    listing_time: Utc::now(),
                    initial_price,
                    detected_at: Utc::now(),
                };

                new_listings.write().await.push(listing);
                known.insert(symbol);
            }
        }

        Ok(())
    }

    async fn fetch_all_symbols(&self) -> Result<Vec<String>> {
        Self::fetch_all_symbols_static(&self.client).await
    }

    async fn fetch_all_symbols_static(_client: &Arc<KuCoinClient>) -> Result<Vec<String>> {
        // This would call KuCoin API to get ALL available symbols
        // For now, return a static list
        Ok(vec![
            "XBTUSDTM".to_string(),
            "ETHUSDTM".to_string(),
            "SOLUSDTM".to_string(),
        ])
    }

    pub async fn get_recent_listings(&self, hours: i64) -> Vec<NewListing> {
        let listings = self.new_listings.read().await;
        let cutoff = Utc::now() - chrono::Duration::hours(hours);

        listings
            .iter()
            .filter(|l| l.detected_at > cutoff)
            .cloned()
            .collect()
    }
}
