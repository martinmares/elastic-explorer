# Plánované vylepšení Search stránky

## 1. ✅ Spinner při načítání výsledků
- Přepsat formulář na HTMX submit
- Zobrazit spinner během načítání
- Výsledky se zobrazí až po dokončení

## 2. ⏳ Náhled JSON v tabulce
- Místo `<details>` zobrazit jen krátký náhled (prvních ~100 znaků jako čistý text)
- Přidat "..." na konec náhledu
- Přidat ikonu/tlačítko pro otevření detailu

## 3. ⏳ Modal s plným JSON
- Otevře se po kliknutí na ikonu detailu
- Pretty-printed JSON (formátovaný)
- Tlačítko "Copy to clipboard" pro kopírování JSON

## 4. ✅ Checkboxy pro výběr dokumentů
- Select-all checkbox v hlavičce tabulky
- Checkbox u každého dokumentu (index + document ID)
- Bulk actions toolbar (zobrazí se když jsou vybrané dokumenty)

## 5. ⏳ Bulk Delete dokumentů
- Endpoint: DELETE /{index}/_doc/{id}
- Modal s potvrzením
- Progress bar během mazání
- Výsledky s úspěch/chyba pro každý dokument

## 6. ⏳ Export do ZIP
- Vybrané dokumenty se uloží jako JSON soubory do ZIP
- Název ZIP: `{index_pattern}_{timestamp}.zip`
- Automatický download v browseru
- Každý dokument: `{index}_{document_id}.json`

---

## Poznámky k implementaci

### Spinner
- Použít HTMX `hx-post` nebo `hx-get` na formuláři
- Target: `#search-results`
- Indicator: spinner div s `htmx-indicator` class

### JSON náhled
- Funkce v Rust: `fn json_preview(json: &Value) -> String`
- Převést JSON na text, vzít prvních 100 znaků
- Ošetřit Unicode správně

### Modal JSON
- Template: `document_detail.html` (nebo inline v search.html)
- JavaScript funkce: `openDocumentDetail(index, id, json)`
- Copy to clipboard: `navigator.clipboard.writeText()`

### Bulk operace
- Podobná struktura jako u indices
- Track selection: `Map<string, {index, id}>`
- Delete endpoint: `POST /search/bulk/delete` s body: `[{index, id}, ...]`
- Export endpoint: `POST /search/bulk/export` s body: `[{index, id, source}, ...]`
