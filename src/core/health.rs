use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    pub status: String,
    pub version: String,
    pub uptime_seconds: u64,
    pub components: ComponentHealth,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentHealth {
    pub database: bool,
    pub redis: bool,
    pub kucoin_api: bool,
    pub ai_models: bool,
}

#[derive(Clone)]
pub struct HealthChecker {
    start_time: std::time::Instant,
    status: Arc<RwLock<ComponentHealth>>,
}

impl HealthChecker {
    pub fn new() -> Self {
        Self {
            start_time: std::time::Instant::now(),
            status: Arc::new(RwLock::new(ComponentHealth {
                database: false,
                redis: false,
                kucoin_api: false,
                ai_models: false,
            })),
        }
    }

    pub async fn get_status(&self) -> HealthStatus {
        let components = self.status.read().await.clone();

        HealthStatus {
            status: if components.database && components.redis {
                "healthy".to_string()
            } else {
                "degraded".to_string()
            },
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime_seconds: self.start_time.elapsed().as_secs(),
            components,
        }
    }

    pub async fn update_component(&self, component: &str, healthy: bool) {
        let mut status = self.status.write().await;
        match component {
            "database" => status.database = healthy,
            "redis" => status.redis = healthy,
            "kucoin_api" => status.kucoin_api = healthy,
            "ai_models" => status.ai_models = healthy,
            _ => {}
        }
    }
}
