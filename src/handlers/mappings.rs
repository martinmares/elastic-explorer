use askama::Template;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{Html, Json},
};
use axum_extra::extract::CookieJar;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

use crate::es::EsClient;
use crate::handlers::endpoints::{
    AppState, default_index_pattern, get_active_endpoint, get_endpoint_password,
};
use crate::templates::{MappingsTemplate, PageContext};

#[derive(Debug, Deserialize)]
pub struct MappingsQuery {
    pub pattern: Option<String>,
    #[serde(default = "default_true")]
    pub hide_internal: bool,
    #[serde(default)]
    pub refresh: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize)]
pub struct MappingFieldOccurrence {
    #[serde(skip_serializing)]
    pub path: String,
    pub index: String,
    pub field_type: String,
    pub category: String,
    pub searchable: bool,
    pub aggregatable: bool,
    pub runtime: bool,
    pub multi_field: bool,
    pub format: Option<String>,
    pub analyzer: Option<String>,
    pub search_analyzer: Option<String>,
    pub mapping_signature: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MappingField {
    pub path: String,
    pub category: String,
    pub types: Vec<String>,
    pub coverage: usize,
    pub coverage_percent: u8,
    pub searchable: bool,
    pub aggregatable: bool,
    pub runtime: bool,
    pub multi_field: bool,
    pub conflict: bool,
    pub conflict_reasons: Vec<String>,
    pub occurrences: Vec<MappingFieldOccurrence>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MappingRelation {
    pub index: String,
    pub field: String,
    pub parent: String,
    pub child: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MappingIndex {
    pub name: String,
    pub aliases: Vec<String>,
    pub write_aliases: Vec<String>,
    pub docs_count: u64,
    pub store_size_bytes: u64,
    pub creation_date_millis: Option<i64>,
    pub health: String,
    pub status: String,
    pub field_count: usize,
    pub mapping_fingerprint: String,
    pub hidden: bool,
    pub data_stream: Option<String>,
    pub ilm_managed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct MappingTypeCount {
    pub category: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct OrphanCandidate {
    pub index: String,
    pub confidence: String,
    pub score: u8,
    pub likely_family: Option<String>,
    pub active_index: Option<String>,
    pub mapping_identical: bool,
    pub docs_count: u64,
    pub active_docs_count: Option<u64>,
    pub store_size_bytes: u64,
    pub active_store_size_bytes: Option<u64>,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MappingAnalysis {
    pub pattern: String,
    pub hide_internal: bool,
    pub excluded_index_count: usize,
    pub generated_at: String,
    pub cache_ttl_seconds: u64,
    pub indices: Vec<MappingIndex>,
    pub fields: Vec<MappingField>,
    pub relations: Vec<MappingRelation>,
    pub type_counts: Vec<MappingTypeCount>,
    pub orphan_candidates: Vec<OrphanCandidate>,
    pub index_count: usize,
    pub unique_field_count: usize,
    pub mapped_field_count: usize,
    pub conflict_count: usize,
    pub nested_count: usize,
    pub relation_count: usize,
    pub runtime_count: usize,
    pub warnings: Vec<String>,
}

pub(crate) fn mapping_fields_for_index(
    index: &str,
    mapping: &Value,
) -> Vec<MappingFieldOccurrence> {
    collect_mapping_fields(index, mapping, &HashMap::new()).0
}

fn collect_mapping_fields(
    index: &str,
    mapping: &Value,
    capabilities: &HashMap<(String, String, String), (bool, bool)>,
) -> (Vec<MappingFieldOccurrence>, Vec<MappingRelation>) {
    let mut fields = Vec::new();
    let mut relations = Vec::new();

    if let Some(properties) = mapping.get("properties").and_then(Value::as_object) {
        flatten_properties(
            index,
            "",
            properties,
            false,
            capabilities,
            &mut fields,
            &mut relations,
        );
    }
    if let Some(runtime) = mapping.get("runtime").and_then(Value::as_object) {
        for (path, definition) in runtime {
            fields.push(occurrence_from_definition(
                index,
                path,
                definition,
                true,
                false,
                capabilities,
            ));
        }
    }
    fields.sort_by(|a, b| a.path.cmp(&b.path));

    (fields, relations)
}

const MAPPING_CACHE_TTL: Duration = Duration::from_secs(300);
const MAPPING_CACHE_MAX_ENTRIES: usize = 4;

#[derive(Clone)]
struct CachedAnalysis {
    created: Instant,
    analysis: MappingAnalysis,
}

static MAPPING_CACHE: OnceLock<RwLock<HashMap<String, CachedAnalysis>>> = OnceLock::new();

fn mapping_cache() -> &'static RwLock<HashMap<String, CachedAnalysis>> {
    MAPPING_CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

#[derive(Debug, Deserialize)]
struct CatIndex {
    index: String,
    #[serde(default)]
    health: String,
    #[serde(default)]
    status: String,
    #[serde(rename = "docs.count", default, deserialize_with = "string_or_number")]
    docs_count: u64,
    #[serde(rename = "store.size", default, deserialize_with = "string_or_number")]
    store_size_bytes: u64,
    #[serde(rename = "creation.date", default, deserialize_with = "optional_i64")]
    creation_date_millis: Option<i64>,
}

fn string_or_number<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    Ok(match value {
        Value::String(value) => value.parse().unwrap_or(0),
        Value::Number(value) => value.as_u64().unwrap_or(0),
        _ => 0,
    })
}

fn optional_i64<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    Ok(match value {
        Value::String(value) if !value.is_empty() && value != "-" => value.parse().ok(),
        Value::Number(value) => value.as_i64(),
        _ => None,
    })
}

pub async fn mappings_page(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Query(query): Query<MappingsQuery>,
) -> Result<Html<String>, (StatusCode, String)> {
    let active_endpoint = get_active_endpoint(&state, &jar).await;
    let default_pattern = query
        .pattern
        .filter(|value| !value.trim().is_empty())
        .or_else(|| active_endpoint.as_ref().and_then(default_index_pattern))
        .unwrap_or_else(|| "*".to_string());
    let ctx = PageContext::new(
        active_endpoint,
        state.base_path.clone(),
        state.logout_url.clone(),
    )
    .with_snapshots(state.snapshots.is_some());

    MappingsTemplate {
        ctx,
        default_pattern,
        default_hide_internal: query.hide_internal,
    }
    .render()
    .map(Html)
    .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))
}

pub async fn mappings_analysis(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Query(query): Query<MappingsQuery>,
) -> Result<Json<MappingAnalysis>, (StatusCode, String)> {
    let endpoint = get_active_endpoint(&state, &jar).await.ok_or((
        StatusCode::BAD_REQUEST,
        "No active endpoint selected".into(),
    ))?;
    let pattern = query
        .pattern
        .unwrap_or_else(|| default_index_pattern(&endpoint).unwrap_or_else(|| "*".into()));
    validate_pattern(&pattern)?;

    let cache_key = format!(
        "{}\n{}\n{}\n{}",
        endpoint.url,
        endpoint.username.as_deref().unwrap_or(""),
        pattern.trim(),
        query.hide_internal
    );
    if !query.refresh {
        let cache = mapping_cache().read().await;
        if let Some(entry) = cache.get(&cache_key)
            && entry.created.elapsed() < MAPPING_CACHE_TTL
        {
            return Ok(Json(entry.analysis.clone()));
        }
    }

    let password = get_endpoint_password(&state, &endpoint).await;
    let client = EsClient::new(
        endpoint.url.clone(),
        endpoint.insecure,
        endpoint.username.clone(),
        password,
    )
    .map_err(internal_error)?;
    let analysis = load_analysis(&client, pattern.trim(), query.hide_internal)
        .await
        .map_err(|error| {
            tracing::error!(%error, pattern = pattern.trim(), "mapping analysis failed");
            (StatusCode::BAD_GATEWAY, error.to_string())
        })?;
    let mut cache = mapping_cache().write().await;
    cache.retain(|_, entry| entry.created.elapsed() < MAPPING_CACHE_TTL);
    if cache.len() >= MAPPING_CACHE_MAX_ENTRIES {
        cache.clear();
    }
    cache.insert(
        cache_key,
        CachedAnalysis {
            created: Instant::now(),
            analysis: analysis.clone(),
        },
    );
    Ok(Json(analysis))
}

fn validate_pattern(pattern: &str) -> Result<(), (StatusCode, String)> {
    let pattern = pattern.trim();
    if pattern.is_empty() || pattern.len() > 512 {
        return Err((StatusCode::BAD_REQUEST, "Invalid index pattern".into()));
    }
    if pattern
        .chars()
        .any(|c| c.is_whitespace() || matches!(c, '/' | '#' | '\\'))
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "Index pattern must not contain whitespace, '/', '#' or '\\'".into(),
        ));
    }
    Ok(())
}

fn internal_error(error: anyhow::Error) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
}

async fn load_analysis(
    client: &EsClient,
    pattern: &str,
    hide_internal: bool,
) -> anyhow::Result<MappingAnalysis> {
    let target = urlencoding::encode(pattern);
    let expand_wildcards = if hide_internal { "open,closed" } else { "all" };
    let mapping_path = format!(
        "/{target}/_mapping?expand_wildcards={expand_wildcards}&allow_no_indices=true&ignore_unavailable=true"
    );
    let aliases_path = format!(
        "/{target}/_alias?expand_wildcards={expand_wildcards}&allow_no_indices=true&ignore_unavailable=true"
    );
    let settings_path = format!(
        "/{target}/_settings?flat_settings=true&expand_wildcards={expand_wildcards}&allow_no_indices=true&ignore_unavailable=true&filter_path=*.settings.index.hidden,*.settings.index.lifecycle.*"
    );
    let cat_path = format!(
        "/_cat/indices/{target}?format=json&bytes=b&expand_wildcards={expand_wildcards}&allow_no_indices=true&ignore_unavailable=true&h=index,health,status,docs.count,store.size,creation.date"
    );
    let field_caps_path = format!(
        "/{target}/_field_caps?fields=*&include_unmapped=true&expand_wildcards={expand_wildcards}&allow_no_indices=true&ignore_unavailable=true"
    );

    let (mappings_result, aliases_result, settings_result, cat_result, caps_result, streams_result) = tokio::join!(
        client.get::<Value>(&mapping_path),
        client.get::<Value>(&aliases_path),
        client.get::<Value>(&settings_path),
        client.get::<Vec<CatIndex>>(&cat_path),
        client.get::<Value>(&field_caps_path),
        client.get::<Value>("/_data_stream/*?expand_wildcards=all"),
    );

    let mappings = mappings_result?;
    let mut cat_indices = cat_result?;
    let mut warnings = Vec::new();
    let aliases_failed = aliases_result.is_err();
    let settings_failed = settings_result.is_err();
    let caps_failed = caps_result.is_err();
    let streams_failed = streams_result.is_err();
    let aliases = aliases_result.unwrap_or_else(|_| Value::Object(Default::default()));
    let settings = settings_result.unwrap_or_else(|_| Value::Object(Default::default()));
    let field_caps = caps_result.unwrap_or_else(|_| Value::Object(Default::default()));
    if aliases_failed {
        warnings.push("Aliases could not be loaded; orphan analysis is incomplete.".into());
    }
    if settings_failed {
        warnings.push(
            "Index settings could not be loaded; hidden and ILM status may be incomplete.".into(),
        );
    }
    if caps_failed {
        warnings.push("Field capabilities could not be loaded; searchable and aggregatable flags are inferred.".into());
    }

    let data_stream_indices = parse_data_streams(streams_result.as_ref().ok());
    if streams_failed {
        warnings.push(
            "Data streams could not be loaded; backing indices cannot be excluded reliably.".into(),
        );
    }

    let original_index_count = cat_indices.len();
    if hide_internal {
        cat_indices.retain(|index| !is_hidden_index(&index.index, &settings));
    }
    let allowed_indices: BTreeSet<_> = cat_indices
        .iter()
        .map(|index| index.index.clone())
        .collect();
    let excluded_index_count = original_index_count.saturating_sub(cat_indices.len());

    let capability_lookup = parse_field_caps(&field_caps);
    let mut occurrences_by_path: BTreeMap<String, Vec<MappingFieldOccurrence>> = BTreeMap::new();
    let mut occurrences_by_index: HashMap<String, Vec<MappingFieldOccurrence>> = HashMap::new();
    let mut relations = Vec::new();

    if let Some(indices) = mappings.as_object() {
        for (index_name, root) in indices {
            if !allowed_indices.contains(index_name) {
                continue;
            }
            let mapping = root.get("mappings").unwrap_or(&Value::Null);
            let (index_occurrences, index_relations) =
                collect_mapping_fields(index_name, mapping, &capability_lookup);
            relations.extend(index_relations);
            for occurrence in index_occurrences {
                occurrences_by_path
                    .entry(occurrence.path.clone())
                    .or_default()
                    .push(occurrence.clone());
                occurrences_by_index
                    .entry(index_name.clone())
                    .or_default()
                    .push(occurrence);
            }
        }
    }

    let index_count = cat_indices.len();
    let fields = build_fields(occurrences_by_path, index_count);
    let mut index_models = build_indices(
        cat_indices,
        &aliases,
        &settings,
        &data_stream_indices,
        &occurrences_by_index,
    );
    index_models.sort_by(|a, b| a.name.cmp(&b.name));
    let orphan_candidates = find_orphan_candidates(&index_models);

    let mut categories: BTreeMap<String, usize> = BTreeMap::new();
    for field in &fields {
        *categories.entry(field.category.clone()).or_default() += 1;
    }
    let mut type_counts: Vec<_> = categories
        .into_iter()
        .map(|(category, count)| MappingTypeCount { category, count })
        .collect();
    type_counts.sort_by(|a, b| b.count.cmp(&a.count).then(a.category.cmp(&b.category)));

    Ok(MappingAnalysis {
        pattern: pattern.into(),
        hide_internal,
        excluded_index_count,
        generated_at: chrono::Utc::now().to_rfc3339(),
        cache_ttl_seconds: MAPPING_CACHE_TTL.as_secs(),
        index_count,
        unique_field_count: fields.len(),
        mapped_field_count: fields.iter().map(|field| field.coverage).sum(),
        conflict_count: fields.iter().filter(|field| field.conflict).count(),
        nested_count: fields
            .iter()
            .filter(|field| field.types.iter().any(|kind| kind == "nested"))
            .count(),
        relation_count: relations.len(),
        runtime_count: fields.iter().filter(|field| field.runtime).count(),
        indices: index_models,
        fields,
        relations,
        type_counts,
        orphan_candidates,
        warnings,
    })
}

fn flatten_properties(
    index: &str,
    prefix: &str,
    properties: &serde_json::Map<String, Value>,
    multi_field: bool,
    capabilities: &HashMap<(String, String, String), (bool, bool)>,
    output: &mut Vec<MappingFieldOccurrence>,
    relations: &mut Vec<MappingRelation>,
) {
    for (name, definition) in properties {
        let path = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}.{name}")
        };
        let occurrence =
            occurrence_from_definition(index, &path, definition, false, multi_field, capabilities);
        output.push(occurrence);

        if definition.get("type").and_then(Value::as_str) == Some("join") {
            if let Some(join_relations) = definition.get("relations").and_then(Value::as_object) {
                for (parent, children) in join_relations {
                    match children {
                        Value::String(child) => relations.push(MappingRelation {
                            index: index.into(),
                            field: path.clone(),
                            parent: parent.clone(),
                            child: child.clone(),
                        }),
                        Value::Array(children) => {
                            for child in children.iter().filter_map(Value::as_str) {
                                relations.push(MappingRelation {
                                    index: index.into(),
                                    field: path.clone(),
                                    parent: parent.clone(),
                                    child: child.into(),
                                });
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        if let Some(children) = definition.get("properties").and_then(Value::as_object) {
            flatten_properties(
                index,
                &path,
                children,
                false,
                capabilities,
                output,
                relations,
            );
        }
        if let Some(fields) = definition.get("fields").and_then(Value::as_object) {
            flatten_properties(index, &path, fields, true, capabilities, output, relations);
        }
    }
}

fn occurrence_from_definition(
    index: &str,
    path: &str,
    definition: &Value,
    runtime: bool,
    multi_field: bool,
    capabilities: &HashMap<(String, String, String), (bool, bool)>,
) -> MappingFieldOccurrence {
    let field_type = definition
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_else(|| {
            if definition.get("properties").is_some() {
                "object"
            } else {
                "unknown"
            }
        })
        .to_string();
    let (inferred_searchable, inferred_aggregatable) =
        inferred_capabilities(&field_type, definition);
    let (searchable, aggregatable) = capabilities
        .get(&(index.into(), path.into(), field_type.clone()))
        .copied()
        .unwrap_or((inferred_searchable, inferred_aggregatable));
    let format = string_property(definition, "format");
    let analyzer = string_property(definition, "analyzer");
    let search_analyzer = string_property(definition, "search_analyzer");
    let signature = mapping_signature(&field_type, definition, runtime, multi_field);
    MappingFieldOccurrence {
        path: path.into(),
        index: index.into(),
        field_type: field_type.clone(),
        category: category_for_type(&field_type).into(),
        searchable,
        aggregatable,
        runtime,
        multi_field,
        format,
        analyzer,
        search_analyzer,
        mapping_signature: signature,
    }
}

fn mapping_signature(
    field_type: &str,
    definition: &Value,
    runtime: bool,
    multi_field: bool,
) -> String {
    const MATERIAL_OPTIONS: &[&str] = &[
        "format",
        "analyzer",
        "search_analyzer",
        "normalizer",
        "ignore_above",
        "scaling_factor",
        "index_options",
        "similarity",
        "time_series_dimension",
        "relations",
        "null_value",
        "dynamic",
    ];
    let default_doc_values = !matches!(
        field_type,
        "text" | "match_only_text" | "object" | "nested" | "unknown"
    );
    let default_norms = matches!(field_type, "text" | "match_only_text");
    let mut parts = vec![
        format!("type={field_type}"),
        format!("runtime={runtime}"),
        format!("multi_field={multi_field}"),
        format!(
            "index={}",
            definition
                .get("index")
                .and_then(Value::as_bool)
                .unwrap_or(true)
        ),
        format!(
            "doc_values={}",
            definition
                .get("doc_values")
                .and_then(Value::as_bool)
                .unwrap_or(default_doc_values)
        ),
        format!(
            "store={}",
            definition
                .get("store")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        ),
        format!(
            "norms={}",
            definition
                .get("norms")
                .and_then(Value::as_bool)
                .unwrap_or(default_norms)
        ),
        format!(
            "coerce={}",
            definition
                .get("coerce")
                .and_then(Value::as_bool)
                .unwrap_or(true)
        ),
        format!(
            "enabled={}",
            definition
                .get("enabled")
                .and_then(Value::as_bool)
                .unwrap_or(true)
        ),
    ];
    for option in MATERIAL_OPTIONS {
        if let Some(value) = definition.get(*option) {
            parts.push(format!("{option}={value}"));
        }
    }
    parts.join(";")
}

fn string_property(value: &Value, property: &str) -> Option<String> {
    value.get(property).and_then(Value::as_str).map(Into::into)
}

fn inferred_capabilities(field_type: &str, definition: &Value) -> (bool, bool) {
    if definition.get("index").and_then(Value::as_bool) == Some(false) {
        return (false, false);
    }
    let aggregatable = definition.get("doc_values").and_then(Value::as_bool) != Some(false)
        && !matches!(
            field_type,
            "text" | "object" | "nested" | "join" | "unknown"
        );
    let searchable = !matches!(field_type, "object" | "nested" | "unknown");
    (searchable, aggregatable)
}

fn category_for_type(field_type: &str) -> &'static str {
    match field_type {
        "date" | "date_nanos" => "Date & time",
        "byte" | "short" | "integer" | "long" | "unsigned_long" | "half_float" | "float"
        | "double" | "scaled_float" => "Number",
        "text" | "keyword" | "constant_keyword" | "wildcard" | "match_only_text" => "Text",
        "boolean" => "Boolean",
        "object" | "flattened" => "Object",
        "nested" => "Nested",
        "join" => "Relation",
        "geo_point" | "geo_shape" | "point" | "shape" => "Geo",
        value if value.ends_with("_range") => "Range",
        _ => "Other",
    }
}

fn parse_field_caps(value: &Value) -> HashMap<(String, String, String), (bool, bool)> {
    let mut output = HashMap::new();
    let Some(fields) = value.get("fields").and_then(Value::as_object) else {
        return output;
    };
    for (path, types) in fields {
        let Some(types) = types.as_object() else {
            continue;
        };
        for (field_type, caps) in types {
            let searchable = caps
                .get("searchable")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let aggregatable = caps
                .get("aggregatable")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let indices = caps
                .get("indices")
                .and_then(Value::as_array)
                .map(|values| values.iter().filter_map(Value::as_str).collect::<Vec<_>>());
            if let Some(indices) = indices {
                for index in indices {
                    output.insert(
                        (index.into(), path.clone(), field_type.clone()),
                        (searchable, aggregatable),
                    );
                }
            }
        }
    }
    output
}

fn build_fields(
    fields: BTreeMap<String, Vec<MappingFieldOccurrence>>,
    index_count: usize,
) -> Vec<MappingField> {
    fields
        .into_iter()
        .filter(|(path, _)| !path.is_empty())
        .map(|(path, mut occurrences)| {
            occurrences.sort_by(|a, b| a.index.cmp(&b.index));
            let types: BTreeSet<_> = occurrences
                .iter()
                .map(|item| item.field_type.clone())
                .collect();
            let categories: BTreeSet<_> = occurrences
                .iter()
                .map(|item| item.category.clone())
                .collect();
            let signatures: BTreeSet<_> = occurrences
                .iter()
                .map(|item| item.mapping_signature.clone())
                .collect();
            let mut conflict_reasons = Vec::new();
            if types.len() > 1 {
                conflict_reasons.push(format!(
                    "Different types: {}",
                    types.iter().cloned().collect::<Vec<_>>().join(", ")
                ));
            }
            if signatures.len() > 1 && types.len() == 1 {
                conflict_reasons.push("Mapping options differ between indices".into());
            }
            let coverage = occurrences
                .iter()
                .map(|item| &item.index)
                .collect::<BTreeSet<_>>()
                .len();
            MappingField {
                path,
                category: if categories.len() == 1 {
                    categories
                        .into_iter()
                        .next()
                        .unwrap_or_else(|| "Other".into())
                } else {
                    "Conflict".into()
                },
                types: types.into_iter().collect(),
                coverage,
                coverage_percent: if index_count == 0 {
                    0
                } else {
                    ((coverage * 100) / index_count) as u8
                },
                searchable: occurrences.iter().all(|item| item.searchable),
                aggregatable: occurrences.iter().all(|item| item.aggregatable),
                runtime: occurrences.iter().any(|item| item.runtime),
                multi_field: occurrences.iter().any(|item| item.multi_field),
                conflict: !conflict_reasons.is_empty(),
                conflict_reasons,
                occurrences,
            }
        })
        .collect()
}

fn parse_aliases(value: &Value, index: &str) -> (Vec<String>, Vec<String>) {
    let alias_map = value
        .get(index)
        .and_then(|item| item.get("aliases"))
        .and_then(Value::as_object);
    let mut aliases: Vec<String> = alias_map
        .map(|aliases| aliases.keys().cloned().collect())
        .unwrap_or_default();
    let mut write_aliases: Vec<String> = alias_map
        .map(|aliases| {
            aliases
                .iter()
                .filter(|(_, definition)| {
                    definition.get("is_write_index").and_then(Value::as_bool) == Some(true)
                })
                .map(|(name, _)| name.clone())
                .collect()
        })
        .unwrap_or_default();
    aliases.sort();
    write_aliases.sort();
    (aliases, write_aliases)
}

fn parse_data_streams(value: Option<&Value>) -> HashMap<String, String> {
    let mut output = HashMap::new();
    let Some(streams) = value
        .and_then(|value| value.get("data_streams"))
        .and_then(Value::as_array)
    else {
        return output;
    };
    for stream in streams {
        let Some(name) = stream.get("name").and_then(Value::as_str) else {
            continue;
        };
        if let Some(indices) = stream.get("indices").and_then(Value::as_array) {
            for index in indices {
                if let Some(index_name) = index.get("index_name").and_then(Value::as_str) {
                    output.insert(index_name.into(), name.into());
                }
            }
        }
    }
    output
}

fn is_hidden_index(index: &str, settings: &Value) -> bool {
    index.starts_with('.')
        || settings
            .get(index)
            .and_then(|value| value.get("settings"))
            .and_then(|value| value.get("index.hidden"))
            .and_then(Value::as_str)
            == Some("true")
}

fn build_indices(
    cat_indices: Vec<CatIndex>,
    aliases: &Value,
    settings: &Value,
    data_streams: &HashMap<String, String>,
    occurrences: &HashMap<String, Vec<MappingFieldOccurrence>>,
) -> Vec<MappingIndex> {
    cat_indices
        .into_iter()
        .map(|index| {
            let index_occurrences = occurrences.get(&index.index).cloned().unwrap_or_default();
            let mapping_fingerprint = index_occurrences
                .iter()
                .map(|item| item.mapping_signature.as_str())
                .collect::<Vec<_>>()
                .join("|");
            let index_settings = settings
                .get(&index.index)
                .and_then(|value| value.get("settings"));
            let hidden = index.index.starts_with('.')
                || index_settings
                    .and_then(|value| value.get("index.hidden"))
                    .and_then(Value::as_str)
                    == Some("true");
            let ilm_managed = index_settings
                .and_then(|value| value.get("index.lifecycle.name"))
                .and_then(Value::as_str)
                .is_some_and(|value| !value.is_empty());
            let (aliases, write_aliases) = parse_aliases(aliases, &index.index);
            MappingIndex {
                aliases,
                write_aliases,
                field_count: index_occurrences.len(),
                mapping_fingerprint,
                data_stream: data_streams.get(&index.index).cloned(),
                hidden,
                ilm_managed,
                name: index.index,
                docs_count: index.docs_count,
                store_size_bytes: index.store_size_bytes,
                creation_date_millis: index.creation_date_millis,
                health: index.health,
                status: index.status,
            }
        })
        .collect()
}

fn find_orphan_candidates(indices: &[MappingIndex]) -> Vec<OrphanCandidate> {
    let aliases: BTreeSet<_> = indices
        .iter()
        .flat_map(|index| index.aliases.iter().cloned())
        .collect();
    let mut candidates = Vec::new();
    for index in indices {
        if !index.aliases.is_empty()
            || index.hidden
            || index.data_stream.is_some()
            || index.ilm_managed
        {
            continue;
        }
        let likely_family = aliases
            .iter()
            .filter(|alias| name_belongs_to_alias_family(&index.name, alias))
            .max_by_key(|alias| alias.len())
            .cloned();
        let active = likely_family.as_ref().and_then(|family| {
            let carriers: Vec<_> = indices
                .iter()
                .filter(|candidate| candidate.aliases.contains(family))
                .collect();
            carriers
                .iter()
                .copied()
                .find(|candidate| candidate.write_aliases.contains(family))
                .or_else(|| {
                    if carriers.len() == 1 {
                        carriers.first().copied()
                    } else {
                        carriers
                            .into_iter()
                            .max_by_key(|candidate| candidate.creation_date_millis.unwrap_or(0))
                    }
                })
        });
        let mut score = 25u8;
        let mut reasons = vec!["Index has no alias".into()];
        if let Some(family) = &likely_family {
            score = score.saturating_add(30);
            reasons.push(format!("Name matches alias family {family}"));
        }
        let mut mapping_identical = false;
        if let Some(active) = active {
            if !index.mapping_fingerprint.is_empty()
                && index.mapping_fingerprint == active.mapping_fingerprint
            {
                score = score.saturating_add(20);
                mapping_identical = true;
                reasons.push(format!("Mapping is identical to {}", active.name));
            }
            if is_older(index, active) {
                score = score.saturating_add(10);
                reasons.push("A newer index currently carries the family alias".into());
            }
            if values_are_close(index.docs_count, active.docs_count, 2) {
                score = score.saturating_add(10);
                reasons.push("Document counts differ by no more than 2%".into());
            }
            if index.docs_count > 0
                && active.docs_count > 0
                && values_are_close(index.store_size_bytes, active.store_size_bytes, 20)
            {
                score = score.saturating_add(5);
                reasons.push("Store sizes differ by no more than 20%".into());
            }
        }
        let confidence = if score >= 80 {
            "High"
        } else if score >= 55 {
            "Medium"
        } else {
            "Low"
        };
        candidates.push(OrphanCandidate {
            index: index.name.clone(),
            confidence: confidence.into(),
            score: score.min(100),
            likely_family,
            active_index: active.map(|value| value.name.clone()),
            mapping_identical,
            docs_count: index.docs_count,
            active_docs_count: active.map(|value| value.docs_count),
            store_size_bytes: index.store_size_bytes,
            active_store_size_bytes: active.map(|value| value.store_size_bytes),
            reasons,
        });
    }
    candidates.sort_by(|a, b| b.score.cmp(&a.score).then(a.index.cmp(&b.index)));
    candidates
}

fn name_belongs_to_alias_family(index: &str, alias: &str) -> bool {
    index
        .strip_prefix(alias)
        .is_some_and(|suffix| suffix.starts_with(['-', '_', '.']))
}

fn is_older(candidate: &MappingIndex, active: &MappingIndex) -> bool {
    matches!(
        (candidate.creation_date_millis, active.creation_date_millis),
        (Some(candidate), Some(active)) if candidate < active
    )
}

fn values_are_close(left: u64, right: u64, tolerance_percent: u64) -> bool {
    if left == 0 || right == 0 {
        return false;
    }
    let maximum = left.max(right);
    let difference = left.abs_diff(right);
    maximum > 0 && difference.saturating_mul(100) <= maximum.saturating_mul(tolerance_percent)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_dot_prefixed_and_explicitly_hidden_indices() {
        let settings = serde_json::json!({
            "hidden-business-index": {
                "settings": { "index.hidden": "true" }
            }
        });
        assert!(is_hidden_index(".kibana", &settings));
        assert!(is_hidden_index("hidden-business-index", &settings));
        assert!(!is_hidden_index("visible-business-index", &settings));
    }

    fn index(name: &str, aliases: &[&str], created: i64, mapping: &str, docs: u64) -> MappingIndex {
        MappingIndex {
            name: name.into(),
            aliases: aliases.iter().map(|value| (*value).into()).collect(),
            write_aliases: aliases.iter().map(|value| (*value).into()).collect(),
            docs_count: docs,
            store_size_bytes: docs * 10,
            creation_date_millis: Some(created),
            health: "green".into(),
            status: "open".into(),
            field_count: 2,
            mapping_fingerprint: mapping.into(),
            hidden: false,
            data_stream: None,
            ilm_managed: false,
        }
    }

    #[test]
    fn detects_reindex_leftover_as_high_confidence_candidate() {
        let indices = vec![
            index("tsm-partticket-20260605-copy", &[], 1, "same", 1000),
            index(
                "tsm-partticket-20260828-000001",
                &["tsm-partticket"],
                2,
                "same",
                1001,
            ),
            index(
                "tsm-partticket-template-20260828",
                &["tsm-partticket-template"],
                2,
                "other",
                5,
            ),
        ];
        let candidates = find_orphan_candidates(&indices);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].confidence, "High");
        assert_eq!(
            candidates[0].likely_family.as_deref(),
            Some("tsm-partticket")
        );
        assert_eq!(
            candidates[0].active_index.as_deref(),
            Some("tsm-partticket-20260828-000001")
        );
    }

    #[test]
    fn excludes_aliased_hidden_stream_and_ilm_indices() {
        let mut hidden = index(".hidden", &[], 1, "a", 1);
        hidden.hidden = true;
        let mut stream = index("stream-000001", &[], 1, "a", 1);
        stream.data_stream = Some("stream".into());
        let mut ilm = index("ilm-000001", &[], 1, "a", 1);
        ilm.ilm_managed = true;
        let aliased = index("active-000001", &["active"], 1, "a", 1);
        assert!(find_orphan_candidates(&[hidden, stream, ilm, aliased]).is_empty());
    }

    #[test]
    fn reports_type_conflicts() {
        let fields = build_fields(
            BTreeMap::from([(
                "createdAt".into(),
                vec![
                    MappingFieldOccurrence {
                        path: "createdAt".into(),
                        index: "one".into(),
                        field_type: "date".into(),
                        category: "Date & time".into(),
                        searchable: true,
                        aggregatable: true,
                        runtime: false,
                        multi_field: false,
                        format: None,
                        analyzer: None,
                        search_analyzer: None,
                        mapping_signature: "date".into(),
                    },
                    MappingFieldOccurrence {
                        path: "createdAt".into(),
                        index: "two".into(),
                        field_type: "keyword".into(),
                        category: "Text".into(),
                        searchable: true,
                        aggregatable: true,
                        runtime: false,
                        multi_field: false,
                        format: None,
                        analyzer: None,
                        search_analyzer: None,
                        mapping_signature: "keyword".into(),
                    },
                ],
            )]),
            2,
        );
        assert!(fields[0].conflict);
        assert_eq!(fields[0].coverage_percent, 100);
    }

    #[test]
    fn flattens_index_detail_mapping_fields() {
        let mapping = serde_json::json!({
            "properties": {
                "account": {
                    "properties": {
                        "createdAt": { "type": "date", "format": "strict_date_time" }
                    }
                },
                "name": {
                    "type": "text",
                    "fields": { "keyword": { "type": "keyword" } }
                }
            },
            "runtime": {
                "displayName": { "type": "keyword" }
            }
        });

        let fields = mapping_fields_for_index("accounts", &mapping);
        let paths: Vec<_> = fields.iter().map(|field| field.path.as_str()).collect();

        assert_eq!(
            paths,
            [
                "account",
                "account.createdAt",
                "displayName",
                "name",
                "name.keyword"
            ]
        );
        assert!(
            fields
                .iter()
                .any(|field| field.path == "displayName" && field.runtime)
        );
        assert!(
            fields
                .iter()
                .any(|field| field.path == "name.keyword" && field.multi_field)
        );
    }

    #[test]
    fn empty_indices_do_not_gain_similarity_points() {
        assert!(!values_are_close(0, 0, 2));
        assert!(!values_are_close(0, 100, 20));
        assert!(values_are_close(980, 1000, 2));
    }
}
