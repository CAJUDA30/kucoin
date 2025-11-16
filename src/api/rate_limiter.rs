// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// ⚠️  CRITICAL: KUCOIN API RATE LIMITING ENFORCEMENT
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
//
// ⚠️  WARNING: VIOLATING KUCOIN'S RATE LIMITS WILL RESULT IN:
//     - Immediate API request throttling
//     - Temporary account suspension
//     - Permanent API key revocation
//     - Possible account termination
//
// ⚠️  DO NOT MODIFY THE RATE LIMITS WITHOUT EXPLICIT AUTHORIZATION
// ⚠️  ALL LIMITS ARE SET 20% BELOW KUCOIN'S PUBLISHED MAXIMUMS FOR SAFETY
//
// Official Documentation:
// https://www.kucoin.com/docs/beginners/rate-limiting
//
// Last Verified: 2025-11-16
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

use anyhow::Result;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, Semaphore};
use std::collections::VecDeque;

/// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
/// KUCOIN OFFICIAL RATE LIMITS (DO NOT EXCEED)
/// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// KuCoin Futures API Rate Limits (VIP 0 - Default)
/// Source: https://www.kucoin.com/docs/beginners/rate-limiting-futures
const KUCOIN_FUTURES_POOL_LIMIT: u32 = 1000;  // Official limit per 30s
const KUCOIN_WINDOW_SECONDS: u64 = 30;        // Rolling 30-second window

/// Safety margin: We enforce 80% of official limits to prevent accidental violations
const SAFETY_MARGIN: f32 = 0.80;

/// Maximum concurrent requests (to prevent burst violations)
const MAX_CONCURRENT_REQUESTS: usize = 20;  // Down from 50 to be safe

/// Endpoint weights (as documented by KuCoin)
const WEIGHT_TICKER: u32 = 2;
const WEIGHT_SYMBOLS: u32 = 2;
const WEIGHT_ORDER_BOOK: u32 = 2;
const WEIGHT_ACCOUNT_INFO: u32 = 5;
const WEIGHT_POSITIONS: u32 = 5;
const WEIGHT_ORDERS: u32 = 5;
const WEIGHT_PLACE_ORDER: u32 = 5;
const WEIGHT_CANCEL_ORDER: u32 = 2;
const WEIGHT_TRADE_HISTORY: u32 = 10;

/// Circuit breaker thresholds
const CIRCUIT_BREAKER_THRESHOLD: f32 = 0.90;  // Pause at 90% capacity
const CIRCUIT_BREAKER_COOLDOWN_MS: u64 = 5000; // 5 second cooldown

/// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
/// Rate Limiter Implementation
/// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    pub max_weight_per_window: u32,
    pub window_duration: Duration,
    pub max_concurrent: usize,
    pub circuit_breaker_threshold: f32,
    pub circuit_breaker_cooldown: Duration,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        // Apply safety margin to official limits
        let safe_limit = (KUCOIN_FUTURES_POOL_LIMIT as f32 * SAFETY_MARGIN) as u32;
        
        Self {
            max_weight_per_window: safe_limit,  // 800 (80% of 1000)
            window_duration: Duration::from_secs(KUCOIN_WINDOW_SECONDS),
            max_concurrent: MAX_CONCURRENT_REQUESTS,
            circuit_breaker_threshold: CIRCUIT_BREAKER_THRESHOLD,
            circuit_breaker_cooldown: Duration::from_millis(CIRCUIT_BREAKER_COOLDOWN_MS),
        }
    }
}

#[derive(Debug)]
struct RequestLog {
    timestamp: Instant,
    weight: u32,
    endpoint: String,
}

pub struct KuCoinRateLimiter {
    config: RateLimitConfig,
    request_history: Arc<RwLock<VecDeque<RequestLog>>>,
    semaphore: Arc<Semaphore>,
    circuit_breaker_until: Arc<RwLock<Option<Instant>>>,
    total_requests: Arc<RwLock<u64>>,
    total_weight_used: Arc<RwLock<u64>>,
    rate_limit_violations: Arc<RwLock<u64>>,
}

impl KuCoinRateLimiter {
    pub fn new(config: RateLimitConfig) -> Self {
        tracing::info!(
            "🛡️  RATE LIMITER INITIALIZED - KUCOIN API COMPLIANCE MODE"
        );
        tracing::info!(
            "   • Max weight per 30s: {} ({}% of official limit {})",
            config.max_weight_per_window,
            (SAFETY_MARGIN * 100.0) as u32,
            KUCOIN_FUTURES_POOL_LIMIT
        );
        tracing::info!(
            "   • Max concurrent requests: {}",
            config.max_concurrent
        );
        tracing::info!(
            "   • Circuit breaker: {}% capacity",
            (config.circuit_breaker_threshold * 100.0) as u32
        );
        tracing::warn!(
            "⚠️  WARNING: Exceeding these limits may result in account suspension!"
        );
        
        Self {
            semaphore: Arc::new(Semaphore::new(config.max_concurrent)),
            config,
            request_history: Arc::new(RwLock::new(VecDeque::new())),
            circuit_breaker_until: Arc::new(RwLock::new(None)),
            total_requests: Arc::new(RwLock::new(0)),
            total_weight_used: Arc::new(RwLock::new(0)),
            rate_limit_violations: Arc::new(RwLock::new(0)),
        }
    }

    /// Acquire permission to make an API request
    /// This enforces rate limits and circuit breaker logic
    pub async fn acquire(&self, endpoint: &str, weight: u32) -> Result<RateLimitGuard> {
        // Check circuit breaker first
        {
            let breaker = self.circuit_breaker_until.read().await;
            if let Some(until) = *breaker {
                if Instant::now() < until {
                    let remaining = until.duration_since(Instant::now());
                    tracing::warn!(
                        "🔴 CIRCUIT BREAKER ACTIVE - Cooling down for {:?}",
                        remaining
                    );
                    tokio::time::sleep(remaining).await;
                }
            }
        }

        // Acquire semaphore permit (limits concurrent requests)
        let permit = self.semaphore.clone().acquire_owned().await?;

        // Clean old requests outside the window
        self.clean_old_requests().await;

        // Check if we can make this request
        let current_weight = self.get_current_window_weight().await;
        let projected_weight = current_weight + weight;

        if projected_weight > self.config.max_weight_per_window {
            // Would exceed limit - wait for window to reset
            let wait_time = self.calculate_wait_time().await;
            
            tracing::warn!(
                "⚠️  RATE LIMIT APPROACHING - Current: {}/{}, Requested: {}, Waiting: {:?}",
                current_weight,
                self.config.max_weight_per_window,
                weight,
                wait_time
            );
            
            // Increment violation counter
            *self.rate_limit_violations.write().await += 1;
            
            tokio::time::sleep(wait_time).await;
            
            // Recursively try again after waiting (using Box::pin for async recursion)
            return Box::pin(self.acquire(endpoint, weight)).await;
        }

        // Check circuit breaker threshold
        let usage_percent = projected_weight as f32 / self.config.max_weight_per_window as f32;
        if usage_percent >= self.config.circuit_breaker_threshold {
            tracing::error!(
                "🔴 CIRCUIT BREAKER TRIGGERED - Usage at {:.1}%! Forcing cooldown...",
                usage_percent * 100.0
            );
            
            let mut breaker = self.circuit_breaker_until.write().await;
            *breaker = Some(Instant::now() + self.config.circuit_breaker_cooldown);
            
            tokio::time::sleep(self.config.circuit_breaker_cooldown).await;
        }

        // Log the request
        {
            let mut history = self.request_history.write().await;
            history.push_back(RequestLog {
                timestamp: Instant::now(),
                weight,
                endpoint: endpoint.to_string(),
            });
        }

        // Update metrics
        *self.total_requests.write().await += 1;
        *self.total_weight_used.write().await += weight as u64;

        tracing::debug!(
            "✅ Rate limit OK: {} (weight: {}, usage: {}/{})",
            endpoint,
            weight,
            projected_weight,
            self.config.max_weight_per_window
        );

        Ok(RateLimitGuard { _permit: permit })
    }

    /// Clean requests older than the window duration
    async fn clean_old_requests(&self) {
        let mut history = self.request_history.write().await;
        let cutoff = Instant::now() - self.config.window_duration;
        
        while let Some(req) = history.front() {
            if req.timestamp < cutoff {
                history.pop_front();
            } else {
                break;
            }
        }
    }

    /// Get total weight used in current window
    async fn get_current_window_weight(&self) -> u32 {
        let history = self.request_history.read().await;
        history.iter().map(|r| r.weight).sum()
    }

    /// Calculate how long to wait before making another request
    async fn calculate_wait_time(&self) -> Duration {
        let history = self.request_history.read().await;
        
        if let Some(oldest) = history.front() {
            let elapsed = Instant::now().duration_since(oldest.timestamp);
            if elapsed < self.config.window_duration {
                return self.config.window_duration - elapsed + Duration::from_millis(100);
            }
        }
        
        Duration::from_millis(100) // Minimum wait
    }

    /// Get current rate limiter statistics
    pub async fn get_stats(&self) -> RateLimiterStats {
        let current_weight = self.get_current_window_weight().await;
        let history = self.request_history.read().await;
        
        RateLimiterStats {
            current_weight,
            max_weight: self.config.max_weight_per_window,
            usage_percent: (current_weight as f32 / self.config.max_weight_per_window as f32) * 100.0,
            requests_in_window: history.len(),
            total_requests: *self.total_requests.read().await,
            total_weight_used: *self.total_weight_used.read().await,
            violations: *self.rate_limit_violations.read().await,
        }
    }

    /// Get endpoint-specific weight
    pub fn get_endpoint_weight(endpoint: &str) -> u32 {
        if endpoint.contains("/ticker") {
            WEIGHT_TICKER
        } else if endpoint.contains("/contracts") {
            WEIGHT_SYMBOLS
        } else if endpoint.contains("/level2") {
            WEIGHT_ORDER_BOOK
        } else if endpoint.contains("/account-overview") {
            WEIGHT_ACCOUNT_INFO
        } else if endpoint.contains("/positions") {
            WEIGHT_POSITIONS
        } else if endpoint.contains("/orders") && endpoint.contains("POST") {
            WEIGHT_PLACE_ORDER
        } else if endpoint.contains("/orders") {
            WEIGHT_ORDERS
        } else if endpoint.contains("/cancel") {
            WEIGHT_CANCEL_ORDER
        } else if endpoint.contains("/trade/history") {
            WEIGHT_TRADE_HISTORY
        } else {
            5 // Default weight for unknown endpoints (safe)
        }
    }
}

/// RAII guard that releases the semaphore permit when dropped
pub struct RateLimitGuard {
    _permit: tokio::sync::OwnedSemaphorePermit,
}

#[derive(Debug, Clone)]
pub struct RateLimiterStats {
    pub current_weight: u32,
    pub max_weight: u32,
    pub usage_percent: f32,
    pub requests_in_window: usize,
    pub total_requests: u64,
    pub total_weight_used: u64,
    pub violations: u64,
}

impl RateLimiterStats {
    pub fn is_healthy(&self) -> bool {
        self.usage_percent < 80.0 && self.violations == 0
    }

    pub fn format_status(&self) -> String {
        let status_icon = if self.usage_percent < 70.0 {
            "🟢"
        } else if self.usage_percent < 85.0 {
            "🟡"
        } else {
            "🔴"
        };

        format!(
            "{} Rate Limit: {}/{} ({:.1}%) | Requests: {} | Violations: {}",
            status_icon,
            self.current_weight,
            self.max_weight,
            self.usage_percent,
            self.requests_in_window,
            self.violations
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rate_limiter_basic() {
        let limiter = KuCoinRateLimiter::new(RateLimitConfig::default());
        
        let _guard = limiter.acquire("/api/v1/ticker", WEIGHT_TICKER).await.unwrap();
        
        let stats = limiter.get_stats().await;
        assert_eq!(stats.current_weight, WEIGHT_TICKER);
        assert!(stats.is_healthy());
    }

    #[tokio::test]
    async fn test_rate_limiter_exceeds_limit() {
        let config = RateLimitConfig {
            max_weight_per_window: 10,
            ..Default::default()
        };
        let limiter = KuCoinRateLimiter::new(config);
        
        // First request should succeed
        let _guard1 = limiter.acquire("/api/v1/ticker", 5).await.unwrap();
        
        // Second request should wait (would exceed limit)
        let start = Instant::now();
        let _guard2 = limiter.acquire("/api/v1/ticker", 10).await.unwrap();
        let elapsed = start.elapsed();
        
        // Should have waited
        assert!(elapsed.as_millis() > 0);
    }
}

