-- Console history for Dev Console
-- Stores HTTP requests made through the console with endpoint context

CREATE TABLE IF NOT EXISTS console_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    endpoint_id INTEGER NOT NULL,
    method TEXT NOT NULL CHECK(method IN ('GET', 'POST', 'PUT', 'DELETE', 'PATCH', 'HEAD')),
    path TEXT NOT NULL,
    body TEXT, -- JSON or request body (NULL for GET/DELETE)
    response_status INTEGER, -- HTTP status code (e.g., 200, 404)
    response_body TEXT, -- Response content (can be large JSON)
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (endpoint_id) REFERENCES endpoints(id) ON DELETE CASCADE
);

-- Index pro rychlé načítání historie podle endpointu
CREATE INDEX IF NOT EXISTS idx_console_history_endpoint ON console_history(endpoint_id, created_at DESC);

-- Index pro rychlé načítání nejnovější historie
CREATE INDEX IF NOT EXISTS idx_console_history_created ON console_history(created_at DESC);
