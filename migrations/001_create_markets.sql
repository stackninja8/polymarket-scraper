CREATE TABLE IF NOT EXISTS markets (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    description TEXT,
    current_price REAL,
    volume REAL,
    end_date TEXT,
    discovered_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_markets_discovered_at ON markets(discovered_at);
CREATE INDEX IF NOT EXISTS idx_markets_updated_at ON markets(updated_at);

