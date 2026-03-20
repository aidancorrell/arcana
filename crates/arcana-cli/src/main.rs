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

#[derive(Debug, serde::Deserialize)]
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
        adapter_filter.map_or(true, |f| f == name)
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

            let adapter = arcana_adapters::dbt::DbtAdapter::new(adapter_cfg);
            let output = adapter.sync().await?;

            upsert_sync_output(&*store, &output).await?;
            total_tables += output.tables.len();
            total_columns += output.columns.len();
            println!(
                "    dbt: {} schemas, {} tables, {} columns, {} definitions",
                output.schemas.len(),
                output.tables.len(),
                output.columns.len(),
                output.semantic_definitions.len()
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

async fn cmd_serve(cfg: &AppConfig, bind_override: Option<&str>, stdio: bool) -> Result<()> {
    let store: Arc<dyn arcana_core::store::MetadataStore> = Arc::new(open_store(cfg).await?);
    let embedding_provider = build_embedding_provider(cfg)?;
    let dimensions = cfg.embeddings.dimensions.unwrap_or(1536);

    let entity_index = Arc::new(arcana_core::embeddings::VectorIndex::new(dimensions));
    let chunk_index = Arc::new(arcana_core::embeddings::VectorIndex::new(dimensions));

    // Warm the in-memory index from stored embeddings
    let all_defs = store.list_all_semantic_definitions().await?;
    for def in &all_defs {
        if let Some(emb) = &def.embedding {
            entity_index.upsert(def.entity_id, emb.clone())?;
        }
    }

    let ranker = Arc::new(arcana_recommender::RelevanceRanker::new(
        store.clone(),
        embedding_provider,
        entity_index,
        chunk_index,
    ));

    let serializer = Arc::new(arcana_recommender::ContextSerializer {
        max_tokens: cfg.mcp.max_context_tokens,
        ..Default::default()
    });

    let mut server = arcana_mcp::ArcanaServer::new(store, ranker, serializer);

    // Attach Snowflake config if present (enables estimate_cost tool)
    if let Some(sf_cfg) = &cfg.snowflake {
        server = server.with_snowflake_config(build_snowflake_config(sf_cfg));
    }

    if stdio {
        println!("Starting Arcana MCP server on stdio...");
        server.serve_stdio().await
    } else {
        let bind_addr = bind_override.unwrap_or(&cfg.mcp.bind_addr);
        anyhow::bail!(
            "TCP MCP server not yet supported. Use --stdio for Claude Desktop integration. (bind_addr: {bind_addr})"
        );
    }
}

async fn cmd_ask(cfg: &AppConfig, query: &str, top_k: usize, format: &str) -> Result<()> {
    let store: Arc<dyn arcana_core::store::MetadataStore> = Arc::new(open_store(cfg).await?);
    let embedding_provider = build_embedding_provider(cfg)?;
    let dimensions = cfg.embeddings.dimensions.unwrap_or(1536);

    let entity_index = Arc::new(arcana_core::embeddings::VectorIndex::new(dimensions));
    let chunk_index = Arc::new(arcana_core::embeddings::VectorIndex::new(dimensions));

    // Warm the in-memory index from stored embeddings
    let all_defs = store.list_all_semantic_definitions().await?;
    for def in &all_defs {
        if let Some(emb) = &def.embedding {
            entity_index.upsert(def.entity_id, emb.clone())?;
        }
    }

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

async fn cmd_status(cfg: &AppConfig) -> Result<()> {
    let store = open_store(cfg).await?;

    let data_sources = store.list_data_sources().await?;
    let mut table_count = 0usize;
    let mut column_count = 0usize;

    for ds in &data_sources {
        let schemas = store.list_schemas(ds.id).await?;
        for schema in &schemas {
            let tables = store.list_tables(schema.id).await?;
            for table in &tables {
                table_count += 1;
                let cols = store.list_columns(table.id).await?;
                column_count += cols.len();
            }
        }
    }

    let metrics = store.list_metrics().await?;

    println!("Arcana Status");
    println!("=============");
    println!("  Data sources:  {}", data_sources.len());
    println!("  Tables:        {}", table_count);
    println!("  Columns:       {}", column_count);
    println!("  Metrics:       {}", metrics.len());

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

    let threshold = below_confidence.unwrap_or(f64::MAX);

    // Re-embed semantic definitions
    let data_sources = store.list_data_sources().await?;
    let mut def_count = 0usize;

    for ds in &data_sources {
        let schemas = store.list_schemas(ds.id).await?;
        for schema in &schemas {
            let tables = store.list_tables(schema.id).await?;
            for table in &tables {
                let defs = store.get_semantic_definitions(table.id).await?;
                let mut batch_texts: Vec<String> = Vec::new();
                let mut batch_defs: Vec<arcana_core::entities::SemanticDefinition> = Vec::new();

                for def in defs {
                    if def.confidence < threshold {
                        batch_texts.push(def.definition.clone());
                        batch_defs.push(def);
                    }

                    if batch_texts.len() >= batch_size {
                        let refs: Vec<&str> =
                            batch_texts.iter().map(|s| s.as_str()).collect();
                        let embeddings = embedding_provider.embed_batch(&refs).await?;
                        for (mut d, emb) in
                            batch_defs.drain(..).zip(embeddings.into_iter())
                        {
                            d.embedding = Some(emb);
                            store.upsert_semantic_definition(&d).await?;
                            def_count += 1;
                        }
                        batch_texts.clear();
                    }
                }

                // Flush remaining batch
                if !batch_texts.is_empty() {
                    let refs: Vec<&str> = batch_texts.iter().map(|s| s.as_str()).collect();
                    let embeddings = embedding_provider.embed_batch(&refs).await?;
                    for (mut d, emb) in batch_defs.drain(..).zip(embeddings.into_iter()) {
                        d.embedding = Some(emb);
                        store.upsert_semantic_definition(&d).await?;
                        def_count += 1;
                    }
                }

                // Also re-embed columns
                let col_defs = store.list_columns(table.id).await?;
                for col in &col_defs {
                    let cdefs = store.get_semantic_definitions(col.id).await?;
                    for mut cdef in cdefs {
                        if cdef.confidence < threshold {
                            let emb = embedding_provider.embed(&cdef.definition).await?;
                            cdef.embedding = Some(emb);
                            store.upsert_semantic_definition(&cdef).await?;
                            def_count += 1;
                        }
                    }
                }
            }
        }
    }

    println!("  Re-embedded {} semantic definitions.", def_count);
    println!("Done.");

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

fn build_embedding_provider(
    cfg: &AppConfig,
) -> Result<Arc<dyn arcana_core::embeddings::EmbeddingProvider>> {
    let api_key = cfg
        .embeddings
        .openai_api_key
        .clone()
        .or_else(|| std::env::var("OPENAI_API_KEY").ok())
        .context("OpenAI API key required — set embeddings.openai_api_key in config or OPENAI_API_KEY env var")?;

    let model = cfg
        .embeddings
        .openai_model
        .as_deref()
        .unwrap_or("text-embedding-3-small");
    let dimensions = cfg.embeddings.dimensions.unwrap_or(1536);

    Ok(Arc::new(arcana_core::embeddings::openai::OpenAiEmbeddingProvider::new(
        api_key, model, dimensions,
    )))
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
        store.upsert_schema(schema).await?;
    }
    for table in &output.tables {
        store.upsert_table(table).await?;
    }
    for column in &output.columns {
        store.upsert_column(column).await?;
    }
    for edge in &output.lineage_edges {
        store.upsert_lineage_edge(edge).await?;
    }
    for def in &output.semantic_definitions {
        store.upsert_semantic_definition(def).await?;
    }
    for metric in &output.metrics {
        store.upsert_metric(metric).await?;
    }
    Ok(())
}
