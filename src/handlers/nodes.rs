use axum::{
    extract::{Path, State},
    response::{Html, Json},
    http::StatusCode,
};
use axum_extra::extract::CookieJar;
use std::sync::Arc;
use askama::Template;
use serde::Serialize;

use crate::handlers::endpoints::{AppState, get_active_endpoint};
use crate::templates::{NodeDetailTemplate, PageContext};
use crate::es::EsClient;
use crate::models::NodeDetail;

#[derive(Debug, Serialize)]
pub struct NodeMetrics {
    pub cpu_percent: Option<u8>,
    pub heap_percent: Option<u8>,
    pub ram_percent: Option<u8>,
    pub disk_percent: Option<u8>,
}

/// GET /nodes/{id} - Zobrazí detail nodu
pub async fn node_detail(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Path(node_id): Path<String>,
) -> Result<Html<String>, (StatusCode, String)> {
    // Získej aktivní endpoint z cookie
    let active_endpoint = get_active_endpoint(&state, &jar).await;

    if active_endpoint.is_none() {
        return Err((StatusCode::BAD_REQUEST, "No active endpoint selected".to_string()));
    }

    let endpoint = active_endpoint.as_ref().unwrap();

    // Načti data o nodu s timeoutem
    let data = match tokio::time::timeout(
        tokio::time::Duration::from_secs(30),
        load_node_detail(&state, endpoint, &node_id)
    ).await {
        Ok(Ok(d)) => Some(d),
        Ok(Err(e)) => {
            if e.to_string().contains("Node not found") {
                tracing::debug!("Node detail not found for: {}", node_id);
            } else {
                tracing::error!("Failed to load node detail: {}", e);
            }
            None
        }
        Err(_) => {
            tracing::error!("Timeout loading node detail for: {}", node_id);
            None
        }
    };

    let ctx = PageContext::new(active_endpoint);
    let template = NodeDetailTemplate { ctx, data, node_id };

    template.render()
        .map(Html)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

async fn load_node_detail(
    state: &AppState,
    endpoint: &crate::db::models::Endpoint,
    node_id: &str,
) -> anyhow::Result<NodeDetail> {
    let password = state.db.get_endpoint_password(endpoint).await;

    let mut client = EsClient::new(
        endpoint.url.clone(),
        endpoint.insecure,
        endpoint.username.clone(),
        password,
    )?;

    // Detekce verze
    client.detect_version().await?;

    // Získej node info
    let nodes_response: serde_json::Value = client.get("/_nodes").await?;
    let node_data = &nodes_response["nodes"][node_id];

    if node_data.is_null() {
        return Err(anyhow::anyhow!("Node not found"));
    }

    let name = node_data["name"].as_str().unwrap_or("unknown").to_string();
    let roles = node_data["roles"].as_array()
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default();

    let ip = node_data["ip"].as_str().unwrap_or("-").to_string();
    let version = node_data["version"].as_str().unwrap_or("-").to_string();
    let os_name = node_data["os"]["name"].as_str().unwrap_or("-").to_string();
    let os_arch = node_data["os"]["arch"].as_str().unwrap_or("-").to_string();
    let jvm_version = node_data["jvm"]["version"].as_str().unwrap_or("-").to_string();

    // Získej node stats
    let stats_response: serde_json::Value = client.get(&format!("/_nodes/{}/stats", node_id)).await?;
    let stats = &stats_response["nodes"][node_id];

    // CPU
    let cpu_percent = stats["os"]["cpu"]["percent"].as_u64().map(|v| v as u8);

    // JVM Heap
    let heap_percent = stats["jvm"]["mem"]["heap_used_percent"].as_u64().map(|v| v as u8);
    let heap_used = stats["jvm"]["mem"]["heap_used_in_bytes"].as_u64().unwrap_or(0);
    let heap_max = stats["jvm"]["mem"]["heap_max_in_bytes"].as_u64().unwrap_or(0);

    // RAM
    let ram_used = stats["os"]["mem"]["used_in_bytes"].as_u64().unwrap_or(0);
    let ram_total = stats["os"]["mem"]["total_in_bytes"].as_u64().unwrap_or(0);
    let ram_percent = if ram_total > 0 {
        Some(((ram_used * 100) / ram_total) as u8)
    } else {
        None
    };

    // Disk
    let disk_available = stats["fs"]["total"]["available_in_bytes"].as_u64().unwrap_or(0);
    let disk_total = stats["fs"]["total"]["total_in_bytes"].as_u64().unwrap_or(0);
    let disk_used = disk_total - disk_available;
    let disk_percent = if disk_total > 0 {
        Some(((disk_used * 100) / disk_total) as u8)
    } else {
        None
    };

    // Documents
    let docs_count = stats["indices"]["docs"]["count"].as_u64().unwrap_or(0);
    let docs_deleted = stats["indices"]["docs"]["deleted"].as_u64().unwrap_or(0);

    // Store
    let store_size = stats["indices"]["store"]["size_in_bytes"].as_u64().unwrap_or(0);

    Ok(NodeDetail {
        id: node_id.to_string(),
        name,
        roles,
        ip,
        version,
        os_name,
        os_arch,
        jvm_version,
        cpu_percent,
        heap_percent,
        heap_used_bytes: heap_used,
        heap_max_bytes: heap_max,
        ram_percent,
        ram_used_bytes: ram_used,
        ram_total_bytes: ram_total,
        disk_percent,
        disk_used_bytes: disk_used,
        disk_total_bytes: disk_total,
        docs_count,
        docs_deleted,
        store_size_bytes: store_size,
    })
}

/// GET /nodes/{id}/metrics - Vrátí aktuální metriky nodu jako JSON
pub async fn node_metrics(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Path(node_id): Path<String>,
) -> Result<Json<NodeMetrics>, (StatusCode, String)> {
    // Získej aktivní endpoint z cookie
    let active_endpoint = get_active_endpoint(&state, &jar).await;

    if active_endpoint.is_none() {
        return Err((StatusCode::BAD_REQUEST, "No active endpoint selected".to_string()));
    }

    let endpoint = active_endpoint.as_ref().unwrap();

    // Načti pouze metriky s timeoutem
    let metrics = match tokio::time::timeout(
        tokio::time::Duration::from_secs(30),
        load_node_metrics(&state, endpoint, &node_id)
    ).await {
        Ok(Ok(m)) => m,
        Ok(Err(e)) => {
            if e.to_string().contains("Node not found") {
                tracing::debug!("Node metrics not found for: {}", node_id);
                return Err((StatusCode::NOT_FOUND, "Node not found".to_string()));
            }
            tracing::error!("Failed to load node metrics: {}", e);
            return Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string()));
        }
        Err(_) => {
            tracing::error!("Timeout loading node metrics for: {}", node_id);
            return Err((StatusCode::REQUEST_TIMEOUT, "Timeout".to_string()));
        }
    };

    Ok(Json(metrics))
}

async fn load_node_metrics(
    state: &AppState,
    endpoint: &crate::db::models::Endpoint,
    node_id: &str,
) -> anyhow::Result<NodeMetrics> {
    let password = state.db.get_endpoint_password(endpoint).await;

    let client = EsClient::new(
        endpoint.url.clone(),
        endpoint.insecure,
        endpoint.username.clone(),
        password,
    )?;

    // Získej node stats
    let stats_response: serde_json::Value = client.get(&format!("/_nodes/{}/stats", node_id)).await?;
    let stats = &stats_response["nodes"][node_id];

    if stats.is_null() {
        return Err(anyhow::anyhow!("Node not found"));
    }

    // CPU
    let cpu_percent = stats["os"]["cpu"]["percent"].as_u64().map(|v| v as u8);

    // JVM Heap
    let heap_percent = stats["jvm"]["mem"]["heap_used_percent"].as_u64().map(|v| v as u8);

    // RAM
    let ram_used = stats["os"]["mem"]["used_in_bytes"].as_u64().unwrap_or(0);
    let ram_total = stats["os"]["mem"]["total_in_bytes"].as_u64().unwrap_or(0);
    let ram_percent = if ram_total > 0 {
        Some(((ram_used * 100) / ram_total) as u8)
    } else {
        None
    };

    // Disk
    let disk_available = stats["fs"]["total"]["available_in_bytes"].as_u64().unwrap_or(0);
    let disk_total = stats["fs"]["total"]["total_in_bytes"].as_u64().unwrap_or(0);
    let disk_used = disk_total - disk_available;
    let disk_percent = if disk_total > 0 {
        Some(((disk_used * 100) / disk_total) as u8)
    } else {
        None
    };

    Ok(NodeMetrics {
        cpu_percent,
        heap_percent,
        ram_percent,
        disk_percent,
    })
}
