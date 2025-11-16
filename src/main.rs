use anyhow::Result;
use std::sync::Arc;
use tokio::signal;
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod api;
mod db;
mod metrics;
mod models;
mod scraper;

const DEFAULT_DATABASE_URL: &str = "sqlite:markets.db";
const DEFAULT_API_PORT: u16 = 3000;
const DEFAULT_SCRAPE_INTERVAL_SECS: u64 = 30;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "polymarket_scraper=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Starting Polymarket Scraper Service");

    // Parse command line arguments (simple implementation)
    let args: Vec<String> = std::env::args().collect();
    let database_url = args
        .iter()
        .position(|a| a == "--database-url")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.as_str())
        .unwrap_or(DEFAULT_DATABASE_URL);

    let api_port = args
        .iter()
        .position(|a| a == "--port")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_API_PORT);

    let scrape_interval = args
        .iter()
        .position(|a| a == "--scrape-interval")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_SCRAPE_INTERVAL_SECS);

    // Initialize database
    let pool = db::init_db(database_url).await?;
    let pool_arc = Arc::new(pool);

    // Initialize metrics
    let metrics = Arc::new(metrics::Metrics::new());

    // Clone pool and metrics for scraper
    let scraper_pool = Arc::clone(&pool_arc);
    let scraper_metrics = Arc::clone(&metrics);

    // Spawn scraper task
    let scraper_handle = tokio::spawn(async move {
        if let Err(e) = scraper::run_scraper(scraper_pool, scrape_interval, scraper_metrics).await {
            error!("Scraper task failed: {}", e);
        }
    });

    // Clone metrics for API
    let api_metrics = Arc::clone(&metrics);

    // Create API router
    let app = api::create_router(pool_arc, api_metrics);

    // Create server with graceful shutdown
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", api_port))
        .await
        .map_err(|e| anyhow::anyhow!("Failed to bind to port {}: {}", api_port, e))?;

    info!("API server listening on http://0.0.0.0:{}", api_port);
    info!("Health check available at http://0.0.0.0:{}/health", api_port);

    // Run server with graceful shutdown
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(|e| anyhow::anyhow!("Server error: {}", e))?;

    // Cancel scraper task
    scraper_handle.abort();
    info!("Service shutdown complete");

    Ok(())
}

/// Handle graceful shutdown signal (Ctrl+C)
async fn shutdown_signal() {
    let ctrl_c = async {
        match signal::ctrl_c().await {
            Ok(()) => {
                info!("Received shutdown signal (Ctrl+C)");
            }
            Err(e) => {
                error!("Failed to install Ctrl+C handler: {}", e);
            }
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match signal::unix::signal(signal::unix::SignalKind::terminate()) {
            Ok(mut sig) => {
                sig.recv().await;
                info!("Received terminate signal");
            }
            Err(e) => {
                error!("Failed to install terminate signal handler: {}", e);
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

