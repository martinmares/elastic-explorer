#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

compose_file="compose.snapshot-smoke.yml"
es_url="${ES_URL:-http://127.0.0.1:19200}"
repository="elastic-explorer-smoke"
snapshot="manual-smoke"
safe_prefix="restore-test"
snapshot_smoke_dir="$(mktemp -d "${TMPDIR:-/tmp}/elastic-explorer-snapshot-smoke.XXXXXX")"
chmod 0777 "$snapshot_smoke_dir"
export SNAPSHOT_SMOKE_DIR="$snapshot_smoke_dir"

for command in docker curl jq; do
  command -v "$command" >/dev/null || { echo "Missing required command: $command" >&2; exit 1; }
done

# Always use the Docker daemon's native architecture. A developer-wide
# DOCKER_DEFAULT_PLATFORM=linux/amd64 otherwise breaks current Elasticsearch
# under emulation on Apple Silicon before the test can even start.
docker_arch="$(docker version --format '{{.Server.Arch}}')"
export DOCKER_DEFAULT_PLATFORM="linux/$docker_arch"

cleanup() {
  if [[ "${KEEP_SNAPSHOT_SMOKE:-0}" != "1" ]]; then
    docker compose -f "$compose_file" down -v --remove-orphans
    rm -rf -- "$snapshot_smoke_dir"
  else
    echo "Keeping snapshot smoke cluster and repository at $snapshot_smoke_dir"
  fi
}
trap cleanup EXIT

docker compose -f "$compose_file" down -v --remove-orphans
docker compose -f "$compose_file" pull --policy always
docker compose -f "$compose_file" up -d --wait

request() {
  local method="$1" path="$2" body="${3:-}"
  if [[ -n "$body" ]]; then
    curl --fail-with-body -sS -X "$method" "$es_url$path" -H 'Content-Type: application/json' --data-binary "$body"
  else
    curl --fail-with-body -sS -X "$method" "$es_url$path"
  fi
}

wait_snapshot() {
  local name="$1" state
  for _ in $(seq 1 90); do
    state="$(request GET "/_snapshot/$repository/$name" | jq -r '.snapshots[0].state // "STARTING"')"
    case "$state" in
      SUCCESS) return 0 ;;
      FAILED|PARTIAL) echo "Snapshot $name ended in state $state" >&2; return 1 ;;
    esac
    sleep 1
  done
  echo "Timed out waiting for snapshot $name" >&2
  return 1
}

echo "Registering and verifying the filesystem repository"
request PUT "/_snapshot/$repository" '{"type":"fs","settings":{"location":"/snapshots/elastic-explorer","compress":true}}' >/dev/null
request POST "/_snapshot/$repository/_verify" >/dev/null

echo "Creating source indices, mappings, documents and write aliases"
request PUT '/tsm-sdaorder_000001' '{"mappings":{"properties":{"id":{"type":"keyword"},"message":{"type":"text"}}},"aliases":{"tsm-sdaorder":{"is_write_index":true},"tsm-sdaorder-active":{"is_write_index":true}}}' >/dev/null
request PUT '/tsm-sdaaccount_000001' '{"mappings":{"properties":{"id":{"type":"keyword"},"name":{"type":"keyword"}}},"aliases":{"tsm-sdaaccount":{"is_write_index":true}}}' >/dev/null
request POST '/_bulk?refresh=true' $'{"index":{"_index":"tsm-sdaorder","_id":"1"}}\n{"id":"1","message":"before snapshot"}\n{"index":{"_index":"tsm-sdaaccount","_id":"1"}}\n{"id":"1","name":"Alice"}\n' >/dev/null

cluster_uuid="$(request GET / | jq -r .cluster_uuid)"
cluster_name="$(request GET / | jq -r .cluster_name)"
snapshot_body="$(jq -nc --arg uuid "$cluster_uuid" --arg cluster "$cluster_name" '{indices:"tsm-sda*",include_global_state:false,partial:false,metadata:{created_by:"elastic-explorer",kind:"manual",scope:"all",note:"Docker smoke test",index_prefix:"tsm-sda",source_cluster_uuid:$uuid,source_cluster_name:$cluster}}')"
request PUT "/_snapshot/$repository/$snapshot?wait_for_completion=false" "$snapshot_body" >/dev/null
wait_snapshot "$snapshot"

echo "Testing safe prefix-replacement restore without aliases"
request POST "/_snapshot/$repository/$snapshot/_restore?wait_for_completion=true" '{"indices":"tsm-sda*","include_global_state":false,"include_aliases":false,"rename_pattern":"^tsm-sda(.*)$","rename_replacement":"restore-test$1"}' >/dev/null
[[ "$(request GET "/${safe_prefix}order_000001/_count" | jq -r .count)" == "1" ]]
[[ "$(request GET "/${safe_prefix}order_000001/_alias" | jq '.[] | .aliases | length')" == "0" ]]
request DELETE "/${safe_prefix}order_000001,${safe_prefix}account_000001" >/dev/null

echo "Testing full in-place restore after exact source-index deletion"
request POST '/tsm-sdaorder/_doc/2?refresh=true' '{"id":"2","message":"must disappear"}' >/dev/null
request DELETE '/tsm-sdaorder_000001,tsm-sdaaccount_000001' >/dev/null
request POST "/_snapshot/$repository/$snapshot/_restore?wait_for_completion=true" '{"indices":"tsm-sda*","include_global_state":false,"include_aliases":true}' >/dev/null
[[ "$(request GET '/tsm-sdaorder_000001/_count' | jq -r .count)" == "1" ]]
[[ "$(request GET '/tsm-sdaorder_000001/_alias/tsm-sdaorder' | jq -r '.[].aliases["tsm-sdaorder"].is_write_index')" == "true" ]]

echo "Testing native snapshot metadata used by the application scheduler"
scheduled_name="elastic-explorer-scheduled-smoke"
scheduled_body="$(jq -nc --arg uuid "$cluster_uuid" --arg cluster "$cluster_name" '{indices:"tsm-sda*",include_global_state:false,partial:false,metadata:{created_by:"elastic-explorer",kind:"scheduled",scope:"all",note:"Automatic smoke test",index_prefix:"tsm-sda",source_cluster_uuid:$uuid,source_cluster_name:$cluster}}')"
request PUT "/_snapshot/$repository/$scheduled_name?wait_for_completion=false" "$scheduled_body" >/dev/null
wait_snapshot "$scheduled_name"
[[ "$(request GET "/_snapshot/$repository/$scheduled_name" | jq -r '.snapshots[0].metadata.kind')" == "scheduled" ]]

echo "Snapshot smoke test passed"
