#!/usr/bin/env bash
set -euo pipefail

SANDBOX_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SANDBOX_DIR/.." && pwd)"

echo "=== Arcana Sandbox Setup ==="
echo "Workspace: $SANDBOX_DIR"
echo ""

# --- 1. Clone jaffle shop ---
if [ -d "$SANDBOX_DIR/jaffle-shop" ]; then
  echo "[skip] jaffle-shop/ already exists"
else
  echo "[clone] Cloning dbt-labs/jaffle-shop..."
  git clone --depth 1 https://github.com/dbt-labs/jaffle-shop.git "$SANDBOX_DIR/jaffle-shop"
fi

# --- 2. Generate dbt artifacts ---
echo ""
echo "[dbt] Generating manifest.json..."
cd "$SANDBOX_DIR/jaffle-shop"

if ! command -v dbt &> /dev/null; then
  echo ""
  echo "ERROR: dbt is not installed."
  echo "Install it with:  pip install dbt-duckdb"
  echo "Then re-run this script."
  exit 1
fi

# Install dbt packages if needed
if [ ! -d "dbt_packages" ]; then
  echo "[dbt] Installing packages..."
  dbt deps 2>/dev/null || true
fi

dbt parse --quiet 2>/dev/null || dbt parse

if [ -f "target/manifest.json" ]; then
  echo "[ok] target/manifest.json created"
else
  echo "ERROR: manifest.json was not created. Check dbt output above."
  exit 1
fi

# catalog.json is optional — needs dbt docs generate which may require a connection
echo "[dbt] Attempting dbt docs generate (optional)..."
dbt docs generate --quiet 2>/dev/null && echo "[ok] target/catalog.json created" || echo "[skip] catalog.json not generated (not required)"

# --- 3. Create arcana.toml ---
cd "$SANDBOX_DIR"

if [ -f "arcana.toml" ]; then
  echo ""
  echo "[skip] arcana.toml already exists"
else
  echo ""
  echo "[config] Creating arcana.toml..."
  cat > arcana.toml << 'TOML'
[database]
url = "sqlite://arcana.db"

[embeddings]
provider = "openai"
# Set OPENAI_API_KEY env var, or fill this in:
openai_api_key = ""
openai_model = "text-embedding-3-small"
dimensions = 1536

[dbt]
project_path = "./jaffle-shop"

[confidence]
decay_rate_per_day = 0.02
stale_threshold = 0.4

[mcp]
bind_addr = "127.0.0.1:8477"
max_context_tokens = 8000
TOML
  echo "[ok] arcana.toml created — set your OPENAI_API_KEY"
fi

# --- 4. Build arcana ---
echo ""
echo "[build] Building arcana..."
cd "$REPO_ROOT"
cargo build --bin arcana 2>&1 | tail -1

# --- 5. Initialize store ---
cd "$SANDBOX_DIR"
if [ -f "arcana.db" ]; then
  echo ""
  echo "[skip] arcana.db already exists"
else
  echo ""
  echo "[init] Initializing Arcana store..."
  "$REPO_ROOT/target/debug/arcana" --config arcana.toml init . 2>/dev/null || \
    "$REPO_ROOT/target/debug/arcana" init .
fi

echo ""
echo "=== Setup Complete ==="
echo ""
echo "Next steps:"
echo "  1. export OPENAI_API_KEY='sk-...'"
echo "  2. cargo run --bin arcana -- --config sandbox/arcana.toml sync --adapter dbt"
echo "  3. cargo run --bin arcana -- --config sandbox/arcana.toml reembed"
echo "  4. cargo run --bin arcana -- --config sandbox/arcana.toml ask 'revenue by customer'"
echo ""
