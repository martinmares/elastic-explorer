use axum::{
    extract::{Query, State},
    response::{Html, Json},
    http::{StatusCode, HeaderMap},
};
use axum_extra::extract::{CookieJar, cookie::Cookie};
use std::sync::Arc;
use askama::Template;
use serde::{Deserialize, Serialize};

use crate::handlers::endpoints::{AppState, get_active_endpoint};
use crate::templates::{SearchTemplate, SearchResultsTemplate, PageContext};
use crate::es::EsClient;

#[derive(Debug, Deserialize, Clone)]
pub struct SearchQuery {
    #[serde(default)]
    pub index_pattern: String,
    #[serde(default)]
    pub query: String,
    #[serde(default = "default_page")]
    pub page: usize,
    #[serde(default = "default_per_page")]
    pub per_page: usize,
}

fn default_page() -> usize { 1 }
fn default_per_page() -> usize { 20 }

const MAX_RESULTS_PER_PAGE: usize = 100;
const MAX_PAGES: usize = 500; // Elasticsearch limit: 10,000 výsledků

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchResultsData {
    pub index_pattern: String,
    pub query: String,
    pub total: u64,
    pub took: u64, // milliseconds
    pub hits: Vec<SearchHit>,
    pub page: usize,
    pub per_page: usize,
    pub total_pages: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchHit {
    pub index: String,
    pub id: String,
    pub score: Option<f64>,
    pub source: serde_json::Value,
}

impl SearchHit {
    /// Vytvoří náhled JSON jako čistý text (prvních ~150 znaků)
    pub fn source_preview(&self) -> String {
        let json_str = self.source.to_string();
        let preview_len = 150;

        if json_str.len() <= preview_len {
            json_str
        } else {
            let mut preview = json_str.chars().take(preview_len).collect::<String>();
            // Najdi poslední mezeru aby se text neseknul uprostřed slova
            if let Some(last_space) = preview.rfind(|c: char| c.is_whitespace() || c == ',' || c == ':') {
                preview.truncate(last_space);
            }
            format!("{}...", preview)
        }
    }

    /// Vrátí source jako formátovaný JSON string
    pub fn source_formatted(&self) -> String {
        serde_json::to_string_pretty(&self.source).unwrap_or_else(|_| self.source.to_string())
    }
}

impl SearchResultsData {
    pub fn showing_from(&self) -> usize {
        if self.total == 0 || self.hits.is_empty() {
            0
        } else {
            (self.page - 1) * self.per_page + 1
        }
    }

    pub fn showing_to(&self) -> usize {
        if self.total == 0 || self.hits.is_empty() {
            0
        } else {
            let end = (self.page - 1) * self.per_page + self.hits.len();
            end.min(self.total as usize)
        }
    }
}

/// GET /search - Zobrazí vyhledávací formulář a výsledky
pub async fn search_page(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    headers: HeaderMap,
    Query(query): Query<SearchQuery>,
) -> Result<(CookieJar, Html<String>), (StatusCode, String)> {
    // Zjisti jestli je to HTMX request
    let is_htmx = headers.get("HX-Request").is_some();
    let active_endpoint = get_active_endpoint(&state, &jar).await;

    let ctx = PageContext {
        active_endpoint: active_endpoint.clone(),
    };

    // Pokud není zadán index pattern nebo query, zkus načíst z cookie
    let mut query = query;
    if query.index_pattern.is_empty() {
        if let Some(cookie_value) = jar.get("search_index_pattern") {
            query.index_pattern = cookie_value.value().to_string();
        }
    }
    if query.query.is_empty() {
        if let Some(cookie_value) = jar.get("search_query") {
            query.query = cookie_value.value().to_string();
        }
    }

    // Pokud stále není zadán index pattern (ani v query ani v cookie), zobraz jen prázdný formulář
    if query.index_pattern.is_empty() {
        let template = SearchTemplate {
            ctx,
            data: None,
        };

        return template.render()
            .map(|html| (jar, Html(html)))
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()));
    }

    // Ulož index_pattern a query do cookies (platnost 30 dní)
    let cookie_pattern = Cookie::build(("search_index_pattern", query.index_pattern.clone()))
        .path("/")
        .max_age(time::Duration::days(30))
        .build();
    let cookie_query = Cookie::build(("search_query", query.query.clone()))
        .path("/")
        .max_age(time::Duration::days(30))
        .build();
    let jar = jar.add(cookie_pattern).add(cookie_query);

    // Pokud NENÍ HTMX request a JSOU parametry, vrať stránku s prázdnými výsledky
    // (data se načtou automaticky přes HTMX pomocí JavaScriptu)
    if !is_htmx {
        // Vytvoř dummy SearchResultsData s parametry pro vyplnění formuláře
        let dummy_data = SearchResultsData {
            index_pattern: query.index_pattern.clone(),
            query: query.query.clone(),
            total: 0,
            took: 0,
            hits: vec![],
            page: 1,
            per_page: 20,
            total_pages: 0,
        };

        let template = SearchTemplate {
            ctx,
            data: Some(dummy_data),
        };

        return template.render()
            .map(|html| (jar, Html(html)))
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()));
    }

    // Předvyplň query na "*" pokud není zadáno
    let effective_query = if query.query.is_empty() {
        "*".to_string()
    } else {
        query.query.clone()
    };

    // Načti data s timeoutem
    let data = match active_endpoint {
        Some(ref endpoint) => {
            // Vytvoř upravenou query strukturu s effective_query
            let mut search_query = query.clone();
            search_query.query = effective_query.clone();

            match tokio::time::timeout(
                tokio::time::Duration::from_secs(30),
                perform_search(&state, endpoint, &search_query)
            ).await {
                Ok(Ok(d)) => Some(d),
                Ok(Err(e)) => {
                    tracing::error!("Failed to perform search: {}", e);
                    None
                }
                Err(_) => {
                    tracing::error!("Timeout performing search");
                    None
                }
            }
        }
        None => None,
    };

    // Pokud je to HTMX request, vrať jen výsledky
    if is_htmx {
        let template = SearchResultsTemplate { data };
        return template.render()
            .map(|html| (jar, Html(html)))
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()));
    }

    // Jinak vrať celou stránku
    let template = SearchTemplate { ctx, data };

    template.render()
        .map(|html| (jar, Html(html)))
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

async fn perform_search(
    state: &AppState,
    endpoint: &crate::db::models::Endpoint,
    query: &SearchQuery,
) -> anyhow::Result<SearchResultsData> {
    let password = state.db.get_endpoint_password(endpoint).await;

    let mut client = EsClient::new(
        endpoint.url.clone(),
        endpoint.insecure,
        endpoint.username.clone(),
        password,
    )?;

    client.detect_version().await?;

    // Omez per_page na maximální povolenou hodnotu
    let safe_per_page = query.per_page.min(MAX_RESULTS_PER_PAGE);

    // Vypočítej from pro pagination
    let from = (query.page - 1) * safe_per_page;

    // Vytvoř Elasticsearch query - simple match query
    let es_query = serde_json::json!({
        "from": from,
        "size": safe_per_page,
        "query": {
            "query_string": {
                "query": query.query,
                "default_operator": "AND"
            }
        },
        "sort": [
            { "_score": { "order": "desc" } },
            { "_doc": { "order": "desc" } }
        ]
    });

    // Proveď search
    let path = format!("/{}/_search", query.index_pattern);
    let response: serde_json::Value = client.post(&path, es_query).await?;

    // Parsuj výsledky
    let took = response["took"].as_u64().unwrap_or(0);
    let total = response["hits"]["total"]["value"].as_u64()
        .or_else(|| response["hits"]["total"].as_u64())
        .unwrap_or(0);

    let mut hits = Vec::new();
    if let Some(hits_array) = response["hits"]["hits"].as_array() {
        for hit in hits_array {
            hits.push(SearchHit {
                index: hit["_index"].as_str().unwrap_or("").to_string(),
                id: hit["_id"].as_str().unwrap_or("").to_string(),
                score: hit["_score"].as_f64(),
                source: hit["_source"].clone(),
            });
        }
    }

    let calculated_pages = ((total as usize + safe_per_page - 1) / safe_per_page).max(1);
    let total_pages = calculated_pages.min(MAX_PAGES);

    Ok(SearchResultsData {
        index_pattern: query.index_pattern.clone(),
        query: query.query.clone(),
        total,
        took,
        hits,
        page: query.page,
        per_page: safe_per_page,
        total_pages,
    })
}

// === Bulk operations ===

#[derive(Debug, Deserialize)]
pub struct BulkDeleteRequest {
    pub documents: Vec<DocumentIdentifier>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DocumentIdentifier {
    pub index: String,
    pub id: String,
}

#[derive(Debug, Serialize)]
pub struct BulkOperationResponse {
    pub success: bool,
    pub document: DocumentIdentifier,
    pub message: String,
}

/// POST /search/bulk/delete - Smaže vybrané dokumenty
pub async fn bulk_delete_documents(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Json(payload): Json<BulkDeleteRequest>,
) -> Result<Json<Vec<BulkOperationResponse>>, (StatusCode, String)> {
    let active_endpoint = get_active_endpoint(&state, &jar).await;

    let endpoint = match active_endpoint {
        Some(ep) => ep,
        None => return Err((StatusCode::BAD_REQUEST, "No active endpoint".to_string())),
    };

    let password = state.db.get_endpoint_password(&endpoint).await;
    let mut client = EsClient::new(
        endpoint.url.clone(),
        endpoint.insecure,
        endpoint.username.clone(),
        password,
    ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    client.detect_version().await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut results = Vec::new();

    for doc in payload.documents {
        match perform_delete_document(&client, &doc).await {
            Ok(_) => {
                results.push(BulkOperationResponse {
                    success: true,
                    document: doc.clone(),
                    message: "Dokument smazán".to_string(),
                });
            }
            Err(e) => {
                results.push(BulkOperationResponse {
                    success: false,
                    document: doc.clone(),
                    message: format!("Chyba: {}", e),
                });
            }
        }
    }

    Ok(Json(results))
}

async fn perform_delete_document(
    client: &EsClient,
    doc: &DocumentIdentifier,
) -> anyhow::Result<()> {
    let path = format!("/{}/_doc/{}", doc.index, doc.id);
    let _response: serde_json::Value = client.delete(&path).await?;
    Ok(())
}
