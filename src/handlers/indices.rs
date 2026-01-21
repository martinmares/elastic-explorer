use axum::{
    extract::{Query, State},
    response::{Html, Json},
    http::StatusCode,
};
use axum_extra::extract::{CookieJar, cookie::Cookie};
use std::sync::Arc;
use askama::Template;
use serde::{Deserialize, Serialize};

use crate::handlers::endpoints::{AppState, get_active_endpoint};
use crate::templates::{IndicesTemplate, IndicesTableTemplate, IndexDetailTemplate, PageContext};
use crate::es::EsClient;
use crate::models::{IndexInfo, IndicesListData, AliasInfo, IndexDetail};
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
pub struct IndicesQuery {
    #[serde(default = "default_filter")]
    pub filter: String,
    #[serde(default = "default_page")]
    pub page: usize,
    #[serde(default = "default_per_page")]
    pub per_page: usize,
    #[serde(default = "default_sort_by")]
    pub sort_by: String,
    #[serde(default = "default_sort_order")]
    pub sort_order: String,
    #[serde(default = "default_hide_internal")]
    pub hide_internal: bool,
}

fn default_filter() -> String {
    "*".to_string()
}

fn default_page() -> usize {
    1
}

fn default_per_page() -> usize {
    50
}

fn default_sort_by() -> String {
    "index".to_string()
}

fn default_sort_order() -> String {
    "asc".to_string()
}

fn default_hide_internal() -> bool {
    true // Defaultně skryté
}

/// GET /indices - Zobrazí seznam indexů
pub async fn list_indices(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Query(mut query): Query<IndicesQuery>,
) -> Result<Html<String>, (StatusCode, String)> {
    let active_endpoint = get_active_endpoint(&state, &jar).await;

    if active_endpoint.is_none() {
        return Err((StatusCode::BAD_REQUEST, "No active endpoint selected".to_string()));
    }

    let endpoint = active_endpoint.as_ref().unwrap();

    // Načti filtr z cookies, pokud není zadán v query (použij pouze když je defaultní "*")
    if query.filter == "*"
        && let Some(cookie) = jar.get("indices_filter") {
            query.filter = cookie.value().to_string();
        }

    // Načti data s timeoutem
    let data = match tokio::time::timeout(
        tokio::time::Duration::from_secs(10),
        load_indices_data(&state, endpoint, &query)
    ).await {
        Ok(Ok(d)) => Some(d),
        Ok(Err(e)) => {
            tracing::error!("Failed to load indices: {}", e);
            None
        }
        Err(_) => {
            tracing::error!("Timeout loading indices");
            None
        }
    };

    let ctx = PageContext::new(active_endpoint);
    let template = IndicesTemplate { ctx, data };

    template.render()
        .map(Html)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

/// GET /indices/table - Vrátí jen tabulku (partial pro HTMX)
pub async fn indices_table(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Query(query): Query<IndicesQuery>,
) -> Result<(CookieJar, Html<String>), (StatusCode, String)> {
    let active_endpoint = get_active_endpoint(&state, &jar).await;

    if active_endpoint.is_none() {
        return Err((StatusCode::BAD_REQUEST, "No active endpoint selected".to_string()));
    }

    let endpoint = active_endpoint.as_ref().unwrap();

    // Ulož filtr do cookies
    let cookie = Cookie::build(("indices_filter", query.filter.clone()))
        .path("/")
        .build();
    let jar = jar.add(cookie);

    // Načti data s timeoutem
    let data = match tokio::time::timeout(
        tokio::time::Duration::from_secs(10),
        load_indices_data(&state, endpoint, &query)
    ).await {
        Ok(Ok(d)) => Some(d),
        Ok(Err(e)) => {
            tracing::error!("Failed to load indices: {}", e);
            None
        }
        Err(_) => {
            tracing::error!("Timeout loading indices");
            None
        }
    };

    let template = IndicesTableTemplate { data };

    template.render()
        .map(|html| (jar, Html(html)))
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

async fn load_indices_data(
    state: &AppState,
    endpoint: &crate::db::models::Endpoint,
    query: &IndicesQuery,
) -> anyhow::Result<IndicesListData> {
    let password = state.db.get_endpoint_password(endpoint).await;

    let mut client = EsClient::new(
        endpoint.url.clone(),
        endpoint.insecure,
        endpoint.username.clone(),
        password,
    )?;

    client.detect_version().await?;

    // Zavolej ES API s filtrem
    let filter = if query.filter.is_empty() {
        "*".to_string()
    } else {
        query.filter.clone()
    };

    let path = format!("/_cat/indices/{}?format=json&bytes=b", filter);
    let mut indices: Vec<IndexInfo> = client.get(&path).await?;

    // Načti aliasy
    let aliases_path = "/_cat/aliases?format=json";
    let aliases: Vec<AliasInfo> = client.get(aliases_path).await.unwrap_or_default();

    // Vytvoř mapu index -> seznam aliasů
    let mut aliases_map: HashMap<String, Vec<String>> = HashMap::new();
    for alias_info in aliases {
        aliases_map
            .entry(alias_info.index)
            .or_default()
            .push(alias_info.alias);
    }

    // Přiřaď aliasy k indexům
    for idx in &mut indices {
        if let Some(aliases) = aliases_map.get(&idx.index) {
            idx.aliases = aliases.clone();
        }
    }

    // Filtruj interní indexy (začínají tečkou)
    if query.hide_internal {
        indices.retain(|idx| !idx.index.starts_with('.'));
    }

    // Sortování
    match query.sort_by.as_str() {
        "index" => {
            indices.sort_by(|a, b| {
                if query.sort_order == "desc" {
                    b.index.cmp(&a.index)
                } else {
                    a.index.cmp(&b.index)
                }
            });
        }
        "health" => {
            indices.sort_by(|a, b| {
                if query.sort_order == "desc" {
                    b.health.cmp(&a.health)
                } else {
                    a.health.cmp(&b.health)
                }
            });
        }
        "docs_count" => {
            indices.sort_by(|a, b| {
                if query.sort_order == "desc" {
                    b.docs_count_num().cmp(&a.docs_count_num())
                } else {
                    a.docs_count_num().cmp(&b.docs_count_num())
                }
            });
        }
        "store_size" => {
            indices.sort_by(|a, b| {
                if query.sort_order == "desc" {
                    b.store_size_bytes().cmp(&a.store_size_bytes())
                } else {
                    a.store_size_bytes().cmp(&b.store_size_bytes())
                }
            });
        }
        _ => {}
    }

    let total = indices.len();
    let total_pages = total.div_ceil(query.per_page);

    // Pagination
    let start = (query.page - 1) * query.per_page;
    let paginated_indices = indices.into_iter()
        .skip(start)
        .take(query.per_page)
        .collect();

    Ok(IndicesListData {
        indices: paginated_indices,
        total,
        page: query.page,
        per_page: query.per_page,
        total_pages,
        filter: query.filter.clone(),
        sort_by: query.sort_by.clone(),
        sort_order: query.sort_order.clone(),
        hide_internal: query.hide_internal,
    })
}

/// GET /indices/detail/:index_name - Vrátí detail indexu pro modální okno
pub async fn index_detail(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    axum::extract::Path(index_name): axum::extract::Path<String>,
) -> Result<Html<String>, (StatusCode, String)> {
    let active_endpoint = get_active_endpoint(&state, &jar).await;

    if active_endpoint.is_none() {
        return Err((StatusCode::BAD_REQUEST, "No active endpoint selected".to_string()));
    }

    let endpoint = active_endpoint.as_ref().unwrap();

    // Načti data s timeoutem
    let data = match tokio::time::timeout(
        tokio::time::Duration::from_secs(10),
        load_index_detail(&state, endpoint, &index_name)
    ).await {
        Ok(Ok(d)) => Some(d),
        Ok(Err(e)) => {
            tracing::error!("Failed to load index detail: {}", e);
            None
        }
        Err(_) => {
            tracing::error!("Timeout loading index detail");
            None
        }
    };

    let template = IndexDetailTemplate { data };

    template.render()
        .map(Html)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

async fn load_index_detail(
    state: &AppState,
    endpoint: &crate::db::models::Endpoint,
    index_name: &str,
) -> anyhow::Result<IndexDetail> {
    let password = state.db.get_endpoint_password(endpoint).await;

    let mut client = EsClient::new(
        endpoint.url.clone(),
        endpoint.insecure,
        endpoint.username.clone(),
        password,
    )?;

    client.detect_version().await?;

    // 1. Načti základní informace z _cat/indices
    let cat_path = format!("/_cat/indices/{}?format=json&bytes=b", index_name);
    let cat_indices: Vec<IndexInfo> = client.get(&cat_path).await?;

    if cat_indices.is_empty() {
        return Err(anyhow::anyhow!("Index not found"));
    }

    let index_info = &cat_indices[0];

    // 2. Načti aliasy - musíme použít GET /{index}/_alias místo _cat/aliases
    let aliases_path = format!("/{}/_alias", index_name);
    let aliases_response: serde_json::Value = client.get(&aliases_path).await.unwrap_or(serde_json::json!({}));

    // Parsuj aliasy z response - struktura je: { "index_name": { "aliases": { "alias1": {}, "alias2": {} } } }
    let mut alias_names: Vec<String> = Vec::new();
    if let Some(aliases_map) = aliases_response.get(index_name)
        .and_then(|index_obj| index_obj.get("aliases"))
        .and_then(|aliases_obj| aliases_obj.as_object()) {
            alias_names = aliases_map.keys().map(|k| k.to_string()).collect();
        }

    // 3. Načti settings
    let settings_path = format!("/{}/_settings", index_name);
    let settings_response: serde_json::Value = client.get(&settings_path).await?;
    let settings = serde_json::to_string_pretty(&settings_response)
        .ok();

    // 4. Načti mappings
    let mappings_path = format!("/{}/_mapping", index_name);
    let mappings_response: serde_json::Value = client.get(&mappings_path).await?;
    let mappings = serde_json::to_string_pretty(&mappings_response)
        .ok();

    // 5. Načti stats
    let stats_path = format!("/{}/_stats", index_name);
    let stats_response: serde_json::Value = client.get(&stats_path).await?;
    let stats = serde_json::to_string_pretty(&stats_response)
        .ok();

    Ok(IndexDetail {
        index_name: index_info.index.clone(),
        health: index_info.health.clone(),
        status: index_info.status.clone(),
        uuid: index_info.uuid.clone(),
        pri: index_info.pri.clone(),
        rep: index_info.rep.clone(),
        docs_count: index_info.docs_count.clone(),
        docs_deleted: index_info.docs_deleted.clone(),
        store_size: index_info.store_size.clone(),
        pri_store_size: index_info.pri_store_size.clone(),
        aliases: alias_names,
        settings,
        mappings,
        stats,
    })
}

#[derive(Serialize)]
pub struct BulkOperationResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// POST /indices/bulk/{action}/{index_name} - Provede bulk operaci na indexu
pub async fn bulk_operation(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    axum::extract::Path((action, index_name)): axum::extract::Path<(String, String)>,
) -> Result<Json<BulkOperationResponse>, (StatusCode, Json<BulkOperationResponse>)> {
    let active_endpoint = get_active_endpoint(&state, &jar).await;

    if active_endpoint.is_none() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(BulkOperationResponse {
                success: false,
                message: None,
                error: Some("No active endpoint selected".to_string()),
            }),
        ));
    }

    let endpoint = active_endpoint.as_ref().unwrap();
    let password = state.db.get_endpoint_password(endpoint).await;

    let mut client = EsClient::new(
        endpoint.url.clone(),
        endpoint.insecure,
        endpoint.username.clone(),
        password,
    ).map_err(|e| (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(BulkOperationResponse {
            success: false,
            message: None,
            error: Some(format!("Failed to create ES client: {}", e)),
        }),
    ))?;

    client.detect_version().await.map_err(|e| (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(BulkOperationResponse {
            success: false,
            message: None,
            error: Some(format!("Failed to detect ES version: {}", e)),
        }),
    ))?;

    // Perform the action
    let result = match action.as_str() {
        "delete" => perform_delete_index(&client, &index_name).await,
        "close" => perform_close_index(&client, &index_name).await,
        "open" => perform_open_index(&client, &index_name).await,
        "refresh" => perform_refresh_index(&client, &index_name).await,
        _ => Err(anyhow::anyhow!("Unknown action: {}", action)),
    };

    match result {
        Ok(message) => Ok(Json(BulkOperationResponse {
            success: true,
            message: Some(message),
            error: None,
        })),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(BulkOperationResponse {
                success: false,
                message: None,
                error: Some(e.to_string()),
            }),
        )),
    }
}

async fn perform_delete_index(client: &EsClient, index_name: &str) -> anyhow::Result<String> {
    let path = format!("/{}", index_name);
    let _response: serde_json::Value = client.delete(&path).await?;
    Ok("Index smazán".to_string())
}

async fn perform_close_index(client: &EsClient, index_name: &str) -> anyhow::Result<String> {
    let path = format!("/{}/_close", index_name);
    let _response: serde_json::Value = client.post(&path, serde_json::json!({})).await?;
    Ok("Index zavřen".to_string())
}

async fn perform_open_index(client: &EsClient, index_name: &str) -> anyhow::Result<String> {
    let path = format!("/{}/_open", index_name);
    let _response: serde_json::Value = client.post(&path, serde_json::json!({})).await?;
    Ok("Index otevřen".to_string())
}

async fn perform_refresh_index(client: &EsClient, index_name: &str) -> anyhow::Result<String> {
    let path = format!("/{}/_refresh", index_name);
    let _response: serde_json::Value = client.post(&path, serde_json::json!({})).await?;
    Ok("Index refreshnut".to_string())
}
