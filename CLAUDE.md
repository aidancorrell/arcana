# Arcana — Implementation Guide

Agent-first data catalog. MCP is the primary interface. The thesis: text-to-SQL accuracy jumps from ~20% to 92%+ when an agent knows which tables matter, what columns mean, and what business rules apply.

Architecture doc: see `catalog-architecture-v2.md` (kept locally, not committed).

---

## Repo Layout

```
crates/
  arcana-core/              # Entity types, SQLite store, confidence system, embedding index
  arcana-adapters/          # Snowflake + dbt metadata adapters
  arcana-documents/         # Document ingestion pipeline (markdown → chunk → link → embed)
  arcana-recommender/       # Context ranker + serializer
  arcana-mcp/               # MCP server (get_context, describe_table, estimate_cost, update_context)
  arcana-cli/               # arcana init|sync|ingest|serve|ask|status|reembed
  arcana-integration-tests/ # MVP end-to-end test suite (101 tests)
config/
  arcana.example.toml
sandbox/
  README.md                 # Instructions for local end-to-end testing (committed)
  setup.sh                  # Bootstrap script: clones jaffle-shop, generates dbt artifacts, inits store (committed)
  # Everything else is gitignored: arcana.toml, arcana.db, jaffle-shop/, etc.
```

---

## MVP Scope

Ship the smallest thing that proves the thesis: **AI agents write better SQL when Arcana provides context.**

In scope:
- SQLite metadata store (fully implemented)
- Snowflake adapter: schema sync, column profiling, cost estimation
- dbt adapter: manifest.json + catalog.json + schema YAML parsing
- Context Recommender: embedding-based semantic search, confidence-weighted ranking
- Document ingestion: local Markdown files only
- Confidence system: scoring, decay, source tracking
- MCP server: `get_context`, `describe_table`, `estimate_cost`, `update_context`
- CLI: `arcana init`, `arcana sync`, `arcana ingest`, `arcana serve --stdio`, `arcana ask`

Out of scope for MVP:
- React frontend
- PostgreSQL backend
- BigQuery, Databricks adapters
- Confluence, Google Docs, Notion, Slack sources
- Usage-based collaborative filtering
- LLM-generated draft descriptions

---

## Implementation Plan

Work in this order. Each phase produces something runnable.

### Phase 1 — SQLite Store ✅ COMPLETE

All 28 `MetadataStore` methods implemented in `crates/arcana-core/src/store/sqlite.rs`. Runtime `sqlx::query()` + `.bind()` throughout. Integration tests cover roundtrip for all entity types.

### Phase 2 — dbt Adapter ✅ COMPLETE

`crates/arcana-adapters/src/dbt/` — `manifest.rs` (model/source/column/lineage extraction), `catalog.rs` (row counts, column types), `semantic.rs` (MetricFlow metrics), `mod.rs` (`DbtAdapter` implementing `MetadataAdapter` trait). All tested.

### Phase 3 — Snowflake Adapter ✅ COMPLETE

`crates/arcana-adapters/src/snowflake/` — `schema_sync.rs`, `cost.rs` (EXPLAIN-based estimation), `profiler.rs` (column stats), `usage.rs` (query history), `client.rs` (SQL API v2 auth + execution). All tested.

### Phase 4 — Embedding Index ✅ COMPLETE

`crates/arcana-core/src/embeddings/` — `provider.rs` (`EmbeddingProvider` trait), `openai.rs` (text-embedding-3-small), `index.rs` (in-memory brute-force cosine similarity). All tested.

### Phase 5 — Recommender ✅ COMPLETE

`crates/arcana-recommender/src/` — `ranker.rs` (`rank()` with confidence decay, combined scoring, dedup, top-k), `serializer.rs` (Markdown/JSON/Prose output), `feedback.rs` (`record_interaction` + `record_feedback`). Store trait extended with `update_interaction_feedback`.

### Phase 6 — Document Pipeline ✅ COMPLETE

`crates/arcana-documents/src/` — `sources/markdown.rs`, `chunker.rs` (heading-aware splitting), `linker.rs` (exact + fuzzy entity linking), `pipeline.rs` (fetch → chunk → embed → link → persist). All tested.

### Phase 7 — CLI Wiring ✅ COMPLETE

`crates/arcana-cli/src/main.rs` — All 7 commands implemented: `init`, `sync` (dbt + Snowflake), `ingest` (Markdown pipeline), `serve --stdio` (MCP server), `ask` (semantic search), `status` (entity counts), `reembed` (batch re-embedding). AppConfig parses `[dbt]` and `[snowflake]` TOML sections.

### Phase 8 — MCP Wiring ✅ COMPLETE

`crates/arcana-mcp/src/tools.rs` — All 4 tools implemented: `get_context`, `describe_table`, `estimate_cost` (delegates to Snowflake adapter), `update_context`. `ArcanaServer` carries optional `SnowflakeConfig` for cost estimation.

### Phase 9 — MVP Test Suite ✅ COMPLETE

`crates/arcana-integration-tests/tests/mvp_end_to_end.rs` — 101 tests covering the full MVP flow: store operations, dbt sync simulation, embedding index, ranker scoring, MCP tools (get_context, describe_table, update_context), feedback loop, confidence decay, and serialization formats. Uses mock embedding provider (no OpenAI key needed).

### Phase 10 — Sandbox (Local E2E Testing) ✅ COMPLETE

`sandbox/` directory with committed README.md and setup.sh. Run `cd sandbox && ./setup.sh` to clone jaffle-shop, generate dbt artifacts, and initialize the store. All runtime files (arcana.toml, arcana.db, jaffle-shop/) are gitignored.

### End-to-end integration test (manual)

1. `cd sandbox && ./setup.sh` (or point at your own dbt project)
2. `export OPENAI_API_KEY="sk-..."`
3. `cargo run --bin arcana -- --config sandbox/arcana.toml sync --adapter dbt`
4. `cargo run --bin arcana -- --config sandbox/arcana.toml reembed`
5. `cargo run --bin arcana -- --config sandbox/arcana.toml ask "monthly revenue by region"` — verify it returns sensible tables/columns
6. Start `arcana serve --stdio`, wire to Claude Desktop
7. Measure first-try SQL accuracy vs baseline (Claude without Arcana)

---

## Key Conventions

- **All store queries use runtime `sqlx::query()` + `.bind()`.** No compile-time `query!` macros (avoids DATABASE_URL requirement at build time).
- **UUIDs as TEXT, timestamps as TEXT (ISO-8601)** in SQLite (matches migration).
- **Confidence scores are always `f64` in [0.0, 1.0]**.
- **`DefinitionSource` enum** tracks provenance of every `SemanticDefinition`. Adapter-sourced = 0.80, document-linked = 0.65, LLM-drafted = 0.40, human-edited = 0.95.
- **Confidence decays exponentially** — see `arcana-core/src/confidence.rs`. Call `current_confidence(def)` before serving a definition, not the raw stored value.
- **Errors via `anyhow`** throughout. No custom error types unless a module genuinely needs them.
- **No `async_trait` on new code** — use `impl Trait` in return position (Rust 1.75+).

---

## Running Locally

```bash
# Build everything
cargo build

# Run tests (101 integration tests, no API keys needed)
cargo test

# Quick sandbox test with jaffle-shop
cd sandbox && ./setup.sh
export OPENAI_API_KEY="sk-..."
cargo run --bin arcana -- --config sandbox/arcana.toml sync --adapter dbt
cargo run --bin arcana -- --config sandbox/arcana.toml reembed
cargo run --bin arcana -- --config sandbox/arcana.toml ask "total revenue by customer"

# Start MCP server (stdio mode for Claude Desktop)
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
