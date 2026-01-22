pub mod models;

use anyhow::{Context, Result};
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use rand::TryRngCore;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use sqlx::Row;
use std::str::FromStr;
use base64::Engine;

use crate::config;
use crate::db::models::{CreateEndpoint, Endpoint, UpdateEndpoint, SavedQuery, ConsoleHistory, CreateConsoleHistory};

pub struct Database {
    pool: SqlitePool,
    encryption_key: [u8; 32],
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

        let encryption_key = config::load_or_create_key()?
            .try_into()
            .map_err(|_| anyhow::anyhow!("Invalid encryption key length"))?;

        Ok(Self { pool, encryption_key })
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

        // Migration 003 - Console history
        let migration_003 = include_str!("../../migrations/003_console_history.sql");
        sqlx::raw_sql(migration_003)
            .execute(pool)
            .await
            .context("Failed to run migration 003")?;

        // Migration 004 - Remove legacy password columns (run only if needed)
        let columns = sqlx::query("PRAGMA table_info(endpoints)")
            .fetch_all(pool)
            .await
            .context("Failed to inspect endpoints schema")?;
        let mut has_legacy_columns = false;
        let mut has_encrypted_column = false;
        for row in &columns {
            let name: String = row.get("name");
            if name == "password_keychain_id" || name == "password_fallback" {
                has_legacy_columns = true;
            }
            if name == "password_encrypted" {
                has_encrypted_column = true;
            }
        }

        if has_legacy_columns || !has_encrypted_column {
            let migration_004 = include_str!("../../migrations/004_remove_legacy_passwords.sql");
            sqlx::raw_sql(migration_004)
                .execute(pool)
                .await
                .context("Failed to run migration 004")?;
        }

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
            "INSERT INTO endpoints (name, url, insecure, username, password_encrypted)
             VALUES (?, ?, ?, ?, ?)"
        )
        .bind(&endpoint.name)
        .bind(&endpoint.url)
        .bind(endpoint.insecure)
        .bind(&endpoint.username)
        .bind::<Option<String>>(None)
        .execute(&mut *tx)
        .await
        .context("Failed to insert endpoint")?;

        let endpoint_id = result.last_insert_rowid();

        // Pokud je heslo, ulož ho šifrovaně do DB
        if let Some(password) = endpoint.password {
            let encrypted = self.encrypt_password(&password)?;
            sqlx::query("UPDATE endpoints SET password_encrypted = ? WHERE id = ?")
                .bind(&encrypted)
                .bind(endpoint_id)
                .execute(&mut *tx)
                .await?;
        }

        tx.commit().await?;

        tracing::info!("Created endpoint: {} (id: {})", endpoint.name, endpoint_id);
        Ok(endpoint_id)
    }

    /// Smaže endpoint
    pub async fn delete_endpoint(&self, id: i64) -> Result<()> {
        // Smaž endpoint z DB
        sqlx::query("DELETE FROM endpoints WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .context("Failed to delete endpoint")?;

        tracing::info!("Deleted endpoint: {}", id);
        Ok(())
    }

    /// Aktualizuje endpoint
    pub async fn update_endpoint(&self, id: i64, endpoint: UpdateEndpoint) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        let name = endpoint.name.context("Missing endpoint name")?;
        let url = endpoint.url.context("Missing endpoint url")?;
        let insecure = endpoint.insecure.context("Missing endpoint insecure flag")?;

        sqlx::query(
            "UPDATE endpoints
             SET name = ?, url = ?, insecure = ?, username = ?, updated_at = CURRENT_TIMESTAMP
             WHERE id = ?"
        )
        .bind(name)
        .bind(url)
        .bind(insecure)
        .bind(endpoint.username)
        .bind(id)
        .execute(&mut *tx)
        .await
        .context("Failed to update endpoint")?;

        if let Some(password) = endpoint.password {
            let encrypted = self.encrypt_password(&password)?;
            sqlx::query("UPDATE endpoints SET password_encrypted = ? WHERE id = ?")
                .bind(&encrypted)
                .bind(id)
                .execute(&mut *tx)
                .await
                .context("Failed to update endpoint password")?;
        }

        tx.commit().await?;
        tracing::info!("Updated endpoint: {}", id);
        Ok(())
    }

    /// Získá heslo pro endpoint (dešifruje z DB)
    pub async fn get_endpoint_password(&self, endpoint: &Endpoint) -> Option<String> {
        if let Some(ref encrypted) = endpoint.password_encrypted {
            match self.decrypt_password(encrypted) {
                Ok(password) => return Some(password),
                Err(e) => {
                    tracing::warn!(
                        "Failed to decrypt password for endpoint {}: {}",
                        endpoint.id,
                        e
                    );
                }
            }
        }

        if endpoint.username.is_some() {
            tracing::warn!("No password available for endpoint {}", endpoint.id);
        } else {
            tracing::debug!("No password available for endpoint {}", endpoint.id);
        }
        None
    }

    fn encrypt_password(&self, password: &str) -> Result<String> {
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&self.encryption_key));
        let mut nonce_bytes = [0u8; 12];
        let mut rng = rand::rngs::OsRng;
        rng.try_fill_bytes(&mut nonce_bytes)
            .context("Failed to generate encryption nonce")?;
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = cipher
            .encrypt(nonce, password.as_bytes())
            .map_err(|_| anyhow::anyhow!("Failed to encrypt password"))?;

        let mut payload = Vec::with_capacity(nonce_bytes.len() + ciphertext.len());
        payload.extend_from_slice(&nonce_bytes);
        payload.extend_from_slice(&ciphertext);
        Ok(base64::prelude::BASE64_STANDARD.encode(payload))
    }

    fn decrypt_password(&self, encrypted: &str) -> Result<String> {
        let payload = base64::prelude::BASE64_STANDARD
            .decode(encrypted)
            .context("Failed to decode encrypted password")?;
        if payload.len() < 12 {
            return Err(anyhow::anyhow!("Encrypted password payload is too short"));
        }
        let (nonce_bytes, ciphertext) = payload.split_at(12);
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&self.encryption_key));
        let nonce = Nonce::from_slice(nonce_bytes);
        let plaintext = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|_| anyhow::anyhow!("Failed to decrypt password"))?;
        let password = String::from_utf8(plaintext)
            .context("Decrypted password is not valid UTF-8")?;
        Ok(password)
    }

    /// Získá všechny uložené queries
    #[allow(dead_code)]
    pub async fn get_saved_queries(&self) -> Result<Vec<SavedQuery>> {
        let queries = sqlx::query_as::<_, SavedQuery>(
            "SELECT * FROM saved_queries ORDER BY name"
        )
        .fetch_all(&self.pool)
        .await
        .context("Failed to fetch saved queries")?;

        Ok(queries)
    }

    #[allow(dead_code)]
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
