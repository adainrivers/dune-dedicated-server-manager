use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    errors::failure,
    models::CommandResult,
    orchestration::{
        OperationSink, OrchestrationEvent, ProviderKind, RemoteCommandRunner, StepAction,
        StepDomain,
    },
    validation::validate_kube_arg,
};

/// Request for enabling the vendor experimental low-memory profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExperimentalSwapRequest {
    /// Kubernetes namespace containing the BattleGroup.
    pub namespace: String,
    /// BattleGroup resource name.
    pub battlegroup_name: String,
    /// Swap file size in GiB.
    pub swap_size_gib: u64,
    /// Whether k3s should be restarted to apply kubelet swap settings.
    pub restart_k3s: bool,
}

impl ExperimentalSwapRequest {
    /// Creates a request using the vendor-style 30 GiB swap file and k3s restart.
    pub fn new(namespace: impl Into<String>, battlegroup_name: impl Into<String>) -> Self {
        Self {
            namespace: namespace.into(),
            battlegroup_name: battlegroup_name.into(),
            swap_size_gib: 30,
            restart_k3s: true,
        }
    }

    fn validate(&self) -> CommandResult<()> {
        validate_kube_arg(&self.namespace, "namespace")?;
        validate_kube_arg(&self.battlegroup_name, "battlegroup name")?;
        if !(1..=256).contains(&self.swap_size_gib) {
            return Err(failure("--swap-size-gib must be between 1 and 256"));
        }
        Ok(())
    }
}

/// Request for applying the low-memory BattleGroup resource profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LowMemoryBattlegroupProfileRequest {
    /// Kubernetes namespace containing the BattleGroup.
    pub namespace: String,
    /// BattleGroup resource name.
    pub battlegroup_name: String,
    /// Swap file size in GiB used to choose the profile strength.
    pub swap_size_gib: u64,
}

impl LowMemoryBattlegroupProfileRequest {
    /// Creates a low-memory resource profile request.
    pub fn new(
        namespace: impl Into<String>,
        battlegroup_name: impl Into<String>,
        swap_size_gib: u64,
    ) -> Self {
        Self {
            namespace: namespace.into(),
            battlegroup_name: battlegroup_name.into(),
            swap_size_gib,
        }
    }

    fn validate(&self) -> CommandResult<()> {
        validate_kube_arg(&self.namespace, "namespace")?;
        validate_kube_arg(&self.battlegroup_name, "battlegroup name")?;
        if !(1..=256).contains(&self.swap_size_gib) {
            return Err(failure("swap size must be between 1 and 256 GiB"));
        }
        Ok(())
    }
}

/// Snapshot of the guest experimental swap state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExperimentalSwapStatus {
    /// Whether `/swapfile` exists.
    pub swap_file_exists: bool,
    /// Whether `/swapfile` is currently active.
    pub swap_active: bool,
    /// Configured `/swapfile` size in bytes, when known.
    pub swap_file_bytes: Option<u64>,
    /// Active swap size in bytes, when active and reported by the kernel.
    pub active_swap_bytes: Option<u64>,
    /// Whether `/etc/fstab` contains a `/swapfile` entry.
    pub fstab_configured: bool,
    /// Whether OpenRC has the swap service enabled.
    pub openrc_swap_enabled: bool,
    /// Whether the k3s kubelet config enables `failSwapOn: false`.
    pub kubelet_swap_configured: bool,
    /// Whether the BattleGroup memory profile already matches this experimental profile.
    pub battlegroup_profile_applied: Option<bool>,
}

/// Result of applying the experimental swap profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExperimentalSwapResult {
    /// Status after applying the profile.
    pub status: ExperimentalSwapStatus,
    /// Number of JSON Patch operations applied to the BattleGroup resource.
    pub battlegroup_patch_operations: usize,
}

/// Enables the guest swap file and patches BattleGroup memory requests/limits.
pub struct ExperimentalSwapOrchestrator<R> {
    runner: R,
}

impl<R> ExperimentalSwapOrchestrator<R>
where
    R: RemoteCommandRunner,
{
    /// Creates an experimental swap orchestrator around a remote guest runner.
    pub fn new(runner: R) -> Self {
        Self { runner }
    }

    /// Reads guest swap state and, optionally, BattleGroup memory profile state.
    pub fn status(
        &self,
        battlegroup: Option<(&str, &str)>,
    ) -> CommandResult<ExperimentalSwapStatus> {
        let mut status: ExperimentalSwapStatus = serde_json::from_value(
            self.runner
                .run_json(EXPERIMENTAL_SWAP_STATUS_SCRIPT, "experimental swap status")?,
        )
        .map_err(|err| failure(format!("Failed to parse experimental swap status: {err}")))?;

        if let Some((namespace, battlegroup_name)) = battlegroup {
            validate_kube_arg(namespace, "namespace")?;
            validate_kube_arg(battlegroup_name, "battlegroup name")?;
            let value = self.battlegroup(namespace, battlegroup_name)?;
            status.battlegroup_profile_applied =
                Some(experimental_swap_patch_operations(&value)?.is_empty());
        }

        Ok(status)
    }

    /// Enables swap and applies the experimental low-memory BattleGroup profile.
    pub fn enable(
        &self,
        request: &ExperimentalSwapRequest,
        sink: &mut impl OperationSink,
    ) -> CommandResult<ExperimentalSwapResult> {
        request.validate()?;

        emit(
            sink,
            "guest-swap.enable",
            "Enabling guest experimental swap.",
            StepDomain::Guest,
            StepAction::Configure,
        );
        self.runner.run_script(&enable_swap_script(
            request.swap_size_gib,
            request.restart_k3s,
        ))?;

        let operation_count = self.apply_battlegroup_memory_profile(
            &LowMemoryBattlegroupProfileRequest::new(
                &request.namespace,
                &request.battlegroup_name,
                request.swap_size_gib,
            ),
            sink,
        )?;

        emit(
            sink,
            "guest-swap.status",
            "Verifying experimental swap status.",
            StepDomain::Guest,
            StepAction::Check,
        );
        let status = self.status(Some((&request.namespace, &request.battlegroup_name)))?;
        Ok(ExperimentalSwapResult {
            status,
            battlegroup_patch_operations: operation_count,
        })
    }

    /// Applies only the BattleGroup memory profile, without touching swap or k3s.
    pub fn apply_battlegroup_memory_profile(
        &self,
        request: &LowMemoryBattlegroupProfileRequest,
        sink: &mut impl OperationSink,
    ) -> CommandResult<usize> {
        request.validate()?;

        emit(
            sink,
            "bg-swap.patch-memory",
            "Applying low-memory BattleGroup memory profile.",
            StepDomain::Kubernetes,
            StepAction::Patch,
        );
        let battlegroup = self.battlegroup(&request.namespace, &request.battlegroup_name)?;
        let operations =
            experimental_swap_patch_operations_for_swap(&battlegroup, request.swap_size_gib)?;
        let operation_count = operations.len();
        if !operations.is_empty() {
            let patch = serde_json::to_string(&operations).map_err(|err| {
                failure(format!(
                    "Failed to serialize experimental swap patch: {err}"
                ))
            })?;
            let command = format!(
                "sudo kubectl patch battlegroup {} -n {} --type=json -p {} -o json",
                sh_single_quoted(&request.battlegroup_name),
                sh_single_quoted(&request.namespace),
                sh_single_quoted(&patch),
            );
            self.runner
                .run_json(&command, "experimental swap battlegroup patch")?;
        }

        Ok(operation_count)
    }

    fn battlegroup(&self, namespace: &str, battlegroup_name: &str) -> CommandResult<Value> {
        let command = format!(
            "sudo kubectl get battlegroup {} -n {} -o json",
            sh_single_quoted(battlegroup_name),
            sh_single_quoted(namespace),
        );
        self.runner
            .run_json(&command, "experimental swap battlegroup")
    }
}

const EXPERIMENTAL_SWAP_STATUS_SCRIPT: &str = r#"
set -euo pipefail
swap_file_exists=false
swap_active=false
swap_file_bytes=null
active_swap_bytes=null
fstab_configured=false
openrc_swap_enabled=false
kubelet_swap_configured=false

if [ -f /swapfile ]; then
  swap_file_exists=true
  swap_file_bytes=$(stat -c '%s' /swapfile 2>/dev/null || echo null)
fi
if awk '$1 == "/swapfile" { found = 1 } END { exit(found ? 0 : 1) }' /proc/swaps 2>/dev/null; then
  swap_active=true
  active_swap_bytes=$(awk '$1 == "/swapfile" { print $3 * 1024 }' /proc/swaps | head -n1)
fi
if grep -Eq '^[[:space:]]*/swapfile[[:space:]]' /etc/fstab 2>/dev/null; then
  fstab_configured=true
fi
if rc-update show 2>/dev/null | grep -Eq '^[[:space:]]*swap[[:space:]].*boot'; then
  openrc_swap_enabled=true
fi
if grep -REq '^[[:space:]]*failSwapOn:[[:space:]]*false[[:space:]]*$' /etc/rancher/k3s/kubelet.config /etc/rancher/k3s/kubelet-config.yaml 2>/dev/null \
  && { grep -Eq 'config=/etc/rancher/k3s/kubelet-config.yaml' /etc/rancher/k3s/config.yaml 2>/dev/null \
    || grep -REq 'config=/etc/rancher/k3s/kubelet.config|config=/etc/rancher/k3s/kubelet-config.yaml' /etc/rancher/k3s/config.yaml.d 2>/dev/null; }; then
  kubelet_swap_configured=true
fi
printf '{"swapFileExists":%s,"swapActive":%s,"swapFileBytes":%s,"activeSwapBytes":%s,"fstabConfigured":%s,"openrcSwapEnabled":%s,"kubeletSwapConfigured":%s,"battlegroupProfileApplied":null}\n' \
  "$swap_file_exists" "$swap_active" "$swap_file_bytes" "$active_swap_bytes" "$fstab_configured" "$openrc_swap_enabled" "$kubelet_swap_configured"
"#;

fn enable_swap_script(swap_size_gib: u64, restart_k3s: bool) -> String {
    let restart = if restart_k3s { "true" } else { "false" };
    format!(
        r#"
set -euo pipefail
swap_size_gib={swap_size_gib}
restart_k3s={restart}
swap_bytes=$((swap_size_gib * 1024 * 1024 * 1024))

if [ ! -f /swapfile ] || [ "$(stat -c '%s' /swapfile 2>/dev/null || echo 0)" -lt "$swap_bytes" ]; then
  sudo swapoff /swapfile >/dev/null 2>&1 || true
  sudo rm -f /swapfile
  sudo dd if=/dev/zero of=/swapfile bs=1M count=$((swap_size_gib * 1024)) status=none
  sudo chmod 600 /swapfile
  sudo mkswap /swapfile >/dev/null
fi

if ! grep -Eq '^[[:space:]]*/swapfile[[:space:]]' /etc/fstab 2>/dev/null; then
  printf '/swapfile none swap sw 0 0\n' | sudo tee -a /etc/fstab >/dev/null
fi

sudo swapon /swapfile >/dev/null 2>&1 || true
sudo rc-update add swap boot >/dev/null 2>&1 || true

sudo mkdir -p /etc/rancher/k3s
if [ ! -f /etc/rancher/k3s/kubelet-config.yaml ] || ! grep -Eq '^[[:space:]]*failSwapOn:[[:space:]]*false[[:space:]]*$' /etc/rancher/k3s/kubelet-config.yaml; then
  printf 'failSwapOn: false\nmemorySwap:\n  swapBehavior: LimitedSwap\n' | sudo tee /etc/rancher/k3s/kubelet-config.yaml >/dev/null
fi
if [ ! -f /etc/rancher/k3s/config.yaml ]; then
  printf 'kubelet-arg:\n- config=/etc/rancher/k3s/kubelet-config.yaml\n' | sudo tee /etc/rancher/k3s/config.yaml >/dev/null
elif grep -q 'kubelet-arg:' /etc/rancher/k3s/config.yaml && ! grep -q 'config=/etc/rancher/k3s/kubelet-config.yaml' /etc/rancher/k3s/config.yaml; then
  echo "k3s config already has kubelet-arg entries; cannot safely add swap config automatically" >&2
  exit 1
elif ! grep -q 'config=/etc/rancher/k3s/kubelet-config.yaml' /etc/rancher/k3s/config.yaml; then
  printf '\nkubelet-arg:\n- config=/etc/rancher/k3s/kubelet-config.yaml\n' | sudo tee -a /etc/rancher/k3s/config.yaml >/dev/null
fi

if [ "$restart_k3s" = "true" ]; then
  sudo rc-service k3s stop >/dev/null 2>&1 || true
  if [ -x /usr/local/bin/k3s-killall.sh ]; then
    sudo /usr/local/bin/k3s-killall.sh >/dev/null 2>&1 || true
  fi
  sudo rc-service k3s start >/dev/null
  elapsed=0
  until sudo kubectl get nodes >/dev/null 2>&1; do
    sleep 2
    elapsed=$((elapsed + 2))
    if [ "$elapsed" -ge 180 ]; then echo "k3s API did not return after enabling swap" >&2; exit 1; fi
  done
  sudo kubectl wait --for=condition=Ready node --all --timeout=180s >/dev/null || true
fi
"#
    )
}

fn experimental_swap_patch_operations(value: &Value) -> CommandResult<Vec<Value>> {
    experimental_swap_patch_operations_for_swap(value, 30)
}

fn experimental_swap_patch_operations_for_swap(
    value: &Value,
    swap_size_gib: u64,
) -> CommandResult<Vec<Value>> {
    let sets_path = ["spec", "serverGroup", "template", "spec", "sets"];
    let sets = value
        .pointer("/spec/serverGroup/template/spec/sets")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            failure("BattleGroup did not contain spec.serverGroup.template.spec.sets")
        })?;
    let mut operations = Vec::new();
    for (index, set) in sets.iter().enumerate() {
        let map = set["map"].as_str().unwrap_or_default();
        let profile = memory_profile_for_map(map, swap_size_gib);
        let mut base = sets_path
            .iter()
            .map(|part| (*part).to_string())
            .collect::<Vec<_>>();
        base.push(index.to_string());
        let resources = set.get("resources");
        if resources.is_none() || !resources.is_some_and(Value::is_object) {
            let mut path = base.clone();
            path.push("resources".to_string());
            operations.push(add_operation(
                &path,
                json!({
                    "limits": { "memory": profile.limit },
                    "requests": { "memory": profile.request },
                }),
            ));
            continue;
        }
        ensure_memory_value(
            set,
            &base,
            "limits",
            profile.limit.as_str(),
            &mut operations,
        );
        ensure_memory_value(
            set,
            &base,
            "requests",
            profile.request.as_str(),
            &mut operations,
        );
    }
    Ok(operations)
}

fn ensure_memory_value(
    set: &Value,
    base_path: &[String],
    resource_kind: &str,
    desired: &str,
    operations: &mut Vec<Value>,
) {
    let resource = set
        .get("resources")
        .and_then(|resources| resources.get(resource_kind));
    if resource.is_none() || !resource.is_some_and(Value::is_object) {
        let mut path = base_path.to_owned();
        path.push("resources".to_string());
        path.push(resource_kind.to_string());
        operations.push(add_operation(&path, json!({ "memory": desired })));
        return;
    }

    let current = resource
        .and_then(|value| value.get("memory"))
        .and_then(Value::as_str);
    if current == Some(desired) {
        return;
    }
    let op = if current.is_some() { "replace" } else { "add" };
    let mut path = base_path.to_owned();
    path.push("resources".to_string());
    path.push(resource_kind.to_string());
    path.push("memory".to_string());
    operations.push(json!({
        "op": op,
        "path": json_pointer(&path),
        "value": desired,
    }));
}

#[derive(Debug, Clone)]
struct MemoryProfile {
    limit: String,
    request: String,
}

fn memory_profile_for_map(map: &str, swap_size_gib: u64) -> MemoryProfile {
    match map {
        "Survival_1" => MemoryProfile {
            limit: scaled_gi_profile(20, 12, swap_size_gib),
            request: scaled_gi_profile(20, 5, swap_size_gib),
        },
        "DeepDesert_1" => MemoryProfile {
            limit: "10Gi".to_string(),
            request: scaled_gi_profile(10, 3, swap_size_gib),
        },
        _ => MemoryProfile {
            limit: "1Gi".to_string(),
            request: "200Mi".to_string(),
        },
    }
}

fn scaled_gi_profile(no_swap_gib: u64, vendor_swap_gib: u64, swap_size_gib: u64) -> String {
    const VENDOR_SWAP_GIB: u64 = 30;
    let swap = swap_size_gib.min(VENDOR_SWAP_GIB);
    let delta = no_swap_gib.saturating_sub(vendor_swap_gib);
    let reduction = (delta * swap).div_ceil(VENDOR_SWAP_GIB);
    let value = no_swap_gib.saturating_sub(reduction).max(vendor_swap_gib);
    format!("{value}Gi")
}

fn emit(
    sink: &mut impl OperationSink,
    step_id: &'static str,
    message: &str,
    domain: StepDomain,
    action: StepAction,
) {
    sink.emit(OrchestrationEvent {
        step_id,
        message: message.to_string(),
        domain,
        action,
        provider: ProviderKind::Ssh,
    });
}

fn add_operation(path: &[String], value: Value) -> Value {
    json!({
        "op": "add",
        "path": json_pointer(path),
        "value": value,
    })
}

fn json_pointer(path: &[String]) -> String {
    format!(
        "/{}",
        path.iter()
            .map(|item| item.replace('~', "~0").replace('/', "~1"))
            .collect::<Vec<_>>()
            .join("/")
    )
}

fn sh_single_quoted(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, collections::VecDeque, rc::Rc};

    use super::*;

    #[derive(Clone, Default)]
    struct MockRemote {
        outputs: Rc<RefCell<VecDeque<String>>>,
        scripts: Rc<RefCell<Vec<String>>>,
    }

    impl MockRemote {
        fn with_outputs(outputs: impl IntoIterator<Item = impl Into<String>>) -> Self {
            Self {
                outputs: Rc::new(RefCell::new(outputs.into_iter().map(Into::into).collect())),
                scripts: Rc::new(RefCell::new(Vec::new())),
            }
        }
    }

    impl RemoteCommandRunner for MockRemote {
        fn run(&self, command: &str) -> CommandResult<String> {
            self.run_script(command)
        }

        fn run_script(&self, script: &str) -> CommandResult<String> {
            self.scripts.borrow_mut().push(script.to_string());
            Ok(self.outputs.borrow_mut().pop_front().unwrap_or_default())
        }
    }

    #[test]
    fn patch_sets_experimental_memory_without_jq() {
        let battlegroup = json!({
            "spec": {
                "serverGroup": {
                    "template": {
                        "spec": {
                            "sets": [
                                {
                                    "map": "Survival_1",
                                    "resources": {
                                        "limits": { "memory": "12Gi" },
                                        "requests": { "memory": "12Gi" }
                                    }
                                },
                                {
                                    "map": "DeepDesert_1",
                                    "resources": {
                                        "limits": { "memory": "15Gi" },
                                        "requests": { "memory": "15Gi" }
                                    }
                                },
                                {
                                    "map": "Overmap",
                                    "resources": {
                                        "limits": { "memory": "2Gi" }
                                    }
                                }
                            ]
                        }
                    }
                }
            }
        });

        let operations = experimental_swap_patch_operations(&battlegroup).unwrap();
        let text = serde_json::to_string(&operations).unwrap();

        assert!(text.contains("/spec/serverGroup/template/spec/sets/0/resources/requests/memory"));
        assert!(text.contains("/spec/serverGroup/template/spec/sets/1/resources/limits/memory"));
        assert!(text.contains("/spec/serverGroup/template/spec/sets/2/resources/requests"));
        assert!(text.contains("3Gi"));
        assert!(text.contains("200Mi"));
        assert!(!text.contains("jq"));
    }

    #[test]
    fn smaller_swap_uses_softer_memory_profile() {
        let battlegroup = json!({
            "spec": {
                "serverGroup": {
                    "template": {
                        "spec": {
                            "sets": [
                                {
                                    "map": "Survival_1",
                                    "resources": {
                                        "limits": { "memory": "20Gi" },
                                        "requests": { "memory": "20Gi" }
                                    }
                                },
                                {
                                    "map": "DeepDesert_1",
                                    "resources": {
                                        "limits": { "memory": "10Gi" },
                                        "requests": { "memory": "10Gi" }
                                    }
                                }
                            ]
                        }
                    }
                }
            }
        });

        let operations = experimental_swap_patch_operations_for_swap(&battlegroup, 10).unwrap();
        let text = serde_json::to_string(&operations).unwrap();

        assert!(text.contains("\"17Gi\""));
        assert!(text.contains("\"15Gi\""));
        assert!(text.contains("\"7Gi\""));
        assert!(!text.contains("\"12Gi\""));
        assert!(!text.contains("\"5Gi\""));
    }

    #[test]
    fn matching_profile_needs_no_patch() {
        let battlegroup = json!({
            "spec": {
                "serverGroup": {
                    "template": {
                        "spec": {
                            "sets": [
                                {
                                    "map": "Survival_1",
                                    "resources": {
                                        "limits": { "memory": "12Gi" },
                                        "requests": { "memory": "5Gi" }
                                    }
                                },
                                {
                                    "map": "DeepDesert_1",
                                    "resources": {
                                        "limits": { "memory": "10Gi" },
                                        "requests": { "memory": "3Gi" }
                                    }
                                }
                            ]
                        }
                    }
                }
            }
        });

        assert!(experimental_swap_patch_operations(&battlegroup)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn enable_runs_swap_script_and_battlegroup_patch() {
        let remote = MockRemote::with_outputs([
            "",
            r#"{"metadata":{"name":"bg"},"spec":{"serverGroup":{"template":{"spec":{"sets":[{"map":"DeepDesert_1","resources":{"limits":{"memory":"15Gi"},"requests":{"memory":"15Gi"}}}]}}}}}"#,
            r#"{"metadata":{"name":"bg"}}"#,
            r#"{"swapFileExists":true,"swapActive":true,"swapFileBytes":32212254720,"activeSwapBytes":32212254720,"fstabConfigured":true,"openrcSwapEnabled":true,"kubeletSwapConfigured":true,"battlegroupProfileApplied":null}"#,
            r#"{"metadata":{"name":"bg"},"spec":{"serverGroup":{"template":{"spec":{"sets":[{"map":"DeepDesert_1","resources":{"limits":{"memory":"10Gi"},"requests":{"memory":"3Gi"}}}]}}}}}"#,
        ]);
        let scripts = remote.scripts.clone();
        let mut sink = crate::orchestration::VecOperationSink::default();

        let result = ExperimentalSwapOrchestrator::new(remote)
            .enable(
                &ExperimentalSwapRequest::new("funcom-seabass-sh-host-abcdef", "bg"),
                &mut sink,
            )
            .unwrap();

        assert_eq!(result.battlegroup_patch_operations, 2);
        let scripts = scripts.borrow().join("\n");
        assert!(scripts.contains("dd if=/dev/zero of=/swapfile"));
        assert!(scripts.contains("kubectl patch battlegroup"));
        assert!(sink
            .events
            .iter()
            .any(|event| event.step_id == "bg-swap.patch-memory"));
    }
}
