pub mod dashboard;
pub mod endpoints;
pub mod nodes;
pub mod indices;
pub mod search;
pub mod shards;
pub mod console;

use axum::{
    response::{IntoResponse, Redirect},
    http::StatusCode,
};

/// Root handler - redirect na dashboard
pub async fn index() -> impl IntoResponse {
    Redirect::to("/dashboard")
}

/// Health check endpoint
pub async fn health() -> impl IntoResponse {
    (StatusCode::OK, "OK")
}

pub use endpoints::AppState;
