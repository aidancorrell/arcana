# Arcana — Implementation Guide

Agent-first data catalog. MCP is the primary interface. The thesis: text-to-SQL accuracy jumps from ~20% to 92%+ when an agent knows which tables matter, what columns mean, and what business rules apply.

Architecture doc: see `catalog-architecture-v2.md` (kept locally, not committed).

---

## Repo Layout

```
crates/
  arcana-core/        # Entity types, SQLite store, confidence system, embedding index
  arcana-adapters/    # Snowflake + dbt metadata adapters
  arcana-documents/   # Document ingestion pipeline (markdown → chunk → link → embed)
  arcana-recommender/ # Context ranker + serializer
  arcana-mcp/         # MCP server (get_context, describe_table, estimate_cost, update_context)
  arcana-cli/         # arcana init|sync|ingest|serve|ask|status|reembed
config/
  arcana.example.toml
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

### Phase 5 — Recommender (partial — serializer done, ranker needs completion)

File: `crates/arcana-recommender/src/ranker.rs`

`serializer.rs` and `record_interaction` in `feedback.rs` are complete.

Remaining work in `ranker.rs` — the `rank()` method returns empty results. Fill in the TODO:
1. Embed the query (already done)
2. Search entity index + chunk index (already done)
3. For each hit: fetch entity from store, apply confidence decay (`confidence.rs`), compute `combined_score(similarity, decayed_confidence)`
4. Sort descending, truncate to `top_k`, filter below `min_confidence`
5. Return `ContextResult` with `ContextItem` per hit

Also in `feedback.rs`: `record_feedback()` (line ~48) has a TODO for the UPDATE query.

### Phase 6 — Document Pipeline ✅ COMPLETE

`crates/arcana-documents/src/` — `sources/markdown.rs`, `chunker.rs` (heading-aware splitting), `linker.rs` (exact + fuzzy entity linking), `pipeline.rs` (fetch → chunk → embed → link → persist). All tested.

### Phase 7 — CLI Wiring (partial — only `cmd_init` done)

File: `crates/arcana-cli/src/main.rs`

`cmd_init` is complete. The remaining 6 commands are stubbed with `todo!()`:

- `cmd_sync`: load config, build adapter (dbt or snowflake based on config), call `sync()`, upsert results into store
- `cmd_ingest`: build `MarkdownSource`, run `IngestPipeline`, report stats
- `cmd_serve --stdio`: build `ArcanaServer`, call `serve_stdio()`
- `cmd_ask`: embed query, call `ranker.rank()`, call `serializer.serialize()`, print
- `cmd_status`: query store for counts, print table
- `cmd_reembed`: fetch all entities and chunks, re-embed in batches, update embeddings in store

### Phase 8 — MCP Wiring (partial — `estimate_cost` tool stubbed)

`crates/arcana-mcp/src/tools.rs` — `get_context`, `describe_table`, and `update_context` tools are complete. `estimate_cost` (line ~191) has a `todo!()` — needs to delegate to `arcana-adapters::snowflake::cost::estimate_query_cost()`.

End-to-end integration test:
1. Point at a real dbt project, run `arcana sync --adapter dbt`
2. Run `arcana ask "monthly revenue by region"` — verify it returns sensible tables/columns
3. Start `arcana serve --stdio`, wire to Claude Desktop
4. Measure first-try SQL accuracy vs baseline (Claude without Arcana)

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

# Run tests
cargo test

# Initialize a local store
cargo run --bin arcana -- init ./test-project

# Start MCP server (stdio mode for Claude Desktop)
cargo run --bin arcana -- serve --stdio
```

Claude Desktop config (`~/Library/Application Support/Claude/claude_desktop_config.json`):
```json
{
  "mcpServers": {
    "arcana": {
      "command": "/path/to/arcana",
      "args": ["serve", "--stdio"],
      "env": {
        "ARCANA_DATABASE__URL": "sqlite:///path/to/arcana.db"
      }
    }
  }
}
```
