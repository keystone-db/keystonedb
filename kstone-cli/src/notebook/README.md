# KeystoneDB Notebook Interface

**Interactive, web-based database exploration for KeystoneDB** - Think Jupyter, but for your database.

## Overview

The KeystoneDB Notebook provides a browser-based interactive interface for exploring and querying your database. Execute PartiQL queries, visualize results, and document your work all in one place.

## Features

### 📓 Notebook Cells
- **Code Cells**: Execute PartiQL/SQL queries interactively
- **Markdown Cells**: Document your queries and findings
- **Rich Output**: Tables, JSON, and formatted results

### 🔄 Real-Time Updates
- **WebSocket Connection**: Instant query execution feedback
- **Live Results**: See query results as they stream in
- **Progress Indicators**: Visual feedback for long-running queries

### 💾 Persistent Storage
- **Saved in Database**: Notebooks stored directly in your KeystoneDB instance
- **Auto-Save**: Changes saved automatically
- **Version History**: Track notebook revisions (coming soon)

### 🎨 User Interface
- **Clean Design**: Minimal, distraction-free interface
- **Keyboard Shortcuts**: Vim-style and standard shortcuts
- **Multiple Formats**: View results as tables, JSON, or CSV

## Quick Start

### Launch Notebook Server

```bash
# Start notebook for existing database
kstone notebook mydb.keystone

# Start with in-memory database
kstone notebook :memory:

# Specify custom port
kstone notebook mydb.keystone --port 9000

# Read-only mode
kstone notebook mydb.keystone --read-only

# Don't auto-open browser
kstone notebook mydb.keystone --no-browser
```

The server will start at `http://localhost:8080` (default) and automatically open your browser.

### Create Your First Notebook

1. Click "New Notebook" in the interface
2. Enter a title (e.g., "Data Exploration")
3. Start adding cells

### Execute Queries

**Code Cell** - Execute PartiQL:
```sql
SELECT * FROM users WHERE age > 25 LIMIT 10
```

**Markdown Cell** - Add documentation:
```markdown
# User Analysis

This query finds all users over 25 years old.
```

## Notebook API

The notebook interface exposes a REST API for programmatic access:

### List Notebooks
```bash
GET /api/notebooks
```

Returns all notebooks in the database.

### Create Notebook
```bash
POST /api/notebooks
Content-Type: application/json

{
  "title": "My Analysis"
}
```

### Get Notebook
```bash
GET /api/notebooks/{id}
```

### Update Notebook
```bash
PUT /api/notebooks/{id}
Content-Type: application/json

{
  "title": "Updated Title",
  "cells": [...]
}
```

### Delete Notebook
```bash
DELETE /api/notebooks/{id}
```

### Execute Cell
```bash
POST /api/notebooks/{id}/cells/{cell_id}/execute
```

Executes a code cell and returns results via WebSocket.

## WebSocket Protocol

Connect to `ws://localhost:8080/ws` for real-time updates:

### Client → Server Messages

**Execute Query**:
```json
{
  "type": "execute",
  "notebook_id": "abc123",
  "cell_id": "cell-1",
  "code": "SELECT * FROM users LIMIT 5"
}
```

**Subscribe to Notebook**:
```json
{
  "type": "subscribe",
  "notebook_id": "abc123"
}
```

### Server → Client Messages

**Query Results**:
```json
{
  "type": "result",
  "cell_id": "cell-1",
  "output": {
    "type": "table",
    "columns": ["id", "name", "age"],
    "rows": [...]
  }
}
```

**Error**:
```json
{
  "type": "error",
  "cell_id": "cell-1",
  "message": "Parse error: invalid SQL syntax"
}
```

**Progress Update**:
```json
{
  "type": "progress",
  "cell_id": "cell-1",
  "scanned": 1000,
  "total": 10000
}
```

## Configuration

### NotebookConfig

```rust
use kstone_cli::notebook::{NotebookConfig, launch_notebook};

let config = NotebookConfig {
    host: "127.0.0.1".to_string(),
    port: 8080,
    read_only: false,
    auto_open_browser: true,
};

launch_notebook(db_path, config).await?;
```

### Environment Variables

- `KSTONE_NOTEBOOK_PORT` - Default port (default: 8080)
- `KSTONE_NOTEBOOK_HOST` - Bind address (default: 127.0.0.1)
- `KSTONE_NOTEBOOK_READ_ONLY` - Read-only mode (default: false)

## Storage Format

Notebooks are stored in the database using a special partition key prefix `__notebook#`:

```
PK: __notebook#{notebook_id}
SK: metadata

{
  "id": "abc123",
  "title": "My Notebook",
  "created_at": 1696000000,
  "updated_at": 1696001000,
  "version": 1
}
```

```
PK: __notebook#{notebook_id}
SK: cell#{cell_id}

{
  "id": "cell-1",
  "type": "code",
  "source": "SELECT * FROM users",
  "output": {...},
  "order": 0
}
```

## Keyboard Shortcuts

### Navigation
- `↑` / `↓` - Navigate between cells
- `Ctrl+Enter` - Execute current cell
- `Shift+Enter` - Execute and move to next cell
- `Alt+Enter` - Execute and insert cell below

### Editing
- `Enter` - Edit current cell
- `Esc` - Exit edit mode
- `Ctrl+S` - Save notebook

### Cell Operations
- `a` - Insert cell above
- `b` - Insert cell below
- `dd` - Delete cell (press twice)
- `m` - Change to markdown cell
- `y` - Change to code cell
- `c` - Copy cell
- `x` - Cut cell
- `v` - Paste cell

## Architecture

```
┌─────────────┐
│   Browser   │
│  (React UI) │
└──────┬──────┘
       │ HTTP/WebSocket
┌──────▼──────────────┐
│  Notebook Server    │
│  (Axum + WebSocket) │
├─────────────────────┤
│  Handlers           │
│  - REST API         │
│  - WebSocket        │
│  - Query Execution  │
└──────┬──────────────┘
       │
┌──────▼──────────┐
│  KeystoneDB     │
│  - Query Engine │
│  - Storage      │
└─────────────────┘
```

### Components

- **server.rs** - HTTP/WebSocket server (Axum)
- **handlers.rs** - REST API endpoints
- **websocket.rs** - WebSocket message handling
- **storage.rs** - Notebook persistence
- **assets.rs** - Static file serving
- **static/** - Frontend assets (HTML/CSS/JS)

## Examples

### Data Analysis Notebook

```sql
-- Cell 1: Count users by age group
SELECT
  CASE
    WHEN age < 18 THEN 'under_18'
    WHEN age < 30 THEN '18_29'
    WHEN age < 50 THEN '30_49'
    ELSE '50_plus'
  END as age_group,
  COUNT(*) as count
FROM users
GROUP BY age_group
ORDER BY age_group;
```

```sql
-- Cell 2: Top active users
SELECT name, email, last_login, activity_score
FROM users
WHERE active = true
ORDER BY activity_score DESC
LIMIT 10;
```

### Schema Exploration

```sql
-- View all partition keys
SELECT DISTINCT pk FROM __all__ LIMIT 100;
```

```sql
-- Count items by partition
SELECT pk, COUNT(*) as item_count
FROM __all__
GROUP BY pk
ORDER BY item_count DESC
LIMIT 20;
```

## Security Considerations

### Read-Only Mode
Launch in read-only mode to prevent data modifications:
```bash
kstone notebook mydb.keystone --read-only
```

In read-only mode:
- ✅ SELECT queries allowed
- ❌ INSERT/UPDATE/DELETE blocked
- ❌ Notebook creation blocked
- ✅ Notebook viewing allowed

### Network Access
By default, the server binds to `127.0.0.1` (localhost only). To allow remote access:

```bash
# ⚠️ WARNING: Only use in trusted networks
kstone notebook mydb.keystone --host 0.0.0.0
```

**Never expose the notebook server to the internet without authentication!**

## Troubleshooting

### Port Already in Use
```bash
# Try a different port
kstone notebook mydb.keystone --port 9000
```

### Browser Doesn't Open
```bash
# Disable auto-open and manually visit URL
kstone notebook mydb.keystone --no-browser
# Then open: http://localhost:8080
```

### WebSocket Connection Failed
- Check firewall settings
- Ensure port is not blocked
- Try disabling browser extensions
- Check browser console for errors

### Notebook Not Saving
- Check database permissions
- Ensure disk space available
- Verify database is not corrupted

## Development

### Building the Frontend

```bash
cd kstone-cli/src/notebook/static
npm install
npm run build
```

### Running in Development Mode

```bash
# Terminal 1: Start notebook server
cargo run -p kstone-cli notebook mydb.keystone

# Terminal 2: Start frontend dev server
cd kstone-cli/src/notebook/static
npm run dev
```

### Adding New Cell Types

1. Update `CellType` enum in `storage.rs`
2. Add handler in `handlers.rs`
3. Update frontend to support new type
4. Add tests

## Future Enhancements

- [ ] **Chart Visualizations** - Built-in charting (line, bar, pie)
- [ ] **Export Notebooks** - Export to PDF, HTML, Jupyter .ipynb
- [ ] **Collaborative Editing** - Multiple users editing simultaneously
- [ ] **Variable Interpolation** - Use results from one cell in another
- [ ] **Scheduled Execution** - Run notebooks on schedule
- [ ] **Authentication** - Multi-user support with permissions
- [ ] **Notebook Templates** - Pre-built analysis templates
- [ ] **AI Assistant** - Query suggestions and optimization

## Contributing

Contributions welcome! Areas for improvement:

1. **UI/UX** - Improve frontend design
2. **Cell Types** - Add new output formats (charts, graphs)
3. **Performance** - Optimize large result rendering
4. **Features** - Variable storage, notebook sharing
5. **Tests** - Increase test coverage

## License

Same as KeystoneDB: MIT OR Apache-2.0

## See Also

- [KeystoneDB README](../../../README.md)
- [PartiQL Documentation](../../../docs/partiql.md)
- [CLI Documentation](../../../docs/cli.md)
