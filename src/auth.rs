use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    Viewer,
    Editor,
    Admin,
}

#[derive(Debug, Clone)]
pub struct ProxyIdentity {
    pub role: Role,
}

#[derive(Debug, Clone)]
pub struct AuthConfig {
    pub enabled: bool,
    pub admin_group: String,
    pub editor_group: String,
    pub viewer_group: String,
}

pub async fn require_viewer(
    State(config): State<Arc<AuthConfig>>,
    req: Request<Body>,
    next: Next,
) -> Response {
    require_role(config, req, next, Role::Viewer).await
}

pub async fn require_editor(
    State(config): State<Arc<AuthConfig>>,
    req: Request<Body>,
    next: Next,
) -> Response {
    require_role(config, req, next, Role::Editor).await
}

pub async fn require_admin(
    State(config): State<Arc<AuthConfig>>,
    req: Request<Body>,
    next: Next,
) -> Response {
    require_role(config, req, next, Role::Admin).await
}

async fn require_role(
    config: Arc<AuthConfig>,
    mut req: Request<Body>,
    next: Next,
    required: Role,
) -> Response {
    if !config.enabled {
        return next.run(req).await;
    }

    let Some(identity) = identity_from_headers(req.headers(), &config) else {
        return (StatusCode::UNAUTHORIZED, "trusted proxy identity required").into_response();
    };

    if identity.role < required {
        return (StatusCode::FORBIDDEN, "insufficient role").into_response();
    }

    req.extensions_mut().insert(identity);
    next.run(req).await
}

fn identity_from_headers(
    headers: &axum::http::HeaderMap,
    config: &AuthConfig,
) -> Option<ProxyIdentity> {
    let _username = first_header(headers, &["x-auth-user", "x-webauth-user"])?;
    let groups = parse_groups(
        &first_header(headers, &["x-auth-groups", "x-webauth-groups"]).unwrap_or_default(),
    );
    let explicit_role = first_header(headers, &["x-auth-role", "x-webauth-role"])
        .and_then(|value| role_from_name(&value));
    let role = explicit_role.or_else(|| role_from_groups(&groups, config))?;

    Some(ProxyIdentity { role })
}

fn first_header(headers: &axum::http::HeaderMap, names: &[&str]) -> Option<String> {
    names
        .iter()
        .find_map(|name| headers.get(*name))
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn parse_groups(input: &str) -> Vec<String> {
    input
        .split(|c: char| c == ',' || c == ';' || c.is_whitespace())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn role_from_name(value: &str) -> Option<Role> {
    match value.trim().to_ascii_lowercase().as_str() {
        "admin" => Some(Role::Admin),
        "editor" => Some(Role::Editor),
        "viewer" => Some(Role::Viewer),
        _ => None,
    }
}

fn role_from_groups(groups: &[String], config: &AuthConfig) -> Option<Role> {
    if groups.iter().any(|group| group == &config.admin_group) {
        Some(Role::Admin)
    } else if groups.iter().any(|group| group == &config.editor_group) {
        Some(Role::Editor)
    } else if groups.iter().any(|group| group == &config.viewer_group) {
        Some(Role::Viewer)
    } else {
        None
    }
}
