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

Out of scope for MVP (may be addressed in scale roadmap):
- React frontend
- PostgreSQL backend
- BigQuery, Databricks adapters
- Confluence, Google Docs, Notion, Slack sources
- Usage-based collaborative filtering

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

`crates/arcana-cli/src/main.rs` — All 8 commands implemented: `init`, `sync` (dbt + Snowflake), `ingest` (Markdown pipeline), `serve --stdio` (MCP server), `ask` (semantic search), `status` (entity counts), `reembed` (batch re-embedding), `enrich` (LLM-generated descriptions). AppConfig parses `[dbt]`, `[snowflake]`, and `[enrichment]` TOML sections.

### Phase 8 — MCP Wiring ✅ COMPLETE

`crates/arcana-mcp/src/tools.rs` — All 4 tools implemented: `get_context`, `describe_table`, `estimate_cost` (delegates to Snowflake adapter), `update_context`. `ArcanaServer` carries optional `SnowflakeConfig` for cost estimation.

### Phase 9 — MVP Test Suite ✅ COMPLETE

`crates/arcana-integration-tests/tests/mvp_end_to_end.rs` — 101 tests covering the full MVP flow: store operations, dbt sync simulation, embedding index, ranker scoring, MCP tools (get_context, describe_table, update_context), feedback loop, confidence decay, and serialization formats. Uses mock embedding provider (no OpenAI key needed).

### Phase 10 — Sandbox (Local E2E Testing) ✅ COMPLETE

`sandbox/` directory with committed README.md and setup.sh. Run `cd sandbox && ./setup.sh` to clone jaffle-shop, generate dbt artifacts, and initialize the store. All runtime files (arcana.toml, arcana.db, jaffle-shop/) are gitignored.

### End-to-end integration test (manual)

1. `cd sandbox && ./setup.sh` (or point at your own dbt project)
2. `export OPENAI_API_KEY="sk-..."` and `export ANTHROPIC_API_KEY="sk-ant-..."`
3. `cargo run --bin arcana -- --config sandbox/arcana.toml sync --adapter dbt`
4. `cargo run --bin arcana -- --config sandbox/arcana.toml enrich --dry-run` — preview generated definitions
5. `cargo run --bin arcana -- --config sandbox/arcana.toml enrich` — write LLM descriptions for undescribed tables/columns
6. `cargo run --bin arcana -- --config sandbox/arcana.toml reembed` — embed all definitions including new ones
7. `cargo run --bin arcana -- --config sandbox/arcana.toml ask "monthly revenue by region"` — verify it returns sensible tables/columns
8. Start `arcana serve --stdio`, wire to Claude Code or Claude Desktop
9. Measure first-try SQL accuracy vs baseline (Claude without Arcana)

---

## Scale Roadmap — 2000+ Model dbt Projects

The MVP works on jaffle-shop (19 models). These phases target production-scale catalogs where the hard problems are search quality, undocumented models, table redundancy, and context budget.

---

### Phase A — Hybrid Search + Re-ranking  ✅ COMPLETE (PR #6, branch `feat/hybrid-search`)

Two-stage retrieval: FTS5 BM25 + dense cosine → Reciprocal Rank Fusion → confidence-weighted scoring.

- `0002_fts.sql` — FTS5 virtual table over `semantic_definitions` with INSERT/UPDATE/DELETE triggers. Backfills existing rows.
- `MetadataStore::fts_search(query, limit)` — BM25 retrieval, input sanitized to strip FTS5 special chars, scores normalized to [0, 1].
- `RelevanceRanker::rank()` — FTS + dense each return `top_k * 4` candidates, fused via RRF (k=60), normalized, then `combined_score(rrf, confidence)`. Document chunks use pure dense search.
- Unit tests: `rrf_rewards_overlap`, `rrf_empty_lists`.

---

### Phase B — Auto-Description Generation  ✅ COMPLETE (PR #6, branch `feat/hybrid-search`)

`arcana enrich` generates semantic definitions for undescribed tables and columns via Claude.

- `arcana-core/src/enrichment/` — `EnrichmentProvider` trait + `ClaudeEnrichmentProvider` (Anthropic Messages API).
  - Structured prompt includes table name, column names, upstream lineage.
  - `---`-delimited response parsing; pads short responses rather than erroring.
- `arcana enrich [--dry-run] [--filter <name>] [--batch-size <n>]`
  - Skips entities already covered by `DbtYaml`, `Manual`, or `SnowflakeComment` definitions.
  - Writes back with `DefinitionSource::LlmInferred` at confidence 0.40.
  - Falls back to `ANTHROPIC_API_KEY` env var; configurable via `[enrichment]` section in arcana.toml.
- After enrich, run `arcana reembed` to embed the new definitions.

---

### Phase C — Redundancy Detection  ⬜ TODO

Cluster semantically similar table definitions, surface duplicates, designate canonicals.

- After reembed, compute pairwise cosine similarity for all table-level definitions
- Tables with similarity > 0.92 flagged as potential duplicates
- `arcana dedup [--threshold 0.92] [--dry-run]` — shows clusters, lets user mark canonical
- New MCP tool: `find_similar_tables(table_name)` — returns similar tables with recommendation
- `get_context` response includes `canonical` flag and deprecation warnings for non-canonical tables

---

### Phase D — Graph-Aware Context Assembly  ⬜ TODO

Lineage traversal on search hits: return the metric + upstream tables + column definitions in one response.

- `RelevanceRanker::rank()` gets optional `expand_lineage: bool`
- For each top-k table hit, call `store.get_upstream(table_id)` and include first-degree upstream tables within token budget
- `ContextResult` gets `lineage_summary: Option<String>` rendered as a DAG path
- Domain clustering: group tables by definition similarity, return full domain picture

---

### Phase E — Self-Improving Feedback Loop  ⬜ TODO

Boost confidence on definitions that get used in successful queries.

- `update_context` tool call after a successful query boosts confidence on definitions that were in the context
- Snowflake `QUERY_HISTORY` mining: extract table co-occurrence from successful queries as evidence
- New `EvidenceRecord` table linking (entity_id, query_text, outcome, source)

---

### Phase F — Performance at Scale  ⬜ TODO

HNSW index, persistent index, incremental sync, embedding cache.

- Replace flat `VectorIndex` with `usearch` crate (HNSW, sub-ms ANN at 1M vectors)
- Serialize index to disk on shutdown, load on startup — skip full warm from SQLite
- Incremental sync: store dbt manifest checksums, only re-sync changed models
- Embedding cache: hash definition text → skip re-embedding unchanged definitions

---

### Phase G — Additional Adapters  ⬜ TODO

Snowflake `INFORMATION_SCHEMA` (no dbt required), BigQuery, Databricks Unity Catalog, LookML, dbt Cloud API.

---

### Summary: Priority for a 2000-Model Project

| Phase | Impact | Status |
|-------|--------|--------|
| A — Hybrid Search | Very High | ✅ Done |
| B — Auto-Description | Very High | ✅ Done |
| C — Redundancy Detection | High | ⬜ Next |
| D — Graph-Aware Context | High | ⬜ Queued |
| E — Feedback Loop | Medium | ⬜ Queued |
| F — Performance | Medium | ⬜ Queued |
| G — Adapters | Varies | ⬜ On demand |

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
