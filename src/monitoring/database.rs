use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TokenRecord {
    pub symbol: String,
    pub base_currency: String,
    pub quote_currency: String,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub status: String, // "active", "delisted", "suspended"
    pub lot_size: Option<f64>,
    pub tick_size: Option<f64>,
    pub multiplier: Option<f64>,
    pub max_leverage: Option<i32>,
    pub funding_rate_symbol: Option<String>,
    pub metadata: String, // JSON string for additional data
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenMetadata {
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub listing_price: Option<f64>,
    pub initial_volume: Option<f64>,
    pub market_cap: Option<f64>,
}

pub struct TokenDatabase {
    pool: SqlitePool,
}

impl TokenDatabase {
    pub async fn new(database_path: &str) -> Result<Self> {
        // Create parent directory if it doesn't exist
        if let Some(parent) = Path::new(database_path).parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Use sqlite:// prefix for consistency
        let connection_string = if database_path.starts_with("sqlite:") {
            database_path.to_string()
        } else {
            format!("sqlite://{}?mode=rwc", database_path)
        };

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&connection_string)
            .await?;

        let db = Self { pool };
        db.initialize_schema().await?;

        Ok(db)
    }

    async fn initialize_schema(&self) -> Result<()> {
        // Create tokens table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS tokens (
                symbol TEXT PRIMARY KEY,
                base_currency TEXT NOT NULL,
                quote_currency TEXT NOT NULL,
                first_seen DATETIME NOT NULL,
                last_seen DATETIME NOT NULL,
                status TEXT NOT NULL,
                lot_size REAL,
                tick_size REAL,
                multiplier REAL,
                max_leverage INTEGER,
                funding_rate_symbol TEXT,
                metadata TEXT NOT NULL DEFAULT '{}'
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        // Create token history table for tracking changes
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS token_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                symbol TEXT NOT NULL,
                event_type TEXT NOT NULL,
                event_time DATETIME NOT NULL,
                details TEXT,
                FOREIGN KEY (symbol) REFERENCES tokens(symbol)
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        // Create index for faster queries
        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_token_history_symbol 
            ON token_history(symbol, event_time DESC)
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_tokens_status 
            ON tokens(status, last_seen DESC)
            "#,
        )
        .execute(&self.pool)
        .await?;

        tracing::info!("✅ Token database schema initialized");

        Ok(())
    }

    pub async fn upsert_token(&self, token: &TokenRecord) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO tokens (
                symbol, base_currency, quote_currency, first_seen, last_seen,
                status, lot_size, tick_size, multiplier, max_leverage,
                funding_rate_symbol, metadata
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(symbol) DO UPDATE SET
                last_seen = excluded.last_seen,
                status = excluded.status,
                lot_size = excluded.lot_size,
                tick_size = excluded.tick_size,
                multiplier = excluded.multiplier,
                max_leverage = excluded.max_leverage,
                funding_rate_symbol = excluded.funding_rate_symbol,
                metadata = excluded.metadata
            "#,
        )
        .bind(&token.symbol)
        .bind(&token.base_currency)
        .bind(&token.quote_currency)
        .bind(&token.first_seen)
        .bind(&token.last_seen)
        .bind(&token.status)
        .bind(token.lot_size)
        .bind(token.tick_size)
        .bind(token.multiplier)
        .bind(token.max_leverage)
        .bind(&token.funding_rate_symbol)
        .bind(&token.metadata)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn add_history_event(
        &self,
        symbol: &str,
        event_type: &str,
        details: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO token_history (symbol, event_type, event_time, details)
            VALUES (?, ?, ?, ?)
            "#,
        )
        .bind(symbol)
        .bind(event_type)
        .bind(Utc::now())
        .bind(details)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_all_tokens(&self) -> Result<Vec<TokenRecord>> {
        let tokens = sqlx::query_as::<_, TokenRecord>(
            r#"
            SELECT * FROM tokens ORDER BY last_seen DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(tokens)
    }

    pub async fn get_active_tokens(&self) -> Result<Vec<TokenRecord>> {
        let tokens = sqlx::query_as::<_, TokenRecord>(
            r#"
            SELECT * FROM tokens WHERE status = 'active' ORDER BY last_seen DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(tokens)
    }

    pub async fn get_token(&self, symbol: &str) -> Result<Option<TokenRecord>> {
        let token = sqlx::query_as::<_, TokenRecord>(
            r#"
            SELECT * FROM tokens WHERE symbol = ?
            "#,
        )
        .bind(symbol)
        .fetch_optional(&self.pool)
        .await?;

        Ok(token)
    }

    pub async fn get_new_listings(&self, since: DateTime<Utc>) -> Result<Vec<TokenRecord>> {
        let tokens = sqlx::query_as::<_, TokenRecord>(
            r#"
            SELECT * FROM tokens 
            WHERE first_seen >= ? 
            ORDER BY first_seen DESC
            "#,
        )
        .bind(since)
        .fetch_all(&self.pool)
        .await?;

        Ok(tokens)
    }

    pub async fn mark_as_delisted(&self, symbol: &str) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE tokens SET status = 'delisted', last_seen = ? WHERE symbol = ?
            "#,
        )
        .bind(Utc::now())
        .bind(symbol)
        .execute(&self.pool)
        .await?;

        self.add_history_event(symbol, "delisted", None).await?;

        Ok(())
    }

    pub async fn get_statistics(&self) -> Result<TokenStatistics> {
        #[derive(sqlx::FromRow)]
        struct Stats {
            total: i64,
            active: i64,
            delisted: i64,
        }

        let stats = sqlx::query_as::<_, Stats>(
            r#"
            SELECT 
                COUNT(*) as total,
                SUM(CASE WHEN status = 'active' THEN 1 ELSE 0 END) as active,
                SUM(CASE WHEN status = 'delisted' THEN 1 ELSE 0 END) as delisted
            FROM tokens
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(TokenStatistics {
            total_tokens: stats.total as usize,
            active_tokens: stats.active as usize,
            delisted_tokens: stats.delisted as usize,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenStatistics {
    pub total_tokens: usize,
    pub active_tokens: usize,
    pub delisted_tokens: usize,
}

