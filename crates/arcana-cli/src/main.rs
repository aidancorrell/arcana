use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// CLI definition
// ---------------------------------------------------------------------------

#[derive(Parser)]
#[command(
    name = "arcana",
    version = env!("CARGO_PKG_VERSION"),
    about = "Arcana — semantic metadata layer for AI agents and data warehouses",
    long_about = None,
)]
struct Cli {
    /// Path to configuration file (default: ./arcana.toml).
    #[arg(short, long, global = true, default_value = "arcana.toml")]
    config: PathBuf,

    /// Log level: trace, debug, info, warn, error (default: info).
    #[arg(short, long, global = true, default_value = "info")]
    log_level: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new Arcana metadata store (creates SQLite DB and example config).
    Init {
        /// Directory to initialize in (default: current directory).
        #[arg(default_value = ".")]
        path: PathBuf,
    },

    /// Sync metadata from configured sources (Snowflake, dbt).
    Sync {
        /// Only sync from this adapter (e.g., `snowflake`, `dbt`).
        #[arg(short, long)]
        adapter: Option<String>,

        /// Perform a full re-sync even if data appears current.
        #[arg(long)]
        full: bool,
    },

    /// Ingest documents (Markdown, Confluence, etc.) into the metadata store.
    Ingest {
        /// Path or glob pattern of documents to ingest.
        path: PathBuf,

        /// Document source type (default: markdown).
        #[arg(short, long, default_value = "markdown")]
        source: String,
    },

    /// Start the MCP server for AI agent integration.
    Serve {
        /// Bind address (overrides config).
        #[arg(short, long)]
        bind: Option<String>,

        /// Run as stdio MCP server (for Claude Desktop / Cursor).
        #[arg(long)]
        stdio: bool,
    },

    /// Semantic search — ask a natural-language question about your data.
    Ask {
        /// The question to ask.
        query: String,

        /// Number of results to return.
        #[arg(short = 'n', long, default_value = "5")]
        top_k: usize,

        /// Output format: markdown, json, prose.
        #[arg(short, long, default_value = "markdown")]
        format: String,
    },

    /// Show sync status: last sync times, stale entities, confidence distribution.
    Status,

    /// Re-embed all entities and document chunks (e.g., after changing embedding model).
    Reembed {
        /// Only re-embed entities below this confidence threshold.
        #[arg(long)]
        below_confidence: Option<f64>,

        /// Batch size for embedding API calls.
        #[arg(long, default_value = "100")]
        batch_size: usize,
    },
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

#[derive(Debug, serde::Deserialize, Default)]
struct AppConfig {
    database: DatabaseConfig,
    embeddings: EmbeddingsConfig,
    mcp: McpConfig,
}

#[derive(Debug, serde::Deserialize)]
struct DatabaseConfig {
    url: String,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: "sqlite://arcana.db".to_string(),
        }
    }
}

#[derive(Debug, serde::Deserialize, Default)]
struct EmbeddingsConfig {
    provider: Option<String>,
    openai_api_key: Option<String>,
    openai_model: Option<String>,
    dimensions: Option<usize>,
}

#[derive(Debug, serde::Deserialize)]
struct McpConfig {
    bind_addr: String,
    max_context_tokens: usize,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            bind_addr: "127.0.0.1:3000".to_string(),
            max_context_tokens: 8000,
        }
    }
}

fn load_config(path: &PathBuf) -> Result<AppConfig> {
    if !path.exists() {
        tracing::warn!(
            "config file {:?} not found, using defaults",
            path
        );
        return Ok(AppConfig::default());
    }

    let cfg = config::Config::builder()
        .add_source(config::File::from(path.as_path()).required(false))
        .add_source(config::Environment::with_prefix("ARCANA"))
        .build()
        .context("failed to build configuration")?;

    cfg.try_deserialize::<AppConfig>()
        .context("failed to deserialize configuration")
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize tracing
    let log_filter = format!("arcana={},warn", cli.log_level);
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&log_filter)),
        )
        .init();

    let cfg = load_config(&cli.config)?;

    match cli.command {
        Commands::Init { path } => cmd_init(&path, &cfg).await,
        Commands::Sync { adapter, full } => cmd_sync(&cfg, adapter.as_deref(), full).await,
        Commands::Ingest { path, source } => cmd_ingest(&cfg, &path, &source).await,
        Commands::Serve { bind, stdio } => cmd_serve(&cfg, bind.as_deref(), stdio).await,
        Commands::Ask { query, top_k, format } => cmd_ask(&cfg, &query, top_k, &format).await,
        Commands::Status => cmd_status(&cfg).await,
        Commands::Reembed { below_confidence, batch_size } => {
            cmd_reembed(&cfg, below_confidence, batch_size).await
        }
    }
}

// ---------------------------------------------------------------------------
// Command implementations
// ---------------------------------------------------------------------------

async fn cmd_init(path: &PathBuf, _cfg: &AppConfig) -> Result<()> {
    println!("Initializing Arcana at {}", path.display());

    // Create the directory if needed
    if !path.exists() {
        std::fs::create_dir_all(path)
            .with_context(|| format!("failed to create directory {:?}", path))?;
    }

    // Write example config if not present
    let config_path = path.join("arcana.toml");
    if !config_path.exists() {
        let example = include_str!("../../../config/arcana.example.toml");
        std::fs::write(&config_path, example)
            .context("failed to write arcana.toml")?;
        println!("  Created arcana.toml — fill in your credentials.");
    } else {
        println!("  arcana.toml already exists, skipping.");
    }

    // Initialize the SQLite store
    let db_url = format!("sqlite://{}/arcana.db", path.display());
    arcana_core::store::SqliteStore::open(&db_url)
        .await
        .context("failed to initialize SQLite metadata store")?;

    println!("  Initialized metadata store at {}/arcana.db", path.display());
    println!("\nArcana initialized. Next steps:");
    println!("  1. Edit arcana.toml with your Snowflake/dbt credentials.");
    println!("  2. Run `arcana sync` to pull metadata.");
    println!("  3. Run `arcana serve --stdio` to start the MCP server.");

    Ok(())
}

async fn cmd_sync(cfg: &AppConfig, adapter_filter: Option<&str>, full: bool) -> Result<()> {
    println!(
        "Syncing metadata{}...",
        if full { " (full refresh)" } else { "" }
    );

    let store = open_store(cfg).await?;
    let _store = std::sync::Arc::new(store);

    // TODO: instantiate configured adapters (Snowflake, dbt) and run sync
    //       for each adapter matching adapter_filter, call adapter.sync()
    //       then upsert all returned entities into the store.
    let _ = adapter_filter;

    println!("Sync complete.");
    Ok(())
}

async fn cmd_ingest(cfg: &AppConfig, path: &PathBuf, source_type: &str) -> Result<()> {
    println!("Ingesting documents from {} (source: {})...", path.display(), source_type);

    let _store = std::sync::Arc::new(open_store(cfg).await?);

    match source_type {
        "markdown" => {
            // TODO: instantiate MarkdownSource + IngestPipeline and run
            println!("  Markdown ingestion not yet wired up.");
        }
        other => {
            anyhow::bail!("unsupported document source: {other}");
        }
    }

    Ok(())
}

async fn cmd_serve(cfg: &AppConfig, bind_override: Option<&str>, stdio: bool) -> Result<()> {
    let bind_addr = bind_override.unwrap_or(&cfg.mcp.bind_addr);

    if stdio {
        println!("Starting Arcana MCP server on stdio...");
        // TODO: instantiate ArcanaServer and call serve_stdio()
        println!("  MCP server not yet wired up.");
    } else {
        println!("Starting Arcana MCP server on {}...", bind_addr);
        // TODO: instantiate ArcanaServer and bind TCP listener
        println!("  MCP TCP server not yet wired up.");
    }

    Ok(())
}

async fn cmd_ask(cfg: &AppConfig, query: &str, top_k: usize, format: &str) -> Result<()> {
    println!("Searching: {:?} (top_k={}, format={})", query, top_k, format);
    let _store = std::sync::Arc::new(open_store(cfg).await?);

    // TODO: embed query, search vector index, serialize and print results
    println!("  Semantic search not yet wired up (run `arcana sync` first).");

    Ok(())
}

async fn cmd_status(cfg: &AppConfig) -> Result<()> {
    let _store = std::sync::Arc::new(open_store(cfg).await?);

    println!("Arcana Status");
    println!("=============");
    // TODO: query store for:
    //   - data_source count
    //   - table count
    //   - column count
    //   - stale entity count (confidence < threshold)
    //   - last sync time
    //   - document count
    //   - chunk count
    println!("  Status not yet wired up.");

    Ok(())
}

async fn cmd_reembed(
    cfg: &AppConfig,
    below_confidence: Option<f64>,
    batch_size: usize,
) -> Result<()> {
    println!(
        "Re-embedding all entities (batch_size={}){}...",
        batch_size,
        below_confidence
            .map(|t| format!(", confidence < {t:.2}"))
            .unwrap_or_default()
    );

    let _store = std::sync::Arc::new(open_store(cfg).await?);

    // TODO: iterate all entities and chunks, re-embed in batches, update store
    println!("  Re-embedding not yet wired up.");

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn open_store(cfg: &AppConfig) -> Result<arcana_core::store::SqliteStore> {
    arcana_core::store::SqliteStore::open(&cfg.database.url)
        .await
        .context("failed to open metadata store")
}
