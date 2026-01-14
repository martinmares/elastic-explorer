pub mod credentials;
pub mod models;

use anyhow::{Context, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::str::FromStr;
use base64::Engine;

use crate::config;
use crate::db::credentials::{delete_password, generate_keychain_id, store_password};
use crate::db::models::{CreateEndpoint, Endpoint, SavedQuery, ConsoleHistory, CreateConsoleHistory};

pub struct Database {
    pool: SqlitePool,
}

impl Database {
    /// Vytvoří novou instanci databáze a provede migrace
    pub async fn new() -> Result<Self> {
        let db_path = config::get_db_path()?;
        let db_url = format!("sqlite://{}", db_path.display());

        tracing::info!("Connecting to database: {}", db_url);

        let options = SqliteConnectOptions::from_str(&db_url)?
            .create_if_missing(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await
            .context("Failed to connect to database")?;

        // Spustí migrace
        Self::run_migrations(&pool).await?;

        Ok(Self { pool })
    }

    /// Spustí SQL migrace
    async fn run_migrations(pool: &SqlitePool) -> Result<()> {
        tracing::info!("Running database migrations...");

        // Migration 001 - Initial schema
        let migration_001 = include_str!("../../migrations/001_init.sql");
        sqlx::raw_sql(migration_001)
            .execute(pool)
            .await
            .context("Failed to run migration 001")?;

        // Migration 002 - Add password fallback (ignore if column already exists)
        let migration_002 = include_str!("../../migrations/002_add_password_fallback.sql");
        match sqlx::raw_sql(migration_002).execute(pool).await {
            Ok(_) => {
                tracing::debug!("Migration 002 applied successfully");
            }
            Err(e) => {
                // Ignore "duplicate column" error - column already exists
                let err_msg = e.to_string();
                if err_msg.contains("duplicate column") {
                    tracing::debug!("Migration 002 skipped - column already exists");
                } else {
                    return Err(e).context("Failed to run migration 002");
                }
            }
        }

        // Migration 003 - Console history
        let migration_003 = include_str!("../../migrations/003_console_history.sql");
        sqlx::raw_sql(migration_003)
            .execute(pool)
            .await
            .context("Failed to run migration 003")?;

        tracing::info!("Migrations completed successfully");
        Ok(())
    }

    /// Získá všechny endpointy
    pub async fn get_endpoints(&self) -> Result<Vec<Endpoint>> {
        let endpoints = sqlx::query_as::<_, Endpoint>(
            "SELECT * FROM endpoints ORDER BY name"
        )
        .fetch_all(&self.pool)
        .await
        .context("Failed to fetch endpoints")?;

        Ok(endpoints)
    }

    /// Získá endpoint podle ID
    pub async fn get_endpoint(&self, id: i64) -> Result<Option<Endpoint>> {
        let endpoint = sqlx::query_as::<_, Endpoint>(
            "SELECT * FROM endpoints WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to fetch endpoint")?;

        Ok(endpoint)
    }

    /// Vytvoří nový endpoint
    pub async fn create_endpoint(&self, endpoint: CreateEndpoint) -> Result<i64> {
        let mut tx = self.pool.begin().await?;

        let result = sqlx::query(
            "INSERT INTO endpoints (name, url, insecure, username, password_keychain_id, password_fallback)
             VALUES (?, ?, ?, ?, ?, ?)"
        )
        .bind(&endpoint.name)
        .bind(&endpoint.url)
        .bind(endpoint.insecure)
        .bind(&endpoint.username)
        .bind::<Option<String>>(None) // Keychain ID nastavíme později
        .bind::<Option<String>>(None) // Fallback password nastavíme později
        .execute(&mut *tx)
        .await
        .context("Failed to insert endpoint")?;

        let endpoint_id = result.last_insert_rowid();

        // Pokud je heslo, ulož ho do keychain a jako fallback do DB
        if let Some(password) = endpoint.password {
            let keychain_id = generate_keychain_id(endpoint_id);

            // Zkus uložit do keychain
            match store_password(&keychain_id, &password) {
                Ok(_) => {
                    // Aktualizuj endpoint s keychain_id
                    sqlx::query("UPDATE endpoints SET password_keychain_id = ? WHERE id = ?")
                        .bind(&keychain_id)
                        .bind(endpoint_id)
                        .execute(&mut *tx)
                        .await?;

                    tracing::info!("Password stored in keychain for endpoint {}", endpoint_id);
                }
                Err(e) => {
                    tracing::warn!("Failed to store password in keychain: {}. Using fallback.", e);
                }
            }

            // VŽDY ulož jako fallback (base64)
            let fallback = base64::prelude::BASE64_STANDARD.encode(&password);
            sqlx::query("UPDATE endpoints SET password_fallback = ? WHERE id = ?")
                .bind(&fallback)
                .bind(endpoint_id)
                .execute(&mut *tx)
                .await?;

            tracing::debug!("Password fallback stored for endpoint {}", endpoint_id);
        }

        tx.commit().await?;

        tracing::info!("Created endpoint: {} (id: {})", endpoint.name, endpoint_id);
        Ok(endpoint_id)
    }

    /// Smaže endpoint
    pub async fn delete_endpoint(&self, id: i64) -> Result<()> {
        // Nejdříve získej endpoint pro keychain_id
        if let Some(endpoint) = self.get_endpoint(id).await? {
            // Smaž heslo z keychain
            if let Some(keychain_id) = endpoint.password_keychain_id {
                if let Err(e) = delete_password(&keychain_id) {
                    tracing::warn!("Failed to delete password from keychain: {}", e);
                }
            }
        }

        // Smaž endpoint z DB
        sqlx::query("DELETE FROM endpoints WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .context("Failed to delete endpoint")?;

        tracing::info!("Deleted endpoint: {}", id);
        Ok(())
    }

    /// Získá heslo pro endpoint (zkusí keychain, pak fallback)
    pub async fn get_endpoint_password(&self, endpoint: &Endpoint) -> Option<String> {
        // Zkus keychain nejprve
        if let Some(ref keychain_id) = endpoint.password_keychain_id {
            match credentials::get_password(keychain_id) {
                Ok(password) => {
                    tracing::debug!("Password retrieved from keychain for endpoint {}", endpoint.id);
                    return Some(password);
                }
                Err(e) => {
                    tracing::warn!("Failed to retrieve password from keychain for endpoint {}: {}. Trying fallback.", endpoint.id, e);
                }
            }
        }

        // Fallback na DB (base64 decoded)
        if let Some(ref fallback) = endpoint.password_fallback {
            match base64::prelude::BASE64_STANDARD.decode(fallback) {
                Ok(decoded_bytes) => {
                    match String::from_utf8(decoded_bytes) {
                        Ok(password) => {
                            tracing::info!("Password retrieved from fallback for endpoint {}", endpoint.id);
                            return Some(password);
                        }
                        Err(e) => {
                            tracing::error!("Failed to decode fallback password for endpoint {}: {}", endpoint.id, e);
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to decode base64 fallback for endpoint {}: {}", endpoint.id, e);
                }
            }
        }

        tracing::warn!("No password available for endpoint {}", endpoint.id);
        None
    }

    /// Získá všechny uložené queries
    pub async fn get_saved_queries(&self) -> Result<Vec<SavedQuery>> {
        let queries = sqlx::query_as::<_, SavedQuery>(
            "SELECT * FROM saved_queries ORDER BY name"
        )
        .fetch_all(&self.pool)
        .await
        .context("Failed to fetch saved queries")?;

        Ok(queries)
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Uloží záznam do console history
    /// Pokud již existuje stejné query (endpoint_id, method, path, body), pouze aktualizuje timestamp a response
    pub async fn save_console_history(&self, history: CreateConsoleHistory) -> Result<i64> {
        // Pokus se najít existující záznam se stejným obsahem
        let existing = sqlx::query_scalar::<_, i64>(
            "SELECT id FROM console_history
             WHERE endpoint_id = ? AND method = ? AND path = ? AND (body IS ? OR (body IS NULL AND ? IS NULL))
             LIMIT 1"
        )
        .bind(history.endpoint_id)
        .bind(&history.method)
        .bind(&history.path)
        .bind(&history.body)
        .bind(&history.body)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to check existing console history")?;

        if let Some(existing_id) = existing {
            // Update existujícího záznamu (obnoví timestamp a response)
            sqlx::query(
                "UPDATE console_history
                 SET response_status = ?, response_body = ?, created_at = CURRENT_TIMESTAMP
                 WHERE id = ?"
            )
            .bind(history.response_status)
            .bind(&history.response_body)
            .bind(existing_id)
            .execute(&self.pool)
            .await
            .context("Failed to update console history")?;

            Ok(existing_id)
        } else {
            // Insert nového záznamu
            let result = sqlx::query(
                "INSERT INTO console_history (endpoint_id, method, path, body, response_status, response_body)
                 VALUES (?, ?, ?, ?, ?, ?)"
            )
            .bind(history.endpoint_id)
            .bind(&history.method)
            .bind(&history.path)
            .bind(&history.body)
            .bind(history.response_status)
            .bind(&history.response_body)
            .execute(&self.pool)
            .await
            .context("Failed to save console history")?;

            Ok(result.last_insert_rowid())
        }
    }

    /// Získá historii console dotazů (poslední N záznamů)
    pub async fn get_console_history(&self, limit: i64, endpoint_id: Option<i64>) -> Result<Vec<ConsoleHistory>> {
        let histories = if let Some(ep_id) = endpoint_id {
            sqlx::query_as::<_, ConsoleHistory>(
                "SELECT * FROM console_history WHERE endpoint_id = ? ORDER BY created_at DESC LIMIT ?"
            )
            .bind(ep_id)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .context("Failed to fetch console history for endpoint")?
        } else {
            sqlx::query_as::<_, ConsoleHistory>(
                "SELECT * FROM console_history ORDER BY created_at DESC LIMIT ?"
            )
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .context("Failed to fetch console history")?
        };

        Ok(histories)
    }

    /// Smaže starou historii (ponechá pouze poslední N záznamů)
    pub async fn cleanup_console_history(&self, keep_last: i64) -> Result<()> {
        sqlx::query(
            "DELETE FROM console_history
             WHERE id NOT IN (
                 SELECT id FROM console_history ORDER BY created_at DESC LIMIT ?
             )"
        )
        .bind(keep_last)
        .execute(&self.pool)
        .await
        .context("Failed to cleanup console history")?;

        Ok(())
    }
}
