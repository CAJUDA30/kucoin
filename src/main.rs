mod api;
mod core;
mod execution;
mod scanner;
mod strategy;

use anyhow::Result;
use api::KuCoinClient;
use core::{Config, HealthChecker};
use execution::{OrderManager, PositionTracker};
use scanner::{MarketScanner, NewListingDetector};
use strategy::AIStrategyEngine;
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

    // Initialize trading components
    tracing::info!("🔧 Initializing trading components...");
    
    let position_tracker = Arc::new(PositionTracker::new());
    let order_manager = Arc::new(tokio::sync::RwLock::new(OrderManager::new(
        kucoin_client.clone(),
        config.clone(),
        position_tracker.clone(),
    )));

    let market_scanner = Arc::new(MarketScanner::new(kucoin_client.clone()));
    let new_listing_detector = Arc::new(NewListingDetector::new(kucoin_client.clone()));

    // Start market scanner
    market_scanner.start().await?;
    health_checker.update_component("market_scanner", true).await;

    // Start new listing detector
    new_listing_detector.start().await?;
    health_checker.update_component("new_listing_detector", true).await;

    // Start health check endpoint
    let health_clone = health_checker.clone();
    let port = config.monitoring.frontend_port;

    tokio::spawn(async move { start_health_server(health_clone, port).await });

    tracing::info!("✅ Health endpoint running on port {}", port);
    tracing::info!("🔸 Trading mode: DRY RUN (orders simulated)");
    tracing::info!("🤖 AI Strategy Engine: ACTIVE");
    tracing::info!("🔍 Market Scanner: ACTIVE");
    tracing::info!("🆕 New Listing Detector: ACTIVE");
    tracing::info!("");
    tracing::info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    tracing::info!("🚀 BOT IS LIVE - Scanning markets and generating signals");
    tracing::info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // Main trading loop
    let mut scan_interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
    let mut position_check_interval = tokio::time::interval(tokio::time::Duration::from_secs(10));
    let mut status_interval = tokio::time::interval(tokio::time::Duration::from_secs(60));

    loop {
        tokio::select! {
            _ = scan_interval.tick() => {
                // Scan markets and generate signals
                if let Err(e) = scan_and_trade(
                    &market_scanner,
                    &new_listing_detector,
                    &order_manager,
                    &health_checker,
                ).await {
                    tracing::error!("Error in scan_and_trade: {}", e);
                }
            }
            
            _ = position_check_interval.tick() => {
                // Check existing positions for stop loss / take profit
                if let Err(e) = order_manager.read().await.check_positions().await {
                    tracing::error!("Error checking positions: {}", e);
                }
            }
            
            _ = status_interval.tick() => {
                // Print status summary
                print_status(
                    &health_checker,
                    &kucoin_client,
                    &order_manager,
                    &position_tracker,
                ).await;
            }
        }
    }
}

async fn scan_and_trade(
    market_scanner: &Arc<MarketScanner>,
    new_listing_detector: &Arc<NewListingDetector>,
    order_manager: &Arc<tokio::sync::RwLock<OrderManager>>,
    health_checker: &Arc<HealthChecker>,
) -> Result<()> {
    // Check for new listings (high priority)
    let recent_listings = new_listing_detector.get_recent_listings(1).await;
    if !recent_listings.is_empty() {
        for listing in recent_listings {
            tracing::info!(
                "🆕 NEW LISTING: {} @ {:.2} (detected {} ago)",
                listing.symbol,
                listing.initial_price,
                format_duration(chrono::Utc::now() - listing.detected_at)
            );
        }
    }

    // Get top trading opportunities
    let opportunities = market_scanner.get_top_opportunities(5).await?;

    if opportunities.is_empty() {
        tracing::debug!("No trading opportunities found in current scan");
        return Ok(());
    }

    tracing::info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    tracing::info!("🎯 Found {} trading opportunities:", opportunities.len());

    for (idx, opportunity) in opportunities.iter().enumerate() {
        tracing::info!(
            "  {}. {} | Score: {:.2} | Signals: {} | Leverage: {}x",
            idx + 1,
            opportunity.symbol,
            opportunity.score,
            opportunity.signals.join(", "),
            opportunity.recommended_leverage
        );

        // Get market snapshot for this symbol
        if let Some(snapshot) = market_scanner.get_snapshot(&opportunity.symbol).await {
            // Analyze with AI engine
            let mut ai_engine = AIStrategyEngine::new();
            
            if let Ok(Some(signal)) = ai_engine.analyze(&snapshot) {
                tracing::info!(
                    "  ↳ 🤖 AI Signal: {} | Confidence: {:.2} | Reason: {}",
                    match signal.signal_type {
                        strategy::SignalType::Long => "LONG",
                        strategy::SignalType::Short => "SHORT",
                        _ => "HOLD",
                    },
                    signal.confidence,
                    signal.reason
                );

                // Execute signal through order manager
                let order_mgr = order_manager.read().await;
                match order_mgr.execute_signal(&signal).await {
                    Ok(true) => {
                        tracing::info!("  ↳ ✅ Order executed successfully");
                        health_checker.update_component("ai_engine", true).await;
                    }
                    Ok(false) => {
                        tracing::debug!("  ↳ ⏭️  Order skipped (validation failed or duplicate)");
                    }
                    Err(e) => {
                        tracing::error!("  ↳ ❌ Order execution error: {}", e);
                        health_checker.update_component("ai_engine", false).await;
                    }
                }
            }
        }
    }

    tracing::info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    Ok(())
}

async fn print_status(
    health_checker: &Arc<HealthChecker>,
    kucoin_client: &Arc<KuCoinClient>,
    order_manager: &Arc<tokio::sync::RwLock<OrderManager>>,
    position_tracker: &Arc<PositionTracker>,
) {
    let status = health_checker.get_status().await;
    
    tracing::info!("");
    tracing::info!("┌────────────────────────────────────────────────────┐");
    tracing::info!("│              📊 BOT STATUS REPORT                  │");
    tracing::info!("├────────────────────────────────────────────────────┤");
    tracing::info!("│ Status: {}                              ", status.status);
    tracing::info!("│ Uptime: {} seconds                      ", status.uptime_seconds);
    tracing::info!("│                                                    │");
    tracing::info!("│ 🔌 Components:                                     │");
    tracing::info!("│   • KuCoin API:     {}                        ", 
        if status.components.kucoin_api { "✅" } else { "❌" });
    tracing::info!("│   • Market Scanner: {}                        ", 
        if status.components.get("market_scanner").unwrap_or(false) { "✅" } else { "❌" });
    tracing::info!("│   • New Listings:   {}                        ", 
        if status.components.get("new_listing_detector").unwrap_or(false) { "✅" } else { "❌" });
    tracing::info!("│   • AI Engine:      {}                        ", 
        if status.components.get("ai_engine").unwrap_or(false) { "✅" } else { "⏸️" });
    tracing::info!("│                                                    │");

    // Account info
    if status.components.kucoin_api {
        if let Ok(account) = kucoin_client.get_account_info().await {
            tracing::info!("│ 💰 Account:                                        │");
            tracing::info!("│   • Equity:     {:.2}                    ", account.account_equity);
            tracing::info!("│   • Available:  {:.2}                    ", account.available_balance);
            tracing::info!("│   • PnL:        {:.2}                    ", account.unrealised_pnl);
        }
    }

    // Position summary
    let position_count = position_tracker.position_count().await;
    let total_pnl = position_tracker.total_pnl().await;
    
    tracing::info!("│                                                    │");
    tracing::info!("│ 📈 Positions:                                      │");
    tracing::info!("│   • Open:       {}                              ", position_count);
    tracing::info!("│   • Total PnL:  {:.2}                          ", total_pnl);
    
    if position_count > 0 {
        let positions = position_tracker.get_all_positions().await;
        for pos in positions.iter().take(3) {
            tracing::info!("│   • {} {} @ {:.2} → {:.2} ({:.2}%)", 
                pos.symbol,
                match pos.side {
                    execution::PositionSide::Long => "L",
                    execution::PositionSide::Short => "S",
                },
                pos.entry_price,
                pos.current_price,
                (pos.unrealized_pnl / pos.size) * 100.0
            );
        }
    }

    tracing::info!("└────────────────────────────────────────────────────┘");
    tracing::info!("");
}

fn format_duration(duration: chrono::Duration) -> String {
    let seconds = duration.num_seconds();
    if seconds < 60 {
        format!("{}s", seconds)
    } else if seconds < 3600 {
        format!("{}m", seconds / 60)
    } else {
        format!("{}h", seconds / 3600)
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
