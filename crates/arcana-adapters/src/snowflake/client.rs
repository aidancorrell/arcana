use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use super::SnowflakeConfig;

/// A lightweight client for the Snowflake SQL API (v2/statements).
///
/// Auth: uses basic username/password via a session token. For production,
/// swap to key-pair JWT — but password auth is simpler for MVP.
pub struct SnowflakeClient {
    http: Client,
    api_url: String,
    config: SnowflakeConfig,
    token: Option<String>,
}

/// Request body for the Snowflake SQL API.
#[derive(Debug, Serialize)]
struct SqlRequest {
    statement: String,
    timeout: u64,
    database: String,
    schema: String,
    warehouse: String,
    role: Option<String>,
}

/// Top-level response from the SQL API.
#[derive(Debug, Deserialize)]
pub struct SqlResponse {
    #[serde(rename = "resultSetMetaData")]
    pub result_set_metadata: ResultSetMetadata,
    #[serde(default)]
    pub data: Vec<Vec<Option<String>>>,
    pub code: Option<String>,
    pub message: Option<String>,
    #[serde(rename = "statementHandle")]
    pub statement_handle: Option<String>,
    #[serde(rename = "statementStatusUrl")]
    pub statement_status_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ResultSetMetadata {
    #[serde(rename = "numRows")]
    pub num_rows: i64,
    #[serde(default)]
    pub format: Option<String>,
    #[serde(rename = "rowType", default)]
    pub row_type: Vec<ColumnMetadata>,
}

#[derive(Debug, Deserialize)]
pub struct ColumnMetadata {
    pub name: String,
    #[serde(rename = "type")]
    pub data_type: Option<String>,
    pub nullable: Option<bool>,
}

/// Login response for session token auth.
#[derive(Debug, Deserialize)]
struct LoginResponse {
    data: Option<LoginData>,
    success: Option<bool>,
    message: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LoginData {
    token: Option<String>,
}

impl SnowflakeClient {
    pub fn new(config: SnowflakeConfig) -> Self {
        let api_url = format!(
            "https://{}.snowflakecomputing.com",
            config.account.replace('_', "-")
        );
        Self {
            http: Client::new(),
            api_url,
            config,
            token: None,
        }
    }

    /// Authenticate via username/password login endpoint and cache the session token.
    async fn login(&mut self) -> Result<()> {
        let password = self
            .config
            .password
            .as_deref()
            .context("SNOWFLAKE_PASSWORD required for login")?;

        let login_url = format!(
            "{}/session/v1/login-request?warehouse={}&databaseName={}&schemaName={}",
            self.api_url, self.config.warehouse, self.config.database, self.config.schema
        );

        let body = serde_json::json!({
            "data": {
                "LOGIN_NAME": self.config.user,
                "PASSWORD": password,
                "ACCOUNT_NAME": self.config.account,
            }
        });

        let resp: LoginResponse = self
            .http
            .post(&login_url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(&body)
            .send()
            .await
            .context("Snowflake login request failed")?
            .json()
            .await
            .context("failed to parse Snowflake login response")?;

        if resp.success != Some(true) {
            anyhow::bail!(
                "Snowflake login failed: {}",
                resp.message.unwrap_or_else(|| "unknown error".into())
            );
        }

        self.token = resp
            .data
            .and_then(|d| d.token)
            .map(|t| t.to_string());

        if self.token.is_none() {
            anyhow::bail!("Snowflake login succeeded but no token returned");
        }

        Ok(())
    }

    /// Ensure we have a valid session token.
    async fn ensure_auth(&mut self) -> Result<()> {
        if self.token.is_none() {
            self.login().await?;
        }
        Ok(())
    }

    /// Execute a SQL statement via the Snowflake SQL API and return the response.
    pub async fn execute_sql(&mut self, sql: &str) -> Result<SqlResponse> {
        self.ensure_auth().await?;

        let url = format!("{}/api/v2/statements", self.api_url);
        let token = self.token.as_deref().unwrap();

        let request = SqlRequest {
            statement: sql.to_string(),
            timeout: 60,
            database: self.config.database.clone(),
            schema: self.config.schema.clone(),
            warehouse: self.config.warehouse.clone(),
            role: self.config.role.clone(),
        };

        let resp = self
            .http
            .post(&url)
            .header("Authorization", format!("Snowflake Token=\"{}\"", token))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .header("X-Snowflake-Authorization-Token-Type", "SNOWFLAKE")
            .json(&request)
            .send()
            .await
            .context("Snowflake SQL API request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Snowflake SQL API returned {}: {}", status, body);
        }

        let response: SqlResponse = resp
            .json()
            .await
            .context("failed to parse Snowflake SQL API response")?;

        // Check for Snowflake-level errors
        if let Some(code) = &response.code {
            if code != "090001" {
                // 090001 = statement executed successfully
                anyhow::bail!(
                    "Snowflake SQL error (code {}): {}",
                    code,
                    response.message.as_deref().unwrap_or("unknown")
                );
            }
        }

        Ok(response)
    }

    /// Execute a simple query to verify connectivity.
    pub async fn health_check(&mut self) -> Result<()> {
        self.execute_sql("SELECT CURRENT_TIMESTAMP()").await?;
        Ok(())
    }

    pub fn config(&self) -> &SnowflakeConfig {
        &self.config
    }
}

/// Helper: get a column value from a row by column index.
pub fn get_cell(row: &[Option<String>], index: usize) -> Option<&str> {
    row.get(index).and_then(|v| v.as_deref())
}

/// Helper: find column index by name in result metadata.
pub fn column_index(metadata: &ResultSetMetadata, name: &str) -> Option<usize> {
    metadata
        .row_type
        .iter()
        .position(|c| c.name.eq_ignore_ascii_case(name))
}
