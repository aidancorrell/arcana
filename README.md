# Arcana

A semantic metadata layer for data warehouses — giving AI agents structured, confident, always-fresh context about your data.

## Overview

Arcana sits between your data infrastructure (Snowflake, dbt) and your AI agents (Claude, GPT-4, Cursor). It maintains a living metadata graph: tables, columns, semantic definitions, data contracts, lineage, usage patterns, and documents — all with confidence scores and automatic decay.

## Architecture

```
arcana/
├── crates/
│   ├── arcana-core          # Metadata store, entity types, embedding index, confidence system
│   ├── arcana-adapters      # Structured metadata adapters (Snowflake, dbt)
│   ├── arcana-documents     # Document ingestion pipeline (Markdown, Confluence, etc.)
│   ├── arcana-recommender   # Context recommendation engine for AI agents
│   ├── arcana-mcp           # MCP server (Model Context Protocol)
│   └── arcana-cli           # CLI binary
├── config/
│   └── arcana.example.toml  # Example configuration
└── README.md
```

### Core Concepts

- **Entities**: Tables, columns, semantic definitions, data contracts, lineage edges, usage records
- **Confidence Scoring**: Every piece of metadata has a confidence score (0.0–1.0) that decays over time unless refreshed
- **Embedding Index**: All entities are embedded for semantic search; documents are chunked and linked to entities
- **MCP Server**: Exposes `get_context`, `describe_table`, `estimate_cost`, and `update_context` tools for AI agents
- **Adapters**: Pull structured metadata from Snowflake INFORMATION_SCHEMA and dbt manifest/catalog JSON

## Getting Started

### Prerequisites

- Rust 1.78+
- A Snowflake account (for the Snowflake adapter)
- A dbt project (for the dbt adapter)
- OpenAI API key (for embeddings)

### Installation

```bash
git clone https://github.com/aidancorrell/arcana
cd arcana
cargo build --release
```

### Configuration

```bash
cp config/arcana.example.toml arcana.toml
# Edit arcana.toml with your credentials
```

### Usage

```bash
# Initialize the metadata store
arcana init

# Sync metadata from Snowflake + dbt
arcana sync

# Ingest documents (Markdown, etc.)
arcana ingest ./docs/

# Start the MCP server
arcana serve

# Ask a question (semantic search)
arcana ask "what does the orders table contain?"

# Show sync status and confidence scores
arcana status
```

## Development

```bash
# Run tests
cargo test --workspace

# Check all crates
cargo check --workspace

# Format
cargo fmt --all

# Lint
cargo clippy --workspace
```

## License

MIT
