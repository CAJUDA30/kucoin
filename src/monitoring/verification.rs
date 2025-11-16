use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::api::KuCoinClient;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub total_symbols_fetched: usize,
    pub api_response_complete: bool,
    pub pagination_handled: bool,
    pub rate_limit_status: RateLimitStatus,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitStatus {
    pub remaining_requests: Option<i32>,
    pub reset_time: Option<chrono::DateTime<chrono::Utc>>,
    pub within_limits: bool,
}

pub struct APIVerifier {
    client: Arc<KuCoinClient>,
}

impl APIVerifier {
    pub fn new(client: Arc<KuCoinClient>) -> Self {
        Self { client }
    }

    pub async fn verify_completeness(&self) -> Result<VerificationResult> {
        let mut result = VerificationResult {
            timestamp: Utc::now(),
            total_symbols_fetched: 0,
            api_response_complete: false,
            pagination_handled: true, // KuCoin Futures API returns all symbols in one call
            rate_limit_status: RateLimitStatus {
                remaining_requests: None,
                reset_time: None,
                within_limits: true,
            },
            errors: Vec::new(),
            warnings: Vec::new(),
        };

        // Try to fetch all symbols
        match self.client.get_all_symbols().await {
            Ok(symbols) => {
                result.total_symbols_fetched = symbols.len();
                result.api_response_complete = true;

                // Verify we got a reasonable number of symbols
                if symbols.is_empty() {
                    result.warnings.push("No symbols returned from API".to_string());
                } else if symbols.len() < 3 {
                    result.warnings.push(format!(
                        "Unusually low number of symbols: {}",
                        symbols.len()
                    ));
                } else {
                    tracing::debug!("✅ Fetched {} symbols successfully", symbols.len());
                }

                // Check for duplicate symbols
                let unique_symbols: std::collections::HashSet<_> =
                    symbols.iter().map(|s| &s.symbol).collect();
                if unique_symbols.len() != symbols.len() {
                    result.warnings.push(format!(
                        "Duplicate symbols detected: {} unique out of {} total",
                        unique_symbols.len(),
                        symbols.len()
                    ));
                }

                // Check for symbols with invalid/empty fields
                for symbol in &symbols {
                    if symbol.symbol.is_empty() {
                        result.errors.push("Found symbol with empty name".to_string());
                    }
                    if symbol.base_currency.is_empty() {
                        result.warnings.push(format!(
                            "Symbol {} has empty base_currency",
                            symbol.symbol
                        ));
                    }
                    if symbol.quote_currency.is_empty() {
                        result.warnings.push(format!(
                            "Symbol {} has empty quote_currency",
                            symbol.symbol
                        ));
                    }
                }
            }
            Err(e) => {
                result.api_response_complete = false;
                result.errors.push(format!("API call failed: {}", e));
            }
        }

        Ok(result)
    }

    pub fn format_verification_result(&self, result: &VerificationResult) -> String {
        let mut output = String::new();

        output.push_str("\n╔══════════════════════════════════════════════════════════════════════╗\n");
        output.push_str("║             API COMPLETENESS VERIFICATION                           ║\n");
        output.push_str("╚══════════════════════════════════════════════════════════════════════╝\n\n");

        output.push_str(&format!(
            "🕐 Timestamp: {}\n\n",
            result.timestamp.format("%Y-%m-%d %H:%M:%S UTC")
        ));

        output.push_str(&format!("📊 Symbols Fetched: {}\n", result.total_symbols_fetched));
        output.push_str(&format!(
            "✅ API Response: {}\n",
            if result.api_response_complete {
                "Complete"
            } else {
                "❌ Incomplete"
            }
        ));
        output.push_str(&format!(
            "📄 Pagination: {}\n\n",
            if result.pagination_handled {
                "Handled"
            } else {
                "⚠️  Needs Attention"
            }
        ));

        if !result.errors.is_empty() {
            output.push_str("❌ ERRORS:\n");
            for error in &result.errors {
                output.push_str(&format!("   • {}\n", error));
            }
            output.push_str("\n");
        }

        if !result.warnings.is_empty() {
            output.push_str("⚠️  WARNINGS:\n");
            for warning in &result.warnings {
                output.push_str(&format!("   • {}\n", warning));
            }
            output.push_str("\n");
        }

        if result.errors.is_empty() && result.warnings.is_empty() {
            output.push_str("✅ All checks passed! API is functioning correctly.\n\n");
        }

        output.push_str("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

        output
    }
}

