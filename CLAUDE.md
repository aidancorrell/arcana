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

### Phase 1 — SQLite Store (unblock everything else)

All store methods in `crates/arcana-core/src/store/sqlite.rs` are `todo!()`. Implement them top to bottom. The migration schema (`0001_initial.sql`) is already correct — just write the queries.

Priority order (most depended-on first):
1. `upsert_data_source` / `get_data_source` / `list_data_sources`
2. `upsert_schema` / `list_schemas`
3. `upsert_table` / `get_table` / `list_tables` / `search_tables` (use FTS5 virtual table)
4. `upsert_column` / `list_columns`
5. `upsert_semantic_definition` / `get_semantic_definitions`
6. `upsert_column_profile`
7. `upsert_metric` / `list_metrics`
8. `upsert_document` / `get_document` / `upsert_chunk` / `list_chunks` / `upsert_entity_link`
9. `insert_usage_record` / `insert_agent_interaction`
10. `upsert_contract` / `list_contracts`
11. `upsert_lineage_edge` / `get_upstream` / `get_downstream`

All rows use TEXT UUIDs and TEXT ISO-8601 timestamps (matching the migration). Use `sqlx::query()` (runtime, not `query!` compile-time macros) and `.bind()` chains as established in the existing `upsert_data_source` / `upsert_schema` / `upsert_table` implementations.

### Phase 2 — dbt Adapter

File: `crates/arcana-adapters/src/dbt/`

Implement `manifest.rs` first — it's the richest source:
- Parse `manifest.json` into `Table`, `Column`, and `SemanticDefinition` entities
- Key paths: `manifest.nodes`, `manifest.sources`, `manifest.metrics`, `manifest.semantic_models`
- Column descriptions from `nodes[*].columns[*].description` → `SemanticDefinition` with `source = AdapterSync { adapter_type: "dbt" }`, confidence 0.80
- Model descriptions from `nodes[*].description` → `SemanticDefinition` on the table

Then `catalog.rs`:
- Parse `catalog.json` for column types and row counts (supplements manifest)

Then `semantic.rs`:
- Parse MetricFlow YAML / `manifest.metrics` for `Metric` entities

Wire up the dbt adapter to `MetadataAdapter` trait in `adapter.rs`. Implement `list_schemas`, `list_tables`, `list_columns`, `sync_descriptions`.

### Phase 3 — Snowflake Adapter

File: `crates/arcana-adapters/src/snowflake/`

Uses Snowflake SQL API (REST, no JDBC needed). Auth via `SNOWFLAKE_USER` / `SNOWFLAKE_PASSWORD` env vars + account identifier.

Order:
1. `schema_sync.rs` — implement `list_schemas` (query `INFORMATION_SCHEMA.SCHEMATA`), `list_tables` (query `INFORMATION_SCHEMA.TABLES`), `list_columns` (query `INFORMATION_SCHEMA.COLUMNS`)
2. `cost.rs` — implement `estimate_cost` using `EXPLAIN` statement, parse bytes scanned, estimate credits at $2.50/TB
3. `profiler.rs` — implement column profiling: `COUNT(*)`, `COUNT(col)`, `COUNT(DISTINCT col)`, `MIN`, `MAX` per column, batch into single multi-stat query
4. `usage.rs` — query `SNOWFLAKE.ACCOUNT_USAGE.QUERY_HISTORY` for table access patterns

For the SQL API, POST to `https://{account}.snowflakecomputing.com/api/v2/statements` with a JWT token. Use `reqwest` (already in workspace deps).

### Phase 4 — Embedding Index

File: `crates/arcana-core/src/embeddings/`

Implement `provider.rs` (`EmbeddingProvider` trait) and `openai.rs` (OpenAI `text-embedding-3-small`).

For `index.rs` (`VectorIndex`): start with an in-memory brute-force cosine similarity search over a `Vec<(Uuid, Vec<f32>)>`. This is fast enough for MVP (< 10k entities). Replace with `usearch` later.

The index needs:
- `insert(id: Uuid, embedding: Vec<f32>)` — add or update a vector
- `search(query: &[f32], top_k: usize) -> Vec<(Uuid, f32)>` — return (id, similarity) pairs

After each `upsert_semantic_definition` / `upsert_chunk`, the pipeline should call `index.insert(entity_id, embedding)`.

### Phase 5 — Recommender

File: `crates/arcana-recommender/src/ranker.rs`

The `rank()` stub is mostly wired. Fill in the TODO:
1. Embed the query (already done)
2. Search entity index + chunk index (already done)
3. For each hit: fetch entity from store, apply confidence decay (`confidence.rs`), compute `combined_score(similarity, decayed_confidence)`
4. Sort descending, truncate to `top_k`, filter below `min_confidence`
5. Return `ContextResult` with `ContextItem` per hit

`serializer.rs`: implement `serialize(result: &ContextResult) -> String` — produces the token-efficient JSON format from the architecture doc (tables with relevant columns, joins, metrics, warnings for low-confidence entities). Also implement `ContextSerializer::format_table` used by `describe_table` in the MCP tools.

`feedback.rs`: implement `record_interaction` — insert an `AgentInteraction` row via the store.

### Phase 6 — Document Pipeline

File: `crates/arcana-documents/src/`

`sources/markdown.rs`: walk a directory, read `.md` files, return `RawDocument` structs.

`chunker.rs` (`StructureAwareChunker`): split on headings (`#`, `##`, `###`), preserve heading ancestry as `section_path`. If a section exceeds `max_chunk_tokens` (512), split on paragraph breaks with 50-token overlap.

`linker.rs` (`EntityLinker`): exact-match scan for known table/column names in each chunk text. Fuzzy match for common aliases. Return `EntityLink` with confidence 0.65 for exact, 0.50 for fuzzy. (LLM-assisted linking is Phase 2.)

`pipeline.rs`: orchestrate the six stages — ingest → chunk → link → embed → enrich → (refresh later). The `enrich` stage creates `SemanticDefinition` rows for each `(chunk, entity_link)` pair with confidence > 0.5.

### Phase 7 — CLI Wiring

File: `crates/arcana-cli/src/main.rs`

All commands have stubs. Wire them up:

- `cmd_init`: already mostly done (writes config, opens store)
- `cmd_sync`: load config, build adapter (dbt or snowflake based on config), call `full_sync()`, upsert results into store
- `cmd_ingest`: build `MarkdownSource`, run `IngestPipeline`, report stats
- `cmd_serve --stdio`: build `ArcanaServer`, call `serve_stdio()`
- `cmd_ask`: embed query, call `ranker.rank()`, call `serializer.serialize()`, print
- `cmd_status`: query store for counts, print table
- `cmd_reembed`: fetch all entities and chunks, re-embed in batches, update embeddings in store

### Phase 8 — MCP Integration Test

With the above complete, do an end-to-end test:
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
