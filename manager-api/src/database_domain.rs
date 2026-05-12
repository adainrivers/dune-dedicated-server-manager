use anyhow::{anyhow, Context, Result};
use k8s_openapi::api::core::v1::Pod;
use kube::{
    api::{AttachParams, ListParams},
    Api,
};
use tokio::io::AsyncReadExt;

use crate::{
    errors::ApiError,
    models::{DatabasePlayerSummary, DatabaseWorldPartition, DatabaseWorldPartitionUpdateRequest},
    state::AppState,
};

const WORLD_PARTITIONS_QUERY: &str = "select coalesce(json_agg(t), '[]'::json) from (select partition_id, server_id, map, partition_definition::text as partition_definition, dimension_index, blocked, label from world_partition order by map, partition_id) t";
const WORLD_PARTITION_SELECT: &str =
    "partition_id, server_id, map, partition_definition::text as partition_definition, dimension_index, blocked, label";
const PLAYER_DIRECTORY_QUERY: &str = "select coalesce(json_agg(t), '[]'::json) from (select ps.account_id, ps.character_name, ps.online_status::text as online_status, ps.life_state::text as life_state, ps.server_id, ps.player_controller_id, ps.player_state_id, ps.previous_server_partition_id, ps.home_dimension_index, ps.last_login_time::text as last_login_time, ps.last_avatar_activity::text as last_avatar_activity, g.guild_id, g.guild_name, coalesce(tags.tags, '[]'::json) as tags from dune.player_state ps left join dune.guild_members gm on gm.player_id = ps.player_state_id left join dune.guilds g on g.guild_id = gm.guild_id left join lateral (select json_agg(pt.tag order by pt.tag) as tags from dune.player_tags pt where pt.account_id = ps.account_id) tags on true order by ps.last_login_time desc nulls last, ps.character_name asc nulls last limit 500) t";

pub async fn list_world_partitions(state: &AppState) -> Result<Vec<DatabaseWorldPartition>> {
    let stdout = exec_database_psql_json(state, WORLD_PARTITIONS_QUERY).await?;

    serde_json::from_str(stdout.trim())
        .with_context(|| "failed to parse world_partition query output".to_string())
}

pub async fn list_database_players(state: &AppState) -> Result<Vec<DatabasePlayerSummary>> {
    let stdout = exec_database_psql_json(state, PLAYER_DIRECTORY_QUERY).await?;

    serde_json::from_str(stdout.trim())
        .with_context(|| "failed to parse database player directory output".to_string())
}

pub async fn update_world_partition(
    state: &AppState,
    partition_id: i64,
    request: DatabaseWorldPartitionUpdateRequest,
) -> Result<Option<DatabaseWorldPartition>, ApiError> {
    if partition_id <= 0 {
        return Err(ApiError::bad_request("partition id must be positive"));
    }
    let label = validate_partition_label(request.label)?;
    let label_sql = match label {
        Some(value) => format!(
            "convert_from(decode('{}', 'hex'), 'UTF8')",
            hex_encode(&value)
        ),
        None => "null".to_string(),
    };
    let blocked_sql = if request.blocked { "true" } else { "false" };
    let query = format!(
        "with updated as (update world_partition set blocked = {blocked_sql}, label = {label_sql} where partition_id = {partition_id} returning {WORLD_PARTITION_SELECT}) select coalesce(json_agg(t), '[]'::json) from (select {WORLD_PARTITION_SELECT} from updated) t",
    );
    let stdout = exec_database_psql_json(state, &query).await?;
    let mut rows: Vec<DatabaseWorldPartition> = serde_json::from_str(stdout.trim())
        .with_context(|| "failed to parse updated world_partition row".to_string())?;

    Ok(rows.pop())
}

async fn exec_database_psql_json(state: &AppState, query: &str) -> Result<String> {
    let pod_name = find_database_pod(state).await?;
    let mut attached = Api::<Pod>::namespaced(state.client.clone(), &state.namespace)
        .exec(
            &pod_name,
            vec![
                "psql",
                "-h",
                "127.0.0.1",
                "-p",
                "15432",
                "-U",
                "dune",
                "-d",
                "dune",
                "-t",
                "-A",
                "-c",
                query,
            ],
            &AttachParams::default().stderr(true),
        )
        .await
        .with_context(|| format!("failed to exec psql in database pod {pod_name}"))?;

    let mut stdout = String::new();
    let mut stderr = String::new();
    if let Some(mut reader) = attached.stdout() {
        reader
            .read_to_string(&mut stdout)
            .await
            .context("failed to read psql stdout")?;
    }
    if let Some(mut reader) = attached.stderr() {
        reader
            .read_to_string(&mut stderr)
            .await
            .context("failed to read psql stderr")?;
    }
    attached
        .join()
        .await
        .with_context(|| format!("psql exited with an error: {}", stderr.trim()))?;

    Ok(stdout)
}

async fn find_database_pod(state: &AppState) -> Result<String> {
    let pods: Api<Pod> = Api::namespaced(state.client.clone(), &state.namespace);
    let list = pods
        .list(&ListParams::default())
        .await
        .context("failed to list pods while locating database")?;
    list.items
        .into_iter()
        .filter_map(|pod| pod.metadata.name)
        .find(|name| name.contains("-db-dbdepl-sts-0"))
        .ok_or_else(|| anyhow!("database pod was not found"))
}

fn validate_partition_label(value: Option<String>) -> Result<Option<String>, ApiError> {
    let Some(value) = value.map(|label| label.trim().to_string()) else {
        return Ok(None);
    };
    if value.is_empty() {
        return Ok(None);
    }
    if value.chars().count() > 80 {
        return Err(ApiError::bad_request(
            "partition label must be 80 characters or less",
        ));
    }
    if value.chars().any(|character| character.is_control()) {
        return Err(ApiError::bad_request(
            "partition label cannot contain control characters",
        ));
    }
    Ok(Some(value))
}

fn hex_encode(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let bytes = value.as_bytes();
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_world_partition_rows() {
        let json = r#"[{"partition_id":8,"server_id":"server-a","map":"DeepDesert_0","partition_definition":"{}","dimension_index":0,"blocked":false,"label":"DeepDesert_0"}]"#;

        let rows: Vec<DatabaseWorldPartition> = serde_json::from_str(json).unwrap();

        assert_eq!(rows[0].partition_id, 8);
        assert_eq!(rows[0].server_id.as_deref(), Some("server-a"));
        assert_eq!(rows[0].map, "DeepDesert_0");
    }

    #[test]
    fn parses_database_player_rows() {
        let json = r#"[{"account_id":42,"character_name":"Siona","online_status":"Online","life_state":"Alive","server_id":"server-a","player_controller_id":10,"player_state_id":11,"previous_server_partition_id":8,"home_dimension_index":0,"last_login_time":"2026-05-12 10:00:00+00","last_avatar_activity":"2026-05-12 10:05:00+00","guild_id":7,"guild_name":"Atreides","tags":["builder","admin"]}]"#;

        let rows: Vec<DatabasePlayerSummary> = serde_json::from_str(json).unwrap();

        assert_eq!(rows[0].account_id, 42);
        assert_eq!(rows[0].character_name.as_deref(), Some("Siona"));
        assert_eq!(rows[0].guild_name.as_deref(), Some("Atreides"));
        assert_eq!(rows[0].tags, vec!["builder", "admin"]);
    }

    #[test]
    fn validates_partition_labels() {
        assert_eq!(
            validate_partition_label(Some("  Deep Desert PvP  ".to_string()))
                .unwrap()
                .as_deref(),
            Some("Deep Desert PvP")
        );
        assert!(validate_partition_label(Some("".to_string()))
            .unwrap()
            .is_none());
        assert!(validate_partition_label(Some("x".repeat(81))).is_err());
        assert!(validate_partition_label(Some("bad\nlabel".to_string())).is_err());
    }

    #[test]
    fn encodes_labels_for_sql_literals() {
        assert_eq!(hex_encode("PvE 1"), "5076452031");
    }
}
