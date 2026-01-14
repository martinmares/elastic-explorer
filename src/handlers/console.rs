use axum::{
    extract::{Query, State},
    response::{Html, Json},
    http::StatusCode,
    Form,
};
use axum_extra::extract::CookieJar;
use std::sync::Arc;
use askama::Template;
use serde::{Deserialize, Serialize};

use crate::handlers::endpoints::{AppState, get_active_endpoint};
use crate::templates::{ConsoleTemplate, PageContext};
use crate::es::EsClient;
use crate::db::models::CreateConsoleHistory;

#[derive(Debug, Deserialize)]
pub struct ConsoleQuery {
    #[serde(default)]
    pub endpoint_filter: Option<i64>, // Filter historii podle endpoint_id
}

#[derive(Debug, Deserialize)]
pub struct ExecuteRequest {
    pub method: String,
    pub path: String,
    #[serde(default)]
    pub body: String,
}

#[derive(Debug, Serialize)]
pub struct ExecuteResponse {
    pub status: u16,
    pub body: String,
    pub is_json: bool,
}

#[derive(Debug, Serialize)]
pub struct ConsoleData {
    pub history: Vec<ConsoleHistoryWithEndpoint>,
    pub endpoint_filter: Option<i64>,
}

impl ConsoleData {
    pub fn history_json(&self) -> String {
        serde_json::to_string(&self.history).unwrap_or_else(|_| "[]".to_string())
    }
}

#[derive(Debug, Serialize)]
pub struct ConsoleHistoryWithEndpoint {
    pub id: i64,
    pub endpoint_id: i64,
    pub endpoint_name: String,
    pub method: String,
    pub path: String,
    pub body: Option<String>,
    pub response_status: Option<i32>,
    pub created_at: String, // Formatted datetime
}

/// GET /console - Zobrazí Dev Console
pub async fn console_page(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Query(query): Query<ConsoleQuery>,
) -> Result<Html<String>, (StatusCode, String)> {
    let active_endpoint = get_active_endpoint(&state, &jar).await;

    let ctx = PageContext {
        active_endpoint: active_endpoint.clone(),
    };

    // Načti historii (poslední 50 záznamů)
    let history_records = state.db.get_console_history(50, query.endpoint_filter).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Získej všechny endpointy pro mapování ID -> name
    let endpoints = state.db.get_endpoints().await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Převeď historii na format s endpoint_name
    let history: Vec<ConsoleHistoryWithEndpoint> = history_records
        .into_iter()
        .map(|h| {
            let endpoint_name = endpoints
                .iter()
                .find(|ep| ep.id == h.endpoint_id)
                .map(|ep| ep.name.clone())
                .unwrap_or_else(|| format!("Unknown ({})", h.endpoint_id));

            ConsoleHistoryWithEndpoint {
                id: h.id,
                endpoint_id: h.endpoint_id,
                endpoint_name,
                method: h.method,
                path: h.path,
                body: h.body,
                response_status: h.response_status,
                created_at: h.created_at.format("%Y-%m-%d %H:%M:%S").to_string(),
            }
        })
        .collect();

    let data = ConsoleData {
        history,
        endpoint_filter: query.endpoint_filter,
    };

    let template = ConsoleTemplate {
        ctx,
        data: Some(data),
    };

    template.render()
        .map(Html)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

/// POST /console/execute - Vykoná HTTP request přes aktivní endpoint
pub async fn execute_request(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Form(req): Form<ExecuteRequest>,
) -> Result<Json<ExecuteResponse>, (StatusCode, String)> {
    let active_endpoint = get_active_endpoint(&state, &jar).await
        .ok_or((StatusCode::BAD_REQUEST, "No active endpoint selected".to_string()))?;

    // Získej heslo
    let password = state.db.get_endpoint_password(&active_endpoint).await;

    let mut client = EsClient::new(
        active_endpoint.url.clone(),
        active_endpoint.insecure,
        active_endpoint.username.clone(),
        password,
    ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    client.detect_version().await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to connect: {}", e)))?;

    // Vykonej request podle metody - používáme raw metody pro podporu plain text i JSON
    let (status_code, response_body) = match req.method.to_uppercase().as_str() {
        "GET" | "HEAD" => {
            match client.get_raw(&req.path).await {
                Ok((status, body)) => {
                    // Zkus parsovat jako JSON a formatovat
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
                        (status, serde_json::to_string_pretty(&json).unwrap_or(body))
                    } else {
                        // Plain text response
                        (status, body)
                    }
                }
                Err(e) => (500, format!("Error: {}", e)),
            }
        }
        "POST" => {
            let body_json: serde_json::Value = serde_json::from_str(&req.body)
                .unwrap_or(serde_json::json!({}));
            match client.post_raw(&req.path, body_json).await {
                Ok((status, body)) => {
                    // Zkus parsovat jako JSON a formatovat
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
                        (status, serde_json::to_string_pretty(&json).unwrap_or(body))
                    } else {
                        // Plain text response
                        (status, body)
                    }
                }
                Err(e) => (500, format!("Error: {}", e)),
            }
        }
        "PUT" => {
            let body_json: serde_json::Value = serde_json::from_str(&req.body)
                .unwrap_or(serde_json::json!({}));
            match client.put_raw(&req.path, body_json).await {
                Ok((status, body)) => {
                    // Zkus parsovat jako JSON a formatovat
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
                        (status, serde_json::to_string_pretty(&json).unwrap_or(body))
                    } else {
                        // Plain text response
                        (status, body)
                    }
                }
                Err(e) => (500, format!("Error: {}", e)),
            }
        }
        "DELETE" => {
            match client.delete_raw(&req.path).await {
                Ok((status, body)) => {
                    // Zkus parsovat jako JSON a formatovat
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
                        (status, serde_json::to_string_pretty(&json).unwrap_or(body))
                    } else {
                        // Plain text response
                        (status, body)
                    }
                }
                Err(e) => (500, format!("Error: {}", e)),
            }
        }
        _ => {
            return Err((StatusCode::BAD_REQUEST, format!("Unsupported method: {}", req.method)));
        }
    };

    // Detekuj jestli je response JSON
    let is_json = response_body.trim().starts_with('{') || response_body.trim().starts_with('[');

    // Ulož do historie
    let history_entry = CreateConsoleHistory {
        endpoint_id: active_endpoint.id,
        method: req.method.clone(),
        path: req.path.clone(),
        body: if req.body.is_empty() { None } else { Some(req.body.clone()) },
        response_status: Some(status_code as i32),
        response_body: Some(response_body.clone()),
    };

    if let Err(e) = state.db.save_console_history(history_entry).await {
        tracing::warn!("Failed to save console history: {}", e);
    }

    // Cleanup staré záznamy (ponechej pouze 200)
    if let Err(e) = state.db.cleanup_console_history(200).await {
        tracing::warn!("Failed to cleanup console history: {}", e);
    }

    Ok(Json(ExecuteResponse {
        status: status_code,
        body: response_body,
        is_json,
    }))
}
