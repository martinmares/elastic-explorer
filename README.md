# Elastic Explorer

Elasticsearch cluster explorer napsaný v Rustu - jednodušší a přehlednější alternativa k ElasticVue.

## Status

🚧 **Projekt je v raném vývoji** 🚧

Momentálně je implementována základní infrastruktura:
- ✅ Backend server (Axum)
- ✅ SQLite databáze s OS keychain pro hesla
- ✅ Elasticsearch klient wrapper (podpora ES 3.x - 8.x)
- ⏳ Web UI (v přípravě)

## Rychlý start

### Build

```bash
cargo build --release
```

### Spuštění

```bash
# Základní spuštění (server na 127.0.0.1:8080)
cargo run

# Vlastní port
cargo run -- --port 3000

# Vlastní host
cargo run -- --host 0.0.0.0 --port 8080

# Neotvírat prohlížeč automaticky
cargo run -- --no-browser

# Help
cargo run -- --help
```

### Instalace

```bash
cargo install --path .
elastic-explorer
```

## Databáze a konfigurace

Aplikace vytvoří adresář pro data podle operačního systému:

- **macOS/Linux**: `~/.elastic-explorer/data/`
- **Windows**: `%APPDATA%\elastic-explorer\data\`

V tomto adresáři se nachází SQLite databáze s konfigurací endpointů.

### Zabezpečení hesel

Hesla pro Basic Auth jsou ukládána do nativního OS credential store:
- **macOS**: Keychain
- **Linux**: Secret Service API (GNOME Keyring, KWallet)
- **Windows**: Credential Manager

## Funkce (plánované)

### Dashboard
- Cluster health a metriky
- Realtime grafy (CPU, RAM, disk)
- Seznam nodů s rolemi
- Auto-refresh s konfigurovatelným intervalem

### Indexy
- Seznam s paginací
- **Regexp filtry** (vylepšení oproti ElasticVue)
- Multi-select operace
- Smazání s potvrzovacím dialogem
- Detail indexu (mapping, settings, stats)

### Nodes
- Seznam nodů
- Detail nodu s metrikami

### Shards
- Přehledná vizualizace (lepší než ElasticVue)
- Filtry podle indexu/nodu/stavu

### Search
- Query DSL editor
- **SQL API support** (ES 7.x+)
- Uložené queries (bookmarks)
- Export výsledků (JSON, CSV)

### Templates
- Index templates
- Component templates (ES 7.8+)
- **Diff view** pro porovnání

## Technologie

- **Backend**: axum 0.8, tokio
- **Database**: SQLite (sqlx), keyring
- **ES Client**: reqwest s custom wrapperem
- **Frontend**: HTMX, Server-Sent Events
- **Templates**: Askama

### ⚠️ Důležité poznámky pro vývoj

**Axum 0.8.x syntaxe:**
- Path parametry používají `{param}` místo `:param`
- Příklad: `.route("/indices/detail/{index_name}", get(handler))`
- **NE**: `.route("/indices/detail/:index_name", get(handler))`

## Dokumentace

- [REQUIREMENTS.md](REQUIREMENTS.md) - Detailní požadavky
- [PROGRESS.md](PROGRESS.md) - Průběh implementace

## Licence

MIT (bude doplněno)

## Autor

Vytvořeno v roce 2026
