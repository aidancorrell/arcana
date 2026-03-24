import { useState, useCallback } from "react";
import { useGraphData } from "./hooks/useGraphData";
import { useSearch } from "./hooks/useSearch";
import { Graph } from "./components/Graph";
import { NodeDetail } from "./components/NodeDetail";
import { SearchBar } from "./components/SearchBar";
import { SchemaFilter } from "./components/SchemaFilter";
import { StatsBar } from "./components/StatsBar";
import type { GraphNode } from "./types";

export default function App() {
  const { graphData, allNodes, schemas, stats, details, selectedSchemas, setSelectedSchemas, loading } = useGraphData();
  const { query, search, results, highlightIds, clearSearch } = useSearch(allNodes);
  const [selectedNode, setSelectedNode] = useState<GraphNode | null>(null);
  const [focusNodeId, setFocusNodeId] = useState<string | null>(null);
  const [sidebarOpen, setSidebarOpen] = useState(true);

  const handleNodeClick = useCallback((node: GraphNode) => {
    setSelectedNode(node);
  }, []);

  const handleSearchSelect = useCallback((node: GraphNode) => {
    setSelectedNode(node);
    setFocusNodeId(node.id);
    if (!selectedSchemas.has(node.schema)) {
      setSelectedSchemas(new Set([...selectedSchemas, node.schema]));
    }
  }, [selectedSchemas, setSelectedSchemas]);

  const handleNavigate = useCallback((id: string) => {
    const node = allNodes.find((n) => n.id === id);
    if (node) {
      setSelectedNode(node);
      setFocusNodeId(node.id);
      if (!selectedSchemas.has(node.schema)) {
        setSelectedSchemas(new Set([...selectedSchemas, node.schema]));
      }
    }
  }, [allNodes, selectedSchemas, setSelectedSchemas]);

  const handleFocusHandled = useCallback(() => {
    setFocusNodeId(null);
  }, []);

  if (loading) {
    return (
      <div className="loading">
        <div className="loading-spinner" />
        <p>Loading metadata graph...</p>
      </div>
    );
  }

  return (
    <div className="app">
      <header className="header">
        <div className="header-left">
          <h1 className="logo">
            <span className="logo-icon">&#9670;</span> Arcana
          </h1>
          <StatsBar stats={stats} />
        </div>
        <SearchBar
          query={query}
          onSearch={search}
          onClear={clearSearch}
          results={results}
          onSelectNode={handleSearchSelect}
        />
      </header>

      <div className="main">
        <aside className={`sidebar ${sidebarOpen ? "" : "sidebar-collapsed"}`}>
          <button className="sidebar-toggle" onClick={() => setSidebarOpen(!sidebarOpen)}>
            {sidebarOpen ? "\u25C0 Hide" : "\u25B6"}
          </button>
          {sidebarOpen && (
            <>
              <SchemaFilter
                schemas={schemas}
                selected={selectedSchemas}
                onChange={setSelectedSchemas}
                allNodes={allNodes}
              />
              <div className="sidebar-info">
                <p>{graphData.nodes.length} tables &middot; {graphData.links.length} edges</p>
              </div>
            </>
          )}
        </aside>

        <div className="graph-wrapper">
          <Graph
            data={graphData}
            highlightIds={highlightIds}
            selectedNode={selectedNode}
            onNodeClick={handleNodeClick}
            focusNodeId={focusNodeId}
            onFocusHandled={handleFocusHandled}
          />
          <div className="layer-legend">
            <div className="legend-item"><span className="legend-dot" style={{ background: "#60a5fa" }} />Sources</div>
            <div className="legend-item"><span className="legend-dot" style={{ background: "#a78bfa" }} />Staging</div>
            <div className="legend-item"><span className="legend-dot" style={{ background: "#fbbf24" }} />Intermediate</div>
            <div className="legend-item"><span className="legend-dot" style={{ background: "#34d399" }} />Marts</div>
            <div className="legend-item"><span className="legend-dot" style={{ background: "#f87171" }} />Exposures</div>
          </div>
        </div>

        <NodeDetail
          node={selectedNode}
          detail={selectedNode ? details[selectedNode.id] ?? null : null}
          onClose={() => setSelectedNode(null)}
          onNavigate={handleNavigate}
        />
      </div>
    </div>
  );
}
