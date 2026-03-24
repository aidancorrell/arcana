# Arcana

An agent-first data catalog that gives AI the context to write correct SQL on the first try.

Text-to-SQL accuracy jumps from ~20% to 92%+ when agents know which tables matter, what columns mean, and what business rules apply. Arcana builds a semantic knowledge layer over your data warehouse — indexing table schemas, column definitions, lineage graphs, and business documentation into a unified search index — then serves exactly the right context to any MCP-compatible agent (Claude Code, Cursor, Copilot, or any custom toolchain).

Written in Rust for single-binary deployment, sub-millisecond search latency, and zero runtime dependencies. Token-minimal by design — the recommender returns only the highest-relevance context within a configurable budget, so agents spend tokens on reasoning, not sifting through schema dumps. Adapter-based architecture means Arcana works with any warehouse (Snowflake, BigQuery, Databricks) and any modeling framework (dbt, LookML, raw DDL) — ship one adapter and the entire platform lights up. SQLite-backed for zero-ops local use; deploy as a shared HTTP/SSE server when the team is ready.

## Features

| Feature | Description |
|---------|-------------|
| **Hybrid search** | BM25 full-text + vector cosine similarity, fused via Reciprocal Rank Fusion |
| **Confidence scoring** | Every definition has a trust score that decays over time unless refreshed |
| **Lineage expansion** | Search results include upstream/downstream tables automatically |
| **Auto-enrichment** | LLM-generated descriptions for undocumented tables and columns |
| **Feedback loop** | Agents report query outcomes; successful definitions get boosted |
| **Redundancy detection** | Semantic clustering to surface duplicate tables |
| **Cost estimation** | Pre-execution Snowflake EXPLAIN-based cost estimates |
| **Document ingestion** | Index Markdown docs, runbooks, and data dictionaries |

## Quick Start

### Option 1: Build from source

```bash
git clone https://github.com/aidancorrell/arcana
cd arcana
cargo build --release
```

### Option 2: Docker

```bash
docker compose up -d
```

This starts Arcana with HTTP/SSE transport on port 8477 — multiple engineers can share one instance.

### Initialize and sync

```bash
# Create a minimal config (see below)
cp config/arcana.example.toml arcana.toml

# Initialize the metadata store
arcana --config arcana.toml init

# Sync from your dbt project
arcana --config arcana.toml sync --adapter dbt

# Embed all definitions for semantic search
arcana --config arcana.toml reembed

# Test it
arcana --config arcana.toml ask "monthly revenue by region"
```

### Connect to Claude Code

Add to your Claude Code MCP config (`~/.claude.json` or project settings):

```json
{
  "mcpServers": {
    "arcana": {
      "command": "/path/to/arcana",
      "args": ["--config", "/path/to/arcana.toml", "serve", "--stdio"]
    }
  }
}
```

## Minimal Configuration

```toml
[database]
url = "sqlite://arcana.db"

[embeddings]
provider = "openai"
openai_api_key = "sk-..."  # or set OPENAI_API_KEY env var
openai_model = "text-embedding-3-small"
dimensions = 1536

[dbt]
project_path = "./my-dbt-project"
```

That's it. Snowflake, enrichment, sync, and MCP sections are all optional. See [`config/arcana.example.toml`](config/arcana.example.toml) for the full reference.

## CLI Commands

```
arcana init         Initialize the metadata store
arcana sync         Sync metadata from dbt and/or Snowflake
arcana ingest       Ingest Markdown documents
arcana reembed      Embed (or re-embed) all semantic definitions
arcana enrich       Generate LLM descriptions for undocumented entities
arcana ask          Semantic search from the command line
arcana dedup        Detect redundant/duplicate tables
arcana status       Show entity counts and definition coverage
arcana serve        Start MCP server (--stdio for local, HTTP for team)
```

## MCP Tools

When connected as an MCP server, Arcana exposes:

| Tool | What it does |
|------|-------------|
| `get_context` | Semantic search — returns relevant tables, columns, and definitions for a query |
| `describe_table` | Full metadata for a specific table including columns, definitions, and lineage |
| `estimate_cost` | Pre-execution Snowflake cost estimate for a SQL query |
| `update_context` | Push corrections or new definitions back into the store |
| `find_similar_tables` | Find duplicate or near-duplicate tables via semantic similarity |
| `report_outcome` | Report query success/failure to adjust confidence scores |

## Architecture

Arcana is a Rust workspace with 6 crates:

```
arcana-core          Metadata store, entity types, embedding index, confidence system
arcana-adapters      Snowflake + dbt metadata adapters
arcana-documents     Document ingestion pipeline (Markdown)
arcana-recommender   Context ranker + token-budget-aware serializer
arcana-mcp           MCP server (stdio + HTTP/SSE transports)
arcana-cli           CLI binary wiring everything together
```

See [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) for the full design document.

## Development

```bash
cargo test --workspace    # 30+ tests, no API keys needed
cargo check --workspace
cargo fmt --all
cargo clippy --workspace
```

### Sandbox testing

```bash
cd sandbox && ./setup.sh  # Clones jaffle-shop, generates dbt artifacts, inits store
```

For scale testing with 3,400+ models, see [`sandbox/README.md`](sandbox/README.md).

## License

[MIT](LICENSE)
