use serde_json::Value;

use crate::{
    clock::now_unix_ms,
    errors::ApiError,
    models::{
        DirectorMapSummary, DirectorPathCapability, DirectorPlayerLists, DirectorPlayerSummary,
        DirectorServerSummary,
    },
};

pub fn director_capabilities_list() -> Vec<DirectorPathCapability> {
    const PATHS: &[(&str, &str)] = &[
        ("GET", "/v0/igwoBattlegroup"),
        ("GET", "/v0/getPodset"),
        ("GET", "/v0/battlegroup"),
        ("GET", "/v0/players"),
        ("GET", "/v0/players/online"),
        ("GET", "/v0/players/intransit"),
        ("GET", "/v0/players/graceperiod"),
        ("GET", "/v0/players/completion"),
        ("GET", "/v0/players/queued"),
        ("POST", "/v0/BattlegroupUpdateServerGroupConfig"),
        ("POST", "/v0/BattlegroupClearMapConfigOverrides"),
        ("GET", "/v0/BattlegroupFetchFlsReportSettings"),
        ("GET", "/v0/BattlegroupFetchCharacterTransferRules"),
        ("POST", "/v0/BattlegroupUpdateFlsReportSettings"),
        ("POST", "/v0/BattlegroupUpdateCharacterTransferSettings"),
        ("POST", "/v0/BattlegroupClearFlsReportOverrides"),
        ("POST", "/v0/BattlegroupClearCharacterTransferOverrides"),
    ];
    PATHS
        .iter()
        .map(|(method, path)| DirectorPathCapability { method, path })
        .collect()
}

pub fn is_allowed_director_api(method: &str, path: &str) -> bool {
    director_capabilities_list()
        .iter()
        .any(|item| item.method.eq_ignore_ascii_case(method) && item.path == path)
}

pub fn query_token(query: Option<&str>) -> Option<&str> {
    query.and_then(|query| {
        query.split('&').find_map(|part| {
            let (key, value) = part.split_once('=')?;
            (key == "token").then_some(value)
        })
    })
}

pub fn director_query(query: Option<&str>) -> Option<String> {
    query.map(|query| {
        query
            .split('&')
            .filter(|part| !part.starts_with("token="))
            .collect::<Vec<_>>()
            .join("&")
    })
}

pub fn validate_director_map_name(value: &str) -> Result<(), ApiError> {
    if value.is_empty()
        || !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
    {
        return Err(ApiError::bad_request("invalid Director map name"));
    }
    Ok(())
}

pub fn is_safe_static_path(value: &str) -> bool {
    !value.is_empty()
        && !value.contains("..")
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | '/'))
}

pub fn director_player_summary(value: &Value) -> DirectorPlayerSummary {
    let mut summary = DirectorPlayerSummary {
        login_requests_total: value_i64(value, "numLoginRequestsTotal"),
        travel_requests_total: value_i64(value, "numTravelRequestsTotal"),
        ..Default::default()
    };
    for map in director_all_map_values(value) {
        for server in director_server_values(map) {
            summary.active += value_i64(server, "numPlayersInGame");
            summary.online += value_i64(server, "numPlayersOnline");
            summary.in_transit += value_i64(server, "numPlayersInTransit");
            summary.grace_period += value_i64(server, "numPlayersInGracePeriod");
            summary.completion += value_i64(server, "numPlayersWithCompletion");
            summary.queued += value_i64(server, "numPlayersInQueue");
        }
        if director_server_values(map).is_empty() {
            summary.active += value_i64(map, "numPlayersInGame");
            summary.online += value_i64(map, "numPlayersOnline");
            summary.in_transit += value_i64(map, "numPlayersInTransit");
            summary.grace_period += value_i64(map, "numPlayersInGracePeriod");
            summary.completion += value_i64(map, "numPlayersWithCompletion");
            summary.queued += value_i64(map, "numPlayersInQueue");
        }
    }
    summary
}

pub fn director_player_ids(value: &Value) -> Vec<String> {
    let mut ids = Vec::new();
    match value {
        Value::Array(items) => {
            for item in items {
                if let Some(id) = player_id_from_value(item) {
                    ids.push(id);
                }
            }
        }
        Value::Object(map) => {
            for item in map.values() {
                if let Some(id) = player_id_from_value(item) {
                    ids.push(id);
                }
            }
        }
        _ => {
            if let Some(id) = player_id_from_value(value) {
                ids.push(id);
            }
        }
    }
    ids.sort();
    ids.dedup();
    ids
}

pub fn director_player_lists(
    all: &Value,
    online: &Value,
    in_transit: &Value,
    grace_period: &Value,
    completion: &Value,
    queued: &Value,
) -> DirectorPlayerLists {
    DirectorPlayerLists {
        all: director_player_ids(all),
        online: director_player_ids(online),
        in_transit: director_player_ids(in_transit),
        grace_period: director_player_ids(grace_period),
        completion: director_player_ids(completion),
        queued: director_player_ids(queued),
    }
}

pub fn director_map_summaries(value: &Value) -> Vec<DirectorMapSummary> {
    let mut maps = Vec::new();
    collect_director_maps(value, "singleServerMaps", "Single", &mut maps);
    collect_director_maps(value, "dimensionMaps", "Dimension", &mut maps);
    collect_director_maps(value, "instancedMaps", "Instanced", &mut maps);
    maps.sort_by(|left, right| left.name.cmp(&right.name));
    maps
}

fn player_id_from_value(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Object(map) => ["playerId", "id", "characterId", "accountId"]
            .iter()
            .find_map(|key| map.get(*key).and_then(player_id_from_value)),
        _ => None,
    }
    .filter(|value| !value.trim().is_empty())
}

fn collect_director_maps(
    value: &Value,
    key: &str,
    kind: &str,
    output: &mut Vec<DirectorMapSummary>,
) {
    let Some(items) = value.get(key).and_then(Value::as_object) else {
        return;
    };
    for (name, map) in items {
        let servers = director_server_values(map)
            .into_iter()
            .map(director_server_summary)
            .collect::<Vec<_>>();
        let players = if servers.is_empty() {
            value_i64(map, "numPlayersInGame")
        } else {
            servers.iter().map(|server| server.players).sum()
        };
        let online = if servers.is_empty() {
            value_i64(map, "numPlayersOnline")
        } else {
            servers.iter().map(|server| server.online).sum()
        };
        let queued = if servers.is_empty() {
            value_i64(map, "numPlayersInQueue")
        } else {
            servers.iter().filter_map(|server| server.queued).sum()
        };
        output.push(DirectorMapSummary {
            name: name.clone(),
            kind: kind.to_string(),
            players,
            online,
            queued,
            servers,
            has_override: !map.get("webOverrideCfg").unwrap_or(&Value::Null).is_null(),
        });
    }
}

fn director_all_map_values(value: &Value) -> Vec<&Value> {
    ["singleServerMaps", "dimensionMaps", "instancedMaps"]
        .iter()
        .flat_map(|key| {
            value
                .get(*key)
                .and_then(Value::as_object)
                .map(|items| items.values().collect::<Vec<_>>())
                .unwrap_or_default()
        })
        .collect()
}

fn director_server_values(map: &Value) -> Vec<&Value> {
    if let Some(items) = map.get("serversByDimension").and_then(Value::as_object) {
        return items.values().collect();
    }
    if let Some(items) = map.get("instances").and_then(Value::as_array) {
        return items.iter().collect();
    }
    if let Some(items) = map.get("instances").and_then(Value::as_object) {
        return items.values().collect();
    }
    if map.get("partition").is_some() {
        return vec![map];
    }
    Vec::new()
}

fn director_server_summary(value: &Value) -> DirectorServerSummary {
    let partition = &value["partition"];
    let status_code = value_i64(value, "status");
    let heartbeat_seconds_ago = value["lastServerState"]["reportTimestamp"]
        .as_i64()
        .map(|timestamp| (now_unix_ms() as i64 / 1000).saturating_sub(timestamp));
    DirectorServerSummary {
        label: partition["label"]
            .as_str()
            .or_else(|| value["lastServerState"]["displayName"].as_str())
            .unwrap_or_default()
            .to_string(),
        server_id: partition["serverId"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        partition_id: partition["partitionId"].as_i64(),
        dimension_index: partition["dimensionIndex"].as_i64(),
        players: value_i64(value, "numPlayersInGame"),
        online: value_i64(value, "numPlayersOnline"),
        queued: value.get("numPlayersInQueue").and_then(Value::as_i64),
        status: match status_code {
            1 => "Allocating",
            2 => "Running But Not Ready",
            3 => "Running",
            _ => "Not Available",
        }
        .to_string(),
        heartbeat_seconds_ago,
        has_override: !value
            .get("webOverrideCfg")
            .unwrap_or(&Value::Null)
            .is_null(),
    }
}

fn value_i64(value: &Value, key: &str) -> i64 {
    value.get(key).and_then(Value::as_i64).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn summarizes_players_across_all_map_groups() {
        let value = json!({
            "singleServerMaps": {
                "Arrakeen": {
                    "partition": { "serverId": "single" },
                    "numPlayersInGame": 2,
                    "numPlayersOnline": 1,
                    "numPlayersInQueue": 3
                }
            },
            "dimensionMaps": {
                "DeepDesert": {
                    "serversByDimension": {
                        "0": {
                            "partition": { "serverId": "dimension" },
                            "numPlayersInGame": 6,
                            "numPlayersOnline": 5,
                            "numPlayersInQueue": 4
                        }
                    }
                }
            },
            "numLoginRequestsTotal": 10,
            "numTravelRequestsTotal": 11
        });

        let summary = director_player_summary(&value);

        assert_eq!(summary.active, 8);
        assert_eq!(summary.online, 6);
        assert_eq!(summary.queued, 7);
        assert_eq!(summary.in_transit, 0);
        assert_eq!(summary.grace_period, 0);
        assert_eq!(summary.completion, 0);
        assert_eq!(summary.login_requests_total, 10);
        assert_eq!(summary.travel_requests_total, 11);
    }

    #[test]
    fn allowlist_rejects_unknown_director_routes() {
        assert!(is_allowed_director_api("GET", "/v0/players"));
        assert!(is_allowed_director_api(
            "post",
            "/v0/BattlegroupUpdateFlsReportSettings"
        ));
        assert!(!is_allowed_director_api("DELETE", "/v0/players"));
        assert!(!is_allowed_director_api("GET", "/v0/not-real"));
    }

    #[test]
    fn normalizes_player_id_payloads() {
        let value = json!([
            "plain",
            42,
            { "playerId": "nested" },
            { "characterId": "fallback" },
            { "ignored": true },
            "plain"
        ]);

        assert_eq!(
            director_player_ids(&value),
            vec![
                "42".to_string(),
                "fallback".to_string(),
                "nested".to_string(),
                "plain".to_string()
            ]
        );
    }
}
