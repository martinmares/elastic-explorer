use axum::{
    extract::{Query, State},
    response::{Html, Json},
    http::StatusCode,
};
use axum_extra::extract::{CookieJar, cookie::Cookie};
use std::sync::Arc;
use askama::Template;
use serde::{Deserialize, Serialize};

use crate::handlers::endpoints::{AppState, get_active_endpoint, get_endpoint_password};
use crate::templates::{IndicesTemplate, IndicesTableTemplate, IndexDetailTemplate, PageContext};
use crate::es::EsClient;
use crate::models::{IndexInfo, IndicesListData, AliasInfo, IndexDetail};
use crate::utils::{format_bytes, format_number, parse_size_to_bytes};
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

fn parse_pattern_expression(input: &str) -> (Vec<String>, Vec<String>) {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return (vec!["*".to_string()], Vec::new());
    }

    let mut includes: Vec<String> = Vec::new();
    let mut excludes: Vec<String> = Vec::new();
    let mut neg_next = false;

    for token in trimmed.replace(',', " , ").split_whitespace() {
        if token == "," || token.eq_ignore_ascii_case("or") || token.eq_ignore_ascii_case("and") {
            continue;
        }
        if token.eq_ignore_ascii_case("not") || token == "-" {
            neg_next = true;
            continue;
        }

        let mut value = token;
        let mut is_exclude = neg_next;
        if value.starts_with('-') {
            is_exclude = true;
            value = &value[1..];
        }
        if value.is_empty() {
            neg_next = false;
            continue;
        }

        if is_exclude {
            excludes.push(value.to_string());
        } else {
            includes.push(value.to_string());
        }
        neg_next = false;
    }

    if includes.is_empty() {
        includes.push("*".to_string());
    }

    (includes, excludes)
}

fn matches_pattern(index_name: &str, pattern: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.is_empty() {
        return index_name == pattern;
    }

    let mut current_pos = 0;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if i == 0 && !pattern.starts_with('*') {
            if !index_name.starts_with(part) {
                return false;
            }
            current_pos = part.len();
            continue;
        }
        if i == parts.len() - 1 && !pattern.ends_with('*') {
            return index_name.ends_with(part);
        }
        if let Some(pos) = index_name[current_pos..].find(part) {
            current_pos += pos + part.len();
        } else {
            return false;
        }
    }

    true
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

    if query.page == 0 {
        query.page = 1;
    }
    // Načti filtr z cookies, pokud není zadán v query (použij pouze když je defaultní "*")
    let filter_cookie_name = format!("indices_filter_{}", endpoint.id);
    if query.filter == "*"
        && let Some(cookie) = jar.get(&filter_cookie_name) {
            query.filter = cookie.value().to_string();
        }
    let per_page_cookie_name = format!("indices_per_page_{}", endpoint.id);
    if query.per_page == default_per_page()
        && let Some(cookie) = jar.get(&per_page_cookie_name) {
            if let Ok(value) = cookie.value().parse::<usize>() {
                query.per_page = value;
            }
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

    let ctx = PageContext::new(active_endpoint, state.base_path.clone());
    let template = IndicesTemplate { ctx, data };

    template.render()
        .map(Html)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

/// GET /indices/table - Vrátí jen tabulku (partial pro HTMX)
pub async fn indices_table(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Query(mut query): Query<IndicesQuery>,
) -> Result<(CookieJar, Html<String>), (StatusCode, String)> {
    let active_endpoint = get_active_endpoint(&state, &jar).await;

    if active_endpoint.is_none() {
        return Err((StatusCode::BAD_REQUEST, "No active endpoint selected".to_string()));
    }

    let endpoint = active_endpoint.as_ref().unwrap();
    if query.page == 0 {
        query.page = 1;
    }

    // Ulož filtr + per_page do cookies (per endpoint)
    let filter_cookie_name = format!("indices_filter_{}", endpoint.id);
    let filter_cookie = Cookie::build((filter_cookie_name, query.filter.clone()))
        .path("/")
        .build();
    let per_page_cookie_name = format!("indices_per_page_{}", endpoint.id);
    let per_page_cookie = Cookie::build((per_page_cookie_name, query.per_page.to_string()))
        .path("/")
        .build();
    let jar = jar.add(filter_cookie).add(per_page_cookie);

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

    let ctx = PageContext::new(active_endpoint, state.base_path.clone());
    let template = IndicesTableTemplate { ctx, data };

    template.render()
        .map(|html| (jar, Html(html)))
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

#[derive(Debug, Serialize)]
pub struct IndicesSummaryRow {
    pub name: String,
    pub note: Option<String>,
    pub indices: Vec<IndicesSummaryIndex>,
    pub is_alias: bool,
    pub has_alias_badge: bool,
}

#[derive(Debug, Serialize)]
pub struct IndicesSummaryIndex {
    pub name: String,
    pub size: String,
    pub shards: String,
    pub replicas: String,
}

#[derive(Debug, Serialize)]
pub struct IndicesSummaryData {
    pub rows: Vec<IndicesSummaryRow>,
    pub filter: String,
}

/// GET /indices/summary - Vrátí shrnutí indexů a aliasů pro aktuální filtr
pub async fn indices_summary(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Query(query): Query<IndicesQuery>,
) -> Result<Html<String>, (StatusCode, String)> {
    let active_endpoint = get_active_endpoint(&state, &jar).await;

    if active_endpoint.is_none() {
        return Err((StatusCode::BAD_REQUEST, "No active endpoint selected".to_string()));
    }

    let endpoint = active_endpoint.as_ref().unwrap();
    let (includes, excludes) = parse_pattern_expression(&query.filter);
    let patterns = if includes.len() == 1 && includes[0] == "*" {
        Vec::new()
    } else {
        includes.clone()
    };

    let password = get_endpoint_password(&state, endpoint).await;
    let mut client = EsClient::new(
        endpoint.url.clone(),
        endpoint.insecure,
        endpoint.username.clone(),
        password,
    ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    client.detect_version().await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let filter = if includes.len() == 1 && includes[0] == "*" {
        "*".to_string()
    } else {
        includes.join(",")
    };
    let path = if filter == "*" {
        "/_cat/indices?format=json&bytes=b&h=health,status,index,uuid,pri,rep,docs.count,docs.deleted,store.size,pri.store.size,creation.date.string".to_string()
    } else {
        format!("/_cat/indices/{}?format=json&bytes=b&h=health,status,index,uuid,pri,rep,docs.count,docs.deleted,store.size,pri.store.size,creation.date.string", filter)
    };
    let mut indices: Vec<IndexInfo> = client.get(&path)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if query.hide_internal {
        indices.retain(|idx| !idx.index.starts_with('.'));
    }
    if !excludes.is_empty() {
        indices.retain(|idx| !excludes.iter().any(|pat| matches_pattern(&idx.index, pat)));
    }

    let aliases: Vec<AliasInfo> = client.get("/_cat/aliases?format=json")
        .await
        .unwrap_or_default();

    let mut alias_to_indices: HashMap<String, Vec<String>> = HashMap::new();
    let mut index_to_aliases: HashMap<String, Vec<String>> = HashMap::new();
    for alias_info in aliases {
        if query.hide_internal && alias_info.alias.starts_with('.') {
            continue;
        }
        alias_to_indices
            .entry(alias_info.alias.clone())
            .or_default()
            .push(alias_info.index.clone());
        index_to_aliases
            .entry(alias_info.index)
            .or_default()
            .push(alias_info.alias);
    }

    let mut rows: Vec<IndicesSummaryRow> = Vec::new();
    let mut covered_indices: HashMap<String, bool> = HashMap::new();
    let mut index_lookup: HashMap<String, &IndexInfo> = HashMap::new();
    for idx in &indices {
        index_lookup.insert(idx.index.clone(), idx);
    }

    // 1) aliasy, které matchují pattern (a jejich indexy)
    let mut matched_aliases: Vec<(String, Vec<String>)> = Vec::new();
    for (alias, indices_for_alias) in &alias_to_indices {
        let match_alias = patterns.iter().any(|pat| matches_pattern(alias, pat));
        if !patterns.is_empty() && !match_alias {
            continue;
        }
        let filtered_indices: Vec<String> = if excludes.is_empty() {
            indices_for_alias.clone()
        } else {
            indices_for_alias
                .iter()
                .filter(|idx| !excludes.iter().any(|pat| matches_pattern(idx, pat)))
                .cloned()
                .collect()
        };
        if filtered_indices.is_empty() {
            continue;
        }
        matched_aliases.push((alias.clone(), filtered_indices));
    }
    matched_aliases.sort_by(|a, b| a.0.cmp(&b.0));
    for (alias, indices_for_alias) in matched_aliases {
        let mut details: Vec<IndicesSummaryIndex> = Vec::new();
        for idx in &indices_for_alias {
            covered_indices.insert(idx.clone(), true);
            if let Some(info) = index_lookup.get(idx) {
                details.push(IndicesSummaryIndex {
                    name: info.index.clone(),
                    size: info.store_size_formatted(),
                    shards: info.pri.clone(),
                    replicas: info.rep.clone(),
                });
            }
        }
        details.sort_by(|a, b| a.name.cmp(&b.name));
        rows.push(IndicesSummaryRow {
            name: alias,
            note: None,
            indices: details,
            is_alias: true,
            has_alias_badge: false,
        });
    }

    // 2) indexy matchující pattern, které nejsou pokryté aliasy
    let mut matched_indices: Vec<String> = indices
        .iter()
        .map(|idx| idx.index.clone())
        .collect();
    matched_indices.sort();
    for idx_name in matched_indices {
        if covered_indices.contains_key(&idx_name) {
            continue;
        }
        let aliases = index_to_aliases.get(&idx_name).cloned().unwrap_or_default();
        let note = if aliases.is_empty() {
            Some("has no alias".to_string())
        } else {
            Some(format!("aliases: {}", aliases.join(", ")))
        };
        let mut details: Vec<IndicesSummaryIndex> = Vec::new();
        if let Some(info) = index_lookup.get(&idx_name) {
            details.push(IndicesSummaryIndex {
                name: info.index.clone(),
                size: info.store_size_formatted(),
                shards: info.pri.clone(),
                replicas: info.rep.clone(),
            });
        }
        rows.push(IndicesSummaryRow {
            name: idx_name,
            note,
            indices: details,
            is_alias: false,
            has_alias_badge: aliases.is_empty(),
        });
    }

    let data = IndicesSummaryData {
        rows,
        filter: query.filter.clone(),
    };

    let template = crate::templates::IndicesSummaryTemplate { data };
    template.render()
        .map(Html)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

#[derive(Debug, Deserialize)]
pub struct IndicesMetricsQuery {
    pub indices: String,
}

#[derive(Debug, Deserialize)]
struct IndexMetricsRow {
    pub index: String,
    #[serde(rename = "docs.count")]
    pub docs_count: String,
    #[serde(rename = "store.size")]
    pub store_size: String,
}

#[derive(Debug, Serialize)]
pub struct IndexMetrics {
    pub docs: String,
    pub size: String,
}

/// GET /indices/metrics - Vrátí docs a size pro vybrané indexy
pub async fn indices_metrics(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Query(query): Query<IndicesMetricsQuery>,
) -> Result<Json<HashMap<String, IndexMetrics>>, (StatusCode, String)> {
    let active_endpoint = get_active_endpoint(&state, &jar).await;

    if active_endpoint.is_none() {
        return Err((StatusCode::BAD_REQUEST, "No active endpoint selected".to_string()));
    }

    let endpoint = active_endpoint.as_ref().unwrap();
    let indices = query
        .indices
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();

    if indices.is_empty() {
        return Ok(Json(HashMap::new()));
    }

    let password = get_endpoint_password(&state, endpoint).await;
    let client = EsClient::new(
        endpoint.url.clone(),
        endpoint.insecure,
        endpoint.username.clone(),
        password,
    ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let path = format!(
        "/_cat/indices/{}?format=json&bytes=b&h=index,docs.count,store.size,creation.date.string",
        indices.join(",")
    );
    let rows: Vec<IndexMetricsRow> = client.get(&path).await.unwrap_or_default();

    let mut metrics = HashMap::new();
    for row in rows {
        let docs_num = row.docs_count.parse::<u64>().unwrap_or(0);
        let size_bytes = parse_size_to_bytes(&row.store_size);
        metrics.insert(
            row.index,
            IndexMetrics {
                docs: format_number(docs_num),
                size: format_bytes(size_bytes),
            },
        );
    }

    Ok(Json(metrics))
}

async fn load_indices_data(
    state: &AppState,
    endpoint: &crate::db::models::Endpoint,
    query: &IndicesQuery,
) -> anyhow::Result<IndicesListData> {
    let password = get_endpoint_password(&state, endpoint).await;

    let mut client = EsClient::new(
        endpoint.url.clone(),
        endpoint.insecure,
        endpoint.username.clone(),
        password,
    )?;

    client.detect_version().await?;

    // Zavolej ES API s filtrem
    let (includes, excludes) = parse_pattern_expression(&query.filter);
    let filter = if includes.len() == 1 && includes[0] == "*" {
        "*".to_string()
    } else {
        includes.join(",")
    };

    // Pokud je pattern "*", použij prázdný path (všechny indexy)
    // Jinak přidej pattern do path - Elasticsearch očekává neenkódovaný pattern
    let path = if filter == "*" {
        "/_cat/indices?format=json&bytes=b&h=health,status,index,uuid,pri,rep,docs.count,docs.deleted,store.size,pri.store.size,creation.date.string".to_string()
    } else {
        // Pattern dej do path BEZ URL encoding - Elasticsearch sám zpracuje wildcards
        format!("/_cat/indices/{}?format=json&bytes=b&h=health,status,index,uuid,pri,rep,docs.count,docs.deleted,store.size,pri.store.size,creation.date.string", filter)
    };

    tracing::debug!("Fetching indices with pattern: {}, path: {}", filter, path);
    let mut indices: Vec<IndexInfo> = client.get(&path).await?;
    tracing::debug!("Received {} indices from Elasticsearch", indices.len());

    if !excludes.is_empty() {
        indices.retain(|idx| !excludes.iter().any(|pat| matches_pattern(&idx.index, pat)));
    }

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
    let password = get_endpoint_password(&state, endpoint).await;

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

    let stats_index = stats_response.get("indices")
        .and_then(|v| v.get(index_name))
        .or_else(|| stats_response.get("_all"))
        .or_else(|| stats_response.get("indices")
            .and_then(|v| v.as_object())
            .and_then(|map| map.values().next()))
        .unwrap_or(&serde_json::Value::Null);

    let get_u64 = |root: &serde_json::Value, path: &[&str]| -> Option<u64> {
        let mut current = root;
        for key in path {
            current = current.get(*key)?;
        }
        current.as_u64()
    };

    let stats_docs_count = get_u64(stats_index, &["total", "docs", "count"])
        .or_else(|| get_u64(stats_index, &["primaries", "docs", "count"]));
    let stats_docs_deleted = get_u64(stats_index, &["total", "docs", "deleted"])
        .or_else(|| get_u64(stats_index, &["primaries", "docs", "deleted"]));
    let stats_store_size_bytes = get_u64(stats_index, &["total", "store", "size_in_bytes"]);
    let stats_pri_store_size_bytes = get_u64(stats_index, &["primaries", "store", "size_in_bytes"]);
    let stats_segments_count = get_u64(stats_index, &["total", "segments", "count"]);
    let stats_segments_memory_bytes = get_u64(stats_index, &["total", "segments", "memory_in_bytes"]);
    let stats_search_query_total = get_u64(stats_index, &["total", "search", "query_total"]);
    let stats_search_query_time_ms = get_u64(stats_index, &["total", "search", "query_time_in_millis"]);
    let stats_indexing_total = get_u64(stats_index, &["total", "indexing", "index_total"]);
    let stats_indexing_time_ms = get_u64(stats_index, &["total", "indexing", "index_time_in_millis"]);
    let stats_primary_store_ratio = match (stats_pri_store_size_bytes, stats_store_size_bytes) {
        (Some(primaries), Some(total)) if total > 0 => {
            Some(((primaries * 100) / total).min(100) as u8)
        }
        _ => None,
    };
    let stats_deleted_ratio = match (stats_docs_count, stats_docs_deleted) {
        (Some(count), Some(deleted)) => {
            let total = count + deleted;
            if total > 0 {
                Some(((deleted * 100) / total).min(100) as u8)
            } else {
                Some(0)
            }
        }
        _ => None,
    };
    let stats_segments_memory_ratio = match (stats_segments_memory_bytes, stats_store_size_bytes) {
        (Some(segments), Some(total)) if total > 0 => {
            Some(((segments * 100) / total).min(100) as u8)
        }
        _ => None,
    };

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
        stats_docs_count,
        stats_docs_deleted,
        stats_store_size_bytes,
        stats_pri_store_size_bytes,
        stats_segments_count,
        stats_segments_memory_bytes,
        stats_search_query_total,
        stats_search_query_time_ms,
        stats_indexing_total,
        stats_indexing_time_ms,
        stats_primary_store_ratio,
        stats_deleted_ratio,
        stats_segments_memory_ratio,
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
    let password = get_endpoint_password(&state, endpoint).await;

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
