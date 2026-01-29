pub mod dashboard;
pub mod endpoints;
pub mod nodes;
pub mod indices;
pub mod search;
pub mod shards;
pub mod console;

use axum::{
    extract::State,
    response::{IntoResponse, Redirect},
    http::StatusCode,
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
