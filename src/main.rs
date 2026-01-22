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
    #[arg(long)]
    no_browser: bool,
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

    // Inicializuj adresáře
    config::init_directories()?;

    // Inicializuj databázi
    let db = db::Database::new().await?;
    tracing::info!("Database initialized successfully");

    // Shared state
    let state = Arc::new(AppState { db });

    // Vytvoř axum router
    let app = Router::new()
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
        .route("/indices/metrics", get(handlers::indices::indices_metrics))
        .route("/indices/detail/{index_name}", get(handlers::indices::index_detail))
        .route("/indices/bulk/{action}/{index_name}", post(handlers::indices::bulk_operation))
        .route("/search", get(handlers::search::search_page))
        .route("/search/bulk/delete", post(handlers::search::bulk_delete_documents))
        .route("/shards", get(handlers::shards::shards_page))
        .route("/console", get(handlers::console::console_page))
        .route("/console/execute", post(handlers::console::execute_request))
        .route("/console/history-table", get(handlers::console::console_history_table))
        .nest_service("/static", ServeDir::new("static"))
        .with_state(state);

    // Adresa serveru
    let addr = format!("{}:{}", args.host, args.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    tracing::info!("Server listening on http://{}", addr);

    // Otevři prohlížeč
    if !args.no_browser {
        let url = format!("http://{}", addr);
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
