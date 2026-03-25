# Contributing to Arcana

Thanks for your interest in contributing! Here's how to get started.

## Development Setup

```bash
git clone https://github.com/aidancorrell/arcana
cd arcana
cargo build
cargo test
```

Tests run without API keys using mock providers.

## Making Changes

1. Fork the repo and create a branch from `main`
2. Write your code — follow existing patterns and conventions
3. Run `cargo clippy --all` and fix any warnings
4. Run `cargo test` and ensure all 101+ tests pass
5. Open a pull request against `main`

## Pull Request Guidelines

- Keep PRs focused — one feature or fix per PR
- Write a clear description of what changed and why
- Add tests for new functionality
- Update documentation if you change public APIs or CLI commands

## Architecture

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the system design. The codebase is organized as a Cargo workspace:

- `arcana-core` — entities, SQLite store, embeddings, confidence system
- `arcana-adapters` — Snowflake + dbt metadata adapters
- `arcana-documents` — Markdown ingestion pipeline
- `arcana-recommender` — search ranking + context serialization
- `arcana-mcp` — MCP server tools
- `arcana-cli` — CLI commands

## Code Style

- Errors via `anyhow` — no custom error types unless strictly needed
- Runtime `sqlx::query()` + `.bind()` — no compile-time query macros
- Confidence scores are `f64` in `[0.0, 1.0]`
- UUIDs and timestamps stored as TEXT in SQLite

## Reporting Issues

Open an issue on GitHub with:
- What you expected to happen
- What actually happened
- Steps to reproduce
- Arcana version (`arcana --version` or `cargo run --bin arcana -- --version`)
