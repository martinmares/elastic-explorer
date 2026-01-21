-- Initial schema for elastic-explorer

CREATE TABLE IF NOT EXISTS endpoints (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    url TEXT NOT NULL,
    insecure BOOLEAN NOT NULL DEFAULT 0,
    username TEXT,
    -- Encrypted password (base64 nonce+ciphertext)
    password_encrypted TEXT,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Index na name pro rychlejší vyhledávání
CREATE INDEX IF NOT EXISTS idx_endpoints_name ON endpoints(name);

-- Trigger pro automatickou aktualizaci updated_at
CREATE TRIGGER IF NOT EXISTS update_endpoints_timestamp
AFTER UPDATE ON endpoints
BEGIN
    UPDATE endpoints SET updated_at = CURRENT_TIMESTAMP WHERE id = NEW.id;
END;

-- Tabulka pro uložené search queries (bookmarks)
CREATE TABLE IF NOT EXISTS saved_queries (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    query_type TEXT NOT NULL CHECK(query_type IN ('dsl', 'sql')),
    query_body TEXT NOT NULL,
    indices TEXT, -- JSON array indexů
    description TEXT,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_saved_queries_name ON saved_queries(name);

CREATE TRIGGER IF NOT EXISTS update_saved_queries_timestamp
AFTER UPDATE ON saved_queries
BEGIN
    UPDATE saved_queries SET updated_at = CURRENT_TIMESTAMP WHERE id = NEW.id;
END;
