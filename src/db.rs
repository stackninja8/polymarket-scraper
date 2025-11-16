use anyhow::Result;
use chrono::Utc;
use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    Pool, Sqlite,
};
use std::str::FromStr;
use tracing::info;

use crate::models::Market;

/// Initialize database connection pool
pub async fn init_db(database_url: &str) -> Result<Pool<Sqlite>> {
    info!("Connecting to database at: {}", database_url);
    
    // For SQLite, ensure the database file can be created
    // Extract file path from connection string (format: sqlite:path or sqlite://path)
    let db_path = database_url
        .strip_prefix("sqlite://")
        .or_else(|| database_url.strip_prefix("sqlite:"))
        .unwrap_or(database_url);
    
    // Ensure parent directory exists if path contains directories
    if let Some(parent) = std::path::Path::new(db_path).parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)
                .map_err(|e| anyhow::anyhow!("Failed to create database directory: {}", e))?;
        }
    }
    
    // Use SqliteConnectOptions to enable create_if_missing
    let options = SqliteConnectOptions::from_str(database_url)?
        .create_if_missing(true);
    
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;

    // Run migrations
    sqlx::migrate!("./migrations").run(&pool).await?;
    
    info!("Database initialized successfully");
    Ok(pool)
}

/// Upsert a market into the database
/// Returns true if the market was newly discovered, false if it was updated
pub async fn upsert_market(pool: &Pool<Sqlite>, market: &Market) -> Result<bool> {
    let is_new = sqlx::query_scalar::<_, bool>(
        "SELECT NOT EXISTS(SELECT 1 FROM markets WHERE id = ?)"
    )
    .bind(&market.id)
    .fetch_one(pool)
    .await?;

    let now = Utc::now();
    
    if is_new {
        // Insert new market
        sqlx::query(
            r#"
            INSERT INTO markets (id, title, description, current_price, volume, end_date, discovered_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&market.id)
        .bind(&market.title)
        .bind(&market.description)
        .bind(market.current_price)
        .bind(market.volume)
        .bind(&market.end_date)
        .bind(now)
        .bind(now)
        .execute(pool)
        .await?;
    } else {
        // Update existing market (preserve discovered_at)
        sqlx::query(
            r#"
            UPDATE markets SET
                title = ?,
                description = ?,
                current_price = ?,
                volume = ?,
                end_date = ?,
                updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(&market.title)
        .bind(&market.description)
        .bind(market.current_price)
        .bind(market.volume)
        .bind(&market.end_date)
        .bind(now)
        .bind(&market.id)
        .execute(pool)
        .await?;
    }

    Ok(is_new)
}

/// Get all markets with pagination
pub async fn get_markets(
    pool: &Pool<Sqlite>,
    limit: u32,
    offset: u32,
) -> Result<(Vec<Market>, i64)> {
    let markets = sqlx::query_as::<_, Market>(
        "SELECT id, title, description, current_price, volume, end_date, discovered_at, updated_at 
         FROM markets 
         ORDER BY discovered_at DESC 
         LIMIT ? OFFSET ?"
    )
    .bind(limit as i64)
    .bind(offset as i64)
    .fetch_all(pool)
    .await?;

    let total = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM markets")
        .fetch_one(pool)
        .await?;

    Ok((markets, total))
}

/// Get markets discovered since a given timestamp
pub async fn get_markets_since(
    pool: &Pool<Sqlite>,
    since: chrono::DateTime<Utc>,
) -> Result<Vec<Market>> {
    let markets = sqlx::query_as::<_, Market>(
        "SELECT id, title, description, current_price, volume, end_date, discovered_at, updated_at 
         FROM markets 
         WHERE discovered_at >= ? 
         ORDER BY discovered_at DESC"
    )
    .bind(since)
    .fetch_all(pool)
    .await?;

    Ok(markets)
}

/// Get a single market by ID
pub async fn get_market_by_id(pool: &Pool<Sqlite>, id: &str) -> Result<Option<Market>> {
    let market = sqlx::query_as::<_, Market>(
        "SELECT id, title, description, current_price, volume, end_date, discovered_at, updated_at 
         FROM markets 
         WHERE id = ?"
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    Ok(market)
}

