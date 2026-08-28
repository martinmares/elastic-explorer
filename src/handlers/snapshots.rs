use anyhow::{Context, Result, anyhow, bail};
use askama::Template;
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::Html,
};
use axum_extra::extract::CookieJar;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::HashSet, sync::Arc, time::Duration};
use tokio::sync::RwLock;

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
    schedule_status: Arc<RwLock<Option<ScheduledSnapshotStatus>>>,
}

#[derive(Debug, Clone)]
pub struct ScheduledSnapshotConfig {
    pub cron: String,
    pub timezone: String,
    pub keep_last: u32,
    pub max_age_days: u32,
    pub note: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScheduledSnapshotStatus {
    pub cron: String,
    pub timezone: String,
    pub keep_last: u32,
    pub max_age_days: u32,
    pub note: String,
    pub state: String,
    pub next_run: Option<DateTime<Utc>>,
    pub last_started_at: Option<DateTime<Utc>>,
    pub last_completed_at: Option<DateTime<Utc>>,
    pub last_result: Option<String>,
    pub last_error: Option<String>,
}

impl ScheduledSnapshotStatus {
    fn from_config(config: &ScheduledSnapshotConfig) -> Self {
        Self {
            cron: config.cron.clone(),
            timezone: config.timezone.clone(),
            keep_last: config.keep_last,
            max_age_days: config.max_age_days,
            note: config.note.clone(),
            state: "Starting".to_string(),
            next_run: None,
            last_started_at: None,
            last_completed_at: None,
            last_result: None,
            last_error: None,
        }
    }
}

impl SnapshotConfig {
    #[allow(clippy::too_many_arguments)]
    pub fn from_args(
        enabled: bool,
        stateless: bool,
        repository: Option<String>,
        index_prefix: Option<String>,
        cron: Option<String>,
        timezone: String,
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
            .map(|cron| {
                validate_application_cron(&cron)?;
                timezone
                    .parse::<chrono_tz::Tz>()
                    .map_err(|_| anyhow!("invalid SCHEDULED_SNAPSHOT_TIMEZONE: {timezone}"))?;
                Ok::<ScheduledSnapshotConfig, anyhow::Error>(ScheduledSnapshotConfig {
                    cron,
                    timezone,
                    keep_last,
                    max_age_days,
                    note: note.trim().to_string(),
                })
            })
            .transpose()?;
        let schedule_status = Arc::new(RwLock::new(
            schedule.as_ref().map(ScheduledSnapshotStatus::from_config),
        ));
        Ok(Some(Self {
            repository,
            index_prefix,
            schedule,
            schedule_status,
        }))
    }

    fn pattern(&self) -> String {
        format!("{}*", self.index_prefix)
    }
}

fn validate_application_cron(value: &str) -> Result<()> {
    if value.split_whitespace().count() != 7 {
        bail!(
            "SCHEDULED_SNAPSHOT_CRON must use seven fields: seconds minutes hours day-of-month month day-of-week year"
        );
    }
    value
        .parse::<cron::Schedule>()
        .map_err(|error| anyhow!("invalid SCHEDULED_SNAPSHOT_CRON: {error}"))?;
    Ok(())
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

fn stateless_client(state: &AppState) -> Result<EsClient> {
    let endpoint = state
        .stateless_endpoint
        .as_ref()
        .context("snapshot mode requires a stateless endpoint")?;
    EsClient::new(
        endpoint.url.clone(),
        endpoint.insecure,
        endpoint.username.clone(),
        state.stateless_password.clone(),
    )
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
    let client = stateless_client(state)?;
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

    // Versions before 0.8.4 used this SLM policy. Remove it during startup so
    // the application scheduler and the legacy policy cannot create duplicates.
    let (legacy_status, legacy_body) =
        client
            .get_raw(&format!("/_slm/policy/{POLICY_ID}"))
            .await
            .context("failed to check for the legacy automatic snapshot SLM policy")?;
    match legacy_status {
        200 => {
            let _: Value = client
                .delete(&format!("/_slm/policy/{POLICY_ID}"))
                .await
                .context("failed to remove the legacy automatic snapshot SLM policy")?;
            tracing::info!(
                policy = POLICY_ID,
                "legacy automatic snapshot SLM policy removed"
            );
        }
        404 => {}
        status => bail!("failed to inspect legacy SLM policy ({status}): {legacy_body}"),
    }
    Ok(())
}

pub fn start_scheduler(state: Arc<AppState>) {
    let Some(snapshot_config) = state.snapshots.clone() else {
        return;
    };
    let Some(schedule_config) = snapshot_config.schedule.clone() else {
        return;
    };

    tokio::spawn(async move {
        let schedule = schedule_config
            .cron
            .parse::<cron::Schedule>()
            .expect("scheduled snapshot cron validated at startup");
        let timezone = schedule_config
            .timezone
            .parse::<chrono_tz::Tz>()
            .expect("scheduled snapshot timezone validated at startup");

        if let Err(error) = initialize_schedule_status(&state, &snapshot_config).await {
            tracing::warn!(error = %error, "failed to load the latest automatic snapshot status");
        }

        loop {
            let now = Utc::now().with_timezone(&timezone);
            let Some(next) = schedule.after(&now).next() else {
                let message = "automatic snapshot cron has no next occurrence".to_string();
                update_schedule_status(&snapshot_config, |status| {
                    status.state = "Failed".to_string();
                    status.next_run = None;
                    status.last_error = Some(message.clone());
                })
                .await;
                tracing::error!(cron = %schedule_config.cron, "{message}");
                return;
            };
            let next_utc = next.with_timezone(&Utc);
            let delay = next_utc
                .signed_duration_since(Utc::now())
                .to_std()
                .unwrap_or_default();
            update_schedule_status(&snapshot_config, |status| {
                status.state = "Waiting".to_string();
                status.next_run = Some(next_utc);
            })
            .await;
            tracing::info!(
                next_run = %next.to_rfc3339(),
                timezone = %schedule_config.timezone,
                pattern = %snapshot_config.pattern(),
                "automatic snapshot waiting"
            );
            tokio::time::sleep(delay).await;

            let started_at = Utc::now();
            update_schedule_status(&snapshot_config, |status| {
                status.state = "Running".to_string();
                status.next_run = None;
                status.last_started_at = Some(started_at);
                status.last_error = None;
            })
            .await;

            match run_scheduled_snapshot(&state, &snapshot_config, &schedule_config).await {
                Ok(name) => {
                    let completed_at = Utc::now();
                    update_schedule_status(&snapshot_config, |status| {
                        status.state = "Waiting".to_string();
                        status.last_completed_at = Some(completed_at);
                        status.last_result = Some("Completed".to_string());
                        status.last_error = None;
                    })
                    .await;
                    tracing::info!(snapshot = %name, "automatic snapshot completed");
                    if let Err(error) =
                        apply_scheduled_retention(&state, &snapshot_config, &schedule_config).await
                    {
                        tracing::warn!(error = %error, "automatic snapshot retention failed");
                    }
                }
                Err(error) => {
                    let completed_at = Utc::now();
                    let message = format!("{error:#}");
                    update_schedule_status(&snapshot_config, |status| {
                        status.state = "Waiting".to_string();
                        status.last_completed_at = Some(completed_at);
                        status.last_result = Some("Failed".to_string());
                        status.last_error = Some(message.clone());
                    })
                    .await;
                    tracing::error!(error = %error, "automatic snapshot failed");
                }
            }
        }
    });
}

async fn update_schedule_status(
    config: &SnapshotConfig,
    update: impl FnOnce(&mut ScheduledSnapshotStatus),
) {
    if let Some(status) = config.schedule_status.write().await.as_mut() {
        update(status);
    }
}

async fn initialize_schedule_status(state: &AppState, config: &SnapshotConfig) -> Result<()> {
    let client = stateless_client(state)?;
    let snapshots = list_snapshots(&client, config).await?;
    let latest = snapshots
        .iter()
        .filter(|snapshot| is_our_scheduled_snapshot(snapshot))
        .max_by_key(|snapshot| snapshot["start_time_in_millis"].as_i64());
    if let Some(snapshot) = latest {
        let started_at = date_time_from_snapshot(snapshot, "start_time_in_millis");
        let completed_at = date_time_from_snapshot(snapshot, "end_time_in_millis");
        let result = snapshot["state"].as_str().unwrap_or("Unknown").to_string();
        update_schedule_status(config, |status| {
            status.last_started_at = started_at;
            status.last_completed_at = completed_at;
            status.last_result = Some(if result == "SUCCESS" {
                "Completed".to_string()
            } else {
                result
            });
        })
        .await;
    }
    Ok(())
}

async fn run_scheduled_snapshot(
    state: &AppState,
    config: &SnapshotConfig,
    schedule: &ScheduledSnapshotConfig,
) -> Result<String> {
    let client = stateless_client(state)?;
    if managed_indices(&client, config).await?.is_empty() {
        bail!("the managed pattern matches no indices");
    }
    let cluster: Value = client.get("/").await?;
    let name = format!(
        "elastic-explorer-scheduled-{}",
        Utc::now().format("%Y%m%d-%H%M%S-%3f")
    );
    let body = json!({
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
            "source_cluster_name": string_at(&cluster, "cluster_name"),
            "source_cluster_uuid": string_at(&cluster, "cluster_uuid")
        }
    });
    let _: Value = client
        .put(
            &format!(
                "/_snapshot/{}/{}?wait_for_completion=false",
                config.repository, name
            ),
            body,
        )
        .await?;

    loop {
        tokio::time::sleep(Duration::from_secs(2)).await;
        match get_snapshot(&client, config, &name).await {
            Ok(snapshot) => match snapshot["state"].as_str().unwrap_or("IN_PROGRESS") {
                "SUCCESS" => return Ok(name),
                "FAILED" | "PARTIAL" | "INCOMPATIBLE" => {
                    bail!(
                        "snapshot {name} completed with state {}",
                        snapshot["state"].as_str().unwrap_or("unknown")
                    )
                }
                _ => {}
            },
            Err(error) => {
                tracing::warn!(snapshot = %name, error = %error, "waiting for automatic snapshot status");
            }
        }
    }
}

async fn apply_scheduled_retention(
    state: &AppState,
    config: &SnapshotConfig,
    schedule: &ScheduledSnapshotConfig,
) -> Result<()> {
    let client = stateless_client(state)?;
    let snapshots = list_snapshots(&client, config).await?;
    let cutoff = Utc::now() - ChronoDuration::days(i64::from(schedule.max_age_days));
    for name in scheduled_retention_candidates(&snapshots, schedule.keep_last, cutoff) {
        let _: Value = client
            .delete(&format!(
                "/_snapshot/{}/{}",
                config.repository,
                urlencoding::encode(&name)
            ))
            .await
            .with_context(|| format!("failed to delete expired automatic snapshot {name}"))?;
        tracing::info!(snapshot = %name, "expired automatic snapshot deleted");
    }
    Ok(())
}

fn scheduled_retention_candidates(
    snapshots: &[Value],
    keep_last: u32,
    cutoff: DateTime<Utc>,
) -> Vec<String> {
    let mut scheduled: Vec<&Value> = snapshots
        .iter()
        .filter(|snapshot| is_our_scheduled_snapshot(snapshot))
        .filter(|snapshot| snapshot["state"].as_str() != Some("IN_PROGRESS"))
        .collect();
    scheduled.sort_by(|a, b| {
        b["start_time_in_millis"]
            .as_i64()
            .cmp(&a["start_time_in_millis"].as_i64())
    });
    scheduled
        .into_iter()
        .skip(keep_last as usize)
        .filter(|snapshot| {
            date_time_from_snapshot(snapshot, "end_time_in_millis")
                .or_else(|| date_time_from_snapshot(snapshot, "start_time_in_millis"))
                .is_some_and(|finished| finished < cutoff)
        })
        .filter_map(|snapshot| snapshot["snapshot"].as_str().map(ToOwned::to_owned))
        .collect()
}

fn is_our_scheduled_snapshot(snapshot: &Value) -> bool {
    snapshot_kind(snapshot.get("metadata")) == "scheduled"
        && created_by_us(snapshot.get("metadata"))
}

fn date_time_from_snapshot(snapshot: &Value, field: &str) -> Option<DateTime<Utc>> {
    DateTime::from_timestamp_millis(snapshot[field].as_i64()?)
}

async fn list_snapshots(client: &EsClient, config: &SnapshotConfig) -> Result<Vec<Value>> {
    let response: Value = client
        .get(&format!(
            "/_snapshot/{}/_all?verbose=true&index_details=true",
            config.repository
        ))
        .await?;
    Ok(response["snapshots"]
        .as_array()
        .cloned()
        .unwrap_or_default())
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
    schedule: Option<ScheduledSnapshotStatus>,
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
    let mut snapshots = list_snapshots(&client, config).await.map_err(api_error)?;
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
    let schedule = config.schedule_status.read().await.clone();
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

pub async fn schedule_status(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Option<ScheduledSnapshotStatus>>, (StatusCode, String)> {
    let config = config(&state).map_err(api_error)?;
    Ok(Json(config.schedule_status.read().await.clone()))
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
    use chrono::{TimeZone, Timelike};

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
    fn application_cron_requires_the_postgres_explorer_format() {
        assert!(validate_application_cron("0 0 20 * * * *").is_ok());
        assert!(validate_application_cron("0 0 20 * * ?").is_err());
    }

    #[test]
    fn local_schedule_keeps_wall_clock_time_across_dst() {
        let timezone = chrono_tz::Europe::Prague;
        let schedule = "0 0 20 * * * *".parse::<cron::Schedule>().unwrap();
        let start = timezone.with_ymd_and_hms(2026, 10, 24, 19, 0, 0).unwrap();
        let occurrences: Vec<_> = schedule.after(&start).take(2).collect();
        assert_eq!(occurrences[0].hour(), 20);
        assert_eq!(occurrences[1].hour(), 20);
        assert_eq!(occurrences[0].with_timezone(&Utc).hour(), 18);
        assert_eq!(occurrences[1].with_timezone(&Utc).hour(), 19);
    }

    #[test]
    fn retention_never_deletes_manual_or_kept_automatic_snapshots() {
        let now = Utc::now();
        let snapshot = |name: &str, age_days: i64, kind: &str| {
            let time = now - ChronoDuration::days(age_days);
            json!({
                "snapshot": name,
                "state": "SUCCESS",
                "start_time_in_millis": time.timestamp_millis(),
                "end_time_in_millis": time.timestamp_millis(),
                "metadata": {"created_by": "elastic-explorer", "kind": kind}
            })
        };
        let snapshots = vec![
            snapshot("automatic-newest", 1, "scheduled"),
            snapshot("automatic-kept", 2, "scheduled"),
            snapshot("automatic-expired", 40, "scheduled"),
            snapshot("manual-old", 60, "manual"),
        ];
        assert_eq!(
            scheduled_retention_candidates(&snapshots, 2, now - ChronoDuration::days(30)),
            vec!["automatic-expired"]
        );
    }
}
