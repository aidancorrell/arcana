import type { GraphNode } from "../types";
import { layerColor } from "../utils/colors";

interface SearchBarProps {
  query: string;
  onSearch: (q: string) => void;
  onClear: () => void;
  results: GraphNode[];
  onSelectNode: (node: GraphNode) => void;
}

export function SearchBar({ query, onSearch, onClear, results, onSelectNode }: SearchBarProps) {
  return (
    <div className="search-container">
      <div className="search-input-wrapper">
        <svg className="search-icon" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
          <circle cx="11" cy="11" r="8" />
          <path d="m21 21-4.35-4.35" />
        </svg>
        <input
          type="text"
          className="search-input"
          placeholder='Search tables... (e.g. "revenue", "pipeline")'
          value={query}
          onChange={(e) => onSearch(e.target.value)}
        />
        {query && (
          <button className="search-clear" onClick={onClear}>
            &times;
          </button>
        )}
      </div>
      {results.length > 0 && (
        <div className="search-results">
          {results.slice(0, 8).map((node) => (
            <button
              key={node.id}
              className="search-result-item"
              onClick={() => onSelectNode(node)}
            >
              <span className="result-schema" style={{ color: layerColor(node.layer) }}>
                {node.schema}.
              </span>
              <span className="result-name">{node.name}</span>
              {node.definition_preview && (
                <span className="result-def">
                  {node.definition_preview.slice(0, 80)}
                </span>
              )}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
