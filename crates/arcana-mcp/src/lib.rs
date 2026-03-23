//! # arcana-mcp
//!
//! MCP server implementation for Arcana. Exposes 6 tools (`get_context`,
//! `describe_table`, `estimate_cost`, `update_context`, `find_similar_tables`,
//! `report_outcome`) over stdio and HTTP/SSE transports.

/// Admin HTTP API (`/health`, `/api/admin/stats`, `/api/sync`).
pub mod admin;
/// MCP server setup and transport handling.
pub mod server;
/// Tool definitions and handlers.
pub mod tools;

pub use admin::{AdminState, admin_router};
pub use server::ArcanaServer;
