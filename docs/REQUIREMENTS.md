# Elastic Explorer - Requirements

## Basic Concept
An Elasticsearch cluster explorer written in Rust - a simpler and more intuitive alternative to ElasticVue.

## Launch
```bash
elastic-explorer
```
- Starts HTTP server (default: `127.0.0.1:8080`)
- Automatically opens browser
  - macOS: `open http://127.0.0.1:8080`
  - Linux: `xdg-open http://127.0.0.1:8080`
  - Windows: `start http://127.0.0.1:8080`

## Data Layer

### Directory Structure
- **Linux/macOS**: `~/.elastic-explorer/data/`
- **Windows**: `%APPDATA%\elastic-explorer\data\`

### SQLite Database
Stores endpoint configurations:

**Table: endpoints**
- `id` - PRIMARY KEY
- `name` - endpoint name (e.g., "Production", "Local Dev")
- `url` - full URL (scheme + hostname + port)
- `insecure` - boolean (for self-signed certificates)
- `username` - for Basic Auth (nullable)
- `password_keychain_id` - OS keychain reference (nullable)
- `password_fallback` - base64 encoded fallback (nullable)
- `created_at` - timestamp
- `updated_at` - timestamp

**Table: console_history**
- `id` - PRIMARY KEY
- `endpoint_id` - FOREIGN KEY to endpoints
- `method` - HTTP method (GET, POST, PUT, DELETE, etc.)
- `path` - API path
- `body` - request body (nullable)
- `response_status` - HTTP status code (nullable)
- `response_body` - response content (nullable)
- `created_at` - timestamp

## Web UI - Structure

### Main Navigation (navbar)
1. **Endpoint selector** - dropdown to switch between saved endpoints
2. **Edit endpoint** - button/icon to edit current endpoint
3. **Auto-refresh**
   - Checkbox to enable/disable
   - Select/input for interval: 1s, 5s, 30s, 1min, 5min

### Dashboard (main page)

#### Cluster Overview
- **Elasticsearch version**
- **Disk usage** - total/free space
- **CPU utilization** - per node
- **RAM utilization** - per node
- **Realtime graphs** - sparkline charts for metrics

#### Nodes
- **Node list** with information:
  - Node name
  - Role (master, data, ingest, ml, ...)
  - Master node indicator
  - CPU/RAM/Disk usage
  - **Click-through** to node detail

### Node Detail
- **Summary** (top section)
  - Basic info: name, IP, version, roles
  - Current metrics: CPU, RAM, Disk, JVM heap
- **Details** (bottom section)
  - Thread pools
  - File descriptors
  - Network stats
  - Plugins

### Indices

#### Index List
- **Paginated table**
- **Filters** (above table):
  - Text filter (substring match)
  - **Regex filter** - option to enter regular expression ⭐
  - Filter by status (green/yellow/red)
  - Filter by size
- **Columns**:
  - Checkbox (for bulk operations)
  - Name
  - Status (colored badge)
  - Health (green/yellow/red)
  - Document count
  - Primary shard count
  - Replica count
  - Total size
  - Primary size
- **Actions** (per row):
  - View detail
  - Delete (with confirmation)
- **Bulk operations**:
  - Close selected indices
  - Delete selected indices (with confirmation)
  - Force merge

#### Index Detail
- **Overview tab**:
  - Basic stats
  - Document count, size
  - Shard count
  - Status
- **Mapping tab**:
  - Field list with types
  - Searchable JSON view
- **Settings tab**:
  - All index settings
  - Editable (dangerous - requires confirmation)
- **Shards tab**:
  - Visual shard distribution
  - Shard status per node

### Search

#### Query Interface
- **Query type selector**: DSL / SQL
- **DSL mode**:
  - Index selector (dropdown + autocomplete)
  - JSON editor for query
  - Syntax highlighting
- **SQL mode** (ES 7.x+):
  - SQL query editor
  - Syntax highlighting
- **Execute button**
- **Results**:
  - Paginated table
  - JSON pretty print per document
  - Export options (JSON, CSV)

#### Saved Queries
- **List of saved queries**
- **Save current query**:
  - Name
  - Description
  - Tags
- **Load saved query** - one click

### Dev Console

- **Interactive API explorer** (like Kibana's Dev Tools)
- **Request editor**:
  - HTTP method selector (GET, POST, PUT, DELETE, HEAD)
  - Path input
  - Request body editor (JSON)
- **Response viewer**:
  - Auto-detection (JSON or plain text)
  - Pretty-printed JSON
  - Status code badge (color-coded)
  - Copy to clipboard
- **Request history**:
  - Stored in SQLite with endpoint context
  - Filter by endpoint
  - Load previous request with one click
- **Confirmation modal** for destructive operations (DELETE, PUT)

### Shards

#### Shard Visualization
- **Visual grid**:
  - Rows = indices
  - Columns = nodes
  - Cells = shards (colored by status)
- **Filters**:
  - Index pattern (regex support)
  - Node filter
  - Status filter (STARTED, RELOCATING, INITIALIZING, UNASSIGNED)
- **Shard detail** (on click):
  - Shard number
  - Index
  - Node
  - State
  - Size
  - Docs
  - Segment count

### Templates

#### Index Templates
- **List of templates**
- **Filter** by name
- **Detail view**:
  - Template pattern
  - Settings
  - Mappings
  - Aliases
- **Edit/Delete** with confirmation
- **Create new template**

#### Component Templates (ES 7.8+)
- **List of component templates**
- **Composable templates** - which templates use which components
- **Diff view** - compare templates ⭐

## Technology Stack

- **Backend**: Axum 0.8, Tokio
- **Database**: SQLite (sqlx), keyring for password storage
- **ES Client**: reqwest with custom wrapper
- **Frontend**: HTMX, Server-Sent Events for real-time updates
- **Templates**: Askama (compile-time checked)
- **UI Framework**: Tabler (Bootstrap 5 based)

## Key Improvements over ElasticVue

1. **Regex filters** for indices - more powerful than substring
2. **Better shard visualization** - clearer layout
3. **Dev Console** with history - like Kibana but faster
4. **SQL query support** for ES 7.x+
5. **Native OS password storage** - more secure
6. **Template diff view** - easier comparison
7. **Faster** - native binary, no Electron overhead
8. **Lower memory usage** - Rust efficiency
