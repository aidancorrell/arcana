# Production Improvements

Known shortcuts and intentional simplifications in the current codebase, with recommended upgrades for production use.

---

## Vector Index — O(n) Linear Scan

**File:** `crates/arcana-core/src/embeddings/index.rs:8-12`

**Current:** Brute-force cosine similarity over all stored vectors in a `HashMap<Uuid, Vec<f32>>`. Every query scans the entire index.

**Limitation:** O(n) per query. Acceptable up to ~100k vectors; degrades beyond that.

**Recommended upgrade:** Replace with an Approximate Nearest Neighbor (ANN) library:
- [`usearch`](https://github.com/unum-cloud/usearch) — Rust-native HNSW, drop-in
- [`hnswlib`](https://github.com/nmslib/hnswlib) — via FFI bindings
- External vector DB (Qdrant, pgvector, Pinecone) if moving off SQLite

---

## Snowflake Auth — Key-Pair JWT  :white_check_mark: IMPLEMENTED

**File:** `crates/arcana-adapters/src/snowflake/client.rs`

**Previous:** Basic username/password authentication via a session token.

**Current:** The Snowflake client now supports both authentication methods:
1. **Key-pair JWT** (preferred) — when `private_key_path` is set in `SnowflakeConfig`, the client generates RS256-signed JWTs with Snowflake's required claims (issuer with SHA-256 public key thumbprint, subject as `ACCOUNT.USER`). No password needed.
2. **Password / session token** (fallback) — the original auth method, used when no private key is configured.

Auth method is selected automatically based on config. Set `private_key_path` to your PKCS#8 PEM file to use JWT.

---

## Content Hashing — SHA-256  :white_check_mark: IMPLEMENTED

**File:** `crates/arcana-documents/src/sources/markdown.rs`

**Previous:** FNV-1a hash used for change detection on ingested Markdown documents.

**Current:** Replaced with proper SHA-256 hashing via the `sha2` crate. Produces a 64-character hex digest for each document, suitable for both change detection and integrity verification.

---

## SQLite — Single-Writer, Local Only

**Referenced in:** `docs/ARCHITECTURE.md`, `CLAUDE.md`

**Current:** SQLite for all metadata persistence. Works perfectly for local/single-user use.

**Limitation:** SQLite's single-writer model becomes a bottleneck with concurrent team usage. No native replication.

**Recommended upgrade:** PostgreSQL — standard SQL, same `sqlx` driver, concurrent writes, native `UUID` and `TIMESTAMPTZ` types, `tsvector`/`GIN` for FTS (replacing FTS5). Already called out as the plan for team/production deployments.

---

## Embedding Provider — Local Fallback  :white_check_mark: IMPLEMENTED

**File:** `crates/arcana-core/src/embeddings/local.rs`

**Previous:** Single implementation (`OpenAiEmbeddingProvider`) using `text-embedding-3-small`. Hard dependency on OpenAI API.

**Current:** Added `LocalEmbeddingProvider` as a zero-dependency fallback. Uses character n-gram hashing to produce fixed-dimension vectors with L2 normalization. The CLI and auto-reembed now automatically fall back to the local provider when no `OPENAI_API_KEY` is set.

The local provider is suitable for:
- Air-gapped or self-hosted deployments
- Development and testing without API credits
- Fallback when the primary provider is unavailable

For production quality, neural embeddings (OpenAI) are still recommended. The `EmbeddingProvider` trait remains the extension point for additional implementations.

---

## Summary Table

| Area | Current | Status |
|------|---------|--------|
| Vector search | O(n) flat scan, ≤100k vectors | Upgrade to HNSW when needed |
| Snowflake auth | Key-pair JWT + password fallback | :white_check_mark: Done |
| Content hashing | SHA-256 (sha2 crate) | :white_check_mark: Done |
| Metadata store | SQLite (single-writer) | Upgrade to PostgreSQL when needed |
| Embedding provider | OpenAI + local fallback | :white_check_mark: Done |
