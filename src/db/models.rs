use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Endpoint {
    pub id: i64,
    pub name: String,
    pub url: String,
    pub insecure: bool,
    pub username: Option<String>,
    pub password_encrypted: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateEndpoint {
    pub name: String,
    pub url: String,
    pub insecure: bool,
    pub username: Option<String>,
    pub password: Option<String>, // Toto se uloží šifrovaně do DB
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateEndpoint {
    pub name: Option<String>,
    pub url: Option<String>,
    pub insecure: Option<bool>,
    pub username: Option<String>,
    pub password: Option<String>, // Pokud je Some, aktualizuj šifrované heslo
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SavedQuery {
    pub id: i64,
    pub name: String,
    pub query_type: String, // 'dsl' nebo 'sql'
    pub query_body: String,
    pub indices: Option<String>, // JSON array
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSavedQuery {
    pub name: String,
    pub query_type: String,
    pub query_body: String,
    pub indices: Option<Vec<String>>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ConsoleHistory {
    pub id: i64,
    pub endpoint_id: i64,
    pub method: String,
    pub path: String,
    pub body: Option<String>,
    pub response_status: Option<i32>,
    pub response_body: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateConsoleHistory {
    pub endpoint_id: i64,
    pub method: String,
    pub path: String,
    pub body: Option<String>,
    pub response_status: Option<i32>,
    pub response_body: Option<String>,
}
