pub mod catalog;
pub mod manifest;
pub mod semantic;

use anyhow::Result;
use async_trait::async_trait;
use std::path::PathBuf;
use uuid::Uuid;

use crate::adapter::{MetadataAdapter, SyncOutput};

/// Configuration for the dbt adapter.
#[derive(Debug, Clone)]
pub struct DbtConfig {
    pub project_path: PathBuf,
    pub manifest_path: PathBuf,
    pub catalog_path: PathBuf,
    /// The data source ID to associate schemas with.
    pub data_source_id: Uuid,
}

impl DbtConfig {
    pub fn new(project_path: impl Into<PathBuf>, data_source_id: Uuid) -> Self {
        let project_path = project_path.into();
        let manifest_path = project_path.join("target/manifest.json");
        let catalog_path = project_path.join("target/catalog.json");
        Self {
            project_path,
            manifest_path,
            catalog_path,
            data_source_id,
        }
    }
}

/// dbt metadata adapter — reads manifest.json, catalog.json, and semantic model YAMLs.
pub struct DbtAdapter {
    config: DbtConfig,
}

impl DbtAdapter {
    pub fn new(config: DbtConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl MetadataAdapter for DbtAdapter {
    fn name(&self) -> &str {
        "dbt"
    }

    async fn sync(&self) -> Result<SyncOutput> {
        let mut output =
            manifest::parse_manifest(&self.config.manifest_path, self.config.data_source_id)
                .await?;

        // Catalog is optional — dbt docs generate may not have been run
        if self.config.catalog_path.exists() {
            catalog::enrich_from_catalog(&self.config.catalog_path, &mut output).await?;
        }

        Ok(output)
    }

    async fn health_check(&self) -> Result<()> {
        if !self.config.manifest_path.exists() {
            anyhow::bail!(
                "dbt manifest.json not found at {:?}",
                self.config.manifest_path
            );
        }
        Ok(())
    }
}
