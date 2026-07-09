use askama::Template;
use axum::{
    Form,
    extract::{Path, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect},
};
use axum_extra::extract::CookieJar;
use axum_extra::extract::cookie::Cookie;
use serde::Deserialize;
use std::sync::Arc;

use crate::db::{
    Database,
    models::{CreateEndpoint, UpdateEndpoint},
};
use crate::templates::{EndpointsTemplate, PageContext};

pub struct AppState {
    pub db: Option<Database>,
    pub base_path: String,
    pub logout_url: Option<String>,
    pub stateless_endpoint: Option<crate::db::models::Endpoint>,
    pub stateless_password: Option<String>,
}

pub fn default_index_pattern(endpoint: &crate::db::models::Endpoint) -> Option<String> {
    endpoint
        .index_pattern
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
}

fn escape_attr(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn render_endpoints_list(
    endpoints: &[crate::db::models::Endpoint],
    active_id: Option<i64>,
) -> String {
    if endpoints.is_empty() {
        return r#"<div class="empty">
            <div class="empty-icon"><i class="ti ti-server-off"></i></div>
            <p class="empty-title">Žádné endpointy</p>
        </div>"#
            .to_string();
    }

    let items: Vec<String> = endpoints.iter().map(|ep| {
        let is_active = active_id == Some(ep.id);
        let active_badge = if is_active {
            r#"<span class="badge bg-green-lt ms-2">Active</span>"#
        } else {
            ""
        };
        let avatar_class = if is_active { "bg-green" } else { "" };
        let insecure_badge = if ep.insecure {
            r#"<span class="badge bg-yellow-lt ms-2">
                                        <i class="ti ti-shield-off"></i> Insecure
                                    </span>"#
        } else {
            ""
        };
        let username_badge = if let Some(username) = &ep.username {
            format!(
                r#"<span class="badge bg-blue-lt ms-2">
                                        <i class="ti ti-user"></i> {}
                                    </span>"#,
                username
            )
        } else {
            String::new()
        };
        let scope_badge = if let Some(pattern) = &ep.index_pattern {
            format!(
                r#"<span class="badge bg-purple-lt ms-2" title="Index scope">
                                        <i class="ti ti-filter"></i> {}
                                    </span>"#,
                escape_attr(pattern)
            )
        } else {
            String::new()
        };

        let username_attr = ep.username.as_deref().unwrap_or("");
        let index_pattern_attr = ep.index_pattern.as_deref().unwrap_or("");
        format!(r##"<div class="list-group-item">
                <div class="row align-items-center">
                    <div class="col-auto">
                        <span class="avatar {}"><i class="ti ti-server"></i></span>
                    </div>
                    <div class="col" style="cursor: pointer;" onclick="document.getElementById('select-form-{}').submit();">
                        <div class="text-truncate">
                            <strong>{}</strong>
                            {}
                        </div>
                        <div class="text-muted">
                            <code>{}</code>
                            {}
                            {}
                            {}
                        </div>
                    </div>
                    <div class="col-auto">
                        <form id="select-form-{}" action="/endpoints/{}/select" method="post" style="display: none;"></form>
                        <div class="btn-list">
                            <button
                                class="btn btn-sm btn-icon btn-success"
                                onclick="event.stopPropagation(); testConnection(event, {}, '{}')"
                                title="Test connection">
                                <i class="ti ti-plug-connected"></i>
                            </button>
                            <button
                                class="btn btn-sm btn-icon btn-ghost-primary"
                                onclick="event.stopPropagation(); openEditEndpoint(this);"
                                data-endpoint-id="{}"
                                data-endpoint-name="{}"
                                data-endpoint-url="{}"
                                data-endpoint-insecure="{}"
                                data-endpoint-index-pattern="{}"
                                data-endpoint-username="{}"
                                title="Edit endpoint">
                                <i class="ti ti-pencil"></i>
                            </button>
                            <button
                                class="btn btn-sm btn-icon btn-ghost-secondary"
                                onclick="event.stopPropagation(); document.getElementById('select-form-{}').submit();"
                                title="Use this endpoint">
                                <i class="ti ti-check"></i>
                            </button>
                            <button
                                class="btn btn-sm btn-icon btn-ghost-danger"
                                onclick="event.stopPropagation(); confirmDelete({}, '{}');"
                                title="Delete">
                                <i class="ti ti-trash"></i>
                            </button>
                        </div>
                    </div>
                </div>
            </div>"##,
            avatar_class,
            ep.id,
            ep.name,
            active_badge,
            ep.url,
            insecure_badge,
            username_badge,
            scope_badge,
            ep.id,
            ep.id,
            ep.id,
            ep.name,
            ep.id,
            escape_attr(&ep.name),
            escape_attr(&ep.url),
            ep.insecure,
            escape_attr(index_pattern_attr),
            escape_attr(username_attr),
            ep.id,
            ep.id,
            ep.name
        )
    }).collect();

    format!(
        r##"<div class="list-group list-group-flush">{}</div>"##,
        items.join("")
    )
}

#[derive(Deserialize)]
pub struct CreateEndpointForm {
    name: String,
    url: String,
    insecure: Option<String>,
    index_pattern: Option<String>,
    username: Option<String>,
    password: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateEndpointForm {
    name: String,
    url: String,
    insecure: Option<String>,
    index_pattern: Option<String>,
    username: Option<String>,
    password: Option<String>,
}

/// GET /endpoints - Zobrazí seznam endpointů
pub async fn list_endpoints(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
) -> Result<Html<String>, (StatusCode, String)> {
    if state.db.is_none() {
        let target = if state.base_path == "/" {
            "/dashboard".to_string()
        } else {
            format!("{}/dashboard", state.base_path)
        };
        let html = format!(
            r#"<div class="empty">
                <div class="empty-icon"><i class="ti ti-server-off"></i></div>
                <p class="empty-title">Stateless mode</p>
                <p class="empty-subtitle text-muted">Endpoints are disabled.</p>
                <div class="empty-action">
                    <a href="{}" class="btn btn-primary">Go to dashboard</a>
                </div>
            </div>"#,
            target
        );
        return Ok(Html(html));
    }
    let db = state.db.as_ref().unwrap();
    let endpoints = db
        .get_endpoints()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let active_endpoint = get_active_endpoint(&state, &jar).await;
    let ctx = PageContext::new(
        active_endpoint,
        state.base_path.clone(),
        state.logout_url.clone(),
    );

    let template = EndpointsTemplate { endpoints, ctx };

    template
        .render()
        .map(Html)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

/// POST /endpoints - Vytvoří nový endpoint
pub async fn create_endpoint(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Form(form): Form<CreateEndpointForm>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    if state.db.is_none() {
        return Err((StatusCode::BAD_REQUEST, "Stateless mode".to_string()));
    }
    let db = state.db.as_ref().unwrap();
    let create_endpoint = CreateEndpoint {
        name: form.name,
        url: form.url,
        insecure: form.insecure.is_some(),
        index_pattern: form.index_pattern.filter(|s| !s.trim().is_empty()),
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

    if let Err(e) = db.create_endpoint(create_endpoint).await {
        tracing::error!("Failed to create endpoint: {}", e);
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to save endpoint: {}", e),
        ));
    }

    // Vrátíme aktualizovaný seznam endpointů (pro HTMX swap)
    let endpoints = db
        .get_endpoints()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let active_id = get_active_endpoint(&state, &jar).await.map(|ep| ep.id);
    let html = render_endpoints_list(&endpoints, active_id);

    Ok(Html(html))
}

/// PUT /endpoints/:id - Aktualizuje endpoint
pub async fn update_endpoint(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    jar: CookieJar,
    Form(form): Form<UpdateEndpointForm>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    if state.db.is_none() {
        return Err((StatusCode::BAD_REQUEST, "Stateless mode".to_string()));
    }
    let db = state.db.as_ref().unwrap();
    let update_endpoint = UpdateEndpoint {
        name: Some(form.name),
        url: Some(form.url),
        insecure: Some(form.insecure.is_some()),
        index_pattern: form.index_pattern.filter(|s| !s.trim().is_empty()),
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

    db.update_endpoint(id, update_endpoint)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let endpoints = db
        .get_endpoints()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let active_id = get_active_endpoint(&state, &jar).await.map(|ep| ep.id);
    let html = render_endpoints_list(&endpoints, active_id);

    Ok(Html(html))
}

/// DELETE /endpoints/:id - Smaže endpoint
pub async fn delete_endpoint(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    jar: CookieJar,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    if state.db.is_none() {
        return Err((StatusCode::BAD_REQUEST, "Stateless mode".to_string()));
    }
    let db = state.db.as_ref().unwrap();
    db.delete_endpoint(id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Vrátíme aktualizovaný seznam
    let endpoints = db
        .get_endpoints()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let active_id = get_active_endpoint(&state, &jar).await.map(|ep| ep.id);
    let html = render_endpoints_list(&endpoints, active_id);

    Ok(Html(html))
}

/// POST /endpoints/:id/select - Vybere endpoint jako aktivní
pub async fn select_endpoint(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    jar: CookieJar,
) -> Result<(CookieJar, Redirect), (StatusCode, String)> {
    if state.db.is_none() {
        return Err((StatusCode::BAD_REQUEST, "Stateless mode".to_string()));
    }
    let db = state.db.as_ref().unwrap();
    // Ověř že endpoint existuje
    let endpoint = db
        .get_endpoint(id)
        .await
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

    let target = if state.base_path == "/" {
        "/dashboard".to_string()
    } else {
        format!("{}/dashboard", state.base_path)
    };
    Ok((jar, Redirect::to(&target)))
}

/// Helper funkce - získá aktivní endpoint z cookie
pub async fn get_active_endpoint(
    state: &AppState,
    jar: &CookieJar,
) -> Option<crate::db::models::Endpoint> {
    if let Some(endpoint) = &state.stateless_endpoint {
        return Some(endpoint.clone());
    }
    let db = state.db.as_ref()?;
    let endpoint_id = jar.get("active_endpoint_id")?.value().parse::<i64>().ok()?;

    db.get_endpoint(endpoint_id).await.ok()?
}

pub async fn get_endpoint_password(
    state: &AppState,
    endpoint: &crate::db::models::Endpoint,
) -> Option<String> {
    if let Some(db) = &state.db {
        db.get_endpoint_password(endpoint).await
    } else {
        state.stateless_password.clone()
    }
}

/// POST /endpoints/:id/test - Otestuje připojení k endpointu
pub async fn test_endpoint(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<axum::Json<serde_json::Value>, (StatusCode, String)> {
    use crate::es::EsClient;

    if state.db.is_none() {
        return Err((StatusCode::BAD_REQUEST, "Stateless mode".to_string()));
    }
    let db = state.db.as_ref().unwrap();
    // Získej endpoint
    let endpoint = db
        .get_endpoint(id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let endpoint = match endpoint {
        Some(e) => e,
        None => return Err((StatusCode::NOT_FOUND, "Endpoint not found".to_string())),
    };

    // Získej heslo pokud existuje
    let password = get_endpoint_password(&state, &endpoint).await;

    // Vytvoř ES klienta
    let mut client = EsClient::new(
        endpoint.url.clone(),
        endpoint.insecure,
        endpoint.username.clone(),
        password,
    )
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Zkus se připojit a získat verzi
    match client.detect_version().await {
        Ok(version) => Ok(axum::Json(serde_json::json!({
            "success": true,
            "message": "Připojení úspěšné",
            "version": format!("{}.{}.{}", version.major, version.minor, version.patch)
        }))),
        Err(e) => Ok(axum::Json(serde_json::json!({
            "success": false,
            "message": format!("Připojení selhalo: {}", e)
        }))),
    }
}
