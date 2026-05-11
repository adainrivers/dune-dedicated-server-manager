//! BattleGroup map instance partition management.
//!
//! The vendor BattleGroup custom resource stores the durable list of map
//! partitions at `spec.database.template.spec.deployment.spec.worldPartitions`.
//! Updating that list, then restarting the BattleGroup, lets the operators and
//! game database converge on additional Survival or Deep Desert instances.
//! Deep Desert instances are distinct partition IDs on dimension zero; Survival
//! instances use distinct dimensions.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    errors::failure,
    models::CommandResult,
    orchestration::{BattlegroupRef, RemoteCommandRunner},
    validation::validate_kube_arg,
};

/// Supported map family for user-facing instance count operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InstanceMap {
    /// The primary survival map, stored as `Survival_1`.
    Survival1,
    /// The Deep Desert map, stored as `DeepDesert_1`.
    DeepDesert,
}

impl InstanceMap {
    /// Parses a CLI/user map name.
    pub fn parse(value: &str) -> CommandResult<Self> {
        match value.to_ascii_lowercase().as_str() {
            "survival-1" | "survival_1" | "survival" => Ok(Self::Survival1),
            "deep-desert" | "deep_desert" | "deepdesert" | "deepdesert_1" | "deep-desert-1" => {
                Ok(Self::DeepDesert)
            }
            _ => Err(failure(format!(
                "Unsupported instance map {value}; use survival-1 or deep-desert"
            ))),
        }
    }

    /// Returns the Kubernetes/game map name.
    pub fn map_name(self) -> &'static str {
        match self {
            Self::Survival1 => "Survival_1",
            Self::DeepDesert => "DeepDesert_1",
        }
    }
}

/// Request for setting the desired number of map partitions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetMapInstancesRequest {
    /// BattleGroup namespace and resource name.
    pub battlegroup: BattlegroupRef,
    /// Map family to modify.
    pub map: InstanceMap,
    /// Desired partition count. Must be at least one.
    pub count: usize,
    /// Deep Desert partition IDs that should be marked PvP in user config.
    ///
    /// `None` leaves config files untouched. `Some(Vec::new())` clears the
    /// configured PvP partition list.
    pub pvp_partition_ids: Option<Vec<i64>>,
}

impl SetMapInstancesRequest {
    /// Creates a request without PvP config changes.
    pub fn new(battlegroup: BattlegroupRef, map: InstanceMap, count: usize) -> Self {
        Self {
            battlegroup,
            map,
            count,
            pvp_partition_ids: None,
        }
    }

    fn validate(&self) -> CommandResult<()> {
        self.battlegroup.validate()?;
        if self.count == 0 || self.count > 64 {
            return Err(failure("--count must be between 1 and 64"));
        }
        if let Some(ids) = &self.pvp_partition_ids {
            for id in ids {
                if *id <= 0 {
                    return Err(failure("PvP partition IDs must be positive"));
                }
            }
            if self.map != InstanceMap::DeepDesert {
                return Err(failure(
                    "PvP partition config is currently supported only for deep-desert",
                ));
            }
        }
        Ok(())
    }
}

/// Result of setting map partitions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetMapInstancesResult {
    /// Map name that was modified.
    pub map: String,
    /// Partition IDs after the patch.
    pub partition_ids: Vec<i64>,
    /// PvP partition IDs written to config.
    pub pvp_partition_ids: Vec<i64>,
    /// Whether a BattleGroup restart is required for all consumers to see the change.
    pub restart_required: bool,
    /// Whether the BattleGroup resource was patched.
    pub battlegroup_patched: bool,
    /// Whether PvP config files were updated.
    pub pvp_config_updated: bool,
}

/// Orchestrates durable BattleGroup map instance updates.
#[derive(Debug, Clone)]
pub struct MapInstanceOrchestrator<R> {
    runner: R,
}

impl<R> MapInstanceOrchestrator<R>
where
    R: RemoteCommandRunner,
{
    /// Creates a map instance orchestrator around a remote command runner.
    pub fn new(runner: R) -> Self {
        Self { runner }
    }

    /// Sets the desired partition count in the BattleGroup resource.
    pub fn set_instances(
        &self,
        request: &SetMapInstancesRequest,
    ) -> CommandResult<SetMapInstancesResult> {
        request.validate()?;

        let battlegroup = self.battlegroup(&request.battlegroup)?;
        let update = build_world_partition_update(&battlegroup, request.map, request.count)?;
        let mut battlegroup_patched = false;

        if update.patch_required {
            let patch = serde_json::to_string(&update.patch_operations)
                .map_err(|err| failure(format!("Failed to serialize instance patch: {err}")))?;
            let command = format!(
                "sudo kubectl patch battlegroup {} -n {} --type=json -p {} -o json",
                sh_single_quoted(&request.battlegroup.name),
                sh_single_quoted(&request.battlegroup.namespace),
                sh_single_quoted(&patch),
            );
            self.runner
                .run_json(&command, "map instance battlegroup patch")?;
            battlegroup_patched = true;
        }

        let mut pvp_config_updated = false;
        if let Some(pvp_partition_ids) = &request.pvp_partition_ids {
            self.write_deep_desert_pvp_config(&request.battlegroup.namespace, pvp_partition_ids)?;
            pvp_config_updated = true;
        }

        Ok(SetMapInstancesResult {
            map: request.map.map_name().to_string(),
            partition_ids: update.partition_ids,
            pvp_partition_ids: request.pvp_partition_ids.clone().unwrap_or_default(),
            restart_required: battlegroup_patched || pvp_config_updated,
            battlegroup_patched,
            pvp_config_updated,
        })
    }

    fn battlegroup(&self, battlegroup: &BattlegroupRef) -> CommandResult<Value> {
        battlegroup.validate()?;
        let command = format!(
            "sudo kubectl get battlegroup {} -n {} -o json",
            sh_single_quoted(&battlegroup.name),
            sh_single_quoted(&battlegroup.namespace),
        );
        self.runner.run_json(&command, "map instance battlegroup")
    }

    fn write_deep_desert_pvp_config(
        &self,
        namespace: &str,
        pvp_partition_ids: &[i64],
    ) -> CommandResult<()> {
        validate_kube_arg(namespace, "namespace")?;
        let list = pvp_partition_ids
            .iter()
            .map(i64::to_string)
            .collect::<Vec<_>>()
            .join(" ");
        self.runner
            .run_script(&write_pvp_config_script(namespace, &list))?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WorldPartitionUpdate {
    partition_ids: Vec<i64>,
    patch_required: bool,
    patch_operations: Vec<Value>,
}

fn build_world_partition_update(
    battlegroup: &Value,
    map: InstanceMap,
    count: usize,
) -> CommandResult<WorldPartitionUpdate> {
    let world_partitions_path = [
        "spec",
        "database",
        "template",
        "spec",
        "deployment",
        "spec",
        "worldPartitions",
    ];
    let world_partitions = descend(battlegroup, &world_partitions_path)?
        .as_array()
        .ok_or_else(|| failure("BattleGroup worldPartitions is not an array"))?;
    let map_name = map.map_name();
    let map_index = world_partitions
        .iter()
        .position(|item| item["map"].as_str() == Some(map_name))
        .ok_or_else(|| {
            failure(format!(
                "BattleGroup has no worldPartitions entry for {map_name}"
            ))
        })?;
    let entry = &world_partitions[map_index];
    let current = entry["partitions"]
        .as_array()
        .ok_or_else(|| failure(format!("{map_name} partitions is not an array")))?;
    if current.is_empty() {
        return Err(failure(format!(
            "{map_name} has no template partition to clone"
        )));
    }

    let mut desired = current.clone();
    desired.sort_by_key(|item| {
        (
            item["dimension"].as_i64().unwrap_or(i64::MAX),
            item["id"].as_i64().unwrap_or(i64::MAX),
        )
    });

    let used_ids = collect_partition_ids(world_partitions);
    while desired.len() < count {
        let dimension = next_partition_dimension(map, &desired);
        let id = next_free_partition_id(&used_ids, &desired)?;
        let mut next = desired[0].clone();
        next["id"] = json!(id);
        next["dimension"] = json!(dimension);
        next["disable"] = json!(false);
        desired.push(next);
    }
    desired.truncate(count);

    let partition_ids = desired
        .iter()
        .map(|item| {
            item["id"]
                .as_i64()
                .ok_or_else(|| failure("Desired partition is missing id"))
        })
        .collect::<CommandResult<Vec<_>>>()?;

    let patch_required = desired != *current;
    let patch_operations = if patch_required {
        vec![json!({
            "op": "replace",
            "path": format!(
                "/spec/database/template/spec/deployment/spec/worldPartitions/{map_index}/partitions"
            ),
            "value": desired,
        })]
    } else {
        Vec::new()
    };

    Ok(WorldPartitionUpdate {
        partition_ids,
        patch_required,
        patch_operations,
    })
}

fn next_partition_dimension(map: InstanceMap, desired: &[Value]) -> i64 {
    match map {
        InstanceMap::DeepDesert => 0,
        InstanceMap::Survival1 => {
            desired
                .iter()
                .filter_map(|item| item["dimension"].as_i64())
                .max()
                .unwrap_or(-1)
                + 1
        }
    }
}

fn descend<'a>(value: &'a Value, path: &[&str]) -> CommandResult<&'a Value> {
    let mut current = value;
    for segment in path {
        current = current
            .get(*segment)
            .ok_or_else(|| failure(format!("BattleGroup is missing {segment}")))?;
    }
    Ok(current)
}

fn collect_partition_ids(world_partitions: &[Value]) -> Vec<i64> {
    let mut ids = Vec::new();
    for entry in world_partitions {
        for partition in entry["partitions"].as_array().into_iter().flatten() {
            if let Some(id) = partition["id"].as_i64() {
                ids.push(id);
            }
        }
    }
    ids
}

fn next_free_partition_id(existing: &[i64], desired: &[Value]) -> CommandResult<i64> {
    let mut used = existing.to_vec();
    used.extend(desired.iter().filter_map(|item| item["id"].as_i64()));
    let max = used.into_iter().max().unwrap_or(0);
    max.checked_add(1)
        .ok_or_else(|| failure("No free partition ID is available"))
}

fn write_pvp_config_script(namespace: &str, pvp_ids: &str) -> String {
    format!(
        r#"
set -euo pipefail
ns={namespace}
pvp_ids={pvp_ids}
pvc=$(sudo kubectl get pvc -n "$ns" --no-headers | awk '$1 !~ /-db-pvc$/ && $1 ~ /-pvc$/ {{ print $1; exit }}')
if [ -z "$pvc" ]; then
  echo "No shared battlegroup PVC found in $ns" >&2
  exit 1
fi
pv=$(sudo kubectl get pvc "$pvc" -n "$ns" -o jsonpath='{{.spec.volumeName}}')
pv_path=$(sudo kubectl get pv "$pv" -o jsonpath='{{.spec.local.path}}{{.spec.hostPath.path}}')
if [ -z "$pv_path" ]; then
  echo "No host path found for PVC $pvc" >&2
  exit 1
fi

update_ini() {{
  file="$1"
  sudo mkdir -p "$(dirname "$file")"
  sudo touch "$file"
  backup="$file.manager-backup-$(date +%Y%m%d%H%M%S)"
  sudo cp "$file" "$backup"
  tmp=$(mktemp)
  sudo awk -v ids="$pvp_ids" '
  BEGIN {{ section="[/Script/DuneSandbox.PvpPveSettings]"; insec=0; wrote=0 }}
  function write_block(    n, parts, i) {{
    if (!wrote) {{
      print section
      print "; Managed by Dune Dedicated Server Manager"
      print "m_bIsInitialized=True"
      print "m_bShouldForceEnablePvpOnAllPartitions=False"
      print "!m_PvpEnabledPartitions=ClearArray"
      n=split(ids, parts, " ")
      for (i=1; i<=n; i++) if (parts[i] != "") print "+m_PvpEnabledPartitions=" parts[i]
      print "!m_EffectivePvpEnabledPartitions=ClearArray"
      for (i=1; i<=n; i++) if (parts[i] != "") print "+m_EffectivePvpEnabledPartitions=(UID=" parts[i] ")"
      wrote=1
    }}
  }}
  $0 == section {{ insec=1; next }}
  /^\[/ {{
    if (insec) {{ write_block(); insec=0 }}
    print
    next
  }}
  insec {{ next }}
  {{ print }}
  END {{ if (insec || !wrote) write_block() }}
  ' "$file" > "$tmp"
  sudo cp "$tmp" "$file"
  rm -f "$tmp"
}}

update_ini "$pv_path/Saved/UserSettings/UserGame.ini"
update_ini "$pv_path/Saved/Config/LinuxServer/Game.ini"
"#,
        namespace = sh_single_quoted(namespace),
        pvp_ids = sh_single_quoted(pvp_ids)
    )
}

fn sh_single_quoted(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_battlegroup() -> Value {
        json!({
            "spec": {
                "database": {
                    "template": {
                        "spec": {
                            "deployment": {
                                "spec": {
                                    "worldPartitions": [
                                        {"map":"Survival_1","partitions":[{"id":1,"dimension":0,"disable":false,"minX":0,"minY":0,"maxX":1,"maxY":1}]},
                                        {"map":"Other","partitions":[{"id":2,"dimension":0,"disable":false}]},
                                        {"map":"DeepDesert_1","partitions":[{"id":8,"dimension":0,"disable":false,"minX":0,"minY":0,"maxX":1,"maxY":1}]}
                                    ]
                                }
                            }
                        }
                    }
                }
            }
        })
    }

    #[test]
    fn adds_deep_desert_partitions_on_dimension_zero() {
        let update =
            build_world_partition_update(&sample_battlegroup(), InstanceMap::DeepDesert, 3)
                .unwrap();

        assert_eq!(update.partition_ids, vec![8, 9, 10]);
        assert!(update.patch_required);
        assert_eq!(
            update.patch_operations[0]["path"],
            "/spec/database/template/spec/deployment/spec/worldPartitions/2/partitions"
        );
        assert_eq!(
            update.patch_operations[0]["value"],
            json!([
                {"id":8,"dimension":0,"disable":false,"minX":0,"minY":0,"maxX":1,"maxY":1},
                {"id":9,"dimension":0,"disable":false,"minX":0,"minY":0,"maxX":1,"maxY":1},
                {"id":10,"dimension":0,"disable":false,"minX":0,"minY":0,"maxX":1,"maxY":1}
            ])
        );
    }

    #[test]
    fn shrinks_survival_partitions_by_dimension_order() {
        let mut bg = sample_battlegroup();
        bg["spec"]["database"]["template"]["spec"]["deployment"]["spec"]["worldPartitions"][0]
            ["partitions"] = json!([
            {"id":1,"dimension":0,"disable":false},
            {"id":30,"dimension":2,"disable":false},
            {"id":29,"dimension":1,"disable":false}
        ]);

        let update = build_world_partition_update(&bg, InstanceMap::Survival1, 2).unwrap();

        assert_eq!(update.partition_ids, vec![1, 29]);
        assert!(update.patch_required);
    }

    #[test]
    fn adds_survival_partitions_with_new_dimensions() {
        let update = build_world_partition_update(&sample_battlegroup(), InstanceMap::Survival1, 3)
            .unwrap();

        assert_eq!(update.partition_ids, vec![1, 9, 10]);
        assert_eq!(
            update.patch_operations[0]["value"],
            json!([
                {"id":1,"dimension":0,"disable":false,"minX":0,"minY":0,"maxX":1,"maxY":1},
                {"id":9,"dimension":1,"disable":false,"minX":0,"minY":0,"maxX":1,"maxY":1},
                {"id":10,"dimension":2,"disable":false,"minX":0,"minY":0,"maxX":1,"maxY":1}
            ])
        );
    }
}
