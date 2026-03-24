# Arcana: Agent-First Data Catalog — Architecture

> **One-liner:** The context layer between AI agents and your data stack.

---

## The Core Thesis

Every existing data catalog was built for humans to browse. Arcana is built for machines to query. The primary interface is MCP, not a UI. The primary consumer is an AI agent (Claude Code, Cursor, Copilot, dbt Agents), not a data analyst clicking through a web app.

This isn't a catalog that "also has an API." It's an API that also has a catalog.

---

## Why This Wins

**The accuracy gap is the business case.** Text-to-SQL accuracy jumps from ~20% to 92-100% when a semantic layer provides context. Every AI-data interaction today is bottlenecked by the same problem: the agent doesn't know which tables matter, what the columns mean, or what business rules apply. Arcana solves that by unifying fragmented metadata and serving it in a token-efficient, agent-optimized format.

**The fragmentation is the moat.** Semantic definitions live across dbt YAML, Snowflake comments, LookML, Cube data models, Confluence pages, Google Docs, Slack threads, and tribal knowledge. No tool unifies these. Arcana becomes the single source of truth that agents call before writing any query.

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                     AI Agents (Consumers)                    │
│  Claude Code · Cursor · dbt Agents · Custom Agents · IDEs   │
└──────────────────────────┬──────────────────────────────────┘
                           │ MCP Protocol
                           ▼
┌─────────────────────────────────────────────────────────────┐
│                      Arcana MCP Server                       │
│                                                              │
│  ┌─────────────┐  ┌──────────────┐  ┌────────────────────┐  │
│  │  Context     │  │  Query       │  │  Contract          │  │
│  │  Recommender │  │  Advisor     │  │  Enforcer          │  │
│  │              │  │              │  │                    │  │
│  │  "For this   │  │  "This will  │  │  "You can't       │  │
│  │   question,  │  │   cost $X    │  │   write to this   │  │
│  │   you need   │  │   and scan   │  │   table; SLA is   │  │
│  │   these 7    │  │   Y rows"    │  │   6hr freshness"  │  │
│  │   tables"    │  │              │  │                    │  │
│  └──────┬───────┘  └──────┬───────┘  └────────┬───────────┘  │
│         │                 │                    │              │
│  ┌──────▼─────────────────▼────────────────────▼───────────┐ │
│  │                   Metadata Store                         │ │
│  │                                                          │ │
│  │  Schemas · Column profiles · Semantic definitions        │ │
│  │  Lineage graph · Data contracts · Usage analytics        │ │
│  │  Embeddings index · Business glossary                    │ │
│  │  Document chunks · Confidence scores · Edit history      │ │
│  └──────┬───────────────────┬───────────────────────────────┘ │
└─────────┼───────────────────┼───────────────────────────────┘
          │                   │
  ┌───────┴────────┐  ┌──────┴──────────────────────┐
  │  Structured     │  │  Document Ingestion          │
  │  Adapters       │  │  Pipeline                    │
  │                 │  │                              │
  │  Snowflake      │  │  Markdown · Confluence       │
  │  dbt            │  │  Google Docs · Notion · Slack │
  │  BigQuery*      │  │                              │
  │  Databricks*    │  │  Ingest → Chunk → Link →     │
  │                 │  │  Embed → Enrich → Refresh    │
  └────────────────┘  └─────────────────────────────┘
                                          * planned
```

---

## Core Components

### 1. Metadata Store (SQLite)

The persistence layer. Stores all metadata in a unified schema regardless of source. All semantic definitions support full edit history for confidence tracking and drift detection.

**Core entities:**

| Entity | Description |
|--------|-------------|
| `DataSource` | A warehouse, database, or external system |
| `Schema` | A namespace within a data source |
| `Table` | Physical or virtual table with row counts, freshness |
| `Column` | Column with type info and profiling stats |
| `SemanticDefinition` | Business meaning attached to any entity, with confidence and provenance |
| `Metric` | Named calculation with dimensions, filters, SQL expression |
| `DataContract` | SLAs, quality thresholds, PII flags, access controls |
| `LineageEdge` | Directed edge in the lineage graph |
| `UsageRecord` | Query pattern tracking for usage-based relevance |
| `Document` | Ingested document from external source |
| `DocumentChunk` | Semantically meaningful section with entity links |
| `EvidenceRecord` | Feedback audit trail for confidence adjustments |
| `TableCluster` | Group of semantically similar tables |

### 2. Structured Adapter System

Each data source implements a `MetadataAdapter` trait providing schema sync, column profiling, usage history, lineage extraction, semantic definitions, and cost estimation.

**Shipped adapters:**
- **Snowflake** — `INFORMATION_SCHEMA`, `ACCOUNT_USAGE` views, `EXPLAIN` for cost estimation, column comments
- **dbt** — `manifest.json` (models, sources, metrics, lineage), `catalog.json` (row counts, column types), schema YAML descriptions

### 3. Document Ingestion Pipeline

Handles unstructured business documentation through a six-stage pipeline:

1. **Ingest** — Pull raw content from source (Markdown files, Confluence, etc.)
2. **Chunk** — Split into semantically meaningful sections respecting heading hierarchy
3. **Link** — Resolve references to known tables/columns via exact + fuzzy matching
4. **Embed** — Generate vector embeddings for each chunk
5. **Enrich** — Convert linked chunks into SemanticDefinitions on referenced entities
6. **Refresh** — Detect and propagate document changes via content hashing

### 4. Confidence & Freshness System

Every SemanticDefinition has a confidence score reflecting trustworthiness and currency.

| Source | Initial Confidence | Rationale |
|--------|-------------------|-----------|
| Human edit | 0.95 | Direct human input, highest trust |
| Agent correction | 0.90 | Human corrected an agent in-context |
| dbt YAML | 0.80 | Maintained by data team, version-controlled |
| Column comment | 0.70 | Often stale but better than nothing |
| Document-linked | 0.65 | Depends on link quality and doc recency |
| Usage-inferred | 0.50 | Statistical signal, no explicit validation |
| LLM-drafted | 0.40 | Generated guess, needs validation |

Confidence decays exponentially over time. Adapter syncs reset the decay clock for unchanged definitions. The feedback loop (via `report_outcome`) boosts confidence for definitions used in successful queries.

### 5. Context Recommender

Given a natural language question, returns the minimal metadata an agent needs:

1. **Embed the query** using the same model as stored embeddings
2. **Hybrid search** — BM25 full-text + dense cosine similarity, fused via Reciprocal Rank Fusion
3. **Lineage expansion** — include direct upstream/downstream tables (1 hop)
4. **Confidence weighting** — higher-confidence results rank higher
5. **Token budget** — serialize top-N results that fit within configurable budget

### 6. MCP Server

The primary interface. Exposes 6 tools:

| Tool | Description |
|------|-------------|
| `get_context` | Semantic search for relevant tables, columns, and definitions |
| `describe_table` | Full metadata for a specific table including columns and lineage |
| `estimate_cost` | Pre-execution Snowflake cost estimate for a SQL query |
| `update_context` | Submit corrections or enrichments for entity definitions |
| `find_similar_tables` | Detect duplicate/redundant tables via semantic similarity |
| `report_outcome` | Report query success/failure to adjust confidence scores |

Supports both **stdio** transport (for Claude Desktop/Code) and **HTTP/SSE** transport (for multi-user team deployments).

---

## Technical Decisions

**Why Rust?**
- Sub-100ms MCP response latency requirement
- Embedding index and lineage graph traversal benefit from memory control
- Single binary deployment — no JVM, no Python runtime
- Consistency with modern data tooling (DataFusion, Polars, dbt Fusion)

**Why SQLite?**
- Zero-config local dev: `arcana init` just works
- Single-file persistence, easy to back up and move
- FTS5 for full-text search built in
- PostgreSQL planned for team/production concurrent access

**Why not extend OpenMetadata/DataHub?**
- Java monoliths designed for human UI consumption
- Bloated API payloads with UI-specific fields
- Not designed for token-efficient, embedding-powered, MCP-native responses

---

## Deployment Options

### Local (single user)
```bash
arcana --config arcana.toml serve --stdio
```
Wire directly to Claude Code or Claude Desktop via MCP config.

### Docker (team)
```bash
docker compose up -d
```
HTTP/SSE transport, shared metadata, shared feedback loop. Multiple engineers connect to one instance.

### CI/CD Integration
GitHub Action triggers `POST /api/sync` on dbt model changes. Background sync worker runs on a configurable interval with optional auto-enrich and auto-reembed.
