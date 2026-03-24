// Deterministic schema-to-color mapping using HSL
const colorCache = new Map<string, string>();

function hashString(str: string): number {
  let hash = 0;
  for (let i = 0; i < str.length; i++) {
    const char = str.charCodeAt(i);
    hash = ((hash << 5) - hash) + char;
    hash |= 0;
  }
  return Math.abs(hash);
}

export function schemaColor(schema: string): string {
  if (colorCache.has(schema)) return colorCache.get(schema)!;
  const hue = hashString(schema) % 360;
  const color = `hsl(${hue}, 70%, 60%)`;
  colorCache.set(schema, color);
  return color;
}

export function schemaColorDim(schema: string): string {
  const hue = hashString(schema) % 360;
  return `hsl(${hue}, 40%, 30%)`;
}

const LAYER_COLORS: Record<string, string> = {
  source: "#60a5fa",      // blue
  staging: "#a78bfa",     // purple
  intermediate: "#fbbf24", // amber
  mart: "#34d399",        // green
  exposure: "#f87171",    // red
};

export function layerColor(layer: string): string {
  return LAYER_COLORS[layer] || "#888";
}

export function confidenceColor(confidence: number): string {
  if (confidence >= 0.7) return "#22c55e"; // green
  if (confidence >= 0.4) return "#eab308"; // yellow
  return "#ef4444"; // red
}
