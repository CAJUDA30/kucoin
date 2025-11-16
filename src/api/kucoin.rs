use anyhow::{Context, Result};
use base64::{engine::general_purpose, Engine as _};
use hmac::{Hmac, Mac};
use reqwest::Client;
use serde::de::DeserializeOwned;
use sha2::Sha256;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use super::types::*;
use super::rate_limiter::{KuCoinRateLimiter, RateLimitConfig};
use crate::core::Config;

type HmacSha256 = Hmac<Sha256>;

pub struct KuCoinClient {
    client: Client,
    config: Config,
    rate_limiter: Arc<KuCoinRateLimiter>,
}

impl KuCoinClient {
    pub fn new(config: Config) -> Self {
        let rate_limiter = Arc::new(KuCoinRateLimiter::new(RateLimitConfig::default()));
        
        Self {
            client: Client::new(),
            config,
            rate_limiter,
        }
    }
    
    /// Get rate limiter statistics
    pub async fn get_rate_limit_stats(&self) -> super::rate_limiter::RateLimiterStats {
        self.rate_limiter.get_stats().await
    }

    fn generate_signature(
        &self,
        timestamp: u64,
        method: &str,
        endpoint: &str,
        body: &str,
    ) -> String {
        let str_to_sign = format!("{}{}{}{}", timestamp, method, endpoint, body);

        let mut mac = HmacSha256::new_from_slice(self.config.kucoin.api_secret.as_bytes())
            .expect("HMAC can take key of any size");
        mac.update(str_to_sign.as_bytes());

        general_purpose::STANDARD.encode(mac.finalize().into_bytes())
    }

    fn generate_passphrase(&self) -> String {
        let mut mac = HmacSha256::new_from_slice(self.config.kucoin.api_secret.as_bytes())
            .expect("HMAC can take key of any size");
        mac.update(self.config.kucoin.api_passphrase.as_bytes());

        general_purpose::STANDARD.encode(mac.finalize().into_bytes())
    }

    async fn request<T: DeserializeOwned>(
        &self,
        method: &str,
        endpoint: &str,
        body: Option<serde_json::Value>,
    ) -> Result<T> {
        // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
        // RATE LIMITING ENFORCEMENT - DO NOT BYPASS!
        // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
        let weight = KuCoinRateLimiter::get_endpoint_weight(endpoint);
        let _guard = self.rate_limiter.acquire(endpoint, weight).await?;
        
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64;

        let body_str = body
            .as_ref()
            .map(|b| serde_json::to_string(b).unwrap())
            .unwrap_or_default();

        let signature = self.generate_signature(timestamp, method, endpoint, &body_str);
        let passphrase = self.generate_passphrase();

        let url = format!("{}{}", self.config.kucoin.base_url, endpoint);

        let mut request = match method {
            "GET" => self.client.get(&url),
            "POST" => self.client.post(&url),
            "DELETE" => self.client.delete(&url),
            _ => return Err(anyhow::anyhow!("Unsupported HTTP method")),
        };

        request = request
            .header("KC-API-KEY", &self.config.kucoin.api_key)
            .header("KC-API-SIGN", signature)
            .header("KC-API-TIMESTAMP", timestamp.to_string())
            .header("KC-API-PASSPHRASE", passphrase)
            .header("KC-API-KEY-VERSION", "2")
            .header("Content-Type", "application/json");

        if let Some(body) = body {
            request = request.json(&body);
        }

        let response = request.send().await?;
        let status = response.status();

        if !status.is_success() {
            let error_text = response.text().await?;
            tracing::error!("KuCoin API error: {} - {}", status, error_text);
            return Err(anyhow::anyhow!(
                "API request failed: {} - {}",
                status,
                error_text
            ));
        }

        let kucoin_response: KuCoinResponse<T> = response
            .json()
            .await
            .context("Failed to parse KuCoin response")?;

        if kucoin_response.code != "200000" {
            return Err(anyhow::anyhow!(
                "KuCoin API error: {}",
                kucoin_response.code
            ));
        }

        Ok(kucoin_response.data)
    }

    // Account endpoints
    pub async fn get_account_info(&self) -> Result<AccountInfo> {
        tracing::debug!("Fetching futures account overview...");
        let result: AccountInfo = self.request("GET", "/api/v1/account-overview", None).await?;
        tracing::debug!("Account overview response: {:?}", result);
        Ok(result)
    }

    pub async fn get_positions(&self) -> Result<Vec<Position>> {
        tracing::debug!("Fetching all positions...");
        let result: Vec<Position> = self.request("GET", "/api/v1/positions", None).await?;
        tracing::debug!("Positions response: {} positions found", result.len());
        Ok(result)
    }
    
    // Get account overview with specific currency
    pub async fn get_account_overview_currency(&self, currency: &str) -> Result<AccountInfo> {
        let endpoint = format!("/api/v1/account-overview?currency={}", currency);
        tracing::debug!("Fetching account overview for currency: {}", currency);
        let result: AccountInfo = self.request("GET", &endpoint, None).await?;
        tracing::debug!("Account overview for {}: {:?}", currency, result);
        Ok(result)
    }

    // Market data endpoints (public, no auth needed)
    pub async fn get_ticker(&self, symbol: &str) -> Result<Ticker> {
        let endpoint = format!("/api/v1/ticker?symbol={}", symbol);
        self.request("GET", &endpoint, None).await
    }

    // Health check
    pub async fn ping(&self) -> Result<bool> {
        let url = format!("{}/api/v1/timestamp", self.config.kucoin.base_url);
        let response = self.client.get(&url).send().await?;
        Ok(response.status().is_success())
    }

    // Test connection with authentication
    pub async fn test_connection(&self) -> Result<bool> {
        match self.get_account_info().await {
            Ok(_) => Ok(true),
            Err(e) => {
                tracing::warn!("KuCoin connection test failed: {}", e);
                Ok(false)
            }
        }
    }

    // === PHASE 4: MARKET DATA ENDPOINTS ===

    pub async fn get_all_symbols(&self) -> Result<Vec<Symbol>> {
        tracing::debug!("Fetching all active symbols...");
        self.request("GET", "/api/v1/contracts/active", None).await
    }

    pub async fn get_kline_data(
        &self,
        symbol: &str,
        granularity: i32, // 1, 5, 15, 30, 60, 120, 240, 480, 720, 1440
        from: Option<i64>,
        to: Option<i64>,
    ) -> Result<Vec<Kline>> {
        let mut endpoint = format!(
            "/api/v1/kline/query?symbol={}&granularity={}",
            symbol, granularity
        );
        
        if let Some(from_ts) = from {
            endpoint.push_str(&format!("&from={}", from_ts));
        }
        if let Some(to_ts) = to {
            endpoint.push_str(&format!("&to={}", to_ts));
        }
        
        tracing::debug!("Fetching kline data: {}", endpoint);
        let data: Vec<Vec<serde_json::Value>> = self.request("GET", &endpoint, None).await?;
        
        let klines = data.into_iter()
            .filter_map(|row| {
                if row.len() >= 6 {
                    Some(Kline {
                        time: row[0].as_i64().unwrap_or(0),
                        open: row[1].as_f64().unwrap_or(0.0),
                        high: row[2].as_f64().unwrap_or(0.0),
                        low: row[3].as_f64().unwrap_or(0.0),
                        close: row[4].as_f64().unwrap_or(0.0),
                        volume: row[5].as_f64().unwrap_or(0.0),
                    })
                } else {
                    None
                }
            })
            .collect();
        
        Ok(klines)
    }

    pub async fn get_mark_price(&self, symbol: &str) -> Result<MarkPrice> {
        let endpoint = format!("/api/v1/mark-price/{}/current", symbol);
        tracing::debug!("Fetching mark price for {}", symbol);
        self.request("GET", &endpoint, None).await
    }

    pub async fn get_funding_rate(&self, symbol: &str) -> Result<FundingRate> {
        let endpoint = format!("/api/v1/funding-rate/{}/current", symbol);
        tracing::debug!("Fetching funding rate for {}", symbol);
        self.request("GET", &endpoint, None).await
    }

    pub async fn get_symbol_info(&self, symbol: &str) -> Result<Symbol> {
        let endpoint = format!("/api/v1/contracts/{}", symbol);
        tracing::debug!("Fetching symbol info for {}", symbol);
        self.request("GET", &endpoint, None).await
    }

    pub async fn get_max_open_size(
        &self,
        symbol: &str,
        price: f64,
        leverage: i32,
    ) -> Result<MaxOpenSize> {
        let endpoint = format!(
            "/api/v1/getMaxOpenSize?symbol={}&price={}&leverage={}",
            symbol, price, leverage
        );
        tracing::debug!("Fetching max open size for {}", symbol);
        self.request("GET", &endpoint, None).await
    }

    // === PHASE 4: ORDER MANAGEMENT ENDPOINTS ===

    pub async fn place_order(&self, order: OrderRequest) -> Result<OrderResponse> {
        tracing::info!(
            "📤 Placing {} {} order for {} (leverage: {}x)",
            order.order_type,
            order.side,
            order.symbol,
            order.leverage
        );
        let body = serde_json::to_value(&order)?;
        self.request("POST", "/api/v1/orders", Some(body)).await
    }

    pub async fn cancel_order(&self, order_id: &str) -> Result<serde_json::Value> {
        let endpoint = format!("/api/v1/orders/{}", order_id);
        tracing::info!("🚫 Canceling order: {}", order_id);
        self.request("DELETE", &endpoint, None).await
    }

    pub async fn get_orders(&self, symbol: Option<&str>, status: Option<&str>) -> Result<Vec<Order>> {
        let mut endpoint = "/api/v1/orders".to_string();
        let mut params = vec![];
        
        if let Some(sym) = symbol {
            params.push(format!("symbol={}", sym));
        }
        if let Some(st) = status {
            params.push(format!("status={}", st));
        }
        
        if !params.is_empty() {
            endpoint.push_str("?");
            endpoint.push_str(&params.join("&"));
        }
        
        tracing::debug!("Fetching orders: {}", endpoint);
        let response: serde_json::Value = self.request("GET", &endpoint, None).await?;
        
        if let Some(items) = response.get("items") {
            Ok(serde_json::from_value(items.clone())?)
        } else {
            Ok(vec![])
        }
    }

    pub async fn change_leverage(&self, symbol: &str, leverage: i32) -> Result<serde_json::Value> {
        tracing::info!("⚙️  Changing leverage for {} to {}x", symbol, leverage);
        let body = serde_json::json!({
            "symbol": symbol,
            "leverage": leverage
        });
        self.request("POST", "/api/v1/changeCrossUserLeverage", Some(body)).await
    }
}
