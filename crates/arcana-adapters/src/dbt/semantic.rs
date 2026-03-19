use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;

use arcana_core::entities::{Metric, MetricType};

/// Parse dbt semantic model YAML files and extract metric definitions.
///
/// dbt Semantic Layer uses `semantic_models:` and `metrics:` blocks in YAML.
pub async fn parse_semantic_models(yaml_path: &Path) -> Result<Vec<Metric>> {
    let raw = tokio::fs::read_to_string(yaml_path)
        .await
        .with_context(|| format!("failed to read semantic model YAML at {:?}", yaml_path))?;

    let doc: SemanticModelFile =
        serde_yaml_ng::from_str(&raw).context("failed to parse semantic model YAML")?;

    let mut metrics = Vec::new();

    for m in doc.metrics.unwrap_or_default() {
        let metric_type = match m.metric_type.as_deref() {
            Some("simple") => MetricType::Simple,
            Some("ratio") => MetricType::Ratio,
            Some("cumulative") => MetricType::Cumulative,
            Some("derived") => MetricType::Derived,
            _ => MetricType::Simple,
        };

        metrics.push(Metric {
            id: uuid::Uuid::new_v4(),
            name: m.name,
            label: m.label,
            description: m.description,
            metric_type,
            source_table_id: None, // resolve after table sync
            expression: m.measure.map(|ms| ms.name),
            dimensions: m.dimensions.unwrap_or_default(),
            filters: m.filter.map(|f| serde_json::Value::String(f)),
            confidence: 1.0,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        });
    }

    Ok(metrics)
}

#[derive(Debug, Deserialize)]
struct SemanticModelFile {
    metrics: Option<Vec<DbtMetricDef>>,
}

#[derive(Debug, Deserialize)]
struct DbtMetricDef {
    name: String,
    label: Option<String>,
    description: Option<String>,
    #[serde(rename = "type")]
    metric_type: Option<String>,
    measure: Option<DbtMeasureRef>,
    dimensions: Option<Vec<String>>,
    filter: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DbtMeasureRef {
    name: String,
}
