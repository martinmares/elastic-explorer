use anyhow::{Context, Result, anyhow, bail};
use askama::Template;
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::Html,
};
use axum_extra::extract::CookieJar;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::HashSet, sync::Arc};

use crate::{
    es::EsClient,
    handlers::endpoints::{AppState, get_active_endpoint, get_endpoint_password},
    templates::{PageContext, SnapshotsTemplate},
};

const POLICY_ID: &str = "elastic-explorer-scheduled";

#[derive(Debug, Clone)]
pub struct SnapshotConfig {
    pub repository: String,
    pub index_prefix: String,
    pub schedule: Option<ScheduledSnapshotConfig>,
}

#[derive(Debug, Clone)]
pub struct ScheduledSnapshotConfig {
    pub cron: String,
    pub keep_last: u32,
    pub max_age_days: u32,
    pub note: String,
}

impl SnapshotConfig {
    #[allow(clippy::too_many_arguments)]
    pub fn from_args(
        enabled: bool,
        stateless: bool,
        repository: Option<String>,
        index_prefix: Option<String>,
        cron: Option<String>,
        keep_last: u32,
        max_age_days: u32,
        note: String,
    ) -> Result<Option<Self>> {
        if !enabled {
            if cron.is_some() {
                bail!("SCHEDULED_SNAPSHOT_CRON requires SNAPSHOTS_ENABLED=true");
            }
            return Ok(None);
        }
        if !stateless {
            bail!("native snapshots are currently supported only with --stateless");
        }
        let repository = required_setting(repository, "SNAPSHOT_REPOSITORY")?;
        validate_resource_name(&repository, "snapshot repository")?;
        let index_prefix = required_setting(index_prefix, "SNAPSHOT_INDEX_PREFIX")?;
        validate_prefix(&index_prefix)?;
        if keep_last == 0 || max_age_days == 0 {
            bail!("scheduled snapshot retention values must be greater than zero");
        }
        let schedule = cron
            .map(|cron| cron.trim().to_string())
            .filter(|cron| !cron.is_empty())
            .map(|cron| ScheduledSnapshotConfig {
                cron,
                keep_last,
                max_age_days,
                note: note.trim().to_string(),
            });
        Ok(Some(Self {
            repository,
            index_prefix,
            schedule,
        }))
    }

    fn pattern(&self) -> String {
        format!("{}*", self.index_prefix)
    }
}

fn required_setting(value: Option<String>, name: &str) -> Result<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("{name} is required when snapshots are enabled"))
}

fn validate_resource_name(value: &str, label: &str) -> Result<()> {
    if value.len() > 255
        || !value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        bail!("invalid {label}: {value}");
    }
    Ok(())
}

fn validate_prefix(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 200
        || !value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '_'))
    {
        bail!("index prefix must contain only lowercase ASCII letters, digits, '-' and '_'");
    }
    Ok(())
}

fn api_error(error: anyhow::Error) -> (StatusCode, String) {
    tracing::error!(error = %error, "snapshot operation failed");
    (StatusCode::BAD_REQUEST, error.to_string())
}

async fn client(state: &AppState, jar: &CookieJar) -> Result<EsClient> {
    let endpoint = get_active_endpoint(state, jar)
        .await
        .ok_or_else(|| anyhow!("no active Elasticsearch endpoint"))?;
    let password = get_endpoint_password(state, &endpoint).await;
    EsClient::new(endpoint.url, endpoint.insecure, endpoint.username, password)
}

fn config(state: &AppState) -> Result<&SnapshotConfig> {
    state
        .snapshots
        .as_ref()
        .ok_or_else(|| anyhow!("snapshots are not enabled"))
}

pub async fn initialize(state: &Arc<AppState>) -> Result<()> {
    let Some(config) = &state.snapshots else {
        return Ok(());
    };
    let endpoint = state
        .stateless_endpoint
        .as_ref()
        .context("snapshot mode requires a stateless endpoint")?;
    let client = EsClient::new(
        endpoint.url.clone(),
        endpoint.insecure,
        endpoint.username.clone(),
        state.stateless_password.clone(),
    )?;
    let _: Value = client
        .get(&format!("/_snapshot/{}", config.repository))
        .await
        .with_context(|| {
            format!(
                "snapshot repository '{}' is not available",
                config.repository
            )
        })?;
    let (status, body) = client
        .post_empty_raw(&format!("/_snapshot/{}/_verify", config.repository))
        .await?;
    if !(200..300).contains(&status) {
        bail!("snapshot repository verification failed ({status}): {body}");
    }

    if let Some(schedule) = &config.schedule {
        let cluster: Value = client.get("/").await?;
        let cluster_name = string_at(&cluster, "cluster_name");
        let cluster_uuid = string_at(&cluster, "cluster_uuid");
        let body = json!({
            "schedule": schedule.cron,
            "name": "<elastic-explorer-scheduled-{now{yyyy.MM.dd-HH.mm.ss}}>",
            "repository": config.repository,
            "config": {
                "indices": config.pattern(),
                "ignore_unavailable": false,
                "include_global_state": false,
                "partial": false,
                "metadata": {
                    "created_by": "elastic-explorer",
                    "kind": "scheduled",
                    "scope": "all",
                    "note": schedule.note,
                    "index_prefix": config.index_prefix,
                    "source_cluster_name": cluster_name,
                    "source_cluster_uuid": cluster_uuid
                }
            },
            "retention": {
                "expire_after": format!("{}d", schedule.max_age_days),
                "min_count": schedule.keep_last
            }
        });
        let existing = client
            .get::<Value>(&format!("/_slm/policy/{POLICY_ID}"))
            .await
            .ok();
        if !slm_policy_matches(existing.as_ref(), &body) {
            let _: Value = client
                .put(&format!("/_slm/policy/{POLICY_ID}"), body)
                .await
                .context("failed to reconcile the automatic snapshot SLM policy")?;
            tracing::info!(
                policy = POLICY_ID,
                "automatic snapshot SLM policy reconciled"
            );
        } else {
            tracing::info!(
                policy = POLICY_ID,
                "automatic snapshot SLM policy unchanged"
            );
        }
    } else if client
        .get::<Value>(&format!("/_slm/policy/{POLICY_ID}"))
        .await
        .is_ok()
    {
        let _: Value = client
            .delete(&format!("/_slm/policy/{POLICY_ID}"))
            .await
            .context("failed to disable the automatic snapshot SLM policy")?;
        tracing::info!(policy = POLICY_ID, "automatic snapshot SLM policy disabled");
    }
    Ok(())
}

fn slm_policy_matches(existing: Option<&Value>, desired: &Value) -> bool {
    let actual = &existing.unwrap_or(&Value::Null)[POLICY_ID]["policy"];
    ["schedule", "name", "repository", "config", "retention"]
        .iter()
        .all(|key| actual[*key] == desired[*key])
}

pub async fn snapshots_page(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
) -> Result<Html<String>, (StatusCode, String)> {
    config(&state).map_err(api_error)?;
    let active_endpoint = get_active_endpoint(&state, &jar).await;
    let ctx = PageContext::new(
        active_endpoint,
        state.base_path.clone(),
        state.logout_url.clone(),
    )
    .with_snapshots(true);
    SnapshotsTemplate { ctx }
        .render()
        .map(Html)
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))
}

#[derive(Debug, Serialize)]
pub struct Overview {
    repository: String,
    index_prefix: String,
    index_pattern: String,
    endpoint_url: String,
    cluster_name: String,
    cluster_uuid: String,
    managed_indices: Vec<Value>,
    snapshots: Vec<Value>,
    schedule: Option<Value>,
}

pub async fn overview(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
) -> Result<Json<Overview>, (StatusCode, String)> {
    let config = config(&state).map_err(api_error)?;
    let client = client(&state, &jar).await.map_err(api_error)?;
    let endpoint_url = state
        .stateless_endpoint
        .as_ref()
        .map(|endpoint| endpoint.url.clone())
        .unwrap_or_default();
    let cluster: Value = client.get("/").await.map_err(api_error)?;
    let managed_indices = managed_indices(&client, config).await.map_err(api_error)?;
    let response: Value = client
        .get(&format!(
            "/_snapshot/{}/_all?verbose=true&index_details=true",
            config.repository
        ))
        .await
        .map_err(api_error)?;
    let mut snapshots = response["snapshots"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    snapshots.sort_by(|a, b| {
        b["start_time_in_millis"]
            .as_i64()
            .cmp(&a["start_time_in_millis"].as_i64())
    });
    for snapshot in &mut snapshots {
        if let Some(object) = snapshot.as_object_mut() {
            let kind = snapshot_kind(object.get("metadata"));
            object.insert("kind".to_string(), Value::String(kind.to_string()));
            object.insert(
                "can_delete".to_string(),
                Value::Bool(kind == "manual" && created_by_us(object.get("metadata"))),
            );
        }
    }
    let schedule = if config.schedule.is_some() {
        client
            .get::<Value>(&format!("/_slm/policy/{POLICY_ID}"))
            .await
            .ok()
    } else {
        None
    };
    Ok(Json(Overview {
        repository: config.repository.clone(),
        index_prefix: config.index_prefix.clone(),
        index_pattern: config.pattern(),
        endpoint_url,
        cluster_name: string_at(&cluster, "cluster_name"),
        cluster_uuid: string_at(&cluster, "cluster_uuid"),
        managed_indices,
        snapshots,
        schedule,
    }))
}

async fn managed_indices(client: &EsClient, config: &SnapshotConfig) -> Result<Vec<Value>> {
    let response: Value = client
        .get(&format!(
            "/_cat/indices/{}?format=json&h=index,docs.count,store.size&s=index",
            config.pattern()
        ))
        .await?;
    Ok(response.as_array().cloned().unwrap_or_default())
}

fn managed_names(values: &[Value]) -> Vec<String> {
    values
        .iter()
        .filter_map(|value| value["index"].as_str().map(ToOwned::to_owned))
        .collect()
}

#[derive(Debug, Deserialize)]
pub struct CreateSnapshotRequest {
    scope: String,
    #[serde(default)]
    indices: Vec<String>,
    #[serde(default)]
    note: String,
}

pub async fn create_snapshot(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Json(request): Json<CreateSnapshotRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let config = config(&state).map_err(api_error)?;
    validate_note(&request.note).map_err(api_error)?;
    let client = client(&state, &jar).await.map_err(api_error)?;
    let current = managed_names(&managed_indices(&client, config).await.map_err(api_error)?);
    if current.is_empty() {
        return Err(api_error(anyhow!("the managed pattern matches no indices")));
    }
    let all = request.scope == "all";
    let selected = if all {
        current.clone()
    } else if request.scope == "selected" {
        validate_selected_indices(&request.indices, &current).map_err(api_error)?
    } else {
        return Err(api_error(anyhow!("scope must be 'all' or 'selected'")));
    };
    let cluster: Value = client.get("/").await.map_err(api_error)?;
    let name = format!(
        "elastic-explorer-manual-{}",
        Utc::now().format("%Y%m%d-%H%M%S-%3f")
    );
    let body = json!({
        "indices": selected.join(","),
        "ignore_unavailable": false,
        "include_global_state": false,
        "partial": false,
        "metadata": {
            "created_by": "elastic-explorer",
            "kind": "manual",
            "scope": if all { "all" } else { "selected" },
            "note": request.note.trim(),
            "index_prefix": config.index_prefix,
            "source_cluster_name": string_at(&cluster, "cluster_name"),
            "source_cluster_uuid": string_at(&cluster, "cluster_uuid")
        }
    });
    let response: Value = client
        .put(
            &format!(
                "/_snapshot/{}/{}?wait_for_completion=false",
                config.repository, name
            ),
            body,
        )
        .await
        .map_err(api_error)?;
    Ok(Json(json!({ "snapshot": name, "response": response })))
}

fn validate_note(note: &str) -> Result<()> {
    if note.len() > 1000 {
        bail!("snapshot note is limited to 1000 bytes");
    }
    Ok(())
}

fn validate_selected_indices(requested: &[String], current: &[String]) -> Result<Vec<String>> {
    let allowed: HashSet<&str> = current.iter().map(String::as_str).collect();
    let mut selected = Vec::new();
    for index in requested {
        if !allowed.contains(index.as_str()) {
            bail!("index is outside the managed scope: {index}");
        }
        if !selected.contains(index) {
            selected.push(index.clone());
        }
    }
    if selected.is_empty() {
        bail!("select at least one index");
    }
    Ok(selected)
}

pub async fn snapshot_status(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Path(name): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let config = config(&state).map_err(api_error)?;
    validate_resource_name(&name, "snapshot name").map_err(api_error)?;
    let client = client(&state, &jar).await.map_err(api_error)?;
    let response: Value = client
        .get(&format!(
            "/_snapshot/{}/{}/_status",
            config.repository, name
        ))
        .await
        .map_err(api_error)?;
    Ok(Json(snapshot_progress(response)))
}

fn snapshot_progress(response: Value) -> Value {
    let snapshot = response["snapshots"].as_array().and_then(|v| v.first());
    let state = snapshot
        .and_then(|v| v["state"].as_str())
        .unwrap_or("STARTING");
    let stats = snapshot.map(|v| &v["stats"]).unwrap_or(&Value::Null);
    let processed_value = stats["processed"]["size_in_bytes"].as_u64();
    let processed = processed_value.unwrap_or(0);
    let total = stats["incremental"]["size_in_bytes"]
        .as_u64()
        .or_else(|| stats["total"]["size_in_bytes"].as_u64())
        .unwrap_or(0);
    let shards = snapshot.map(|v| &v["shards_stats"]).unwrap_or(&Value::Null);
    let done_shards = shards["done"].as_u64().unwrap_or(0);
    let total_shards = shards["total"].as_u64().unwrap_or(0);
    let percent = if processed_value.is_some() && total > 0 {
        (processed.saturating_mul(100) / total).min(100)
    } else if total_shards > 0 {
        done_shards
            .saturating_mul(100)
            .checked_div(total_shards)
            .unwrap_or(0)
            .min(100)
    } else if state == "SUCCESS" {
        100
    } else {
        0
    };
    let displayed_processed = if state == "SUCCESS" && processed_value.is_none() {
        total
    } else {
        processed
    };
    json!({
        "state": state,
        "percent": percent,
        "processed_bytes": displayed_processed,
        "total_bytes": total,
        "done_shards": done_shards,
        "total_shards": total_shards
    })
}

pub async fn delete_snapshot(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Path(name): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let config = config(&state).map_err(api_error)?;
    validate_resource_name(&name, "snapshot name").map_err(api_error)?;
    let client = client(&state, &jar).await.map_err(api_error)?;
    let snapshot = get_snapshot(&client, config, &name)
        .await
        .map_err(api_error)?;
    let metadata = snapshot.get("metadata");
    if snapshot_kind(metadata) != "manual" || !created_by_us(metadata) {
        return Err(api_error(anyhow!(
            "only manual snapshots created by Elastic Explorer can be deleted here"
        )));
    }
    let response: Value = client
        .delete(&format!("/_snapshot/{}/{}", config.repository, name))
        .await
        .map_err(api_error)?;
    Ok(Json(response))
}

#[derive(Debug, Deserialize)]
pub struct SafeRestoreRequest {
    destination_prefix: String,
}

pub async fn restore_safe(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Path(name): Path<String>,
    Json(request): Json<SafeRestoreRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let config = config(&state).map_err(api_error)?;
    validate_resource_name(&name, "snapshot name").map_err(api_error)?;
    validate_prefix(&request.destination_prefix).map_err(api_error)?;
    if request.destination_prefix == config.index_prefix {
        return Err(api_error(anyhow!(
            "safe restore requires a different destination prefix"
        )));
    }
    let client = client(&state, &jar).await.map_err(api_error)?;
    let snapshot = get_snapshot(&client, config, &name)
        .await
        .map_err(api_error)?;
    let source_prefix = snapshot_prefix(&snapshot, config).map_err(api_error)?;
    let sources = snapshot_indices(&snapshot).map_err(api_error)?;
    ensure_prefix_scope(&sources, &source_prefix).map_err(api_error)?;
    let destinations: Vec<String> = sources
        .iter()
        .map(|name| name.replacen(&source_prefix, &request.destination_prefix, 1))
        .collect();
    let current = all_indices(&client).await.map_err(api_error)?;
    let collisions: Vec<&String> = destinations
        .iter()
        .filter(|name| current.contains(*name))
        .collect();
    if !collisions.is_empty() {
        return Err(api_error(anyhow!(
            "restore destination already exists: {}",
            collisions
                .into_iter()
                .take(10)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }
    let body = json!({
        "indices": sources.join(","),
        "ignore_unavailable": false,
        "include_global_state": false,
        "include_aliases": false,
        "rename_pattern": format!("^{}(.*)$", source_prefix),
        "rename_replacement": format!("{}$1", request.destination_prefix)
    });
    let response: Value = client
        .post(
            &format!(
                "/_snapshot/{}/{}/_restore?wait_for_completion=false",
                config.repository, name
            ),
            body,
        )
        .await
        .map_err(api_error)?;
    Ok(Json(json!({
        "mode": "safe",
        "source_indices": sources,
        "destination_indices": destinations,
        "response": response
    })))
}

#[derive(Debug, Deserialize)]
pub struct InPlaceRestoreRequest {
    confirmation: String,
}

pub async fn restore_in_place(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Path(name): Path<String>,
    Json(request): Json<InPlaceRestoreRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let config = config(&state).map_err(api_error)?;
    validate_resource_name(&name, "snapshot name").map_err(api_error)?;
    let client = client(&state, &jar).await.map_err(api_error)?;
    let cluster: Value = client.get("/").await.map_err(api_error)?;
    let cluster_name = string_at(&cluster, "cluster_name");
    let cluster_uuid = string_at(&cluster, "cluster_uuid");
    let expected = format!("DELETE {}* ON {}", config.index_prefix, cluster_name);
    if request.confirmation != expected {
        return Err(api_error(anyhow!("confirmation text does not match")));
    }
    let snapshot = get_snapshot(&client, config, &name)
        .await
        .map_err(api_error)?;
    let metadata = snapshot.get("metadata").unwrap_or(&Value::Null);
    if metadata["source_cluster_uuid"].as_str() != Some(cluster_uuid.as_str()) {
        return Err(api_error(anyhow!(
            "in-place restore is allowed only on the snapshot's source cluster UUID"
        )));
    }
    if metadata["scope"].as_str() != Some("all") {
        return Err(api_error(anyhow!(
            "in-place restore requires a full managed-prefix snapshot"
        )));
    }
    let sources = snapshot_indices(&snapshot).map_err(api_error)?;
    ensure_prefix_scope(&sources, &config.index_prefix).map_err(api_error)?;
    let current = managed_names(&managed_indices(&client, config).await.map_err(api_error)?);
    if !current.is_empty() {
        let _: Value = client
            .delete(&format!("/{}", exact_index_path(&current)))
            .await
            .map_err(|error| {
                api_error(error.context("current managed indices were not deleted"))
            })?;
    }
    let body = json!({
        "indices": sources.join(","),
        "ignore_unavailable": false,
        "include_global_state": false,
        "include_aliases": true
    });
    let response: Value = client
        .post(
            &format!(
                "/_snapshot/{}/{}/_restore?wait_for_completion=false",
                config.repository, name
            ),
            body,
        )
        .await
        .map_err(|error| {
            api_error(error.context(
                "managed indices were deleted, but Elasticsearch rejected the restore; retry the same snapshot restore",
            ))
        })?;
    Ok(Json(json!({
        "mode": "in-place",
        "deleted_indices": current,
        "destination_indices": sources,
        "response": response
    })))
}

#[derive(Debug, Deserialize)]
pub struct RestoreStatusRequest {
    indices: Vec<String>,
}

pub async fn restore_status(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Json(request): Json<RestoreStatusRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    config(&state).map_err(api_error)?;
    if request.indices.is_empty() || request.indices.len() > 10_000 {
        return Err(api_error(anyhow!("invalid restore index list")));
    }
    for name in &request.indices {
        validate_resource_name(name, "index name").map_err(api_error)?;
    }
    let client = client(&state, &jar).await.map_err(api_error)?;
    let response: Value = match client
        .get(&format!(
            "/{}/_recovery?detailed=false",
            exact_index_path(&request.indices)
        ))
        .await
    {
        Ok(response) => response,
        Err(_) => return Ok(Json(json!({ "state": "STARTING", "percent": 0 }))),
    };
    let mut recovered = 0_u64;
    let mut total = 0_u64;
    let mut done = true;
    let mut shards = 0_u64;
    for index in request.indices.iter().filter_map(|name| response.get(name)) {
        for shard in index["shards"].as_array().into_iter().flatten() {
            shards += 1;
            recovered += shard["index"]["size"]["recovered_in_bytes"]
                .as_u64()
                .unwrap_or(0);
            total += shard["index"]["size"]["total_in_bytes"]
                .as_u64()
                .unwrap_or(0);
            done &= shard["stage"].as_str() == Some("DONE");
        }
    }
    let percent = if total > 0 {
        recovered
            .saturating_mul(100)
            .checked_div(total)
            .unwrap_or(0)
            .min(100)
    } else if done && shards > 0 {
        100
    } else {
        0
    };
    Ok(Json(json!({
        "state": if done && shards > 0 { "SUCCESS" } else { "IN_PROGRESS" },
        "percent": percent,
        "recovered_bytes": recovered,
        "total_bytes": total,
        "shards": shards
    })))
}

async fn get_snapshot(client: &EsClient, config: &SnapshotConfig, name: &str) -> Result<Value> {
    let response: Value = client
        .get(&format!("/_snapshot/{}/{}", config.repository, name))
        .await?;
    response["snapshots"]
        .as_array()
        .and_then(|values| values.first())
        .cloned()
        .ok_or_else(|| anyhow!("snapshot not found"))
}

fn snapshot_indices(snapshot: &Value) -> Result<Vec<String>> {
    let indices: Vec<String> = snapshot["indices"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|value| value.as_str().map(ToOwned::to_owned))
        .collect();
    if indices.is_empty() {
        bail!("snapshot contains no indices");
    }
    Ok(indices)
}

fn snapshot_prefix(snapshot: &Value, config: &SnapshotConfig) -> Result<String> {
    let prefix = snapshot["metadata"]["index_prefix"]
        .as_str()
        .unwrap_or(&config.index_prefix)
        .to_string();
    validate_prefix(&prefix)?;
    Ok(prefix)
}

fn ensure_prefix_scope(indices: &[String], prefix: &str) -> Result<()> {
    if let Some(index) = indices.iter().find(|name| !name.starts_with(prefix)) {
        bail!("snapshot index is outside its declared prefix: {index}");
    }
    Ok(())
}

async fn all_indices(client: &EsClient) -> Result<HashSet<String>> {
    let response: Value = client
        .get("/_cat/indices?format=json&h=index&expand_wildcards=all")
        .await?;
    Ok(response
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|value| value["index"].as_str().map(ToOwned::to_owned))
        .collect())
}

fn exact_index_path(indices: &[String]) -> String {
    indices
        .iter()
        .map(|name| urlencoding::encode(name).into_owned())
        .collect::<Vec<_>>()
        .join(",")
}

fn snapshot_kind(metadata: Option<&Value>) -> &'static str {
    match metadata.and_then(|value| value["kind"].as_str()) {
        Some("manual") => "manual",
        Some("scheduled") => "scheduled",
        _ => "external",
    }
}

fn created_by_us(metadata: Option<&Value>) -> bool {
    metadata.and_then(|value| value["created_by"].as_str()) == Some("elastic-explorer")
}

fn string_at(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or("unknown").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_is_literal_and_rejects_wildcards() {
        assert!(validate_prefix("tsm-sda").is_ok());
        assert!(validate_prefix("tsm-sda*").is_err());
        assert!(validate_prefix("TSM-SDA").is_err());
    }

    #[test]
    fn selected_indices_must_be_currently_managed() {
        let current = vec!["tsm-sdaone_1".to_string(), "tsm-sdatwo_1".to_string()];
        assert!(validate_selected_indices(&[current[0].clone()], &current).is_ok());
        assert!(validate_selected_indices(&["other".to_string()], &current).is_err());
    }

    #[test]
    fn snapshot_progress_falls_back_to_shards() {
        let progress = snapshot_progress(json!({"snapshots": [{
            "state": "SUCCESS",
            "stats": {"incremental": {"size_in_bytes": 10}},
            "shards_stats": {"done": 2, "total": 2}
        }]}));
        assert_eq!(progress["percent"], 100);
    }

    #[test]
    fn slm_reconcile_ignores_response_metadata_but_detects_policy_changes() {
        let desired = json!({
            "schedule": "1h", "name": "snap", "repository": "repo",
            "config": {"indices": "tsm-sda*"}, "retention": {"min_count": 1}
        });
        let existing = json!({ POLICY_ID: {
            "version": 7, "modified_date_millis": 42, "policy": desired.clone()
        }});
        assert!(slm_policy_matches(Some(&existing), &desired));
        let changed = json!({
            "schedule": "2h", "name": "snap", "repository": "repo",
            "config": {"indices": "tsm-sda*"}, "retention": {"min_count": 1}
        });
        assert!(!slm_policy_matches(Some(&existing), &changed));
    }
}
