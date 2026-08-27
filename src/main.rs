mod auth;
mod config;
mod db;
mod es;
mod handlers;
mod models;
mod static_assets;
mod templates;
mod utils;

use anyhow::Result;
use axum::{
    Router, middleware,
    routing::{delete, get, post},
};
use chrono::Utc;
use clap::Parser;
use std::sync::Arc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use handlers::AppState;

#[derive(Parser, Debug)]
#[command(name = "elastic-explorer")]
#[command(about = "Elasticsearch cluster explorer", long_about = None)]
struct Args {
    /// Host pro HTTP server
    #[arg(long, env = "HOST", default_value = "127.0.0.1")]
    host: String,

    /// Port pro HTTP server
    #[arg(short, long, env = "PORT", default_value = "8080")]
    port: u16,

    /// Neotvírat prohlížeč automaticky
    #[arg(long = "no-open", alias = "no-browser")]
    no_open: bool,

    /// Base path when running behind reverse proxy (e.g. /elastic-explorer)
    #[arg(long, env = "BASE_PATH", default_value = "/")]
    base_path: String,

    /// Logout URL exposed by an upstream authentication proxy
    #[arg(long, env = "LOGOUT_URL")]
    logout_url: Option<String>,

    /// Stateless mode: no local storage, use single connection from CLI/.env
    #[arg(long, default_value_t = false)]
    stateless: bool,

    /// Connection name (shown in UI)
    #[arg(long, env = "CONF_ES_NAME")]
    conf_es_name: Option<String>,

    /// Elasticsearch URL (e.g. https://host:9200)
    #[arg(long, env = "CONF_ES_URL")]
    conf_es_url: Option<String>,

    /// Elasticsearch username
    #[arg(long, env = "CONF_ES_USERNAME")]
    conf_es_username: Option<String>,

    /// Elasticsearch password
    #[arg(long, env = "CONF_ES_PASSWORD")]
    conf_es_password: Option<String>,

    /// Allow insecure TLS
    #[arg(long, env = "CONF_ES_INSECURE", default_value_t = false)]
    conf_es_insecure: bool,

    /// Trust X-Auth-* / X-WEBAUTH-* headers from an upstream authentication proxy
    #[arg(long, env = "TRUSTED_PROXY_AUTH", default_value_t = false)]
    trusted_proxy_auth: bool,

    /// Group that grants Admin role
    #[arg(
        long,
        env = "AUTH_GROUP_ADMIN",
        default_value = "elastic-explorer:admin"
    )]
    auth_group_admin: String,

    /// Group that grants Editor role
    #[arg(
        long,
        env = "AUTH_GROUP_EDITOR",
        default_value = "elastic-explorer:editor"
    )]
    auth_group_editor: String,

    /// Group that grants Viewer role
    #[arg(
        long,
        env = "AUTH_GROUP_VIEWER",
        default_value = "elastic-explorer:viewer"
    )]
    auth_group_viewer: String,

    /// Enable native Elasticsearch snapshots (stateless mode only)
    #[arg(long, env = "SNAPSHOTS_ENABLED", default_value_t = false)]
    snapshots_enabled: bool,

    /// Elasticsearch snapshot repository name
    #[arg(long, env = "SNAPSHOT_REPOSITORY")]
    snapshot_repository: Option<String>,

    /// Literal managed index prefix; the effective pattern is PREFIX*
    #[arg(long, env = "SNAPSHOT_INDEX_PREFIX")]
    snapshot_index_prefix: Option<String>,

    /// Elasticsearch SLM cron expression (for example: 0 0 20 * * ?)
    #[arg(long, env = "SCHEDULED_SNAPSHOT_CRON")]
    scheduled_snapshot_cron: Option<String>,

    /// Minimum number of automatic snapshots retained by SLM
    #[arg(long, env = "SCHEDULED_SNAPSHOT_KEEP_LAST", default_value_t = 14)]
    scheduled_snapshot_keep_last: u32,

    /// Automatic snapshot age after which SLM may delete it
    #[arg(long, env = "SCHEDULED_SNAPSHOT_MAX_AGE_DAYS", default_value_t = 30)]
    scheduled_snapshot_max_age_days: u32,

    /// Note stored in automatic snapshot metadata
    #[arg(
        long,
        env = "SCHEDULED_SNAPSHOT_NOTE",
        default_value = "Automatic snapshot"
    )]
    scheduled_snapshot_note: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Inicializuj logging
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "elastic_explorer=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Parse CLI argumenty
    let args = Args::parse();

    tracing::info!("Starting Elastic Explorer...");

    let db = if args.stateless {
        None
    } else {
        // Inicializuj adresáře
        config::init_directories()?;
        // Inicializuj databázi
        let db = db::Database::new().await?;
        tracing::info!("Database initialized successfully");
        Some(db)
    };

    // Shared state
    let base_path = normalize_base_path(&args.base_path);
    let stateless_endpoint = if args.stateless {
        let url = args
            .conf_es_url
            .clone()
            .ok_or_else(|| anyhow::anyhow!("--conf-es-url is required in --stateless mode"))?;
        let name = args.conf_es_name.clone().unwrap_or_else(|| url.clone());
        Some(db::models::Endpoint {
            id: 0,
            name,
            url,
            insecure: args.conf_es_insecure,
            index_pattern: None,
            username: args.conf_es_username.clone(),
            password_encrypted: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
    } else {
        None
    };

    let logout_url = args.logout_url.as_deref().map(normalize_logout_url);
    tracing::info!(
        logout_url = logout_url.as_deref().unwrap_or("-"),
        "auth proxy logout URL"
    );

    let snapshots = handlers::snapshots::SnapshotConfig::from_args(
        args.snapshots_enabled,
        args.stateless,
        args.snapshot_repository,
        args.snapshot_index_prefix,
        args.scheduled_snapshot_cron,
        args.scheduled_snapshot_keep_last,
        args.scheduled_snapshot_max_age_days,
        args.scheduled_snapshot_note,
    )?;

    let state = Arc::new(AppState {
        db,
        base_path: base_path.clone(),
        logout_url,
        stateless_endpoint,
        stateless_password: if args.stateless {
            args.conf_es_password.clone()
        } else {
            None
        },
        snapshots,
    });

    handlers::snapshots::initialize(&state).await?;

    let auth_config = Arc::new(auth::AuthConfig {
        enabled: args.trusted_proxy_auth,
        admin_group: args.auth_group_admin.clone(),
        editor_group: args.auth_group_editor.clone(),
        viewer_group: args.auth_group_viewer.clone(),
    });

    // Vytvoř axum router
    let viewer_router = Router::new()
        .route("/", get(handlers::index))
        .route("/health", get(handlers::health))
        .route("/dashboard", get(handlers::dashboard::dashboard))
        .route("/endpoints", get(handlers::endpoints::list_endpoints))
        .route("/nodes/{id}", get(handlers::nodes::node_detail))
        .route("/nodes/{id}/metrics", get(handlers::nodes::node_metrics))
        .route("/indices", get(handlers::indices::list_indices))
        .route("/indices/table", get(handlers::indices::indices_table))
        .route("/indices/summary", get(handlers::indices::indices_summary))
        .route("/indices/metrics", get(handlers::indices::indices_metrics))
        .route(
            "/indices/detail/{index_name}",
            get(handlers::indices::index_detail),
        )
        .route(
            "/indices/{index_name}/new-mapping",
            get(handlers::indices::new_mapping_prepare),
        )
        .route(
            "/indices/{index_name}/aliases",
            get(handlers::indices::index_aliases),
        )
        .route("/search", get(handlers::search::search_page))
        .route("/shards", get(handlers::shards::shards_page))
        .route("/console", get(handlers::console::console_page))
        .route(
            "/console/history-table",
            get(handlers::console::console_history_table),
        )
        .route("/auth/session", get(auth::session))
        .route("/static/{*path}", get(static_assets::serve))
        .layer(middleware::from_fn_with_state(
            auth_config.clone(),
            auth::require_viewer,
        ));

    let editor_router = Router::new()
        .route(
            "/indices/{index_name}/new-mapping",
            post(handlers::indices::new_mapping_create),
        )
        .route(
            "/indices/{index_name}/aliases",
            post(handlers::indices::index_alias_action),
        )
        .route(
            "/indices/bulk/{action}/{index_name}",
            post(handlers::indices::bulk_operation),
        )
        .layer(middleware::from_fn_with_state(
            auth_config.clone(),
            auth::require_editor,
        ));

    let admin_router = Router::new()
        .route("/endpoints", post(handlers::endpoints::create_endpoint))
        .route(
            "/endpoints/{id}",
            axum::routing::put(handlers::endpoints::update_endpoint),
        )
        .route(
            "/endpoints/{id}",
            delete(handlers::endpoints::delete_endpoint),
        )
        .route(
            "/endpoints/{id}/select",
            post(handlers::endpoints::select_endpoint),
        )
        .route(
            "/endpoints/{id}/test",
            post(handlers::endpoints::test_endpoint),
        )
        .route(
            "/search/bulk/delete",
            post(handlers::search::bulk_delete_documents),
        )
        .route("/console/execute", post(handlers::console::execute_request))
        .route("/snapshots", get(handlers::snapshots::snapshots_page))
        .route("/snapshots/overview", get(handlers::snapshots::overview))
        .route(
            "/snapshots/create",
            post(handlers::snapshots::create_snapshot),
        )
        .route(
            "/snapshots/{name}/status",
            get(handlers::snapshots::snapshot_status),
        )
        .route(
            "/snapshots/{name}",
            delete(handlers::snapshots::delete_snapshot),
        )
        .route(
            "/snapshots/{name}/restore/safe",
            post(handlers::snapshots::restore_safe),
        )
        .route(
            "/snapshots/{name}/restore/in-place",
            post(handlers::snapshots::restore_in_place),
        )
        .route(
            "/snapshots/restore/status",
            post(handlers::snapshots::restore_status),
        )
        .layer(middleware::from_fn_with_state(
            auth_config.clone(),
            auth::require_admin,
        ));

    let router = viewer_router
        .merge(editor_router)
        .merge(admin_router)
        .with_state(state);
    let app = if base_path == "/" {
        router
    } else {
        Router::new().nest(&base_path, router)
    };

    // Adresa serveru
    let addr = format!("{}:{}", args.host, args.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    tracing::info!("Server listening on http://{}{}", addr, base_path);

    // Otevři prohlížeč
    if !args.no_open {
        let url = format!("http://{}{}", addr, base_path);
        if let Err(e) = utils::open_browser(&url) {
            tracing::warn!("Failed to open browser: {}", e);
            tracing::info!("Please open {} manually", url);
        }
    }

    // Spusť server
    tracing::info!("Server started successfully");
    axum::serve(listener, app).await?;

    Ok(())
}

fn normalize_base_path(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() || trimmed == "/" {
        return "/".to_string();
    }
    let mut path = trimmed.to_string();
    if !path.starts_with('/') {
        path.insert(0, '/');
    }
    while path.ends_with('/') {
        path.pop();
    }
    path
}

fn normalize_logout_url(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.starts_with('/') || trimmed.starts_with("http://") || trimmed.starts_with("https://")
    {
        trimmed.to_string()
    } else {
        format!("/{trimmed}")
    }
}
