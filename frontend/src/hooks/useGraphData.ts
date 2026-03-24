import { useState, useEffect, useMemo, useRef, useCallback } from "react";
import type { GraphData, GraphNode, GraphLink, SchemaInfo, Stats, TableDetail } from "../types";

const POLL_INTERVAL = 3000;

export function useGraphData() {
  const [allData, setAllData] = useState<GraphData | null>(null);
  const [schemas, setSchemas] = useState<SchemaInfo[]>([]);
  const [stats, setStats] = useState<Stats | null>(null);
  const [details, setDetails] = useState<Record<string, TableDetail>>({});
  const [selectedSchemas, setSelectedSchemas] = useState<Set<string>>(new Set(["common", "common_prep"]));
  const [loading, setLoading] = useState(true);
  const versionRef = useRef<string>("");

  const fetchAll = useCallback(async () => {
    try {
      const [graph, schemasData, statsData, detailsData] = await Promise.all([
        fetch("/api/graph").then((r) => r.json()),
        fetch("/api/schemas").then((r) => r.json()),
        fetch("/api/stats").then((r) => r.json()),
        fetch("/api/details").then((r) => r.json()),
      ]);
      setAllData(graph);
      setSchemas(schemasData);
      setStats(statsData);
      setDetails(detailsData);
      versionRef.current = statsData.data_version || "";
      setLoading(false);
    } catch {
      // API not available — fall back to static files
      const [graph, schemasData, statsData, detailsData] = await Promise.all([
        fetch("/data/graph.json").then((r) => r.json()),
        fetch("/data/schemas.json").then((r) => r.json()),
        fetch("/data/stats.json").then((r) => r.json()),
        fetch("/data/details.json").then((r) => r.json()),
      ]);
      setAllData(graph);
      setSchemas(schemasData);
      setStats(statsData);
      setDetails(detailsData);
      setLoading(false);
    }
  }, []);

  // Initial load
  useEffect(() => {
    fetchAll();
  }, [fetchAll]);

  // Poll for changes
  useEffect(() => {
    const interval = setInterval(async () => {
      try {
        const statsData = await fetch("/api/stats").then((r) => r.json());
        const newVersion = statsData.data_version || "";
        if (newVersion && newVersion !== versionRef.current) {
          // Data changed — re-fetch everything
          setStats(statsData);
          const [graph, schemasData, detailsData] = await Promise.all([
            fetch("/api/graph").then((r) => r.json()),
            fetch("/api/schemas").then((r) => r.json()),
            fetch("/api/details").then((r) => r.json()),
          ]);
          setAllData(graph);
          setSchemas(schemasData);
          setDetails(detailsData);
          versionRef.current = newVersion;
        }
      } catch {
        // API unavailable, skip this poll
      }
    }, POLL_INTERVAL);

    return () => clearInterval(interval);
  }, []);

  const filteredData = useMemo(() => {
    if (!allData) return { nodes: [], links: [] };

    const nodeIds = new Set<string>();
    const nodes = allData.nodes.filter((n: GraphNode) => {
      if (selectedSchemas.has(n.schema)) {
        nodeIds.add(n.id);
        return true;
      }
      return false;
    });

    const links = allData.links.filter((l: GraphLink) => {
      const sourceId = typeof l.source === "string" ? l.source : l.source.id;
      const targetId = typeof l.target === "string" ? l.target : l.target.id;
      return nodeIds.has(sourceId) && nodeIds.has(targetId);
    });

    return { nodes, links };
  }, [allData, selectedSchemas]);

  return {
    graphData: filteredData,
    allNodes: allData?.nodes ?? [],
    schemas,
    stats,
    details,
    selectedSchemas,
    setSelectedSchemas,
    loading,
  };
}
