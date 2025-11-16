use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use std::sync::Arc;
use tokio::sync::RwLock;

use super::database::{TokenDatabase, TokenRecord};
use super::token_registry::TokenRegistry;

pub struct NewTokenDetector {
    registry: Arc<TokenRegistry>,
    database: Arc<TokenDatabase>,
    notification_callbacks: Arc<RwLock<Vec<NotificationCallback>>>,
    check_interval_secs: u64,
}

type NotificationCallback = Box<dyn Fn(&TokenRecord) + Send + Sync>;

impl NewTokenDetector {
    pub fn new(
        registry: Arc<TokenRegistry>,
        database: Arc<TokenDatabase>,
        check_interval_secs: u64,
    ) -> Self {
        Self {
            registry,
            database,
            notification_callbacks: Arc::new(RwLock::new(Vec::new())),
            check_interval_secs,
        }
    }

    pub async fn add_notification_callback<F>(&self, callback: F)
    where
        F: Fn(&TokenRecord) + Send + Sync + 'static,
    {
        self.notification_callbacks
            .write()
            .await
            .push(Box::new(callback));
    }

    pub async fn start(&self) -> Result<()> {
        tracing::info!("🔔 New Token Detector starting...");

        let _registry = self.registry.clone();
        let database = self.database.clone();
        let callbacks = self.notification_callbacks.clone();
        let interval_secs = self.check_interval_secs;

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(
                tokio::time::Duration::from_secs(interval_secs)
            );

            loop {
                interval.tick().await;

                // Check for new listings in the last interval
                let since = Utc::now() - Duration::seconds(interval_secs as i64 * 2);

                match database.get_new_listings(since).await {
                    Ok(new_listings) => {
                        if !new_listings.is_empty() {
                            tracing::info!(
                                "🆕 Detected {} new listings in the last {} seconds",
                                new_listings.len(),
                                interval_secs * 2
                            );

                            for token in &new_listings {
                                // Log the new listing
                                tracing::info!(
                                    "🎊 NEW TOKEN: {} ({}/{}) - First seen: {}",
                                    token.symbol,
                                    token.base_currency,
                                    token.quote_currency,
                                    token.first_seen.format("%Y-%m-%d %H:%M:%S UTC")
                                );

                                // Notify all registered callbacks
                                let callbacks_guard = callbacks.read().await;
                                for callback in callbacks_guard.iter() {
                                    callback(token);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("❌ Failed to check new listings: {}", e);
                    }
                }
            }
        });

        tracing::info!(
            "✅ New Token Detector started (check every {}s)",
            self.check_interval_secs
        );

        Ok(())
    }

    pub async fn get_recent_listings(&self, hours: i64) -> Result<Vec<TokenRecord>> {
        let since = Utc::now() - Duration::hours(hours);
        self.database.get_new_listings(since).await
    }

    pub async fn get_today_listings(&self) -> Result<Vec<TokenRecord>> {
        let today_start = Utc::now()
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc();
        self.database.get_new_listings(today_start).await
    }
}

#[derive(Debug, Clone)]
pub struct ListingReport {
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub new_listings: Vec<TokenRecord>,
    pub total_count: usize,
}

impl ListingReport {
    pub fn generate(
        period_start: DateTime<Utc>,
        period_end: DateTime<Utc>,
        new_listings: Vec<TokenRecord>,
    ) -> Self {
        let total_count = new_listings.len();
        Self {
            period_start,
            period_end,
            new_listings,
            total_count,
        }
    }

    pub fn to_string(&self) -> String {
        let mut report = format!(
            "\n╔══════════════════════════════════════════════════════════════════════╗\n"
        );
        report.push_str(&format!(
            "║             NEW TOKEN LISTINGS REPORT                               ║\n"
        ));
        report.push_str(&format!(
            "╚══════════════════════════════════════════════════════════════════════╝\n"
        ));
        report.push_str(&format!(
            "\nPeriod: {} to {}\n",
            self.period_start.format("%Y-%m-%d %H:%M UTC"),
            self.period_end.format("%Y-%m-%d %H:%M UTC")
        ));
        report.push_str(&format!("Total New Listings: {}\n\n", self.total_count));

        if self.new_listings.is_empty() {
            report.push_str("No new listings during this period.\n");
        } else {
            report.push_str("┌────────────────────────────────────────────────────────────────┐\n");
            report.push_str("│ Symbol           │ Base/Quote    │ First Seen          │ Status │\n");
            report.push_str("├────────────────────────────────────────────────────────────────┤\n");

            for token in &self.new_listings {
                report.push_str(&format!(
                    "│ {:16} │ {}/{:6} │ {} │ {:6} │\n",
                    token.symbol,
                    token.base_currency,
                    token.quote_currency,
                    token.first_seen.format("%Y-%m-%d %H:%M"),
                    token.status
                ));
            }

            report.push_str("└────────────────────────────────────────────────────────────────┘\n");
        }

        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_listing_report() {
        // Test report generation
        let report = ListingReport::generate(
            Utc::now() - Duration::hours(24),
            Utc::now(),
            vec![],
        );
        assert_eq!(report.total_count, 0);
    }
}

