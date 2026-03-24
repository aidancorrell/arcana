import type { GraphNode, TableDetail } from "../types";
import { layerColor, confidenceColor } from "../utils/colors";

interface NodeDetailProps {
  node: GraphNode | null;
  detail: TableDetail | null;
  onClose: () => void;
  onNavigate: (id: string) => void;
}

function ConfidenceBadge({ confidence }: { confidence: number }) {
  return (
    <span
      className="confidence-badge"
      style={{ backgroundColor: confidenceColor(confidence) + "30", color: confidenceColor(confidence) }}
    >
      {(confidence * 100).toFixed(0)}%
    </span>
  );
}

export function NodeDetail({ node, detail, onClose, onNavigate }: NodeDetailProps) {
  if (!node) return null;

  return (
    <div className="detail-panel">
      <div className="detail-header">
        <div>
          <span className="detail-schema" style={{ color: layerColor(node.layer) }}>
            {node.schema}
          </span>
          <h2 className="detail-name">{node.name}</h2>
        </div>
        <button className="detail-close" onClick={onClose}>&times;</button>
      </div>

      <div className="detail-meta">
        <div className="meta-item">
          <span className="meta-label">Layer</span>
          <span className="meta-value" style={{ textTransform: "capitalize" }}>{node.layer}</span>
        </div>
        <div className="meta-item">
          <span className="meta-label">Columns</span>
          <span className="meta-value">{node.column_count}</span>
        </div>
        <div className="meta-item">
          <span className="meta-label">Lineage</span>
          <span className="meta-value">{node.edge_count} edges</span>
        </div>
        <div className="meta-item">
          <span className="meta-label">Confidence</span>
          <ConfidenceBadge confidence={node.confidence} />
        </div>
      </div>

      {node.definition_preview && (
        <div className="detail-section">
          <h3>Definition</h3>
          <p className="detail-definition">{node.definition_preview}</p>
        </div>
      )}

      {detail && detail.upstream.length > 0 && (
        <div className="detail-section">
          <h3>Upstream ({detail.upstream.length})</h3>
          <div className="lineage-list">
            {detail.upstream.map((t) => (
              <button key={t.id} className="lineage-item" onClick={() => onNavigate(t.id)}>
                <span className="lineage-arrow">&larr;</span>
                <span style={{ color: "var(--text-dim)" }}>{t.schema}.</span>
                {t.name}
              </button>
            ))}
          </div>
        </div>
      )}

      {detail && detail.downstream.length > 0 && (
        <div className="detail-section">
          <h3>Downstream ({detail.downstream.length})</h3>
          <div className="lineage-list">
            {detail.downstream.map((t) => (
              <button key={t.id} className="lineage-item" onClick={() => onNavigate(t.id)}>
                <span className="lineage-arrow">&rarr;</span>
                <span style={{ color: "var(--text-dim)" }}>{t.schema}.</span>
                {t.name}
              </button>
            ))}
          </div>
        </div>
      )}

      {detail && detail.columns.length > 0 && (
        <div className="detail-section">
          <h3>Columns ({detail.columns.length})</h3>
          <div className="columns-list">
            {detail.columns.slice(0, 50).map((col) => (
              <div key={col.name} className="column-item">
                <div className="column-header">
                  <code className="column-name">{col.name}</code>
                  <span className="column-type">{col.type || "unknown"}</span>
                </div>
                {col.definition && (
                  <p className="column-def">{col.definition}</p>
                )}
              </div>
            ))}
            {detail.columns.length > 50 && (
              <p className="columns-more">+ {detail.columns.length - 50} more columns</p>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
