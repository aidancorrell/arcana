mod ops;

use anyhow::{Context, Result};
use arcana_adapters::MetadataAdapter;
use arcana_core::store::MetadataStore;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::sync::Arc;

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

        /// Background sync interval (e.g., "6h", "30m"). Overrides config.
        #[arg(long)]
        sync_interval: Option<String>,
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
    Status {
        /// Show detailed per-schema coverage and definition statistics.
        #[arg(long)]
        detailed: bool,
    },

    /// Re-embed all entities and document chunks (e.g., after changing embedding model).
    Reembed {
        /// Only re-embed entities below this confidence threshold.
        #[arg(long)]
        below_confidence: Option<f64>,

        /// Batch size for embedding API calls.
        #[arg(long, default_value = "100")]
        batch_size: usize,
    },

    /// Generate LLM descriptions for tables/columns that have no semantic definition.
    Enrich {
        /// Preview what would be generated without writing anything.
        #[arg(long)]
        dry_run: bool,

        /// Only enrich entities whose names match this substring.
        #[arg(long)]
        filter: Option<String>,

        /// Number of entities to send per LLM call.
        #[arg(long, default_value = "20")]
        batch_size: usize,
    },

    /// Detect redundant/duplicate tables by semantic similarity clustering.
    Dedup {
        /// Cosine similarity threshold for grouping tables (default: 0.92).
        #[arg(long, default_value = "0.92")]
        threshold: f64,

        /// Preview clusters without persisting to the store.
        #[arg(long)]
        dry_run: bool,
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
    #[serde(default)]
    enrichment: EnrichmentConfig,
    #[serde(default)]
    index: IndexSectionConfig,
    #[serde(default)]
    sync: SyncSectionConfig,
    #[serde(default)]
    dbt: Option<DbtSectionConfig>,
    #[serde(default)]
    snowflake: Option<SnowflakeSectionConfig>,
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
    #[allow(dead_code)]
    provider: Option<String>,
    openai_api_key: Option<String>,
    openai_model: Option<String>,
    dimensions: Option<usize>,
}

#[derive(Debug, serde::Deserialize)]
struct McpConfig {
    bind_addr: String,
    max_context_tokens: usize,
    /// Admin API bind address. If set, starts admin/webhook server on this address.
    admin_addr: Option<String>,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            bind_addr: "127.0.0.1:8477".to_string(),
            max_context_tokens: 8000,
            admin_addr: None,
        }
    }
}

#[derive(Debug, serde::Deserialize, Default)]
struct SyncSectionConfig {
    /// Background sync interval (e.g., "6h", "30m", "0" = disabled).
    auto_interval: Option<String>,
    /// Run `enrich` after each auto-sync.
    #[serde(default)]
    auto_enrich: bool,
    /// Run `reembed` after each auto-sync.
    #[serde(default)]
    auto_reembed: bool,
    /// Secret for webhook authentication (POST /api/sync).
    webhook_secret: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct DbtSectionConfig {
    project_path: PathBuf,
    manifest_path: Option<PathBuf>,
    catalog_path: Option<PathBuf>,
}

#[derive(Debug, serde::Deserialize)]
struct SnowflakeSectionConfig {
    account: String,
    warehouse: String,
    database: String,
    schema: String,
    user: String,
    private_key_path: Option<String>,
    password: Option<String>,
    role: Option<String>,
}

#[derive(Debug, serde::Deserialize, Default)]
struct IndexSectionConfig {
    /// Path to persist the vector index on disk (default: none — warm from SQLite each time).
    persist_path: Option<String>,
}

#[derive(Debug, serde::Deserialize, Default)]
struct EnrichmentConfig {
    /// Anthropic API key. Falls back to ANTHROPIC_API_KEY env var.
    anthropic_api_key: Option<String>,
    /// Claude model to use for enrichment (default: claude-haiku-4-5-20251001).
    model: Option<String>,
    /// Max entities per LLM call (default: 20).
    batch_size: Option<usize>,
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
        Commands::Serve { bind, stdio, sync_interval } => {
            cmd_serve(&cfg, bind.as_deref(), stdio, sync_interval.as_deref()).await
        }
        Commands::Ask { query, top_k, format } => cmd_ask(&cfg, &query, top_k, &format).await,
        Commands::Status { detailed } => cmd_status(&cfg, detailed).await,
        Commands::Reembed { below_confidence, batch_size } => {
            cmd_reembed(&cfg, below_confidence, batch_size).await
        }
        Commands::Enrich { dry_run, filter, batch_size } => {
            cmd_enrich(&cfg, dry_run, filter.as_deref(), batch_size).await
        }
        Commands::Dedup { threshold, dry_run } => {
            cmd_dedup(&cfg, threshold, dry_run).await
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
    let abs_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let db_url = format!("sqlite:///{}/arcana.db", abs_path.display());
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

    let store = Arc::new(open_store(cfg).await?);

    let should_run = |name: &str| -> bool {
        adapter_filter.is_none_or(|f| f == name)
    };

    let mut total_tables = 0usize;
    let mut total_columns = 0usize;

    // dbt adapter
    if should_run("dbt") {
        if let Some(dbt_cfg) = &cfg.dbt {
            println!("  Syncing dbt adapter...");
            let ds_id = get_or_create_data_source(
                &*store,
                "dbt",
                arcana_core::entities::DataSourceType::Dbt,
            )
            .await?;

            let mut adapter_cfg =
                arcana_adapters::dbt::DbtConfig::new(&dbt_cfg.project_path, ds_id);
            if let Some(mp) = &dbt_cfg.manifest_path {
                adapter_cfg.manifest_path = mp.clone();
            }
            if let Some(cp) = &dbt_cfg.catalog_path {
                adapter_cfg.catalog_path = cp.clone();
            }

            let output = if full {
                let adapter = arcana_adapters::dbt::DbtAdapter::new(adapter_cfg);
                adapter.sync().await?
            } else {
                // Incremental sync: load known checksums, skip unchanged models
                let known: std::collections::HashMap<String, String> = store
                    .list_sync_checksums("dbt")
                    .await?
                    .into_iter()
                    .collect();
                let manifest_path = std::path::Path::new(&adapter_cfg.manifest_path);
                arcana_adapters::dbt::manifest::parse_manifest_incremental(
                    manifest_path,
                    ds_id,
                    &known,
                )
                .await?
            };

            upsert_sync_output(&*store, &output).await?;

            // Persist changed checksums for next incremental run
            for (key, checksum) in &output.changed_checksums {
                store.upsert_sync_checksum("dbt", key, checksum).await?;
            }

            let skipped_note = if !full && output.changed_checksums.len() < output.tables.len() {
                " (incremental)"
            } else {
                ""
            };
            total_tables += output.tables.len();
            total_columns += output.columns.len();
            println!(
                "    dbt: {} schemas, {} tables, {} columns, {} definitions{}",
                output.schemas.len(),
                output.tables.len(),
                output.columns.len(),
                output.semantic_definitions.len(),
                skipped_note
            );
        } else if adapter_filter == Some("dbt") {
            anyhow::bail!("dbt adapter requested but [dbt] section not found in config");
        }
    }

    // Snowflake adapter
    if should_run("snowflake") {
        if let Some(sf_cfg) = &cfg.snowflake {
            println!("  Syncing Snowflake adapter...");
            let ds_id = get_or_create_data_source(
                &*store,
                "snowflake",
                arcana_core::entities::DataSourceType::Snowflake,
            )
            .await?;

            let sf_config = build_snowflake_config(sf_cfg);
            let adapter = arcana_adapters::snowflake::SnowflakeAdapter::new(sf_config, ds_id);
            let output = adapter.sync().await?;

            upsert_sync_output(&*store, &output).await?;
            total_tables += output.tables.len();
            total_columns += output.columns.len();
            println!(
                "    snowflake: {} schemas, {} tables, {} columns",
                output.schemas.len(),
                output.tables.len(),
                output.columns.len()
            );
        } else if adapter_filter == Some("snowflake") {
            anyhow::bail!(
                "snowflake adapter requested but [snowflake] section not found in config"
            );
        }
    }

    println!(
        "Sync complete. {} tables, {} columns total.",
        total_tables, total_columns
    );
    Ok(())
}

async fn cmd_ingest(cfg: &AppConfig, path: &PathBuf, source_type: &str) -> Result<()> {
    println!(
        "Ingesting documents from {} (source: {})...",
        path.display(),
        source_type
    );

    let store: Arc<dyn arcana_core::store::MetadataStore> = Arc::new(open_store(cfg).await?);
    let embedding_provider = build_embedding_provider(cfg)?;

    match source_type {
        "markdown" => {
            let source = arcana_documents::sources::markdown::MarkdownSource::from_path(path);
            let chunker = Arc::new(arcana_documents::StructureAwareChunker::default());
            let linker = Arc::new(arcana_documents::EntityLinker::new(vec![], 0.5));
            let pipeline = arcana_documents::IngestPipeline::new(
                store,
                embedding_provider,
                chunker,
                linker,
            );
            let result = pipeline.ingest_source(&source).await?;
            println!(
                "  {} documents, {} chunks, {} links, {} embeddings",
                result.documents_processed,
                result.chunks_created,
                result.entity_links_created,
                result.embeddings_generated
            );
            if !result.errors.is_empty() {
                println!("  {} errors:", result.errors.len());
                for err in &result.errors {
                    println!("    - {err}");
                }
            }
        }
        other => {
            anyhow::bail!("unsupported document source: {other}");
        }
    }

    Ok(())
}

async fn cmd_serve(
    cfg: &AppConfig,
    bind_override: Option<&str>,
    stdio: bool,
    sync_interval_override: Option<&str>,
) -> Result<()> {
    let store: Arc<dyn arcana_core::store::MetadataStore> = Arc::new(open_store(cfg).await?);
    let embedding_provider = build_embedding_provider(cfg)?;
    let dimensions = cfg.embeddings.dimensions.unwrap_or(1536);

    let entity_index = load_or_warm_index(cfg, &*store, dimensions).await?;
    let entity_index = Arc::new(entity_index);
    let chunk_index = Arc::new(arcana_core::embeddings::VectorIndex::new(dimensions));

    let ranker = Arc::new(arcana_recommender::RelevanceRanker::new(
        store.clone(),
        embedding_provider,
        entity_index.clone(),
        chunk_index,
    ));

    let serializer = Arc::new(arcana_recommender::ContextSerializer {
        max_tokens: cfg.mcp.max_context_tokens,
        ..Default::default()
    });

    let mut server = arcana_mcp::ArcanaServer::new(store.clone(), ranker, serializer, entity_index.clone());

    // Attach Snowflake config if present (enables estimate_cost tool)
    if let Some(sf_cfg) = &cfg.snowflake {
        server = server.with_snowflake_config(build_snowflake_config(sf_cfg));
    }

    if stdio {
        println!("Starting Arcana MCP server on stdio...");
        let result = server.serve_stdio().await;
        if let Some(persist_path) = &cfg.index.persist_path {
            let path = std::path::Path::new(persist_path);
            if let Err(e) = entity_index.save(path) {
                tracing::warn!("Failed to persist index: {e}");
            }
        }
        result
    } else {
        // Set up background sync if configured
        let sync_interval_str = sync_interval_override
            .or(cfg.sync.auto_interval.as_deref());
        let sync_trigger = if sync_interval_str.is_some() || cfg.mcp.admin_addr.is_some() {
            let (tx, rx) = tokio::sync::mpsc::channel::<()>(1);
            // Spawn background sync worker
            let sync_store = store.clone();
            let dbt_cfg = cfg.dbt.clone();
            let auto_enrich = cfg.sync.auto_enrich;
            let auto_reembed = cfg.sync.auto_reembed;
            let embed_cfg_provider = cfg.embeddings.openai_api_key.clone();
            let embed_cfg_model = cfg.embeddings.openai_model.clone();
            let embed_cfg_dimensions = cfg.embeddings.dimensions;
            let enrichment_key = cfg.enrichment.anthropic_api_key.clone();
            let enrichment_model = cfg.enrichment.model.clone();
            let enrichment_batch_size = cfg.enrichment.batch_size;

            tokio::spawn(background_sync_worker(
                rx,
                sync_store,
                dbt_cfg,
                auto_enrich,
                auto_reembed,
                embed_cfg_provider,
                embed_cfg_model,
                embed_cfg_dimensions,
                enrichment_key,
                enrichment_model,
                enrichment_batch_size,
            ));

            // Spawn interval trigger if configured
            if let Some(interval_str) = sync_interval_str {
                if let Some(duration) = parse_duration(interval_str) {
                    let interval_tx = tx.clone();
                    tokio::spawn(async move {
                        let mut interval = tokio::time::interval(duration);
                        interval.tick().await; // skip first immediate tick
                        loop {
                            interval.tick().await;
                            tracing::info!("scheduled sync trigger");
                            let _ = interval_tx.try_send(());
                        }
                    });
                    eprintln!("  Background sync:  every {interval_str}");
                }
            }

            Some(tx)
        } else {
            None
        };

        let bind_addr = bind_override.unwrap_or(&cfg.mcp.bind_addr);
        let admin_addr = cfg.mcp.admin_addr.as_deref();
        let webhook_secret = cfg.sync.webhook_secret.clone();

        let result = server
            .serve_http(bind_addr, admin_addr, webhook_secret, sync_trigger)
            .await;

        if let Some(persist_path) = &cfg.index.persist_path {
            let path = std::path::Path::new(persist_path);
            if let Err(e) = entity_index.save(path) {
                tracing::warn!("Failed to persist index: {e}");
            }
        }
        result
    }
}

async fn cmd_ask(cfg: &AppConfig, query: &str, top_k: usize, format: &str) -> Result<()> {
    let store: Arc<dyn arcana_core::store::MetadataStore> = Arc::new(open_store(cfg).await?);
    let embedding_provider = build_embedding_provider(cfg)?;
    let dimensions = cfg.embeddings.dimensions.unwrap_or(1536);

    let entity_index = Arc::new(load_or_warm_index(cfg, &*store, dimensions).await?);
    let chunk_index = Arc::new(arcana_core::embeddings::VectorIndex::new(dimensions));

    let ranker = arcana_recommender::RelevanceRanker::new(
        store,
        embedding_provider,
        entity_index,
        chunk_index,
    );

    let serialization_format = match format {
        "json" => arcana_recommender::serializer::SerializationFormat::JsonLines,
        "prose" => arcana_recommender::serializer::SerializationFormat::Prose,
        _ => arcana_recommender::serializer::SerializationFormat::Markdown,
    };

    let serializer = arcana_recommender::ContextSerializer {
        max_tokens: cfg.mcp.max_context_tokens,
        format: serialization_format,
    };

    let request = arcana_recommender::ContextRequest {
        query: query.to_string(),
        top_k,
        filter_table_id: None,
        min_confidence: 0.0,
        expand_lineage: false,
    };

    let result = ranker.rank(&request).await?;
    let item_count = result.items.len();
    let output = serializer.serialize(&result);

    println!("{output}");
    eprintln!(
        "--- {} items, ~{} tokens ---",
        item_count, result.estimated_tokens
    );

    Ok(())
}

async fn cmd_status(cfg: &AppConfig, detailed: bool) -> Result<()> {
    let store = open_store(cfg).await?;

    let data_sources = store.list_data_sources().await?;
    let mut table_count = 0usize;
    let mut column_count = 0usize;
    let mut tables_with_defs = 0usize;

    // Per-schema tracking for detailed mode
    struct SchemaStats {
        ds_name: String,
        schema_name: String,
        tables: usize,
        tables_with_defs: usize,
        columns: usize,
        stale_tables: usize,
    }
    let mut schema_stats: Vec<SchemaStats> = Vec::new();

    for ds in &data_sources {
        let schemas = store.list_schemas(ds.id).await?;
        for schema in &schemas {
            let tables = store.list_tables(schema.id).await?;
            let mut ss = SchemaStats {
                ds_name: ds.name.clone(),
                schema_name: format!("{}.{}", schema.database_name, schema.schema_name),
                tables: tables.len(),
                tables_with_defs: 0,
                columns: 0,
                stale_tables: 0,
            };
            for table in &tables {
                table_count += 1;
                let defs = store.get_semantic_definitions(table.id).await?;
                if !defs.is_empty() {
                    tables_with_defs += 1;
                    ss.tables_with_defs += 1;
                }
                // Check for stale confidence
                let all_stale = defs.iter().all(|d| d.confidence < 0.4);
                if !defs.is_empty() && all_stale {
                    ss.stale_tables += 1;
                }
                let cols = store.list_columns(table.id).await?;
                column_count += cols.len();
                ss.columns += cols.len();
            }
            schema_stats.push(ss);
        }
    }

    let metrics = store.list_metrics().await?;
    let all_defs = store.list_all_semantic_definitions().await?;
    let defs_with_embeddings = all_defs.iter().filter(|d| d.embedding.is_some()).count();
    let clusters = store.list_table_clusters().await?;

    let coverage = if table_count > 0 {
        (tables_with_defs as f64 / table_count as f64) * 100.0
    } else {
        0.0
    };

    println!("Arcana Status");
    println!("=============");
    println!("  Data sources:    {}", data_sources.len());
    println!("  Tables:          {}", table_count);
    println!("  Columns:         {}", column_count);
    println!("  Metrics:         {}", metrics.len());
    println!("  Definitions:     {}", all_defs.len());
    println!("  Embedded:        {}", defs_with_embeddings);
    println!("  Coverage:        {:.1}%", coverage);
    println!("  Clusters:        {}", clusters.len());

    if detailed {
        println!();
        println!("Per-Schema Coverage");
        println!("-------------------");
        for ss in &schema_stats {
            let cov = if ss.tables > 0 {
                (ss.tables_with_defs as f64 / ss.tables as f64) * 100.0
            } else {
                0.0
            };
            println!(
                "  [{}/{}] {} — {} tables, {} columns, {:.0}% coverage{}",
                ss.ds_name,
                ss.schema_name,
                ss.schema_name,
                ss.tables,
                ss.columns,
                cov,
                if ss.stale_tables > 0 {
                    format!(", {} stale", ss.stale_tables)
                } else {
                    String::new()
                }
            );
        }

        // Confidence distribution
        println!();
        println!("Confidence Distribution");
        println!("-----------------------");
        let mut buckets = [0usize; 5]; // [0-0.2, 0.2-0.4, 0.4-0.6, 0.6-0.8, 0.8-1.0]
        for def in &all_defs {
            let idx = ((def.confidence * 5.0).floor() as usize).min(4);
            buckets[idx] += 1;
        }
        let labels = ["0.0-0.2", "0.2-0.4", "0.4-0.6", "0.6-0.8", "0.8-1.0"];
        for (label, count) in labels.iter().zip(buckets.iter()) {
            let bar_width = if all_defs.is_empty() {
                0
            } else {
                (*count * 30) / all_defs.len().max(1)
            };
            println!(
                "  {}: {:>4} {}",
                label,
                count,
                "█".repeat(bar_width)
            );
        }
    }

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

    let store = open_store(cfg).await?;
    let embedding_provider = build_embedding_provider(cfg)?;

    let def_count =
        ops::reembed_definitions(&store, &*embedding_provider, below_confidence, batch_size)
            .await?;

    println!("  Re-embedded {} semantic definitions.", def_count);
    println!("Done.");

    Ok(())
}

async fn cmd_enrich(
    cfg: &AppConfig,
    dry_run: bool,
    filter: Option<&str>,
    batch_size: usize,
) -> Result<()> {
    let store = open_store(cfg).await?;
    let provider = build_enrichment_provider(cfg)?;

    if dry_run {
        println!("Dry run — no definitions will be written.");
    }
    println!("Enriching undescribed entities (batch_size={batch_size})...");

    // Table-level enrichment
    let (table_requests, table_ids, mut skipped) =
        ops::collect_table_enrichment_targets(&store, filter).await?;

    let mut table_count = 0usize;
    for chunk in table_requests.chunks(batch_size) {
        let id_chunk = &table_ids[table_count..table_count + chunk.len()];
        let count = ops::write_enrichment_batch(&store, &*provider, chunk, id_chunk, dry_run).await?;
        table_count += count;
    }

    // Column-level enrichment
    let mut col_count = 0usize;
    let data_sources = store.list_data_sources().await?;
    for ds in &data_sources {
        let schemas = store.list_schemas(ds.id).await?;
        for schema in &schemas {
            let tables = store.list_tables(schema.id).await?;
            for table in &tables {
                if let Some(f) = filter {
                    if !table.name.contains(f) {
                        continue;
                    }
                }

                let cols = store.list_columns(table.id).await?;
                let mut col_requests: Vec<arcana_core::enrichment::EnrichmentRequest> = Vec::new();
                let mut col_ids: Vec<uuid::Uuid> = Vec::new();
                let all_col_names: Vec<String> = cols.iter().map(|c| c.name.clone()).collect();

                for col in &cols {
                    let existing = store.get_semantic_definitions(col.id).await?;
                    let has_good_def = existing.iter().any(|d| {
                        use arcana_core::entities::DefinitionSource;
                        matches!(
                            d.source,
                            DefinitionSource::Manual | DefinitionSource::DbtYaml | DefinitionSource::SnowflakeComment
                        )
                    });
                    if has_good_def {
                        skipped += 1;
                        continue;
                    }

                    col_requests.push(arcana_core::enrichment::EnrichmentRequest {
                        table_name: table.name.clone(),
                        column_names: all_col_names.clone(),
                        upstream_tables: vec![],
                        column_name: Some(col.name.clone()),
                    });
                    col_ids.push(col.id);

                    if col_requests.len() >= batch_size {
                        let count = ops::write_enrichment_batch(
                            &store, &*provider, &col_requests, &col_ids, dry_run,
                        ).await?;
                        col_count += count;
                        col_requests.clear();
                        col_ids.clear();
                    }
                }

                if !col_requests.is_empty() {
                    let count = ops::write_enrichment_batch(
                        &store, &*provider, &col_requests, &col_ids, dry_run,
                    ).await?;
                    col_count += count;
                }
            }
        }
    }

    if dry_run {
        println!(
            "Would enrich {} tables, {} columns ({} skipped — already have definitions).",
            table_count, col_count, skipped
        );
    } else {
        println!(
            "Enriched {} tables, {} columns ({} skipped — already have definitions).",
            table_count, col_count, skipped
        );
        println!("Run `arcana reembed` to generate embeddings for the new definitions.");
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Background sync worker
// ---------------------------------------------------------------------------

/// Parse a human-readable duration string like "6h", "30m", "1d".
fn parse_duration(s: &str) -> Option<std::time::Duration> {
    let s = s.trim();
    if s == "0" || s.is_empty() {
        return None;
    }
    let (num_str, unit) = if let Some(n) = s.strip_suffix('d') {
        (n, 'd')
    } else if let Some(n) = s.strip_suffix('h') {
        (n, 'h')
    } else if let Some(n) = s.strip_suffix('m') {
        (n, 'm')
    } else if let Some(n) = s.strip_suffix('s') {
        (n, 's')
    } else {
        return None;
    };
    let num: u64 = num_str.parse().ok()?;
    let secs = match unit {
        'd' => num * 86400,
        'h' => num * 3600,
        'm' => num * 60,
        's' => num,
        _ => return None,
    };
    Some(std::time::Duration::from_secs(secs))
}

/// Background worker that listens for sync triggers (from webhook or timer)
/// and runs sync + optional enrich + reembed.
#[allow(clippy::too_many_arguments)]
async fn background_sync_worker(
    mut rx: tokio::sync::mpsc::Receiver<()>,
    store: Arc<dyn arcana_core::store::MetadataStore>,
    dbt_cfg: Option<DbtSectionConfig>,
    auto_enrich: bool,
    auto_reembed: bool,
    embed_api_key: Option<String>,
    embed_model: Option<String>,
    embed_dimensions: Option<usize>,
    enrichment_api_key: Option<String>,
    enrichment_model: Option<String>,
    enrichment_batch_size: Option<usize>,
) {
    while rx.recv().await.is_some() {
        tracing::info!("background sync starting");
        if let Err(e) = run_background_sync(
            &store,
            &dbt_cfg,
            auto_enrich,
            auto_reembed,
            &embed_api_key,
            embed_model.as_deref(),
            embed_dimensions,
            &enrichment_api_key,
            enrichment_model.as_deref(),
            enrichment_batch_size,
        )
        .await
        {
            tracing::error!("background sync failed: {e}");
        } else {
            tracing::info!("background sync complete");
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_background_sync(
    store: &Arc<dyn arcana_core::store::MetadataStore>,
    dbt_cfg: &Option<DbtSectionConfig>,
    auto_enrich: bool,
    auto_reembed: bool,
    embed_api_key: &Option<String>,
    embed_model: Option<&str>,
    embed_dimensions: Option<usize>,
    enrichment_api_key: &Option<String>,
    enrichment_model: Option<&str>,
    enrichment_batch_size: Option<usize>,
) -> Result<()> {
    // dbt sync
    if let Some(dbt) = dbt_cfg {
        let ds_id = get_or_create_data_source(
            store.as_ref(),
            "dbt",
            arcana_core::entities::DataSourceType::Dbt,
        )
        .await?;

        let mut adapter_cfg =
            arcana_adapters::dbt::DbtConfig::new(&dbt.project_path, ds_id);
        if let Some(mp) = &dbt.manifest_path {
            adapter_cfg.manifest_path = mp.clone();
        }
        if let Some(cp) = &dbt.catalog_path {
            adapter_cfg.catalog_path = cp.clone();
        }

        // Incremental sync
        let known: std::collections::HashMap<String, String> = store
            .list_sync_checksums("dbt")
            .await?
            .into_iter()
            .collect();
        let manifest_path = std::path::Path::new(&adapter_cfg.manifest_path);
        let output = arcana_adapters::dbt::manifest::parse_manifest_incremental(
            manifest_path,
            ds_id,
            &known,
        )
        .await?;

        upsert_sync_output(store.as_ref(), &output).await?;
        for (key, checksum) in &output.changed_checksums {
            store.upsert_sync_checksum("dbt", key, checksum).await?;
        }
        tracing::info!(
            tables = output.tables.len(),
            columns = output.columns.len(),
            "dbt sync complete"
        );
    }

    // Auto-enrich
    if auto_enrich {
        let api_key = enrichment_api_key
            .clone()
            .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok())
            .or_else(|| std::env::var("ANTHROPIC_ARCANA_KEY").ok());

        if let Some(key) = api_key {
            let model = enrichment_model.unwrap_or("claude-haiku-4-5-20251001");
            let batch_size = enrichment_batch_size.unwrap_or(20);
            let provider = arcana_core::enrichment::claude::ClaudeEnrichmentProvider::new(
                key,
                model.to_string(),
                batch_size,
            );
            tracing::info!("running auto-enrich");

            let (requests, ids, _skipped) =
                ops::collect_table_enrichment_targets(store.as_ref(), None).await?;
            for chunk_start in (0..requests.len()).step_by(batch_size) {
                let chunk_end = (chunk_start + batch_size).min(requests.len());
                ops::write_enrichment_batch(
                    store.as_ref(),
                    &provider,
                    &requests[chunk_start..chunk_end],
                    &ids[chunk_start..chunk_end],
                    false,
                )
                .await?;
            }
        }
    }

    // Auto-reembed
    if auto_reembed {
        let api_key = embed_api_key
            .clone()
            .or_else(|| std::env::var("OPENAI_API_KEY").ok());

        let provider: Box<dyn arcana_core::embeddings::EmbeddingProvider> = if let Some(key) = api_key {
            let model = embed_model.unwrap_or("text-embedding-3-small");
            let dimensions = embed_dimensions.unwrap_or(1536);
            Box::new(arcana_core::embeddings::openai::OpenAiEmbeddingProvider::new(
                key, model, dimensions,
            ))
        } else {
            let dimensions = embed_dimensions.unwrap_or(384);
            tracing::info!("auto-reembed: no OpenAI key, using local embedding provider");
            Box::new(arcana_core::embeddings::LocalEmbeddingProvider::new(dimensions))
        };
        tracing::info!("running auto-reembed");
        ops::reembed_definitions(store.as_ref(), provider.as_ref(), None, 100).await?;
    }

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

/// Load the vector index from disk if a persist_path is configured and the file exists,
/// otherwise warm it from stored embeddings in SQLite.
async fn load_or_warm_index(
    cfg: &AppConfig,
    store: &dyn arcana_core::store::MetadataStore,
    dimensions: usize,
) -> Result<arcana_core::embeddings::VectorIndex> {
    if let Some(persist_path) = &cfg.index.persist_path {
        let path = std::path::Path::new(persist_path);
        if path.exists() {
            match arcana_core::embeddings::VectorIndex::load(path) {
                Ok(index) => {
                    println!("Loaded index from {} ({} vectors).", persist_path, index.len());
                    return Ok(index);
                }
                Err(e) => {
                    tracing::warn!("Failed to load persisted index, warming from SQLite: {e}");
                }
            }
        }
    }

    let index = arcana_core::embeddings::VectorIndex::new(dimensions);
    let all_defs = store.list_all_semantic_definitions().await?;
    let mut count = 0usize;
    for def in &all_defs {
        if let Some(emb) = &def.embedding {
            index.upsert(def.entity_id, emb.clone())?;
            count += 1;
        }
    }
    if count > 0 {
        println!("Warmed index from SQLite ({count} vectors).");
    }
    Ok(index)
}

fn build_embedding_provider(
    cfg: &AppConfig,
) -> Result<Arc<dyn arcana_core::embeddings::EmbeddingProvider>> {
    let api_key = cfg
        .embeddings
        .openai_api_key
        .clone()
        .or_else(|| std::env::var("OPENAI_API_KEY").ok());

    match api_key {
        Some(key) => {
            let model = cfg
                .embeddings
                .openai_model
                .as_deref()
                .unwrap_or("text-embedding-3-small");
            let dimensions = cfg.embeddings.dimensions.unwrap_or(1536);
            Ok(Arc::new(arcana_core::embeddings::openai::OpenAiEmbeddingProvider::new(
                key, model, dimensions,
            )))
        }
        None => {
            let dimensions = cfg.embeddings.dimensions.unwrap_or(384);
            tracing::warn!(
                "No OpenAI API key found — using local n-gram hash embeddings ({dimensions}d). \
                 Set OPENAI_API_KEY or embeddings.openai_api_key for higher quality embeddings."
            );
            println!(
                "Warning: Using local embedding provider (no OpenAI API key). \
                 Quality will be lower than neural embeddings."
            );
            Ok(Arc::new(arcana_core::embeddings::LocalEmbeddingProvider::new(dimensions)))
        }
    }
}

async fn cmd_dedup(cfg: &AppConfig, threshold: f64, dry_run: bool) -> Result<()> {
    let store: Arc<dyn MetadataStore> =
        Arc::new(arcana_core::store::sqlite::SqliteStore::open(&cfg.database.url).await?);

    let dimensions = cfg.embeddings.dimensions.unwrap_or(1536);
    let entity_index = Arc::new(arcana_core::embeddings::VectorIndex::new(dimensions));

    // Warm the index from stored embeddings
    let all_defs = store.list_all_semantic_definitions().await?;
    for def in &all_defs {
        if let Some(emb) = &def.embedding {
            entity_index.upsert(def.entity_id, emb.clone())?;
        }
    }

    println!(
        "Searching for duplicate tables (threshold: {:.2}, {} embeddings loaded)...",
        threshold,
        entity_index.len()
    );

    let clusters =
        arcana_recommender::dedup::find_clusters(store.as_ref(), &entity_index, threshold).await?;

    if clusters.is_empty() {
        println!("No duplicate clusters found at threshold {:.2}.", threshold);
        return Ok(());
    }

    println!("Found {} cluster(s):\n", clusters.len());

    for (i, cluster) in clusters.iter().enumerate() {
        println!("--- Cluster {} ({} tables) ---", i + 1, cluster.tables.len());
        for (table, sim) in &cluster.tables {
            let canonical_marker = if table.id == cluster.suggested_canonical {
                " [canonical]"
            } else {
                ""
            };
            println!(
                "  {} (similarity: {:.3}, confidence: {:.2}){}",
                table.name, sim, table.confidence, canonical_marker
            );
        }
        println!();
    }

    if dry_run {
        println!("Dry run — no changes persisted.");
        return Ok(());
    }

    // Persist clusters to the store
    store.clear_table_clusters().await?;
    for cluster in &clusters {
        let tc = arcana_core::entities::TableCluster {
            id: uuid::Uuid::new_v4(),
            label: None,
            canonical_id: Some(cluster.suggested_canonical),
            threshold,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        store.upsert_table_cluster(&tc).await?;

        for (table, sim) in &cluster.tables {
            let member = arcana_core::entities::TableClusterMember {
                cluster_id: tc.id,
                table_id: table.id,
                similarity: *sim,
            };
            store.upsert_cluster_member(&member).await?;
        }
    }

    println!(
        "Persisted {} cluster(s) to the store. Non-canonical tables will show warnings in get_context.",
        clusters.len()
    );
    Ok(())
}

fn build_enrichment_provider(
    cfg: &AppConfig,
) -> Result<Arc<dyn arcana_core::enrichment::EnrichmentProvider>> {
    let api_key = cfg
        .enrichment
        .anthropic_api_key
        .clone()
        .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok())
        .or_else(|| std::env::var("ANTHROPIC_ARCANA_KEY").ok())
        .context("Anthropic API key required — set enrichment.anthropic_api_key in config or ANTHROPIC_API_KEY / ANTHROPIC_ARCANA_KEY env var")?;

    let model = cfg
        .enrichment
        .model
        .as_deref()
        .unwrap_or("claude-haiku-4-5-20251001")
        .to_string();
    let batch_size = cfg.enrichment.batch_size.unwrap_or(20);

    Ok(Arc::new(
        arcana_core::enrichment::claude::ClaudeEnrichmentProvider::new(api_key, model, batch_size),
    ))
}

fn build_snowflake_config(
    sf_cfg: &SnowflakeSectionConfig,
) -> arcana_adapters::snowflake::SnowflakeConfig {
    arcana_adapters::snowflake::SnowflakeConfig {
        account: sf_cfg.account.clone(),
        warehouse: sf_cfg.warehouse.clone(),
        database: sf_cfg.database.clone(),
        schema: sf_cfg.schema.clone(),
        user: sf_cfg.user.clone(),
        private_key_path: sf_cfg.private_key_path.clone(),
        password: sf_cfg.password.clone(),
        role: sf_cfg.role.clone(),
    }
}

async fn get_or_create_data_source(
    store: &dyn arcana_core::store::MetadataStore,
    name: &str,
    source_type: arcana_core::entities::DataSourceType,
) -> Result<uuid::Uuid> {
    let existing = store.list_data_sources().await?;
    if let Some(ds) = existing.iter().find(|ds| ds.name == name) {
        return Ok(ds.id);
    }

    let ds = arcana_core::entities::DataSource {
        id: uuid::Uuid::new_v4(),
        name: name.to_string(),
        source_type,
        connection_info: serde_json::json!({}),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    store.upsert_data_source(&ds).await?;
    Ok(ds.id)
}

async fn upsert_sync_output(
    store: &dyn arcana_core::store::MetadataStore,
    output: &arcana_adapters::adapter::SyncOutput,
) -> Result<()> {
    for schema in &output.schemas {
        store.upsert_schema(schema).await
            .with_context(|| format!("upserting schema {}", schema.schema_name))?;
    }
    for table in &output.tables {
        store.upsert_table(table).await
            .with_context(|| format!("upserting table {} (schema_id={})", table.name, table.schema_id))?;
    }
    let mut col_skipped = 0usize;
    for column in &output.columns {
        if let Err(e) = store.upsert_column(column).await {
            tracing::debug!("skipping column {} (table_id={}): {e}", column.name, column.table_id);
            col_skipped += 1;
        }
    }
    if col_skipped > 0 {
        tracing::warn!("{col_skipped} columns skipped (missing parent table — likely name collisions from versioned models)");
    }
    let mut edge_skipped = 0usize;
    for edge in &output.lineage_edges {
        if let Err(e) = store.upsert_lineage_edge(edge).await {
            tracing::debug!("skipping lineage edge {} -> {}: {e}", edge.upstream_id, edge.downstream_id);
            edge_skipped += 1;
        }
    }
    if edge_skipped > 0 {
        tracing::warn!("{edge_skipped} lineage edges skipped (missing referenced tables)");
    }
    for def in &output.semantic_definitions {
        store.upsert_semantic_definition(def).await
            .with_context(|| format!("upserting definition for entity {}", def.entity_id))?;
    }
    for metric in &output.metrics {
        store.upsert_metric(metric).await
            .with_context(|| format!("upserting metric {}", metric.name))?;
    }
    Ok(())
}
