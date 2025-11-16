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
    pub is_new: bool, // True if listed within last 24 hours
    pub delisted_at: Option<DateTime<Utc>>,
    pub lot_size: Option<f64>,
    pub tick_size: Option<f64>,
    pub multiplier: Option<f64>,
    pub max_leverage: Option<i32>,
    pub funding_rate_symbol: Option<String>,
    pub metadata: String, // JSON string for additional data
}

impl TokenRecord {
    /// Returns a visual badge for this token
    pub fn get_badge(&self) -> &str {
        if self.status == "delisted" {
            "🔴 DELISTED"
        } else if self.is_new {
            "🆕 NEW"
        } else {
            "✅ ACTIVE"
        }
    }

    /// Returns colored display string for terminal
    pub fn get_colored_status(&self) -> String {
        match self.status.as_str() {
            "delisted" => format!("\x1b[31m{}\x1b[0m", "DELISTED"), // Red
            "suspended" => format!("\x1b[33m{}\x1b[0m", "SUSPENDED"), // Yellow
            _ if self.is_new => format!("\x1b[32;1m{}\x1b[0m", "NEW"), // Bright Green
            _ => format!("\x1b[32m{}\x1b[0m", "ACTIVE"), // Green
        }
    }

    /// Check if token is still considered "new" (within 24 hours)
    pub fn is_still_new(&self) -> bool {
        if !self.is_new {
            return false;
        }
        let age = Utc::now() - self.first_seen;
        age.num_hours() < 24
    }
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
        // Create tokens table with enhanced tracking
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS tokens (
                symbol TEXT PRIMARY KEY,
                base_currency TEXT NOT NULL,
                quote_currency TEXT NOT NULL,
                first_seen DATETIME NOT NULL,
                last_seen DATETIME NOT NULL,
                status TEXT NOT NULL,
                is_new BOOLEAN NOT NULL DEFAULT 1,
                delisted_at DATETIME,
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
                status, is_new, delisted_at, lot_size, tick_size, multiplier, max_leverage,
                funding_rate_symbol, metadata
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(symbol) DO UPDATE SET
                last_seen = excluded.last_seen,
                status = excluded.status,
                is_new = excluded.is_new,
                delisted_at = excluded.delisted_at,
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
        .bind(token.is_new)
        .bind(token.delisted_at)
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
        let now = Utc::now();
        sqlx::query(
            r#"
            UPDATE tokens SET status = 'delisted', delisted_at = ?, is_new = 0, last_seen = ? WHERE symbol = ?
            "#,
        )
        .bind(now)
        .bind(now)
        .bind(symbol)
        .execute(&self.pool)
        .await?;

        self.add_history_event(symbol, "delisted", None).await?;
        tracing::warn!("🔴 TOKEN DELISTED: {}", symbol);

        Ok(())
    }

    /// Update tokens that are no longer "new" (older than 24 hours)
    pub async fn update_new_status(&self) -> Result<usize> {
        let cutoff = Utc::now() - chrono::Duration::hours(24);
        
        let result = sqlx::query(
            r#"
            UPDATE tokens 
            SET is_new = 0 
            WHERE is_new = 1 AND first_seen < ?
            "#,
        )
        .bind(cutoff)
        .execute(&self.pool)
        .await?;

        let count = result.rows_affected() as usize;
        if count > 0 {
            tracing::debug!("Updated {} tokens from NEW to ACTIVE status", count);
        }

        Ok(count)
    }

    /// Get all tokens grouped by their status for visual display
    pub async fn get_tokens_by_category(&self) -> Result<TokenCategories> {
        let all_tokens = self.get_active_tokens().await?;
        
        let mut new_listings = Vec::new();
        let mut active_listings = Vec::new();
        
        for token in all_tokens {
            if token.is_still_new() {
                new_listings.push(token);
            } else {
                active_listings.push(token);
            }
        }

        let delisted = sqlx::query_as::<_, TokenRecord>(
            r#"
            SELECT * FROM tokens WHERE status = 'delisted' ORDER BY delisted_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(TokenCategories {
            new_listings,
            active_listings,
            delisted_tokens: delisted,
        })
    }

    pub async fn get_statistics(&self) -> Result<TokenStatistics> {
        #[derive(sqlx::FromRow)]
        struct Stats {
            total: i64,
            active: i64,
            delisted: i64,
            new: i64,
        }

        let stats = sqlx::query_as::<_, Stats>(
            r#"
            SELECT 
                COUNT(*) as total,
                SUM(CASE WHEN status = 'active' THEN 1 ELSE 0 END) as active,
                SUM(CASE WHEN status = 'delisted' THEN 1 ELSE 0 END) as delisted,
                SUM(CASE WHEN is_new = 1 AND status = 'active' THEN 1 ELSE 0 END) as new
            FROM tokens
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(TokenStatistics {
            total_tokens: stats.total as usize,
            active_tokens: stats.active as usize,
            delisted_tokens: stats.delisted as usize,
            new_tokens: stats.new as usize,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenCategories {
    pub new_listings: Vec<TokenRecord>,
    pub active_listings: Vec<TokenRecord>,
    pub delisted_tokens: Vec<TokenRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenStatistics {
    pub total_tokens: usize,
    pub active_tokens: usize,
    pub delisted_tokens: usize,
    pub new_tokens: usize,
}

