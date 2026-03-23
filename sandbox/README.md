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

## Scale Test (GitLab Analytics — 3,400+ models)

GitLab's production dbt project is public and makes an excellent stress test for Arcana.

### 1. Run the setup script

```bash
cd sandbox/
./setup-gitlab.sh
```

This will:
- Clone [gitlab-data/analytics](https://gitlab.com/gitlab-data/analytics) (shallow)
- Patch out private package dependencies
- Generate `target/manifest.json` via `dbt parse` with stub profiles (~2 min)
- Create `arcana-gitlab.toml` and initialize `arcana-gitlab.db`

**Stats:** 3,473 models (2,577 with descriptions), 28,535 columns (19,312 with descriptions), 1,467 sources.

### 2. Sync, enrich, embed

```bash
export OPENAI_API_KEY="sk-..."
export ANTHROPIC_API_KEY="sk-ant-..."

cargo run --bin arcana -- --config sandbox/arcana-gitlab.toml sync --adapter dbt
cargo run --bin arcana -- --config sandbox/arcana-gitlab.toml status --detailed
cargo run --bin arcana -- --config sandbox/arcana-gitlab.toml enrich --dry-run
cargo run --bin arcana -- --config sandbox/arcana-gitlab.toml reembed
```

### 3. Test queries

| Question | Expected domain |
|----------|----------------|
| "monthly recurring revenue by product tier" | ARR/subscription models |
| "customer churn rate" | retention, subscription lifecycle |
| "CI pipeline duration trends" | gitlab_dotcom CI tables |
| "merge request cycle time" | gitlab_dotcom MR models |
| "namespace storage usage" | product usage models |

---

## Using your own dbt project

Edit `sandbox/arcana.toml` and change `[dbt] project_path` to point at your project. Make sure you've run `dbt parse` (and optionally `dbt docs generate`) so `target/manifest.json` exists.

## What's in this directory

```
sandbox/
  README.md              # This file (committed)
  setup.sh               # Bootstrap: jaffle-shop (committed)
  setup-gitlab.sh        # Bootstrap: GitLab analytics (committed)
  arcana.toml            # Config for jaffle-shop (gitignored)
  arcana-gitlab.toml     # Config for GitLab (gitignored)
  arcana.db              # SQLite store — jaffle-shop (gitignored)
  arcana-gitlab.db       # SQLite store — GitLab (gitignored)
  jaffle-shop/           # Cloned dbt project (gitignored)
  gitlab-analytics/      # Cloned GitLab repo (gitignored)
```
