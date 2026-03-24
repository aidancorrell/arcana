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

## Snowflake Auth — Password / Session Token

**File:** `crates/arcana-adapters/src/snowflake/client.rs:9-10`

**Current:** Basic username/password authentication via a session token.

**Limitation:** Password auth is less secure and doesn't support SSO, MFA, or fine-grained rotation.

**Recommended upgrade:** Key-pair JWT authentication — Snowflake's recommended method for service accounts. Avoids passwords in config entirely.

---

## Content Hashing — FNV-1a (Non-Cryptographic)

**File:** `crates/arcana-documents/src/sources/markdown.rs:102-103`

**Current:** FNV-1a hash used for change detection on ingested Markdown documents. Fast, no extra dependencies.

**Limitation:** Not cryptographically secure — trivially collidable if an attacker can control document content. Fine for change detection, not for integrity guarantees.

**Recommended upgrade:** Replace with `sha2` crate (SHA-256) if documents come from untrusted sources or if hash integrity is a compliance requirement.

---

## SQLite — Single-Writer, Local Only

**Referenced in:** `docs/ARCHITECTURE.md`, `CLAUDE.md`

**Current:** SQLite for all metadata persistence. Works perfectly for local/single-user use.

**Limitation:** SQLite's single-writer model becomes a bottleneck with concurrent team usage. No native replication.

**Recommended upgrade:** PostgreSQL — standard SQL, same `sqlx` driver, concurrent writes, native `UUID` and `TIMESTAMPTZ` types, `tsvector`/`GIN` for FTS (replacing FTS5). Already called out as the plan for team/production deployments.

---

## Embedding Provider — OpenAI Only

**File:** `crates/arcana-core/src/embeddings/`

**Current:** Single implementation (`OpenAiEmbeddingProvider`) using `text-embedding-3-small`. The `EmbeddingProvider` trait is already abstracted.

**Limitation:** Hard dependency on OpenAI API availability and pricing. Air-gapped or self-hosted deployments can't use it.

**Recommended upgrade:** Add a local embedding provider (e.g., via `fastembed-rs` or `candle`) as a fallback. The trait is already in place — it's purely an additional implementation.

---

## Summary Table

| Area | Current | Upgrade Path |
|------|---------|-------------|
| Vector search | O(n) flat scan, ≤100k vectors | HNSW (usearch) or external vector DB |
| Snowflake auth | Password / session token | Key-pair JWT |
| Content hashing | FNV-1a | SHA-256 (sha2 crate) |
| Metadata store | SQLite (single-writer) | PostgreSQL (concurrent, replication) |
| Embedding provider | OpenAI only | Add local fallback (fastembed-rs) |
