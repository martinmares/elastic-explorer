use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct NodeDetail {
    pub id: String,
    pub name: String,
    pub roles: Vec<String>,
    pub ip: String,
    pub version: String,
    pub os_name: String,
    pub os_arch: String,
    pub jvm_version: String,

    // Metriky
    pub cpu_percent: Option<u8>,
    pub heap_percent: Option<u8>,
    pub heap_used_bytes: u64,
    pub heap_max_bytes: u64,
    pub ram_percent: Option<u8>,
    pub ram_used_bytes: u64,
    pub ram_total_bytes: u64,
    pub disk_percent: Option<u8>,
    pub disk_used_bytes: u64,
    pub disk_total_bytes: u64,
    pub docs_count: u64,
    pub docs_deleted: u64,
    pub store_size_bytes: u64,
}

impl NodeDetail {
    /// Formátuje byty na lidsky čitelný formát
    pub fn format_bytes(bytes: u64) -> String {
        const KB: u64 = 1024;
        const MB: u64 = KB * 1024;
        const GB: u64 = MB * 1024;
        const TB: u64 = GB * 1024;

        if bytes >= TB {
            format!("{:.2} TB", bytes as f64 / TB as f64)
        } else if bytes >= GB {
            format!("{:.2} GB", bytes as f64 / GB as f64)
        } else if bytes >= MB {
            format!("{:.2} MB", bytes as f64 / MB as f64)
        } else if bytes >= KB {
            format!("{:.2} KB", bytes as f64 / KB as f64)
        } else {
            format!("{} B", bytes)
        }
    }

    pub fn heap_used_formatted(&self) -> String {
        Self::format_bytes(self.heap_used_bytes)
    }

    pub fn heap_max_formatted(&self) -> String {
        Self::format_bytes(self.heap_max_bytes)
    }

    pub fn ram_used_formatted(&self) -> String {
        Self::format_bytes(self.ram_used_bytes)
    }

    pub fn ram_total_formatted(&self) -> String {
        Self::format_bytes(self.ram_total_bytes)
    }

    pub fn disk_used_formatted(&self) -> String {
        Self::format_bytes(self.disk_used_bytes)
    }

    pub fn disk_total_formatted(&self) -> String {
        Self::format_bytes(self.disk_total_bytes)
    }

    pub fn store_size_formatted(&self) -> String {
        Self::format_bytes(self.store_size_bytes)
    }
}
