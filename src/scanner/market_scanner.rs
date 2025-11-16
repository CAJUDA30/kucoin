use anyhow::Result;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::api::{KuCoinClient, Ticker};

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
    scan_interval_secs: u64,
}

impl MarketScanner {
    pub fn new(client: Arc<KuCoinClient>) -> Self {
        Self {
            client,
            snapshots: Arc::new(RwLock::new(HashMap::new())),
            scan_interval_secs: 10, // Scan every 10 seconds
        }
    }

    pub async fn start(&self) -> Result<()> {
        tracing::info!("🔍 Market scanner starting...");

        let client = self.client.clone();
        let snapshots = self.snapshots.clone();
        let interval_secs = self.scan_interval_secs;

        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(tokio::time::Duration::from_secs(interval_secs));

            loop {
                interval.tick().await;

                if let Err(e) = Self::scan_markets(&client, &snapshots).await {
                    tracing::error!("Market scan error: {}", e);
                }
            }
        });

        Ok(())
    }

    async fn scan_markets(
        client: &Arc<KuCoinClient>,
        snapshots: &Arc<RwLock<HashMap<String, MarketSnapshot>>>,
    ) -> Result<()> {
        // Get list of all futures symbols
        let symbols = Self::get_active_symbols(client).await?;

        tracing::debug!("Scanning {} symbols", symbols.len());

        for symbol in symbols {
            match client.get_ticker(&symbol).await {
                Ok(ticker) => {
                    let snapshot = Self::create_snapshot(&symbol, &ticker);
                    snapshots.write().await.insert(symbol.clone(), snapshot);
                }
                Err(e) => {
                    tracing::warn!("Failed to get ticker for {}: {}", symbol, e);
                }
            }
        }

        Ok(())
    }

    async fn get_active_symbols(_client: &Arc<KuCoinClient>) -> Result<Vec<String>> {
        // This would call KuCoin API to get all active futures symbols
        // For now, return popular pairs
        Ok(vec![
            "XBTUSDTM".to_string(),  // Bitcoin
            "ETHUSDTM".to_string(),  // Ethereum
            "SOLUSDTM".to_string(),  // Solana
            "BNBUSDTM".to_string(),  // BNB
            "ADAUSDTM".to_string(),  // Cardano
            "DOTUSDTM".to_string(),  // Polkadot
            "MATICUSDTM".to_string(), // Polygon
            "AVAXUSDTM".to_string(), // Avalanche
        ])
    }

    fn create_snapshot(symbol: &str, ticker: &Ticker) -> MarketSnapshot {
        let price = ticker.price.parse::<f64>().unwrap_or(0.0);
        let volume = ticker.size as f64;

        MarketSnapshot {
            symbol: symbol.to_string(),
            price,
            volume_24h: volume,
            price_change_24h: 0.0, // Calculate from historical data
            volatility: 0.0,        // Calculate from price history
            timestamp: Utc::now(),
        }
    }

    pub async fn get_top_opportunities(&self, limit: usize) -> Result<Vec<ScanResult>> {
        let snapshots = self.snapshots.read().await;
        let mut opportunities = Vec::new();

        for (symbol, snapshot) in snapshots.iter() {
            let score = Self::calculate_opportunity_score(snapshot);

            if score > 0.5 {
                // Threshold for consideration
                opportunities.push(ScanResult {
                    symbol: symbol.clone(),
                    score,
                    signals: Self::generate_signals(snapshot),
                    recommended_leverage: Self::calculate_leverage(snapshot),
                });
            }
        }

        // Sort by score descending
        opportunities.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        opportunities.truncate(limit);

        Ok(opportunities)
    }

    fn calculate_opportunity_score(snapshot: &MarketSnapshot) -> f64 {
        let mut score: f64 = 0.0;

        // Volume factor (0-0.3)
        if snapshot.volume_24h > 1_000_000.0 {
            score += 0.3;
        } else if snapshot.volume_24h > 100_000.0 {
            score += 0.2;
        } else if snapshot.volume_24h > 10_000.0 {
            score += 0.1;
        }

        // Volatility factor (0-0.3)
        if snapshot.volatility > 0.05 {
            score += 0.3;
        } else if snapshot.volatility > 0.03 {
            score += 0.2;
        } else if snapshot.volatility > 0.01 {
            score += 0.1;
        }

        // Price change factor (0-0.4)
        let abs_change = snapshot.price_change_24h.abs();
        if abs_change > 0.10 {
            score += 0.4;
        } else if abs_change > 0.05 {
            score += 0.3;
        } else if abs_change > 0.02 {
            score += 0.2;
        }

        score.min(1.0_f64)
    }

    fn generate_signals(snapshot: &MarketSnapshot) -> Vec<String> {
        let mut signals = Vec::new();

        if snapshot.volume_24h > 1_000_000.0 {
            signals.push("High volume".to_string());
        }

        if snapshot.volatility > 0.05 {
            signals.push("High volatility".to_string());
        }

        if snapshot.price_change_24h > 0.05 {
            signals.push("Strong uptrend".to_string());
        } else if snapshot.price_change_24h < -0.05 {
            signals.push("Strong downtrend".to_string());
        }

        signals
    }

    fn calculate_leverage(snapshot: &MarketSnapshot) -> u32 {
        // Lower leverage for high volatility
        if snapshot.volatility > 0.10 {
            3
        } else if snapshot.volatility > 0.05 {
            5
        } else if snapshot.volatility > 0.03 {
            7
        } else {
            10
        }
    }

    pub async fn get_snapshot(&self, symbol: &str) -> Option<MarketSnapshot> {
        self.snapshots.read().await.get(symbol).cloned()
    }
}
