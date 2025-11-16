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
            "📊 Total Tokens: {} (\x1b[32;1m🆕 NEW: {}\x1b[0m | \x1b[32m✅ Active: {}\x1b[0m | \x1b[31m🔴 Delisted: {}\x1b[0m)\n\n",
            report.statistics.total_tokens,
            report.statistics.new_tokens,
            report.statistics.active_tokens - report.statistics.new_tokens,
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

    /// Generate categorized report with visual badges
    pub async fn generate_categorized_report(&self) -> Result<String> {
        let categories = self.database.get_tokens_by_category().await?;
        let mut output = String::new();

        output.push_str("\n╔══════════════════════════════════════════════════════════════════════╗\n");
        output.push_str("║          CATEGORIZED TOKEN LISTINGS REPORT                          ║\n");
        output.push_str("╚══════════════════════════════════════════════════════════════════════╝\n\n");

        // NEW LISTINGS SECTION
        if !categories.new_listings.is_empty() {
            output.push_str("\x1b[32;1m🆕 NEW LISTINGS (Last 24 Hours)\x1b[0m\n");
            output.push_str("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");
            for token in &categories.new_listings {
                let age_hours = (Utc::now() - token.first_seen).num_hours();
                output.push_str(&format!(
                    "  {} {} ({}/{}) - Listed {} hours ago\n",
                    token.get_badge(),
                    token.symbol,
                    token.base_currency,
                    token.quote_currency,
                    age_hours
                ));
            }
            output.push_str(&format!("\nTotal NEW: {}\n\n", categories.new_listings.len()));
        }

        // ACTIVE LISTINGS SECTION
        output.push_str("\x1b[32m✅ ACTIVE LISTINGS\x1b[0m\n");
        output.push_str("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");
        let display_count = std::cmp::min(10, categories.active_listings.len());
        for token in categories.active_listings.iter().take(display_count) {
            output.push_str(&format!(
                "  {} {} ({}/{})\n",
                token.get_badge(),
                token.symbol,
                token.base_currency,
                token.quote_currency
            ));
        }
        if categories.active_listings.len() > display_count {
            output.push_str(&format!(
                "  ... and {} more active listings\n",
                categories.active_listings.len() - display_count
            ));
        }
        output.push_str(&format!("\nTotal ACTIVE: {}\n\n", categories.active_listings.len()));

        // DELISTED TOKENS SECTION
        if !categories.delisted_tokens.is_empty() {
            output.push_str("\x1b[31m🔴 DELISTED TOKENS\x1b[0m\n");
            output.push_str("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");
            for token in categories.delisted_tokens.iter().take(5) {
                let delisted_at = token.delisted_at.unwrap_or(token.last_seen);
                output.push_str(&format!(
                    "  {} {} - Delisted {}\n",
                    token.get_badge(),
                    token.symbol,
                    delisted_at.format("%Y-%m-%d")
                ));
            }
            output.push_str(&format!("\nTotal DELISTED: {}\n\n", categories.delisted_tokens.len()));
        }

        output.push_str("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

        Ok(output)
    }
}

