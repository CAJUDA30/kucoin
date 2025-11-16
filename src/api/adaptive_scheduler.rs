// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// ADAPTIVE REQUEST SCHEDULER - MAXIMIZE UPTIME WITHIN LIMITS
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
//
// This scheduler ensures the bot operates at maximum efficiency while staying
// within KuCoin's 30-second rolling window limits by:
// 1. Spreading operations evenly across time windows
// 2. Dynamic throttling based on usage
// 3. Precise timing with nanosecond accuracy
// 4. Automatic cooldown management
//
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

use anyhow::Result;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use std::collections::VecDeque;

/// Precise timing constants
const LIMIT_WINDOW_SECONDS: u64 = 30;          // KuCoin's 30-second window
const RESET_BUFFER_MS: u64 = 1000;              // 1-second safety buffer
const TOTAL_RESET_DURATION_MS: u64 = (LIMIT_WINDOW_SECONDS * 1000) + RESET_BUFFER_MS;
const HEARTBEAT_INTERVAL_MS: u64 = 500;        // Monitor every 500ms
const OPERATION_SPREAD_FACTOR: f64 = 0.85;     // Use 85% of window for operations
const THROTTLE_THRESHOLD: f64 = 0.75;          // Start throttling at 75%
const EMERGENCY_THROTTLE: f64 = 0.85;          // Heavy throttling at 85%
const JITTER_MAX_MS: u64 = 200;                // Random jitter up to 200ms

/// Operation timing record
#[derive(Debug, Clone)]
struct OperationRecord {
    timestamp: Instant,
    weight: u32,
    operation_type: String,
    elapsed_since_window_start_ns: u128,
}

/// Cooldown state
#[derive(Debug, Clone, Copy, PartialEq)]
enum CooldownState {
    Active,           // Normal operations
    Throttled,        // Reduced operations (75-85% usage)
    HeavyThrottle,    // Minimal operations (85-90% usage)
    Cooldown,         // Full cooldown triggered
    Resetting,        // Reset in progress
}

impl CooldownState {
    fn as_str(&self) -> &str {
        match self {
            CooldownState::Active => "ACTIVE",
            CooldownState::Throttled => "THROTTLED",
            CooldownState::HeavyThrottle => "HEAVY_THROTTLE",
            CooldownState::Cooldown => "COOLDOWN",
            CooldownState::Resetting => "RESETTING",
        }
    }
    
    fn emoji(&self) -> &str {
        match self {
            CooldownState::Active => "🟢",
            CooldownState::Throttled => "🟡",
            CooldownState::HeavyThrottle => "🟠",
            CooldownState::Cooldown => "🔴",
            CooldownState::Resetting => "🔵",
        }
    }
}

/// Comprehensive scheduler state
#[derive(Debug)]
struct SchedulerState {
    state: CooldownState,
    window_start: Instant,
    cooldown_triggered_at: Option<Instant>,
    last_reset: Option<Instant>,
    operations: VecDeque<OperationRecord>,
    total_weight_used: u32,
    max_weight: u32,
    operations_count: u64,
    cooldowns_triggered: u64,
    successful_resets: u64,
    failed_resets: u64,
}

/// Adaptive Request Scheduler
pub struct AdaptiveScheduler {
    state: Arc<RwLock<SchedulerState>>,
    heartbeat_handle: Option<tokio::task::JoinHandle<()>>,
}

impl AdaptiveScheduler {
    pub fn new(max_weight: u32) -> Self {
        let state = Arc::new(RwLock::new(SchedulerState {
            state: CooldownState::Active,
            window_start: Instant::now(),
            cooldown_triggered_at: None,
            last_reset: None,
            operations: VecDeque::new(),
            total_weight_used: 0,
            max_weight,
            operations_count: 0,
            cooldowns_triggered: 0,
            successful_resets: 0,
            failed_resets: 0,
        }));

        tracing::info!(
            "🎯 ADAPTIVE SCHEDULER INITIALIZED"
        );
        tracing::info!(
            "   • Window: {}s | Buffer: {}ms | Total Reset: {}ms",
            LIMIT_WINDOW_SECONDS,
            RESET_BUFFER_MS,
            TOTAL_RESET_DURATION_MS
        );
        tracing::info!(
            "   • Heartbeat: {}ms | Jitter: {}ms max",
            HEARTBEAT_INTERVAL_MS,
            JITTER_MAX_MS
        );

        Self {
            state,
            heartbeat_handle: None,
        }
    }

    /// Start the heartbeat monitor
    pub async fn start_heartbeat(&mut self) {
        let state = self.state.clone();
        
        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(
                Duration::from_millis(HEARTBEAT_INTERVAL_MS)
            );
            
            loop {
                interval.tick().await;
                
                if let Err(e) = Self::heartbeat_check(state.clone()).await {
                    tracing::error!("❌ Heartbeat check failed: {}", e);
                }
            }
        });
        
        self.heartbeat_handle = Some(handle);
        tracing::info!("💓 Heartbeat monitor started");
    }

    /// Heartbeat check - monitors state and triggers resets
    async fn heartbeat_check(state: Arc<RwLock<SchedulerState>>) -> Result<()> {
        let mut s = state.write().await;
        
        // Clean old operations outside the window
        let window_cutoff = Instant::now() - Duration::from_secs(LIMIT_WINDOW_SECONDS);
        while let Some(op) = s.operations.front() {
            if op.timestamp < window_cutoff {
                s.total_weight_used = s.total_weight_used.saturating_sub(op.weight);
                s.operations.pop_front();
            } else {
                break;
            }
        }
        
        // Calculate current usage
        let usage = s.total_weight_used as f64 / s.max_weight as f64;
        
        // Handle cooldown reset
        if s.state == CooldownState::Cooldown {
            if let Some(cooldown_start) = s.cooldown_triggered_at {
                let elapsed = Instant::now().duration_since(cooldown_start);
                let remaining = Duration::from_millis(TOTAL_RESET_DURATION_MS)
                    .saturating_sub(elapsed);
                
                if remaining.as_millis() == 0 {
                    // Time to reset!
                    tracing::info!(
                        "🔵 RESET INITIATING - {}ms elapsed since cooldown",
                        elapsed.as_millis()
                    );
                    
                    s.state = CooldownState::Resetting;
                    
                    // Validate state before reset
                    if Self::validate_pre_reset(&s) {
                        // Perform reset
                        s.window_start = Instant::now();
                        s.cooldown_triggered_at = None;
                        s.operations.clear();
                        s.total_weight_used = 0;
                        s.state = CooldownState::Active;
                        s.successful_resets += 1;
                        s.last_reset = Some(Instant::now());
                        
                        tracing::info!(
                            "✅ RESET SUCCESSFUL - Window restarted | Total resets: {}",
                            s.successful_resets
                        );
                        
                        // Log state snapshot
                        Self::log_state_snapshot("POST_RESET", &s);
                    } else {
                        s.failed_resets += 1;
                        tracing::error!(
                            "❌ RESET VALIDATION FAILED - Attempting recovery | Failures: {}",
                            s.failed_resets
                        );
                        
                        // Fallback: Force reset anyway but log warning
                        s.window_start = Instant::now();
                        s.cooldown_triggered_at = None;
                        s.operations.clear();
                        s.total_weight_used = 0;
                        s.state = CooldownState::Active;
                    }
                } else if remaining.as_secs() % 5 == 0 && remaining.as_millis() % 1000 < HEARTBEAT_INTERVAL_MS as u128 {
                    // Log countdown every 5 seconds
                    tracing::debug!(
                        "⏳ Cooldown: {:.1}s remaining",
                        remaining.as_secs_f64()
                    );
                }
            }
        } else {
            // Update state based on usage
            let new_state = if usage >= 0.90 {
                // Emergency: trigger cooldown
                if s.state != CooldownState::Cooldown {
                    s.cooldown_triggered_at = Some(Instant::now());
                    s.cooldowns_triggered += 1;
                    
                    tracing::warn!(
                        "🔴 COOLDOWN TRIGGERED - Usage: {:.1}% | Count: {}",
                        usage * 100.0,
                        s.cooldowns_triggered
                    );
                    
                    Self::log_state_snapshot("COOLDOWN_TRIGGER", &s);
                }
                CooldownState::Cooldown
            } else if usage >= EMERGENCY_THROTTLE {
                if s.state != CooldownState::HeavyThrottle {
                    tracing::warn!(
                        "🟠 HEAVY THROTTLE ACTIVE - Usage: {:.1}%",
                        usage * 100.0
                    );
                }
                CooldownState::HeavyThrottle
            } else if usage >= THROTTLE_THRESHOLD {
                if s.state != CooldownState::Throttled {
                    tracing::info!(
                        "🟡 THROTTLE ACTIVE - Usage: {:.1}%",
                        usage * 100.0
                    );
                }
                CooldownState::Throttled
            } else {
                CooldownState::Active
            };
            
            s.state = new_state;
        }
        
        Ok(())
    }

    /// Validate system state before reset
    fn validate_pre_reset(state: &SchedulerState) -> bool {
        // Check 1: Cooldown was actually triggered
        if state.cooldown_triggered_at.is_none() {
            tracing::error!("❌ Validation: No cooldown timestamp");
            return false;
        }
        
        // Check 2: Enough time has passed
        let elapsed = Instant::now()
            .duration_since(state.cooldown_triggered_at.unwrap())
            .as_millis();
        
        if elapsed < TOTAL_RESET_DURATION_MS as u128 {
            tracing::error!(
                "❌ Validation: Insufficient time - {}ms < {}ms",
                elapsed,
                TOTAL_RESET_DURATION_MS
            );
            return false;
        }
        
        // Check 3: State is correct
        if state.state != CooldownState::Resetting {
            tracing::error!(
                "❌ Validation: Wrong state - {:?}",
                state.state
            );
            return false;
        }
        
        tracing::debug!("✅ Pre-reset validation passed");
        true
    }

    /// Log detailed state snapshot
    fn log_state_snapshot(event: &str, state: &SchedulerState) {
        let window_age = Instant::now()
            .duration_since(state.window_start)
            .as_millis();
        
        let cooldown_age = state.cooldown_triggered_at
            .map(|t| Instant::now().duration_since(t).as_millis())
            .unwrap_or(0);
        
        tracing::info!(
            "📸 STATE SNAPSHOT [{}]",
            event
        );
        tracing::info!(
            "   State: {} {} | Window age: {}ms | Cooldown age: {}ms",
            state.state.emoji(),
            state.state.as_str(),
            window_age,
            cooldown_age
        );
        tracing::info!(
            "   Weight: {}/{} ({:.1}%) | Operations: {}",
            state.total_weight_used,
            state.max_weight,
            (state.total_weight_used as f64 / state.max_weight as f64) * 100.0,
            state.operations.len()
        );
        tracing::info!(
            "   Lifetime: ops={}, cooldowns={}, resets={}/{} success",
            state.operations_count,
            state.cooldowns_triggered,
            state.successful_resets,
            state.successful_resets + state.failed_resets
        );
    }

    /// Check if operation can proceed
    pub async fn can_proceed(&self, weight: u32) -> (bool, Duration) {
        let state = self.state.read().await;
        
        match state.state {
            CooldownState::Cooldown | CooldownState::Resetting => {
                // Calculate wait time
                if let Some(cooldown_start) = state.cooldown_triggered_at {
                    let elapsed = Instant::now().duration_since(cooldown_start);
                    let wait = Duration::from_millis(TOTAL_RESET_DURATION_MS)
                        .saturating_sub(elapsed);
                    (false, wait)
                } else {
                    (false, Duration::from_secs(31))
                }
            }
            CooldownState::HeavyThrottle => {
                // Only allow if well under limit
                let projected = state.total_weight_used + weight;
                if (projected as f64 / state.max_weight as f64) < 0.88 {
                    // Add jitter delay
                    let jitter = Self::calculate_jitter(400, 800);
                    (true, Duration::from_millis(jitter))
                } else {
                    (false, Duration::from_millis(1000))
                }
            }
            CooldownState::Throttled => {
                // Add moderate delay with jitter
                let jitter = Self::calculate_jitter(100, 300);
                (true, Duration::from_millis(jitter))
            }
            CooldownState::Active => {
                // Check if operation would push us over
                let usage = state.total_weight_used as f64 / state.max_weight as f64;
                if usage > OPERATION_SPREAD_FACTOR {
                    // Start backing off
                    let jitter = Self::calculate_jitter(50, 150);
                    (true, Duration::from_millis(jitter))
                } else {
                    // Minimal jitter for pattern prevention
                    let jitter = Self::calculate_jitter(10, 50);
                    (true, Duration::from_millis(jitter))
                }
            }
        }
    }

    /// Record an operation
    pub async fn record_operation(&self, weight: u32, operation_type: String) {
        let mut state = self.state.write().await;
        
        let record = OperationRecord {
            timestamp: Instant::now(),
            weight,
            operation_type: operation_type.clone(),
            elapsed_since_window_start_ns: Instant::now()
                .duration_since(state.window_start)
                .as_nanos(),
        };
        
        state.operations.push_back(record);
        state.total_weight_used += weight;
        state.operations_count += 1;
        
        tracing::trace!(
            "📝 Operation recorded: {} (weight: {}, total: {}/{})",
            operation_type,
            weight,
            state.total_weight_used,
            state.max_weight
        );
    }

    /// Calculate jitter to prevent pattern detection
    fn calculate_jitter(min_ms: u64, max_ms: u64) -> u64 {
        use std::collections::hash_map::RandomState;
        use std::hash::{BuildHasher, Hash, Hasher};
        
        let s = RandomState::new();
        let mut hasher = s.build_hasher();
        Instant::now().hash(&mut hasher);
        let hash = hasher.finish();
        
        min_ms + (hash % (max_ms - min_ms + 1))
    }

    /// Get current statistics
    pub async fn get_stats(&self) -> SchedulerStats {
        let state = self.state.read().await;
        
        let usage = state.total_weight_used as f64 / state.max_weight as f64;
        let window_age = Instant::now()
            .duration_since(state.window_start)
            .as_millis();
        
        let cooldown_remaining = if let Some(cooldown_start) = state.cooldown_triggered_at {
            let elapsed = Instant::now().duration_since(cooldown_start);
            Duration::from_millis(TOTAL_RESET_DURATION_MS)
                .saturating_sub(elapsed)
        } else {
            Duration::ZERO
        };
        
        SchedulerStats {
            state: state.state,
            usage_percent: usage * 100.0,
            weight_used: state.total_weight_used,
            max_weight: state.max_weight,
            operations_in_window: state.operations.len(),
            window_age_ms: window_age as u64,
            cooldown_remaining_ms: cooldown_remaining.as_millis() as u64,
            lifetime_operations: state.operations_count,
            lifetime_cooldowns: state.cooldowns_triggered,
            lifetime_resets: state.successful_resets,
            failed_resets: state.failed_resets,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SchedulerStats {
    pub state: CooldownState,
    pub usage_percent: f64,
    pub weight_used: u32,
    pub max_weight: u32,
    pub operations_in_window: usize,
    pub window_age_ms: u64,
    pub cooldown_remaining_ms: u64,
    pub lifetime_operations: u64,
    pub lifetime_cooldowns: u64,
    pub lifetime_resets: u64,
    pub failed_resets: u64,
}

impl SchedulerStats {
    pub fn format_status(&self) -> String {
        format!(
            "{} {} | Usage: {:.1}% ({}/{}) | Window: {:.1}s | Ops: {} | Cooldowns: {} | Resets: {}/{}",
            self.state.emoji(),
            self.state.as_str(),
            self.usage_percent,
            self.weight_used,
            self.max_weight,
            self.window_age_ms as f64 / 1000.0,
            self.operations_in_window,
            self.lifetime_cooldowns,
            self.lifetime_resets,
            self.lifetime_resets + self.failed_resets
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_scheduler_initialization() {
        let scheduler = AdaptiveScheduler::new(800);
        let stats = scheduler.get_stats().await;
        
        assert_eq!(stats.state, CooldownState::Active);
        assert_eq!(stats.weight_used, 0);
        assert_eq!(stats.max_weight, 800);
    }

    #[tokio::test]
    async fn test_operation_recording() {
        let scheduler = AdaptiveScheduler::new(800);
        
        scheduler.record_operation(10, "test_op".to_string()).await;
        
        let stats = scheduler.get_stats().await;
        assert_eq!(stats.weight_used, 10);
        assert_eq!(stats.operations_in_window, 1);
    }
}

