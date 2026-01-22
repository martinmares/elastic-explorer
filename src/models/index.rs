use serde::{Deserialize, Serialize};
use crate::utils::{format_bytes, format_number, parse_size_to_bytes};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct IndexInfo {
    pub health: String,        // green, yellow, red
    pub status: String,        // open, close
    pub index: String,         // název indexu
    pub uuid: String,
    pub pri: String,           // primary shards
    pub rep: String,           // replicas
    #[serde(rename = "docs.count")]
    pub docs_count: String,    // počet dokumentů
    #[serde(rename = "docs.deleted")]
    pub docs_deleted: String,  // smazané dokumenty
    #[serde(rename = "store.size")]
    pub store_size: String,    // celková velikost
    #[serde(rename = "pri.store.size")]
    pub pri_store_size: String, // velikost primary shards
    #[serde(skip)]
    pub aliases: Vec<String>,  // seznam aliasů
}

#[derive(Debug, Deserialize)]
pub struct AliasInfo {
    pub alias: String,
    pub index: String,
}

impl IndexInfo {
    /// Parsuje docs_count jako číslo pro sorting
    pub fn docs_count_num(&self) -> u64 {
        self.docs_count.parse().unwrap_or(0)
    }

    /// Vrátí docs_count jako formátované číslo s mezerami
    pub fn docs_count_formatted(&self) -> String {
        format_number(self.docs_count_num())
    }

    /// Parsuje store_size jako bytes pro sorting
    pub fn store_size_bytes(&self) -> u64 {
        parse_size_to_bytes(&self.store_size)
    }

    /// Vrátí store_size jako human-readable formát (KB, MB, GB, ...)
    pub fn store_size_formatted(&self) -> String {
        format_bytes(self.store_size_bytes())
    }

}

#[derive(Debug, Serialize, Deserialize)]
pub struct IndicesListData {
    pub indices: Vec<IndexInfo>,
    pub total: usize,
    pub page: usize,
    pub per_page: usize,
    pub total_pages: usize,
    pub filter: String,
    pub sort_by: String,
    pub sort_order: String, // asc, desc
    pub hide_internal: bool,
}

impl IndicesListData {
    pub fn showing_from(&self) -> usize {
        (self.page - 1) * self.per_page + 1
    }

    pub fn showing_to(&self) -> usize {
        let end = (self.page - 1) * self.per_page + self.indices.len();
        end.min(self.total)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IndexDetail {
    pub index_name: String,
    pub health: String,
    pub status: String,
    pub uuid: String,
    pub pri: String,
    pub rep: String,
    pub docs_count: String,
    pub docs_deleted: String,
    pub store_size: String,
    pub pri_store_size: String,
    pub aliases: Vec<String>,
    pub settings: Option<String>,  // JSON formatted
    pub mappings: Option<String>,  // JSON formatted
    pub stats: Option<String>,     // JSON formatted
    pub stats_docs_count: Option<u64>,
    pub stats_docs_deleted: Option<u64>,
    pub stats_store_size_bytes: Option<u64>,
    pub stats_pri_store_size_bytes: Option<u64>,
    pub stats_segments_count: Option<u64>,
    pub stats_segments_memory_bytes: Option<u64>,
    pub stats_search_query_total: Option<u64>,
    pub stats_search_query_time_ms: Option<u64>,
    pub stats_indexing_total: Option<u64>,
    pub stats_indexing_time_ms: Option<u64>,
    pub stats_primary_store_ratio: Option<u8>,
    pub stats_deleted_ratio: Option<u8>,
    pub stats_segments_memory_ratio: Option<u8>,
}

impl IndexDetail {
    /// Vrátí docs_count jako formátované číslo s mezerami
    pub fn docs_count_formatted(&self) -> String {
        let num: u64 = self.docs_count.parse().unwrap_or(0);
        format_number(num)
    }

    /// Vrátí docs_deleted jako formátované číslo s mezerami
    pub fn docs_deleted_formatted(&self) -> String {
        let num: u64 = self.docs_deleted.parse().unwrap_or(0);
        format_number(num)
    }

    /// Vrátí store_size jako human-readable formát
    pub fn store_size_formatted(&self) -> String {
        format_bytes(parse_size_to_bytes(&self.store_size))
    }

    /// Vrátí pri_store_size jako human-readable formát
    pub fn pri_store_size_formatted(&self) -> String {
        format_bytes(parse_size_to_bytes(&self.pri_store_size))
    }

    pub fn stats_docs_count_formatted(&self) -> String {
        format_number(self.stats_docs_count.unwrap_or(0))
    }

    pub fn stats_docs_deleted_formatted(&self) -> String {
        format_number(self.stats_docs_deleted.unwrap_or(0))
    }

    pub fn stats_store_size_formatted(&self) -> String {
        format_bytes(self.stats_store_size_bytes.unwrap_or(0))
    }

    pub fn stats_pri_store_size_formatted(&self) -> String {
        format_bytes(self.stats_pri_store_size_bytes.unwrap_or(0))
    }

    pub fn stats_segments_memory_formatted(&self) -> String {
        format_bytes(self.stats_segments_memory_bytes.unwrap_or(0))
    }

    pub fn stats_search_query_total_formatted(&self) -> String {
        format_number(self.stats_search_query_total.unwrap_or(0))
    }

    pub fn stats_indexing_total_formatted(&self) -> String {
        format_number(self.stats_indexing_total.unwrap_or(0))
    }

    pub fn stats_search_time_formatted(&self) -> String {
        format!("{} ms", format_number(self.stats_search_query_time_ms.unwrap_or(0)))
    }

    pub fn stats_indexing_time_formatted(&self) -> String {
        format!("{} ms", format_number(self.stats_indexing_time_ms.unwrap_or(0)))
    }
}
