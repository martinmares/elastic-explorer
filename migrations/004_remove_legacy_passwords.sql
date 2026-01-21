-- Remove legacy password columns from endpoints

BEGIN TRANSACTION;

ALTER TABLE endpoints RENAME TO endpoints_old;

CREATE TABLE endpoints (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    url TEXT NOT NULL,
    insecure BOOLEAN NOT NULL DEFAULT 0,
    username TEXT,
    password_encrypted TEXT,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

INSERT INTO endpoints (id, name, url, insecure, username, password_encrypted, created_at, updated_at)
SELECT id, name, url, insecure, username, NULL, created_at, updated_at
FROM endpoints_old;

DROP TABLE endpoints_old;

CREATE INDEX IF NOT EXISTS idx_endpoints_name ON endpoints(name);

CREATE TRIGGER IF NOT EXISTS update_endpoints_timestamp
AFTER UPDATE ON endpoints
BEGIN
    UPDATE endpoints SET updated_at = CURRENT_TIMESTAMP WHERE id = NEW.id;
END;

COMMIT;
