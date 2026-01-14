use serde::{Deserialize, Serialize};
use crate::utils::format_number;

#[derive(Debug, Serialize, Deserialize)]
pub struct DashboardData {
    pub cluster_name: String,
    pub cluster_status: String, // green, yellow, red
    pub nodes_total: u32,
    pub nodes_data: u32,
    pub indices_count: u32,
    pub documents_count: u64,
    pub store_size: String,
    pub nodes: Vec<NodeSummary>,
}

impl DashboardData {
    /// Vrátí documents_count jako formátované číslo s mezerami
    pub fn documents_count_formatted(&self) -> String {
        format_number(self.documents_count)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NodeSummary {
    pub id: String,
    pub name: String,
    pub roles: Vec<String>,
    pub is_master: bool,
    pub cpu_percent: Option<u8>,
    pub heap_percent: Option<u8>,
    pub ram_percent: Option<u8>,
    pub disk_used_percent: Option<u8>,
}
