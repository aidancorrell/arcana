-- Evidence records: track when definitions are used in successful/failed queries
CREATE TABLE IF NOT EXISTS evidence_records (
    id              TEXT PRIMARY KEY,
    entity_id       TEXT NOT NULL,
    interaction_id  TEXT REFERENCES agent_interactions(id) ON DELETE SET NULL,
    query_text      TEXT,
    outcome         TEXT NOT NULL CHECK (outcome IN ('success', 'failure')),
    source          TEXT NOT NULL CHECK (source IN ('agent_feedback', 'query_history', 'co_occurrence')),
    confidence_delta REAL NOT NULL DEFAULT 0.0,
    created_at      TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_evidence_entity_id ON evidence_records(entity_id);
CREATE INDEX IF NOT EXISTS idx_evidence_created_at ON evidence_records(created_at);

-- Embedding cache: hash of definition text to skip re-embedding unchanged definitions
ALTER TABLE semantic_definitions ADD COLUMN definition_hash TEXT;

-- Incremental sync: track checksums per adapter entity to skip unchanged models
CREATE TABLE IF NOT EXISTS sync_checksums (
    adapter     TEXT NOT NULL,
    entity_key  TEXT NOT NULL,
    checksum    TEXT NOT NULL,
    synced_at   TEXT NOT NULL,
    PRIMARY KEY (adapter, entity_key)
);
