#![allow(unused_imports)] // Public API exports

pub mod kucoin;
pub mod types;
pub mod websocket;
pub mod rate_limiter;
pub mod adaptive_scheduler;
pub mod unified_rate_controller;

pub use kucoin::KuCoinClient;
pub use types::*;
pub use websocket::WebSocketManager;
pub use rate_limiter::{KuCoinRateLimiter, RateLimiterStats};
pub use adaptive_scheduler::{AdaptiveScheduler, SchedulerStats};
pub use unified_rate_controller::{UnifiedRateController, Priority, OperationCategory, ControllerStats};
