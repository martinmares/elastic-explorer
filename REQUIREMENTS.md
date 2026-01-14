# Elastic Explorer - Požadavky

## Základní koncept
Elasticsearch prohlížeč v Rustu - jednodušší a přehlednější alternativa k ElasticVue.

## Spuštění
```bash
elastic-explorer
```
- Nastartuje HTTP server (default: `127.0.0.1:8080`)
- Automaticky otevře prohlížeč
  - macOS: `open http://127.0.0.1:8080`
  - Linux: `xdg-open http://127.0.0.1:8080`
  - Windows: `start http://127.0.0.1:8080`

## Datová vrstva

### Adresářová struktura
- **Linux/macOS**: `~/.elastic-explorer/data/`
- **Windows**: `%APPDATA%\elastic-explorer\data\`

### SQLite databáze
Ukládání konfigurace endpointů:

**Tabulka: endpoints**
- `id` - PRIMARY KEY
- `name` - název endpointu (např. "Production", "Local Dev")
- `url` - celé URL (scheme + hostname + port)
- `insecure` - boolean (self-signed certifikáty)
- `username` - pro Basic Auth (nullable)
- `password` - pro Basic Auth (nullable, šifrované?)
- `created_at` - timestamp
- `updated_at` - timestamp

## Web UI - Struktura

### Hlavní menu (navbar)
1. **Výběr endpointu** - dropdown s možností přepnutí mezi uloženými endpointy
2. **Editace endpointu** - tlačítko/ikona pro úpravu aktuálního endpointu
3. **Auto-refresh**
   - Checkbox pro zapnutí/vypnutí
   - Select/input pro interval: 1s, 5s, 30s, 1min, 5min

### Dashboard (hlavní stránka)

#### Cluster Overview
- **Verze Elasticsearch**
- **Využití disku** - celkem/volné místo
- **CPU utilizace** - per node
- **RAM utilizace** - per node
- **Realtime grafy** - sparkline charts pro metriky

#### Nodes
- **Seznam nodů** s informacemi:
  - Název nodu
  - Role (master, data, ingest, ml, ...)
  - Označení master nodu
  - CPU/RAM/Disk usage
  - **Proklik** na detail nodu

### Detail nodu
- **Summary** (horní blok)
  - Základní info: název, IP, verze, role
  - Aktuální metriky: CPU, RAM, Disk, JVM heap
- **Detail** (dolní blok)
  - Thread pools
  - File descriptors
  - Network stats
  - Plugins

### Indexy

#### Seznam indexů
- **Tabulka s paginací**
- **Filtry** (nad tabulkou):
  - Textový filtr (substring match)
  - **Regexp filtr** - možnost zadat regulární výraz ⭐
  - Filtr podle stavu (green/yellow/red)
  - Filtr podle velikosti
- **Sloupce**:
  - Název indexu (proklik na detail)
  - Health status
  - Počet dokumentů
  - Velikost (primární + repliky)
  - Počet shardů (primární/repliky)
- **Operace**:
  - Checkbox pro výběr více indexů
  - Smazání indexu/ů - **s konfirmačním dialogem (ANO/NE)**
  - Refresh, Flush, Force merge
  - Close/Open index

#### Detail indexu
- Mapping
- Settings
- Stats
- Proklik na **Search** (předvyplněný tento index)

### Shards

**Přehlednější vizualizace než ElasticVue!**

#### Summary (horní blok)
- Celkový počet shardů
- Stav: initializing, relocating, unassigned
- Breakdown podle stavů

#### Detail (dolní blok)
- **Tabulka/Grid view** shardů
  - Index name
  - Shard number
  - Typ (primary/replica)
  - State
  - Docs count
  - Size
  - Node umístění
- **Vizualizace** - např. grid/heatmapa podle nodů
- Filtry: podle indexu, podle nodu, podle stavu

### Search

#### Výběr indexů
- Multi-select dropdown pro výběr indexů
- Možnost vybrat všechny
- Proklik z detailu indexu = předvyplněný index

#### Query interface
- **Standard Query DSL** (JSON editor)
- **SQL API** - možnost psát SQL dotazy ⭐
  - ```sql
    SELECT * FROM my-index WHERE field = 'value'
    ```
- Switch mezi Query DSL / SQL
- Ukládání oblíbených queries (optional, do SQLite)

#### Výsledky
- Tabulkový view s nastavitelnými sloupci
- JSON view (raw documents)
- Paginace
- Export (CSV, JSON)

## Technický stack

### Backend
- **axum 0.8.x** - web framework
- **tokio** - async runtime
- **sqlx** - SQLite databáze
- **reqwest** - HTTP klient pro Elasticsearch API
- **serde/serde_json** - JSON serialization

### Frontend
- **HTMX** - dynamic HTML updates ⭐
- **Server-Sent Events (SSE)** - pro auto-refresh
- **Askama** - templating (pokud potřeba)
- **TailwindCSS/Bootstrap** - styling (optional)
- **Chart.js / Apache ECharts** - grafy a sparklines

### Cross-platform specifika
- **Adresáře**:
  - Linux/macOS: `~/.elastic-explorer/`
  - Windows: `%APPDATA%\elastic-explorer\`
- **Otevření prohlížeče**:
  ```rust
  #[cfg(target_os = "macos")]
  Command::new("open").arg(url).spawn()?;

  #[cfg(target_os = "linux")]
  Command::new("xdg-open").arg(url).spawn()?;

  #[cfg(target_os = "windows")]
  Command::new("cmd").args(&["/C", "start", url]).spawn()?;
  ```

## Vylepšení oproti ElasticVue

1. ✅ **Regexp filtry** v seznamu indexů
2. ✅ **Lepší vizualizace shardů**
3. ✅ **SQL API** pro search
4. ✅ **Přehlednější node detail**
5. ✅ **Konfirmační dialogy** pro destruktivní operace
6. ✅ **Realtime grafy** (sparklines)
7. ✅ **Native aplikace** - rychlejší než webová aplikace

## TODO / Nice to have
- [ ] Snapshot management
- [ ] Template management
- [ ] ILM policies viewer
- [ ] Cluster settings editor
- [ ] Export/import endpoint configurations
- [ ] Dark mode
- [ ] Keyboard shortcuts
- [ ] Query history (kromě oblíbených)
