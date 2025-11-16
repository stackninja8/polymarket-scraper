# Polymarket Scraper

A Rust service that detects new prediction markets from Polymarket, stores them in a SQLite database, and exposes the data via a REST API.

## Features

### Core Features
- **Market Scraper**: Fetches markets from Polymarket API every 30 seconds (configurable)
- **Database Storage**: Stores markets in SQLite with upsert functionality
- **REST API**: Exposes markets via HTTP endpoints with pagination
- **Graceful Shutdown**: Handles Ctrl+C and waits for in-flight requests
- **Structured Logging**: Uses `tracing` for comprehensive logging
- **Error Handling**: Proper error propagation without panics

### Bonus Features ✨
- **Retry Logic**: Exponential backoff retry (3 attempts) for API failures
- **Rate Limiting**: Minimum 1 second between scraper requests
- **Metrics Endpoint**: Track total markets, scrape counts, and last scrape time
- **CLI Flags**: Configurable scrape interval, database URL, and API port
- **Docker Support**: Multi-stage Dockerfile for containerized deployment
- **Unit Tests**: Test coverage for parsing logic and metrics

## Requirements

- Rust 1.75+ (stable)
- SQLite3

## Setup

1. **Install Rust** (if not already installed):
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. **Clone and build the project**:
   ```bash
   cd rust_takehome
   cargo build --release
   ```

3. **Run the service**:
   ```bash
   cargo run
   ```

   Or with custom options:
   ```bash
   cargo run -- --database-url sqlite:custom.db --port 8080 --scrape-interval 60
   ```

### CLI Options

- `--database-url`: Database connection string (default: `sqlite:markets.db`)
- `--port`: API server port (default: `3000`)
- `--scrape-interval`: Scraper interval in seconds (default: `30`)

The database will be created automatically on first run, and migrations will be applied.

### Adding New Migrations

This project uses `sqlx` migrations. To add a new migration:

**Option 1: Using sqlx-cli (recommended)**
1. Install sqlx-cli (if not already installed):
   ```bash
   cargo install sqlx-cli --no-default-features --features sqlite
   ```

2. Create a new migration:
   ```bash
   sqlx migrate add <migration_name>
   ```
   For example:
   ```bash
   sqlx migrate add add_user_preferences
   ```
   This creates a new file in `migrations/` with a timestamp prefix (e.g., `002_add_user_preferences.sql`).

**Option 2: Manual creation**
1. Create a new file in `migrations/` directory following the naming pattern:
   ```
   migrations/002_<migration_name>.sql
   ```
   The number should be sequential (002, 003, 004, etc.).

2. Add your SQL to the file:
   ```sql
   -- migrations/002_add_user_preferences.sql
   ALTER TABLE markets ADD COLUMN user_notes TEXT;
   ```

**Applying Migrations**
- Migrations run **automatically** when the application starts (see `src/db.rs`).
- Or apply manually (requires sqlx-cli):
  ```bash
  sqlx migrate run
  ```

**Note**: Migrations are applied automatically on startup. The application checks for pending migrations and applies them before starting the scraper and API server.

## Usage

### Service Endpoints

The API runs on `http://localhost:3000` by default.

#### Health Check
```bash
curl http://localhost:3000/health
```

Response:
```json
{"status":"ok"}
```

#### Metrics
```bash
curl http://localhost:3000/metrics
```

Response:
```json
{
  "total_markets": 150,
  "total_scrapes": 42,
  "successful_scrapes": 40,
  "failed_scrapes": 2,
  "last_scrape_time": "2024-01-15T10:30:00Z"
}
```

#### Get All Markets (Paginated)
```bash
curl "http://localhost:3000/markets?limit=20&offset=0"
```

Response:
```json
{
  "markets": [
    {
      "id": "market-123",
      "title": "Will X happen?",
      "description": "Market description",
      "current_price": 0.65,
      "volume": 10000.0,
      "end_date": "2024-12-31T23:59:59Z"
    }
  ],
  "total": 100,
  "limit": 20,
  "offset": 0
}
```

#### Get New Markets Since Timestamp
```bash
curl "http://localhost:3000/markets/new?since=2024-01-01T00:00:00Z"
```

Response:
```json
[
  {
    "id": "market-456",
    "title": "New Market",
    ...
  }
]
```

#### Get Single Market by ID
```bash
curl http://localhost:3000/markets/market-123
```

Response:
```json
{
  "id": "market-123",
  "title": "Will X happen?",
  ...
}
```

## Project Structure

```
polymarket-scraper/
├── Cargo.toml              # Dependencies and project config
├── README.md               # This file
├── src/
│   ├── main.rs            # Entry point, orchestrates scraper + API
│   ├── scraper.rs         # Polymarket API fetching logic
│   ├── api.rs             # REST API handlers and routes
│   ├── db.rs              # Database operations
│   └── models.rs          # Data structures and serialization
└── migrations/
    └── 001_create_markets.sql  # Database schema
```

## Design Decisions

### Technology Choices

- **Tokio**: Async runtime for concurrent scraper and API operations
- **Axum**: Lightweight, modern web framework with excellent async support
- **SQLx**: Type-safe SQL with compile-time query checking
- **SQLite**: Simple, file-based database (easy to deploy, can upgrade to PostgreSQL later)
- **Tracing**: Structured logging with environment-based filtering
- **Reqwest**: Async HTTP client for API requests

### Architecture

1. **Concurrent Execution**: Scraper and API run in separate Tokio tasks, sharing the database connection pool via `Arc<Pool>`
2. **Error Handling**: All errors are propagated using `Result<T, E>` and `anyhow::Result` - no `.unwrap()` in production paths
3. **Database Pooling**: Connection pooling ensures efficient database access from both scraper and API
4. **Graceful Shutdown**: Handles SIGINT/SIGTERM, allowing in-flight requests to complete

### Trade-offs

- **SQLite vs PostgreSQL**: Chose SQLite for simplicity and zero-config deployment. In production, would use PostgreSQL for better concurrency and features
- **API Endpoint**: Implements dynamic build ID discovery from Polymarket homepage as required by the assignment. Discovers the build ID once at startup and uses it for all subsequent requests. Falls back to the default build ID (`keyXdCWmEdmqkd-AH927v`) if discovery fails.
- **JSON Parsing**: Polymarket API structure may vary, so implemented flexible parsing that tries multiple field names and handles Next.js response format (`pageProps.markets` or direct arrays)
- **Error Recovery**: Scraper continues running even if individual API calls fail, logging errors instead of crashing
- **Pagination**: Simple offset-based pagination (could be improved with cursor-based pagination for large datasets)

## Improvements for Production

Given more time, I would add:

1. **Prometheus Metrics**: Export metrics in Prometheus format for better observability
2. **PostgreSQL Support**: Detect connection string type and support PostgreSQL in addition to SQLite
3. **Integration Tests**: Add API endpoint tests and database operation tests
4. **Redis Caching**: Cache frequently accessed markets to reduce database load
5. **WebSocket Support**: Real-time push notifications for new market discoveries
6. **Advanced Filtering**: Query parameters for filtering by price range, volume, category
7. **Cursor-based Pagination**: More efficient pagination for large datasets
8. **Configuration File**: TOML/YAML config file for easier configuration management
9. **Health Check Enhancements**: Include database connectivity and API health checks
10. **Request ID Tracing**: Add request IDs for distributed tracing and debugging
11. **API Rate Limiting**: Rate limiting middleware to protect API endpoints
12. **OpenAPI/Swagger**: Auto-generated API documentation

## Docker Deployment

### Build Docker Image
```bash
docker build -t polymarket-scraper .
```

### Run with Docker
**Basic usage** (uses default settings):
```bash
docker run -p 3000:3000 -v $(pwd)/markets.db:/app/markets.db polymarket-scraper
```

**With custom options**:
```bash
docker run -p 8080:8080 \
  -v $(pwd)/custom.db:/app/custom.db \
  polymarket-scraper \
  --database-url sqlite:custom.db \
  --port 8080 \
  --scrape-interval 60
```

**Run in detached mode** (background):
```bash
docker run -d -p 3000:3000 -v $(pwd)/markets.db:/app/markets.db --name polymarket polymarket-scraper
```

### Docker Management
```bash
# Check logs
docker logs -f polymarket

# Stop container
docker stop polymarket

# Restart container
docker restart polymarket

# Remove container
docker rm polymarket
```

## Development

### Running Tests
```bash
cargo test
```

### Running Tests with Output
```bash
cargo test -- --nocapture
```

### Running with Debug Logging
```bash
RUST_LOG=polymarket_scraper=debug cargo run
```

### Database Inspection
```bash
sqlite3 markets.db
sqlite> SELECT COUNT(*) FROM markets;
sqlite> SELECT * FROM markets ORDER BY discovered_at DESC LIMIT 10;
```

## License

This project is a take-home assignment implementation.

