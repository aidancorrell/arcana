-- Redundancy detection: table clusters for dedup.
-- Each cluster groups semantically similar tables and designates one as canonical.

CREATE TABLE IF NOT EXISTS table_clusters (
    id              TEXT PRIMARY KEY,
    label           TEXT,
    canonical_id    TEXT REFERENCES tables(id) ON DELETE SET NULL,
    threshold       REAL NOT NULL,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS table_cluster_members (
    cluster_id  TEXT NOT NULL REFERENCES table_clusters(id) ON DELETE CASCADE,
    table_id    TEXT NOT NULL REFERENCES tables(id) ON DELETE CASCADE,
    similarity  REAL NOT NULL,
    PRIMARY KEY (cluster_id, table_id)
);

CREATE INDEX IF NOT EXISTS idx_cluster_members_table ON table_cluster_members(table_id);
