//! PostgreSQL access for the Dune game database.
//!
//! The vendor stack exposes the game database inside the VM and, during local
//! development, we sometimes expose it to the host for inspection. This module
//! provides the small typed surface the core needs for setup verification and
//! instance-management tooling without leaking database credentials into CLI
//! output.

use postgres::{Client, NoTls};
use serde::Serialize;

use crate::errors::failure;
use crate::models::CommandResult;

/// Default database name used by the vendor server package.
pub const DEFAULT_DUNE_DATABASE: &str = "dune";

/// Default database user used by the vendor server package.
pub const DEFAULT_DUNE_DATABASE_USER: &str = "dune";

/// Default database port exposed by the test VM/database service.
pub const DEFAULT_DUNE_DATABASE_PORT: u16 = 15432;

/// Connection settings for the Dune PostgreSQL database.
#[derive(Debug, Clone)]
pub struct DuneDatabaseConfig {
    /// Database host or IP address.
    pub host: String,
    /// TCP port.
    pub port: u16,
    /// Database name.
    pub database: String,
    /// Database user.
    pub user: String,
    /// Database password.
    pub password: String,
}

impl DuneDatabaseConfig {
    /// Creates a config using the vendor-default database name, user, and port.
    pub fn local_vendor_defaults(host: impl Into<String>, password: impl Into<String>) -> Self {
        Self {
            host: host.into(),
            port: DEFAULT_DUNE_DATABASE_PORT,
            database: DEFAULT_DUNE_DATABASE.to_string(),
            user: DEFAULT_DUNE_DATABASE_USER.to_string(),
            password: password.into(),
        }
    }

    /// Returns a password-free summary suitable for logs and JSON output.
    pub fn redacted_summary(&self) -> DuneDatabaseSummary {
        DuneDatabaseSummary {
            host: self.host.clone(),
            port: self.port,
            database: self.database.clone(),
            user: self.user.clone(),
        }
    }

    fn connect(&self) -> CommandResult<Client> {
        let mut params = postgres::Config::new();
        params
            .host(&self.host)
            .port(self.port)
            .dbname(&self.database)
            .user(&self.user)
            .password(&self.password);
        params.connect(NoTls).map_err(|err| {
            failure(format!(
                "Failed to connect to PostgreSQL at {}:{} database {} as {}: {err}",
                self.host, self.port, self.database, self.user
            ))
        })
    }
}

/// Password-free database connection summary.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DuneDatabaseSummary {
    /// Database host or IP address.
    pub host: String,
    /// TCP port.
    pub port: u16,
    /// Database name.
    pub database: String,
    /// Database user.
    pub user: String,
}

/// Lightweight database health result.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DuneDatabaseHealth {
    /// Whether a connection and simple query succeeded.
    pub ok: bool,
    /// Password-free connection summary.
    pub connection: DuneDatabaseSummary,
    /// Database server version string from `version()`.
    pub server_version: String,
}

/// Row from the game `world_partition` table.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorldPartition {
    /// Partition identifier used by game servers and PvP/PvE settings.
    pub partition_id: i64,
    /// Current server identifier assigned by running game servers, if any.
    pub server_id: Option<String>,
    /// Game map name, for example `DeepDesert_1`.
    pub map: String,
    /// JSON partition definition stored by the game.
    pub partition_definition: String,
    /// Dimension index exposed to the client instance selector.
    pub dimension_index: i32,
    /// Whether the partition is blocked.
    pub blocked: bool,
    /// Human-facing partition label, when present.
    pub label: Option<String>,
}

/// Synchronous Dune database client.
#[derive(Debug, Clone)]
pub struct DuneDatabase {
    config: DuneDatabaseConfig,
}

impl DuneDatabase {
    /// Creates a database client from connection settings.
    pub fn new(config: DuneDatabaseConfig) -> Self {
        Self { config }
    }

    /// Runs a connection check and returns the database server version.
    pub fn health(&self) -> CommandResult<DuneDatabaseHealth> {
        let mut client = self.config.connect()?;
        let row = client
            .query_one("select version()", &[])
            .map_err(|err| failure(format!("Failed to query PostgreSQL version: {err}")))?;
        Ok(DuneDatabaseHealth {
            ok: true,
            connection: self.config.redacted_summary(),
            server_version: row.get::<_, String>(0),
        })
    }

    /// Lists world partitions, optionally filtered to a single map.
    pub fn world_partitions(&self, map: Option<&str>) -> CommandResult<Vec<WorldPartition>> {
        let mut client = self.config.connect()?;
        let query = "select partition_id, server_id, map, partition_definition::text as partition_definition, dimension_index, blocked, label from world_partition";
        let rows = if let Some(map) = map {
            client
                .query(
                    &format!("{query} where map = $1 order by partition_id"),
                    &[&map],
                )
                .map_err(|err| failure(format!("Failed to query world partitions: {err}")))?
        } else {
            client
                .query(&format!("{query} order by map, partition_id"), &[])
                .map_err(|err| failure(format!("Failed to query world partitions: {err}")))?
        };

        rows.into_iter()
            .map(|row| {
                Ok(WorldPartition {
                    partition_id: row.try_get("partition_id").map_err(row_error)?,
                    server_id: row.try_get("server_id").map_err(row_error)?,
                    map: row.try_get("map").map_err(row_error)?,
                    partition_definition: row.try_get("partition_definition").map_err(row_error)?,
                    dimension_index: row.try_get("dimension_index").map_err(row_error)?,
                    blocked: row.try_get("blocked").map_err(row_error)?,
                    label: row.try_get("label").map_err(row_error)?,
                })
            })
            .collect()
    }
}

fn row_error(err: postgres::Error) -> crate::models::CommandFailure {
    failure(format!("Failed to read world_partition row: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacted_summary_does_not_include_password() {
        let config = DuneDatabaseConfig::local_vendor_defaults("192.0.2.10", "secret-password");
        let summary = serde_json::to_string(&config.redacted_summary()).unwrap();

        assert!(summary.contains("192.0.2.10"));
        assert!(!summary.contains("secret-password"));
    }
}
