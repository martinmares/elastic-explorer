pub mod console;
pub mod dashboard;
pub mod endpoints;
pub mod indices;
pub mod mappings;
pub mod nodes;
pub mod search;
pub mod shards;
pub mod snapshots;

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Redirect},
};
use std::sync::Arc;

/// Root handler - redirect na dashboard
pub async fn index(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let base_path = state.base_path.clone();
    if base_path == "/" {
        Redirect::to("/dashboard")
    } else {
        Redirect::to(&format!("{}/dashboard", base_path))
    }
}

/// Health check endpoint
pub async fn health() -> impl IntoResponse {
    (StatusCode::OK, "OK")
}

pub use endpoints::AppState;
