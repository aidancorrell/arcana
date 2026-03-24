import { useMemo } from "react";
import type { SchemaInfo, GraphNode } from "../types";
import { layerColor } from "../utils/colors";

interface SchemaFilterProps {
  schemas: SchemaInfo[];
  selected: Set<string>;
  onChange: (selected: Set<string>) => void;
  allNodes: GraphNode[];
}

export function SchemaFilter({ schemas, selected, onChange, allNodes }: SchemaFilterProps) {
  const topSchemas = schemas.filter((s) => s.table_count >= 5).slice(0, 30);

  // Compute dominant layer per schema
  const schemaLayerColor = useMemo(() => {
    const counts = new Map<string, Map<string, number>>();
    for (const n of allNodes) {
      if (!counts.has(n.schema)) counts.set(n.schema, new Map());
      const m = counts.get(n.schema)!;
      m.set(n.layer, (m.get(n.layer) || 0) + 1);
    }
    const result = new Map<string, string>();
    for (const [schema, layers] of counts) {
      let best = "intermediate";
      let bestCount = 0;
      for (const [layer, count] of layers) {
        if (count > bestCount) { best = layer; bestCount = count; }
      }
      result.set(schema, layerColor(best));
    }
    return result;
  }, [allNodes]);

  const toggle = (name: string) => {
    const next = new Set(selected);
    if (next.has(name)) {
      next.delete(name);
    } else {
      next.add(name);
    }
    onChange(next);
  };

  const selectAll = () => onChange(new Set(topSchemas.map((s) => s.name)));
  const selectNone = () => onChange(new Set());

  return (
    <div className="schema-filter">
      <div className="schema-filter-header">
        <span className="schema-filter-title">Schemas</span>
        <div className="schema-filter-actions">
          <button onClick={selectAll}>All</button>
          <button onClick={selectNone}>None</button>
        </div>
      </div>
      <div className="schema-filter-list">
        {topSchemas.map((s) => {
          const color = schemaLayerColor.get(s.name) || "#888";
          return (
            <button
              key={s.id}
              className={`schema-chip ${selected.has(s.name) ? "active" : ""}`}
              onClick={() => toggle(s.name)}
              style={{
                borderColor: selected.has(s.name) ? color : "transparent",
                backgroundColor: selected.has(s.name) ? color + "20" : undefined,
              }}
            >
              <span className="schema-dot" style={{ backgroundColor: color }} />
              <span className="schema-chip-name">{s.name}</span>
              <span className="schema-chip-count">{s.table_count}</span>
            </button>
          );
        })}
      </div>
    </div>
  );
}
