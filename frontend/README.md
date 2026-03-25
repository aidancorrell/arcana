# Arcana Frontend — Interactive 3D Knowledge Graph

Interactive demo that visualizes Arcana's metadata catalog as a 3D knowledge graph. Tables are positioned in a DAG funnel layout by dbt layer (sources → staging → intermediate → marts), color-coded, and connected by lineage edges.

## Quick Start

```bash
# Extract data from SQLite (one-time, or after re-syncing)
python3 scripts/extract.py

# Install and run
npm install
npm run dev
```

Open `http://localhost:5173`.

## Live Mode

For real-time updates when MCP tools modify the database:

```bash
# Terminal 1: API server
python3 scripts/api_server.py

# Terminal 2: Dev server
npm run dev
```

The frontend polls the API every 3 seconds. When `update_context` or `report_outcome` changes the database, the graph updates automatically. Falls back to static JSON files if the API server isn't running.
