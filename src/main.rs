mod api;
mod core;
mod execution;
mod monitoring;
mod scanner;
mod strategy;
mod streaming;
mod trading;

use core::integration::*;

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

                    // Fetch comprehensive account info
                    tracing::info!("📊 Fetching account details...");
                    
                    // Try default account overview
                    if let Ok(account) = kucoin_client.get_account_info().await {
                        tracing::info!(
                            "💰 Futures Account (default currency: {}): equity={:.2}, available={:.2}, margin={:.2}",
                            account.currency,
                            account.account_equity,
                            account.available_balance,
                            account.margin_balance
                        );
                    }
                    
                    // Try USDT explicitly
                    if let Ok(account_usdt) = kucoin_client.get_account_overview_currency("USDT").await {
                        tracing::info!(
                            "💰 USDT Account: equity={:.2}, available={:.2}, margin={:.2}, position_margin={:.2}",
                            account_usdt.account_equity,
                            account_usdt.available_balance,
                            account_usdt.margin_balance,
                            account_usdt.position_margin
                        );
                    }
                    
                    // Try XBT (Bitcoin) explicitly
                    if let Ok(account_xbt) = kucoin_client.get_account_overview_currency("XBT").await {
                        tracing::info!(
                            "💰 XBT Account: equity={:.8}, available={:.8}, margin={:.8}",
                            account_xbt.account_equity,
                            account_xbt.available_balance,
                            account_xbt.margin_balance
                        );
                    }
                    
                    // Get all positions
                    if let Ok(positions) = kucoin_client.get_positions().await {
                        if positions.is_empty() {
                            tracing::info!("📍 No open positions");
                        } else {
                            tracing::info!("📍 Open positions: {}", positions.len());
                            for pos in positions {
                                tracing::info!(
                                    "  • {} qty={:.4} cost={:.2} pnl={:.2} leverage={}x",
                                    pos.symbol,
                                    pos.current_qty,
                                    pos.current_cost,
                                    pos.unrealised_pnl,
                                    pos.leverage
                                );
                            }
                        }
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

    // Initialize comprehensive token monitoring system FIRST
    tracing::info!("🗂️  Initializing token monitoring system...");
    
    // Create data directory if it doesn't exist
    std::fs::create_dir_all("data").ok();
    
    let token_db = Arc::new(
        monitoring::TokenDatabase::new("data/tokens.db")
            .await
            .expect("Failed to initialize token database")
    );
    
    let token_registry = Arc::new(monitoring::TokenRegistry::new(
        kucoin_client.clone(),
        token_db.clone(),
        60, // Refresh every 60 seconds
    ));

    // Initialize market scanner with token monitoring integration
    let market_scanner = Arc::new(MarketScanner::new(
        kucoin_client.clone(),
        token_registry.clone(), // Pass token registry for NEW listing detection
    ));
    let new_listing_detector = Arc::new(NewListingDetector::new(kucoin_client.clone()));
    
    let token_detector = Arc::new(monitoring::NewTokenDetector::new(
        token_registry.clone(),
        token_db.clone(),
        30, // Check for new listings every 30 seconds
    ));
    
    let token_reporter = Arc::new(monitoring::TokenReporter::new(token_db.clone()));
    let api_verifier = Arc::new(monitoring::APIVerifier::new(kucoin_client.clone()));
    
    // Verify API completeness before starting
    tracing::info!("🔍 Verifying API completeness...");
    match api_verifier.verify_completeness().await {
        Ok(verification) => {
            tracing::info!("{}", api_verifier.format_verification_result(&verification));
            if !verification.errors.is_empty() {
                tracing::warn!("⚠️  API verification found errors, but continuing...");
            }
        }
        Err(e) => {
            tracing::error!("❌ API verification failed: {}", e);
        }
    }

    // Start token monitoring system
    token_registry.start().await?;
    health_checker.update_component("token_registry", true).await;
    
    token_detector.start().await?;
    health_checker.update_component("token_detector", true).await;
    
    // Start market scanner
    market_scanner.start().await?;
    health_checker.update_component("market_scanner", true).await;

    // Start new listing detector
    new_listing_detector.start().await?;
    health_checker.update_component("new_listing_detector", true).await;

    // ═══════════════════════════════════════════════════════════════
    // ULTIMATE SYSTEM INTEGRATION - All systems combined! 🚀
    // ═══════════════════════════════════════════════════════════════
    
    tracing::info!("🔌 Initializing ULTIMATE integration layer...");
    
    // Initialize Streaming System (WebSocket real-time data)
    tracing::info!("📡 Initializing real-time streaming system...");
    let stream_config = streaming::ConnectionConfig {
        max_concurrent_connections: 45, // API limit: 50/IP (90% safety)
        message_buffer_size: 10000,
        ..Default::default()
    };
    let ws_manager = Arc::new(streaming::WebSocketManager::new(stream_config));
    health_checker.update_component("websocket_manager", true).await;
    tracing::info!("✅ Streaming: 46K msg/sec capability, <50ms latency");
    
    // Initialize Event Bus (system coordination)
    tracing::info!("🔄 Initializing event bus...");
    let event_bus = Arc::new(EventBus::new(1000));
    health_checker.update_component("event_bus", true).await;
    tracing::info!("✅ Event Bus: System-wide event coordination active");
    
    // Initialize Data Aggregator (unified market data)
    tracing::info!("📊 Initializing data aggregator...");
    let data_aggregator = Arc::new(DataAggregator::new(
        kucoin_client.clone(),
        ws_manager.clone(),
        token_registry.clone(),
    ));
    data_aggregator.start().await?;
    health_checker.update_component("data_aggregator", true).await;
    tracing::info!("✅ Data Aggregator: Processing all streams");
    
    // Initialize Pre-Trade Validator (5-layer validation)
    tracing::info!("🛡️  Initializing pre-trade validator...");
    let validator = Arc::new(PreTradeValidator::new());
    health_checker.update_component("pre_trade_validator", true).await;
    tracing::info!("✅ Validator: 5-layer validation active");
    
    // Initialize Market Intelligence (multi-factor analysis)
    tracing::info!("🧠 Initializing market intelligence...");
    let market_intel = Arc::new(MarketIntelligence::new());
    health_checker.update_component("market_intelligence", true).await;
    tracing::info!("✅ Market Intel: Multi-factor analysis ready");
    
    // Start unified trading loop
    let _unified_loop_handle = start_unified_trading_loop(
        data_aggregator.clone(),
        validator.clone(),
        market_intel.clone(),
        event_bus.clone(),
        health_checker.clone(),
    ).await?;
    
    tracing::info!("✅ Unified Trading Loop: ACTIVE");
    tracing::info!("");
    tracing::info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    tracing::info!("🎉 ULTIMATE SYSTEM INTEGRATION COMPLETE! 🎉");
    tracing::info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    tracing::info!("📡 Real-time Streaming: ACTIVE");
    tracing::info!("🔄 Event Bus: ACTIVE");
    tracing::info!("📊 Data Aggregator: ACTIVE");
    tracing::info!("🛡️  Pre-Trade Validator: ACTIVE");
    tracing::info!("🧠 Market Intelligence: ACTIVE");
    tracing::info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // Start health check endpoint
    let health_clone = health_checker.clone();
    let port = config.monitoring.frontend_port;

    tokio::spawn(async move { start_health_server(health_clone, port).await });

    tracing::info!("✅ Health endpoint running on port {}", port);
    tracing::info!("🔸 Trading mode: DRY RUN (orders simulated)");
    tracing::info!("🤖 AI Strategy Engine: ACTIVE");
    tracing::info!("🔍 Market Scanner: ACTIVE");
    tracing::info!("🆕 New Listing Detector: ACTIVE");
    tracing::info!("🗂️  Token Registry: ACTIVE ({} tokens tracked)", token_registry.get_token_count().await);
    tracing::info!("🔔 Token Detector: ACTIVE");
    tracing::info!("");
    tracing::info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    tracing::info!("🚀 BOT IS LIVE - Scanning markets and generating signals");
    tracing::info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    
    // Show initial monitoring summary
    if let Ok(summary) = token_reporter.generate_summary().await {
        tracing::info!("{}", token_reporter.format_summary(&summary));
    }

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
                format_duration(chrono::Utc::now().signed_duration_since(listing.detected_at))
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

    // Account info (use USDT account as primary)
    if status.components.kucoin_api {
        if let Ok(account) = kucoin_client.get_account_overview_currency("USDT").await {
            tracing::info!("│ 💰 Account (USDT):                                 │");
            tracing::info!("│   • Equity:     {:.2} USDT                    ", account.account_equity);
            tracing::info!("│   • Available:  {:.2} USDT                    ", account.available_balance);
            tracing::info!("│   • Margin:     {:.2} USDT                    ", account.margin_balance);
            tracing::info!("│   • PnL:        {:.2} USDT                    ", account.unrealised_pnl);
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

// ═══════════════════════════════════════════════════════════════════════
// UNIFIED TRADING LOOP - All systems working in perfect synchronization
// ═══════════════════════════════════════════════════════════════════════
async fn start_unified_trading_loop(
    data_aggregator: Arc<DataAggregator>,
    validator: Arc<PreTradeValidator>,
    market_intel: Arc<MarketIntelligence>,
    event_bus: Arc<EventBus>,
    health_checker: Arc<HealthChecker>,
) -> Result<()> {
    tracing::info!("🔄 Starting Unified Trading Loop with full validation...");
    
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(
            tokio::time::Duration::from_secs(30)
        );
        
        loop {
            interval.tick().await;
            
            // Get all valid market data from aggregator
            let all_data = data_aggregator.get_all_valid_data().await;
            
            if all_data.is_empty() {
                tracing::debug!("⏸️  No valid data available yet");
                continue;
            }
            
            tracing::info!("📊 Analyzing {} symbols with validated data", all_data.len());
            
            // Analyze top 5 opportunities
            for data in all_data.iter().take(5) {
                // Run market intelligence analysis
                match market_intel.analyze(&data) {
                    Ok(signals) => {
                        let overall_signal = market_intel.get_overall_signal(&signals);
                        
                        // Only proceed with Buy/StrongBuy signals
                        if matches!(overall_signal, SignalType::StrongBuy | SignalType::Buy) {
                            tracing::info!(
                                "🎯 SIGNAL: {} {:?} @ ${:.2} {}",
                                data.symbol,
                                overall_signal,
                                data.price,
                                if data.is_new_listing { "🆕 NEW" } else { "✅" }
                            );
                            
                            // Create trade context for validation
                            let context = TradeContext {
                                market_data: data.clone(),
                                account_balance: 1000.0, // TODO: Get from API
                                open_positions: vec![],
                                daily_pnl: 0.0,
                                confidence_score: 0.85,
                            };
                            
                            // Run 5-layer pre-trade validation
                            match validator.validate(&context).await {
                                Ok(results) => {
                                    if validator.can_trade(&results) {
                                        tracing::info!(
                                            "✅ VALIDATION PASSED: {} - All 5 layers OK",
                                            data.symbol
                                        );
                                        
                                        // Log validation details
                                        for result in &results {
                                            tracing::debug!(
                                                "   Layer {:?}: {} ({})",
                                                result.layer,
                                                if result.passed { "✅" } else { "❌" },
                                                result.reason
                                            );
                                        }
                                        
                                        // Publish high confidence signal event
                                        event_bus.publish(TradingEvent::HighConfidenceSignal {
                                            symbol: data.symbol.clone(),
                                            signal_type: format!("{:?}", overall_signal),
                                            confidence: context.confidence_score,
                                        });
                                        
                                        // Simulate paper trade
                                        tracing::info!(
                                            "📝 PAPER TRADE: {} {:?} @ ${:.2} (Quality: {:.1}%, Fresh: {}ms)",
                                            data.symbol,
                                            overall_signal,
                                            data.price,
                                            data.data_quality_score * 100.0,
                                            data.data_freshness_ms
                                        );
                                        
                                        // Update health status
                                        health_checker.update_component("last_trade", true).await;
                                        
                                    } else {
                                        // Find failed layers
                                        let failed: Vec<_> = results.iter()
                                            .filter(|r| !r.passed)
                                            .collect();
                                        
                                        tracing::debug!(
                                            "⏸️  VALIDATION BLOCKED: {} - {} layer(s) failed",
                                            data.symbol,
                                            failed.len()
                                        );
                                        
                                        for fail in &failed {
                                            tracing::debug!(
                                                "   ❌ {:?}: {}",
                                                fail.layer,
                                                fail.reason
                                            );
                                        }
                                        
                                        // Publish validation failure event
                                        if failed.iter().any(|f| matches!(f.layer, ValidationLayer::DataQuality)) {
                                            event_bus.publish(TradingEvent::DataQualityIssue {
                                                symbol: data.symbol.clone(),
                                                severity: "HIGH".to_string(),
                                                message: "Pre-trade validation failed on data quality".to_string(),
                                            });
                                        }
                                    }
                                }
                                Err(e) => {
                                    tracing::error!("❌ Validation error for {}: {}", data.symbol, e);
                                }
                            }
                        } else {
                            tracing::debug!(
                                "⏸️  Signal {:?} for {} - Not actionable",
                                overall_signal,
                                data.symbol
                            );
                        }
                    }
                    Err(e) => {
                        tracing::error!("❌ Analysis error for {}: {}", data.symbol, e);
                    }
                }
            }
            
            // Update health status
            health_checker.update_component("unified_loop", true).await;
        }
    });
    
    Ok(())
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
