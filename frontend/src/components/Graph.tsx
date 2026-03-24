import { useRef, useCallback, useEffect, useState, useMemo } from "react";
import ForceGraph3D from "react-force-graph-3d";
import * as THREE from "three";
import type { GraphNode, GraphData } from "../types";
import { layerColor } from "../utils/colors";

interface GraphProps {
  data: GraphData;
  highlightIds: Set<string>;
  selectedNode: GraphNode | null;
  onNodeClick: (node: GraphNode) => void;
  focusNodeId: string | null;
  onFocusHandled: () => void;
}

// dbt layer ordering for DAG funnel layout
const LAYER_ORDER: Record<string, number> = {
  source: 0,
  staging: 1,
  intermediate: 2,
  mart: 3,
  exposure: 4,
};

const LAYER_LABELS: Record<string, string> = {
  source: "Sources",
  staging: "Staging",
  intermediate: "Intermediate",
  mart: "Marts",
  exposure: "Exposures",
};

export function Graph({ data, highlightIds, selectedNode, onNodeClick, focusNodeId, onFocusHandled }: GraphProps) {
  const fgRef = useRef<any>(null);
  const hasSearch = highlightIds.size > 0;
  const [hoverNode, setHoverNode] = useState<GraphNode | null>(null);

  // Build adjacency map
  const neighborMap = useMemo(() => {
    const map = new Map<string, Set<string>>();
    for (const link of data.links) {
      const srcId = typeof link.source === "string" ? link.source : (link.source as GraphNode).id;
      const tgtId = typeof link.target === "string" ? link.target : (link.target as GraphNode).id;
      if (!map.has(srcId)) map.set(srcId, new Set());
      if (!map.has(tgtId)) map.set(tgtId, new Set());
      map.get(srcId)!.add(tgtId);
      map.get(tgtId)!.add(srcId);
    }
    return map;
  }, [data.links]);

  // Layer index for each node
  const layerMap = useMemo(() => {
    const map = new Map<string, number>();
    for (const n of data.nodes) {
      map.set(n.id, LAYER_ORDER[n.layer] ?? 2);
    }
    return map;
  }, [data.nodes]);

  // Hub threshold: top 15 most connected
  const hubThreshold = useMemo(() => {
    const edgeCounts = data.nodes.map((n) => n.edge_count).sort((a, b) => b - a);
    return edgeCounts[Math.min(15, edgeCounts.length - 1)] || 0;
  }, [data.nodes]);

  const hubIds = useMemo(() => {
    const set = new Set<string>();
    for (const n of data.nodes) {
      if (n.edge_count >= hubThreshold && hubThreshold > 0) set.add(n.id);
    }
    return set;
  }, [data.nodes, hubThreshold]);

  useEffect(() => {
    if (focusNodeId && fgRef.current) {
      const node = data.nodes.find((n) => n.id === focusNodeId);
      if (node && (node as any).x !== undefined) {
        fgRef.current.cameraPosition(
          { x: (node as any).x, y: (node as any).y, z: (node as any).z + 80 },
          { x: (node as any).x, y: (node as any).y, z: (node as any).z },
          500
        );
        onFocusHandled();
      }
    }
  }, [focusNodeId, data.nodes, onFocusHandled]);

  // DAG layout: pin Y based on dbt layer, funnel shape with width constraints
  useEffect(() => {
    if (fgRef.current) {
      const fg = fgRef.current;
      fg.d3Force("charge")?.strength(-120);
      fg.d3Force("link")?.distance(40);

      const LAYER_SPACING = 150;
      // Funnel widths: sources spread wide, marts tight
      const LAYER_RADIUS: Record<number, number> = {
        0: 300,  // sources — widest
        1: 220,  // staging
        2: 160,  // intermediate
        3: 100,  // marts — tightest
        4: 60,   // exposures
      };

      for (const node of data.nodes as any[]) {
        const layerIdx = layerMap.get(node.id) ?? 2;
        node.fy = -layerIdx * LAYER_SPACING;

        // Initialize X/Z within funnel radius for this layer
        const radius = LAYER_RADIUS[layerIdx] ?? 160;
        if (node.x === undefined || node.x === 0) {
          const angle = Math.random() * Math.PI * 2;
          const r = Math.random() * radius;
          node.x = Math.cos(angle) * r;
          node.z = Math.sin(angle) * r;
        }
      }

      // Custom force to constrain X/Z within funnel radius per layer
      fg.d3Force("funnel", () => {
        for (const node of data.nodes as any[]) {
          const layerIdx = layerMap.get(node.id) ?? 2;
          const maxR = LAYER_RADIUS[layerIdx] ?? 160;
          const dist = Math.sqrt((node.x || 0) ** 2 + (node.z || 0) ** 2);
          if (dist > maxR) {
            const scale = maxR / dist;
            node.x *= scale;
            node.z *= scale;
          }
        }
      });
    }
  }, [data, layerMap]);

  const nodeColor = useCallback(
    (node: any) => {
      const n = node as GraphNode;
      const isHovered = hoverNode?.id === n.id;
      const isNeighbor = hoverNode ? (neighborMap.get(hoverNode.id)?.has(n.id) ?? false) : false;
      const color = layerColor(n.layer);

      if (hasSearch) {
        return highlightIds.has(n.id) ? color : "rgba(40,40,50,0.2)";
      }
      if (hoverNode) {
        if (isHovered || isNeighbor) return color;
        return "rgba(40,40,50,0.2)";
      }
      return color;
    },
    [hoverNode, hasSearch, highlightIds, neighborMap]
  );

  const nodeVal = useCallback((node: any) => {
    const n = node as GraphNode;
    return Math.max(1, Math.log2(n.column_count + 1) * 0.8);
  }, []);

  // Hub labels as canvas-texture sprites
  const nodeThreeObject = useCallback(
    (node: any) => {
      const n = node as GraphNode;
      if (!hubIds.has(n.id)) return undefined;

      const label = n.name;
      const color = layerColor(n.layer);
      const fontSize = 48;
      const canvas = document.createElement("canvas");
      const ctx = canvas.getContext("2d")!;
      ctx.font = `600 ${fontSize}px Inter, system-ui, sans-serif`;
      const textWidth = ctx.measureText(label).width;
      const padding = 16;
      canvas.width = textWidth + padding * 2;
      canvas.height = fontSize + padding * 2;

      // Background
      ctx.fillStyle = "rgba(0,0,0,0.7)";
      ctx.beginPath();
      ctx.roundRect(0, 0, canvas.width, canvas.height, 8);
      ctx.fill();

      // Text
      ctx.font = `600 ${fontSize}px Inter, system-ui, sans-serif`;
      ctx.fillStyle = color;
      ctx.textAlign = "center";
      ctx.textBaseline = "middle";
      ctx.fillText(label, canvas.width / 2, canvas.height / 2);

      const texture = new THREE.CanvasTexture(canvas);
      const material = new THREE.SpriteMaterial({ map: texture, transparent: true });
      const sprite = new THREE.Sprite(material);
      const scale = canvas.width / canvas.height;
      sprite.scale.set(scale * 5, 5, 1);
      sprite.position.y = 5;
      return sprite;
    },
    [hubIds]
  );

  const nodeLabel = useCallback((node: any) => {
    const n = node as GraphNode;
    const layerLabel = LAYER_LABELS[n.layer] || n.layer;
    const def = n.definition_preview
      ? `<br/><span style="color:#999;font-size:11px">${n.definition_preview.slice(0, 120)}</span>`
      : "";
    return `<div style="background:rgba(0,0,0,0.9);color:#fff;padding:8px 12px;border-radius:6px;font-family:Inter,system-ui;font-size:13px;max-width:300px;border-left:3px solid ${layerColor(n.layer)}">
      <span style="color:${layerColor(n.layer)};font-size:11px">${n.schema}</span>
      <span style="color:#666;font-size:10px;float:right">${layerLabel}</span><br/>
      <strong>${n.name}</strong><br/>
      <span style="color:#aaa;font-size:11px">${n.column_count} cols &middot; ${n.edge_count} edges &middot; ${(n.confidence * 100).toFixed(0)}% confidence</span>
      ${def}
    </div>`;
  }, []);

  const linkColor = useCallback(
    (link: any) => {
      if (hoverNode) {
        const srcId = typeof link.source === "string" ? link.source : link.source?.id;
        const tgtId = typeof link.target === "string" ? link.target : link.target?.id;
        if (srcId === hoverNode.id || tgtId === hoverNode.id) {
          return layerColor((link.source as GraphNode)?.layer || "intermediate");
        }
        return "rgba(255,255,255,0.02)";
      }
      if (hasSearch) {
        const srcId = typeof link.source === "string" ? link.source : link.source?.id;
        const tgtId = typeof link.target === "string" ? link.target : link.target?.id;
        if (highlightIds.has(srcId) && highlightIds.has(tgtId)) return "rgba(200,200,255,0.5)";
        return "rgba(255,255,255,0.02)";
      }
      return "rgba(255,255,255,0.1)";
    },
    [hoverNode, hasSearch, highlightIds]
  );

  const linkWidth = useCallback(
    (link: any) => {
      if (!hoverNode) return 0.3;
      const srcId = typeof link.source === "string" ? link.source : link.source?.id;
      const tgtId = typeof link.target === "string" ? link.target : link.target?.id;
      if (srcId === hoverNode.id || tgtId === hoverNode.id) return 1.5;
      return 0.1;
    },
    [hoverNode]
  );

  const linkParticles = useCallback(
    (link: any) => {
      if (!hoverNode) return 0;
      const srcId = typeof link.source === "string" ? link.source : link.source?.id;
      const tgtId = typeof link.target === "string" ? link.target : link.target?.id;
      return (srcId === hoverNode.id || tgtId === hoverNode.id) ? 2 : 0;
    },
    [hoverNode]
  );

  const handleNodeHover = useCallback((node: any) => {
    setHoverNode(node ? (node as GraphNode) : null);
    document.body.style.cursor = node ? "pointer" : "default";
  }, []);

  return (
    <div className="graph-container">
      <ForceGraph3D
        ref={fgRef}
        graphData={data}
        nodeId="id"
        nodeColor={nodeColor}
        nodeVal={nodeVal}
        nodeLabel={nodeLabel}
        nodeOpacity={0.9}
        nodeResolution={12}
        nodeThreeObject={nodeThreeObject}
        nodeThreeObjectExtend={true}
        linkColor={linkColor}
        linkWidth={linkWidth}
        linkOpacity={0.6}
        linkDirectionalArrowLength={3}
        linkDirectionalArrowRelPos={0.95}
        linkDirectionalArrowColor={linkColor}
        linkDirectionalParticles={linkParticles}
        linkDirectionalParticleWidth={1}
        linkDirectionalParticleSpeed={0.006}
        linkDirectionalParticleColor={linkColor}
        onNodeClick={(node: any) => onNodeClick(node as GraphNode)}
        onNodeHover={handleNodeHover}
        backgroundColor="#0a0a12"
        showNavInfo={false}
        cooldownTicks={100}
        warmupTicks={50}
      />
      {data.nodes.length === 0 && (
        <div className="graph-empty">
          Select one or more schemas to visualize
        </div>
      )}
    </div>
  );
}
