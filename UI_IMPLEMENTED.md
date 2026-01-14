# Web UI - Implementované funkce

## ✅ Co je hotové (2026-01-12)

### 1. Layout a Design
- **Tabler CSS framework** - moderní admin UI
- **Dark mode** - přepínač světlý/tmavý/auto režim
- **Responsive design** - funguje na desktop i mobil
- **HTMX** - pro dynamické aktualizace bez reloadů

### 2. Navigace
**Hlavní menu (horizontal navbar):**
- Dashboard
- Indexy
- Nodes
- Shards
- Search
- Templates (dropdown: Index Templates, Component Templates)
- Uložené Queries

**Top navbar:**
- Logo a název aplikace
- Endpoint selector (dropdown s možností přepnout aktivní endpoint)
- Dark mode toggle
- Link na správu endpointů

### 3. Endpoints Management (100% hotovo)

#### Seznam endpointů (`/endpoints`)
- Zobrazí všechny uložené ES endpointy
- Prázdný stav (empty state) pokud nejsou žádné

#### Endpoint card zobrazuje:
- Název endpointu
- URL
- Badge "Insecure" (pokud je self-signed cert)
- Badge s username (pokud je Basic Auth)

#### Akce pro každý endpoint:
- **Test připojení** (připraveno pro implementaci)
- **Použít endpoint** (select jako aktivní)
- **Editovat** (připraveno pro implementaci)
- **Smazat** - s konfirmačním dialogem ⭐

#### Přidání nového endpointu (modal):
- Název (povinné)
- URL (povinné, validace URL)
- Checkbox: "Povolit self-signed certifikáty (insecure)"
- **Basic Authentication** sekce:
  - Username
  - Password
  - Info o keychain zabezpečení

#### HTMX funkce:
- Vytvoření endpointu bez reload stránky
- Smazání endpointu s dynamickou aktualizací seznamu
- Automatické zavření modalu po úspěchu

#### Bezpečnost:
- **Konfirmační dialog** před smazáním (ANO/NE) ⭐
- Hesla ukládána do OS keychain (ne v plain textu)

### 4. Dashboard (základní kostra)

#### Když není vybrán endpoint:
- Empty state s výzvou k přidání/výběru endpointu
- Tlačítko "Spravovat endpointy"

#### Když je vybrán endpoint (placeholder):
- 4 metrické karty (Cluster Status, Nodes, Indices, Documents)
- Sekce pro seznam nodů
- Info alert o dokončení integrace

## 🎨 Design prvky

### Tabler komponenty použité:
- Cards (karty)
- Modals (modální okna)
- Forms (formuláře)
- Buttons (tlačítka)
- Badges (štítky)
- Empty states (prázdné stavy)
- Alerts (upozornění)
- Dropdown menus
- Icons (Tabler Icons)

### Barvy a stavy:
- **Green** - zdravý stav (cluster green)
- **Yellow** - warning (cluster yellow)
- **Red** - kritický stav (cluster red)
- **Blue** - informace (Basic Auth badge)

## 🚀 Jak to používat

### 1. Spuštění
```bash
cargo run
```
Server běží na `http://127.0.0.1:8080` a automaticky otevře prohlížeč.

### 2. První kroky
1. Server přesměruje na Dashboard
2. Dashboard zobrazí empty state (žádný endpoint)
3. Klikni "Spravovat endpointy" nebo jdi na `/endpoints`
4. Klikni "Přidat endpoint"
5. Vyplň formulář:
   - Název: např. "Production"
   - URL: např. "https://elasticsearch.example.com:9200"
   - Případně username + password
6. Klikni "Uložit endpoint"
7. Endpoint se objeví v seznamu

### 3. Přepnutí dark mode
- Klikni na ikonu slunce v pravém horním rohu
- Vyber světlý/tmavý/auto režim
- Nastavení se uloží do localStorage

## 📸 Popisy stránek

### `/` (root)
Redirect na `/dashboard`

### `/dashboard`
- Empty state pokud není vybrán endpoint
- Jinak zobrazí cluster overview (prozatím placeholder)

### `/endpoints`
- Seznam všech endpointů
- Tlačítko "Přidat endpoint"
- CRUD operace s HTMX

### `/health`
Health check endpoint (vrací "OK")

## 🔧 Technické detaily

### HTMX použití:
```html
<!-- POST na vytvoření endpointu -->
<form hx-post="/endpoints" hx-target="#endpoints-list" hx-swap="innerHTML">

<!-- DELETE na smazání endpointu -->
htmx.ajax('DELETE', `/endpoints/${id}`, { target: '#endpoints-list', swap: 'innerHTML' });
```

### Askama templates:
- `base.html` - hlavní layout
- `endpoints.html` - správa endpointů
- `dashboard.html` - dashboard

### Handlers:
- `GET /endpoints` → `list_endpoints()` → vrací HTML s Askama
- `POST /endpoints` → `create_endpoint()` → vrací HTML fragment pro HTMX
- `DELETE /endpoints/{id}` → `delete_endpoint()` → vrací HTML fragment

## 📋 Co zbývá implementovat

### Dashboard - živá data:
- [ ] Integrace s ES API
- [ ] Session management pro aktivní endpoint
- [ ] Realtime metriky (SSE)
- [ ] Sparkline grafy

### Endpoints:
- [ ] Test connection funkce
- [ ] Select endpoint (nastavit jako aktivní)
- [ ] Edit endpoint

### Další stránky:
- [ ] Indexy (`/indices`)
- [ ] Nodes (`/nodes`)
- [ ] Shards (`/shards`)
- [ ] Search (`/search`)
- [ ] Templates (`/templates/index`, `/templates/component`)
- [ ] Saved Queries (`/saved-queries`)

## 💡 Výhody současné implementace

1. **Zero build process** - jen CDN pro Tabler a HTMX
2. **Server-side rendering** - rychlé načítání
3. **Progressive enhancement** - funguje i bez JS
4. **Responsive** - funguje na všech zařízeních
5. **Dark mode** - nativní podpora
6. **Bezpečné** - hesla v OS keychain
7. **Type-safe** - Askama templates s compile-time checking

---

Vytvořeno: 2026-01-12
