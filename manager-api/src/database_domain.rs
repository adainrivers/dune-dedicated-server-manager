use anyhow::{anyhow, Context, Result};
use k8s_openapi::api::core::v1::Pod;
use kube::{
    api::{AttachParams, ListParams},
    Api,
};
use tokio::io::AsyncReadExt;

use crate::{
    errors::ApiError,
    models::{
        DatabaseGuildProfile, DatabaseGuildSummary, DatabasePlayerProfile,
        DatabasePlayerStatistics, DatabasePlayerSummary, DatabasePlayerTagRequest,
        DatabasePlayerTagsUpdate, DatabaseWorldPartition, DatabaseWorldPartitionUpdateRequest,
    },
    state::AppState,
};

const WORLD_PARTITIONS_QUERY: &str = "select coalesce(json_agg(t), '[]'::json) from (select partition_id, server_id, map, partition_definition::text as partition_definition, dimension_index, blocked, label from world_partition order by map, partition_id) t";
const WORLD_PARTITION_SELECT: &str =
    "partition_id, server_id, map, partition_definition::text as partition_definition, dimension_index, blocked, label";
const PLAYER_DIRECTORY_QUERY: &str = "select coalesce(json_agg(t), '[]'::json) from (select ps.account_id, ps.character_name, ps.online_status::text as online_status, ps.life_state::text as life_state, ps.server_id, ps.player_controller_id, ps.player_state_id, ps.previous_server_partition_id, ps.home_dimension_index, ps.last_login_time::text as last_login_time, ps.last_avatar_activity::text as last_avatar_activity, g.guild_id, g.guild_name, coalesce(tags.tags, '[]'::json) as tags from dune.player_state ps left join dune.guild_members gm on gm.player_id = ps.player_state_id left join dune.guilds g on g.guild_id = gm.guild_id left join lateral (select json_agg(pt.tag order by pt.tag) as tags from dune.player_tags pt where pt.account_id = ps.account_id) tags on true order by ps.last_login_time desc nulls last, ps.character_name asc nulls last limit 500) t";
const GUILD_DIRECTORY_QUERY: &str = "select coalesce(json_agg(t), '[]'::json) from (select g.guild_id, g.guild_name, g.guild_description, g.guild_faction, count(gm.player_id) as member_count from dune.guilds g left join dune.guild_members gm on gm.guild_id = g.guild_id group by g.guild_id, g.guild_name, g.guild_description, g.guild_faction order by member_count desc, g.guild_name asc limit 250) t";
const PLAYER_STATISTICS_QUERY: &str = "select json_build_object('total_accounts', (select count(*) from dune.accounts), 'total_players', (select count(*) from dune.player_state), 'guilds', (select count(*) from dune.guilds), 'guild_members', (select count(*) from dune.guild_members), 'tagged_players', (select count(distinct account_id) from dune.player_tags), 'online_statuses', (select coalesce(json_agg(s), '[]'::json) from (select coalesce(online_status::text, 'Unknown') as name, count(*) as count from dune.player_state group by 1 order by count desc, name asc) s), 'life_states', (select coalesce(json_agg(s), '[]'::json) from (select coalesce(life_state::text, 'Unknown') as name, count(*) as count from dune.player_state group by 1 order by count desc, name asc) s), 'recent_players', (select coalesce(json_agg(r), '[]'::json) from (select account_id, character_name, online_status::text as online_status, last_login_time::text as last_login_time from dune.player_state order by last_login_time desc nulls last, character_name asc nulls last limit 8) r))";

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

pub async fn list_database_guilds(state: &AppState) -> Result<Vec<DatabaseGuildSummary>> {
    let stdout = exec_database_psql_json(state, GUILD_DIRECTORY_QUERY).await?;

    serde_json::from_str(stdout.trim())
        .with_context(|| "failed to parse database guild directory output".to_string())
}

pub async fn database_guild_profile(
    state: &AppState,
    guild_id: i64,
) -> Result<Option<DatabaseGuildProfile>, ApiError> {
    if guild_id <= 0 {
        return Err(ApiError::bad_request("guild id must be positive"));
    }
    let query = guild_profile_query(guild_id);
    let stdout = exec_database_psql_json(state, &query).await?;
    let value: serde_json::Value = serde_json::from_str(stdout.trim())
        .map_err(|_| ApiError::bad_gateway("failed to parse guild profile"))?;
    if value.is_null() {
        return Ok(None);
    }
    let profile = serde_json::from_value(value)
        .map_err(|_| ApiError::bad_gateway("failed to parse guild profile fields"))?;
    Ok(Some(profile))
}

pub async fn database_player_statistics(state: &AppState) -> Result<DatabasePlayerStatistics> {
    let stdout = exec_database_psql_json(state, PLAYER_STATISTICS_QUERY).await?;

    serde_json::from_str(stdout.trim())
        .with_context(|| "failed to parse database player statistics output".to_string())
}

pub async fn database_player_profile(
    state: &AppState,
    account_id: i64,
) -> Result<Option<DatabasePlayerProfile>, ApiError> {
    if account_id <= 0 {
        return Err(ApiError::bad_request("account id must be positive"));
    }
    let query = player_profile_query(account_id);
    let stdout = exec_database_psql_json(state, &query).await?;
    let value: serde_json::Value = serde_json::from_str(stdout.trim())
        .map_err(|_| ApiError::bad_gateway("failed to parse player profile"))?;
    if value.is_null() {
        return Ok(None);
    }
    let profile = serde_json::from_value(value)
        .map_err(|_| ApiError::bad_gateway("failed to parse player profile fields"))?;
    Ok(Some(profile))
}

pub async fn add_database_player_tag(
    state: &AppState,
    account_id: i64,
    request: DatabasePlayerTagRequest,
) -> Result<DatabasePlayerTagsUpdate, ApiError> {
    let tag = validate_player_tag(request.tag)?;
    let query = player_tag_update_query("insert", account_id, &tag)?;
    let stdout = exec_database_psql_json(state, &query).await?;
    parse_player_tags_update(&stdout)
}

pub async fn remove_database_player_tag(
    state: &AppState,
    account_id: i64,
    request: DatabasePlayerTagRequest,
) -> Result<DatabasePlayerTagsUpdate, ApiError> {
    let tag = validate_player_tag(request.tag)?;
    let query = player_tag_update_query("delete", account_id, &tag)?;
    let stdout = exec_database_psql_json(state, &query).await?;
    parse_player_tags_update(&stdout)
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

fn validate_player_tag(value: String) -> Result<String, ApiError> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(ApiError::bad_request("player tag is required"));
    }
    if value.chars().count() > 64 {
        return Err(ApiError::bad_request(
            "player tag must be 64 characters or less",
        ));
    }
    if value.chars().any(|character| character.is_control()) {
        return Err(ApiError::bad_request(
            "player tag cannot contain control characters",
        ));
    }
    Ok(value)
}

fn player_tag_update_query(action: &str, account_id: i64, tag: &str) -> Result<String, ApiError> {
    if account_id <= 0 {
        return Err(ApiError::bad_request("account id must be positive"));
    }
    let tag_sql = format!("convert_from(decode('{}', 'hex'), 'UTF8')", hex_encode(tag));
    let mutation = match action {
        "insert" => format!(
            "insert into dune.player_tags(account_id, tag) select {account_id}, {tag_sql} where exists (select 1 from dune.encrypted_accounts where id = {account_id}) on conflict do nothing returning account_id"
        ),
        "delete" => {
            format!("delete from dune.player_tags where account_id = {account_id} and tag = {tag_sql} returning account_id")
        }
        _ => return Err(ApiError::bad_request("invalid player tag action")),
    };
    Ok(format!(
        "with account_check as (select exists(select 1 from dune.encrypted_accounts where id = {account_id}) as account_exists), mutation as ({mutation}), tags as (select coalesce(json_agg(tag order by tag), '[]'::json) as tags from dune.player_tags where account_id = {account_id}) select json_build_object('account_id', {account_id}, 'account_exists', (select account_exists from account_check), 'tags', (select tags from tags))"
    ))
}

fn player_profile_query(account_id: i64) -> String {
    format!(
        "select coalesce((select json_build_object('account_id', ps.account_id, 'character_name', ps.character_name, 'platform_name', ea.platform_name, 'takeoverable', ea.takeoverable, 'online_status', ps.online_status::text, 'life_state', ps.life_state::text, 'server_id', ps.server_id, 'previous_server_partition_id', ps.previous_server_partition_id, 'home_dimension_index', ps.home_dimension_index, 'last_login_time', ps.last_login_time::text, 'last_avatar_activity', ps.last_avatar_activity::text, 'guild_id', g.guild_id, 'guild_name', g.guild_name, 'guild_role_id', gm.role_id, 'tags', coalesce(tags.tags, '[]'::json), 'factions', coalesce(factions.rows, '[]'::json), 'currency_balances', coalesce(currency.rows, '[]'::json), 'access_codes', coalesce(access_codes.rows, '[]'::json), 'cheat_flags', coalesce(cheat_flags.rows, '[]'::json), 'removal_logs', coalesce(removal_logs.rows, '[]'::json)) from dune.player_state ps left join dune.encrypted_accounts ea on ea.id = ps.account_id left join dune.guild_members gm on gm.player_id = ps.player_state_id left join dune.guilds g on g.guild_id = gm.guild_id left join lateral (select json_agg(pt.tag order by pt.tag) as tags from dune.player_tags pt where pt.account_id = ps.account_id) tags on true left join lateral (select json_agg(f order by f.faction_id) as rows from (select faction_id, utc_time_faction_change::text as changed_at from dune.player_faction where actor_id in (ps.player_state_id, ps.player_controller_id, ps.player_pawn_id)) f) factions on true left join lateral (select json_agg(c order by c.currency_id) as rows from (select currency_id, balance from dune.player_virtual_currency_balances where player_controller_id = ps.player_controller_id) c) currency on true left join lateral (select json_agg(a order by a.access_code_type, a.access_code) as rows from (select access_code_type, access_code, is_resettable as resettable from dune.player_access_codes where account_id = ps.account_id limit 20) a) access_codes on true left join lateral (select json_agg(cf order by cf.event_time desc) as rows from (select event_time::text as event_time, cheat_type::text as cheat_type from dune.cheater_tracking where fls_id = ea.platform_id order by event_time desc limit 20) cf) cheat_flags on true left join lateral (select json_agg(rl order by rl.event_time desc) as rows from (select event_time::text as event_time, reason from dune.account_removal_log where account_id = ps.account_id order by event_time desc limit 20) rl) removal_logs on true where ps.account_id = {account_id} limit 1), 'null'::json)"
    )
}

fn guild_profile_query(guild_id: i64) -> String {
    format!(
        "select coalesce((select json_build_object('guild_id', g.guild_id, 'guild_name', g.guild_name, 'guild_description', g.guild_description, 'guild_faction', g.guild_faction, 'member_count', (select count(*) from dune.guild_members where guild_id = g.guild_id), 'online_members', (select count(*) from dune.guild_members gm join dune.player_state ps on ps.player_state_id = gm.player_id where gm.guild_id = g.guild_id and ps.online_status::text = 'Online'), 'members', coalesce(members.rows, '[]'::json)) from dune.guilds g left join lateral (select json_agg(m order by m.role_id, m.character_name nulls last, m.player_state_id) as rows from (select ps.account_id, gm.player_id as player_state_id, ps.character_name, gm.role_id, ps.online_status::text as online_status, ps.life_state::text as life_state, ps.last_login_time::text as last_login_time from dune.guild_members gm left join dune.player_state ps on ps.player_state_id = gm.player_id where gm.guild_id = g.guild_id order by gm.role_id, ps.character_name nulls last limit 500) m) members on true where g.guild_id = {guild_id} limit 1), 'null'::json)"
    )
}

fn parse_player_tags_update(stdout: &str) -> Result<DatabasePlayerTagsUpdate, ApiError> {
    let value: serde_json::Value = serde_json::from_str(stdout.trim())
        .map_err(|_| ApiError::bad_gateway("failed to parse player tags update"))?;
    if !value["account_exists"].as_bool().unwrap_or(false) {
        return Err(ApiError::not_found("player account was not found"));
    }
    serde_json::from_value(value)
        .map_err(|_| ApiError::bad_gateway("failed to parse updated player tags"))
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
    fn parses_database_guild_rows() {
        let json = r#"[{"guild_id":7,"guild_name":"Atreides","guild_description":"Desert power","guild_faction":1,"member_count":12}]"#;

        let rows: Vec<DatabaseGuildSummary> = serde_json::from_str(json).unwrap();

        assert_eq!(rows[0].guild_id, 7);
        assert_eq!(rows[0].guild_name, "Atreides");
        assert_eq!(rows[0].member_count, 12);
    }

    #[test]
    fn parses_database_guild_profile() {
        let json = r#"{"guild_id":7,"guild_name":"Atreides","guild_description":"Desert power","guild_faction":1,"member_count":2,"online_members":1,"members":[{"account_id":42,"player_state_id":11,"character_name":"Siona","role_id":1,"online_status":"Online","life_state":"Alive","last_login_time":"2026-05-12 10:00:00+00"}]}"#;

        let profile: DatabaseGuildProfile = serde_json::from_str(json).unwrap();

        assert_eq!(profile.guild_id, 7);
        assert_eq!(profile.online_members, 1);
        assert_eq!(profile.members[0].character_name.as_deref(), Some("Siona"));
    }

    #[test]
    fn parses_database_player_statistics() {
        let json = r#"{"total_accounts":3,"total_players":2,"guilds":1,"guild_members":1,"tagged_players":1,"online_statuses":[{"name":"Online","count":1}],"life_states":[{"name":"Alive","count":2}],"recent_players":[{"account_id":42,"character_name":"Siona","online_status":"Online","last_login_time":"2026-05-12 10:00:00+00"}]}"#;

        let statistics: DatabasePlayerStatistics = serde_json::from_str(json).unwrap();

        assert_eq!(statistics.total_accounts, 3);
        assert_eq!(statistics.total_players, 2);
        assert_eq!(statistics.online_statuses[0].name, "Online");
        assert_eq!(statistics.recent_players[0].account_id, 42);
    }

    #[test]
    fn parses_database_player_profile() {
        let json = r#"{"account_id":42,"character_name":"Siona","platform_name":"Steam","takeoverable":false,"online_status":"Online","life_state":"Alive","server_id":"server-a","previous_server_partition_id":8,"home_dimension_index":0,"last_login_time":"2026-05-12 10:00:00+00","last_avatar_activity":"2026-05-12 10:05:00+00","guild_id":7,"guild_name":"Atreides","guild_role_id":2,"tags":["helper"],"factions":[{"faction_id":1,"changed_at":"2026-05-12 09:00:00+00"}],"currency_balances":[{"currency_id":3,"balance":500}],"access_codes":[{"access_code_type":1,"access_code":1234,"resettable":true}],"cheat_flags":[{"event_time":"2026-05-12 08:00:00+00","cheat_type":"Speed"}],"removal_logs":[{"event_time":"2026-05-12 07:00:00+00","reason":"test"}]}"#;

        let profile: DatabasePlayerProfile = serde_json::from_str(json).unwrap();

        assert_eq!(profile.account_id, 42);
        assert_eq!(profile.platform_name.as_deref(), Some("Steam"));
        assert_eq!(profile.factions[0].faction_id, 1);
        assert_eq!(profile.currency_balances[0].balance, 500);
        assert_eq!(profile.access_codes[0].access_code, 1234);
    }

    #[test]
    fn validates_player_tags() {
        assert_eq!(
            validate_player_tag("  community helper  ".to_string()).unwrap(),
            "community helper"
        );
        assert!(validate_player_tag("".to_string()).is_err());
        assert!(validate_player_tag("x".repeat(65)).is_err());
        assert!(validate_player_tag("bad\ntag".to_string()).is_err());
    }

    #[test]
    fn parses_player_tags_update() {
        let result = parse_player_tags_update(
            r#"{"account_id":42,"account_exists":true,"tags":["admin","builder"]}"#,
        )
        .unwrap();

        assert_eq!(result.account_id, 42);
        assert_eq!(result.tags, vec!["admin", "builder"]);
    }

    #[test]
    fn rejects_player_tags_update_for_missing_accounts() {
        let result =
            parse_player_tags_update(r#"{"account_id":42,"account_exists":false,"tags":[]}"#);

        assert!(result.is_err());
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
