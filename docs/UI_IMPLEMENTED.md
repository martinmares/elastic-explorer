# UI Implementation Details

## Layout Structure

### Base Template (`base.html`)
- Responsive navbar with endpoint selector
- Dark/Light theme toggle with persistence
- Collapsible sidebar navigation
- Main content area with page header
- Footer with version info

### Navigation Menu
1. **Endpoints** - Manage Elasticsearch connections
2. **Dashboard** - Cluster overview and health
3. **Indices** - Index management and operations
4. **Shards** - Shard distribution visualization
5. **Search** - Query execution (DSL/SQL)
6. **Dev Console** - Interactive API explorer
7. **Templates** (dropdown):
   - Index Templates
   - Component Templates
8. **Saved Queries** - Query bookmarks

## Page Implementations

### Endpoints Page
- **List view**: All saved endpoints with test/delete actions
- **Add form**: Name, URL, username, password fields
- **Insecure checkbox**: For self-signed certificates
- **Test connection**: Validates endpoint before saving
- **Active indicator**: Shows currently selected endpoint

### Dashboard
- **Cluster cards**: Health, nodes, indices, documents
- **Status indicators**: Color-coded (green/yellow/red)
- **Node list**: Sortable table with roles and metrics
- **Master badge**: Highlights master-eligible nodes
- **Click-through**: Node names link to detail page
- **Auto-refresh**: Optional with configurable interval (future)

### Indices
- **Filter bar**: Text search and regex pattern matching
- **Health filter**: Dropdown for green/yellow/red
- **Sort controls**: Click column headers to sort
- **Bulk actions toolbar**: Appears when items selected
- **Pagination**: Page size selector and page navigation
- **Index cards**: Name, health, docs, shards, size
- **Actions menu**: View detail, close, delete (with confirmation)

### Index Detail
- **Tabs**: Overview, Mapping, Settings, Aliases
- **Overview**: Stats cards with metrics
- **Mapping**: JSON viewer with syntax highlighting
- **Settings**: JSON viewer (editable in future)
- **Aliases**: List with actions

### Search
- **Index selector**: Dropdown with autocomplete
- **Query editor**: Textarea with JSON validation
- **Execute button**: Runs search query
- **Results table**: Paginated with document viewer
- **Document modal**: JSON detail with copy button
- **Bulk delete**: Multi-select with confirmation

### Dev Console
- **Two-column layout**: Request editor (left) and response viewer (right)
- **Method selector**: Dropdown for GET/POST/PUT/DELETE/HEAD
- **Path input**: With helpful placeholder examples
- **Body editor**: Textarea with JSON placeholder
- **Execute button**: Runs API request
- **Response area**: Auto-detects JSON vs plain text
- **Status badge**: Color-coded by HTTP status
- **Copy button**: Copies response to clipboard
- **History table**: Past requests with status codes
- **Load from history**: Click row to reload request
- **Confirmation modal**: For DELETE/PUT operations

### Shards
- **Visual grid**: Matrix of indices × nodes
- **Color coding**: Shards colored by status
- **Filter controls**: Index pattern (regex), node, status
- **Shard modal**: Click shard for detailed information
- **Unassigned list**: Separate section for unassigned shards
- **Tooltips**: Hover for quick shard info

## UI Components

### Modals
- **Confirmation dialogs**: For destructive operations
- **Detail viewers**: For indices, nodes, documents
- **Form dialogs**: For creating/editing entities

### Tables
- **Sortable**: Click headers to sort
- **Selectable**: Checkboxes for bulk operations
- **Paginated**: With page size control
- **Responsive**: Scrollable on mobile

### Cards
- **Stat cards**: For metrics display
- **Entity cards**: For indices, nodes, etc.
- **Action cards**: With buttons/menus

### Forms
- **Validation**: Client-side validation
- **Error messages**: Clear error display
- **Help text**: Contextual hints
- **Placeholders**: Example values

## Theming

### Color Scheme
- **Success**: Green (#2fb344)
- **Warning**: Orange/Yellow (#f59f00)
- **Danger**: Red (#d63939)
- **Info**: Blue (#0054a6)
- **Secondary**: Gray (#667382)

### Dark Mode Support
- Auto-detection based on OS preference
- Manual toggle with persistence
- All components support both themes
- Consistent color contrast ratios

## Responsive Design

### Breakpoints
- **Mobile**: < 768px (single column layouts)
- **Tablet**: 768px - 1024px (adapted layouts)
- **Desktop**: > 1024px (full layouts)

### Mobile Optimizations
- Collapsible navigation
- Touch-friendly buttons
- Horizontal scroll for tables
- Simplified card layouts

## HTMX Integration

### Dynamic Updates
- **Partial page updates**: Without full page reload
- **Loading indicators**: During async operations
- **Error handling**: User-friendly error messages

### Use Cases
- Index table updates after filter changes
- Search results pagination
- Bulk operation progress
- Real-time metrics updates (future)

## Accessibility

### Keyboard Navigation
- Tab order follows logical flow
- Enter key submits forms
- Escape key closes modals

### Screen Reader Support
- ARIA labels on interactive elements
- Semantic HTML structure
- Alt text for icons

### Visual Accessibility
- High contrast ratios
- Readable font sizes
- Clear focus indicators
- Color not sole indicator

## Performance Optimizations

### Client-Side
- Minimal JavaScript
- CSS loaded once
- Icon fonts cached
- Lazy loading for images

### Server-Side
- Compiled templates (Askama)
- Efficient database queries
- Connection pooling
- Response compression

---

Last updated: 2026-01-14
