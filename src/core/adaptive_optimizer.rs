// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// ADAPTIVE PARAMETER OPTIMIZER - DYNAMIC SELF-TUNING SYSTEM
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
//
// This system continuously monitors performance and automatically adjusts
// operational parameters to optimize:
// 1. Accuracy rates
// 2. Response reliability
// 3. Resource utilization
// 4. User satisfaction metrics
//
// Unlike fixed-percentage systems, this adapts in real-time based on
// actual performance data and market conditions.
//
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

use anyhow::Result;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use serde::{Serialize, Deserialize};

/// Configuration for safe operating ranges
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyBounds {
    // Rate limiting bounds
    pub min_capacity_percent: f64,      // Never below this (e.g., 60%)
    pub max_capacity_percent: f64,      // Never above this (e.g., 95%)
    
    // Throttling bounds
    pub min_throttle_multiplier: f64,   // Minimum speed (e.g., 0.10 = 10%)
    pub max_throttle_multiplier: f64,   // Maximum speed (e.g., 1.0 = 100%)
    
    // Scan interval bounds
    pub min_scan_interval_secs: u64,    // Fastest scan (e.g., 300s = 5 min)
    pub max_scan_interval_secs: u64,    // Slowest scan (e.g., 7200s = 2 hours)
    
    // Batch size bounds
    pub min_batch_size: usize,          // Minimum batch (e.g., 5)
    pub max_batch_size: usize,          // Maximum batch (e.g., 50)
}

impl Default for SafetyBounds {
    fn default() -> Self {
        Self {
            min_capacity_percent: 0.60,
            max_capacity_percent: 0.95,
            min_throttle_multiplier: 0.10,
            max_throttle_multiplier: 1.0,
            min_scan_interval_secs: 300,      // 5 minutes
            max_scan_interval_secs: 7200,     // 2 hours
            min_batch_size: 5,
            max_batch_size: 50,
        }
    }
}

/// Real-time performance metrics
#[derive(Debug, Clone)]
pub struct PerformanceMetrics {
    pub timestamp: Instant,
    
    // Accuracy metrics
    pub successful_operations: u64,
    pub failed_operations: u64,
    pub accuracy_rate: f64,              // 0.0 - 1.0
    
    // Reliability metrics
    pub api_errors: u64,
    pub rate_limit_hits: u64,
    pub timeout_count: u64,
    pub reliability_score: f64,          // 0.0 - 1.0
    
    // Performance metrics
    pub avg_response_time_ms: f64,
    pub p95_response_time_ms: f64,
    pub throughput_per_minute: f64,
    
    // Resource utilization
    pub capacity_usage_percent: f64,
    pub queue_depth: usize,
    pub memory_usage_mb: f64,
    
    // User satisfaction proxy
    pub trade_execution_success_rate: f64,
    pub data_freshness_score: f64,       // 0.0 - 1.0
    pub overall_satisfaction: f64,        // 0.0 - 1.0
}

/// Dynamic parameters that get adjusted
#[derive(Debug, Clone)]
pub struct DynamicParameters {
    pub capacity_target: f64,            // Target capacity utilization
    pub throttle_threshold: f64,         // When to start throttling
    pub recovery_threshold: f64,         // When to recover from throttle
    pub scan_interval_secs: u64,         // How often to scan
    pub batch_size: usize,               // API batch size
    pub priority_boost: f64,             // Boost for critical ops (1.0-2.0)
    
    // Metadata
    pub last_updated: Instant,
    pub update_reason: String,
    pub confidence_score: f64,           // How confident we are (0.0-1.0)
}

impl Default for DynamicParameters {
    fn default() -> Self {
        Self {
            capacity_target: 0.75,
            throttle_threshold: 0.80,
            recovery_threshold: 0.60,
            scan_interval_secs: 3600,
            batch_size: 20,
            priority_boost: 1.0,
            last_updated: Instant::now(),
            update_reason: "Initial defaults".to_string(),
            confidence_score: 0.5,
        }
    }
}

/// Historical performance data point
#[derive(Debug, Clone)]
struct PerformanceSnapshot {
    metrics: PerformanceMetrics,
    parameters: DynamicParameters,
}

/// Optimizer statistics
#[derive(Debug, Clone)]
pub struct OptimizerStats {
    pub total_adjustments: u64,
    pub successful_adjustments: u64,
    pub reverted_adjustments: u64,
    pub current_confidence: f64,
    pub avg_accuracy_improvement: f64,
    pub avg_reliability_improvement: f64,
    pub time_since_last_adjustment: Duration,
    pub safety_bounds_hits: u64,
}

/// Adaptive Parameter Optimizer
pub struct AdaptiveOptimizer {
    // Configuration
    safety_bounds: SafetyBounds,
    
    // Current state
    current_parameters: Arc<RwLock<DynamicParameters>>,
    current_metrics: Arc<RwLock<PerformanceMetrics>>,
    
    // Historical data (keep last 1000 snapshots)
    history: Arc<RwLock<VecDeque<PerformanceSnapshot>>>,
    
    // Statistics
    stats: Arc<RwLock<OptimizerStats>>,
    
    // Monitoring
    monitor_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
}

impl AdaptiveOptimizer {
    pub fn new(safety_bounds: Option<SafetyBounds>) -> Self {
        let bounds = safety_bounds.unwrap_or_default();
        
        tracing::info!("🧠 ADAPTIVE OPTIMIZER INITIALIZING");
        tracing::info!("   Safety Bounds:");
        tracing::info!("   • Capacity: {:.0}%-{:.0}%", 
            bounds.min_capacity_percent * 100.0,
            bounds.max_capacity_percent * 100.0
        );
        tracing::info!("   • Throttle: {:.0}%-{:.0}%",
            bounds.min_throttle_multiplier * 100.0,
            bounds.max_throttle_multiplier * 100.0
        );
        tracing::info!("   • Scan Interval: {}s-{}s",
            bounds.min_scan_interval_secs,
            bounds.max_scan_interval_secs
        );
        tracing::info!("   • Batch Size: {}-{}",
            bounds.min_batch_size,
            bounds.max_batch_size
        );
        
        Self {
            safety_bounds: bounds,
            current_parameters: Arc::new(RwLock::new(DynamicParameters::default())),
            current_metrics: Arc::new(RwLock::new(Self::initial_metrics())),
            history: Arc::new(RwLock::new(VecDeque::with_capacity(1000))),
            stats: Arc::new(RwLock::new(OptimizerStats {
                total_adjustments: 0,
                successful_adjustments: 0,
                reverted_adjustments: 0,
                current_confidence: 0.5,
                avg_accuracy_improvement: 0.0,
                avg_reliability_improvement: 0.0,
                time_since_last_adjustment: Duration::ZERO,
                safety_bounds_hits: 0,
            })),
            monitor_handle: Arc::new(RwLock::new(None)),
        }
    }
    
    fn initial_metrics() -> PerformanceMetrics {
        PerformanceMetrics {
            timestamp: Instant::now(),
            successful_operations: 0,
            failed_operations: 0,
            accuracy_rate: 1.0,
            api_errors: 0,
            rate_limit_hits: 0,
            timeout_count: 0,
            reliability_score: 1.0,
            avg_response_time_ms: 100.0,
            p95_response_time_ms: 200.0,
            throughput_per_minute: 10.0,
            capacity_usage_percent: 0.0,
            queue_depth: 0,
            memory_usage_mb: 0.0,
            trade_execution_success_rate: 1.0,
            data_freshness_score: 1.0,
            overall_satisfaction: 1.0,
        }
    }
    
    /// Start the adaptive optimization loop
    pub async fn start_optimization(&self) {
        let current_parameters = self.current_parameters.clone();
        let current_metrics = self.current_metrics.clone();
        let history = self.history.clone();
        let stats = self.stats.clone();
        let safety_bounds = self.safety_bounds.clone();
        
        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60)); // Optimize every minute
            
            loop {
                interval.tick().await;
                
                // Capture current state
                let metrics = current_metrics.read().await.clone();
                let params = current_parameters.read().await.clone();
                
                // Analyze and optimize
                if let Some(new_params) = Self::analyze_and_optimize(
                    &metrics,
                    &params,
                    &safety_bounds,
                    &history,
                ).await {
                    // Store snapshot before change
                    let snapshot = PerformanceSnapshot {
                        metrics: metrics.clone(),
                        parameters: params.clone(),
                    };
                    
                    let mut hist = history.write().await;
                    hist.push_back(snapshot);
                    if hist.len() > 1000 {
                        hist.pop_front();
                    }
                    drop(hist);
                    
                    // Apply new parameters
                    *current_parameters.write().await = new_params.clone();
                    
                    // Update stats
                    let mut s = stats.write().await;
                    s.total_adjustments += 1;
                    s.time_since_last_adjustment = Duration::ZERO;
                    
                    tracing::info!(
                        "🔧 Parameters adjusted: {}",
                        new_params.update_reason
                    );
                    tracing::debug!(
                        "   Capacity: {:.1}% | Throttle: {:.1}% | Scan: {}s | Batch: {} | Confidence: {:.2}",
                        new_params.capacity_target * 100.0,
                        new_params.throttle_threshold * 100.0,
                        new_params.scan_interval_secs,
                        new_params.batch_size,
                        new_params.confidence_score
                    );
                } else {
                    // Update time since last adjustment
                    let mut s = stats.write().await;
                    s.time_since_last_adjustment += Duration::from_secs(60);
                }
            }
        });
        
        *self.monitor_handle.write().await = Some(handle);
        tracing::info!("🧠 Adaptive optimization started");
    }
    
    /// Analyze performance and determine if optimization is needed
    async fn analyze_and_optimize(
        metrics: &PerformanceMetrics,
        current_params: &DynamicParameters,
        safety_bounds: &SafetyBounds,
        history: &Arc<RwLock<VecDeque<PerformanceSnapshot>>>,
    ) -> Option<DynamicParameters> {
        let mut new_params = current_params.clone();
        let mut adjusted = false;
        let mut reasons = Vec::new();
        
        // Calculate trend from history
        let trend = Self::calculate_trend(history).await;
        
        // 1. ACCURACY OPTIMIZATION
        if metrics.accuracy_rate < 0.95 && metrics.failed_operations > 10 {
            // Poor accuracy - be more conservative
            new_params.batch_size = (new_params.batch_size as f64 * 0.8) as usize;
            new_params.batch_size = new_params.batch_size.max(safety_bounds.min_batch_size);
            new_params.scan_interval_secs = (new_params.scan_interval_secs as f64 * 1.2) as u64;
            new_params.scan_interval_secs = new_params.scan_interval_secs.min(safety_bounds.max_scan_interval_secs);
            adjusted = true;
            reasons.push("accuracy_low");
        } else if metrics.accuracy_rate > 0.99 && trend.accuracy_improving {
            // Excellent accuracy - can be more aggressive
            new_params.batch_size = (new_params.batch_size as f64 * 1.1) as usize;
            new_params.batch_size = new_params.batch_size.min(safety_bounds.max_batch_size);
            new_params.scan_interval_secs = (new_params.scan_interval_secs as f64 * 0.9) as u64;
            new_params.scan_interval_secs = new_params.scan_interval_secs.max(safety_bounds.min_scan_interval_secs);
            adjusted = true;
            reasons.push("accuracy_excellent");
        }
        
        // 2. RELIABILITY OPTIMIZATION
        if metrics.rate_limit_hits > 0 || metrics.api_errors > 5 {
            // Hitting rate limits or errors - back off
            new_params.capacity_target *= 0.85;
            new_params.capacity_target = new_params.capacity_target.max(safety_bounds.min_capacity_percent);
            new_params.throttle_threshold *= 0.90;
            new_params.batch_size = (new_params.batch_size as f64 * 0.75) as usize;
            new_params.batch_size = new_params.batch_size.max(safety_bounds.min_batch_size);
            adjusted = true;
            reasons.push("reliability_issues");
        } else if metrics.reliability_score > 0.98 && metrics.rate_limit_hits == 0 {
            // Very reliable - can push harder
            new_params.capacity_target *= 1.05;
            new_params.capacity_target = new_params.capacity_target.min(safety_bounds.max_capacity_percent);
            new_params.throttle_threshold *= 1.05;
            new_params.throttle_threshold = new_params.throttle_threshold.min(0.95);
            adjusted = true;
            reasons.push("reliability_excellent");
        }
        
        // 3. PERFORMANCE OPTIMIZATION
        if metrics.avg_response_time_ms > 500.0 {
            // Slow responses - reduce load
            new_params.batch_size = (new_params.batch_size as f64 * 0.9) as usize;
            new_params.batch_size = new_params.batch_size.max(safety_bounds.min_batch_size);
            new_params.capacity_target *= 0.95;
            adjusted = true;
            reasons.push("performance_slow");
        } else if metrics.avg_response_time_ms < 100.0 && trend.performance_stable {
            // Fast responses - can do more
            new_params.batch_size = (new_params.batch_size as f64 * 1.05) as usize;
            new_params.batch_size = new_params.batch_size.min(safety_bounds.max_batch_size);
            adjusted = true;
            reasons.push("performance_fast");
        }
        
        // 4. CAPACITY OPTIMIZATION
        if metrics.capacity_usage_percent > 0.85 {
            // High capacity usage - throttle more aggressively
            new_params.throttle_threshold = metrics.capacity_usage_percent - 0.10;
            new_params.recovery_threshold = new_params.throttle_threshold - 0.15;
            new_params.scan_interval_secs = (new_params.scan_interval_secs as f64 * 1.3) as u64;
            new_params.scan_interval_secs = new_params.scan_interval_secs.min(safety_bounds.max_scan_interval_secs);
            adjusted = true;
            reasons.push("capacity_high");
        } else if metrics.capacity_usage_percent < 0.40 && trend.capacity_stable {
            // Low capacity usage - can be more aggressive
            new_params.throttle_threshold *= 1.10;
            new_params.throttle_threshold = new_params.throttle_threshold.min(0.90);
            new_params.scan_interval_secs = (new_params.scan_interval_secs as f64 * 0.85) as u64;
            new_params.scan_interval_secs = new_params.scan_interval_secs.max(safety_bounds.min_scan_interval_secs);
            adjusted = true;
            reasons.push("capacity_low");
        }
        
        // 5. USER SATISFACTION OPTIMIZATION
        if metrics.overall_satisfaction < 0.80 {
            // Low satisfaction - prioritize quality over quantity
            new_params.priority_boost = 1.5;
            new_params.scan_interval_secs = (new_params.scan_interval_secs as f64 * 0.9) as u64;
            new_params.batch_size = (new_params.batch_size as f64 * 0.9) as usize;
            adjusted = true;
            reasons.push("satisfaction_low");
        } else if metrics.overall_satisfaction > 0.95 {
            // High satisfaction - maintain current approach
            new_params.priority_boost = 1.0;
        }
        
        if adjusted {
            // Update metadata
            new_params.last_updated = Instant::now();
            new_params.update_reason = reasons.join(", ");
            
            // Calculate confidence based on data quality
            new_params.confidence_score = Self::calculate_confidence(metrics, &trend);
            
            // Safety check
            if Self::within_safety_bounds(&new_params, safety_bounds) {
                Some(new_params)
            } else {
                tracing::warn!("⚠️  Proposed parameters outside safety bounds, reverting");
                None
            }
        } else {
            None
        }
    }
    
    /// Calculate performance trend from history
    async fn calculate_trend(history: &Arc<RwLock<VecDeque<PerformanceSnapshot>>>) -> PerformanceTrend {
        let hist = history.read().await;
        
        if hist.len() < 10 {
            return PerformanceTrend::default();
        }
        
        // Get last 10 snapshots
        let recent: Vec<_> = hist.iter().rev().take(10).collect();
        
        // Calculate trends
        let accuracy_improving = recent.windows(2).filter(|w| {
            w[0].metrics.accuracy_rate > w[1].metrics.accuracy_rate
        }).count() > 5;
        
        let reliability_improving = recent.windows(2).filter(|w| {
            w[0].metrics.reliability_score > w[1].metrics.reliability_score
        }).count() > 5;
        
        let performance_stable = recent.windows(2).filter(|w| {
            (w[0].metrics.avg_response_time_ms - w[1].metrics.avg_response_time_ms).abs() < 50.0
        }).count() > 7;
        
        let capacity_stable = recent.windows(2).filter(|w| {
            (w[0].metrics.capacity_usage_percent - w[1].metrics.capacity_usage_percent).abs() < 0.10
        }).count() > 7;
        
        PerformanceTrend {
            accuracy_improving,
            reliability_improving,
            performance_stable,
            capacity_stable,
        }
    }
    
    /// Calculate confidence score
    fn calculate_confidence(metrics: &PerformanceMetrics, trend: &PerformanceTrend) -> f64 {
        let mut confidence: f64 = 0.5;
        
        // More data = more confidence
        let total_ops = metrics.successful_operations + metrics.failed_operations;
        if total_ops > 1000 {
            confidence += 0.2;
        } else if total_ops > 100 {
            confidence += 0.1;
        }
        
        // Stable trends = more confidence
        if trend.performance_stable && trend.capacity_stable {
            confidence += 0.2;
        }
        
        // High reliability = more confidence
        if metrics.reliability_score > 0.95 {
            confidence += 0.1;
        }
        
        confidence.min(1.0)
    }
    
    /// Check if parameters are within safety bounds
    fn within_safety_bounds(params: &DynamicParameters, bounds: &SafetyBounds) -> bool {
        params.capacity_target >= bounds.min_capacity_percent
            && params.capacity_target <= bounds.max_capacity_percent
            && params.scan_interval_secs >= bounds.min_scan_interval_secs
            && params.scan_interval_secs <= bounds.max_scan_interval_secs
            && params.batch_size >= bounds.min_batch_size
            && params.batch_size <= bounds.max_batch_size
    }
    
    /// Update metrics (called by external systems)
    pub async fn update_metrics(&self, metrics: PerformanceMetrics) {
        *self.current_metrics.write().await = metrics;
    }
    
    /// Get current parameters
    pub async fn get_parameters(&self) -> DynamicParameters {
        self.current_parameters.read().await.clone()
    }
    
    /// Get optimizer statistics
    pub async fn get_stats(&self) -> OptimizerStats {
        self.stats.read().await.clone()
    }
    
    /// Force a parameter reset to defaults
    pub async fn reset_to_defaults(&self) {
        let mut params = self.current_parameters.write().await;
        *params = DynamicParameters::default();
        params.update_reason = "Manual reset to defaults".to_string();
        
        tracing::warn!("🔄 Parameters reset to defaults");
    }
}

#[derive(Debug, Clone, Default)]
struct PerformanceTrend {
    accuracy_improving: bool,
    reliability_improving: bool,
    performance_stable: bool,
    capacity_stable: bool,
}

impl OptimizerStats {
    pub fn format_report(&self) -> String {
        format!(
            "\n┌────────────────────────────────────────────────────────────┐\n\
             │         🧠 ADAPTIVE OPTIMIZER STATUS                       │\n\
             ├────────────────────────────────────────────────────────────┤\n\
             │ Adjustments:                                               │\n\
             │   • Total: {}                                         │\n\
             │   • Successful: {} ({:.1}%)                           │\n\
             │   • Reverted: {}                                      │\n\
             │                                                            │\n\
             │ Performance:                                               │\n\
             │   • Current Confidence: {:.1}%                         │\n\
             │   • Avg Accuracy Gain: {:+.2}%                         │\n\
             │   • Avg Reliability Gain: {:+.2}%                      │\n\
             │                                                            │\n\
             │ Status:                                                    │\n\
             │   • Time Since Last Adj: {:.0}s                        │\n\
             │   • Safety Bounds Hits: {}                            │\n\
             └────────────────────────────────────────────────────────────┘",
            self.total_adjustments,
            self.successful_adjustments,
            if self.total_adjustments > 0 {
                (self.successful_adjustments as f64 / self.total_adjustments as f64) * 100.0
            } else {
                0.0
            },
            self.reverted_adjustments,
            self.current_confidence * 100.0,
            self.avg_accuracy_improvement * 100.0,
            self.avg_reliability_improvement * 100.0,
            self.time_since_last_adjustment.as_secs(),
            self.safety_bounds_hits
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_optimizer_initialization() {
        let optimizer = AdaptiveOptimizer::new(None);
        let params = optimizer.get_parameters().await;
        
        assert_eq!(params.batch_size, 20);
        assert_eq!(params.scan_interval_secs, 3600);
    }

    #[tokio::test]
    async fn test_safety_bounds() {
        let bounds = SafetyBounds::default();
        let mut params = DynamicParameters::default();
        
        // Test within bounds
        assert!(AdaptiveOptimizer::within_safety_bounds(&params, &bounds));
        
        // Test outside bounds
        params.capacity_target = 0.99;
        assert!(!AdaptiveOptimizer::within_safety_bounds(&params, &bounds));
    }
}

