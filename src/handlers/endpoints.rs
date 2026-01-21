use axum::{
    extract::{Path, State},
    response::{Html, IntoResponse, Redirect},
    Form,
    http::StatusCode,
};
use axum_extra::extract::CookieJar;
use axum_extra::extract::cookie::Cookie;
use serde::Deserialize;
use std::sync::Arc;
use askama::Template;

use crate::db::{Database, models::CreateEndpoint};
use crate::templates::{EndpointsTemplate, PageContext};

pub struct AppState {
    pub db: Database,
}

#[derive(Deserialize)]
pub struct CreateEndpointForm {
    name: String,
    url: String,
    insecure: Option<String>,
    username: Option<String>,
    password: Option<String>,
}

/// GET /endpoints - Zobrazí seznam endpointů
pub async fn list_endpoints(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
) -> Result<Html<String>, (StatusCode, String)> {
    let endpoints = state.db.get_endpoints().await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let active_endpoint = get_active_endpoint(&state, &jar).await;
    let ctx = PageContext::new(active_endpoint);

    let template = EndpointsTemplate { endpoints, ctx };

    template.render()
        .map(Html)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

/// POST /endpoints - Vytvoří nový endpoint
pub async fn create_endpoint(
    State(state): State<Arc<AppState>>,
    Form(form): Form<CreateEndpointForm>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let create_endpoint = CreateEndpoint {
        name: form.name,
        url: form.url,
        insecure: form.insecure.is_some(),
        username: if form.username.as_ref().map(|s| s.is_empty()).unwrap_or(true) {
            None
        } else {
            form.username
        },
        password: if form.password.as_ref().map(|s| s.is_empty()).unwrap_or(true) {
            None
        } else {
            form.password
        },
    };

    if let Err(e) = state.db.create_endpoint(create_endpoint).await {
        tracing::error!("Failed to create endpoint: {}", e);
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to save endpoint: {}", e),
        ));
    }

    // Vrátíme aktualizovaný seznam endpointů (pro HTMX swap)
    let endpoints = state.db.get_endpoints().await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Vygenerujeme jen list-group část (pro HTMX)
    let html = if endpoints.is_empty() {
        r#"<div class="empty">
            <div class="empty-icon"><i class="ti ti-server-off"></i></div>
            <p class="empty-title">Žádné endpointy</p>
        </div>"#.to_string()
    } else {
        let items: Vec<String> = endpoints.iter().map(|ep| {
            format!(r##"<div class="list-group-item">
                <div class="row align-items-center">
                    <div class="col-auto">
                        <span class="avatar"><i class="ti ti-server"></i></span>
                    </div>
                    <div class="col">
                        <div class="text-truncate"><strong>{}</strong></div>
                        <div class="text-muted"><code>{}</code></div>
                    </div>
                    <div class="col-auto">
                        <div class="btn-list">
                            <button class="btn btn-sm btn-icon btn-ghost-success" title="Test připojení">
                                <i class="ti ti-plug-connected"></i>
                            </button>
                            <button class="btn btn-sm btn-icon btn-ghost-danger"
                                onclick="confirmDelete({}, '{}')">
                                <i class="ti ti-trash"></i>
                            </button>
                        </div>
                    </div>
                </div>
            </div>"##, ep.name, ep.url, ep.id, ep.name)
        }).collect();

        format!(r##"<div class="list-group list-group-flush">{}</div>"##, items.join(""))
    };

    Ok(Html(html))
}

/// DELETE /endpoints/:id - Smaže endpoint
pub async fn delete_endpoint(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    state.db.delete_endpoint(id).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Vrátíme aktualizovaný seznam
    let endpoints = state.db.get_endpoints().await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let html = if endpoints.is_empty() {
        r##"<div class="empty">
            <div class="empty-icon"><i class="ti ti-server-off"></i></div>
            <p class="empty-title">Žádné endpointy</p>
            <p class="empty-subtitle text-muted">
                Začněte přidáním vašeho prvního Elasticsearch endpointu
            </p>
            <div class="empty-action">
                <button class="btn btn-primary" data-bs-toggle="modal" data-bs-target="#modal-endpoint">
                    <i class="ti ti-plus"></i>
                    Přidat endpoint
                </button>
            </div>
        </div>"##.to_string()
    } else {
        let items: Vec<String> = endpoints.iter().map(|ep| {
            format!(r##"<div class="list-group-item">
                <div class="row align-items-center">
                    <div class="col-auto">
                        <span class="avatar"><i class="ti ti-server"></i></span>
                    </div>
                    <div class="col">
                        <div class="text-truncate"><strong>{}</strong></div>
                        <div class="text-muted"><code>{}</code></div>
                    </div>
                    <div class="col-auto">
                        <div class="btn-list">
                            <button class="btn btn-sm btn-icon btn-ghost-success" title="Test připojení">
                                <i class="ti ti-plug-connected"></i>
                            </button>
                            <button class="btn btn-sm btn-icon btn-ghost-danger"
                                onclick="confirmDelete({}, '{}')">
                                <i class="ti ti-trash"></i>
                            </button>
                        </div>
                    </div>
                </div>
            </div>"##, ep.name, ep.url, ep.id, ep.name)
        }).collect();

        format!(r##"<div class="list-group list-group-flush">{}</div>"##, items.join(""))
    };

    Ok(Html(html))
}

/// POST /endpoints/:id/select - Vybere endpoint jako aktivní
pub async fn select_endpoint(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    jar: CookieJar,
) -> Result<(CookieJar, Redirect), (StatusCode, String)> {
    // Ověř že endpoint existuje
    let endpoint = state.db.get_endpoint(id).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if endpoint.is_none() {
        return Err((StatusCode::NOT_FOUND, "Endpoint not found".to_string()));
    }

    // Nastav cookie s ID endpointu (platnost 30 dní)
    let cookie = Cookie::build(("active_endpoint_id", id.to_string()))
        .path("/")
        .max_age(time::Duration::days(30))
        .build();

    let jar = jar.add(cookie);

    // Přesměruj na dashboard
    Ok((jar, Redirect::to("/dashboard")))
}

/// Helper funkce - získá aktivní endpoint z cookie
pub async fn get_active_endpoint(
    state: &AppState,
    jar: &CookieJar,
) -> Option<crate::db::models::Endpoint> {
    let endpoint_id = jar.get("active_endpoint_id")?
        .value()
        .parse::<i64>()
        .ok()?;

    state.db.get_endpoint(endpoint_id).await.ok()?
}

/// POST /endpoints/:id/test - Otestuje připojení k endpointu
pub async fn test_endpoint(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<axum::Json<serde_json::Value>, (StatusCode, String)> {
    use crate::es::EsClient;

    // Získej endpoint
    let endpoint = state.db.get_endpoint(id).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let endpoint = match endpoint {
        Some(e) => e,
        None => return Err((StatusCode::NOT_FOUND, "Endpoint not found".to_string())),
    };

    // Získej heslo pokud existuje
    let password = state.db.get_endpoint_password(&endpoint).await;

    // Vytvoř ES klienta
    let mut client = EsClient::new(
        endpoint.url.clone(),
        endpoint.insecure,
        endpoint.username.clone(),
        password,
    ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Zkus se připojit a získat verzi
    match client.detect_version().await {
        Ok(version) => {
            Ok(axum::Json(serde_json::json!({
                "success": true,
                "message": "Připojení úspěšné",
                "version": format!("{}.{}.{}", version.major, version.minor, version.patch)
            })))
        }
        Err(e) => {
            Ok(axum::Json(serde_json::json!({
                "success": false,
                "message": format!("Připojení selhalo: {}", e)
            })))
        }
    }
}
