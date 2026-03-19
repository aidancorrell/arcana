pub mod catalog;
pub mod manifest;
pub mod semantic;

use anyhow::Result;
use async_trait::async_trait;
use std::path::PathBuf;

use crate::adapter::{MetadataAdapter, SyncOutput};

/// Configuration for the dbt adapter.
#[derive(Debug, Clone)]
pub struct DbtConfig {
    pub project_path: PathBuf,
    pub manifest_path: PathBuf,
    pub catalog_path: PathBuf,
}

impl DbtConfig {
    pub fn new(project_path: impl Into<PathBuf>) -> Self {
        let project_path = project_path.into();
        let manifest_path = project_path.join("target/manifest.json");
        let catalog_path = project_path.join("target/catalog.json");
        Self {
            project_path,
            manifest_path,
            catalog_path,
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
        let manifest_output = manifest::parse_manifest(&self.config.manifest_path).await?;
        let catalog_output = catalog::parse_catalog(&self.config.catalog_path).await?;

        // Merge manifest + catalog outputs
        let mut output = manifest_output;
        // Enrich columns with catalog type info
        output.columns.extend(catalog_output.columns);
        output.stats.columns_upserted += catalog_output.stats.columns_upserted;

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
