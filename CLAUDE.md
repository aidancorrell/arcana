# Arcana — Implementation Guide

Agent-first data catalog. MCP is the primary interface. The thesis: text-to-SQL accuracy improves dramatically when an agent knows which tables matter, what columns mean, and what business rules apply.

Architecture doc: see [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md).

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
Dockerfile                    # Multi-stage build (rust builder → debian slim runtime)
docker-compose.yml            # One-command deployment with volume + config
arcana.docker.toml            # Docker-optimized config template
.github/workflows/
  arcana-sync.yml             # GitHub Action: auto-sync on dbt model changes
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

`crates/arcana-mcp/src/tools.rs` — All 6 tools implemented: `get_context`, `describe_table`, `estimate_cost` (delegates to Snowflake adapter), `update_context`, `find_similar_tables` (redundancy detection), `report_outcome` (feedback loop). `ArcanaServer` carries optional `SnowflakeConfig` for cost estimation.

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

### Phase E — Self-Improving Feedback Loop  ✅ COMPLETE (branch `feat/scale-phase-ef`)

Evidence-based confidence adjustment: definitions used in successful queries get boosted.

- `0004_evidence_and_cache.sql` — `evidence_records` table, `definition_hash` column on `semantic_definitions`, `sync_checksums` table.
- `arcana-core/src/entities/evidence.rs` — `EvidenceRecord`, `EvidenceOutcome`, `EvidenceSource` types.
- `MetadataStore` extended with `insert_evidence_record`, `get_evidence_for_entity`, `boost_confidence`.
- `boost_confidence(entity_id, delta)` — updates confidence on all definitions for an entity, clamped to [0.0, 1.0].
- New MCP tool: `report_outcome(entity_ids, outcome, query_text)` — agents call this after a query succeeds/fails. Success = +0.05 confidence, failure = -0.03.
- Evidence records persist the full audit trail (entity, outcome, source, delta, timestamp).
- Boosted confidence propagates naturally through the existing ranker scoring formula.

---

### Phase F — Performance at Scale  ✅ COMPLETE (branch `feat/scale-phase-ef`)

Persistent index, incremental sync, embedding cache. (HNSW deferred — flat index sufficient for ≤100k vectors.)

- **Persistent index:** `VectorIndex::save(path)` / `VectorIndex::load(path)` via bincode serialization. MCP server saves index on shutdown, loads on startup — skips SQLite warm step.
- **Config:** `[index] persist_path = "./arcana.idx"` in arcana.toml.
- **Embedding cache:** `definition_hash` (SHA-256) on `SemanticDefinition`. `arcana reembed` skips re-embedding when text is unchanged and embedding already exists.
- **Incremental dbt sync:** `sync_checksums` table stores per-model checksums from dbt manifest. `arcana sync --adapter dbt` skips unchanged models (use `--full` to force full re-sync). `DbtNode.checksum` parsed from manifest.
- `MetadataStore` extended with `get_sync_checksum`, `upsert_sync_checksum`, `list_sync_checksums`.

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
| E — Feedback Loop | Medium | ✅ Done |
| F — Performance | Medium | ✅ Done |
| G — Adapters | Varies | ⬜ On demand |

---

## Deployment Roadmap — From Local Tool to Team Infrastructure

The MVP and scale phases prove Arcana works. The deployment roadmap takes it from "one engineer runs it locally" to "shared infrastructure the whole data team relies on."

---

### Phase H — HTTP/SSE MCP Transport (Multi-User Server)  ✅ COMPLETE

**Problem:** Stdio transport means each user runs their own Arcana process with their own metadata copy. No shared state, no shared feedback loop, no centralized management.

**Solution:** Add HTTP+SSE server transport so multiple MCP clients connect to one running Arcana instance.

**Implementation:**
- `rmcp` already has `transport-sse-server` feature — axum-based SSE server with session management.
- Add `axum` dependency, enable `rmcp` feature `transport-sse-server`.
- `ArcanaServer::serve_http(bind_addr)` — starts axum router with SSE endpoint (`GET /sse`) and message endpoint (`POST /message`).
- `arcana serve` (no `--stdio`) binds to `[mcp] bind_addr` (default `127.0.0.1:8477`).
- API key auth middleware: `[mcp] api_keys = ["key1", "key2"]` in config. Validates `Authorization: Bearer <key>` header.
- Health endpoint: `GET /health` returns server status, entity counts, index stats.

**Config:**
```toml
[mcp]
bind_addr = "0.0.0.0:8477"
max_context_tokens = 8000
api_keys = ["arcana-team-key-1"]  # optional; if empty, no auth required
```

**Client config (Claude Desktop / Claude Code):**
```json
{
  "mcpServers": {
    "arcana": {
      "url": "http://arcana-server.internal:8477/sse",
      "headers": { "Authorization": "Bearer arcana-team-key-1" }
    }
  }
}
```

**Success metric:** 5 engineers on the same team all get context from one Arcana server. Feedback from one user's `report_outcome` improves results for everyone.

---

### Phase I — Docker Packaging & Deployment  ✅ COMPLETE

Multi-stage Docker build producing a minimal container image.

- `Dockerfile` — two-stage build: `rust:1.82-slim-bookworm` builder (with dependency caching layer) → `debian:bookworm-slim` runtime with `ca-certificates`, `libssl3`, `curl`.
- `docker-compose.yml` — Arcana server with named volume (`arcana-data`) for SQLite DB + index. API keys passed via environment. Config injected via Docker configs.
- `arcana.docker.toml` — Docker-optimized config template: SQLite at `/data/arcana.db`, index at `/data/arcana.idx`, binds `0.0.0.0:8477`, dbt project mountable at `/dbt`.
- `.dockerignore` — excludes `target/`, `.git/`, `sandbox/`, secrets.
- Non-root `arcana` user in container. Health check via `curl` to SSE endpoint.
- `ENTRYPOINT ["arcana"]`, `CMD ["--config", "/data/arcana.toml", "serve"]`.

**Deployment:**
```bash
# Quickstart
docker run -p 8477:8477 \
  -v arcana-data:/data \
  -e OPENAI_API_KEY=sk-... \
  arcana:latest

# With docker-compose
cp arcana.docker.toml arcana.toml  # customize if needed
docker compose up -d
```

---

### Phase J — CI/CD Integration (Automated Sync)  ✅ COMPLETE

**Problem:** Metadata goes stale if someone has to remember to run `arcana sync` after dbt changes.

**Solution:** Automated sync triggered by dbt CI events.

**Implementation:**
- **GitHub Action** (`.github/workflows/arcana-sync.yml`): runs on push to `main` when `models/` or `schema.yml` changes. Calls `POST /api/sync` on the shared server.
- **Webhook endpoint**: `POST /api/sync` on the admin HTTP server — triggers sync for a given adapter. Secured by `Authorization: Bearer <key>`.
- **Scheduled sync**: `arcana serve --sync-interval 6h` — background task runs incremental sync + optional enrich + reembed on a timer.
- **Background sync worker**: Receives triggers from webhook or timer, runs incremental dbt sync, optional auto-enrich, optional auto-reembed.

**Config:**
```toml
[mcp]
admin_addr = "0.0.0.0:8478"  # admin API on separate port

[sync]
auto_interval = "6h"         # background sync interval (0 = disabled)
auto_enrich = true            # run enrich after each sync
auto_reembed = true           # run reembed after each sync
webhook_secret = "whsec_..."  # for webhook authentication
```

---

### Phase K — Observability & Admin  ✅ COMPLETE

**Problem:** No visibility into what Arcana knows, what's stale, what agents are asking, or whether the system is healthy.

**Solution:** Admin API endpoints and an enhanced `arcana status` command.

**Implementation:**
- **Admin API** (on separate port, under `/api/admin/`):
  - `GET /api/admin/stats` — entity counts, definition coverage %, embedded count, index size, cluster count.
  - `GET /api/admin/coverage` — per-schema breakdown of tables with/without definitions.
  - `GET /api/admin/evidence` — recent evidence records (feedback trail), sorted by date, limited to 100.
  - `GET /health` — basic health check with version info.
- **`arcana status --detailed`** — enhanced CLI command showing per-schema coverage, stale entity counts, and confidence distribution histogram.
- Prometheus metrics deferred (not needed for MVP deployment).

---

### Deployment Summary

| Phase | What | Unlocks | Status |
|-------|------|---------|--------|
| H — HTTP Transport | Multi-user MCP server | Shared team deployment | ✅ Done |
| I — Docker | Container image + compose | Easy deployment | ✅ Done |
| J — CI/CD | Auto-sync on dbt changes | Always-fresh metadata | ✅ Done |
| K — Observability | Admin API + metrics | Operational visibility | ✅ Done |

Phase H is the critical path — without it, Phases I–K have no server to deploy/monitor.

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
