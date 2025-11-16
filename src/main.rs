mod api;
mod core;

use anyhow::Result;
use api::KuCoinClient;
use core::{Config, HealthChecker};
use std::sync::Arc;
use warp::Filter;

#[tokio::main]
async fn main() -> Result<()> {
    // Load configuration
    let config = Config::from_env()?;

    // Initialize logging
    core::logging::init_logging(&config.monitoring.log_level);

    tracing::info!("🚀 KuCoin Ultimate Trading Bot starting...");
    tracing::info!("Version: {}", env!("CARGO_PKG_VERSION"));
    tracing::info!("Sandbox mode: {}", config.kucoin.sandbox_mode);
    tracing::info!("KuCoin API URL: {}", config.kucoin.base_url);

    // Initialize health checker
    let health_checker = Arc::new(HealthChecker::new());

    // Initialize KuCoin client
    let kucoin_client = Arc::new(KuCoinClient::new(config.clone()));

    // Test KuCoin connection
    tracing::info!("Testing KuCoin API connection...");
    match kucoin_client.ping().await {
        Ok(true) => {
            tracing::info!("✅ KuCoin API ping successful");

            // Test authentication
            match kucoin_client.test_connection().await {
                Ok(true) => {
                    tracing::info!("✅ KuCoin API authentication successful");
                    health_checker.update_component("kucoin_api", true).await;

                    // Fetch account info
                    if let Ok(account) = kucoin_client.get_account_info().await {
                        tracing::info!(
                            "💰 Account equity: {:.2} (available: {:.2})",
                            account.account_equity,
                            account.available_balance
                        );
                    }
                }
                Ok(false) => {
                    tracing::warn!("⚠️  KuCoin API authentication failed - check credentials");
                    health_checker.update_component("kucoin_api", false).await;
                }
                Err(e) => {
                    tracing::error!("❌ KuCoin API test failed: {}", e);
                    health_checker.update_component("kucoin_api", false).await;
                }
            }
        }
        Ok(false) | Err(_) => {
            tracing::warn!("⚠️  KuCoin API connection failed");
            health_checker.update_component("kucoin_api", false).await;
        }
    }

    // Start health check endpoint
    let health_clone = health_checker.clone();
    let port = config.monitoring.frontend_port;

    tokio::spawn(async move { start_health_server(health_clone, port).await });

    tracing::info!("✅ Health endpoint running on port {}", port);

    // Main monitoring loop
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60));

    loop {
        interval.tick().await;

        let status = health_checker.get_status().await;
        tracing::info!(
            "📊 Bot status: {} | uptime: {}s | api: {}",
            status.status,
            status.uptime_seconds,
            status.components.kucoin_api
        );

        // Periodically fetch account info if API is healthy
        if status.components.kucoin_api {
            if let Ok(account) = kucoin_client.get_account_info().await {
                tracing::info!(
                    "💰 Equity: {:.2} | Available: {:.2} | PnL: {:.2}",
                    account.account_equity,
                    account.available_balance,
                    account.unrealised_pnl
                );
            }
        }
    }
}

async fn start_health_server(health_checker: Arc<HealthChecker>, port: u16) {
    let health = warp::path("health")
        .and(warp::any().map(move || health_checker.clone()))
        .and_then(|checker: Arc<HealthChecker>| async move {
            let status = checker.get_status().await;
            Ok::<_, warp::Rejection>(warp::reply::json(&status))
        });

    warp::serve(health).run(([0, 0, 0, 0], port)).await;
}
