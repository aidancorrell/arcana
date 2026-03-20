-- FTS5 virtual table for hybrid (BM25 + dense) search over semantic definitions.
-- Uses content= to avoid duplicating the text; triggers keep it in sync.

CREATE VIRTUAL TABLE IF NOT EXISTS semantic_definitions_fts
USING fts5(
    definition,
    entity_id UNINDEXED,
    content='semantic_definitions',
    content_rowid='rowid'
);

-- Backfill from existing rows
INSERT INTO semantic_definitions_fts(rowid, definition, entity_id)
SELECT rowid, definition, entity_id FROM semantic_definitions;

-- Keep FTS in sync with the source table
CREATE TRIGGER IF NOT EXISTS sdef_fts_insert
AFTER INSERT ON semantic_definitions BEGIN
    INSERT INTO semantic_definitions_fts(rowid, definition, entity_id)
    VALUES (new.rowid, new.definition, new.entity_id);
END;

CREATE TRIGGER IF NOT EXISTS sdef_fts_delete
AFTER DELETE ON semantic_definitions BEGIN
    INSERT INTO semantic_definitions_fts(semantic_definitions_fts, rowid, definition, entity_id)
    VALUES ('delete', old.rowid, old.definition, old.entity_id);
END;

CREATE TRIGGER IF NOT EXISTS sdef_fts_update
AFTER UPDATE ON semantic_definitions BEGIN
    INSERT INTO semantic_definitions_fts(semantic_definitions_fts, rowid, definition, entity_id)
    VALUES ('delete', old.rowid, old.definition, old.entity_id);
    INSERT INTO semantic_definitions_fts(rowid, definition, entity_id)
    VALUES (new.rowid, new.definition, new.entity_id);
END;
