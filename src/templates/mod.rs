use crate::db::models::Endpoint;
use askama::Template;

// Shared context pro všechny stránky
#[derive(Clone)]
pub struct PageContext {
    pub active_endpoint: Option<Endpoint>,
    pub version: &'static str,
    pub base_path: String,
}

impl PageContext {
    pub fn new(active_endpoint: Option<Endpoint>, base_path: String) -> Self {
        Self {
            active_endpoint,
            version: env!("CARGO_PKG_VERSION"),
            base_path,
        }
    }
}

#[derive(Template)]
#[template(path = "endpoints.html")]
pub struct EndpointsTemplate {
    pub endpoints: Vec<Endpoint>,
    pub ctx: PageContext,
}

#[derive(Template)]
#[template(path = "dashboard.html")]
pub struct DashboardTemplate {
    pub endpoint_name: Option<String>,
    pub ctx: PageContext,
    pub data: Option<crate::models::DashboardData>,
}

#[derive(Template)]
#[template(path = "node_detail.html")]
pub struct NodeDetailTemplate {
    pub ctx: PageContext,
    pub data: Option<crate::models::NodeDetail>,
    pub node_id: String,
}

#[derive(Template)]
#[template(path = "indices.html")]
pub struct IndicesTemplate {
    pub ctx: PageContext,
    pub data: Option<crate::models::IndicesListData>,
}

#[derive(Template)]
#[template(path = "indices_table.html")]
pub struct IndicesTableTemplate {
    pub ctx: PageContext,
    pub data: Option<crate::models::IndicesListData>,
}

#[derive(Template)]
#[template(path = "indices_summary.html")]
pub struct IndicesSummaryTemplate {
    pub data: crate::handlers::indices::IndicesSummaryData,
}

#[derive(Template)]
#[template(path = "index_detail.html")]
pub struct IndexDetailTemplate {
    pub data: Option<crate::models::IndexDetail>,
}

#[derive(Template)]
#[template(path = "search.html")]
pub struct SearchTemplate {
    pub ctx: PageContext,
    pub data: Option<crate::handlers::search::SearchResultsData>,
}

#[derive(Template)]
#[template(path = "search_results.html")]
pub struct SearchResultsTemplate {
    pub data: Option<crate::handlers::search::SearchResultsData>,
}

#[derive(Template)]
#[template(path = "shards.html")]
pub struct ShardsTemplate {
    pub ctx: PageContext,
    pub data: Option<crate::handlers::shards::ShardsData>,
    pub pattern: String,
}

#[derive(Template)]
#[template(path = "console.html")]
pub struct ConsoleTemplate {
    pub ctx: PageContext,
    pub data: Option<crate::handlers::console::ConsoleData>,
}
