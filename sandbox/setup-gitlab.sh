#!/usr/bin/env bash
set -euo pipefail

SANDBOX_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SANDBOX_DIR/.." && pwd)"
GITLAB_DIR="$SANDBOX_DIR/gitlab-analytics"
DBT_DIR="$GITLAB_DIR/transform/snowflake-dbt"

echo "=== Arcana Sandbox Setup (GitLab Analytics — 3,400+ models) ==="
echo "Workspace: $SANDBOX_DIR"
echo ""

# --- 1. Clone GitLab analytics ---
if [ -d "$GITLAB_DIR" ]; then
  echo "[skip] gitlab-analytics/ already exists"
else
  echo "[clone] Cloning gitlab-data/analytics (shallow)..."
  git clone --depth 1 https://gitlab.com/gitlab-data/analytics.git "$GITLAB_DIR"
fi

# --- 2. Check dbt is installed ---
if ! command -v dbt &> /dev/null; then
  echo ""
  echo "ERROR: dbt is not installed."
  echo "Install it with:  pip install dbt-duckdb"
  echo "Then re-run this script."
  exit 1
fi

# --- 3. Patch packages.yml (remove private GitLab packages) ---
echo ""
echo "[patch] Stripping private packages from packages.yml..."
cat > "$DBT_DIR/packages.yml" << 'PKGYML'
packages:
  - package: dbt-labs/audit_helper
    version: 0.12.2
  - package: dbt-labs/dbt_utils
    version: 1.3.3
  - package: dbt-labs/dbt_external_tables
    version: 0.12.0
PKGYML

# --- 4. Remove models that depend on private packages ---
echo "[patch] Removing models that depend on private packages..."
rm -rf "$DBT_DIR/models/workspaces/workspace_data/dbt/"

# Remove dbt_artifacts upload_results hook from dbt_project.yml
sed -i '' 's/.*dbt_artifacts.upload_results.*//' "$DBT_DIR/dbt_project.yml" 2>/dev/null || \
  sed -i 's/.*dbt_artifacts.upload_results.*//' "$DBT_DIR/dbt_project.yml"

# --- 5. Create stub profiles.yml ---
echo "[config] Creating stub profiles.yml for dbt-duckdb..."
mkdir -p "$DBT_DIR/profile"
cat > "$DBT_DIR/profile/profiles.yml" << 'PROFILES'
gitlab-snowflake:
  target: dev
  outputs:
    dev:
      type: duckdb
      path: /tmp/arcana-gitlab-stub.duckdb
PROFILES

# --- 6. Install dbt packages ---
echo ""
echo "[dbt] Installing packages..."
cd "$DBT_DIR"

# Set all required env vars (stub values — only needed for parsing, not execution)
export SNOWFLAKE_PREP_DATABASE=PREP
export SNOWFLAKE_PROD_DATABASE=PROD
export SNOWFLAKE_LOAD_DATABASE=RAW
export SNOWFLAKE_SNAPSHOT_DATABASE=SNAPSHOTS
export SNOWFLAKE_STATIC_DATABASE=STATIC
export SNOWFLAKE_TRANSFORM_WAREHOUSE=TRANSFORM
export DATA_TEST_BRANCH=main
export SALT_IP=stub
export SALT_NAME=stub
export SALT_EMAIL=stub
export SALT_PASSWORD=stub
export SALT=stub
export DBT_SYNAPSE_DB=stub
export DBT_SYNAPSE_PWD=stub
export DBT_SYNAPSE_SERVER=stub
export DBT_SYNAPSE_UID=stub

dbt deps --profiles-dir profile 2>&1 | grep -v "Update"

# --- 7. Generate manifest.json ---
echo ""
echo "[dbt] Parsing project (this takes ~2 minutes for 3,400+ models)..."
dbt parse --profiles-dir profile 2>&1 | grep -v "metadata-based freshness" | tail -5

if [ -f "target/manifest.json" ]; then
  MODEL_COUNT=$(python3 -c "
import json
with open('target/manifest.json') as f:
    m = json.load(f)
models = [v for v in m['nodes'].values() if v['resource_type'] == 'model']
with_desc = sum(1 for v in models if v.get('description', '').strip())
cols = sum(len(v.get('columns', {})) for v in models)
print(f'{len(models)} models ({with_desc} with descriptions), {cols} columns')
")
  echo "[ok] target/manifest.json created — $MODEL_COUNT"
else
  echo "ERROR: manifest.json was not created. Check dbt output above."
  exit 1
fi

# --- 8. Create arcana.toml ---
cd "$SANDBOX_DIR"

if [ -f "arcana-gitlab.toml" ]; then
  echo ""
  echo "[skip] arcana-gitlab.toml already exists"
else
  echo ""
  echo "[config] Creating arcana-gitlab.toml..."
  cat > arcana-gitlab.toml << 'TOML'
[database]
url = "sqlite://arcana-gitlab.db"

[embeddings]
provider = "openai"
# Set OPENAI_API_KEY env var, or fill this in:
openai_api_key = ""
openai_model = "text-embedding-3-small"
dimensions = 1536

[index]
persist_path = "./arcana-gitlab.idx"

[dbt]
project_path = "./gitlab-analytics/transform/snowflake-dbt"

[enrichment]
# Set ANTHROPIC_API_KEY env var for auto-enrichment

[confidence]
decay_rate_per_day = 0.02
stale_threshold = 0.4

[mcp]
bind_addr = "127.0.0.1:8477"
max_context_tokens = 8000
TOML
  echo "[ok] arcana-gitlab.toml created — set your OPENAI_API_KEY"
fi

# --- 9. Build arcana ---
echo ""
echo "[build] Building arcana..."
cd "$REPO_ROOT"
cargo build --bin arcana 2>&1 | tail -1

# --- 10. Initialize store ---
cd "$SANDBOX_DIR"
if [ -f "arcana-gitlab.db" ]; then
  echo ""
  echo "[skip] arcana-gitlab.db already exists"
else
  echo ""
  echo "[init] Initializing Arcana store..."
  "$REPO_ROOT/target/debug/arcana" --config arcana-gitlab.toml init . 2>/dev/null || \
    "$REPO_ROOT/target/debug/arcana" init .
fi

# Clean up stub duckdb
rm -f /tmp/arcana-gitlab-stub.duckdb

echo ""
echo "=== Setup Complete ==="
echo ""
echo "GitLab Analytics: 3,400+ models, 28,000+ columns"
echo ""
echo "Next steps:"
echo "  1. export OPENAI_API_KEY='sk-...'"
echo "  2. cargo run --bin arcana -- --config sandbox/arcana-gitlab.toml sync --adapter dbt"
echo "  3. cargo run --bin arcana -- --config sandbox/arcana-gitlab.toml status --detailed"
echo "  4. cargo run --bin arcana -- --config sandbox/arcana-gitlab.toml enrich --dry-run"
echo "  5. cargo run --bin arcana -- --config sandbox/arcana-gitlab.toml reembed"
echo "  6. cargo run --bin arcana -- --config sandbox/arcana-gitlab.toml ask 'monthly recurring revenue'"
echo ""
echo "Test queries for GitLab's data warehouse:"
echo "  - 'monthly recurring revenue by product tier'"
echo "  - 'customer churn rate'"
echo "  - 'CI pipeline duration trends'"
echo "  - 'merge request cycle time'"
echo "  - 'namespace storage usage'"
echo ""
