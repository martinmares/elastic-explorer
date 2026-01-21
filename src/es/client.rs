use anyhow::{anyhow, Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct EsClient {
    base_url: String,
    client: Client,
    username: Option<String>,
    password: Option<String>,
    version: Option<EsVersion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EsVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl EsVersion {
    pub fn from_string(version_str: &str) -> Result<Self> {
        // Očekáváme formát "7.17.0" nebo "8.11.1"
        let parts: Vec<&str> = version_str.split('.').collect();
        if parts.len() < 3 {
            return Err(anyhow!("Invalid version format: {}", version_str));
        }

        Ok(Self {
            major: parts[0].parse().context("Invalid major version")?,
            minor: parts[1].parse().context("Invalid minor version")?,
            patch: parts[2].parse().context("Invalid patch version")?,
        })
    }
}

#[derive(Debug, Deserialize)]
struct RootResponse {
    version: VersionInfo,
}

#[derive(Debug, Deserialize)]
struct VersionInfo {
    number: String,
}

impl EsClient {
    pub fn new(base_url: String, insecure: bool, username: Option<String>, password: Option<String>) -> Result<Self> {
        // Ořízni trailing slash
        let base_url = base_url.trim_end_matches('/').to_string();

        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .danger_accept_invalid_certs(insecure)
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self {
            base_url,
            client,
            username,
            password,
            version: None,
        })
    }

    /// Detekuje verzi Elasticsearch
    pub async fn detect_version(&mut self) -> Result<EsVersion> {
        let response: RootResponse = self.get("").await?;
        let version = EsVersion::from_string(&response.version.number)?;

        tracing::info!("Detected Elasticsearch version: {}.{}.{}",
            version.major, version.minor, version.patch);

        self.version = Some(version.clone());
        Ok(version)
    }

    /// Univerzální GET request
    pub async fn get<T>(&self, path: &str) -> Result<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        let url = format!("{}/{}", self.base_url, path.trim_start_matches('/'));

        let mut request = self.client.get(&url);

        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        }

        let response = request.send().await
            .context("Failed to send GET request")?;

        self.handle_response(response).await
    }

    /// Univerzální POST request
    pub async fn post<T>(&self, path: &str, body: Value) -> Result<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        let url = format!("{}/{}", self.base_url, path.trim_start_matches('/'));

        let mut request = self.client.post(&url).json(&body);

        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        }

        let response = request.send().await
            .context("Failed to send POST request")?;

        self.handle_response(response).await
    }

    /// Univerzální PUT request
    #[allow(dead_code)]
    pub async fn put<T>(&self, path: &str, body: Value) -> Result<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        let url = format!("{}/{}", self.base_url, path.trim_start_matches('/'));

        let mut request = self.client.put(&url).json(&body);

        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        }

        let response = request.send().await
            .context("Failed to send PUT request")?;

        self.handle_response(response).await
    }

    /// Univerzální DELETE request
    pub async fn delete<T>(&self, path: &str) -> Result<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        let url = format!("{}/{}", self.base_url, path.trim_start_matches('/'));

        let mut request = self.client.delete(&url);

        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        }

        let response = request.send().await
            .context("Failed to send DELETE request")?;

        self.handle_response(response).await
    }

    /// Raw GET request (returns text instead of JSON)
    pub async fn get_raw(&self, path: &str) -> Result<(u16, String)> {
        let url = format!("{}/{}", self.base_url, path.trim_start_matches('/'));

        let mut request = self.client.get(&url);

        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        }

        let response = request.send().await
            .context("Failed to send GET request")?;

        self.handle_raw_response(response).await
    }

    /// Raw POST request (returns text instead of JSON)
    pub async fn post_raw(&self, path: &str, body: Value) -> Result<(u16, String)> {
        let url = format!("{}/{}", self.base_url, path.trim_start_matches('/'));

        let mut request = self.client.post(&url).json(&body);

        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        }

        let response = request.send().await
            .context("Failed to send POST request")?;

        self.handle_raw_response(response).await
    }

    /// Raw PUT request (returns text instead of JSON)
    pub async fn put_raw(&self, path: &str, body: Value) -> Result<(u16, String)> {
        let url = format!("{}/{}", self.base_url, path.trim_start_matches('/'));

        let mut request = self.client.put(&url).json(&body);

        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        }

        let response = request.send().await
            .context("Failed to send PUT request")?;

        self.handle_raw_response(response).await
    }

    /// Raw DELETE request (returns text instead of JSON)
    pub async fn delete_raw(&self, path: &str) -> Result<(u16, String)> {
        let url = format!("{}/{}", self.base_url, path.trim_start_matches('/'));

        let mut request = self.client.delete(&url);

        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        }

        let response = request.send().await
            .context("Failed to send DELETE request")?;

        self.handle_raw_response(response).await
    }

    async fn handle_response<T>(&self, response: reqwest::Response) -> Result<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        let status = response.status();

        if !status.is_success() {
            let error_text = response.text().await
                .unwrap_or_else(|_| "Failed to read error response".to_string());
            return Err(anyhow!("Elasticsearch error ({}): {}", status, error_text));
        }

        let body = response.json::<T>().await
            .context("Failed to parse response JSON")?;

        Ok(body)
    }

    async fn handle_raw_response(&self, response: reqwest::Response) -> Result<(u16, String)> {
        let status = response.status();
        let status_code = status.as_u16();

        let body = response.text().await
            .context("Failed to read response text")?;

        Ok((status_code, body))
    }

    #[allow(dead_code)]
    pub fn version(&self) -> Option<&EsVersion> {
        self.version.as_ref()
    }

    #[allow(dead_code)]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_parsing() {
        let v = EsVersion::from_string("7.17.0").unwrap();
        assert_eq!(v.major, 7);
        assert_eq!(v.minor, 17);
        assert_eq!(v.patch, 0);

        let v2 = EsVersion::from_string("8.11.1").unwrap();
        assert_eq!(v2.major, 8);
        assert_eq!(v2.minor, 11);
        assert_eq!(v2.patch, 1);
    }
}
