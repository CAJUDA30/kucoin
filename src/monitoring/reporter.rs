use anyhow::Result;
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};

use super::database::{TokenDatabase, TokenStatistics};
use super::token_detector::ListingReport;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringReport {
    pub statistics: TokenStatistics,
    pub listings_last_hour: usize,
    pub listings_last_24h: usize,
    pub listings_last_7d: usize,
    pub listings_last_30d: usize,
}

pub struct TokenReporter {
    database: std::sync::Arc<TokenDatabase>,
}

impl TokenReporter {
    pub fn new(database: std::sync::Arc<TokenDatabase>) -> Self {
        Self { database }
    }

    pub async fn generate_summary(&self) -> Result<MonitoringReport> {
        let stats = self.database.get_statistics().await?;
        let now = Utc::now();

        let listings_1h = self
            .database
            .get_new_listings(now - Duration::hours(1))
            .await?
            .len();
        let listings_24h = self
            .database
            .get_new_listings(now - Duration::hours(24))
            .await?
            .len();
        let listings_7d = self
            .database
            .get_new_listings(now - Duration::days(7))
            .await?
            .len();
        let listings_30d = self
            .database
            .get_new_listings(now - Duration::days(30))
            .await?
            .len();

        Ok(MonitoringReport {
            statistics: stats,
            listings_last_hour: listings_1h,
            listings_last_24h: listings_24h,
            listings_last_7d: listings_7d,
            listings_last_30d: listings_30d,
        })
    }

    pub async fn generate_listing_report(&self, hours: i64) -> Result<ListingReport> {
        let now = Utc::now();
        let since = now - Duration::hours(hours);
        let listings = self.database.get_new_listings(since).await?;

        Ok(ListingReport::generate(since, now, listings))
    }

    pub fn format_summary(&self, report: &MonitoringReport) -> String {
        let mut output = String::new();

        output.push_str("\n╔══════════════════════════════════════════════════════════════════════╗\n");
        output.push_str("║             TOKEN MONITORING SUMMARY                                ║\n");
        output.push_str("╚══════════════════════════════════════════════════════════════════════╝\n\n");

        output.push_str(&format!(
            "📊 Total Tokens: {} (Active: {} | Delisted: {})\n\n",
            report.statistics.total_tokens,
            report.statistics.active_tokens,
            report.statistics.delisted_tokens
        ));

        output.push_str("🆕 New Listings:\n");
        output.push_str(&format!("   • Last Hour:   {}\n", report.listings_last_hour));
        output.push_str(&format!("   • Last 24h:    {}\n", report.listings_last_24h));
        output.push_str(&format!("   • Last 7 days: {}\n", report.listings_last_7d));
        output.push_str(&format!("   • Last 30 days: {}\n\n", report.listings_last_30d));

        output.push_str("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

        output
    }
}

