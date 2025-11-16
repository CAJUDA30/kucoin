use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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
    #[serde(flatten)]
    pub extra: HashMap<String, bool>,
}

impl ComponentHealth {
    pub fn get(&self, key: &str) -> Option<bool> {
        match key {
            "database" => Some(self.database),
            "redis" => Some(self.redis),
            "kucoin_api" => Some(self.kucoin_api),
            "ai_models" => Some(self.ai_models),
            _ => self.extra.get(key).copied(),
        }
    }
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
                extra: HashMap::new(),
            })),
        }
    }

    pub async fn get_status(&self) -> HealthStatus {
        let components = self.status.read().await.clone();

        HealthStatus {
            status: if components.kucoin_api {
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
            _ => {
                status.extra.insert(component.to_string(), healthy);
            }
        }
    }
}
