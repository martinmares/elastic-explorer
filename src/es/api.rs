use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::client::EsClient;

/// Cluster health response
#[derive(Debug, Serialize, Deserialize)]
pub struct ClusterHealth {
    pub cluster_name: String,
    pub status: String,
    pub timed_out: bool,
    pub number_of_nodes: u32,
    pub number_of_data_nodes: u32,
    pub active_primary_shards: u32,
    pub active_shards: u32,
    pub relocating_shards: u32,
    pub initializing_shards: u32,
    pub unassigned_shards: u32,
}

/// Cluster stats response (zjednodušená verze)
#[derive(Debug, Serialize, Deserialize)]
pub struct ClusterStats {
    pub cluster_name: String,
    pub nodes: NodesStats,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NodesStats {
    pub count: NodesCount,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NodesCount {
    pub total: u32,
    pub data: u32,
    pub master: u32,
}

/// Node info
#[derive(Debug, Serialize, Deserialize)]
pub struct NodeInfo {
    pub name: String,
    pub roles: Vec<String>,
    pub version: String,
}

/// Index info
#[derive(Debug, Serialize, Deserialize)]
pub struct IndexInfo {
    pub health: String,
    pub status: String,
    pub index: String,
    pub uuid: String,
    pub pri: String,
    pub rep: String,
    #[serde(rename = "docs.count")]
    pub docs_count: Option<String>,
    #[serde(rename = "store.size")]
    pub store_size: Option<String>,
}

impl EsClient {
    /// Získá cluster health
    pub async fn cluster_health(&self) -> Result<ClusterHealth> {
        self.get("/_cluster/health").await
    }

    /// Získá cluster stats
    pub async fn cluster_stats(&self) -> Result<ClusterStats> {
        self.get("/_cluster/stats").await
    }

    /// Získá seznam nodů
    pub async fn get_nodes(&self) -> Result<Value> {
        self.get("/_nodes").await
    }

    /// Získá detailní info o konkrétním nodu
    pub async fn get_node(&self, node_id: &str) -> Result<Value> {
        let path = format!("/_nodes/{}", node_id);
        self.get(&path).await
    }

    /// Získá seznam indexů (cat API)
    pub async fn get_indices(&self) -> Result<Vec<IndexInfo>> {
        self.get("/_cat/indices?format=json&h=health,status,index,uuid,pri,rep,docs.count,store.size").await
    }

    /// Získá detailní info o indexu
    pub async fn get_index(&self, index_name: &str) -> Result<Value> {
        let path = format!("/{}", index_name);
        self.get(&path).await
    }

    /// Smaže index
    pub async fn delete_index(&self, index_name: &str) -> Result<Value> {
        let path = format!("/{}", index_name);
        self.delete(&path).await
    }

    /// Získá seznam shardů
    pub async fn get_shards(&self) -> Result<Value> {
        self.get("/_cat/shards?format=json").await
    }

    /// Search pomocí Query DSL
    pub async fn search(&self, indices: &[String], query: Value) -> Result<Value> {
        let path = if indices.is_empty() {
            "/_search".to_string()
        } else {
            format!("/{}/_search", indices.join(","))
        };

        self.post(&path, query).await
    }

    /// Search pomocí SQL (ES 7.x+)
    pub async fn search_sql(&self, query: &str) -> Result<Value> {
        // Zkontroluj verzi
        if let Some(version) = self.version() {
            if version.major < 7 {
                return Err(anyhow::anyhow!("SQL API requires Elasticsearch 7.x or higher"));
            }
        }

        let body = json!({
            "query": query
        });

        self.post("/_sql", body).await
    }

    /// Získá mappings indexu
    pub async fn get_mapping(&self, index_name: &str) -> Result<Value> {
        let path = format!("/{}/_mapping", index_name);
        self.get(&path).await
    }

    /// Získá settings indexu
    pub async fn get_settings(&self, index_name: &str) -> Result<Value> {
        let path = format!("/{}/_settings", index_name);
        self.get(&path).await
    }

    /// Získá index templates
    pub async fn get_index_templates(&self) -> Result<Value> {
        // Pro ES 7.8+ použij /_index_template
        // Pro starší verze /_template
        if let Some(version) = self.version() {
            if version.major >= 8 || (version.major == 7 && version.minor >= 8) {
                self.get("/_index_template").await
            } else {
                self.get("/_template").await
            }
        } else {
            // Fallback na nové API
            self.get("/_index_template").await
        }
    }

    /// Získá component templates (ES 7.8+)
    pub async fn get_component_templates(&self) -> Result<Value> {
        if let Some(version) = self.version() {
            if version.major >= 8 || (version.major == 7 && version.minor >= 8) {
                return self.get("/_component_template").await;
            }
        }
        Err(anyhow::anyhow!("Component templates require Elasticsearch 7.8 or higher"))
    }
}
