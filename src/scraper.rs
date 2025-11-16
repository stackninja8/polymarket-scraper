use anyhow::{Context, Result};
use reqwest::Client;
use sqlx::Pool;
use sqlx::Sqlite;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{error, info, warn};

use crate::db;
use crate::metrics::Metrics;
use crate::models::Market;

// Polymarket API endpoints
const POLYMARKET_BASE_URL: &str = "https://polymarket.com/_next/data";
const DEFAULT_BUILD_ID: &str = "keyXdCWmEdmqkd-AH927v"; // Default build ID from assignment
const MIN_REQUEST_INTERVAL_SECS: u64 = 1; // Rate limiting: minimum 1 second between requests
const MAX_RETRIES: u32 = 3;
const INITIAL_RETRY_DELAY_SECS: u64 = 1;

/// Run the scraper in a loop, fetching markets at specified interval
pub async fn run_scraper(
    pool: Arc<Pool<Sqlite>>,
    scrape_interval_secs: u64,
    metrics: Arc<Metrics>,
) -> Result<()> {
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .context("Failed to create HTTP client")?;

    // Discover build ID once at startup
    info!("Discovering build ID from Polymarket homepage...");
    let build_id = match discover_build_id(&client).await {
        Ok(id) => {
            info!("Successfully discovered build ID: {}", id);
            id
        }
        Err(e) => {
            warn!("Failed to discover build ID dynamically: {}, using default build ID", e);
            DEFAULT_BUILD_ID.to_string()
        }
    };

    let mut interval = tokio::time::interval(Duration::from_secs(scrape_interval_secs));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    // Rate limiter: track last request time
    let mut last_request_time = tokio::time::Instant::now();

    info!("Starting scraper with {} second interval, using build ID: {}", scrape_interval_secs, build_id);

    loop {
        interval.tick().await;

        // Rate limiting: ensure minimum time between requests
        let elapsed = last_request_time.elapsed();
        if elapsed.as_secs() < MIN_REQUEST_INTERVAL_SECS {
            let wait_time = Duration::from_secs(MIN_REQUEST_INTERVAL_SECS) - elapsed;
            sleep(wait_time).await;
        }
        last_request_time = tokio::time::Instant::now();

        match fetch_and_store_markets_with_retry(&client, &pool, &metrics, &build_id).await {
            Ok(new_count) => {
                metrics.record_scrape(true);
                if new_count > 0 {
                    info!("Discovered {} new markets", new_count);
                } else {
                    info!("Scrape completed, no new markets found");
                }
            }
            Err(e) => {
                metrics.record_scrape(false);
                error!("Scraper error after retries: {}", e);
                // Continue running despite errors
            }
        }
    }
}

/// Fetch markets from Polymarket API with retry logic and exponential backoff
async fn fetch_and_store_markets_with_retry(
    client: &Client,
    pool: &Arc<Pool<Sqlite>>,
    _metrics: &Arc<Metrics>,
    build_id: &str,
) -> Result<usize> {
    let mut last_error = None;
    
    for attempt in 0..MAX_RETRIES {
        match fetch_and_store_markets(client, pool, build_id).await {
            Ok(count) => return Ok(count),
            Err(e) => {
                last_error = Some(e);
                if attempt < MAX_RETRIES - 1 {
                    let delay = INITIAL_RETRY_DELAY_SECS * 2_u64.pow(attempt);
                    warn!(
                        "Scrape attempt {} failed, retrying in {} seconds...",
                        attempt + 1,
                        delay
                    );
                    sleep(Duration::from_secs(delay)).await;
                }
            }
        }
    }
    
    // Return the last error, or create a generic error if somehow no error was captured
    Err(last_error.unwrap_or_else(|| {
        anyhow::anyhow!("Failed after {} retry attempts", MAX_RETRIES)
    }))
}

/// Discover the Next.js build ID from Polymarket homepage
async fn discover_build_id(client: &Client) -> Result<String> {
    
    let html = client
        .get("https://polymarket.com")
        .send()
        .await
        .context("Failed to fetch Polymarket homepage")?
        .text()
        .await
        .context("Failed to read homepage HTML")?;

    // Look for build ID in script tags or _next/static paths
    // Pattern: /_next/static/{buildId}/ or __NEXT_DATA__ with buildId
    if let Some(build_id) = extract_build_id_from_html(&html) {
        info!("Discovered build ID: {}", build_id);
        return Ok(build_id);
    }

    warn!("Could not discover build ID, using default: {}", DEFAULT_BUILD_ID);
    Ok(DEFAULT_BUILD_ID.to_string())
}

/// Extract build ID from HTML content
fn extract_build_id_from_html(html: &str) -> Option<String> {
    // Try to find build ID in __NEXT_DATA__ script tag
    if let Some(start) = html.find("__NEXT_DATA__") {
        if let Some(data_start) = html[start..].find('{') {
            let data_str = &html[start + data_start..];
            if let Some(end) = data_str.find("</script>") {
                let json_str = &data_str[..end];
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(json_str) {
                    if let Some(build_id) = json.get("buildId").and_then(|v| v.as_str()) {
                        return Some(build_id.to_string());
                    }
                }
            }
        }
    }

    // Try to find build ID in _next/static/{buildId}/ paths
    // Look for pattern: /_next/static/{buildId}/
    let static_prefix = "/_next/static/";
    let mut search_start = 0;
    while let Some(prefix_pos) = html[search_start..].find(static_prefix) {
        let start_pos = search_start + prefix_pos + static_prefix.len();
        if let Some(slash_pos) = html[start_pos..].find('/') {
            let build_id = &html[start_pos..start_pos + slash_pos];
            // Filter out common non-build-id patterns
            if !build_id.starts_with("chunks") 
                && !build_id.starts_with("css") 
                && !build_id.starts_with("media")
                && build_id.len() > 10 {
                return Some(build_id.to_string());
            }
            search_start = start_pos + slash_pos;
        } else {
            break;
        }
    }

    None
}

/// Try to fetch JSON from Next.js endpoint with a given build ID
async fn try_fetch_with_build_id(
    client: &Client,
    build_id: &str,
) -> Result<Option<serde_json::Value>> {
    let nextjs_url = format!("{}/{}/index.json", POLYMARKET_BASE_URL, build_id);
    info!("Attempting to fetch from Next.js endpoint: {}", nextjs_url);
    
    let response = client
        .get(&nextjs_url)
        .header("Accept", "application/json")
        .send()
        .await;

    match response {
        Ok(resp) if resp.status().is_success() => {
            // Check content type to ensure it's JSON
            let content_type = resp.headers()
                .get("content-type")
                .and_then(|h| h.to_str().ok())
                .unwrap_or("unknown");
            
            if !content_type.contains("application/json") {
                warn!(
                    "Next.js endpoint returned non-JSON content type: {}",
                    content_type
                );
                return Ok(None);
            }

            // Try to parse as JSON
            match resp.json().await {
                Ok(json_value) => {
                    info!("Successfully fetched and parsed JSON from Next.js endpoint with build ID: {}", build_id);
                    Ok(Some(json_value))
                }
                Err(e) => {
                    warn!(
                        "Failed to parse JSON from Next.js endpoint: {}",
                        e
                    );
                    Ok(None)
                }
            }
        }
        Ok(resp) => {
            warn!(
                "Next.js endpoint returned status {} with build ID: {}",
                resp.status(),
                build_id
            );
            Ok(None)
        }
        Err(e) => {
            warn!(
                "Failed to fetch from Next.js endpoint with build ID {}: {}",
                build_id,
                e
            );
            Ok(None)
        }
    }
}

/// Fetch markets from Polymarket API and store new ones
/// Uses the provided build ID (discovered once at startup)
async fn fetch_and_store_markets(
    client: &Client,
    pool: &Arc<Pool<Sqlite>>,
    build_id: &str,
) -> Result<usize> {
    // Fetch with the discovered build ID
    let json = match try_fetch_with_build_id(client, build_id).await? {
        Some(json) => json,
        None => {
            return Err(anyhow::anyhow!(
                "Failed to fetch from Next.js endpoint with build ID: {}",
                build_id
            ));
        }
    };

    // Parse and store markets
    let markets = parse_markets_from_json(&json)?;
    info!("Parsed {} markets from API", markets.len());

    let mut new_count = 0;
    for market in markets {
        match db::upsert_market(pool, &market).await {
            Ok(is_new) => {
                if is_new {
                    new_count += 1;
                    info!(
                        "New market discovered: {} - {}",
                        market.id,
                        market.title
                    );
                }
            }
            Err(e) => {
                warn!("Failed to upsert market {}: {}", market.id, e);
            }
        }
    }

    Ok(new_count)
}

/// Parse markets from Polymarket JSON response
/// Handles Next.js endpoint response structure (pageProps.markets) and direct arrays
fn parse_markets_from_json(json: &serde_json::Value) -> Result<Vec<Market>> {
    let mut markets = Vec::new();

    // Next.js endpoint can return an array directly, or wrapped in pageProps
    if let Some(array) = json.as_array() {
        // Direct array response
        for market_json in array {
            if let Ok(market) = parse_single_market(market_json) {
                markets.push(market);
            }
        }
    } else if let Some(markets_array) = json.get("markets").and_then(|v| v.as_array()) {
        // Wrapped in an object with "markets" key
        for market_json in markets_array {
            if let Ok(market) = parse_single_market(market_json) {
                markets.push(market);
            }
        }
    } else if let Some(page_props) = json.get("pageProps") {
        // Legacy Next.js structure (fallback)
        if let Some(markets_array) = page_props.get("markets").and_then(|v| v.as_array()) {
            for market_json in markets_array {
                if let Ok(market) = parse_single_market(market_json) {
                    markets.push(market);
                }
            }
        }
    }

    Ok(markets)
}

/// Parse a single market from JSON
/// Handles Next.js API response structure with flexible field names
fn parse_single_market(json: &serde_json::Value) -> Result<Market> {
    // ID can be a number or string in the API
    let id = json
        .get("id")
        .and_then(|v| {
            if let Some(s) = v.as_str() {
                Some(s.to_string())
            } else if let Some(n) = v.as_u64() {
                Some(n.to_string())
            } else { v.as_i64().map(|n| n.to_string()) }
        })
        .or_else(|| json.get("slug").and_then(|v| v.as_str()).map(|s| s.to_string()))
        .or_else(|| json.get("marketId").and_then(|v| v.as_str()).map(|s| s.to_string()))
        .or_else(|| json.get("market_slug").and_then(|v| v.as_str()).map(|s| s.to_string()))
        .ok_or_else(|| anyhow::anyhow!("Market missing ID"))?;

    let title = json
        .get("question")
        .or_else(|| json.get("title"))
        .or_else(|| json.get("name"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "Untitled Market".to_string());

    let description = json
        .get("description")
        .or_else(|| json.get("descriptionText"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Extract current price from tokens array (first token's price)
    let current_price = json
        .get("tokens")
        .and_then(|v| v.as_array())
        .and_then(|tokens| tokens.first())
        .and_then(|token| token.get("price"))
        .and_then(|v| v.as_f64())
        .or_else(|| json.get("currentPrice").and_then(|v| v.as_f64()))
        .or_else(|| json.get("price").and_then(|v| v.as_f64()))
        .or_else(|| json.get("probability").and_then(|v| v.as_f64()));

    // Volume can be a number or string
    let volume = json
        .get("volumeNum")
        .or_else(|| json.get("volume"))
        .or_else(|| json.get("totalVolume"))
        .and_then(|v| {
            if let Some(n) = v.as_f64() {
                Some(n)
            } else if let Some(s) = v.as_str() {
                s.parse::<f64>().ok()
            } else {
                None
            }
        });

    // End date
    let end_date = json
        .get("end_date_iso")
        .or_else(|| json.get("endDate"))
        .or_else(|| json.get("end_date"))
        .or_else(|| json.get("endTime"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    Ok(Market {
        id,
        title,
        description,
        current_price,
        volume,
        end_date,
        discovered_at: None,
        updated_at: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_single_market() {
        let json = serde_json::json!({
            "id": "test-market-123",
            "title": "Test Market",
            "description": "A test market",
            "currentPrice": 0.65,
            "volume": 1000.0,
            "endDate": "2024-12-31T23:59:59Z"
        });

        let market = parse_single_market(&json).unwrap();
        assert_eq!(market.id, "test-market-123");
        assert_eq!(market.title, "Test Market");
        assert_eq!(market.description, Some("A test market".to_string()));
        assert_eq!(market.current_price, Some(0.65));
        assert_eq!(market.volume, Some(1000.0));
        assert_eq!(market.end_date, Some("2024-12-31T23:59:59Z".to_string()));
    }

    #[test]
    fn test_parse_market_alternative_fields() {
        let json = serde_json::json!({
            "slug": "alternative-id",
            "question": "Alternative Title",
            "price": 0.75,
            "volumeNum": 500.0
        });

        let market = parse_single_market(&json).unwrap();
        assert_eq!(market.id, "alternative-id");
        assert_eq!(market.title, "Alternative Title");
        assert_eq!(market.current_price, Some(0.75));
        assert_eq!(market.volume, Some(500.0));
    }

    #[test]
    fn test_parse_market_missing_id() {
        let json = serde_json::json!({
            "title": "No ID Market"
        });

        assert!(parse_single_market(&json).is_err());
    }

    #[test]
    fn test_parse_markets_from_json() {
        // Test direct array response (Gamma API format)
        let json = serde_json::json!([
            {
                "id": 1,
                "question": "Market 1",
                "market_slug": "market-1"
            },
            {
                "id": 2,
                "question": "Market 2",
                "market_slug": "market-2"
            }
        ]);

        let markets = parse_markets_from_json(&json).unwrap();
        assert_eq!(markets.len(), 2);
        assert_eq!(markets[0].id, "1");
        assert_eq!(markets[0].title, "Market 1");
        assert_eq!(markets[1].id, "2");
        assert_eq!(markets[1].title, "Market 2");
    }
}

