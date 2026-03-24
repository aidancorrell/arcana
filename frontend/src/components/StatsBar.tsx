import type { Stats } from "../types";

function formatNumber(n: number): string {
  if (n >= 1000) return `${(n / 1000).toFixed(1)}k`;
  return n.toString();
}

export function StatsBar({ stats }: { stats: Stats | null }) {
  if (!stats) return null;

  const items = [
    { label: "Tables", value: formatNumber(stats.tables) },
    { label: "Columns", value: formatNumber(stats.columns) },
    { label: "Definitions", value: formatNumber(stats.definitions) },
    { label: "Lineage Edges", value: formatNumber(stats.lineage_edges) },
    { label: "Embedded", value: formatNumber(stats.embedded) },
  ];

  return (
    <div className="stats-bar">
      {items.map((item) => (
        <div key={item.label} className="stat-item">
          <span className="stat-value">{item.value}</span>
          <span className="stat-label">{item.label}</span>
        </div>
      ))}
    </div>
  );
}
