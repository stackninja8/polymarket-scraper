use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// Market data structure representing a prediction market from Polymarket
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Market {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub current_price: Option<f64>,
    pub volume: Option<f64>,
    pub end_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discovered_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
}

/// Response structure for paginated market lists
#[derive(Debug, Serialize)]
pub struct MarketsResponse {
    pub markets: Vec<Market>,
    pub total: i64,
    pub limit: u32,
    pub offset: u32,
}

/// Health check response
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
}

/// Metrics response
#[derive(Debug, Serialize)]
pub struct MetricsResponse {
    pub total_markets: i64,
    pub total_scrapes: u64,
    pub successful_scrapes: u64,
    pub failed_scrapes: u64,
    pub last_scrape_time: Option<chrono::DateTime<Utc>>,
}

/// Polymarket API response structure (simplified - actual structure may vary)
/// These structs are kept for potential future use with typed deserialization
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct PolymarketResponse {
    #[serde(rename = "pageProps")]
    pub page_props: Option<PageProps>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct PageProps {
    pub markets: Option<Vec<PolymarketMarket>>,
}

#[derive(Debug, Deserialize)]
pub struct PolymarketMarket {
    pub id: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    #[serde(rename = "currentPrice")]
    pub current_price: Option<f64>,
    pub volume: Option<f64>,
    #[serde(rename = "endDate")]
    pub end_date: Option<String>,
    // Polymarket API may have different field names, so we'll need to adapt
}

impl From<PolymarketMarket> for Market {
    fn from(pm: PolymarketMarket) -> Self {
        Market {
            id: pm.id.unwrap_or_default(),
            title: pm.title.unwrap_or_default(),
            description: pm.description,
            current_price: pm.current_price,
            volume: pm.volume,
            end_date: pm.end_date,
            discovered_at: None,
            updated_at: None,
        }
    }
}
