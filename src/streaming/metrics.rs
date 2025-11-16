use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

#[derive(Debug)]
pub struct PerformanceMetrics {
    messages_received: AtomicU64,
    messages_sent: AtomicU64,
    errors_count: AtomicU64,
    total_latency_ms: AtomicU64,
    latency_samples: AtomicUsize,
    connection_latencies: Arc<RwLock<Vec<Duration>>>,
    start_time: Instant,
}

impl PerformanceMetrics {
    pub fn new() -> Self {
        Self {
            messages_received: AtomicU64::new(0),
            messages_sent: AtomicU64::new(0),
            errors_count: AtomicU64::new(0),
            total_latency_ms: AtomicU64::new(0),
            latency_samples: AtomicUsize::new(0),
            connection_latencies: Arc::new(RwLock::new(Vec::new())),
            start_time: Instant::now(),
        }
    }

    pub fn increment_messages_received(&self) {
        self.messages_received.fetch_add(1, Ordering::Relaxed);
    }

    pub fn increment_messages_sent(&self) {
        self.messages_sent.fetch_add(1, Ordering::Relaxed);
    }

    pub fn increment_errors(&self) {
        self.errors_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_message_latency(&self, latency: Duration) {
        self.total_latency_ms.fetch_add(latency.as_millis() as u64, Ordering::Relaxed);
        self.latency_samples.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_connection_latency(&self, latency: Duration) {
        let latencies = self.connection_latencies.clone();
        tokio::spawn(async move {
            let mut lat = latencies.write().await;
            lat.push(latency);
            // Keep only last 100 samples
            if lat.len() > 100 {
                let excess = lat.len() - 100;
                lat.drain(0..excess);
            }
        });
    }

    pub fn get_messages_received(&self) -> u64 {
        self.messages_received.load(Ordering::Relaxed)
    }

    pub fn get_messages_sent(&self) -> u64 {
        self.messages_sent.load(Ordering::Relaxed)
    }

    pub fn get_errors_count(&self) -> u64 {
        self.errors_count.load(Ordering::Relaxed)
    }

    pub fn get_average_latency_ms(&self) -> f64 {
        let total = self.total_latency_ms.load(Ordering::Relaxed);
        let samples = self.latency_samples.load(Ordering::Relaxed);
        
        if samples == 0 {
            0.0
        } else {
            total as f64 / samples as f64
        }
    }

    pub fn get_uptime_secs(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }

    pub fn get_message_rate(&self) -> f64 {
        let uptime = self.get_uptime_secs();
        if uptime == 0 {
            0.0
        } else {
            self.get_messages_received() as f64 / uptime as f64
        }
    }

    pub fn get_error_rate(&self) -> f64 {
        let received = self.get_messages_received();
        if received == 0 {
            0.0
        } else {
            self.get_errors_count() as f64 / received as f64
        }
    }

    pub async fn get_snapshot(&self) -> HashMap<String, serde_json::Value> {
        let mut snapshot = HashMap::new();
        
        snapshot.insert("messages_received".to_string(), 
            serde_json::json!(self.get_messages_received()));
        snapshot.insert("messages_sent".to_string(), 
            serde_json::json!(self.get_messages_sent()));
        snapshot.insert("errors_count".to_string(), 
            serde_json::json!(self.get_errors_count()));
        snapshot.insert("average_latency_ms".to_string(), 
            serde_json::json!(self.get_average_latency_ms()));
        snapshot.insert("uptime_secs".to_string(), 
            serde_json::json!(self.get_uptime_secs()));
        snapshot.insert("message_rate_per_sec".to_string(), 
            serde_json::json!(self.get_message_rate()));
        snapshot.insert("error_rate".to_string(), 
            serde_json::json!(self.get_error_rate()));
        
        // Calculate percentiles for connection latency
        {
            let latencies = self.connection_latencies.read().await;
            if !latencies.is_empty() {
                let mut sorted: Vec<u128> = latencies.iter()
                    .map(|d| d.as_millis())
                    .collect();
                sorted.sort_unstable();
                
                let p50_idx = sorted.len() / 2;
                let p95_idx = (sorted.len() as f64 * 0.95) as usize;
                let p99_idx = (sorted.len() as f64 * 0.99) as usize;
                
                snapshot.insert("connection_latency_p50_ms".to_string(), 
                    serde_json::json!(sorted.get(p50_idx).unwrap_or(&0)));
                snapshot.insert("connection_latency_p95_ms".to_string(), 
                    serde_json::json!(sorted.get(p95_idx).unwrap_or(&0)));
                snapshot.insert("connection_latency_p99_ms".to_string(), 
                    serde_json::json!(sorted.get(p99_idx).unwrap_or(&0)));
            }
        }
        
        snapshot
    }

    pub async fn print_report(&self) {
        let snapshot = self.get_snapshot().await;
        
        tracing::info!("
╔══════════════════════════════════════════════════════════════════════╗
║              STREAMING PERFORMANCE METRICS                          ║
╚══════════════════════════════════════════════════════════════════════╝

📊 Message Statistics:
   • Messages Received:  {}
   • Messages Sent:      {}
   • Errors:             {}
   • Message Rate:       {:.2} msg/sec

⚡ Latency Metrics:
   • Average Latency:    {:.2} ms
   • P50 Conn Latency:   {} ms
   • P95 Conn Latency:   {} ms
   • P99 Conn Latency:   {} ms

🔧 System Health:
   • Uptime:             {} seconds
   • Error Rate:         {:.4}%
   • SLA Compliance:     {:.2}%

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
",
            snapshot.get("messages_received").unwrap(),
            snapshot.get("messages_sent").unwrap(),
            snapshot.get("errors_count").unwrap(),
            snapshot.get("message_rate_per_sec").unwrap().as_f64().unwrap_or(0.0),
            snapshot.get("average_latency_ms").unwrap().as_f64().unwrap_or(0.0),
            snapshot.get("connection_latency_p50_ms").unwrap_or(&serde_json::json!(0)),
            snapshot.get("connection_latency_p95_ms").unwrap_or(&serde_json::json!(0)),
            snapshot.get("connection_latency_p99_ms").unwrap_or(&serde_json::json!(0)),
            snapshot.get("uptime_secs").unwrap(),
            snapshot.get("error_rate").unwrap().as_f64().unwrap_or(0.0) * 100.0,
            (1.0 - snapshot.get("error_rate").unwrap().as_f64().unwrap_or(0.0)) * 100.0,
        );
    }
}

impl Default for PerformanceMetrics {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct StreamMetrics {
    pub symbol: String,
    pub last_update: Instant,
    pub update_count: u64,
    pub average_latency_ms: f64,
}

impl StreamMetrics {
    pub fn new(symbol: String) -> Self {
        Self {
            symbol,
            last_update: Instant::now(),
            update_count: 0,
            average_latency_ms: 0.0,
        }
    }

    pub fn update(&mut self, latency_ms: u64) {
        self.last_update = Instant::now();
        self.update_count += 1;
        
        // Running average
        self.average_latency_ms = 
            (self.average_latency_ms * (self.update_count - 1) as f64 + latency_ms as f64) 
            / self.update_count as f64;
    }

    pub fn is_stale(&self, timeout_secs: u64) -> bool {
        self.last_update.elapsed().as_secs() > timeout_secs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_initialization() {
        let metrics = PerformanceMetrics::new();
        assert_eq!(metrics.get_messages_received(), 0);
        assert_eq!(metrics.get_average_latency_ms(), 0.0);
    }

    #[test]
    fn test_metrics_increments() {
        let metrics = PerformanceMetrics::new();
        metrics.increment_messages_received();
        metrics.increment_messages_received();
        assert_eq!(metrics.get_messages_received(), 2);
    }

    #[test]
    fn test_latency_recording() {
        let metrics = PerformanceMetrics::new();
        metrics.record_message_latency(Duration::from_millis(10));
        metrics.record_message_latency(Duration::from_millis(20));
        assert_eq!(metrics.get_average_latency_ms(), 15.0);
    }

    #[test]
    fn test_stream_metrics() {
        let mut stream = StreamMetrics::new("XBTUSDTM".to_string());
        stream.update(10);
        stream.update(20);
        assert_eq!(stream.update_count, 2);
        assert_eq!(stream.average_latency_ms, 15.0);
    }
}

