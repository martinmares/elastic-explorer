use axum::{
    extract::{Query, State},
    response::Html,
    http::StatusCode,
};
use axum_extra::extract::CookieJar;
use std::sync::Arc;
use std::collections::HashMap;
use askama::Template;
use serde::{Deserialize, Serialize};

use crate::handlers::endpoints::{AppState, get_active_endpoint};
use crate::templates::{ShardsTemplate, PageContext};
use crate::es::EsClient;
use crate::utils::{generate_index_color, shard_state_color, get_text_color_for_background};

#[derive(Debug, Deserialize)]
pub struct ShardsQuery {
    #[serde(default)]
    pub pattern: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct ShardInfo {
    pub index: String,
    pub shard: String,
    pub prirep: String, // "p" or "r"
    pub state: String,  // "STARTED", "RELOCATING", etc.
    pub docs: String,
    pub store: String,
    pub node: String,
    pub unassigned_reason: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct NodeShards {
    pub node_name: String,
    pub shards: Vec<ShardInfo>,
}

#[derive(Debug, Serialize)]
pub struct ShardsData {
    pub pattern: String,
    pub nodes: Vec<NodeShards>,
    pub all_shards: Vec<ShardInfo>,
    pub stats: ShardStats,
}

#[derive(Debug, Serialize)]
pub struct ShardStats {
    pub total: usize,
    pub primary: usize,
    pub replica: usize,
    pub started: usize,
    pub relocating: usize,
    pub unassigned: usize,
}

impl ShardsData {
    /// Generuje konzistentní HSL barvu z názvu indexu
    pub fn generate_index_color(&self, index_name: &str) -> String {
        generate_index_color(index_name)
    }

    /// Vrací barvu podle stavu shardu (pro border)
    pub fn shard_state_color(&self, state: &str) -> String {
        shard_state_color(state)
    }

    /// Rozhodne zda použít bílý nebo černý text podle barvy pozadí
    pub fn get_text_color(&self, bg_color: &str) -> String {
        get_text_color_for_background(bg_color)
    }

    /// Vrací všechny shardy jako JSON string pro JavaScript
    pub fn all_shards_json(&self) -> String {
        serde_json::to_string(&self.all_shards).unwrap_or_else(|_| "[]".to_string())
    }
}

/// Porovná index name s pattern (podporuje * wildcard)
fn matches_pattern(index_name: &str, pattern: &str) -> bool {
    // Jednoduchá wildcard matching funkce
    // Podporuje * jako libovolný počet znaků

    // Rozděl pattern podle '*'
    let parts: Vec<&str> = pattern.split('*').collect();

    if parts.is_empty() {
        return index_name == pattern;
    }

    let mut current_pos = 0;

    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }

        // První část musí být na začátku (pokud pattern nezačíná *)
        if i == 0 && !pattern.starts_with('*') {
            if !index_name.starts_with(part) {
                return false;
            }
            current_pos = part.len();
            continue;
        }

        // Poslední část musí být na konci (pokud pattern nekončí *)
        if i == parts.len() - 1 && !pattern.ends_with('*') {
            return index_name.ends_with(part);
        }

        // Střední části - hledej v řetězci
        if let Some(pos) = index_name[current_pos..].find(part) {
            current_pos += pos + part.len();
        } else {
            return false;
        }
    }

    true
}

/// GET /shards - Zobrazí stránku se shardy
pub async fn shards_page(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Query(query): Query<ShardsQuery>,
) -> Result<Html<String>, (StatusCode, String)> {
    let active_endpoint = get_active_endpoint(&state, &jar).await;

    let ctx = PageContext {
        active_endpoint: active_endpoint.clone(),
    };

    // Vezmi pattern z query, nebo z cookies (per endpoint), nebo default "*"
    let pattern = if !query.pattern.is_empty() {
        query.pattern.clone()
    } else {
        if let Some(ref endpoint) = active_endpoint {
            let cookie_name = format!("indices_filter_{}", endpoint.id);
            jar.get(&cookie_name)
                .map(|c| c.value().to_string())
                .unwrap_or_else(|| "*".to_string())
        } else {
            "*".to_string()
        }
    };

    // Načti data
    let data = match active_endpoint {
        Some(ref endpoint) => {
            match tokio::time::timeout(
                tokio::time::Duration::from_secs(30),
                load_shards_data(&state, endpoint, &pattern)
            ).await {
                Ok(Ok(d)) => Some(d),
                Ok(Err(e)) => {
                    tracing::error!("Failed to load shards: {}", e);
                    None
                }
                Err(_) => {
                    tracing::error!("Timeout loading shards");
                    None
                }
            }
        }
        None => None,
    };

    let template = ShardsTemplate {
        ctx,
        data,
        pattern: pattern.clone(),
    };

    template.render()
        .map(Html)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

async fn load_shards_data(
    state: &AppState,
    endpoint: &crate::db::models::Endpoint,
    pattern: &str,
) -> anyhow::Result<ShardsData> {
    let password = state.db.get_endpoint_password(endpoint).await;

    let mut client = EsClient::new(
        endpoint.url.clone(),
        endpoint.insecure,
        endpoint.username.clone(),
        password,
    )?;

    client.detect_version().await?;

    // Vždy zavolej _cat/shards pro všechny indexy - filtrování uděláme v Rustu
    // Důvod: ES může vrátit chybu když pattern neodpovídá žádnému indexu
    let path = "/_cat/shards?format=json&h=index,shard,prirep,state,docs,store,node,unassigned.reason";

    tracing::info!("Loading all shards, will filter by pattern: {}", pattern);
    let response: serde_json::Value = client.get(path).await?;
    tracing::info!("Received shards response, array length: {}", response.as_array().map(|a| a.len()).unwrap_or(0));

    let mut shards: Vec<ShardInfo> = Vec::new();

    if let Some(arr) = response.as_array() {
        for item in arr {
            let index_name = item["index"].as_str().unwrap_or("").to_string();

            // Pokud pattern není "*" nebo prázdný, filtruj podle něj
            // Pattern může obsahovat wildcards jako "*audit*"
            if pattern != "*" && !pattern.is_empty()
                && !matches_pattern(&index_name, pattern) {
                    continue;
                }

            shards.push(ShardInfo {
                index: index_name,
                shard: item["shard"].as_str().unwrap_or("").to_string(),
                prirep: item["prirep"].as_str().unwrap_or("").to_string(),
                state: item["state"].as_str().unwrap_or("").to_string(),
                docs: item["docs"].as_str().unwrap_or("0").to_string(),
                store: item["store"].as_str().unwrap_or("0b").to_string(),
                node: item["node"].as_str().unwrap_or("UNASSIGNED").to_string(),
                unassigned_reason: item.get("unassigned.reason")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
            });
        }
    }

    // Seskup podle nodů
    let mut nodes_map: HashMap<String, Vec<ShardInfo>> = HashMap::new();
    for shard in &shards {
        nodes_map.entry(shard.node.clone())
            .or_default()
            .push(shard.clone());
    }

    let nodes: Vec<NodeShards> = nodes_map.into_iter()
        .map(|(node_name, shards)| NodeShards { node_name, shards })
        .collect();

    // Statistiky
    let stats = ShardStats {
        total: shards.len(),
        primary: shards.iter().filter(|s| s.prirep == "p").count(),
        replica: shards.iter().filter(|s| s.prirep == "r").count(),
        started: shards.iter().filter(|s| s.state == "STARTED").count(),
        relocating: shards.iter().filter(|s| s.state == "RELOCATING").count(),
        unassigned: shards.iter().filter(|s| s.state == "UNASSIGNED").count(),
    };

    Ok(ShardsData {
        pattern: pattern.to_string(),
        nodes,
        all_shards: shards,
        stats,
    })
}
