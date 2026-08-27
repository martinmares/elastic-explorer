# Elastic Explorer

A modern Elasticsearch cluster explorer written in Rust - a simpler and more intuitive alternative to ElasticVue.

## Features

- 🔍 **Dashboard** - Cluster health, metrics, and node overview
- 📊 **Indices** - List, filter (with regex), bulk operations, and detailed information
- 🔎 **Search** - Query DSL and SQL support with saved queries
- 🖥️ **Dev Console** - Interactive API explorer (like Kibana's Dev Tools)
- 🔧 **Shards** - Visual shard distribution and status
- 📝 **Templates** - Index and component template management
- 🔐 **Secure** - Passwords stored encrypted in the local SQLite database
- 📷 **Snapshots** - Native Elasticsearch snapshots, SLM scheduling and guarded restore workflows

## Quick Start

### Prerequisites

- Rust 1.70 or later
- Elasticsearch 3.x - 8.x

### Build

```bash
just build-macos
```

See [Build and embedded UI assets](#build-and-embedded-ui-assets) for all
supported targets.

### Run

```bash
# Basic usage (server on 127.0.0.1:8080)
cargo run

# Custom port
cargo run -- --port 3000

# Custom host
cargo run -- --host 0.0.0.0 --port 8080

# Don't open browser automatically
cargo run -- --no-browser

# Run behind a reverse proxy subpath (e.g. https://host/elastic-explorer/)
cargo run -- --base-path /elastic-explorer

# Stateless mode (no local storage)
cargo run -- --stateless \
  --conf-es-url "https://127.0.0.1:9200" \
  --conf-es-username elastic \
  --conf-es-password changeme

# Help
cargo run -- --help
```

### Install

```bash
cargo install --path .
elastic-explorer
```

The application will automatically open in your default browser at http://127.0.0.1:8080

## Stateless mode (no local storage)

Use `--stateless` to run without SQLite and provide a single connection via CLI
or `.env`:

```bash
elastic-explorer --stateless --conf-es-url "https://127.0.0.1:9200"
```

Supported `--conf-es-*` parameters (also via `.env`):
- `--conf-es-name` / `CONF_ES_NAME`
- `--conf-es-url` / `CONF_ES_URL`
- `--conf-es-username` / `CONF_ES_USERNAME`
- `--conf-es-password` / `CONF_ES_PASSWORD`
- `--conf-es-insecure` / `CONF_ES_INSECURE`

### Managed snapshots (stateless mode)

Snapshots use a repository already registered in Elasticsearch. `SNAPSHOT_INDEX_PREFIX`
is a literal prefix; the application appends `*`, so `tsm-sda` manages `tsm-sda*`.

```bash
elastic-explorer --stateless \
  --conf-es-url "http://127.0.0.1:9200" \
  --snapshots-enabled \
  --snapshot-repository elastic-explorer \
  --snapshot-index-prefix tsm-sda
```

Equivalent environment variables are `SNAPSHOTS_ENABLED`,
`SNAPSHOT_REPOSITORY`, and `SNAPSHOT_INDEX_PREFIX`. Optional automatic snapshots
are reconciled into Elasticsearch SLM at startup:

```dotenv
SCHEDULED_SNAPSHOT_CRON=0 0 20 * * ?
SCHEDULED_SNAPSHOT_KEEP_LAST=14
SCHEDULED_SNAPSHOT_MAX_AGE_DAYS=30
SCHEDULED_SNAPSHOT_NOTE=Automatic snapshot
```

SLM evaluates cron schedules in UTC. It has no policy timezone field, so a
fixed Europe/Prague wall-clock time requires accounting for daylight-saving
changes (or using an interval schedule such as `24h`).

The Snapshots page is Admin-only. Safe restore replaces the source prefix,
creates no aliases and refuses name collisions. In-place restore is deliberately
non-atomic: after cluster UUID and typed-confirmation checks it deletes every
current managed-prefix index and restores a full snapshot with aliases.

Run the isolated Elasticsearch 8.19.17 integration test with:

```bash
./integration/snapshot-smoke.sh
```

Set `KEEP_SNAPSHOT_SMOKE=1` to leave the Compose cluster running on port 19200.

## Trusted proxy authentication

When deployed behind `simple-idm-oauth2-proxy` or `simple-idm-ad-proxy`, enable
trusted proxy mode and bind the app to loopback:

```bash
elastic-explorer \
  --host 127.0.0.1 \
  --base-path /elastic-explorer \
  --stateless \
  --trusted-proxy-auth
```

The proxy must strip incoming client-supplied `X-Auth-*` / `X-WEBAUTH-*` headers
and set trusted identity headers after authentication. Supported role groups:

- `elastic-explorer:admin`
- `elastic-explorer:editor`
- `elastic-explorer:viewer`

Equivalent environment variables:

- `TRUSTED_PROXY_AUTH=true`
- `AUTH_GROUP_ADMIN=elastic-explorer:admin`
- `AUTH_GROUP_EDITOR=elastic-explorer:editor`
- `AUTH_GROUP_VIEWER=elastic-explorer:viewer`

Role model:

- `Viewer`: read-only pages.
- `Editor`: aliases, refresh/open/close, replica changes and new index from
  mapping.
- `Admin`: endpoint changes, raw console execution, document bulk delete and
  index delete.

## Configuration

The application creates a data directory based on your operating system:

- **macOS/Linux**: `~/.elastic-explorer/data/`
- **Windows**: `%APPDATA%\elastic-explorer\data\`

This directory contains the SQLite database with endpoint configurations and the encryption key file.

### Password Security

Basic Auth passwords are stored encrypted in the local SQLite database using AES-256-GCM.
The encryption key is generated on first run and stored in `~/.elastic-explorer/db.key`.

Important: back up the entire `~/.elastic-explorer/` directory (the SQLite DB plus `db.key`).
If the key is lost, stored passwords cannot be recovered.

## Documentation

- [Development Progress](docs/PROGRESS.md) - Implementation status
- [Requirements](docs/REQUIREMENTS.md) - Detailed requirements
- [UI Implementation](docs/UI_IMPLEMENTED.md) - UI implementation details
- [Search Improvements](docs/SEARCH_IMPROVEMENTS.md) - Future search enhancements

## Technology Stack

- **Backend**: Axum 0.8, Tokio
- **Database**: SQLite (sqlx)
- **ES Client**: reqwest with custom wrapper
- **Frontend**: HTMX, Server-Sent Events
- **Templates**: Askama
- **UI**: Tabler, Bootstrap 5

## Development

### Important Notes

**Axum 0.8.x syntax:**
- Path parameters use `{param}` instead of `:param`
- Example: `.route("/indices/detail/{index_name}", get(handler))`
- **NOT**: `.route("/indices/detail/:index_name", get(handler))`

### Running Tests

```bash
cargo test
```

### Build and embedded UI assets

The UI never loads JavaScript, CSS or fonts from a CDN at runtime. Versioned
vendor files live under `static/vendor/` and are embedded into the binary.

```bash
just assets-check
just build-linux    # x86_64 Linux MUSL via cargo-zigbuild
just build-macos    # Apple Silicon macOS
just build-windows  # x86_64 Windows GNU
just build-all
```

Run `just assets-update` only when intentionally updating UI dependencies,
then review and commit the downloaded files. Normal builds are offline with
respect to UI assets. The Linux build requires Zig and `cargo-zigbuild`.

## License

AGPLv3 License - see [LICENSE](LICENSE) file for details

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## Author

Martin Mareš
