# Stage 1: Build
FROM rust:1.82-slim-bookworm AS builder

RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy manifests first for layer caching
COPY Cargo.toml Cargo.lock* ./
COPY crates/arcana-core/Cargo.toml crates/arcana-core/Cargo.toml
COPY crates/arcana-adapters/Cargo.toml crates/arcana-adapters/Cargo.toml
COPY crates/arcana-documents/Cargo.toml crates/arcana-documents/Cargo.toml
COPY crates/arcana-recommender/Cargo.toml crates/arcana-recommender/Cargo.toml
COPY crates/arcana-mcp/Cargo.toml crates/arcana-mcp/Cargo.toml
COPY crates/arcana-cli/Cargo.toml crates/arcana-cli/Cargo.toml
COPY crates/arcana-integration-tests/Cargo.toml crates/arcana-integration-tests/Cargo.toml

# Create dummy source files for dependency caching
RUN for dir in arcana-core arcana-adapters arcana-documents arcana-recommender arcana-mcp arcana-cli arcana-integration-tests; do \
      mkdir -p crates/$dir/src && echo "" > crates/$dir/src/lib.rs; \
    done && \
    echo 'fn main() {}' > crates/arcana-cli/src/main.rs && \
    echo '' > crates/arcana-integration-tests/src/lib.rs

# Build dependencies only (cached layer)
RUN cargo build --release --bin arcana 2>/dev/null || true

# Copy actual source code
COPY crates/ crates/
COPY config/ config/

# Touch source files to invalidate the dummy builds
RUN find crates/ -name "*.rs" -exec touch {} +

# Build the real binary
RUN cargo build --release --bin arcana

# Stage 2: Runtime
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -r -m -d /home/arcana arcana

COPY --from=builder /app/target/release/arcana /usr/local/bin/arcana
COPY config/arcana.example.toml /etc/arcana/arcana.example.toml

# Data directory for SQLite DB and index
RUN mkdir -p /data && chown arcana:arcana /data

USER arcana
WORKDIR /data

# Default config location — mount or override via env
ENV ARCANA_CONFIG=/data/arcana.toml

EXPOSE 8477

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -sf http://localhost:8477/sse > /dev/null || exit 1

ENTRYPOINT ["arcana"]
CMD ["--config", "/data/arcana.toml", "serve"]
