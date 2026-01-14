# Search Feature - Future Improvements

This document outlines potential improvements to the search functionality.

## Current Implementation

### Features
- Index pattern selector (with wildcard support)
- Query string search
- Pagination (10/20/50/100 results per page)
- Document detail viewer
- Copy JSON to clipboard
- Bulk delete with confirmation

### Limitations
- Only supports simple query string syntax
- No DSL query builder
- No SQL support
- No result export
- No query saving

## Planned Improvements

### 1. Query Type Selector

**Current**: Only query string
**Improved**: Multiple query types

```
┌─────────────────────────────────────┐
│ Query Type: [DSL ▼] [SQL] [String] │
└─────────────────────────────────────┘
```

### 2. DSL Query Editor

**Features**:
- JSON editor with syntax highlighting
- Auto-completion for field names
- Query validation before execution
- Common query templates (dropdown)
  - Match all
  - Term query
  - Range query
  - Bool query with multiple clauses
  - Aggregations

**Example Templates**:
```json
{
  "query": {
    "match_all": {}
  }
}
```

```json
{
  "query": {
    "bool": {
      "must": [
        { "match": { "field": "value" } }
      ],
      "filter": [
        { "range": { "timestamp": { "gte": "now-1d" } } }
      ]
    }
  }
}
```

### 3. SQL Query Support (ES 7.x+)

**Features**:
- SQL editor with syntax highlighting
- Query validation
- Result translation to tabular format
- Common SQL templates

**Example**:
```sql
SELECT * FROM my-index
WHERE timestamp > NOW() - INTERVAL 1 DAY
LIMIT 100
```

### 4. Saved Queries

**Features**:
- Save query with name and description
- Tag/categorize queries
- Quick load from dropdown
- Share queries (export/import JSON)
- Edit saved queries
- Delete with confirmation

**UI**:
```
┌─────────────────────────────────────┐
│ Saved Queries                    [+]│
├─────────────────────────────────────┤
│ ○ Recent 5xx errors                 │
│ ○ High CPU nodes                    │
│ ○ Large documents                   │
└─────────────────────────────────────┘
```

### 5. Result Export

**Formats**:
- JSON (pretty-printed or compact)
- CSV (with field selection)
- Excel (XLSX)
- Copy to clipboard (formatted)

**Options**:
- Export current page
- Export all results (with limit)
- Select fields to export
- Custom field ordering

### 6. Advanced Filtering

**Features**:
- Filter results by field values (client-side)
- Column visibility toggle
- Custom column order
- Field-based sorting (multiple fields)

### 7. Query History

**Features**:
- Automatic history per endpoint
- Timestamp and result count
- Re-run previous query with one click
- Clear history option
- History export

**Storage**:
- SQLite table: `search_history`
  - endpoint_id
  - query_type (string/dsl/sql)
  - query_body
  - index_pattern
  - result_count
  - executed_at

### 8. Field Explorer

**Features**:
- Browse index fields with types
- Field statistics (cardinality, null count)
- Click field to add to query
- Field search/filter

**UI**:
```
┌─────────────────────────────────────┐
│ Fields                          [×] │
├─────────────────────────────────────┤
│ 🔍 [search fields...]               │
├─────────────────────────────────────┤
│ ▼ _id (keyword)                     │
│ ▼ timestamp (date)                  │
│ ▼ user                              │
│   ├─ name (text)                    │
│   └─ email (keyword)                │
└─────────────────────────────────────┘
```

### 9. Query Builder (Visual)

**For non-technical users**:
- Drag-and-drop query builder
- Visual bool query composition
- Field selector with autocomplete
- Operator selection (equals, contains, range, etc.)
- Real-time query preview (shows generated DSL)

**Example**:
```
┌─────────────────────────────────────┐
│ Field: [timestamp ▼]                │
│ Operator: [greater than ▼]         │
│ Value: [2024-01-01]                 │
│                           [+ Add]   │
└─────────────────────────────────────┘
```

### 10. Result Visualization

**Features**:
- Automatic chart generation for aggregations
- Bar charts for terms aggregations
- Line charts for date histograms
- Pie charts for distribution
- Export charts as images

## Implementation Priority

### Phase 1 (High Priority)
1. Saved queries
2. DSL query editor
3. Result export (JSON, CSV)
4. Query history

### Phase 2 (Medium Priority)
5. SQL query support
6. Field explorer
7. Advanced filtering
8. Query templates

### Phase 3 (Low Priority)
9. Visual query builder
10. Result visualization
11. Query sharing
12. Collaborative features

## Technical Considerations

### Storage
- Use SQLite for saved queries and history
- Consider query size limits
- Implement cleanup policy (e.g., keep last 1000 queries)

### Performance
- Limit result export size (warn for large exports)
- Use streaming for large result sets
- Implement query timeout
- Show progress for long-running queries

### Security
- Validate all queries before execution
- Prevent dangerous operations (delete_by_query without confirmation)
- Sanitize user input
- Rate limiting for query execution

### UI/UX
- Keyboard shortcuts for common operations
- Query snippets/autocomplete
- Error messages with helpful hints
- Loading indicators
- Progress tracking for bulk operations

---

Last updated: 2026-01-14
