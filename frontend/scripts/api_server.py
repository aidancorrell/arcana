#!/usr/bin/env python3
"""Live API server for Arcana frontend — reads directly from SQLite."""

import os
import sqlite3
from pathlib import Path
from fastapi import FastAPI
from fastapi.middleware.gzip import GZipMiddleware

DB_PATH = os.environ.get(
    "ARCANA_DB",
    str(Path(__file__).resolve().parent.parent.parent / "sandbox" / "arcana-gitlab.db"),
)

app = FastAPI(title="Arcana Frontend API")
app.add_middleware(GZipMiddleware, minimum_size=500)


def get_conn():
    conn = sqlite3.connect(f"file:{DB_PATH}?mode=ro", uri=True, check_same_thread=False)
    conn.row_factory = sqlite3.Row
    return conn


# --- Layer classification (same as extract.py) ---

def classify_layer(name: str, schema: str, has_upstream: bool, has_downstream: bool) -> str:
    name_lower = name.lower()
    schema_lower = schema.lower()

    if "source" in schema_lower or name_lower.startswith("source_"):
        return "source"
    if "_prep" in schema_lower or name_lower.startswith("stg_") or name_lower.startswith("prep_"):
        return "staging"
    if "_mart" in schema_lower or "mart" in schema_lower.split("_"):
        return "mart"
    if name_lower.startswith("dim_") or name_lower.startswith("fct_") or name_lower.startswith("rpt_"):
        return "mart"
    if "_mapping" in schema_lower:
        return "intermediate"
    if not has_upstream:
        return "source"
    if not has_downstream:
        return "mart"
    return "intermediate"


# --- Endpoints ---

@app.get("/api/stats")
def stats():
    conn = get_conn()
    result = {}
    for table, key in [
        ("tables", "tables"), ("columns", "columns"),
        ("semantic_definitions", "definitions"),
        ("lineage_edges", "lineage_edges"), ("schemas", "schemas"),
    ]:
        row = conn.execute(f"SELECT COUNT(*) as c FROM {table}").fetchone()
        result[key] = row["c"]
    result["embedded"] = conn.execute(
        "SELECT COUNT(*) as c FROM semantic_definitions WHERE embedding IS NOT NULL AND length(embedding) > 2"
    ).fetchone()["c"]

    # Data version: latest update timestamp
    row = conn.execute(
        "SELECT MAX(updated_at) as v FROM semantic_definitions"
    ).fetchone()
    result["data_version"] = row["v"] or ""

    conn.close()
    return result


@app.get("/api/schemas")
def schemas():
    conn = get_conn()
    rows = conn.execute("""
        SELECT s.id, s.schema_name, s.database_name,
               COUNT(DISTINCT t.id) as table_count,
               COUNT(DISTINCT sd.id) as definition_count
        FROM schemas s
        LEFT JOIN tables t ON t.schema_id = s.id
        LEFT JOIN semantic_definitions sd ON sd.entity_id = t.id
        GROUP BY s.id
        ORDER BY table_count DESC
    """).fetchall()
    result = []
    for row in rows:
        tc = row["table_count"]
        dc = row["definition_count"]
        result.append({
            "id": row["id"],
            "name": row["schema_name"],
            "database": row["database_name"],
            "table_count": tc,
            "definition_count": dc,
            "coverage": round(dc / tc, 2) if tc > 0 else 0,
        })
    conn.close()
    return result


@app.get("/api/graph")
def graph():
    conn = get_conn()

    nodes = []
    table_ids = set()
    for row in conn.execute("""
        SELECT t.id, t.name, t.table_type, t.schema_id,
               s.schema_name,
               (SELECT COUNT(*) FROM columns c WHERE c.table_id = t.id) as column_count,
               sd.definition as def_text,
               sd.confidence as confidence
        FROM tables t
        JOIN schemas s ON t.schema_id = s.id
        LEFT JOIN semantic_definitions sd ON sd.entity_id = t.id
        ORDER BY t.name
    """):
        tid = row["id"]
        table_ids.add(tid)
        nodes.append({
            "id": tid,
            "name": row["name"],
            "schema": row["schema_name"],
            "schema_id": row["schema_id"],
            "column_count": row["column_count"],
            "has_definition": row["def_text"] is not None,
            "definition_preview": (row["def_text"][:200] + "...") if row["def_text"] and len(row["def_text"]) > 200 else row["def_text"],
            "confidence": row["confidence"] or 0,
            "table_type": row["table_type"],
        })

    links = []
    for row in conn.execute("SELECT upstream_id, downstream_id FROM lineage_edges"):
        if row["upstream_id"] in table_ids and row["downstream_id"] in table_ids:
            links.append({"source": row["upstream_id"], "target": row["downstream_id"]})

    # Edge counts + layer classification
    edge_counts = {}
    has_upstream = set()
    has_downstream = set()
    for link in links:
        edge_counts[link["source"]] = edge_counts.get(link["source"], 0) + 1
        edge_counts[link["target"]] = edge_counts.get(link["target"], 0) + 1
        has_downstream.add(link["source"])
        has_upstream.add(link["target"])

    for node in nodes:
        node["edge_count"] = edge_counts.get(node["id"], 0)
        node["layer"] = classify_layer(
            node["name"], node["schema"],
            node["id"] in has_upstream, node["id"] in has_downstream,
        )

    conn.close()
    return {"nodes": nodes, "links": links}


@app.get("/api/details")
def details():
    conn = get_conn()

    table_ids = [row["id"] for row in conn.execute("SELECT id FROM tables")]
    result = {}

    for tid in table_ids:
        cols = []
        for col in conn.execute("""
            SELECT c.name, c.data_type, sd.definition, sd.confidence
            FROM columns c
            LEFT JOIN semantic_definitions sd ON sd.entity_id = c.id
            WHERE c.table_id = ?
            ORDER BY c.ordinal_position
        """, (tid,)):
            cols.append({
                "name": col["name"],
                "type": col["data_type"],
                "definition": col["definition"],
                "confidence": col["confidence"],
            })

        upstream = []
        for row in conn.execute("""
            SELECT t.id, t.name, s.schema_name
            FROM lineage_edges le
            JOIN tables t ON t.id = le.upstream_id
            JOIN schemas s ON t.schema_id = s.id
            WHERE le.downstream_id = ?
        """, (tid,)):
            upstream.append({"id": row["id"], "name": row["name"], "schema": row["schema_name"]})

        downstream = []
        for row in conn.execute("""
            SELECT t.id, t.name, s.schema_name
            FROM lineage_edges le
            JOIN tables t ON t.id = le.downstream_id
            JOIN schemas s ON t.schema_id = s.id
            WHERE le.upstream_id = ?
        """, (tid,)):
            downstream.append({"id": row["id"], "name": row["name"], "schema": row["schema_name"]})

        result[tid] = {"columns": cols, "upstream": upstream, "downstream": downstream}

    conn.close()
    return result


if __name__ == "__main__":
    import uvicorn
    print(f"Arcana API server — reading from {DB_PATH}")
    uvicorn.run(app, host="127.0.0.1", port=8100)
