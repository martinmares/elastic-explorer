# Elastic Explorer - Průběh implementace

## ✅ Hotovo (2026-01-12)

### Základ projektu
- [x] Cargo.toml s dependencies (axum 0.8, tokio, sqlx, reqwest, keyring, clap, askama)
- [x] Struktura adresářů (`src/{config,db,es,handlers,models,templates,utils}`)
- [x] .gitignore

### Config modul (`src/config/mod.rs`)
- [x] Funkce pro získání app directory dle OS
  - macOS/Linux: `~/.elastic-explorer/`
  - Windows: `%APPDATA%\elastic-explorer\`
- [x] Funkce pro data directory a DB cestu
- [x] Inicializace adresářů

### Databáze (`src/db/`)
- [x] SQLite migrace (`migrations/001_init.sql`)
  - Tabulka `endpoints` (id, name, url, insecure, username, password_keychain_id, timestamps)
  - Tabulka `saved_queries` (id, name, query_type, query_body, indices, description, timestamps)
  - Indexy a triggery pro auto-update timestamps
- [x] Modely (`src/db/models.rs`)
  - `Endpoint`, `CreateEndpoint`, `UpdateEndpoint`
  - `SavedQuery`, `CreateSavedQuery`
- [x] Credential management (`src/db/credentials.rs`)
  - `store_password()` - ukládání do OS keychain
  - `get_password()` - načtení z keychain
  - `delete_password()` - smazání z keychain
  - `generate_keychain_id()` - generování ID
- [x] Database struct (`src/db/mod.rs`)
  - Připojení k SQLite
  - Spuštění migrací
  - CRUD operace pro endpoints
  - Integrace s keychain pro hesla
  - Query operations pro saved_queries

### Elasticsearch klient (`src/es/`)
- [x] `EsClient` wrapper (`src/es/client.rs`)
  - Inicializace s URL, insecure flag, Basic Auth
  - Automatická detekce verze ES
  - Univerzální GET/POST/DELETE metody
  - Error handling
- [x] API metody (`src/es/api.rs`)
  - `cluster_health()`
  - `cluster_stats()`
  - `get_nodes()`, `get_node()`
  - `get_indices()`, `get_index()`, `delete_index()`
  - `get_shards()`
  - `search()` - Query DSL
  - `search_sql()` - SQL API (ES 7.x+)
  - `get_mapping()`, `get_settings()`
  - `get_index_templates()` - kompatibilita pro ES 7.x a 8.x
  - `get_component_templates()` - ES 7.8+

### Utilities (`src/utils/`)
- [x] `open_browser()` - otevření prohlížeče dle OS
  - macOS: `open`
  - Linux: `xdg-open`
  - Windows: `cmd /C start`

### Web server (`src/`)
- [x] Main.rs s axum serverem
  - CLI argumenty (--host, --port, --no-browser)
  - Inicializace DB a adresářů
  - Router s routes pro dashboard, endpoints
  - Static files serving (`/static`)
  - Auto-open prohlížeče při startu
  - Shared state (AppState) s DB poolem
- [x] Handlers
  - `index()` - redirect na dashboard
  - `health()` - health check endpoint
  - `dashboard::dashboard()` - dashboard stránka
  - `endpoints::list_endpoints()` - seznam endpointů
  - `endpoints::create_endpoint()` - vytvoření endpointu
  - `endpoints::delete_endpoint()` - smazání endpointu

### Web UI (`src/templates/`)
- [x] Base layout template (`base.html`) s **Tabler CSS** ⭐
  - Navbar s endpoint selectorem
  - Horizontal navigace (Dashboard, Indexy, Nodes, Shards, Search, Templates, Saved Queries)
  - **Dark mode toggle** (světlý/tmavý/auto) ⭐
  - Footer
  - HTMX integrace (CDN)
  - Tabler Icons
- [x] Askama template moduly (`templates/mod.rs`)
  - `EndpointsTemplate`
  - `DashboardTemplate`

### Endpoints management ✅
- [x] Seznam endpointů (`endpoints.html`)
- [x] Formulář pro vytvoření endpointu (modal)
  - Název, URL, Insecure checkbox
  - Basic Auth (username, password)
  - Keychain integrace info
- [x] HTMX pro dynamické aktualizace
- [x] **Konfirmační dialog pro smazání** ⭐
- [x] Empty state (když nejsou endpointy)
- [x] Tlačítka pro Test connection a Select endpoint (připraveno)

### Dashboard 🚧 (základní kostra)
- [x] Dashboard stránka (`dashboard.html`)
  - Empty state (když není vybrán endpoint)
  - Placeholder karty pro metriky
  - Připraveno pro živá data

### Dashboard
- [ ] Cluster overview
  - Cluster health (green/yellow/red)
  - Verze ES
  - Počet nodů (total, data nodes)
  - Počet indexů
- [ ] Disk utilization
- [ ] CPU a RAM metriky (per node)
- [ ] Sparkline grafy pro realtime metriky
- [ ] SSE endpoint pro auto-refresh dat
- [ ] Seznam nodů s rolemi a master označením

### Nodes
- [ ] Seznam nodů (tabulka)
- [ ] Detail nodu (proklik)
  - Summary blok (název, IP, verze, role, metriky)
  - Detail blok (thread pools, file descriptors, network stats, plugins)

### Indexy
- [ ] Seznam indexů (tabulka s paginací)
- [ ] Filtry
  - Textový filtr (substring)
  - **Regexp filtr** ⭐
  - Filtr podle health (green/yellow/red)
  - Filtr podle velikosti
- [ ] Sloupce
  - Health status
  - Název (proklik na detail)
  - Počet dokumentů
  - Velikost (primární + repliky)
  - Počet shardů
- [ ] Operace
  - Checkbox pro multi-select
  - Smazání indexu/indexů s konfirmačním dialogem ⭐
  - Refresh, Flush, Force merge
  - Close/Open index
- [ ] Detail indexu
  - Mapping
  - Settings
  - Stats
  - Proklik na Search s předvyplněným indexem

### Shards
- [ ] Summary (horní blok)
  - Celkový počet shardů
  - Breakdown podle stavů (active/initializing/relocating/unassigned)
- [ ] Detail (dolní blok)
  - Tabulka/Grid view shardů
  - Filtry (podle indexu, nodu, stavu)
  - Lepší vizualizace než ElasticVue ⭐

### Search
- [ ] Výběr indexů (multi-select)
- [ ] Switch mezi Query DSL / SQL ⭐
- [ ] Query DSL editor (textarea s JSON)
- [ ] SQL editor (textarea)
- [ ] Execute button
- [ ] Výsledky
  - Tabulkový view
  - JSON view (raw)
  - Pagination
  - Export (CSV, JSON)
- [ ] Bookmark/Save query ⭐

### Templates
- [ ] Seznam index templates ⭐
- [ ] Detail template
- [ ] Seznam component templates (ES 7.8+) ⭐
- [ ] Detail component template
- [ ] Diff view pro porovnání templates ⭐

### Saved Queries
- [ ] Seznam uložených queries
- [ ] Spuštění uložené query
- [ ] Smazání query
- [ ] Editace query

## 📋 TODO - Features

### Auto-refresh
- [ ] Checkbox v navbar
- [ ] Select pro interval (1s, 5s, 30s, 1min, 5min)
- [ ] SSE implementace pro real-time updates

### Bulk operations
- [ ] Reindex multiple indexes
- [ ] Snapshot multiple indexes

### UI/UX
- [ ] Dark mode toggle ⭐
- [ ] Keyboard shortcuts
- [ ] Toast notifications (success/error messages)

## 🧪 Testing

- [ ] Otestovat na macOS
- [ ] Otestovat na Linux
- [ ] Otestovat na Windows
- [ ] Otestovat s ES 3.x, 5.x, 6.x, 7.x, 8.x
- [ ] Otestovat keychain fallback při selhání
- [ ] Unit testy
- [ ] Integration testy

## 📝 Poznámky

### Technický stack
- **Backend**: axum 0.8, tokio, sqlx (SQLite)
- **Frontend**: HTMX, Server-Sent Events, vanilla JS
- **Templates**: Askama
- **HTTP Client**: reqwest
- **Security**: keyring (OS native credential storage)

### Struktura souborů
```
elastic-explorer/
├── Cargo.toml
├── REQUIREMENTS.md
├── PROGRESS.md
├── src/
│   ├── main.rs
│   ├── config/mod.rs
│   ├── db/
│   │   ├── mod.rs
│   │   ├── models.rs
│   │   └── credentials.rs
│   ├── es/
│   │   ├── mod.rs
│   │   ├── client.rs
│   │   └── api.rs
│   ├── handlers/mod.rs
│   ├── models/mod.rs
│   ├── templates/
│   └── utils/
│       ├── mod.rs
│       └── browser.rs
├── static/
└── migrations/
    └── 001_init.sql
```

### Vylepšení oproti ElasticVue
1. ✅ **Regexp filtry** v indexech (připraveno v API)
2. 🚧 **Lepší vizualizace shardů** (TODO)
3. ✅ **SQL API** pro search (připraveno v API)
4. ✅ **Native aplikace** (Rust CLI)
5. ✅ **OS keychain** pro hesla
6. 🚧 **Index/Component templates** (připraveno v API, UI TODO)
7. 🚧 **Template diff view** (TODO)

## 🚧 Rozpracováno

*Žádné rozpracované úkoly*

## 📋 TODO - Web UI (další kroky)

### Dashboard - živá data
- [ ] Integrace s ES API pro cluster metriky
- [ ] Session/Cookie pro aktivní endpoint
- [ ] Realtime refresh metrik
- [ ] Sparkline grafy
- [ ] Seznam nodů s detaily

---

**Poslední aktualizace**: 2026-01-12 20:22 CET
