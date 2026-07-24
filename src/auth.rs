use std::sync::Arc;

use axum::Json;
use axum::body::Body;
use axum::extract::{Extension, Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum Role {
    Viewer,
    Editor,
    Admin,
}

#[derive(Debug, Clone, Serialize)]
pub struct IdentityHeader {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProxyIdentity {
    pub subject: Option<String>,
    pub username: String,
    pub email: Option<String>,
    pub groups: Vec<String>,
    pub role: Role,
    pub headers: Vec<IdentityHeader>,
}

#[derive(Debug, Serialize)]
pub struct AuthSession {
    pub mode: &'static str,
    pub subject: Option<String>,
    pub username: String,
    pub email: Option<String>,
    pub groups: Vec<String>,
    pub role: Role,
    pub headers: Vec<IdentityHeader>,
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
    let username = first_header(headers, &["x-auth-user", "x-webauth-user"])?;
    let subject = first_header(headers, &["x-auth-subject", "x-webauth-subject"]);
    let email = first_header(headers, &["x-auth-email", "x-webauth-email"]);
    let groups = parse_groups(
        &first_header(headers, &["x-auth-groups", "x-webauth-groups"]).unwrap_or_default(),
    );
    let explicit_role = first_header(headers, &["x-auth-role", "x-webauth-role"])
        .and_then(|value| role_from_name(&value));
    let role = explicit_role.or_else(|| role_from_groups(&groups, config))?;

    Some(ProxyIdentity {
        subject,
        username,
        email,
        groups,
        role,
        headers: visible_identity_headers(headers),
    })
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

fn visible_identity_headers(headers: &axum::http::HeaderMap) -> Vec<IdentityHeader> {
    const VISIBLE_HEADERS: [(&str, &str); 10] = [
        ("x-auth-subject", "X-Auth-Subject"),
        ("x-auth-user", "X-Auth-User"),
        ("x-auth-email", "X-Auth-Email"),
        ("x-auth-groups", "X-Auth-Groups"),
        ("x-auth-role", "X-Auth-Role"),
        ("x-webauth-subject", "X-WEBAUTH-SUBJECT"),
        ("x-webauth-user", "X-WEBAUTH-USER"),
        ("x-webauth-email", "X-WEBAUTH-EMAIL"),
        ("x-webauth-groups", "X-WEBAUTH-GROUPS"),
        ("x-webauth-role", "X-WEBAUTH-ROLE"),
    ];

    VISIBLE_HEADERS
        .iter()
        .filter_map(|(lookup, display)| {
            headers
                .get(*lookup)
                .and_then(|value| value.to_str().ok())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| IdentityHeader {
                    name: (*display).to_string(),
                    value: value.to_string(),
                })
        })
        .collect()
}

pub async fn session(identity: Option<Extension<ProxyIdentity>>) -> Json<AuthSession> {
    let session = match identity {
        Some(Extension(identity)) => AuthSession {
            mode: "trusted-proxy",
            subject: identity.subject,
            username: identity.username,
            email: identity.email,
            groups: identity.groups,
            role: identity.role,
            headers: identity.headers,
        },
        None => AuthSession {
            mode: "local",
            subject: None,
            username: "local".to_string(),
            email: None,
            groups: Vec::new(),
            role: Role::Admin,
            headers: Vec::new(),
        },
    };
    Json(session)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{HeaderMap, HeaderValue};

    fn config() -> AuthConfig {
        AuthConfig {
            enabled: true,
            admin_group: "elastic-explorer:admin".to_string(),
            editor_group: "elastic-explorer:editor".to_string(),
            viewer_group: "elastic-explorer:viewer".to_string(),
        }
    }

    #[test]
    fn canonical_headers_take_precedence_and_are_preserved_for_ui() {
        let mut headers = HeaderMap::new();
        headers.insert("x-auth-subject", HeaderValue::from_static("subject-1"));
        headers.insert("x-auth-user", HeaderValue::from_static("canonical"));
        headers.insert("x-webauth-user", HeaderValue::from_static("alias"));
        headers.insert(
            "x-auth-email",
            HeaderValue::from_static("user@example.test"),
        );
        headers.insert(
            "x-auth-groups",
            HeaderValue::from_static("elastic-explorer:admin"),
        );

        let identity = identity_from_headers(&headers, &config()).unwrap();
        assert_eq!(identity.username, "canonical");
        assert_eq!(identity.subject.as_deref(), Some("subject-1"));
        assert_eq!(identity.role, Role::Admin);
        assert!(
            identity
                .headers
                .iter()
                .any(|header| header.name == "X-Auth-User")
        );
        assert!(
            identity
                .headers
                .iter()
                .any(|header| header.name == "X-WEBAUTH-USER")
        );
    }

    #[test]
    fn webauth_aliases_are_accepted() {
        let mut headers = HeaderMap::new();
        headers.insert("x-webauth-user", HeaderValue::from_static("mares"));
        headers.insert("x-webauth-role", HeaderValue::from_static("Editor"));

        let identity = identity_from_headers(&headers, &config()).unwrap();
        assert_eq!(identity.username, "mares");
        assert_eq!(identity.role, Role::Editor);
    }

    #[test]
    fn missing_identity_or_role_is_rejected() {
        let mut headers = HeaderMap::new();
        headers.insert("x-auth-role", HeaderValue::from_static("Viewer"));
        assert!(identity_from_headers(&headers, &config()).is_none());

        let mut headers = HeaderMap::new();
        headers.insert("x-auth-user", HeaderValue::from_static("mares"));
        assert!(identity_from_headers(&headers, &config()).is_none());
    }
}
