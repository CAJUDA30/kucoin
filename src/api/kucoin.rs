use anyhow::{Context, Result};
use base64::{engine::general_purpose, Engine as _};
use hmac::{Hmac, Mac};
use reqwest::Client;
use serde::de::DeserializeOwned;
use sha2::Sha256;
use std::time::{SystemTime, UNIX_EPOCH};

use super::types::*;
use crate::core::Config;

type HmacSha256 = Hmac<Sha256>;

pub struct KuCoinClient {
    client: Client,
    config: Config,
}

impl KuCoinClient {
    pub fn new(config: Config) -> Self {
        Self {
            client: Client::new(),
            config,
        }
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
        self.request("GET", "/api/v1/account-overview", None).await
    }

    pub async fn get_positions(&self) -> Result<Vec<Position>> {
        self.request("GET", "/api/v1/positions", None).await
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
}
