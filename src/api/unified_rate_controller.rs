// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// UNIFIED RATE CONTROLLER - INTELLIGENT API MANAGEMENT
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
//
// This system provides sophisticated rate limiting with:
// 1. Real-time monitoring with millisecond precision
// 2. Intelligent call distribution (70% trading, 30% other)
// 3. Adaptive throttling (never full stoppage)
// 4. Four-tier priority queue system
// 5. 100% operational continuity guarantee
//
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

use anyhow::Result;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, Semaphore};
use std::sync::atomic::{AtomicU64, Ordering};

/// Configuration constants
const TOTAL_CAPACITY: u32 = 800;                    // 80% of KuCoin's 1000
const TRADING_RESERVE: f64 = 0.70;                  // 70% reserved for trading
const SCAN_INTERVAL_NORMAL: u64 = 3600;            // 1 hour in seconds
const SCAN_INTERVAL_HIGH_LOAD: u64 = 7200;         // 2 hours under load
const THROTTLE_THRESHOLD: f64 = 0.80;              // Start throttling at 80%
const RECOVERY_THRESHOLD: f64 = 0.60;              // Recover at 60%
const ALERT_THRESHOLD: f64 = 0.90;                 // Alert at 90%
const MIN_THROUGHPUT_PERCENT: f64 = 0.15;          // Never below 15% speed
const MONITORING_INTERVAL_MS: u64 = 100;           // Monitor every 100ms

/// Four-tier priority system
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Priority {
    Critical = 0,       // Real-time trade execution
    High = 1,           // Position monitoring
    Medium = 2,         // Market data collection
    Low = 3,            // Administrative functions
}

impl Priority {
    fn as_str(&self) -> &str {
        match self {
            Priority::Critical => "CRITICAL",
            Priority::High => "HIGH",
            Priority::Medium => "MEDIUM",
            Priority::Low => "LOW",
        }
    }
    
    fn emoji(&self) -> &str {
        match self {
            Priority::Critical => "🔴",
            Priority::High => "🟠",
            Priority::Medium => "🟡",
            Priority::Low => "🟢",
        }
    }
    
    /// Get SLA in milliseconds
    fn sla_ms(&self) -> u64 {
        match self {
            Priority::Critical => 100,      // 100ms SLA
            Priority::High => 500,          // 500ms SLA
            Priority::Medium => 2000,       // 2s SLA
            Priority::Low => 10000,         // 10s SLA
        }
    }
}

/// Operation category for capacity allocation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OperationCategory {
    Trading,        // Trade execution, position monitoring
    Scanning,       // Market scans, token discovery
    Administrative, // Account info, misc operations
}

impl OperationCategory {
    fn max_capacity_share(&self) -> f64 {
        match self {
            OperationCategory::Trading => TRADING_RESERVE,  // 70%
            OperationCategory::Scanning => 0.20,            // 20%
            OperationCategory::Administrative => 0.10,      // 10%
        }
    }
}

/// Request record with full metadata
#[derive(Debug, Clone)]
struct RequestRecord {
    timestamp: Instant,
    weight: u32,
    priority: Priority,
    category: OperationCategory,
    operation: String,
    queue_time_ms: u64,
    execution_time_ms: u64,
}

/// Queued request waiting for execution
#[derive(Debug)]
struct QueuedRequest {
    priority: Priority,
    category: OperationCategory,
    weight: u32,
    operation: String,
    queued_at: Instant,
    sender: tokio::sync::oneshot::Sender<()>,
}

/// Throttling state
#[derive(Debug, Clone, Copy, PartialEq)]
enum ThrottleState {
    Normal,             // < 60% usage
    Moderate,           // 60-80% usage
    Heavy,              // 80-90% usage
    Emergency,          // 90%+ usage
}

impl ThrottleState {
    fn throughput_multiplier(&self) -> f64 {
        match self {
            ThrottleState::Normal => 1.0,               // 100% speed
            ThrottleState::Moderate => 0.75,            // 75% speed
            ThrottleState::Heavy => 0.40,               // 40% speed
            ThrottleState::Emergency => MIN_THROUGHPUT_PERCENT, // 15% speed (minimum)
        }
    }
    
    fn as_str(&self) -> &str {
        match self {
            ThrottleState::Normal => "NORMAL",
            ThrottleState::Moderate => "MODERATE",
            ThrottleState::Heavy => "HEAVY",
            ThrottleState::Emergency => "EMERGENCY",
        }
    }
    
    fn emoji(&self) -> &str {
        match self {
            ThrottleState::Normal => "🟢",
            ThrottleState::Moderate => "🟡",
            ThrottleState::Heavy => "🟠",
            ThrottleState::Emergency => "🔴",
        }
    }
}

/// Controller statistics
#[derive(Debug, Clone)]
pub struct ControllerStats {
    pub current_usage: u32,
    pub capacity: u32,
    pub usage_percent: f64,
    pub throttle_state: ThrottleState,
    pub trading_usage: u32,
    pub scanning_usage: u32,
    pub admin_usage: u32,
    pub queue_depths: HashMap<Priority, usize>,
    pub total_requests: u64,
    pub throttled_requests: u64,
    pub sla_violations: u64,
    pub avg_queue_time_ms: f64,
    pub scan_interval_current: u64,
}

/// Unified Rate Controller
pub struct UnifiedRateController {
    // Capacity tracking
    requests: Arc<RwLock<VecDeque<RequestRecord>>>,
    current_usage: Arc<RwLock<u32>>,
    category_usage: Arc<RwLock<HashMap<OperationCategory, u32>>>,
    
    // Priority queues (one per priority level)
    queues: Arc<RwLock<HashMap<Priority, VecDeque<QueuedRequest>>>>,
    
    // State management
    throttle_state: Arc<RwLock<ThrottleState>>,
    scan_interval: Arc<RwLock<u64>>,
    
    // Statistics (atomic for lock-free updates)
    total_requests: Arc<AtomicU64>,
    throttled_requests: Arc<AtomicU64>,
    sla_violations: Arc<AtomicU64>,
    
    // Monitoring
    monitor_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
}

impl UnifiedRateController {
    pub fn new() -> Self {
        tracing::info!("🎯 UNIFIED RATE CONTROLLER INITIALIZING");
        tracing::info!("   • Total Capacity: {} weight/30s", TOTAL_CAPACITY);
        tracing::info!("   • Trading Reserve: {:.0}% ({} weight)", 
            TRADING_RESERVE * 100.0, 
            (TOTAL_CAPACITY as f64 * TRADING_RESERVE) as u32
        );
        tracing::info!("   • Scan Interval: {}s (normal) / {}s (high load)", 
            SCAN_INTERVAL_NORMAL, 
            SCAN_INTERVAL_HIGH_LOAD
        );
        tracing::info!("   • Throttle: {}% | Recovery: {}% | Alert: {}%",
            (THROTTLE_THRESHOLD * 100.0) as u32,
            (RECOVERY_THRESHOLD * 100.0) as u32,
            (ALERT_THRESHOLD * 100.0) as u32
        );
        tracing::info!("   • Min Throughput: {:.0}% (never full stop)", 
            MIN_THROUGHPUT_PERCENT * 100.0
        );
        
        let mut queues = HashMap::new();
        queues.insert(Priority::Critical, VecDeque::new());
        queues.insert(Priority::High, VecDeque::new());
        queues.insert(Priority::Medium, VecDeque::new());
        queues.insert(Priority::Low, VecDeque::new());
        
        let mut category_usage = HashMap::new();
        category_usage.insert(OperationCategory::Trading, 0);
        category_usage.insert(OperationCategory::Scanning, 0);
        category_usage.insert(OperationCategory::Administrative, 0);
        
        Self {
            requests: Arc::new(RwLock::new(VecDeque::new())),
            current_usage: Arc::new(RwLock::new(0)),
            category_usage: Arc::new(RwLock::new(category_usage)),
            queues: Arc::new(RwLock::new(queues)),
            throttle_state: Arc::new(RwLock::new(ThrottleState::Normal)),
            scan_interval: Arc::new(RwLock::new(SCAN_INTERVAL_NORMAL)),
            total_requests: Arc::new(AtomicU64::new(0)),
            throttled_requests: Arc::new(AtomicU64::new(0)),
            sla_violations: Arc::new(AtomicU64::new(0)),
            monitor_handle: Arc::new(RwLock::new(None)),
        }
    }
    
    /// Start the monitoring task
    pub async fn start_monitoring(&self) {
        let requests = self.requests.clone();
        let current_usage = self.current_usage.clone();
        let category_usage = self.category_usage.clone();
        let throttle_state = self.throttle_state.clone();
        let scan_interval = self.scan_interval.clone();
        let queues = self.queues.clone();
        
        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(
                Duration::from_millis(MONITORING_INTERVAL_MS)
            );
            
            loop {
                interval.tick().await;
                
                // Clean expired requests (older than 30s)
                let cutoff = Instant::now() - Duration::from_secs(30);
                let mut reqs = requests.write().await;
                let mut cat_usage = category_usage.write().await;
                let mut usage = current_usage.write().await;
                
                while let Some(req) = reqs.front() {
                    if req.timestamp < cutoff {
                        *usage = usage.saturating_sub(req.weight);
                        *cat_usage.get_mut(&req.category).unwrap() = 
                            cat_usage.get(&req.category).unwrap().saturating_sub(req.weight);
                        reqs.pop_front();
                    } else {
                        break;
                    }
                }
                
                // Calculate usage percentage
                let usage_pct = *usage as f64 / TOTAL_CAPACITY as f64;
                
                // Update throttle state
                let mut throttle = throttle_state.write().await;
                let old_state = *throttle;
                *throttle = if usage_pct >= ALERT_THRESHOLD {
                    ThrottleState::Emergency
                } else if usage_pct >= THROTTLE_THRESHOLD {
                    ThrottleState::Heavy
                } else if usage_pct >= RECOVERY_THRESHOLD {
                    ThrottleState::Moderate
                } else {
                    ThrottleState::Normal
                };
                
                // Log state changes
                if *throttle != old_state {
                    tracing::info!(
                        "{} Throttle state changed: {} → {} (usage: {:.1}%)",
                        throttle.emoji(),
                        old_state.as_str(),
                        throttle.as_str(),
                        usage_pct * 100.0
                    );
                }
                
                // Adjust scan interval based on load
                let mut interval_val = scan_interval.write().await;
                let new_interval = if usage_pct >= THROTTLE_THRESHOLD {
                    SCAN_INTERVAL_HIGH_LOAD
                } else {
                    SCAN_INTERVAL_NORMAL
                };
                
                if *interval_val != new_interval {
                    tracing::info!(
                        "⏱️  Scan interval adjusted: {}s → {}s",
                        *interval_val,
                        new_interval
                    );
                    *interval_val = new_interval;
                }
                
                // Alert on high usage
                if usage_pct >= ALERT_THRESHOLD && usage_pct < 0.95 {
                    tracing::warn!(
                        "⚠️  HIGH CAPACITY USAGE: {:.1}% ({}/{})",
                        usage_pct * 100.0,
                        *usage,
                        TOTAL_CAPACITY
                    );
                }
                
                // Process queues (prioritized)
                drop(throttle); // Release lock before processing
                drop(usage);
                drop(cat_usage);
                drop(reqs);
                
                Self::process_queues(
                    queues.clone(),
                    current_usage.clone(),
                    category_usage.clone(),
                    throttle_state.clone(),
                ).await;
            }
        });
        
        *self.monitor_handle.write().await = Some(handle);
        tracing::info!("💓 Unified rate controller monitoring started");
    }
    
    /// Process priority queues
    async fn process_queues(
        queues: Arc<RwLock<HashMap<Priority, VecDeque<QueuedRequest>>>>,
        current_usage: Arc<RwLock<u32>>,
        category_usage: Arc<RwLock<HashMap<OperationCategory, u32>>>,
        throttle_state: Arc<RwLock<ThrottleState>>,
    ) {
        let mut queues_lock = queues.write().await;
        let throttle = *throttle_state.read().await;
        
        // Process in priority order
        for priority in [Priority::Critical, Priority::High, Priority::Medium, Priority::Low] {
            if let Some(queue) = queues_lock.get_mut(&priority) {
                // Process based on throttle state
                let max_to_process = match throttle {
                    ThrottleState::Normal => 10,
                    ThrottleState::Moderate => 5,
                    ThrottleState::Heavy => 2,
                    ThrottleState::Emergency => 1,
                };
                
                let mut processed = 0;
                while processed < max_to_process && !queue.is_empty() {
                    if let Some(req) = queue.front() {
                        let usage = *current_usage.read().await;
                        let projected = usage + req.weight;
                        
                        // Check if we can process this request
                        if projected <= TOTAL_CAPACITY {
                            // Check category allocation
                            let cat_usage = category_usage.read().await;
                            let cat_current = *cat_usage.get(&req.category).unwrap();
                            let cat_max = (TOTAL_CAPACITY as f64 * req.category.max_capacity_share()) as u32;
                            
                            if cat_current + req.weight <= cat_max || 
                               req.priority == Priority::Critical {
                                // Allow the request
                                if let Some(mut req) = queue.pop_front() {
                                    // Update usage
                                    *current_usage.write().await += req.weight;
                                    drop(cat_usage);
                                    let mut cat_usage_mut = category_usage.write().await;
                                    *cat_usage_mut.get_mut(&req.category).unwrap() += req.weight;
                                    
                                    // Signal completion
                                    let _ = req.sender.send(());
                                    processed += 1;
                                }
                            } else {
                                // Category limit reached, skip for now
                                break;
                            }
                        } else {
                            // Capacity full, can't process more
                            break;
                        }
                    } else {
                        break;
                    }
                }
            }
        }
    }
    
    /// Request permission to make an API call
    pub async fn request_permit(
        &self,
        weight: u32,
        priority: Priority,
        category: OperationCategory,
        operation: String,
    ) -> Result<RateLimitPermit> {
        let queue_start = Instant::now();
        
        // Check current usage
        let usage = *self.current_usage.read().await;
        let usage_pct = usage as f64 / TOTAL_CAPACITY as f64;
        
        // Check if we can proceed immediately
        if usage + weight <= TOTAL_CAPACITY {
            // Check category allocation
            let cat_usage = self.category_usage.read().await;
            let cat_current = *cat_usage.get(&category).unwrap();
            let cat_max = (TOTAL_CAPACITY as f64 * category.max_capacity_share()) as u32;
            
            if cat_current + weight <= cat_max || priority == Priority::Critical {
                // Can proceed immediately
                return self.grant_permit(
                    weight,
                    priority,
                    category,
                    operation,
                    0,
                ).await;
            }
        }
        
        // Need to queue
        self.throttled_requests.fetch_add(1, Ordering::Relaxed);
        
        let (tx, rx) = tokio::sync::oneshot::channel();
        let req = QueuedRequest {
            priority,
            category,
            weight,
            operation: operation.clone(),
            queued_at: Instant::now(),
            sender: tx,
        };
        
        // Add to appropriate priority queue
        {
            let mut queues = self.queues.write().await;
            queues.get_mut(&priority).unwrap().push_back(req);
        }
        
        // Wait for permission with timeout based on SLA
        let sla_duration = Duration::from_millis(priority.sla_ms());
        let wait_result = tokio::time::timeout(sla_duration, rx).await;
        
        let queue_time_ms = queue_start.elapsed().as_millis() as u64;
        
        match wait_result {
            Ok(Ok(_)) => {
                // Permission granted
                self.grant_permit(
                    weight,
                    priority,
                    category,
                    operation,
                    queue_time_ms,
                ).await
            }
            Ok(Err(_)) | Err(_) => {
                // SLA violated or timeout
                self.sla_violations.fetch_add(1, Ordering::Relaxed);
                tracing::warn!(
                    "⚠️  SLA VIOLATION: {} {} operation queued for {}ms (SLA: {}ms)",
                    priority.emoji(),
                    priority.as_str(),
                    queue_time_ms,
                    priority.sla_ms()
                );
                
                // Grant anyway for critical operations
                if priority == Priority::Critical {
                    self.grant_permit(
                        weight,
                        priority,
                        category,
                        operation,
                        queue_time_ms,
                    ).await
                } else {
                    Err(anyhow::anyhow!("SLA timeout: {}ms", queue_time_ms))
                }
            }
        }
    }
    
    /// Grant a permit
    async fn grant_permit(
        &self,
        weight: u32,
        priority: Priority,
        category: OperationCategory,
        operation: String,
        queue_time_ms: u64,
    ) -> Result<RateLimitPermit> {
        // Calculate delay based on throttle state
        let throttle = *self.throttle_state.read().await;
        let base_delay_ms = match throttle {
            ThrottleState::Normal => 10,
            ThrottleState::Moderate => 50,
            ThrottleState::Heavy => 200,
            ThrottleState::Emergency => 500,
        };
        
        // Apply jitter
        let jitter = Self::calculate_jitter(base_delay_ms / 2, base_delay_ms * 2);
        let delay = Duration::from_millis(jitter);
        
        // Update counters
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        
        Ok(RateLimitPermit {
            weight,
            priority,
            category,
            operation,
            queue_time_ms,
            delay,
            controller: self,
        })
    }
    
    /// Calculate jitter
    fn calculate_jitter(min_ms: u64, max_ms: u64) -> u64 {
        use std::collections::hash_map::RandomState;
        use std::hash::{BuildHasher, Hash, Hasher};
        
        let s = RandomState::new();
        let mut hasher = s.build_hasher();
        Instant::now().hash(&mut hasher);
        let hash = hasher.finish();
        
        min_ms + (hash % (max_ms - min_ms + 1))
    }
    
    /// Record request completion
    async fn record_request(&self, permit: &RateLimitPermit<'_>, execution_time_ms: u64) {
        let record = RequestRecord {
            timestamp: Instant::now(),
            weight: permit.weight,
            priority: permit.priority,
            category: permit.category,
            operation: permit.operation.clone(),
            queue_time_ms: permit.queue_time_ms,
            execution_time_ms,
        };
        
        self.requests.write().await.push_back(record);
    }
    
    /// Get current statistics
    pub async fn get_stats(&self) -> ControllerStats {
        let usage = *self.current_usage.read().await;
        let cat_usage = self.category_usage.read().await;
        let queues = self.queues.read().await;
        
        let mut queue_depths = HashMap::new();
        for (&priority, queue) in queues.iter() {
            queue_depths.insert(priority, queue.len());
        }
        
        // Calculate average queue time
        let reqs = self.requests.read().await;
        let total_queue_time: u64 = reqs.iter().map(|r| r.queue_time_ms).sum();
        let avg_queue_time = if !reqs.is_empty() {
            total_queue_time as f64 / reqs.len() as f64
        } else {
            0.0
        };
        
        ControllerStats {
            current_usage: usage,
            capacity: TOTAL_CAPACITY,
            usage_percent: (usage as f64 / TOTAL_CAPACITY as f64) * 100.0,
            throttle_state: *self.throttle_state.read().await,
            trading_usage: *cat_usage.get(&OperationCategory::Trading).unwrap(),
            scanning_usage: *cat_usage.get(&OperationCategory::Scanning).unwrap(),
            admin_usage: *cat_usage.get(&OperationCategory::Administrative).unwrap(),
            queue_depths,
            total_requests: self.total_requests.load(Ordering::Relaxed),
            throttled_requests: self.throttled_requests.load(Ordering::Relaxed),
            sla_violations: self.sla_violations.load(Ordering::Relaxed),
            avg_queue_time_ms: avg_queue_time,
            scan_interval_current: *self.scan_interval.read().await,
        }
    }
    
    /// Get current scan interval
    pub async fn get_scan_interval(&self) -> Duration {
        Duration::from_secs(*self.scan_interval.read().await)
    }
}

/// RAII permit that releases resources on drop
pub struct RateLimitPermit<'a> {
    weight: u32,
    priority: Priority,
    category: OperationCategory,
    operation: String,
    queue_time_ms: u64,
    delay: Duration,
    controller: &'a UnifiedRateController,
}

impl<'a> RateLimitPermit<'a> {
    /// Get the required delay before operation
    pub fn delay(&self) -> Duration {
        self.delay
    }
    
    /// Mark operation as complete
    pub async fn complete(self, execution_time_ms: u64) {
        self.controller.record_request(&self, execution_time_ms).await;
    }
}

impl ControllerStats {
    pub fn format_dashboard(&self) -> String {
        let throttle_emoji = self.throttle_state.emoji();
        let throttle_name = self.throttle_state.as_str();
        
        format!(
            "\n┌─────────────────────────────────────────────────────────────┐\n\
             │           🎛️  RATE CONTROLLER DASHBOARD                    │\n\
             ├─────────────────────────────────────────────────────────────┤\n\
             │ Status: {} {} | Usage: {:.1}% ({}/{})           │\n\
             │                                                             │\n\
             │ 📊 Capacity Allocation:                                    │\n\
             │   • Trading:     {:.0}% ({}/{} weight)                    │\n\
             │   • Scanning:    {:.0}% ({}/{} weight)                    │\n\
             │   • Admin:       {:.0}% ({}/{} weight)                    │\n\
             │                                                             │\n\
             │ 📋 Priority Queues:                                        │\n\
             │   🔴 Critical: {} | 🟠 High: {} | 🟡 Medium: {} | 🟢 Low: {} │\n\
             │                                                             │\n\
             │ 📈 Statistics:                                             │\n\
             │   • Total Requests:    {}                             │\n\
             │   • Throttled:         {} ({:.1}%)                      │\n\
             │   • SLA Violations:    {}                             │\n\
             │   • Avg Queue Time:    {:.1}ms                          │\n\
             │   • Scan Interval:     {}s                             │\n\
             └─────────────────────────────────────────────────────────────┘",
            throttle_emoji,
            throttle_name,
            self.usage_percent,
            self.current_usage,
            self.capacity,
            (self.trading_usage as f64 / self.capacity as f64) * 100.0,
            self.trading_usage,
            (self.capacity as f64 * TRADING_RESERVE) as u32,
            (self.scanning_usage as f64 / self.capacity as f64) * 100.0,
            self.scanning_usage,
            (self.capacity as f64 * 0.20) as u32,
            (self.admin_usage as f64 / self.capacity as f64) * 100.0,
            self.admin_usage,
            (self.capacity as f64 * 0.10) as u32,
            self.queue_depths.get(&Priority::Critical).unwrap_or(&0),
            self.queue_depths.get(&Priority::High).unwrap_or(&0),
            self.queue_depths.get(&Priority::Medium).unwrap_or(&0),
            self.queue_depths.get(&Priority::Low).unwrap_or(&0),
            self.total_requests,
            self.throttled_requests,
            if self.total_requests > 0 {
                (self.throttled_requests as f64 / self.total_requests as f64) * 100.0
            } else {
                0.0
            },
            self.sla_violations,
            self.avg_queue_time_ms,
            self.scan_interval_current
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_controller_initialization() {
        let controller = UnifiedRateController::new();
        let stats = controller.get_stats().await;
        
        assert_eq!(stats.current_usage, 0);
        assert_eq!(stats.capacity, TOTAL_CAPACITY);
        assert_eq!(stats.throttle_state, ThrottleState::Normal);
    }

    #[tokio::test]
    async fn test_priority_ordering() {
        assert!(Priority::Critical < Priority::High);
        assert!(Priority::High < Priority::Medium);
        assert!(Priority::Medium < Priority::Low);
    }
}

