use anyhow::Result;
use arcana_core::store::MetadataStore;
use arcana_recommender::{ranker::RelevanceRanker, serializer::ContextSerializer};
use rmcp::{
    ServerHandler,
    model::{
        CallToolRequestParam, CallToolResult, Content, Implementation, ListToolsResult,
        PaginatedRequestParam, ProtocolVersion, ServerCapabilities, ServerInfo, Tool,
    },
    service::RequestContext,
    transport::SseServer,
    RoleServer,
};
use serde_json::Value;
use std::net::SocketAddr;
use std::sync::Arc;

use arcana_core::embeddings::VectorIndex;
use crate::tools::{
    DescribeTableInput, EstimateCostInput, FindSimilarTablesInput, GetContextInput,
    ReportOutcomeInput, UpdateContextInput, handle_describe_table, handle_estimate_cost,
    handle_find_similar_tables, handle_get_context, handle_report_outcome, handle_update_context,
};

/// The Arcana MCP server — exposes metadata context to AI agents via the Model Context Protocol.
#[derive(Clone)]
pub struct ArcanaServer {
    store: Arc<dyn MetadataStore>,
    ranker: Arc<RelevanceRanker>,
    serializer: Arc<ContextSerializer>,
    entity_index: Arc<VectorIndex>,
    snowflake_config: Option<Arc<arcana_adapters::snowflake::SnowflakeConfig>>,
}

impl ArcanaServer {
    pub fn new(
        store: Arc<dyn MetadataStore>,
        ranker: Arc<RelevanceRanker>,
        serializer: Arc<ContextSerializer>,
        entity_index: Arc<VectorIndex>,
    ) -> Self {
        Self { store, ranker, serializer, entity_index, snowflake_config: None }
    }

    pub fn with_snowflake_config(
        mut self,
        config: arcana_adapters::snowflake::SnowflakeConfig,
    ) -> Self {
        self.snowflake_config = Some(Arc::new(config));
        self
    }

    /// Start the MCP server on stdio (for Claude Desktop / Cursor integration).
    pub async fn serve_stdio(self) -> Result<()> {
        let transport = (tokio::io::stdin(), tokio::io::stdout());
        rmcp::service::serve_server(self, transport)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?
            .waiting()
            .await
            .map(|_| ())
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    /// Start the MCP server over HTTP+SSE transport (multi-user mode).
    ///
    /// Clients connect via `GET /sse` for server-sent events and `POST /message`
    /// to send requests. Each SSE connection gets its own MCP session sharing
    /// the same underlying store and index.
    pub async fn serve_http(self, bind_addr: &str) -> Result<()> {
        let addr: SocketAddr = bind_addr
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid bind address '{bind_addr}': {e}"))?;

        tracing::info!(%addr, "starting MCP SSE server");

        let sse_server = SseServer::serve(addr)
            .await
            .map_err(|e| anyhow::anyhow!("failed to bind SSE server on {addr}: {e}"))?;

        let ct = sse_server.config.ct.clone();
        let server = self;
        let _ct = sse_server.with_service(move || server.clone());

        // Log connection info
        eprintln!("Arcana MCP server listening on {addr}");
        eprintln!("  SSE endpoint:     http://{addr}/sse");
        eprintln!("  Message endpoint: http://{addr}/message");

        // Block until ctrl-c
        tokio::signal::ctrl_c()
            .await
            .map_err(|e| anyhow::anyhow!("signal error: {e}"))?;

        tracing::info!("shutting down MCP server");
        ct.cancel();

        Ok(())
    }

    fn tool_list() -> Vec<Tool> {
        vec![
            Tool {
                name: "get_context".into(),
                description: "Search for relevant metadata context about tables, columns, and \
                     definitions for a natural-language query."
                    .into(),
                input_schema: Arc::new(
                    serde_json::from_value(serde_json::json!({
                        "type": "object",
                        "properties": {
                            "query": {"type": "string"},
                            "top_k": {"type": "integer", "default": 10},
                            "min_confidence": {"type": "number", "default": 0.0},
                            "expand_lineage": {"type": "boolean", "default": true}
                        },
                        "required": ["query"]
                    }))
                    .unwrap(),
                ),
            },
            Tool {
                name: "describe_table".into(),
                description: "Get detailed metadata about a specific table: columns, semantic \
                     definitions, data contracts, lineage, and usage stats."
                    .into(),
                input_schema: Arc::new(
                    serde_json::from_value(serde_json::json!({
                        "type": "object",
                        "properties": {
                            "table_ref": {"type": "string"},
                            "include_columns": {"type": "boolean", "default": true},
                            "include_definitions": {"type": "boolean", "default": true},
                            "include_contracts": {"type": "boolean", "default": false},
                            "include_lineage": {"type": "boolean", "default": false}
                        },
                        "required": ["table_ref"]
                    }))
                    .unwrap(),
                ),
            },
            Tool {
                name: "estimate_cost".into(),
                description: "Estimate the Snowflake compute cost for running a SQL query.".into(),
                input_schema: Arc::new(
                    serde_json::from_value(serde_json::json!({
                        "type": "object",
                        "properties": {
                            "sql": {"type": "string"},
                            "warehouse_size": {"type": "string", "default": "SMALL"}
                        },
                        "required": ["sql"]
                    }))
                    .unwrap(),
                ),
            },
            Tool {
                name: "update_context".into(),
                description: "Push a semantic definition or annotation back into Arcana.".into(),
                input_schema: Arc::new(
                    serde_json::from_value(serde_json::json!({
                        "type": "object",
                        "properties": {
                            "entity_id": {"type": "string", "format": "uuid"},
                            "entity_type": {"type": "string", "enum": ["table", "column", "metric"]},
                            "definition": {"type": "string"},
                            "confidence": {"type": "number", "default": 0.8}
                        },
                        "required": ["entity_id", "entity_type", "definition"]
                    }))
                    .unwrap(),
                ),
            },
            Tool {
                name: "find_similar_tables".into(),
                description: "Find tables semantically similar to a given table. Useful for \
                     detecting duplicates or finding related tables."
                    .into(),
                input_schema: Arc::new(
                    serde_json::from_value(serde_json::json!({
                        "type": "object",
                        "properties": {
                            "table_ref": {"type": "string", "description": "Table name or UUID"},
                            "threshold": {"type": "number", "default": 0.85},
                            "limit": {"type": "integer", "default": 5}
                        },
                        "required": ["table_ref"]
                    }))
                    .unwrap(),
                ),
            },
            Tool {
                name: "report_outcome".into(),
                description: "Report whether a query using Arcana context succeeded or failed. \
                     Boosts or reduces confidence on the entities that were in the context."
                    .into(),
                input_schema: Arc::new(
                    serde_json::from_value(serde_json::json!({
                        "type": "object",
                        "properties": {
                            "entity_ids": {
                                "type": "array",
                                "items": {"type": "string", "format": "uuid"},
                                "description": "Entity IDs that were in the context"
                            },
                            "outcome": {
                                "type": "string",
                                "enum": ["success", "failure"],
                                "description": "Whether the query succeeded or failed"
                            },
                            "query_text": {
                                "type": "string",
                                "description": "The SQL query that was executed (optional)"
                            }
                        },
                        "required": ["entity_ids", "outcome"]
                    }))
                    .unwrap(),
                ),
            },
        ]
    }
}

impl ServerHandler for ArcanaServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::default(),
            capabilities: ServerCapabilities {
                tools: Some(rmcp::model::ToolsCapability { list_changed: Some(false) }),
                ..Default::default()
            },
            server_info: Implementation {
                name: "arcana".into(),
                version: env!("CARGO_PKG_VERSION").into(),
            },
            instructions: Some(
                "Arcana provides semantic metadata context for your data warehouse. \
                 Use get_context to find relevant tables/columns, describe_table for \
                 full table metadata, estimate_cost to preview query costs, and \
                 update_context to push new definitions back."
                    .into(),
            ),
        }
    }

    fn list_tools(
        &self,
        _request: PaginatedRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListToolsResult, rmcp::Error>> + Send + '_ {
        std::future::ready(Ok(ListToolsResult {
            tools: Self::tool_list(),
            next_cursor: None,
        }))
    }

    fn call_tool(
        &self,
        request: CallToolRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<CallToolResult, rmcp::Error>> + Send + '_ {
        let store = self.store.clone();
        let ranker = self.ranker.clone();
        let serializer = self.serializer.clone();
        let entity_index = self.entity_index.clone();
        let snowflake_config = self.snowflake_config.clone();

        async move {
            let result: anyhow::Result<String> = match request.name.as_ref() {
                "get_context" => {
                    let input: GetContextInput =
                        serde_json::from_value(Value::Object(
                            request.arguments.unwrap_or_default(),
                        ))
                        .map_err(|e| rmcp::Error::invalid_params(e.to_string(), None))?;
                    handle_get_context(input, ranker, serializer)
                        .await
                        .map(|o| serde_json::to_string_pretty(&o).unwrap_or_default())
                }
                "describe_table" => {
                    let input: DescribeTableInput =
                        serde_json::from_value(Value::Object(
                            request.arguments.unwrap_or_default(),
                        ))
                        .map_err(|e| rmcp::Error::invalid_params(e.to_string(), None))?;
                    handle_describe_table(input, store)
                        .await
                        .map(|o| serde_json::to_string_pretty(&o).unwrap_or_default())
                }
                "estimate_cost" => {
                    let input: EstimateCostInput =
                        serde_json::from_value(Value::Object(
                            request.arguments.unwrap_or_default(),
                        ))
                        .map_err(|e| rmcp::Error::invalid_params(e.to_string(), None))?;
                    handle_estimate_cost(input, snowflake_config)
                        .await
                        .map(|o| serde_json::to_string_pretty(&o).unwrap_or_default())
                }
                "update_context" => {
                    let input: UpdateContextInput =
                        serde_json::from_value(Value::Object(
                            request.arguments.unwrap_or_default(),
                        ))
                        .map_err(|e| rmcp::Error::invalid_params(e.to_string(), None))?;
                    handle_update_context(input, store)
                        .await
                        .map(|o| serde_json::to_string_pretty(&o).unwrap_or_default())
                }
                "find_similar_tables" => {
                    let input: FindSimilarTablesInput =
                        serde_json::from_value(Value::Object(
                            request.arguments.unwrap_or_default(),
                        ))
                        .map_err(|e| rmcp::Error::invalid_params(e.to_string(), None))?;
                    handle_find_similar_tables(input, store, entity_index)
                        .await
                        .map(|o| serde_json::to_string_pretty(&o).unwrap_or_default())
                }
                "report_outcome" => {
                    let input: ReportOutcomeInput =
                        serde_json::from_value(Value::Object(
                            request.arguments.unwrap_or_default(),
                        ))
                        .map_err(|e| rmcp::Error::invalid_params(e.to_string(), None))?;
                    handle_report_outcome(input, store)
                        .await
                        .map(|o| serde_json::to_string_pretty(&o).unwrap_or_default())
                }
                name => {
                    return Err(rmcp::Error::invalid_params(
                        format!("unknown tool: {name}"),
                        None,
                    ));
                }
            };

            match result {
                Ok(text) => Ok(CallToolResult {
                    content: vec![Content::text(text)],
                    is_error: Some(false),
                }),
                Err(e) => Ok(CallToolResult {
                    content: vec![Content::text(format!("Error: {e}"))],
                    is_error: Some(true),
                }),
            }
        }
    }
}
