use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::get,
    Router,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use sqlx::Pool;
use sqlx::Sqlite;
use std::sync::Arc;
use tracing::{error, info};

use crate::db;
use crate::metrics::Metrics;
use crate::models::{HealthResponse, Market, MarketsResponse, MetricsResponse};

/// Query parameters for pagination
#[derive(Debug, Deserialize)]
pub struct PaginationParams {
    #[serde(default = "default_limit")]
    pub limit: u32,
    #[serde(default = "default_offset")]
    pub offset: u32,
}

fn default_limit() -> u32 {
    20
}

fn default_offset() -> u32 {
    0
}

/// Query parameters for filtering markets by discovery date
#[derive(Debug, Deserialize)]
pub struct SinceParams {
    pub since: DateTime<Utc>,
}

/// API state containing both database pool and metrics
#[derive(Clone)]
pub struct AppState {
    pub pool: Arc<Pool<Sqlite>>,
    pub metrics: Arc<Metrics>,
}

/// Create the API router
pub fn create_router(pool: Arc<Pool<Sqlite>>, metrics: Arc<Metrics>) -> Router {
    let state = AppState { pool, metrics };
    
    Router::new()
        .route("/health", get(health_handler))
        .route("/metrics", get(metrics_handler))
        .route("/markets", get(markets_handler))
        .route("/markets/new", get(new_markets_handler))
        .route("/markets/:id", get(market_by_id_handler))
        .with_state(state)
}

/// Health check endpoint
async fn health_handler() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
    })
}

/// Metrics endpoint
async fn metrics_handler(
    State(state): State<AppState>,
) -> Result<Json<MetricsResponse>, StatusCode> {
    let total_markets = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM markets")
        .fetch_one(&*state.pool)
        .await
        .map_err(|e| {
            error!("Database error in metrics_handler: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let last_scrape_time = state.metrics.get_last_scrape_time();

    Ok(Json(MetricsResponse {
        total_markets,
        total_scrapes: state.metrics.get_total_scrapes(),
        successful_scrapes: state.metrics.get_successful_scrapes(),
        failed_scrapes: state.metrics.get_failed_scrapes(),
        last_scrape_time,
    }))
}

/// Get all markets with pagination
async fn markets_handler(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<MarketsResponse>, StatusCode> {
    info!(
        "Fetching markets with limit={}, offset={}",
        params.limit, params.offset
    );

    let (markets, total) = db::get_markets(&state.pool, params.limit, params.offset)
        .await
        .map_err(|e| {
            error!("Database error in markets_handler: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(MarketsResponse {
        markets,
        total,
        limit: params.limit,
        offset: params.offset,
    }))
}

/// Get markets discovered since a given timestamp
async fn new_markets_handler(
    State(state): State<AppState>,
    Query(params): Query<SinceParams>,
) -> Result<Json<Vec<Market>>, StatusCode> {
    info!("Fetching markets discovered since: {}", params.since);

    let markets = db::get_markets_since(&state.pool, params.since)
        .await
        .map_err(|e| {
            error!("Database error in new_markets_handler: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(markets))
}

/// Get a single market by ID
async fn market_by_id_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Market>, StatusCode> {
    info!("Fetching market with ID: {}", id);

    let market = db::get_market_by_id(&state.pool, &id)
        .await
        .map_err(|e| {
            error!("Database error in market_by_id_handler: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    match market {
        Some(m) => Ok(Json(m)),
        None => Err(StatusCode::NOT_FOUND),
    }
}

