# Arcana Sandbox — Local Testing

This directory is a gitignored workspace for testing Arcana end-to-end against real dbt projects. Only this README and the setup script are committed.

## Quick Start (jaffle shop)

### 1. Run the setup script

```bash
cd sandbox/
./setup.sh
```

This will:
- Clone [jaffle-shop](https://github.com/dbt-labs/jaffle-shop) into `sandbox/jaffle-shop/`
- Generate `target/manifest.json` via `dbt parse`
- Create a pre-filled `arcana.toml` pointing at the jaffle shop project
- Initialize the SQLite store (`arcana.db`)

### 2. Set your OpenAI API key

```bash
export OPENAI_API_KEY="sk-..."
```

Or edit `sandbox/arcana.toml` and fill in the `openai_api_key` field.

### 3. Sync dbt metadata

```bash
# From repo root:
cargo run --bin arcana -- --config sandbox/arcana.toml sync --adapter dbt
```

### 4. Check what landed

```bash
cargo run --bin arcana -- --config sandbox/arcana.toml status
```

You should see jaffle shop's tables, columns, and lineage edges.

### 5. Embed and query

```bash
cargo run --bin arcana -- --config sandbox/arcana.toml reembed
cargo run --bin arcana -- --config sandbox/arcana.toml ask "total revenue by customer"
```

Does it return `orders`, `order_items`, `customers` with relevant columns? That's the thesis.

### 6. Wire to Claude Desktop

```bash
cargo run --bin arcana -- --config sandbox/arcana.toml serve --stdio
```

Claude Desktop config (`~/Library/Application Support/Claude/claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "arcana": {
      "command": "/path/to/arcana/target/debug/arcana",
      "args": ["--config", "/path/to/arcana/sandbox/arcana.toml", "serve", "--stdio"],
      "env": {
        "OPENAI_API_KEY": "sk-..."
      }
    }
  }
}
```

### 7. Test SQL accuracy

Ask Claude these queries with and without Arcana:

| Question | Expected tables |
|----------|----------------|
| "Total revenue by customer" | `orders`, `order_items`, `customers` |
| "Most popular product category" | `order_items`, `products` |
| "Average order value over time" | `orders` |
| "Customer lifetime value" | `orders`, `customers` |
| "Supply cost breakdown" | `supplies`, `products` |

---

## Using your own dbt project

Edit `sandbox/arcana.toml` and change `[dbt] project_path` to point at your project. Make sure you've run `dbt parse` (and optionally `dbt docs generate`) so `target/manifest.json` exists.

## What's in this directory

```
sandbox/
  README.md          # This file (committed)
  setup.sh           # Bootstrap script (committed)
  arcana.toml        # Local config (gitignored)
  arcana.db          # SQLite store (gitignored)
  jaffle-shop/       # Cloned dbt project (gitignored)
```
