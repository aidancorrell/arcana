-- Arcana initial schema migration
-- All UUIDs stored as TEXT; timestamps stored as TEXT (ISO 8601).

PRAGMA foreign_keys = ON;

-- -------------------------------------------------------------------------
-- Data Sources
-- -------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS data_sources (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    source_type     TEXT NOT NULL,
    connection_info TEXT NOT NULL DEFAULT '{}',
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);

-- -------------------------------------------------------------------------
-- Schemas
-- -------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS schemas (
    id              TEXT PRIMARY KEY,
    data_source_id  TEXT NOT NULL REFERENCES data_sources(id) ON DELETE CASCADE,
    database_name   TEXT NOT NULL,
    schema_name     TEXT NOT NULL,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    UNIQUE(data_source_id, database_name, schema_name)
);

-- -------------------------------------------------------------------------
-- Tables
-- -------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS tables (
    id                       TEXT PRIMARY KEY,
    schema_id                TEXT NOT NULL REFERENCES schemas(id) ON DELETE CASCADE,
    name                     TEXT NOT NULL,
    table_type               TEXT NOT NULL DEFAULT '"base_table"',
    description              TEXT,
    dbt_model                TEXT,
    owner                    TEXT,
    row_count                INTEGER,
    byte_size                INTEGER,
    confidence               REAL NOT NULL DEFAULT 1.0,
    confidence_refreshed_at  TEXT,
    tags                     TEXT NOT NULL DEFAULT '{}',
    created_at               TEXT NOT NULL,
    updated_at               TEXT NOT NULL,
    UNIQUE(schema_id, name)
);

CREATE INDEX IF NOT EXISTS idx_tables_schema_id ON tables(schema_id);
CREATE INDEX IF NOT EXISTS idx_tables_confidence ON tables(confidence);

-- FTS5 virtual table for full-text search on table names/descriptions
CREATE VIRTUAL TABLE IF NOT EXISTS tables_fts USING fts5(
    id UNINDEXED,
    name,
    description,
    content=tables,
    content_rowid=rowid
);

-- -------------------------------------------------------------------------
-- Columns
-- -------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS columns (
    id                       TEXT PRIMARY KEY,
    table_id                 TEXT NOT NULL REFERENCES tables(id) ON DELETE CASCADE,
    name                     TEXT NOT NULL,
    data_type                TEXT NOT NULL,
    ordinal_position         INTEGER NOT NULL,
    is_nullable              INTEGER NOT NULL DEFAULT 1,
    is_primary_key           INTEGER NOT NULL DEFAULT 0,
    is_foreign_key           INTEGER NOT NULL DEFAULT 0,
    description              TEXT,
    dbt_meta                 TEXT,
    tags                     TEXT NOT NULL DEFAULT '{}',
    confidence               REAL NOT NULL DEFAULT 1.0,
    confidence_refreshed_at  TEXT,
    created_at               TEXT NOT NULL,
    updated_at               TEXT NOT NULL,
    UNIQUE(table_id, name)
);

CREATE INDEX IF NOT EXISTS idx_columns_table_id ON columns(table_id);

-- -------------------------------------------------------------------------
-- Column Profiles
-- -------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS column_profiles (
    id              TEXT PRIMARY KEY,
    column_id       TEXT NOT NULL REFERENCES columns(id) ON DELETE CASCADE,
    null_count      INTEGER NOT NULL DEFAULT 0,
    null_pct        REAL NOT NULL DEFAULT 0.0,
    distinct_count  INTEGER,
    min_value       TEXT,
    max_value       TEXT,
    mean_value      REAL,
    stddev_value    REAL,
    top_values      TEXT,
    profiled_at     TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_column_profiles_column_id ON column_profiles(column_id);

-- -------------------------------------------------------------------------
-- Semantic Definitions
-- -------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS semantic_definitions (
    id                       TEXT PRIMARY KEY,
    entity_id                TEXT NOT NULL,
    entity_type              TEXT NOT NULL,
    definition               TEXT NOT NULL,
    source                   TEXT NOT NULL,
    confidence               REAL NOT NULL DEFAULT 1.0,
    confidence_refreshed_at  TEXT,
    embedding                TEXT,   -- JSON array of f32
    created_at               TEXT NOT NULL,
    updated_at               TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_semantic_definitions_entity_id ON semantic_definitions(entity_id);

-- -------------------------------------------------------------------------
-- Metrics
-- -------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS metrics (
    id               TEXT PRIMARY KEY,
    name             TEXT NOT NULL UNIQUE,
    label            TEXT,
    description      TEXT,
    metric_type      TEXT NOT NULL,
    source_table_id  TEXT REFERENCES tables(id) ON DELETE SET NULL,
    expression       TEXT,
    dimensions       TEXT NOT NULL DEFAULT '[]',
    filters          TEXT,
    confidence       REAL NOT NULL DEFAULT 1.0,
    created_at       TEXT NOT NULL,
    updated_at       TEXT NOT NULL
);

-- -------------------------------------------------------------------------
-- Data Contracts
-- -------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS data_contracts (
    id                  TEXT PRIMARY KEY,
    name                TEXT NOT NULL,
    entity_id           TEXT NOT NULL,
    entity_type         TEXT NOT NULL,
    contract_type       TEXT NOT NULL,
    description         TEXT,
    expression          TEXT NOT NULL DEFAULT '{}',
    status              TEXT NOT NULL DEFAULT '"active"',
    last_evaluated_at   TEXT,
    last_result         TEXT,
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_contracts_entity_id ON data_contracts(entity_id);

-- -------------------------------------------------------------------------
-- Lineage Edges
-- -------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS lineage_edges (
    id                     TEXT PRIMARY KEY,
    upstream_id            TEXT NOT NULL,
    upstream_type          TEXT NOT NULL,
    downstream_id          TEXT NOT NULL,
    downstream_type        TEXT NOT NULL,
    source                 TEXT NOT NULL,
    transform_expression   TEXT,
    confidence             REAL NOT NULL DEFAULT 1.0,
    created_at             TEXT NOT NULL,
    updated_at             TEXT NOT NULL,
    UNIQUE(upstream_id, downstream_id)
);

CREATE INDEX IF NOT EXISTS idx_lineage_upstream ON lineage_edges(upstream_id);
CREATE INDEX IF NOT EXISTS idx_lineage_downstream ON lineage_edges(downstream_id);

-- -------------------------------------------------------------------------
-- Documents
-- -------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS documents (
    id            TEXT PRIMARY KEY,
    title         TEXT NOT NULL,
    source_type   TEXT NOT NULL,
    source_uri    TEXT NOT NULL UNIQUE,
    raw_content   TEXT,
    content       TEXT NOT NULL,
    content_hash  TEXT NOT NULL,
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL
);

-- -------------------------------------------------------------------------
-- Document Chunks
-- -------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS document_chunks (
    id            TEXT PRIMARY KEY,
    document_id   TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    chunk_index   INTEGER NOT NULL,
    content       TEXT NOT NULL,
    char_start    INTEGER NOT NULL,
    char_end      INTEGER NOT NULL,
    section_path  TEXT NOT NULL DEFAULT '[]',
    embedding     TEXT,   -- JSON array of f32
    created_at    TEXT NOT NULL,
    UNIQUE(document_id, chunk_index)
);

CREATE INDEX IF NOT EXISTS idx_chunks_document_id ON document_chunks(document_id);

-- -------------------------------------------------------------------------
-- Entity Links
-- -------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS entity_links (
    id           TEXT PRIMARY KEY,
    chunk_id     TEXT NOT NULL REFERENCES document_chunks(id) ON DELETE CASCADE,
    entity_id    TEXT NOT NULL,
    entity_type  TEXT NOT NULL,
    link_method  TEXT NOT NULL,
    confidence   REAL NOT NULL DEFAULT 1.0,
    created_at   TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_entity_links_chunk_id ON entity_links(chunk_id);
CREATE INDEX IF NOT EXISTS idx_entity_links_entity_id ON entity_links(entity_id);

-- -------------------------------------------------------------------------
-- Usage Records
-- -------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS usage_records (
    id            TEXT PRIMARY KEY,
    table_id      TEXT NOT NULL REFERENCES tables(id) ON DELETE CASCADE,
    actor         TEXT,
    warehouse     TEXT,
    query_type    TEXT NOT NULL,
    bytes_scanned INTEGER,
    credits_used  REAL,
    duration_ms   INTEGER,
    executed_at   TEXT NOT NULL,
    created_at    TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_usage_table_id ON usage_records(table_id);
CREATE INDEX IF NOT EXISTS idx_usage_executed_at ON usage_records(executed_at);

-- -------------------------------------------------------------------------
-- Agent Interactions
-- -------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS agent_interactions (
    id                     TEXT PRIMARY KEY,
    tool_name              TEXT NOT NULL,
    input                  TEXT NOT NULL DEFAULT '{}',
    referenced_entity_ids  TEXT NOT NULL DEFAULT '[]',
    agent_id               TEXT,
    was_helpful            INTEGER,
    latency_ms             INTEGER,
    created_at             TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_agent_interactions_tool ON agent_interactions(tool_name);
CREATE INDEX IF NOT EXISTS idx_agent_interactions_created_at ON agent_interactions(created_at);
