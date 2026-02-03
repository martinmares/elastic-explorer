mod config;
mod db;
mod es;
mod handlers;
mod models;
mod templates;
mod utils;

use anyhow::Result;
use axum::{
    routing::{get, post, delete},
    Router,
};
use clap::Parser;
use std::sync::Arc;
use tower_http::services::ServeDir;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use chrono::Utc;

use handlers::AppState;

#[derive(Parser, Debug)]
#[command(name = "elastic-explorer")]
#[command(about = "Elasticsearch cluster explorer", long_about = None)]
struct Args {
    /// Host pro HTTP server
    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    /// Port pro HTTP server
    #[arg(short, long, default_value = "8080")]
    port: u16,

    /// Neotvírat prohlížeč automaticky
    #[arg(long = "no-open", alias = "no-browser")]
    no_open: bool,

    /// Base path when running behind reverse proxy (e.g. /elastic-explorer)
    #[arg(long, default_value = "/")]
    base_path: String,

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
}

#[tokio::main]
async fn main() -> Result<()> {
    let _ = dotenvy::dotenv();
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
        let url = args.conf_es_url.clone().ok_or_else(|| anyhow::anyhow!("--conf-es-url is required in --stateless mode"))?;
        let name = args.conf_es_name.clone().unwrap_or_else(|| url.clone());
        Some(db::models::Endpoint {
            id: 0,
            name,
            url,
            insecure: args.conf_es_insecure,
            username: args.conf_es_username.clone(),
            password_encrypted: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
    } else {
        None
    };

    let state = Arc::new(AppState {
        db,
        base_path: base_path.clone(),
        stateless_endpoint,
        stateless_password: if args.stateless { args.conf_es_password.clone() } else { None },
    });

    // Vytvoř axum router
    let router = Router::new()
        .route("/", get(handlers::index))
        .route("/health", get(handlers::health))
        .route("/dashboard", get(handlers::dashboard::dashboard))
        .route("/endpoints", get(handlers::endpoints::list_endpoints))
        .route("/endpoints", post(handlers::endpoints::create_endpoint))
        .route("/endpoints/{id}", axum::routing::put(handlers::endpoints::update_endpoint))
        .route("/endpoints/{id}", delete(handlers::endpoints::delete_endpoint))
        .route("/endpoints/{id}/select", post(handlers::endpoints::select_endpoint))
        .route("/endpoints/{id}/test", post(handlers::endpoints::test_endpoint))
        .route("/nodes/{id}", get(handlers::nodes::node_detail))
        .route("/nodes/{id}/metrics", get(handlers::nodes::node_metrics))
        .route("/indices", get(handlers::indices::list_indices))
        .route("/indices/table", get(handlers::indices::indices_table))
        .route("/indices/summary", get(handlers::indices::indices_summary))
        .route("/indices/metrics", get(handlers::indices::indices_metrics))
        .route("/indices/detail/{index_name}", get(handlers::indices::index_detail))
        .route("/indices/{index_name}/aliases", get(handlers::indices::index_aliases))
        .route("/indices/{index_name}/aliases", post(handlers::indices::index_alias_action))
        .route("/indices/bulk/{action}/{index_name}", post(handlers::indices::bulk_operation))
        .route("/search", get(handlers::search::search_page))
        .route("/search/bulk/delete", post(handlers::search::bulk_delete_documents))
        .route("/shards", get(handlers::shards::shards_page))
        .route("/console", get(handlers::console::console_page))
        .route("/console/execute", post(handlers::console::execute_request))
        .route("/console/history-table", get(handlers::console::console_history_table))
        .nest_service("/static", ServeDir::new("static"))
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
