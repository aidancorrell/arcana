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

`crates/arcana-cli/src/main.rs` — All 9 commands implemented: `init`, `sync` (dbt + Snowflake), `ingest` (Markdown pipeline), `serve --stdio` (MCP server), `ask` (semantic search), `status` (entity counts), `reembed` (batch re-embedding), `enrich` (LLM-generated descriptions), `dedup` (redundancy detection). AppConfig parses `[dbt]`, `[snowflake]`, and `[enrichment]` TOML sections.

### Phase 8 — MCP Wiring ✅ COMPLETE

`crates/arcana-mcp/src/tools.rs` — All 5 tools implemented: `get_context`, `describe_table`, `estimate_cost` (delegates to Snowflake adapter), `update_context`, `find_similar_tables` (redundancy detection). `ArcanaServer` carries optional `SnowflakeConfig` for cost estimation.

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

### Phase C — Redundancy Detection  ✅ COMPLETE (branch `feat/scale-phase-cd`)

Cluster semantically similar table definitions, surface duplicates, designate canonicals.

- `0003_table_clusters.sql` — `table_clusters` and `table_cluster_members` tables with foreign keys.
- `arcana-core/src/entities/cluster.rs` — `TableCluster` and `TableClusterMember` structs.
- `MetadataStore` extended with 5 cluster methods: `upsert_table_cluster`, `upsert_cluster_member`, `get_cluster_for_table`, `list_table_clusters`, `clear_table_clusters`.
- `VectorIndex::pairs_above_threshold(f32)` — pairwise cosine similarity for all stored vectors. `VectorIndex::get(Uuid)` — retrieve stored embedding.
- `arcana-recommender/src/dedup.rs` — `find_clusters()` (union-find single-linkage clustering) + `find_similar_to()` (kNN filtered to tables).
- `arcana dedup [--threshold 0.92] [--dry-run]` — CLI command shows clusters, persists canonical designations.
- New MCP tool: `find_similar_tables(table_ref, threshold, limit)` — returns similar tables with similarity scores and canonical flags.
- `get_context` response labels non-canonical tables with `[non-canonical — prefer X]` warning.

---

### Phase D — Graph-Aware Context Assembly  ✅ COMPLETE (branch `feat/scale-phase-cd`)

Lineage traversal on search hits: return the metric + upstream tables + column definitions in one response.

- `ContextRequest` extended with `expand_lineage: bool` (default: false for CLI, true for MCP).
- `ContextResult` extended with `lineage_edges: Vec<LineageEdge>`.
- `RelevanceRanker::rank()` — when `expand_lineage` is true, fetches first-degree upstream tables for each table hit and injects them as `[upstream]` context items within token budget.
- Serializer renders lineage edges as a compact DAG section in Markdown output.
- MCP `get_context` tool exposes `expand_lineage` parameter (default true).

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
| C — Redundancy Detection | High | ✅ Done |
| D — Graph-Aware Context | High | ✅ Done |
| E — Feedback Loop | Medium | ⬜ Queued |
| F — Performance | Medium | ⬜ Queued |
| G — Adapters | Varies | ⬜ On demand |

---

## Scale Roadmap — 2000+ Model dbt Projects

The MVP works on jaffle-shop (19 models). The following phases target production-scale catalogs where the hard problems are search quality, undocumented models, table redundancy, and context budget.

Priority order is based on impact: a large org with 2000 models and no descriptions gets zero value from arcana unless phases A and B ship first.

---

### Phase A — Hybrid Search + Re-ranking  ⬜ TODO
**Problem:** At 2000 models the flat cosine scan returns too many mediocre hits (all scores within 0.05 of each other). The churn query above illustrates this — 8 results, all at 0.44–0.49, no clear winner.

**Solution:** Two-stage retrieval.
- Stage 1 (recall): BM25 sparse retrieval (tantivy crate) OR SQLite FTS5 `MATCH` query retrieves top-50 candidates by keyword overlap
- Stage 2 (precision): dense cosine re-ranks the 50 candidates to top-k
- Optional stage 3: cross-encoder re-ranking via a small Claude call (classify each candidate as relevant/not for the query) — expensive but accurate

**Implementation sketch:**
- Add `arcana-core/src/store/fts.rs` — FTS5 virtual table over `semantic_definitions.definition`
- `MetadataStore` gets `fts_search(query, limit) -> Vec<(Uuid, f32)>`
- `RelevanceRanker::rank()` fuses BM25 hits + vector hits via Reciprocal Rank Fusion before re-ranking
- No new dependencies if using FTS5 (already in SQLite)

**Success metric:** "churn" query returns `most_recent_order_date` as the clear #1 hit, not buried at #7.

---

### Phase B — Auto-Description Generation  ⬜ TODO
**Problem:** Large orgs have hundreds of undocumented models. Arcana's semantic search is only as good as the definitions it has. Models with no dbt description = invisible.

**Solution:** For every table/column with no `SemanticDefinition` (or a very low-confidence one), call Claude to synthesize a definition from:
- Table name + column names
- Sample SQL from the dbt model (from `manifest.json` `compiled_code` field)
- Upstream table names from lineage

Auto-generated definitions get `DefinitionSource::LlmDrafted` (confidence 0.40) and are flagged for human review.

**New CLI command:** `arcana enrich [--dry-run] [--model <table>]`
- Finds all entities with no definition or confidence < 0.5
- Calls Claude in batches (50 tables per prompt) with structured output
- Writes definitions back via `upsert_semantic_definition`
- Optionally flags them in the store for human review

**New config key:**
```toml
[enrichment]
model = "claude-opus-4-6"  # or haiku for cost efficiency
batch_size = 50
auto_enrich_on_sync = false  # opt-in: run enrich after every sync
```

**Success metric:** 2000-model project goes from 20% definition coverage to 90%+ in a single `arcana enrich` run.

---

### Phase C — Redundancy Detection  ✅ COMPLETE
See concise summary above.

---

### Phase D — Graph-Aware Context Assembly  ✅ COMPLETE
See concise summary above.

---

### Phase E — Self-Improving Feedback Loop  ⬜ TODO
**Problem:** The catalog starts cold. Confidence is static (0.80 for all dbt adapter-sourced definitions). There's no mechanism for the catalog to get better from use.

**Solution:** Query history as evidence. Every time an agent uses arcana context to write a query that succeeds, that interaction is evidence that the definitions used were correct. Boost confidence on those definitions.

**Signals:**
1. **Explicit feedback:** `update_context` tool call from the agent after a successful query — already exists, just needs a confidence-boost side effect
2. **Query history mining:** `arcana sync --adapter snowflake` now also ingests `QUERY_HISTORY` — extract which tables were joined together in successful queries, weight those joins as evidence
3. **Cross-session pattern mining:** If 40 distinct agents have fetched context about `customers` + `orders` in the same session, that co-occurrence is a strong signal — surface it as a pre-built join recommendation

**New concept: `EvidenceRecord`** — a lightweight table linking (entity_id, query_text, outcome: success|failure, source: agent|human) used to compute empirical confidence alongside the decay-based confidence.

**Success metric:** After 30 days of agent usage on a production catalog, confidence scores reflect actual query success rates, not just definition source provenance.

---

### Phase F — Performance at Scale  ⬜ TODO
**Problem:** 2000 models × ~10 definitions + all columns = ~25k–50k vectors. The flat O(n) cosine scan becomes slow. Incremental sync is also missing — a full re-sync of a 2000-model project is expensive.

**Solutions:**
- **HNSW index:** Replace `VectorIndex` flat scan with `usearch` crate (pure Rust, zero-copy). Sub-millisecond ANN search at 1M vectors.
- **Persistent index:** Serialize the HNSW index to disk on shutdown, load on startup — skip warming from SQLite on every launch
- **Incremental sync:** dbt manifest has `node_relation.checksum` per model. Store checksums; only re-sync models whose checksum changed. Re-embed only changed definitions.
- **Embedding cache:** Hash definition text → cache embedding. Reembed only when text changes, not on every `reembed` invocation.

**New config key:**
```toml
[index]
backend = "hnsw"         # or "flat" (default, no deps)
persist_path = "./arcana.idx"
ef_construction = 200
m = 16
```

**Success metric:** `arcana serve --stdio` startup time on a 50k-vector index < 200ms. Full sync of a 2000-model project with 10% changed models takes < 30s instead of minutes.

---

### Phase G — Additional Adapters  ⬜ TODO
- **Snowflake `INFORMATION_SCHEMA`** — schema/column sync without needing dbt. Any Snowflake warehouse works, not just dbt-managed ones.
- **BigQuery `INFORMATION_SCHEMA`** — same pattern.
- **Databricks Unity Catalog** — REST API for table/column metadata, lineage from Delta Lake.
- **LookML** — parse `view` and `explore` files for field definitions (high value: LookML definitions are often very good).
- **dbt Cloud API** — pull dbt docs from hosted dbt Cloud instead of local `manifest.json`.

---

### Summary: Priority for a 2000-Model Project

| Phase | Impact | Effort | Ship first? |
|-------|--------|--------|-------------|
| A — Hybrid Search | Very High | Medium | ✅ Yes |
| B — Auto-Description | Very High | Medium | ✅ Yes |
| C — Redundancy Detection | High | Medium | ✅ Done |
| D — Graph-Aware Context | High | High | ✅ Done |
| E — Feedback Loop | Medium | High | Third batch |
| F — Performance | Medium | Medium | Third batch |
| G — Adapters | Varies | Medium | On demand |

Phases A and B together turn arcana from "works on well-documented small projects" to "works on any real-world dbt project."

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
