use axum::{
    extract::State,
    response::Html,
    http::StatusCode,
};
use axum_extra::extract::CookieJar;
use std::sync::Arc;
use askama::Template;

use crate::handlers::endpoints::{AppState, get_active_endpoint};
use crate::templates::{DashboardTemplate, PageContext};
use crate::es::EsClient;
use crate::models::{DashboardData, NodeSummary};

/// GET /dashboard - Zobrazí dashboard
pub async fn dashboard(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
) -> Result<Html<String>, (StatusCode, String)> {
    // Získej aktivní endpoint z cookie
    let active_endpoint = get_active_endpoint(&state, &jar).await;
    let endpoint_name = active_endpoint.as_ref().map(|e| e.name.clone());

    // Pokud je aktivní endpoint, načti data z ES s timeoutem
    let data = if let Some(ref endpoint) = active_endpoint {
        match tokio::time::timeout(
            tokio::time::Duration::from_secs(30),
            load_dashboard_data(&state, endpoint)
        ).await {
            Ok(Ok(d)) => Some(d),
            Ok(Err(e)) => {
                tracing::error!("Failed to load dashboard data: {}", e);
                None
            }
            Err(_) => {
                tracing::error!("Timeout loading dashboard data for endpoint: {}", endpoint.name);
                None
            }
        }
    } else {
        None
    };

    let ctx = PageContext::new(active_endpoint);
    let template = DashboardTemplate { endpoint_name, ctx, data };

    template.render()
        .map(Html)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

async fn load_dashboard_data(
    state: &AppState,
    endpoint: &crate::db::models::Endpoint,
) -> anyhow::Result<DashboardData> {
    let password = state.db.get_endpoint_password(endpoint).await;

    let mut client = EsClient::new(
        endpoint.url.clone(),
        endpoint.insecure,
        endpoint.username.clone(),
        password,
    )?;

    // Detekce verze
    client.detect_version().await?;

    // Získej cluster health
    let health = client.cluster_health().await?;

    // Získej cat indices pro počet indexů a dokumentů
    let indices: Vec<crate::es::api::IndexInfo> = client.get_indices().await?;
    let indices_count = indices.len() as u32;
    let documents_count: u64 = indices.iter()
        .filter_map(|i| i.docs_count.as_ref()?.parse::<u64>().ok())
        .sum();

    // Získej nodes info a stats
    let nodes_response: serde_json::Value = client.get_nodes().await?;
    let nodes_map = nodes_response["nodes"].as_object()
        .ok_or_else(|| anyhow::anyhow!("Invalid nodes response"))?;

    // Získej node stats pro metriky
    let stats_response: serde_json::Value = client.get("/_nodes/stats").await?;
    let stats_map = stats_response["nodes"].as_object();

    // Získej master node ID
    let master_node_id: Option<String> = match client.get::<serde_json::Value>("/_cat/master?format=json").await {
        Ok(master_response) => {
            master_response.as_array()
                .and_then(|arr| arr.first())
                .and_then(|obj| obj["id"].as_str())
                .map(|s| s.to_string())
        }
        Err(_) => None, // Fallback pokud API selže
    };

    let mut nodes = Vec::new();
    for (node_id, node_data) in nodes_map {
        let name = node_data["name"].as_str().unwrap_or("unknown").to_string();
        let roles = node_data["roles"].as_array()
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
            .unwrap_or_else(Vec::new);

        // Získej stats pro tento node
        let (cpu, heap, ram, disk) = if let Some(stats) = stats_map.and_then(|m| m.get(node_id)) {
            let cpu = stats["os"]["cpu"]["percent"].as_u64().map(|v| v as u8);
            let heap = stats["jvm"]["mem"]["heap_used_percent"].as_u64().map(|v| v as u8);

            // RAM percent
            let ram = if let (Some(used), Some(total)) = (
                stats["os"]["mem"]["used_in_bytes"].as_u64(),
                stats["os"]["mem"]["total_in_bytes"].as_u64()
            ) {
                if total > 0 {
                    Some(((used * 100) / total) as u8)
                } else {
                    None
                }
            } else {
                None
            };

            // Disk percent
            let disk = if let (Some(avail), Some(total)) = (
                stats["fs"]["total"]["available_in_bytes"].as_u64(),
                stats["fs"]["total"]["total_in_bytes"].as_u64()
            ) {
                if total > 0 {
                    let used = total - avail;
                    Some(((used * 100) / total) as u8)
                } else {
                    None
                }
            } else {
                None
            };

            (cpu, heap, ram, disk)
        } else {
            (None, None, None, None)
        };

        // Kontrola zda je tento node master
        let is_master = master_node_id.as_ref().map(|mid| mid == node_id).unwrap_or(false);

        nodes.push(NodeSummary {
            id: node_id.clone(),
            name,
            roles,
            is_master,
            cpu_percent: cpu,
            heap_percent: heap,
            ram_percent: ram,
            disk_used_percent: disk,
        });
    }

    Ok(DashboardData {
        cluster_name: health.cluster_name,
        cluster_status: health.status,
        nodes_total: health.number_of_nodes,
        nodes_data: health.number_of_data_nodes,
        indices_count,
        documents_count,
        store_size: "N/A".to_string(), // TODO: spočítat z indices
        nodes,
    })
}
