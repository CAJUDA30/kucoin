use anyhow::Result;
use serde::Deserialize;
use std::env;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub kucoin: KuCoinConfig,
    pub database: DatabaseConfig,
    pub trading: TradingConfig,
    pub monitoring: MonitoringConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct KuCoinConfig {
    pub api_key: String,
    pub api_secret: String,
    pub api_passphrase: String,
    pub sandbox_mode: bool,
    pub base_url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    pub postgres_url: String,
    pub redis_url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TradingConfig {
    pub max_position_size: f64,
    pub max_leverage: u32,
    pub daily_loss_limit: f64,
    pub ai_consensus_threshold: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MonitoringConfig {
    pub prometheus_port: u16,
    pub frontend_port: u16,
    pub log_level: String,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        dotenv::dotenv().ok();

        Ok(Config {
            kucoin: KuCoinConfig {
                api_key: env::var("KUCOIN_API_KEY").unwrap_or_else(|_| "sandbox_key".to_string()),
                api_secret: env::var("KUCOIN_API_SECRET")
                    .unwrap_or_else(|_| "sandbox_secret".to_string()),
                api_passphrase: env::var("KUCOIN_API_PASSPHRASE")
                    .unwrap_or_else(|_| "sandbox_passphrase".to_string()),
                sandbox_mode: env::var("KUCOIN_SANDBOX_MODE")
                    .unwrap_or_else(|_| "true".to_string())
                    .parse()
                    .unwrap_or(true),
                base_url: if env::var("KUCOIN_SANDBOX_MODE")
                    .unwrap_or_else(|_| "true".to_string())
                    .parse()
                    .unwrap_or(true)
                {
                    "https://api-sandbox-futures.kucoin.com".to_string()
                } else {
                    "https://api-futures.kucoin.com".to_string()
                },
            },
            database: DatabaseConfig {
                postgres_url: env::var("POSTGRES_URL").unwrap_or_else(|_| {
                    "postgresql://trading:password@localhost:5432/trading_bot".to_string()
                }),
                redis_url: env::var("REDIS_URL")
                    .unwrap_or_else(|_| "redis://localhost:6379".to_string()),
            },
            trading: TradingConfig {
                max_position_size: env::var("MAX_POSITION_SIZE")
                    .unwrap_or_else(|_| "0.20".to_string())
                    .parse()
                    .unwrap_or(0.20),
                max_leverage: env::var("MAX_LEVERAGE")
                    .unwrap_or_else(|_| "10".to_string())
                    .parse()
                    .unwrap_or(10),
                daily_loss_limit: env::var("DAILY_LOSS_LIMIT")
                    .unwrap_or_else(|_| "0.20".to_string())
                    .parse()
                    .unwrap_or(0.20),
                ai_consensus_threshold: env::var("AI_CONSENSUS_THRESHOLD")
                    .unwrap_or_else(|_| "0.85".to_string())
                    .parse()
                    .unwrap_or(0.85),
            },
            monitoring: MonitoringConfig {
                prometheus_port: env::var("PROMETHEUS_PORT")
                    .unwrap_or_else(|_| "9090".to_string())
                    .parse()
                    .unwrap_or(9090),
                frontend_port: env::var("FRONTEND_PORT")
                    .unwrap_or_else(|_| "3000".to_string())
                    .parse()
                    .unwrap_or(3000),
                log_level: env::var("LOG_LEVEL").unwrap_or_else(|_| "info".to_string()),
            },
        })
    }
}
