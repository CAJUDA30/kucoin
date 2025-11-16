mod core;

use anyhow::Result;
use core::{Config, HealthChecker};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    // Load configuration
    let config = Config::from_env()?;

    // Initialize logging
    core::logging::init_logging(&config.monitoring.log_level);

    tracing::info!("🚀 KuCoin Ultimate Trading Bot starting...");
    tracing::info!("Version: {}", env!("CARGO_PKG_VERSION"));
    tracing::info!("Sandbox mode: {}", config.kucoin.sandbox_mode);

    // Initialize health checker
    let health_checker = Arc::new(HealthChecker::new());

    // Start health check endpoint
    let health_clone = health_checker.clone();
    let health_port = config.monitoring.frontend_port;
    tokio::spawn(async move { start_health_server(health_clone, health_port).await });

    tracing::info!(
        "✅ Health endpoint running on port {}",
        config.monitoring.frontend_port
    );

    // Keep running
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
        let status = health_checker.get_status().await;
        tracing::info!(
            "Bot status: {} (uptime: {}s)",
            status.status,
            status.uptime_seconds
        );
    }
}

async fn start_health_server(health_checker: Arc<HealthChecker>, port: u16) {
    use warp::Filter;

    let health = warp::path("health")
        .and(warp::any().map(move || health_checker.clone()))
        .and_then(|checker: Arc<HealthChecker>| async move {
            let status = checker.get_status().await;
            Ok::<_, warp::Rejection>(warp::reply::json(&status))
        });

    warp::serve(health).run(([0, 0, 0, 0], port)).await;
}
