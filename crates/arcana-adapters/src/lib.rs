//! # arcana-adapters
//!
//! Structured metadata adapters that pull schema, column, lineage, and semantic
//! information from external systems (Snowflake, dbt) into the Arcana store.

/// The [`MetadataAdapter`] trait that all adapters implement.
pub mod adapter;
/// dbt adapter — parses `manifest.json`, `catalog.json`, and schema YAML.
pub mod dbt;
/// Snowflake adapter — reads `INFORMATION_SCHEMA`, `ACCOUNT_USAGE`, and `EXPLAIN`.
pub mod snowflake;

pub use adapter::MetadataAdapter;
