use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, fmt};

pub fn init_logging(log_level: &str) {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(log_level));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt::layer())
        .init();

    tracing::info!("Logging initialized at level: {}", log_level);
}
