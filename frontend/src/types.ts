export interface GraphNode {
  id: string;
  name: string;
  schema: string;
  schema_id: string;
  column_count: number;
  has_definition: boolean;
  definition_preview: string | null;
  confidence: number;
  table_type: string;
  edge_count: number;
  layer: "source" | "staging" | "intermediate" | "mart" | "exposure";
  // Force graph internal
  x?: number;
  y?: number;
  vx?: number;
  vy?: number;
}

export interface GraphLink {
  source: string | GraphNode;
  target: string | GraphNode;
}

export interface GraphData {
  nodes: GraphNode[];
  links: GraphLink[];
}

export interface SchemaInfo {
  id: string;
  name: string;
  database: string;
  table_count: number;
  definition_count: number;
  coverage: number;
}

export interface Stats {
  tables: number;
  columns: number;
  definitions: number;
  lineage_edges: number;
  schemas: number;
  embedded: number;
  data_version?: string;
}

export interface ColumnDetail {
  name: string;
  type: string;
  definition: string | null;
  confidence: number | null;
}

export interface LineageNeighbor {
  id: string;
  name: string;
  schema: string;
}

export interface TableDetail {
  columns: ColumnDetail[];
  upstream: LineageNeighbor[];
  downstream: LineageNeighbor[];
}
