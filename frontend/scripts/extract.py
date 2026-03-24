#!/usr/bin/env python3
"""Extract graph data from Arcana SQLite database into static JSON files."""

import json
import sqlite3
import sys
import os
from pathlib import Path

DB_PATH = os.environ.get("ARCANA_DB", str(Path(__file__).resolve().parent.parent.parent / "sandbox" / "arcana-gitlab.db"))
OUT_DIR = Path(__file__).resolve().parent.parent / "public" / "data"

# dbt layer classification
LAYER_ORDER = ["source", "staging", "intermediate", "mart", "exposure"]


def classify_layer(name: str, schema: str, has_upstream: bool, has_downstream: bool) -> str:
    """Classify a table into a dbt layer based on naming conventions and lineage position."""
    name_lower = name.lower()
    schema_lower = schema.lower()

    # Explicit source indicators
    if "source" in schema_lower or name_lower.startswith("source_"):
        return "source"

    # Staging: prep schemas, stg_ prefix
    if "_prep" in schema_lower or name_lower.startswith("stg_") or name_lower.startswith("prep_"):
        return "staging"

    # Marts: mart schemas, dim_/fct_ prefixes
    if "_mart" in schema_lower or "mart" in schema_lower.split("_"):
        return "mart"
    if name_lower.startswith("dim_") or name_lower.startswith("fct_") or name_lower.startswith("rpt_"):
        return "mart"

    # Exposure: mart-layer tables with no downstream
    if not has_downstream and has_upstream:
        # Leaf nodes that aren't sources
        if "_mart" in schema_lower or name_lower.startswith("dim_") or name_lower.startswith("fct_"):
            return "exposure"

    # Mapping schemas → intermediate
    if "_mapping" in schema_lower:
        return "intermediate"

    # Fallback: use lineage position
    if not has_upstream:
        return "source"
    if not has_downstream:
        return "mart"

    return "intermediate"


def main():
    if not Path(DB_PATH).exists():
        print(f"Database not found: {DB_PATH}")
        sys.exit(1)

    OUT_DIR.mkdir(parents=True, exist_ok=True)
    conn = sqlite3.connect(DB_PATH)
    conn.row_factory = sqlite3.Row

    # --- stats.json ---
    stats = {}
    for table, key in [("tables", "tables"), ("columns", "columns"),
                        ("semantic_definitions", "definitions"),
                        ("lineage_edges", "lineage_edges"), ("schemas", "schemas")]:
        row = conn.execute(f"SELECT COUNT(*) as c FROM {table}").fetchone()
        stats[key] = row["c"]
    stats["embedded"] = conn.execute(
        "SELECT COUNT(*) as c FROM semantic_definitions WHERE embedding IS NOT NULL AND length(embedding) > 2"
    ).fetchone()["c"]
    write_json("stats.json", stats)

    # --- schemas.json ---
    schemas = []
    for row in conn.execute("""
        SELECT s.id, s.schema_name, s.database_name,
               COUNT(DISTINCT t.id) as table_count,
               COUNT(DISTINCT sd.id) as definition_count
        FROM schemas s
        LEFT JOIN tables t ON t.schema_id = s.id
        LEFT JOIN semantic_definitions sd ON sd.entity_id = t.id
        GROUP BY s.id
        ORDER BY table_count DESC
    """):
        tc = row["table_count"]
        dc = row["definition_count"]
        schemas.append({
            "id": row["id"],
            "name": row["schema_name"],
            "database": row["database_name"],
            "table_count": tc,
            "definition_count": dc,
            "coverage": round(dc / tc, 2) if tc > 0 else 0,
        })
    write_json("schemas.json", schemas)

    # --- graph.json ---
    # Build nodes: every table with metadata
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

    # Build links: lineage edges
    links = []
    for row in conn.execute("SELECT upstream_id, downstream_id FROM lineage_edges"):
        if row["upstream_id"] in table_ids and row["downstream_id"] in table_ids:
            links.append({
                "source": row["upstream_id"],
                "target": row["downstream_id"],
            })

    # Add edge_count and layer to each node
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

    # Print layer distribution
    from collections import Counter
    layer_counts = Counter(n["layer"] for n in nodes)
    print(f"  Layers: {dict(layer_counts)}")

    write_json("graph.json", {"nodes": nodes, "links": links})

    # --- details.json ---
    # Keyed by table ID, contains columns + definitions
    details = {}
    for node in nodes:
        tid = node["id"]
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

        # Lineage neighbors
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

        details[tid] = {
            "columns": cols,
            "upstream": upstream,
            "downstream": downstream,
        }

    write_json("details.json", details)

    conn.close()

    print(f"Extracted: {len(nodes)} nodes, {len(links)} links")
    print(f"Schemas: {len(schemas)}")
    print(f"Output: {OUT_DIR}")


def write_json(filename, data):
    path = OUT_DIR / filename
    with open(path, "w") as f:
        json.dump(data, f, separators=(",", ":"))
    size_mb = path.stat().st_size / 1024 / 1024
    print(f"  {filename}: {size_mb:.1f} MB")


if __name__ == "__main__":
    main()
