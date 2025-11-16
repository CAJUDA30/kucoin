use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Semaphore, RwLock};
use tokio::time;
use tracing::warn;

#[derive(Debug, Clone)]
pub struct RateLimiterConfig {
    pub max_requests: usize,
    pub window_duration_secs: u64,
    pub burst_allowance: usize,
}

impl Default for RateLimiterConfig {
    fn default() -> Self {
        Self {
            max_requests: 90,  // API limit is 100 msg/sec, using 90% for safety
            window_duration_secs: 1,  // 1-second window for precise rate limiting
            burst_allowance: 45,  // 50% burst allowance
        }
    }
}

pub struct RateLimiter {
    config: RateLimiterConfig,
    semaphore: Arc<Semaphore>,
    request_times: Arc<RwLock<VecDeque<Instant>>>,
}

impl RateLimiter {
    pub fn new(config: RateLimiterConfig) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(config.burst_allowance)),
            request_times: Arc::new(RwLock::new(VecDeque::new())),
            config,
        }
    }

    pub async fn acquire(&self) -> Result<RateLimitPermit, String> {
        // Acquire semaphore permit for burst control
        let permit = self.semaphore.clone().acquire_owned().await
            .map_err(|e| format!("Failed to acquire semaphore: {}", e))?;

        // Check sliding window
        loop {
            let mut times = self.request_times.write().await;
            let now = Instant::now();
            let window_start = now - Duration::from_secs(self.config.window_duration_secs);

            // Remove old entries
            while let Some(&front) = times.front() {
                if front < window_start {
                    times.pop_front();
                } else {
                    break;
                }
            }

            // Check if we're within rate limit
            if times.len() < self.config.max_requests {
                times.push_back(now);
                return Ok(RateLimitPermit {
                    _permit: permit,
                    start_time: now,
                });
            }

            // Calculate wait time
            if let Some(&oldest) = times.front() {
                let wait_duration = oldest + Duration::from_secs(self.config.window_duration_secs) - now;
                warn!("⏳ Rate limit reached, waiting {:?}", wait_duration);
                drop(times); // Release lock before sleeping
                time::sleep(wait_duration).await;
            } else {
                break;
            }
        }

        Ok(RateLimitPermit {
            _permit: permit,
            start_time: Instant::now(),
        })
    }

    pub async fn get_current_usage(&self) -> usize {
        let times = self.request_times.read().await;
        let now = Instant::now();
        let window_start = now - Duration::from_secs(self.config.window_duration_secs);

        times.iter().filter(|&&t| t >= window_start).count()
    }

    pub async fn get_available_capacity(&self) -> usize {
        let usage = self.get_current_usage().await;
        self.config.max_requests.saturating_sub(usage)
    }

    pub fn get_max_requests(&self) -> usize {
        self.config.max_requests
    }

    pub fn get_window_duration_secs(&self) -> u64 {
        self.config.window_duration_secs
    }
}

pub struct RateLimitPermit {
    _permit: tokio::sync::OwnedSemaphorePermit,
    start_time: Instant,
}

impl RateLimitPermit {
    pub fn elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rate_limiter_creation() {
        let config = RateLimiterConfig::default();
        let limiter = RateLimiter::new(config.clone());
        
        assert_eq!(limiter.get_max_requests(), config.max_requests);
        assert_eq!(limiter.get_window_duration_secs(), config.window_duration_secs);
    }

    #[tokio::test]
    async fn test_rate_limiter_acquire() {
        let config = RateLimiterConfig {
            max_requests: 10,
            window_duration_secs: 1,
            burst_allowance: 5,
        };
        let limiter = RateLimiter::new(config);

        let permit = limiter.acquire().await;
        assert!(permit.is_ok());
        
        let usage = limiter.get_current_usage().await;
        assert_eq!(usage, 1);
    }

    #[tokio::test]
    async fn test_rate_limiter_capacity() {
        let config = RateLimiterConfig {
            max_requests: 100,
            window_duration_secs: 10,
            burst_allowance: 50,
        };
        let limiter = RateLimiter::new(config);

        let capacity = limiter.get_available_capacity().await;
        assert_eq!(capacity, 100);
    }
}

