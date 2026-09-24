use dune_manager_core::errors::failure;
use dune_manager_core::models::CommandResult;
use dune_manager_core::orchestration::{RemoteCommandRunner, RusshRunner};
use serde_json::Value;

use crate::commands::shared::sh_single_quoted;
use crate::commands::status_disk::read_disk_usage;
use crate::commands::status_helpers::{pod_component, server_resource_components};
use crate::commands::status_naming::friendly_map_name;
use crate::commands::status_pod_age::read_server_pod_starts;
use crate::dto::{
    RemoteBattlegroupServerStat, RemoteBattlegroupStatus, RemoteServerComponent,
    RemoteServerPackageStatus, RemoteServerStatus,
};

pub fn read_remote_server_status(
    runner: &RusshRunner,
    namespace: &str,
    battlegroup_name: &str,
) -> CommandResult<RemoteServerStatus> {
    // The vendor wrapper's `status` text output is the source of truth in
    // older operator versions, but the format keeps shifting across Funcom
    // releases (newer wrappers show the partial world name in "Status",
    // "N/M" ratios under "Director", and semantic words like "Healthy"
    // under "Uptime" — none of which match the older
    // `Running/Running/Running/Running/1h2m` shape we used to parse).
    // Read the BattleGroup CR's `status` object directly so we stay
    // pinned to the stable Kubernetes schema instead of the rotating
    // text rendering.
    let bg = runner.run_json(
        &format!(
            "sudo kubectl get battlegroup -n {} {} -o json",
            sh_single_quoted(namespace),
            sh_single_quoted(battlegroup_name),
        ),
        "remote battlegroup",
    )?;
    // Per-partition live data (player count, gamePhase, ready) lives on a
    // separate ServerStats CRD published by the Funcom operator — the same
    // source `F:\Dune\Server\gt-server-status\gt_server_status.py` consumes.
    // Failing to fetch this is non-fatal; the table just shows blank
    // players where it can't be merged.
    let stats = runner
        .run_json(
            &format!(
                "sudo kubectl get serverstats -n {} -o json",
                sh_single_quoted(namespace),
            ),
            "remote serverstats",
        )
        .unwrap_or_else(|_| Value::Null);
    let pod_starts = read_server_pod_starts(runner, namespace);
    let battlegroup = battlegroup_status_from_json_full(&bg, &stats, &pod_starts).ok_or_else(|| {
        failure(format!(
            "BattleGroup `{battlegroup_name}` returned no status object yet (likely still initialising)"
        ))
    })?;
    let package = read_guest_package_status(runner, namespace, battlegroup_name)?;
    let disks = read_disk_usage(runner);
    Ok(RemoteServerStatus {
        battlegroup,
        package,
        disks,
    })
}

/// Maps a raw `kubectl get battlegroup ... -o json` payload into the UI's
/// `RemoteBattlegroupStatus` and merges per-partition
/// live data (players, gamePhase, ready) from a `kubectl get serverstats`
/// JSON payload. Pass `Value::Null` when no stats are available.
#[cfg(test)]
pub(crate) fn battlegroup_status_from_json_with_stats(
    bg: &Value,
    serverstats: &Value,
) -> Option<RemoteBattlegroupStatus> {
    battlegroup_status_from_json_full(bg, serverstats, &std::collections::HashMap::new())
}

/// Same as `battlegroup_status_from_json_with_stats`, plus per-server pod
/// start times (`partition index -> RFC 3339`) used for each row's age.
pub(crate) fn battlegroup_status_from_json_full(
    bg: &Value,
    serverstats: &Value,
    pod_starts: &std::collections::HashMap<u64, String>,
) -> Option<RemoteBattlegroupStatus> {
    bg.get("metadata")?.get("name")?.as_str()?;
    let spec = bg.get("spec").cloned().unwrap_or(Value::Null);
    let status = bg.get("status").cloned().unwrap_or(Value::Null);

    let stop = spec
        .get("stop")
        .and_then(Value::as_bool)
        .or_else(|| status.get("stop").and_then(Value::as_bool))
        .unwrap_or(false);

    // Funcom's CR carries `status.startTimestamp` at the BG level (when the
    // BG first scheduled) but not per-server. We render it on every row as a
    // best-effort age — accurate when partitions all came up together, off
    // by however long a partition has restarted independently.
    let bg_age = status
        .get("startTimestamp")
        .and_then(Value::as_str)
        .map(format_age_since_iso)
        .unwrap_or_default();

    let stats_by_partition = index_serverstats_by_partition(serverstats);

    let server_stats = status
        .get("servers")
        .and_then(Value::as_array)
        .map(|servers| {
            servers
                .iter()
                .map(|s| server_stat_from_json(s, &bg_age, &stats_by_partition, pod_starts))
                .collect()
        })
        .unwrap_or_default();

    // Database/director phases are nested in the live CR, not top-level
    // fields. Fall back to top-level keys for older operator builds.
    let database_phase = status
        .get("database")
        .and_then(|d| d.get("phase"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| string_field(&status, "databasePhase"));
    let director_phase = status
        .get("utilities")
        .and_then(|u| u.get("director"))
        .and_then(|d| d.get("phase"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| string_field(&status, "directorPhase"));
    let gateway_phase = status
        .get("utilities")
        .and_then(|u| u.get("serverGateway"))
        .and_then(|g| g.get("phase"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_default();
    // Uptime: the CR doesn't expose a pre-formatted string anymore, so we
    // compute it from `status.startTimestamp` (the same field we use for
    // per-row age). Older operators that set a literal `uptime` string win.
    let uptime_literal = string_field(&status, "uptime");
    let uptime = if uptime_literal.is_empty() {
        bg_age.clone()
    } else {
        uptime_literal
    };

    Some(RemoteBattlegroupStatus {
        stop,
        phase: string_field(&status, "phase"),
        database_phase,
        server_group_phase: string_field(&status, "serverGroupPhase"),
        gateway_phase,
        director_phase,
        uptime,
        server_stats,
    })
}

#[derive(Default, Clone)]
struct PartitionStats {
    players: Option<i64>,
}

/// Build a `partition_index -> PartitionStats` map from a `kubectl get
/// serverstats -n <ns> -o json` payload. The Funcom operator emits one
/// ServerStats CR per partition with `spec.area.partition` as the id and
/// `status.runtime.players` as the live count. Same source the
/// `gt_server_status.py` cron script consumes.
fn index_serverstats_by_partition(stats: &Value) -> std::collections::HashMap<i64, PartitionStats> {
    let mut out = std::collections::HashMap::new();
    let Some(items) = stats.get("items").and_then(Value::as_array) else {
        return out;
    };
    for item in items {
        let partition = item
            .get("spec")
            .and_then(|s| s.get("area"))
            .and_then(|a| a.get("partition"))
            .and_then(Value::as_i64);
        let Some(partition) = partition else { continue };
        let players = item
            .get("status")
            .and_then(|s| s.get("runtime"))
            .and_then(|r| r.get("players"))
            .and_then(Value::as_i64);
        out.insert(partition, PartitionStats { players });
    }
    out
}

fn string_field(value: &Value, key: &str) -> String {
    match value.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(b)) => b.to_string(),
        _ => String::new(),
    }
}

fn server_stat_from_json(
    server: &Value,
    bg_age: &str,
    stats_by_partition: &std::collections::HashMap<i64, PartitionStats>,
    pod_starts: &std::collections::HashMap<u64, String>,
) -> RemoteBattlegroupServerStat {
    // The Funcom operator names this field `partitionMap` in the BattleGroup
    // CR's `status.servers[]` — confirmed against backed-up live CR YAML.
    // Older / alternate operators have used `map` or `name`, so we keep
    // those as fallbacks. With no map at all `friendly_map_name` returns
    // "Game Server" which is what we want to avoid here.
    let raw_map = server
        .get("partitionMap")
        .and_then(Value::as_str)
        .or_else(|| server.get("map").and_then(Value::as_str))
        .or_else(|| server.get("name").and_then(Value::as_str))
        .unwrap_or_default();
    let partition_index = server
        .get("partitionIndex")
        .and_then(Value::as_u64)
        .or_else(|| server.get("ordinalIndex").and_then(Value::as_u64));
    let friendly = friendly_map_name(raw_map, raw_map);
    let labelled = match partition_index {
        Some(idx) => format!("{friendly} #{idx}"),
        None => friendly,
    };
    let ready_str = match server.get("ready") {
        Some(Value::Bool(b)) => b.to_string(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    };
    // The BG CR's status.servers[] entries don't carry a player count or
    // age. Age comes from the server's own pod start time when known (#21),
    // else the BG-level age; the player count is merged from the matching
    // ServerStats CR (keyed by partitionIndex).
    let age = if let Some(start) = server.get("startTimestamp").and_then(Value::as_str) {
        format_age_since_iso(start)
    } else if let Some(start) = partition_index.and_then(|idx| pod_starts.get(&idx)) {
        format_age_since_iso(start)
    } else {
        bg_age.to_string()
    };
    let players = partition_index
        .and_then(|idx| stats_by_partition.get(&(idx as i64)))
        .and_then(|s| s.players)
        .map(|n| n.to_string())
        .unwrap_or_default();
    RemoteBattlegroupServerStat {
        map: labelled,
        phase: string_field(server, "phase"),
        ready: ready_str,
        players,
        age,
    }
}

/// Format an RFC 3339 timestamp like `"2026-05-22T01:27:53Z"` as a compact
/// elapsed-time string (`5d 3h`, `2h 17m`, `45m`, `12s`). Returns empty
/// string when parsing fails — the UI just shows an empty cell.
fn format_age_since_iso(iso_ts: &str) -> String {
    let parsed = chrono::DateTime::parse_from_rfc3339(iso_ts.trim());
    let Ok(start) = parsed else {
        return String::new();
    };
    let now = chrono::Utc::now();
    let diff = now.signed_duration_since(start.with_timezone(&chrono::Utc));
    let secs = diff.num_seconds().max(0);
    if secs < 60 {
        return format!("{secs}s");
    }
    let minutes = secs / 60;
    if minutes < 60 {
        return format!("{minutes}m");
    }
    let hours = minutes / 60;
    let mins_rem = minutes % 60;
    if hours < 24 {
        return format!("{hours}h {mins_rem}m");
    }
    let days = hours / 24;
    let hours_rem = hours % 24;
    format!("{days}d {hours_rem}h")
}

fn read_guest_package_status(
    runner: &RusshRunner,
    namespace: &str,
    battlegroup_name: &str,
) -> CommandResult<RemoteServerPackageStatus> {
    let script = r#"
set -u
download=/home/dune/.dune/download
manifest="$download/steamapps/appmanifest_4754530.acf"
ns=__NAMESPACE__
bg=__BATTLEGROUP__
read_vdf_value() {
  key="$1"
  file="$2"
  [ -f "$file" ] || return 0
  awk -F '"' -v wanted="$key" '$2 == wanted { print $4; exit }' "$file" 2>/dev/null || true
}
read_file() {
  file="$1"
  [ -f "$file" ] || return 0
  head -n 1 "$file" 2>/dev/null | tr -d '\r\n'
}
printf 'installedBuildId=%s\n' "$(read_vdf_value buildid "$manifest")"
printf 'battlegroupVersion=%s\n' "$(read_file "$download/images/battlegroup/version.txt")"
printf 'operatorVersion=%s\n' "$(read_file "$download/images/operators/version.txt")"
live_image=$(sudo kubectl get battlegroup "$bg" -n "$ns" -o jsonpath='{..image}' 2>/dev/null | tr ' ' '\n' | awk -F: '/self-hosting\/(igw-server|seabass-server):/ { print $NF; exit }' || true)
printf 'liveBattlegroupVersion=%s\n' "$live_image"
"#
    .replace("__NAMESPACE__", &sh_single_quoted(namespace))
    .replace("__BATTLEGROUP__", &sh_single_quoted(battlegroup_name));
    let output = runner.run_script(&script)?;
    let value = |key: &str| {
        output.lines().find_map(|line| {
            let (name, value) = line.split_once('=')?;
            (name == key && !value.trim().is_empty()).then(|| value.trim().to_string())
        })
    };
    Ok(RemoteServerPackageStatus {
        installed_build_id: value("installedBuildId"),
        battlegroup_version: value("battlegroupVersion"),
        live_battlegroup_version: value("liveBattlegroupVersion"),
        operator_version: value("operatorVersion"),
    })
}

pub fn read_remote_server_components(
    runner: &RusshRunner,
    namespace: &str,
) -> CommandResult<Vec<RemoteServerComponent>> {
    let pods = runner.run_json(
        &format!(
            "sudo kubectl get pods -n {} -o json",
            sh_single_quoted(namespace)
        ),
        "remote server pods",
    )?;
    let resources = runner.run_json(
        &format!(
            "sudo kubectl get servergroups,servergateways,serversets -n {} -o json",
            sh_single_quoted(namespace)
        ),
        "remote server resources",
    )?;

    let mut components = vec![
        pod_component("Database", "database", &pods, |role, name| {
            role.contains("database") && !name.contains("-util-")
        }),
        pod_component(
            "Database utilities",
            "database-utilities",
            &pods,
            |role, _| {
                role.contains("database-utility")
                    || role.contains("database-monitor")
                    || role.contains("database-pghero")
            },
        ),
        pod_component("Message Queue", "message-queue", &pods, |role, name| {
            role.contains("message-queue") || name.contains("-mq-")
        }),
        pod_component("Director", "director", &pods, |role, name| {
            role.contains("battlegroup-director") || name.contains("-bgd-")
        }),
        pod_component("Gateway", "gateway", &pods, |role, name| {
            role.contains("server-gateway") || name.contains("-sgw-")
        }),
        pod_component("Text Router", "text-router", &pods, |role, name| {
            role.contains("text-router") || name.contains("-tr-")
        }),
        pod_component("File Browser", "file-browser", &pods, |role, name| {
            role.contains("filebrowser") || name.contains("-fb-")
        }),
    ];
    components.extend(server_resource_components(&resources));
    Ok(components
        .into_iter()
        .filter(|component| component.state != "Not present")
        .collect())
}

pub fn remote_records_from_battlegroups(
    request: &crate::dto::RemoteConnectionRequest,
    value: &Value,
) -> Vec<crate::dto::RemoteServerRecord> {
    value
        .get("items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| remote_record_from_battlegroup(request, item))
        .collect()
}

fn remote_record_from_battlegroup(
    request: &crate::dto::RemoteConnectionRequest,
    item: &Value,
) -> Option<crate::dto::RemoteServerRecord> {
    let namespace = item
        .get("metadata")?
        .get("namespace")?
        .as_str()?
        .to_string();
    let battlegroup_name = item.get("metadata")?.get("name")?.as_str()?.to_string();
    let title = item
        .get("spec")
        .and_then(|spec| spec.get("title"))
        .and_then(Value::as_str)
        .unwrap_or(&battlegroup_name)
        .to_string();
    let phase = item
        .get("status")
        .and_then(|status| status.get("phase"))
        .and_then(Value::as_str)
        .unwrap_or("Unknown")
        .to_string();
    let server_type = request
        .server_type
        .as_deref()
        .unwrap_or("ubuntu")
        .trim()
        .to_string();
    let user = request
        .user
        .as_deref()
        .map(str::trim)
        .unwrap_or_default()
        .to_string();
    Some(crate::dto::RemoteServerRecord {
        id: remote_record_id(&server_type, &request.host, request.key_path.as_deref()),
        name: title,
        host: request.host.clone(),
        user,
        key_path: request.key_path.clone().unwrap_or_default(),
        port: request.port,
        server_type,
        namespace,
        battlegroup_name: battlegroup_name.clone(),
        world_unique_name: battlegroup_name,
        phase,
    })
}

fn remote_record_id(_server_type: &str, host: &str, key_path: Option<&str>) -> String {
    format!(
        "ubuntu:{}:{}",
        host.trim().to_lowercase(),
        key_path.unwrap_or_default().trim().to_lowercase()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn bg(spec: Value, status: Value) -> Value {
        json!({
            "metadata": {"name": "sh-test-bg", "namespace": "funcom-seabass-sh-test"},
            "spec": spec,
            "status": status,
        })
    }

    fn bg_status(bg: &Value) -> Option<RemoteBattlegroupStatus> {
        battlegroup_status_from_json_with_stats(bg, &Value::Null)
    }

    #[test]
    fn maps_reconciling_bg_with_null_director_phase() {
        // Mirrors the user-reported payload: phase Reconciling, gateway
        // Running, director not yet populated. Prior text-parse path was
        // confusing the UI into greying the Director tunnel; under direct
        // kubectl read the director_phase is just "" which the UI treats
        // as "ready enough".
        let value = bg(
            json!({"stop": false}),
            json!({
                "phase": "Reconciling",
                "serverGroupPhase": "Running",
                "directorPhase": Value::Null,
                "stop": Value::Null,
            }),
        );
        let dto = bg_status(&value).expect("status maps");
        assert!(!dto.stop);
        assert_eq!(dto.phase, "Reconciling");
        assert_eq!(dto.server_group_phase, "Running");
        assert_eq!(dto.director_phase, "");
        assert_eq!(dto.uptime, "");
    }

    #[test]
    fn gateway_phase_comes_from_server_gateway_not_server_group() {
        // #21: shape from a backed-up live CR. The gateway reports Healthy
        // while the map servers are still coming up.
        let value = bg(
            json!({"stop": false}),
            json!({
                "phase": "Healthy",
                "serverGroupPhase": "Initializing",
                "utilities": {"serverGateway": {"phase": "Healthy"}},
            }),
        );
        let dto = bg_status(&value).expect("status maps");
        assert_eq!(dto.gateway_phase, "Healthy");
        assert_eq!(dto.server_group_phase, "Initializing");

        let bare = bg(json!({}), json!({"phase": "Stopped"}));
        assert_eq!(bg_status(&bare).expect("status maps").gateway_phase, "");
    }

    #[test]
    fn server_row_age_prefers_its_own_pod_start_over_bg_start() {
        // #21: a partition restarted 5 minutes ago must not show the BG age.
        let pod_start = (chrono::Utc::now() - chrono::Duration::minutes(5)).to_rfc3339();
        let value = bg(
            json!({"stop": false}),
            json!({
                "phase": "Healthy",
                "startTimestamp": "2020-01-01T00:00:00Z",
                "servers": [
                    {"partitionMap": "Survival_1", "partitionIndex": 31, "phase": "Running"},
                    {"partitionMap": "Survival_1", "partitionIndex": 1, "phase": "Running"},
                ],
            }),
        );
        let pod_starts = std::collections::HashMap::from([(31u64, pod_start)]);
        let dto =
            battlegroup_status_from_json_full(&value, &Value::Null, &pod_starts).expect("maps");
        assert_eq!(dto.server_stats[0].age, "5m");
        assert_eq!(dto.server_stats[1].age, dto.uptime);
    }

    #[test]
    fn falls_back_to_status_stop_when_spec_missing() {
        let value = bg(json!({}), json!({"phase": "Stopped", "stop": true}));
        let dto = bg_status(&value).expect("status maps");
        assert!(dto.stop);
        assert_eq!(dto.phase, "Stopped");
    }

    #[test]
    fn server_stats_pulled_from_status_servers_array() {
        let value = bg(
            json!({"stop": false}),
            json!({
                "phase": "Running",
                "servers": [
                    {"map": "Survival_1", "phase": "Running", "ready": true},
                    {"name": "DeepDesert_1", "phase": "Stopped", "ready": false},
                ]
            }),
        );
        let dto = bg_status(&value).expect("status maps");
        assert_eq!(dto.server_stats.len(), 2);
        assert_eq!(
            dto.server_stats[0].map,
            friendly_map_name("Survival_1", "Survival_1")
        );
        assert_eq!(dto.server_stats[0].phase, "Running");
        assert_eq!(dto.server_stats[0].ready, "true");
        // Players empty when no ServerStats CR is supplied — that data lives
        // on a separate CRD and is merged via `_with_stats`.
        assert_eq!(dto.server_stats[0].players, "");
        assert_eq!(
            dto.server_stats[1].map,
            friendly_map_name("DeepDesert_1", "DeepDesert_1")
        );
        assert_eq!(dto.server_stats[1].ready, "false");
        assert_eq!(dto.server_stats[1].age, "");
    }

    #[test]
    fn server_stats_merge_player_count_from_serverstats_crd() {
        // Mirrors the data shape gt_server_status.py reads: each ServerStats
        // CR has spec.area.partition matching the BG's partitionIndex, and
        // status.runtime.players is the live count.
        let value = bg(
            json!({"stop": false}),
            json!({
                "phase": "Healthy",
                "servers": [
                    {"partitionMap": "Survival_1", "partitionIndex": 1, "phase": "Running", "ready": true},
                    {"partitionMap": "Survival_1", "partitionIndex": 31, "phase": "Running", "ready": true},
                    {"partitionMap": "Overmap", "partitionIndex": 2, "phase": "Running", "ready": true},
                ],
            }),
        );
        let stats = json!({
            "items": [
                {"spec": {"area": {"partition": 1, "map": "Survival_1"}}, "status": {"runtime": {"players": 7}}},
                {"spec": {"area": {"partition": 31, "map": "Survival_1"}}, "status": {"runtime": {"players": 0}}},
                {"spec": {"area": {"partition": 2, "map": "Overmap"}}, "status": {"runtime": {"players": 3}}},
            ],
        });
        let dto = battlegroup_status_from_json_with_stats(&value, &stats).expect("status maps");
        assert_eq!(dto.server_stats[0].players, "7");
        assert_eq!(dto.server_stats[1].players, "0");
        assert_eq!(dto.server_stats[2].players, "3");
    }

    #[test]
    fn server_stats_player_count_blank_when_partition_missing_from_stats() {
        let value = bg(
            json!({"stop": false}),
            json!({
                "servers": [
                    {"partitionMap": "Survival_1", "partitionIndex": 1, "phase": "Running", "ready": true},
                ],
            }),
        );
        let stats = json!({"items": []});
        let dto = battlegroup_status_from_json_with_stats(&value, &stats).expect("status maps");
        assert_eq!(dto.server_stats[0].players, "");
    }

    #[test]
    fn server_stats_use_partition_map_and_index_from_real_cr() {
        // Mirrors the actual Funcom operator status.servers[] shape captured
        // from a live BattleGroup CR backup. Pre-fix the map column showed
        // "Game Server" for every row because we were reading `map`/`name`
        // instead of `partitionMap`.
        let value = bg(
            json!({"stop": false}),
            json!({
                "phase": "Healthy",
                "servers": [
                    {
                        "partitionMap": "Survival_1",
                        "partitionIndex": 1,
                        "phase": "Running",
                        "ready": true,
                    },
                    {
                        "partitionMap": "Survival_1",
                        "partitionIndex": 31,
                        "phase": "Running",
                        "ready": true,
                    },
                    {
                        "partitionMap": "Overmap",
                        "partitionIndex": 2,
                        "phase": "Running",
                        "ready": true,
                    },
                ]
            }),
        );
        let dto = bg_status(&value).expect("status maps");
        assert_eq!(dto.server_stats.len(), 3);
        assert_eq!(dto.server_stats[0].map, "Hagga Basin #1");
        assert_eq!(dto.server_stats[1].map, "Hagga Basin #31");
        assert_eq!(dto.server_stats[2].map, "Overmap #2");
        assert!(dto.server_stats.iter().all(|s| s.phase == "Running"));
        assert!(dto.server_stats.iter().all(|s| s.ready == "true"));
    }

    #[test]
    fn returns_none_when_not_a_battlegroup_resource() {
        let value = json!({"kind": "Pod", "spec": {}, "status": {}});
        assert!(bg_status(&value).is_none());
    }

    #[test]
    fn bg_start_timestamp_propagates_to_every_server_row_when_per_server_missing() {
        // status.startTimestamp from the live CR backup is one minute in the
        // past for this test.
        let one_min_ago = (chrono::Utc::now() - chrono::Duration::minutes(1))
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let value = bg(
            json!({"stop": false}),
            json!({
                "phase": "Running",
                "startTimestamp": one_min_ago,
                "servers": [
                    {"partitionMap": "Survival_1", "partitionIndex": 1, "phase": "Running", "ready": true},
                    {"partitionMap": "Overmap", "partitionIndex": 2, "phase": "Running", "ready": true},
                ],
            }),
        );
        let dto = bg_status(&value).expect("status maps");
        // All rows pick up the same BG-level age.
        assert_eq!(dto.server_stats.len(), 2);
        for row in &dto.server_stats {
            assert!(
                row.age == "1m" || row.age == "60s",
                "row age was {:?}",
                row.age
            );
        }
    }

    #[test]
    fn database_director_phases_pulled_from_nested_status() {
        // Live CR shape: status.database.phase + status.utilities.director.phase,
        // not top-level databasePhase/directorPhase.
        let value = bg(
            json!({"stop": false}),
            json!({
                "phase": "Healthy",
                "serverGroupPhase": "Running",
                "database": {"phase": "Ready", "address": "1.2.3.4:15432"},
                "utilities": {
                    "director": {"phase": "Healthy", "address": "1.2.3.4:30393"},
                },
            }),
        );
        let dto = bg_status(&value).expect("status maps");
        assert_eq!(dto.database_phase, "Ready");
        assert_eq!(dto.director_phase, "Healthy");
    }

    #[test]
    fn uptime_derived_from_start_timestamp_when_no_literal() {
        let one_hr_ago =
            (chrono::Utc::now() - chrono::Duration::hours(1) - chrono::Duration::minutes(2))
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let value = bg(
            json!({"stop": false}),
            json!({"phase": "Healthy", "startTimestamp": one_hr_ago}),
        );
        let dto = bg_status(&value).expect("status maps");
        assert_eq!(dto.uptime, "1h 2m");
    }

    #[test]
    fn uptime_prefers_literal_string_when_older_operator_set_it() {
        let value = bg(
            json!({"stop": false}),
            json!({
                "phase": "Healthy",
                "uptime": "1h2m",
                "startTimestamp": "2026-05-22T01:27:53Z",
            }),
        );
        let dto = bg_status(&value).expect("status maps");
        assert_eq!(dto.uptime, "1h2m");
    }

    #[test]
    fn format_age_since_iso_handles_common_shapes() {
        assert_eq!(format_age_since_iso(""), "");
        assert_eq!(format_age_since_iso("not a timestamp"), "");
        let recent = (chrono::Utc::now() - chrono::Duration::seconds(30))
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        assert!(format_age_since_iso(&recent).ends_with('s'));
        let hours =
            (chrono::Utc::now() - chrono::Duration::hours(3) - chrono::Duration::minutes(15))
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        assert_eq!(format_age_since_iso(&hours), "3h 15m");
        let days = (chrono::Utc::now() - chrono::Duration::days(5) - chrono::Duration::hours(7))
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        assert_eq!(format_age_since_iso(&days), "5d 7h");
    }
}
