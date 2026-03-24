import { useState, useMemo, useCallback } from "react";
import Fuse from "fuse.js";
import type { GraphNode } from "../types";

export function useSearch(allNodes: GraphNode[]) {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<GraphNode[]>([]);
  const [highlightIds, setHighlightIds] = useState<Set<string>>(new Set());

  const fuse = useMemo(() => {
    return new Fuse(allNodes, {
      keys: [
        { name: "name", weight: 2 },
        { name: "definition_preview", weight: 1 },
        { name: "schema", weight: 0.5 },
      ],
      threshold: 0.4,
      includeScore: true,
    });
  }, [allNodes]);

  const search = useCallback(
    (q: string) => {
      setQuery(q);
      if (q.trim().length < 2) {
        setResults([]);
        setHighlightIds(new Set());
        return;
      }
      const hits = fuse.search(q, { limit: 20 });
      const nodes = hits.map((h) => h.item);
      setResults(nodes);
      setHighlightIds(new Set(nodes.map((n) => n.id)));
    },
    [fuse]
  );

  const clearSearch = useCallback(() => {
    setQuery("");
    setResults([]);
    setHighlightIds(new Set());
  }, []);

  return { query, search, results, highlightIds, clearSearch };
}
