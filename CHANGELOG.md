# Changelog

All notable changes to Arcana are documented here.

## [0.1.0] - 2026-03-23

### Core Platform
- **Metadata store** — SQLite-backed storage for tables, columns, schemas, semantic definitions, data contracts, lineage edges, metrics, usage records, and documents
- **Confidence system** — Time-decaying confidence scores with evidence-based boosting from agent feedback loops
- **Embedding index** — Persistent vector index with incremental updates and definition-hash caching to skip unchanged embeddings

### Adapters
- **dbt adapter** — Parses `manifest.json` and `catalog.json` to extract models, sources, columns, descriptions, and lineage; incremental sync via content checksums
- **Snowflake adapter** — Reads `INFORMATION_SCHEMA` for tables/columns/comments, `ACCOUNT_USAGE` for query history and access patterns, `EXPLAIN` for cost estimation

### Search & Retrieval
- **Hybrid search** — BM25 full-text search (SQLite FTS5) fused with dense cosine similarity via Reciprocal Rank Fusion (RRF)
- **Graph-aware context** — Lineage expansion walks upstream/downstream edges to include related tables in search results
- **Context recommender** — Token-budget-aware serializer that packs the most relevant metadata into a single context window

### AI Integration
- **MCP server** — 6 tools (`get_context`, `describe_table`, `estimate_cost`, `update_context`, `find_similar_tables`, `report_outcome`) over stdio and HTTP/SSE transports
- **Auto-enrichment** — LLM-generated descriptions for undescribed tables and columns using Claude
- **Feedback loop** — `report_outcome` and `update_context` tools let agents push corrections and evidence back into the store

### Document Pipeline
- **Markdown ingestion** — Glob-based document discovery, chunking, entity linking, and embedding for supplementary docs (runbooks, wiki pages, data dictionaries)

### Quality & Operations
- **Redundancy detection** — Semantic clustering to surface duplicate or near-duplicate tables with canonical flagging
- **CI/CD integration** — GitHub Action for auto-sync on dbt model changes; webhook endpoint for external triggers
- **Observability** — Admin API with `/health`, `/api/admin/stats`, entity coverage reporting
- **Docker packaging** — Multi-stage Dockerfile, docker-compose for one-command deployment
- **Security hardening** — Timing-safe auth, parameterized queries, shell injection prevention, command allowlisting

### Testing
- 30+ unit and integration tests covering store operations, adapter parsing, confidence decay, ranking, and end-to-end MCP flows
- Scale-tested with GitLab Analytics (3,400+ dbt models, 28,700+ columns, 22,000+ semantic definitions)
