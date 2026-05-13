use std::{
    collections::HashMap,
    net::{TcpListener, TcpStream},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
};

use dune_manager_core::environment::{detect_setup_environment, SetupEnvironment};
use dune_manager_core::models::{CommandFailure, CommandResult};
use dune_manager_core::orchestration::{
    classify_dune_vm, is_started_state, openssh_base_args, BattlegroupManagementOrchestrator,
    BattlegroupRef, CreatedWorld, DuneVmCandidate, DuneVmConfidence, ExperimentalSwapOrchestrator,
    ExperimentalSwapRequest, GuestBootstrapPlan, GuestBootstrapProvider, GuestNetworkConfig,
    GuestNetworkPlan, HyperVInitialSetupOrchestrator, HyperVInitialSetupRequest,
    HyperVVmLifecycleOrchestrator, HyperVVmSetupRequest, InstanceMap, KubernetesProvider,
    MapInstanceOrchestrator, MemoryProfile, OpenSshGuestProvider, OpenSshRunner, OpenSshTarget,
    OperationSink, OrchestrationEvent, RemoteCommandRunner, SetMapInstancesRequest,
    SshGuestBootstrapProvider, StrictPowerShellHyperV, StructuredKubectl, UbuntuSshPreflight,
    UbuntuSshPrepareRequest, UbuntuSshSetup, UbuntuSwapRequest, VmProvider, WorldManifestRequest,
};
use dune_manager_core::security::redact_text;
use dune_manager_core::shell::{ps_single_quoted, run_powershell};
use dune_manager_core::toolchain::{
    default_server_package_dir, default_vm_destination, prepare_vendor_ssh_key, ManagedTool,
    Toolchain,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{Emitter, Manager};

#[tauri::command]
async fn detect_environment() -> Result<SetupEnvironment, String> {
    tauri::async_runtime::spawn_blocking(detect_setup_environment)
        .await
        .map_err(|err| format!("Environment detection worker failed: {err}"))?
        .map_err(|err| err.message)
}

#[tauri::command]
async fn default_vm_location() -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(|| {
        default_vm_destination()
            .map(|path| path.to_string_lossy().to_string())
            .map_err(|err| err.message)
    })
    .await
    .map_err(|err| format!("Default VM location worker failed: {err}"))?
}

#[tauri::command]
async fn vm_destination_has_vm(path: String) -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if path.trim().is_empty() {
            return Ok(false);
        }
        Ok(destination_has_vm_artifacts(std::path::Path::new(&path)))
    })
    .await
    .map_err(|err| format!("Path check worker failed: {err}"))?
}

#[tauri::command]
async fn register_local_hyperv_server(
    request: LocalHyperVServerRequest,
) -> Result<DuneVmCandidate, String> {
    tauri::async_runtime::spawn_blocking(move || local_hyperv_candidate(&request.vm_name))
        .await
        .map_err(|err| format!("Local Hyper-V registration worker failed: {err}"))?
}

#[tauri::command]
async fn start_local_hyperv_server(
    app: tauri::AppHandle,
    request: LocalHyperVServerRequest,
) -> Result<DuneVmCandidate, String> {
    run_local_hyperv_action(app, request, "start").await
}

#[tauri::command]
async fn stop_local_hyperv_server(
    app: tauri::AppHandle,
    request: LocalHyperVServerRequest,
) -> Result<DuneVmCandidate, String> {
    run_local_hyperv_action(app, request, "stop").await
}

fn destination_has_vm_artifacts(path: &std::path::Path) -> bool {
    if !path.exists() {
        return false;
    }
    if path.join("Virtual Machines").is_dir() || path.join("Virtual Hard Disks").is_dir() {
        return true;
    }
    path.read_dir()
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .any(|entry| {
            let path = entry.path();
            path.extension().is_some_and(|extension| {
                ["vmcx", "vmrs", "vhd", "vhdx"]
                    .iter()
                    .any(|candidate| extension.to_string_lossy().eq_ignore_ascii_case(candidate))
            })
        })
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetupRequest {
    vm_destination: String,
    vm_name: String,
    disk_gb: u64,
    memory_gb: u64,
    processor_count: u32,
    enable_swap: bool,
    network_mode: String,
    switch_name: String,
    adapter_name: String,
    static_ip: String,
    gateway: String,
    dns: String,
    player_ip: String,
    world_name: String,
    region: String,
    self_host_token: String,
    survival_instances: usize,
    deep_desert_pve_instances: usize,
    deep_desert_pvp_instances: usize,
    deep_desert_warm_servers: usize,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemoteSetupRequest {
    host: String,
    user: String,
    key_path: String,
    player_ip: String,
    world_name: String,
    region: String,
    self_host_token: String,
    survival_instances: usize,
    deep_desert_pve_instances: usize,
    deep_desert_pvp_instances: usize,
    deep_desert_warm_servers: usize,
    enable_swap: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemoteConnectionRequest {
    host: String,
    key_path: Option<String>,
    server_type: Option<String>,
    user: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GenerateSshKeyRequest {
    directory: String,
    file_name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct GenerateSshKeyResult {
    private_key_path: String,
    public_key_path: String,
    public_key: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalHyperVServerRequest {
    vm_name: String,
    host: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemoteServerActionRequest {
    server_type: Option<String>,
    host: String,
    user: String,
    key_path: Option<String>,
    namespace: String,
    battlegroup_name: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ServerTunnelStartRequest {
    tunnel_id: String,
    server_kind: String,
    service: String,
    host: String,
    user: Option<String>,
    key_path: Option<String>,
    vm_name: Option<String>,
    namespace: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ServerTunnelStopRequest {
    tunnel_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ServerTunnelStatus {
    tunnel_id: String,
    service: String,
    local_port: u16,
    remote_port: u16,
    url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RemoteBattlegroupStatus {
    stop: bool,
    phase: String,
    server_group_phase: String,
    director_phase: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RemoteServerStatus {
    battlegroup: RemoteBattlegroupStatus,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RemoteServerComponent {
    name: String,
    log_key: String,
    category: String,
    state: String,
    tone: String,
    summary: String,
    details: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemoteComponentLogRequest {
    server_type: Option<String>,
    host: String,
    user: String,
    key_path: Option<String>,
    namespace: String,
    component: String,
    tail: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RemoteComponentLogResult {
    component: String,
    output: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemoteComponentRestartRequest {
    server_type: Option<String>,
    host: String,
    user: String,
    key_path: Option<String>,
    namespace: String,
    component: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RemoteComponentRestartResult {
    component: String,
    output: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LocalHyperVRuntime {
    namespace: String,
    battlegroup_name: String,
    status: RemoteServerStatus,
    components: Vec<RemoteServerComponent>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalHyperVComponentLogRequest {
    vm_name: String,
    host: Option<String>,
    namespace: String,
    component: String,
    tail: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalHyperVBattlegroupActionRequest {
    vm_name: String,
    host: Option<String>,
    namespace: String,
    battlegroup_name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SetupLogPayload {
    level: &'static str,
    scope: String,
    message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SetupRunResult {
    vm_name: String,
    namespace: String,
    battlegroup_name: String,
    world_unique_name: String,
    director_node_port: Option<u16>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RemoteSetupRunResult {
    namespace: String,
    battlegroup_name: String,
    world_unique_name: String,
    preflight: UbuntuSshPreflight,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RemoteServerRecord {
    #[serde(rename = "type")]
    server_type: String,
    id: String,
    name: String,
    host: String,
    user: String,
    key_path: String,
    namespace: String,
    battlegroup_name: String,
    world_unique_name: String,
    phase: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RollbackRequest {
    vm_name: String,
    vm_destination: String,
    switch_name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RollbackResult {
    vm_removed: bool,
    files_removed: bool,
    switch_removed: bool,
}

struct TauriOperationSink {
    app: tauri::AppHandle,
}

#[derive(Default, Clone)]
struct TunnelRegistry {
    tunnels: Arc<Mutex<HashMap<String, ManagedTunnel>>>,
}

struct ManagedTunnel {
    child: Child,
    status: ServerTunnelStatus,
}

impl TunnelRegistry {
    fn stop_all(&self) {
        let Ok(mut tunnels) = self.tunnels.lock() else {
            return;
        };
        for (_, mut tunnel) in tunnels.drain() {
            let _ = tunnel.child.kill();
            let _ = tunnel.child.wait();
        }
    }
}

impl TauriOperationSink {
    fn info(&self, scope: impl Into<String>, message: impl Into<String>) {
        let _ = self.app.emit(
            "setup-log",
            SetupLogPayload {
                level: "info",
                scope: scope.into(),
                message: message.into(),
            },
        );
    }

    fn error(&self, scope: impl Into<String>, message: impl Into<String>) {
        let _ = self.app.emit(
            "setup-log",
            SetupLogPayload {
                level: "error",
                scope: scope.into(),
                message: message.into(),
            },
        );
    }

    fn warn(&self, scope: impl Into<String>, message: impl Into<String>) {
        let _ = self.app.emit(
            "setup-log",
            SetupLogPayload {
                level: "warn",
                scope: scope.into(),
                message: message.into(),
            },
        );
    }
}

impl OperationSink for TauriOperationSink {
    fn emit(&mut self, event: OrchestrationEvent) {
        self.info(event.step_id, event.message);
    }
}

#[tauri::command]
async fn start_full_setup(
    app: tauri::AppHandle,
    request: SetupRequest,
) -> Result<SetupRunResult, String> {
    let worker_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut sink = TauriOperationSink { app: worker_app };
        sink.info("setup", "Starting full setup workflow.");
        match run_full_setup(request, &mut sink) {
            Ok(result) => {
                sink.info("setup", "Full setup workflow completed.");
                Ok(result)
            }
            Err(err) => {
                sink.error("setup", err.message.clone());
                if !err.stderr.trim().is_empty() {
                    sink.error("stderr", err.stderr);
                }
                if !err.stdout.trim().is_empty() {
                    sink.error("stdout", err.stdout);
                }
                Err("Setup failed; see setup log for details.".to_string())
            }
        }
    })
    .await
    .map_err(|err| format!("Setup worker failed: {err}"))?
}

#[tauri::command]
async fn preflight_remote_ubuntu(
    request: RemoteSetupRequest,
) -> Result<UbuntuSshPreflight, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let toolchain = Toolchain::from_default_root().map_err(|err| err.message)?;
        toolchain
            .install(ManagedTool::OpenSsh, false, None)
            .map_err(|err| err.message)?;
        let ssh_path = toolchain.status(ManagedTool::OpenSsh).executable;
        let runner = OpenSshRunner::new(OpenSshTarget::new(
            ssh_path,
            PathBuf::from(request.key_path),
            request.user,
            request.host,
        ));
        UbuntuSshSetup::new(runner)
            .preflight()
            .map_err(|err| err.message)
    })
    .await
    .map_err(|err| format!("Remote preflight worker failed: {err}"))?
}

#[tauri::command]
async fn detect_remote_ubuntu_servers(
    request: RemoteConnectionRequest,
) -> Result<Vec<RemoteServerRecord>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let request = RemoteConnectionRequest {
            server_type: Some("ubuntu".to_string()),
            user: Some("root".to_string()),
            ..request
        };
        let runner = runner_for_remote_kind(
            request.server_type.as_deref(),
            request.host.clone(),
            request.user.as_deref().unwrap_or("root").to_string(),
            request.key_path.clone(),
        )?;
        let value = runner
            .run_json(
                "sudo kubectl get battlegroups -A -o json",
                "remote ubuntu battlegroups",
            )
            .map_err(command_error_message)?;
        Ok(remote_records_from_battlegroups(&request, &value))
    })
    .await
    .map_err(|err| format!("Remote server detection worker failed: {err}"))?
}

#[tauri::command]
async fn detect_remote_alpine_servers(
    request: RemoteConnectionRequest,
) -> Result<Vec<RemoteServerRecord>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let request = RemoteConnectionRequest {
            server_type: Some("alpine".to_string()),
            user: Some("dune".to_string()),
            key_path: None,
            ..request
        };
        let runner = runner_for_remote_kind(
            request.server_type.as_deref(),
            request.host.clone(),
            "dune".to_string(),
            None,
        )?;
        let value = runner
            .run_json(
                "sudo kubectl get battlegroups -A -o json",
                "remote alpine guest battlegroups",
            )
            .map_err(command_error_message)?;
        Ok(remote_records_from_battlegroups(&request, &value))
    })
    .await
    .map_err(|err| format!("Remote Alpine guest detection worker failed: {err}"))?
}

#[tauri::command]
async fn generate_ubuntu_ssh_key(
    request: GenerateSshKeyRequest,
) -> Result<GenerateSshKeyResult, String> {
    tauri::async_runtime::spawn_blocking(move || generate_ubuntu_ssh_key_inner(request))
        .await
        .map_err(|err| format!("SSH key generation worker failed: {err}"))?
}

#[tauri::command]
async fn remote_server_status(
    request: RemoteServerActionRequest,
) -> Result<RemoteServerStatus, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let runner = runner_for_remote_kind(
            request.server_type.as_deref(),
            request.host,
            request.user,
            request.key_path,
        )?;
        read_remote_server_status(&runner, &request.namespace, &request.battlegroup_name)
            .map_err(command_error_message)
    })
    .await
    .map_err(|err| format!("Remote status worker failed: {err}"))?
}

#[tauri::command]
async fn remote_server_components(
    request: RemoteServerActionRequest,
) -> Result<Vec<RemoteServerComponent>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let runner = runner_for_remote_kind(
            request.server_type.as_deref(),
            request.host,
            request.user,
            request.key_path,
        )?;
        read_remote_server_components(&runner, &request.namespace).map_err(command_error_message)
    })
    .await
    .map_err(|err| format!("Remote component diagnostics worker failed: {err}"))?
}

#[tauri::command]
async fn local_hyperv_runtime(
    request: LocalHyperVServerRequest,
) -> Result<LocalHyperVRuntime, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let runner = local_hyperv_runner(&request.vm_name, request.host.as_deref())?;
        let value = runner
            .run_json(
                "sudo kubectl get battlegroups -A -o json",
                "local hyperv battlegroups",
            )
            .map_err(command_error_message)?;
        let connection = RemoteConnectionRequest {
            host: request.vm_name.clone(),
            key_path: None,
            server_type: Some("hyperv".to_string()),
            user: Some("dune".to_string()),
        };
        let records = remote_records_from_battlegroups(&connection, &value);
        let Some(record) = records.first() else {
            return Err("No Dune battlegroups were detected inside the VM.".to_string());
        };
        let status =
            read_remote_server_status(&runner, &record.namespace, &record.battlegroup_name)
                .map_err(command_error_message)?;
        let components = read_remote_server_components(&runner, &record.namespace)
            .map_err(command_error_message)?;
        Ok(LocalHyperVRuntime {
            namespace: record.namespace.clone(),
            battlegroup_name: record.battlegroup_name.clone(),
            status,
            components,
        })
    })
    .await
    .map_err(|err| format!("Local Hyper-V runtime worker failed: {err}"))?
}

#[tauri::command]
async fn start_server_tunnel(
    registry: tauri::State<'_, TunnelRegistry>,
    request: ServerTunnelStartRequest,
) -> Result<ServerTunnelStatus, String> {
    let registry = registry.inner().clone();
    tauri::async_runtime::spawn_blocking(move || start_server_tunnel_inner(&registry, request))
        .await
        .map_err(|err| format!("Tunnel worker failed: {err}"))?
}

#[tauri::command]
async fn stop_server_tunnel(
    registry: tauri::State<'_, TunnelRegistry>,
    request: ServerTunnelStopRequest,
) -> Result<(), String> {
    let registry = registry.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        stop_server_tunnel_inner(&registry, &request.tunnel_id)
    })
    .await
    .map_err(|err| format!("Tunnel stop worker failed: {err}"))?
}

#[tauri::command]
async fn server_tunnel_status(
    registry: tauri::State<'_, TunnelRegistry>,
    request: ServerTunnelStopRequest,
) -> Result<Option<ServerTunnelStatus>, String> {
    let registry = registry.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        existing_running_tunnel(&registry, request.tunnel_id.trim())
    })
    .await
    .map_err(|err| format!("Tunnel status worker failed: {err}"))?
}

#[tauri::command]
async fn stop_all_tunnels(registry: tauri::State<'_, TunnelRegistry>) -> Result<(), String> {
    registry.stop_all();
    Ok(())
}

#[tauri::command]
async fn remote_component_log_tail(
    request: RemoteComponentLogRequest,
) -> Result<RemoteComponentLogResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let runner = runner_for_remote_kind(
            request.server_type.as_deref(),
            request.host,
            request.user,
            request.key_path,
        )?;
        read_remote_component_log_tail(
            &runner,
            &request.namespace,
            &request.component,
            request.tail,
        )
        .map_err(command_error_message)
    })
    .await
    .map_err(|err| format!("Remote component log worker failed: {err}"))?
}

#[tauri::command]
async fn local_hyperv_component_log_tail(
    request: LocalHyperVComponentLogRequest,
) -> Result<RemoteComponentLogResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let runner = local_hyperv_runner(&request.vm_name, request.host.as_deref())?;
        read_remote_component_log_tail(
            &runner,
            &request.namespace,
            &request.component,
            request.tail,
        )
        .map_err(command_error_message)
    })
    .await
    .map_err(|err| format!("Local Hyper-V component log worker failed: {err}"))?
}

#[tauri::command]
async fn restart_remote_component(
    request: RemoteComponentRestartRequest,
) -> Result<RemoteComponentRestartResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let runner = runner_for_remote_kind(
            request.server_type.as_deref(),
            request.host,
            request.user,
            request.key_path,
        )?;
        restart_remote_component_inner(&runner, &request.namespace, &request.component)
            .map_err(command_error_message)
    })
    .await
    .map_err(|err| format!("Remote component restart worker failed: {err}"))?
}

#[tauri::command]
async fn restart_local_hyperv_component(
    request: LocalHyperVComponentLogRequest,
) -> Result<RemoteComponentRestartResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let runner = local_hyperv_runner(&request.vm_name, request.host.as_deref())?;
        restart_remote_component_inner(&runner, &request.namespace, &request.component)
            .map_err(command_error_message)
    })
    .await
    .map_err(|err| format!("Local Hyper-V component restart worker failed: {err}"))?
}

#[tauri::command]
async fn start_remote_battlegroup(
    app: tauri::AppHandle,
    request: RemoteServerActionRequest,
) -> Result<RemoteServerStatus, String> {
    run_remote_battlegroup_action(app, request, false).await
}

#[tauri::command]
async fn stop_remote_battlegroup(
    app: tauri::AppHandle,
    request: RemoteServerActionRequest,
) -> Result<RemoteServerStatus, String> {
    run_remote_battlegroup_action(app, request, true).await
}

#[tauri::command]
async fn start_local_hyperv_battlegroup(
    app: tauri::AppHandle,
    request: LocalHyperVBattlegroupActionRequest,
) -> Result<RemoteServerStatus, String> {
    run_local_hyperv_battlegroup_action(app, request, false).await
}

#[tauri::command]
async fn stop_local_hyperv_battlegroup(
    app: tauri::AppHandle,
    request: LocalHyperVBattlegroupActionRequest,
) -> Result<RemoteServerStatus, String> {
    run_local_hyperv_battlegroup_action(app, request, true).await
}

async fn run_local_hyperv_action(
    app: tauri::AppHandle,
    request: LocalHyperVServerRequest,
    action: &'static str,
) -> Result<DuneVmCandidate, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut sink = TauriOperationSink { app };
        let provider = StrictPowerShellHyperV::new();
        let lifecycle = HyperVVmLifecycleOrchestrator::new(&provider);
        match action {
            "start" => lifecycle
                .start(&request.vm_name, &mut sink)
                .map_err(|err| err.message)?,
            "stop" => lifecycle
                .stop(&request.vm_name, &mut sink)
                .map_err(|err| err.message)?,
            _ => unreachable!("unsupported local Hyper-V action"),
        }
        local_hyperv_candidate(&request.vm_name)
    })
    .await
    .map_err(|err| format!("Local Hyper-V action worker failed: {err}"))?
}

async fn run_local_hyperv_battlegroup_action(
    app: tauri::AppHandle,
    request: LocalHyperVBattlegroupActionRequest,
    stop: bool,
) -> Result<RemoteServerStatus, String> {
    let worker_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut sink = TauriOperationSink { app: worker_app };
        sink.info("bg.check", "Checking local Hyper-V battlegroup state.");
        let runner = local_hyperv_runner(&request.vm_name, request.host.as_deref())?;
        run_battlegroup_action_with_runner(
            &runner,
            &mut sink,
            request.namespace,
            request.battlegroup_name,
            stop,
        )
    })
    .await
    .map_err(|err| format!("Local Hyper-V battlegroup action worker failed: {err}"))?
}

async fn run_remote_battlegroup_action(
    app: tauri::AppHandle,
    request: RemoteServerActionRequest,
    stop: bool,
) -> Result<RemoteServerStatus, String> {
    let worker_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut sink = TauriOperationSink { app: worker_app };
        sink.info("bg.check", "Checking remote battlegroup state.");
        let runner = runner_for_remote_kind(
            request.server_type.as_deref(),
            request.host,
            request.user,
            request.key_path,
        )?;
        run_battlegroup_action_with_runner(
            &runner,
            &mut sink,
            request.namespace,
            request.battlegroup_name,
            stop,
        )
    })
    .await
    .map_err(|err| format!("Remote battlegroup action worker failed: {err}"))?
}

fn run_battlegroup_action_with_runner(
    runner: &OpenSshRunner,
    sink: &mut TauriOperationSink,
    namespace: String,
    battlegroup_name: String,
    stop: bool,
) -> Result<RemoteServerStatus, String> {
    let kubernetes = StructuredKubectl::new(runner.clone());
    let before = kubernetes
        .battlegroup_state(&namespace, &battlegroup_name)
        .map_err(command_error_message)?;
    let before_started = is_started_state(&before);
    if stop && !before_started {
        return Err(format!(
            "Battlegroup is not running (phase={}, stop={}, serverGroup={}, director={}).",
            before.phase, before.stop, before.server_group_phase, before.director_phase
        ));
    }
    if !stop && before_started {
        return Err("Battlegroup is already started.".to_string());
    }
    let battlegroup = BattlegroupRef {
        namespace,
        name: battlegroup_name,
    };
    let manager = BattlegroupManagementOrchestrator::new(kubernetes);
    if stop {
        manager
            .stop(&battlegroup, sink)
            .map_err(command_error_message)?;
    } else {
        manager
            .start_and_wait_director(&battlegroup, 180, sink)
            .map_err(command_error_message)?;
    }
    sink.info("bg.check", "Refreshing battlegroup state.");
    read_remote_server_status(runner, &battlegroup.namespace, &battlegroup.name)
        .map_err(command_error_message)
}

#[tauri::command]
async fn start_remote_ubuntu_setup(
    app: tauri::AppHandle,
    request: RemoteSetupRequest,
) -> Result<RemoteSetupRunResult, String> {
    let worker_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut sink = TauriOperationSink { app: worker_app };
        sink.warn(
            "ubuntu",
            "Remote Ubuntu setup can modify packages, users, k3s, firewall state, and server files on the target host.",
        );
        match run_remote_ubuntu_setup(request, &mut sink) {
            Ok(result) => {
                sink.info("ubuntu", "Remote Ubuntu setup completed.");
                Ok(result)
            }
            Err(err) => {
                sink.error("ubuntu", err.message.clone());
                if !err.stderr.trim().is_empty() {
                    sink.error("stderr", err.stderr);
                }
                if !err.stdout.trim().is_empty() {
                    sink.error("stdout", err.stdout);
                }
                Err("Remote Ubuntu setup failed; see setup log for details.".to_string())
            }
        }
    })
    .await
    .map_err(|err| format!("Remote Ubuntu setup worker failed: {err}"))?
}

#[tauri::command]
async fn rollback_setup(
    app: tauri::AppHandle,
    request: RollbackRequest,
) -> Result<RollbackResult, String> {
    let worker_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let sink = TauriOperationSink { app: worker_app };
        sink.warn("rollback", "Rolling back setup artifacts.");
        rollback_setup_inner(request, &sink).map_err(|err| {
            sink.error("rollback", err.message.clone());
            err.message
        })
    })
    .await
    .map_err(|err| format!("Rollback worker failed: {err}"))?
}

fn rollback_setup_inner(
    request: RollbackRequest,
    sink: &TauriOperationSink,
) -> CommandResult<RollbackResult> {
    let provider = StrictPowerShellHyperV::new();
    let mut vm_removed = false;
    if provider.get_vm(&request.vm_name)?.is_some() {
        sink.warn(
            "rollback",
            format!("Removing Hyper-V VM '{}'.", request.vm_name),
        );
        provider.remove_vm(&request.vm_name)?;
        vm_removed = true;
    }

    let destination = PathBuf::from(&request.vm_destination);
    let mut files_removed = false;
    if destination.exists()
        && (destination_has_vm_artifacts(&destination) || is_empty_dir(&destination))
    {
        sink.warn(
            "rollback",
            format!("Removing VM files at {}.", destination.display()),
        );
        std::fs::remove_dir_all(&destination).map_err(|err| {
            dune_manager_core::errors::failure(format!(
                "Failed to remove VM files at {}: {err}",
                destination.display()
            ))
        })?;
        files_removed = true;
    }

    let switch_removed = remove_switch_if_unused(&request.switch_name)?;
    if switch_removed {
        sink.warn(
            "rollback",
            format!("Removed unused Hyper-V switch '{}'.", request.switch_name),
        );
    } else {
        sink.info(
            "rollback",
            format!(
                "Hyper-V switch '{}' was left in place because it is missing or still used.",
                request.switch_name
            ),
        );
    }

    Ok(RollbackResult {
        vm_removed,
        files_removed,
        switch_removed,
    })
}

fn run_full_setup(
    request: SetupRequest,
    sink: &mut TauriOperationSink,
) -> CommandResult<SetupRunResult> {
    let toolchain = Toolchain::from_default_root()?;
    let server_package_dir = default_server_package_dir()?;
    let provider = StrictPowerShellHyperV::new();
    let vm_destination = PathBuf::from(&request.vm_destination);

    sink.info("setup", "Checking VM destination and existing VM state.");
    if destination_has_vm_artifacts(&vm_destination) {
        return Err(dune_manager_core::errors::failure(format!(
            "VM Location already contains VM files: {}. Choose another destination or remove the existing VM files first.",
            vm_destination.display()
        )));
    }
    if provider.get_vm(&request.vm_name)?.is_some() {
        return Err(dune_manager_core::errors::failure(format!(
            "A Hyper-V VM named '{}' already exists. Remove it before setup.",
            request.vm_name
        )));
    }

    sink.info("tools", "Installing or validating SteamCMD.");
    toolchain.install(ManagedTool::SteamCmd, false, None)?;
    sink.info("tools", "Installing or validating OpenSSH.");
    toolchain.install(ManagedTool::OpenSsh, false, None)?;

    sink.info("steam", "Installing or validating the server package.");
    toolchain.install_server_package(&server_package_dir)?;

    sink.info("ssh", "Preparing the vendor VM SSH key.");
    let ssh_key = prepare_vendor_ssh_key(&server_package_dir)?;
    let ssh_path = toolchain.status(ManagedTool::OpenSsh).executable;

    if !request.network_mode.eq_ignore_ascii_case("static") {
        return Err(dune_manager_core::errors::failure(
            "Full setup currently requires Static internal IP mode so bootstrap can continue after the first boot",
        ));
    }

    let environment = detect_setup_environment()?;
    let adapter = environment
        .network_adapters
        .iter()
        .find(|adapter| adapter.name == request.adapter_name)
        .ok_or_else(|| {
            dune_manager_core::errors::failure(
                "Selected network adapter was not found in the current host environment",
            )
        })?;

    let address_cidr = format!("{}/{}", request.static_ip.trim(), adapter.prefix_length);
    let guest_network = GuestNetworkPlan::Static(GuestNetworkConfig {
        interface: "eth0".to_string(),
        address_cidr,
        gateway: request.gateway.clone(),
        dns: request.dns.clone(),
    });

    let guest_plan = GuestBootstrapPlan::from_self_host_token(
        request.player_ip.clone(),
        request.world_name.clone(),
        request.region.clone(),
        request.self_host_token.clone(),
    )?;
    let bootstrap_target = OpenSshTarget::new(
        ssh_path.clone(),
        ssh_key.clone(),
        "dune",
        request.static_ip.clone(),
    );
    let bootstrap_runner = OpenSshRunner::new(bootstrap_target.clone());
    let initial = HyperVInitialSetupRequest {
        vm: HyperVVmSetupRequest {
            install_path: server_package_dir.clone(),
            vm_name: request.vm_name.clone(),
            destination_path: vm_destination,
            switch_name: request.switch_name.clone(),
            adapter_name: request.adapter_name.clone(),
            memory: MemoryProfile::CustomBytes(
                request.memory_gb.saturating_mul(1024 * 1024 * 1024),
            ),
            processor_count: request.processor_count,
            replace_existing_vm: false,
            clear_destination: false,
            disk_size_bytes: request.disk_gb.saturating_mul(1024 * 1024 * 1024),
        },
        guest_network,
        guest_bootstrap: guest_plan,
        vm_ip_timeout_seconds: 180,
        ssh_timeout_seconds: 180,
    };

    let result = HyperVInitialSetupOrchestrator::new(
        &provider,
        &provider,
        OpenSshGuestProvider::new(ssh_path, ssh_key, "dune"),
        SshGuestBootstrapProvider::new(bootstrap_runner.clone()),
    )
    .run(&initial, sink)?;

    wait_for_database_ready(&bootstrap_runner, &result.bootstrap.namespace, 900, sink)?;

    let layout_changed = apply_instance_layout(
        &request,
        &result.bootstrap.namespace,
        &result.bootstrap.battlegroup_name,
        &bootstrap_runner,
        sink,
    )?;

    if layout_changed {
        wait_for_database_ready(&bootstrap_runner, &result.bootstrap.namespace, 900, sink)?;
    }

    if request.enable_swap {
        sink.info("guest-swap", "Enabling experimental swap profile.");
        let mut swap = ExperimentalSwapRequest::new(
            result.bootstrap.namespace.clone(),
            result.bootstrap.battlegroup_name.clone(),
        );
        swap.restart_k3s = true;
        ExperimentalSwapOrchestrator::new(bootstrap_runner.clone()).enable(&swap, sink)?;
    }

    sink.info(
        "setup",
        "Initial setup complete. The battlegroup is provisioned but not started.",
    );
    Ok(SetupRunResult {
        vm_name: result.vm.vm_name,
        namespace: result.bootstrap.namespace,
        battlegroup_name: result.bootstrap.battlegroup_name,
        world_unique_name: result.bootstrap.world_unique_name,
        director_node_port: None,
    })
}

fn run_remote_ubuntu_setup(
    request: RemoteSetupRequest,
    sink: &mut TauriOperationSink,
) -> CommandResult<RemoteSetupRunResult> {
    let toolchain = Toolchain::from_default_root()?;
    toolchain.install(ManagedTool::OpenSsh, false, None)?;
    let ssh_path = toolchain.status(ManagedTool::OpenSsh).executable;
    let target = OpenSshTarget::new(
        ssh_path,
        PathBuf::from(&request.key_path),
        request.user.clone(),
        request.host.clone(),
    );
    let runner = OpenSshRunner::new(target);
    let ubuntu = UbuntuSshSetup::new(runner.clone());
    let prepare = UbuntuSshPrepareRequest::default();

    sink.info("ubuntu.preflight", "Checking remote Ubuntu host resources.");
    let preflight = ubuntu.preflight()?;
    if !preflight.passwordless_sudo && preflight.uid != 0 {
        return Err(dune_manager_core::errors::failure(
            "Remote setup requires root login or passwordless sudo.",
        ));
    }

    ubuntu.prepare_host(&prepare, sink)?;
    if request.enable_swap {
        let swap_size_gib = recommended_ubuntu_swap_gib(&preflight, &request);
        let required_gib = required_layout_memory_gib(
            request.survival_instances,
            request.deep_desert_pve_instances + request.deep_desert_pvp_instances,
        );
        sink.warn(
            "ubuntu.swap.native",
            format!(
                "Creating {swap_size_gib} GiB native Ubuntu swap. This can help memory pressure but may reduce performance; selected layout recommends {required_gib} GiB RAM."
            ),
        );
        let swap = ubuntu.configure_swap(&UbuntuSwapRequest::new(swap_size_gib), sink)?;
        if !swap.swap_active || !swap.kubelet_swap_configured {
            return Err(dune_manager_core::errors::failure(
                "Ubuntu swap was requested but did not become fully active/configured.",
            ));
        }
    }
    ubuntu.install_k3s(&prepare, sink)?;
    let payload = ubuntu.install_server_payload(&prepare, sink)?;
    if !payload.setup_script_present || !payload.battlegroup_script_present {
        return Err(dune_manager_core::errors::failure(format!(
            "Dune server payload is incomplete at {} (setup script present: {}, battlegroup script present: {}).",
            payload.download_path, payload.setup_script_present, payload.battlegroup_script_present
        )));
    }
    ubuntu.bootstrap_kubernetes(&prepare, sink)?;

    let plan = GuestBootstrapPlan::from_self_host_token(
        request.player_ip.clone(),
        request.world_name.clone(),
        request.region.clone(),
        request.self_host_token.clone(),
    )?;
    plan.validate()?;
    ubuntu.write_player_settings(&plan.player_ip, sink)?;

    let provider = SshGuestBootstrapProvider::new(runner.clone());
    let world_request = WorldManifestRequest {
        world_name: plan.world_name.clone(),
        world_region: plan.world_region.clone(),
        world_unique_name: plan.world_unique_name(),
        self_host_token: plan.self_host_token.clone(),
    };
    let created = ensure_remote_world(&provider, &runner, &world_request, sink)?;
    sink.info("ubuntu.helper.install", "Installing battlegroup helper.");
    provider.install_battlegroup_helper()?;
    sink.info(
        "ubuntu.images.import",
        "Importing BattleGroup container images.",
    );
    provider.import_battlegroup_images()?;
    sink.info("ubuntu.images.patch", "Patching BattleGroup image tags.");
    provider.patch_battlegroup_images(&created.namespace, &created.battlegroup_name)?;
    ubuntu.use_default_scheduler(&created.namespace, &created.battlegroup_name, sink)?;
    sink.info("ubuntu.defaults.apply", "Applying default user settings.");
    provider.apply_default_user_settings(&created.namespace, &created.battlegroup_name)?;

    let layout_request = SetupRequest {
        vm_destination: String::new(),
        vm_name: String::new(),
        disk_gb: 0,
        memory_gb: 0,
        processor_count: 0,
        enable_swap: request.enable_swap,
        network_mode: "static".to_string(),
        switch_name: String::new(),
        adapter_name: String::new(),
        static_ip: String::new(),
        gateway: String::new(),
        dns: String::new(),
        player_ip: request.player_ip.clone(),
        world_name: request.world_name.clone(),
        region: request.region.clone(),
        self_host_token: request.self_host_token.clone(),
        survival_instances: request.survival_instances,
        deep_desert_pve_instances: request.deep_desert_pve_instances,
        deep_desert_pvp_instances: request.deep_desert_pvp_instances,
        deep_desert_warm_servers: request.deep_desert_warm_servers,
    };
    apply_instance_layout(
        &layout_request,
        &created.namespace,
        &created.battlegroup_name,
        &runner,
        sink,
    )?;

    wait_for_database_ready(&runner, &created.namespace, 900, sink)?;

    let battlegroup = BattlegroupRef {
        namespace: created.namespace.clone(),
        name: created.battlegroup_name.clone(),
    };
    sink.info("bg", "Starting battlegroup after remote setup.");
    let _director_node_port =
        BattlegroupManagementOrchestrator::new(StructuredKubectl::new(runner.clone()))
            .start_and_wait_director(&battlegroup, 180, sink)?;

    Ok(RemoteSetupRunResult {
        namespace: created.namespace,
        battlegroup_name: created.battlegroup_name,
        world_unique_name: plan.world_unique_name(),
        preflight,
    })
}

fn wait_for_database_ready<R: RemoteCommandRunner>(
    runner: &R,
    namespace: &str,
    timeout_seconds: u64,
    sink: &TauriOperationSink,
) -> CommandResult<()> {
    sink.info(
        "database.wait",
        "Waiting for database schema initialization to complete.",
    );
    let script = format!(
        r#"
set -eu
ns={ns}
timeout={timeout}
elapsed=0
last_phase=""
while [ "$elapsed" -le "$timeout" ]; do
  phases=$(sudo kubectl get databasedeployments -n "$ns" -o jsonpath='{{range .items[*]}}{{.status.phase}}{{"\n"}}{{end}}' 2>/dev/null || true)
  if [ -n "$phases" ]; then
    last_phase=$(printf '%s' "$phases" | tr '\n' ',' | sed 's/,$//')
    if printf '%s\n' "$phases" | grep -Eq '^(Ready|Healthy|Running|Succeeded)$' &&
       ! printf '%s\n' "$phases" | grep -Eq '^(|Pending|Failed|Error)$'; then
      echo "Database ready: $last_phase"
      exit 0
    fi
  fi
  sleep 5
  elapsed=$((elapsed + 5))
done
echo "Database did not become ready within $timeout seconds. Last phase: ${{last_phase:-unknown}}" >&2
failed_pod=$(sudo kubectl get pods -n "$ns" --no-headers 2>/dev/null | awk '/db.*util/ && ($3 == "Error" || $3 == "Failed" || $3 == "CrashLoopBackOff") {{print $1; exit}}' || true)
if [ -n "$failed_pod" ]; then
  echo "Last failed database schema utility pod: $failed_pod" >&2
  sudo kubectl logs -n "$ns" "$failed_pod" --tail=120 2>&1 |
    sed -E 's/(password|token|secret|auth|key)([^[:space:]]*)[=:][^[:space:]]+/\1\2=<redacted>/Ig' >&2 || true
fi
exit 1
"#,
        ns = sh_single_quoted(namespace),
        timeout = timeout_seconds
    );
    runner.run_script(&script).map(|_| ())
}

fn remote_records_from_battlegroups(
    request: &RemoteConnectionRequest,
    value: &Value,
) -> Vec<RemoteServerRecord> {
    value
        .get("items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| remote_record_from_battlegroup(request, item))
        .collect()
}

fn remote_record_from_battlegroup(
    request: &RemoteConnectionRequest,
    item: &Value,
) -> Option<RemoteServerRecord> {
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
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            if is_alpine_remote_kind(&server_type) {
                "dune"
            } else {
                "root"
            }
        })
        .to_string();
    Some(RemoteServerRecord {
        id: remote_record_id(&server_type, &request.host, request.key_path.as_deref()),
        name: title,
        host: request.host.clone(),
        user,
        key_path: request.key_path.clone().unwrap_or_default(),
        server_type,
        namespace,
        battlegroup_name: battlegroup_name.clone(),
        world_unique_name: battlegroup_name,
        phase,
    })
}

fn remote_record_id(server_type: &str, host: &str, key_path: Option<&str>) -> String {
    if is_alpine_remote_kind(server_type) {
        format!("alpine:{}", host.trim().to_lowercase())
    } else {
        format!(
            "ubuntu:{}:{}",
            host.trim().to_lowercase(),
            key_path.unwrap_or_default().trim().to_lowercase()
        )
    }
}

fn ensure_remote_world<R>(
    provider: &SshGuestBootstrapProvider<R>,
    runner: &R,
    request: &WorldManifestRequest,
    sink: &TauriOperationSink,
) -> CommandResult<CreatedWorld>
where
    R: RemoteCommandRunner + Clone,
{
    let namespace = format!("funcom-seabass-{}", request.world_unique_name);
    let battlegroup_name = request.world_unique_name.clone();
    let existing = StructuredKubectl::new(runner.clone())
        .battlegroup_state(&namespace, &battlegroup_name)
        .is_ok();
    if existing {
        sink.warn(
            "ubuntu.world.create",
            format!("Existing BattleGroup {namespace}/{battlegroup_name} found; resuming setup."),
        );
        return Ok(CreatedWorld {
            namespace,
            battlegroup_name,
        });
    }

    sink.info(
        "ubuntu.world.create",
        "Creating BattleGroup world resources.",
    );
    provider.create_world(request)
}

fn remote_runner(host: String, user: String, key_path: String) -> Result<OpenSshRunner, String> {
    let toolchain = Toolchain::from_default_root().map_err(|err| err.message)?;
    toolchain
        .install(ManagedTool::OpenSsh, false, None)
        .map_err(|err| err.message)?;
    let ssh_path = toolchain.status(ManagedTool::OpenSsh).executable;
    Ok(OpenSshRunner::new(OpenSshTarget::new(
        ssh_path,
        PathBuf::from(key_path),
        user,
        host,
    )))
}

fn runner_for_remote_kind(
    server_type: Option<&str>,
    host: String,
    user: String,
    key_path: Option<String>,
) -> Result<OpenSshRunner, String> {
    if is_alpine_remote_kind(server_type.unwrap_or_default()) {
        return alpine_guest_runner(host);
    }
    let key_path = key_path
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "SSH private key is required for remote Ubuntu servers.".to_string())?;
    remote_runner(host, user, key_path)
}

fn alpine_guest_runner(host: String) -> Result<OpenSshRunner, String> {
    let host = host.trim().to_string();
    if host.is_empty() {
        return Err("Remote Alpine guest IP is required.".to_string());
    }
    let toolchain = Toolchain::from_default_root().map_err(|err| err.message)?;
    toolchain
        .install(ManagedTool::OpenSsh, false, None)
        .map_err(|err| err.message)?;
    let ssh_path = toolchain.status(ManagedTool::OpenSsh).executable;
    let server_package_dir = default_server_package_dir().map_err(|err| err.message)?;
    let key_path = prepare_vendor_ssh_key(&server_package_dir).map_err(command_error_message)?;
    Ok(OpenSshRunner::new(OpenSshTarget::new(
        ssh_path, key_path, "dune", host,
    )))
}

fn is_alpine_remote_kind(server_type: &str) -> bool {
    matches!(server_type.trim(), "alpine" | "remoteHyperv")
}

fn start_server_tunnel_inner(
    registry: &TunnelRegistry,
    request: ServerTunnelStartRequest,
) -> Result<ServerTunnelStatus, String> {
    let tunnel_id = request.tunnel_id.trim();
    if tunnel_id.is_empty() {
        return Err("Tunnel id is required.".to_string());
    }
    if let Some(status) = existing_running_tunnel(registry, tunnel_id)? {
        return Ok(status);
    }

    let target = tunnel_target(&request)?;
    target.validate().map_err(|err| err.message)?;
    let service = normalize_tunnel_service(&request.service)?;
    let remote_port = match service.as_str() {
        "director" => discover_director_tunnel_port(&target, &request.namespace)?,
        "fileBrowser" => 18888,
        _ => unreachable!(),
    };
    let local_port = pick_available_local_port()?;
    if !is_local_port_available(local_port) {
        return Err(format!("Local port {local_port} is already in use."));
    }

    let mut command = Command::new(&target.ssh_path);
    let mut args = openssh_base_args(&target);
    args.extend([
        "-o".to_string(),
        "ExitOnForwardFailure=yes".to_string(),
        "-N".to_string(),
        "-L".to_string(),
        format!("127.0.0.1:{local_port}:127.0.0.1:{remote_port}"),
        target.destination(),
    ]);
    command.args(args);
    command.stdin(Stdio::null());
    command.stdout(Stdio::null());
    command.stderr(Stdio::null());
    dune_manager_core::shell::suppress_console_window(&mut command);

    let mut child = command
        .spawn()
        .map_err(|err| format!("Failed to start SSH tunnel: {err}"))?;
    std::thread::sleep(std::time::Duration::from_millis(700));
    if let Some(status) = child
        .try_wait()
        .map_err(|err| format!("Failed to inspect SSH tunnel: {err}"))?
    {
        return Err(format!(
            "SSH tunnel exited immediately with status {status}."
        ));
    }
    if !wait_for_local_tunnel(local_port, std::time::Duration::from_secs(3)) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(format!(
            "SSH tunnel did not start listening on local port {local_port}."
        ));
    }

    let status = ServerTunnelStatus {
        tunnel_id: tunnel_id.to_string(),
        service,
        local_port,
        remote_port,
        url: format!("http://127.0.0.1:{local_port}/"),
    };
    let mut tunnels = registry
        .tunnels
        .lock()
        .map_err(|_| "Tunnel registry is unavailable.".to_string())?;
    if let Some(mut existing) = tunnels.remove(tunnel_id) {
        let _ = existing.child.kill();
        let _ = existing.child.wait();
    }
    tunnels.insert(
        tunnel_id.to_string(),
        ManagedTunnel {
            child,
            status: status.clone(),
        },
    );
    Ok(status)
}

fn stop_server_tunnel_inner(registry: &TunnelRegistry, tunnel_id: &str) -> Result<(), String> {
    let mut tunnels = registry
        .tunnels
        .lock()
        .map_err(|_| "Tunnel registry is unavailable.".to_string())?;
    if let Some(mut tunnel) = tunnels.remove(tunnel_id.trim()) {
        let _ = tunnel.child.kill();
        let _ = tunnel.child.wait();
    }
    Ok(())
}

fn existing_running_tunnel(
    registry: &TunnelRegistry,
    tunnel_id: &str,
) -> Result<Option<ServerTunnelStatus>, String> {
    let mut tunnels = registry
        .tunnels
        .lock()
        .map_err(|_| "Tunnel registry is unavailable.".to_string())?;
    let Some(tunnel) = tunnels.get_mut(tunnel_id) else {
        return Ok(None);
    };
    match tunnel
        .child
        .try_wait()
        .map_err(|err| format!("Failed to inspect SSH tunnel: {err}"))?
    {
        None => Ok(Some(tunnel.status.clone())),
        Some(_) => {
            tunnels.remove(tunnel_id);
            Ok(None)
        }
    }
}

fn tunnel_target(request: &ServerTunnelStartRequest) -> Result<OpenSshTarget, String> {
    let toolchain = Toolchain::from_default_root().map_err(|err| err.message)?;
    toolchain
        .install(ManagedTool::OpenSsh, false, None)
        .map_err(|err| err.message)?;
    let ssh_path = toolchain.status(ManagedTool::OpenSsh).executable;
    match request.server_kind.trim() {
        "ubuntu" => Ok(OpenSshTarget::new(
            ssh_path,
            PathBuf::from(
                request
                    .key_path
                    .as_deref()
                    .unwrap_or_default()
                    .trim()
                    .to_string(),
            ),
            request.user.as_deref().unwrap_or("root").trim().to_string(),
            request.host.trim().to_string(),
        )),
        "alpine" | "remoteHyperv" => {
            let host = request.host.trim().to_string();
            if host.is_empty() {
                return Err("Remote Alpine guest IP is required.".to_string());
            }
            let server_package_dir = default_server_package_dir().map_err(|err| err.message)?;
            let key_path =
                prepare_vendor_ssh_key(&server_package_dir).map_err(command_error_message)?;
            Ok(OpenSshTarget::new(ssh_path, key_path, "dune", host))
        }
        "hyperv" => {
            let vm_name = request
                .vm_name
                .as_deref()
                .unwrap_or_default()
                .trim()
                .to_string();
            if vm_name.is_empty() {
                return Err("Hyper-V VM name is required.".to_string());
            }
            let candidate = local_hyperv_candidate(&vm_name)?;
            let host = request
                .host
                .trim()
                .to_string()
                .is_empty()
                .then(|| candidate.vm.ipv4_addresses.first().cloned())
                .flatten()
                .unwrap_or_else(|| request.host.trim().to_string());
            if host.trim().is_empty() {
                return Err(format!(
                    "Hyper-V VM '{}' has no reported IPv4 address and no configured static IP.",
                    candidate.vm.name
                ));
            }
            let server_package_dir = default_server_package_dir().map_err(|err| err.message)?;
            let key_path =
                prepare_vendor_ssh_key(&server_package_dir).map_err(command_error_message)?;
            Ok(OpenSshTarget::new(ssh_path, key_path, "dune", host))
        }
        other => Err(format!("Unsupported tunnel server kind: {other}")),
    }
}

fn normalize_tunnel_service(service: &str) -> Result<String, String> {
    match service.trim() {
        "director" => Ok("director".to_string()),
        "fileBrowser" => Ok("fileBrowser".to_string()),
        other => Err(format!("Unsupported tunnel service: {other}")),
    }
}

fn discover_director_tunnel_port(target: &OpenSshTarget, namespace: &str) -> Result<u16, String> {
    let namespace = namespace.trim();
    if namespace.is_empty() {
        return Err(
            "BattleGroup namespace is required before starting the Director tunnel.".to_string(),
        );
    }
    let runner = OpenSshRunner::new(target.clone());
    let value = runner
        .run_json(
            &format!(
                "sudo kubectl get svc -n {} -o json",
                sh_single_quoted(namespace)
            ),
            "director service list",
        )
        .map_err(command_error_message)?;
    for service in value["items"].as_array().cloned().unwrap_or_default() {
        for port in service["spec"]["ports"]
            .as_array()
            .cloned()
            .unwrap_or_default()
        {
            if port["port"].as_u64() == Some(11717) {
                if let Some(node_port) = port["nodePort"]
                    .as_u64()
                    .and_then(|value| u16::try_from(value).ok())
                {
                    return Ok(node_port);
                }
            }
        }
    }
    Err("Director service is not currently exposed in Kubernetes.".to_string())
}

fn pick_available_local_port() -> Result<u16, String> {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|err| format!("Failed to reserve a local tunnel port: {err}"))?;
    let port = listener
        .local_addr()
        .map_err(|err| format!("Failed to read local tunnel port: {err}"))?
        .port();
    drop(listener);
    Ok(port)
}

fn is_local_port_available(port: u16) -> bool {
    TcpListener::bind(("127.0.0.1", port)).is_ok()
}

fn wait_for_local_tunnel(port: u16, timeout: std::time::Duration) -> bool {
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    false
}

fn local_hyperv_runner(
    vm_name: &str,
    explicit_host: Option<&str>,
) -> Result<OpenSshRunner, String> {
    let candidate = local_hyperv_candidate(vm_name)?;
    let ip = explicit_host
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| candidate.vm.ipv4_addresses.first().cloned())
        .ok_or_else(|| {
            format!(
                "Hyper-V VM '{}' has no reported IPv4 address and no configured static IP.",
                candidate.vm.name
            )
        })?;
    let toolchain = Toolchain::from_default_root().map_err(|err| err.message)?;
    toolchain
        .install(ManagedTool::OpenSsh, false, None)
        .map_err(|err| err.message)?;
    let ssh_path = toolchain.status(ManagedTool::OpenSsh).executable;
    let server_package_dir = default_server_package_dir().map_err(|err| err.message)?;
    let key_path = prepare_vendor_ssh_key(&server_package_dir).map_err(command_error_message)?;
    Ok(OpenSshRunner::new(OpenSshTarget::new(
        ssh_path, key_path, "dune", ip,
    )))
}

fn generate_ubuntu_ssh_key_inner(
    request: GenerateSshKeyRequest,
) -> Result<GenerateSshKeyResult, String> {
    let directory = PathBuf::from(request.directory.trim());
    if directory.as_os_str().is_empty() {
        return Err("SSH key directory is required.".to_string());
    }
    std::fs::create_dir_all(&directory)
        .map_err(|err| format!("Failed to create SSH key directory: {err}"))?;
    let file_name = request.file_name.trim();
    if file_name.is_empty()
        || file_name.contains('\\')
        || file_name.contains('/')
        || file_name.contains(':')
    {
        return Err("SSH key file name is invalid.".to_string());
    }
    let mut private_key = directory.join(file_name);
    let mut public_key = directory.join(format!("{file_name}.pub"));
    if private_key.exists() || public_key.exists() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or_default();
        let unique_name = format!("{file_name}-{unique}");
        private_key = directory.join(&unique_name);
        public_key = directory.join(format!("{unique_name}.pub"));
    }

    let toolchain = Toolchain::from_default_root().map_err(|err| err.message)?;
    toolchain
        .install(ManagedTool::OpenSsh, false, None)
        .map_err(command_error_message)?;
    let ssh_status = toolchain.status(ManagedTool::OpenSsh);
    let keygen = ssh_status.install_dir.join("ssh-keygen.exe");
    if !keygen.is_file() {
        return Err(format!(
            "OpenSSH key generator was not found: {}",
            keygen.display()
        ));
    }

    let mut command = Command::new(keygen);
    dune_manager_core::shell::suppress_console_window(&mut command);
    let output = command
        .args(["-t", "ed25519", "-N", "", "-C", "dune-manager-ubuntu", "-f"])
        .arg(&private_key)
        .output()
        .map_err(|err| format!("Failed to run ssh-keygen: {err}"))?;
    if !output.status.success() {
        return Err(command_error_message(
            dune_manager_core::errors::command_failure("ssh-keygen exited with an error", output),
        ));
    }
    let public_key_text = std::fs::read_to_string(&public_key)
        .map_err(|err| format!("Failed to read generated public key: {err}"))?;
    Ok(GenerateSshKeyResult {
        private_key_path: private_key.to_string_lossy().to_string(),
        public_key_path: public_key.to_string_lossy().to_string(),
        public_key: public_key_text.trim().to_string(),
    })
}

fn command_error_message(err: CommandFailure) -> String {
    let mut parts = vec![err.message];
    if !err.stderr.trim().is_empty() {
        parts.push(err.stderr);
    }
    if !err.stdout.trim().is_empty() {
        parts.push(err.stdout);
    }
    parts.join("\n")
}

fn local_hyperv_candidate(vm_name: &str) -> Result<DuneVmCandidate, String> {
    let vm_name = vm_name.trim();
    if vm_name.is_empty() {
        return Err("VM name is required.".to_string());
    }
    let provider = StrictPowerShellHyperV::new();
    let Some(vm) = provider.get_vm(vm_name).map_err(|err| err.message)? else {
        return Err(format!("Hyper-V VM '{vm_name}' was not found."));
    };
    Ok(classify_dune_vm(vm.clone()).unwrap_or(DuneVmCandidate {
        vm,
        confidence: DuneVmConfidence::Low,
        reasons: vec!["registered manually".to_string()],
    }))
}

fn read_remote_server_status(
    runner: &OpenSshRunner,
    namespace: &str,
    battlegroup_name: &str,
) -> CommandResult<RemoteServerStatus> {
    let kubernetes = StructuredKubectl::new(runner.clone());
    let battlegroup = kubernetes.battlegroup_state(namespace, battlegroup_name)?;
    Ok(RemoteServerStatus {
        battlegroup: RemoteBattlegroupStatus {
            stop: battlegroup.stop,
            phase: battlegroup.phase,
            server_group_phase: battlegroup.server_group_phase,
            director_phase: battlegroup.director_phase,
        },
    })
}

fn read_remote_server_components(
    runner: &OpenSshRunner,
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

fn read_remote_component_log_tail(
    runner: &OpenSshRunner,
    namespace: &str,
    component: &str,
    tail: u32,
) -> CommandResult<RemoteComponentLogResult> {
    let component = component.trim();
    let (mode, pattern) = component_pod_selection(component)?;
    let tail = tail.clamp(20, 500);
    let script = format!(
        r#"
ns={ns}
mode={mode}
pattern={pattern}
tail_lines={tail}
component={component}

if [ "$mode" = "role" ]; then
  pods=$(sudo kubectl get pods -n "$ns" -l "role=$pattern" --no-headers -o custom-columns=NAME:.metadata.name 2>/dev/null || true)
elif [ "$mode" = "roles" ]; then
  pods=$(sudo kubectl get pods -n "$ns" --no-headers -o custom-columns=NAME:.metadata.name,ROLE:.metadata.labels.role 2>/dev/null | grep -E "$pattern" | awk '{{print $1}}' || true)
else
  pods=$(sudo kubectl get pods -n "$ns" --no-headers -o custom-columns=NAME:.metadata.name 2>/dev/null | grep -- "$pattern" || true)
fi

if [ -z "$pods" ]; then
  echo "No pods found for $component."
  exit 0
fi

for pod in $pods; do
  echo "== $pod =="
  sudo kubectl logs -n "$ns" "$pod" --all-containers --tail="$tail_lines" 2>&1 || true
done
"#,
        ns = sh_single_quoted(namespace),
        mode = sh_single_quoted(mode),
        pattern = sh_single_quoted(pattern),
        tail = tail,
        component = sh_single_quoted(component),
    );
    let output = runner.run_script(&script)?;
    Ok(RemoteComponentLogResult {
        component: component.to_string(),
        output: redact_text(&output),
    })
}

fn restart_remote_component_inner(
    runner: &OpenSshRunner,
    namespace: &str,
    component: &str,
) -> CommandResult<RemoteComponentRestartResult> {
    let component = component.trim();
    let (mode, pattern) = component_pod_selection(component)?;
    let script = format!(
        r#"
ns={ns}
mode={mode}
pattern={pattern}
component={component}

if [ "$mode" = "role" ]; then
  pods=$(sudo kubectl get pods -n "$ns" -l "role=$pattern" --no-headers -o custom-columns=NAME:.metadata.name 2>/dev/null || true)
elif [ "$mode" = "roles" ]; then
  pods=$(sudo kubectl get pods -n "$ns" --no-headers -o custom-columns=NAME:.metadata.name,ROLE:.metadata.labels.role 2>/dev/null | grep -E "$pattern" | awk '{{print $1}}' || true)
else
  pods=$(sudo kubectl get pods -n "$ns" --no-headers -o custom-columns=NAME:.metadata.name 2>/dev/null | grep -- "$pattern" || true)
fi

if [ -z "$pods" ]; then
  echo "No pods found for $component."
  exit 0
fi

for pod in $pods; do
  echo "Restarting $pod"
  sudo kubectl delete pod -n "$ns" "$pod" --wait=false
done
"#,
        ns = sh_single_quoted(namespace),
        mode = sh_single_quoted(mode),
        pattern = sh_single_quoted(pattern),
        component = sh_single_quoted(component),
    );
    let output = runner.run_script(&script)?;
    Ok(RemoteComponentRestartResult {
        component: component.to_string(),
        output: redact_text(&output),
    })
}

fn component_pod_selection(component: &str) -> CommandResult<(&'static str, &'static str)> {
    match component {
        "database" => Ok(("role", "igw-database")),
        "database-utilities" => Ok((
            "roles",
            "igw-database-utility|igw-database-monitor|igw-database-pghero",
        )),
        "message-queue" => Ok(("role", "igw-message-queue")),
        "director" => Ok(("role", "igw-battlegroup-director")),
        "gateway" | "gateway-resource" => Ok(("role", "igw-server-gateway")),
        "text-router" => Ok(("role", "igw-text-router")),
        "file-browser" => Ok(("role", "igw-filebrowser")),
        "server-group" => Ok(("role", "igw-server")),
        "map-survival-1" => Ok(("name", "-sg-survival-1-")),
        "map-overmap" => Ok(("name", "-sg-overmap-")),
        "map-deepdesert" => Ok(("name", "-sg-deepdesert-")),
        "map-social-arrakeen" => Ok(("name", "-sg-sh-arrakeen-")),
        "map-social-harkovillage" => Ok(("name", "-sg-sh-harkovillage-")),
        _ => Err(dune_manager_core::errors::failure(format!(
            "Unknown component key: {component}"
        ))),
    }
}

fn pod_component(
    label: &str,
    log_key: &str,
    pods: &Value,
    matches: impl Fn(&str, &str) -> bool,
) -> RemoteServerComponent {
    let mut total = 0usize;
    let mut ready = 0usize;
    let mut restarts = 0u64;
    let mut reasons = Vec::new();
    let mut phases = Vec::new();
    for item in pods["items"].as_array().cloned().unwrap_or_default() {
        let name = item["metadata"]["name"].as_str().unwrap_or_default();
        let role = item["metadata"]["labels"]["role"]
            .as_str()
            .unwrap_or_default();
        if !matches(role, name) {
            continue;
        }
        total += 1;
        let phase = item["status"]["phase"].as_str().unwrap_or_default();
        if !phase.is_empty() {
            phases.push(phase.to_string());
        }
        let statuses = item["status"]["containerStatuses"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let pod_ready = !statuses.is_empty()
            && statuses
                .iter()
                .all(|status| status["ready"].as_bool().unwrap_or(false));
        if pod_ready || phase == "Succeeded" {
            ready += 1;
        }
        for status in statuses {
            restarts += status["restartCount"].as_u64().unwrap_or_default();
            if let Some(reason) = status["state"]["waiting"]["reason"].as_str() {
                reasons.push(reason.to_string());
            }
            if let Some(reason) = status["state"]["terminated"]["reason"].as_str() {
                if reason != "Completed" {
                    reasons.push(reason.to_string());
                }
            }
        }
    }

    if total == 0 {
        return component(
            label,
            log_key,
            "system",
            "Not present",
            "gray",
            "No matching runtime component was found.",
            vec![],
        );
    }
    let details = compact_details(vec![
        format!("{ready}/{total} pods ready"),
        if restarts > 0 {
            format!("{restarts} container restarts")
        } else {
            String::new()
        },
        if reasons.is_empty() {
            String::new()
        } else {
            format!("Reason: {}", reasons.join(", "))
        },
    ]);
    if ready == total && reasons.is_empty() {
        component(
            label,
            log_key,
            "system",
            "Ready",
            "green",
            "All pods are ready.",
            details,
        )
    } else if reasons.iter().any(|reason| is_bad_reason(reason))
        || phases.iter().any(|phase| phase == "Failed")
    {
        component(
            label,
            log_key,
            "system",
            "Problem",
            "red",
            "One or more pods are failing.",
            details,
        )
    } else {
        component(
            label,
            log_key,
            "system",
            "Starting",
            "amber",
            "Waiting for pods to become ready.",
            details,
        )
    }
}

fn server_resource_components(resources: &Value) -> Vec<RemoteServerComponent> {
    let mut items = resources["items"].as_array().cloned().unwrap_or_default();
    items.sort_by(|left, right| {
        left["metadata"]["name"]
            .as_str()
            .unwrap_or_default()
            .cmp(right["metadata"]["name"].as_str().unwrap_or_default())
    });
    let mut output = Vec::new();
    for item in items {
        let kind = item["kind"].as_str().unwrap_or_default();
        let name = item["metadata"]["name"].as_str().unwrap_or_default();
        match kind {
            "ServerGroup" => output.push(server_group_component(&item)),
            "ServerGateway" => output.push(resource_phase_component("Gateway Resource", &item)),
            "ServerSet" => {
                if should_show_serverset(&item) {
                    output.push(serverset_component(name, &item));
                }
            }
            _ => {}
        }
    }
    output
}

fn server_group_component(item: &Value) -> RemoteServerComponent {
    let phase = item["status"]["phase"].as_str().unwrap_or("Unknown");
    phase_component(
        "Server Group",
        "server-group",
        "system",
        phase,
        format!("Server Group reports {phase}."),
        vec![],
    )
}

fn resource_phase_component(label: &str, item: &Value) -> RemoteServerComponent {
    let phase = item["status"]["phase"].as_str().unwrap_or("Unknown");
    phase_component(
        label,
        "gateway-resource",
        "system",
        phase,
        format!("{label} reports {phase}."),
        vec![],
    )
}

fn serverset_component(name: &str, item: &Value) -> RemoteServerComponent {
    let map = item["spec"]["map"].as_str().unwrap_or_default();
    let label = friendly_map_name(map, name);
    let phase = item["status"]["phase"].as_str().unwrap_or("Unknown");
    let target = item["status"]["targetReplicas"]
        .as_u64()
        .unwrap_or_default();
    let ready = item["status"]["readyReplicas"].as_u64().unwrap_or_default();
    let completed = item["status"]["completedReplicas"]
        .as_u64()
        .unwrap_or_default();
    let pods = item["status"]["pods"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let game_ready = pods
        .iter()
        .filter(|pod| pod["ready"].as_bool().unwrap_or(false))
        .count();
    let details = compact_details(vec![
        format!("{ready}/{target} Kubernetes-ready replicas"),
        format!("{completed}/{target} completed game replicas"),
        format!("{game_ready}/{target} game-ready servers"),
    ]);
    let summary =
        if phase == "Initializing" && ready >= target && target > 0 && game_ready < target as usize
        {
            "Game process is running, but game readiness has not completed.".to_string()
        } else {
            format!("{label} reports {phase}.")
        };
    phase_component(
        &label,
        &serverset_log_key(name, map),
        "map",
        phase,
        summary,
        details,
    )
}

fn should_show_serverset(item: &Value) -> bool {
    let phase = item["status"]["phase"].as_str().unwrap_or_default();
    let target = item["status"]["targetReplicas"]
        .as_u64()
        .unwrap_or_default();
    let map = item["spec"]["map"].as_str().unwrap_or_default();
    phase != "Stopped" || target > 0 || matches!(map, "Survival_1" | "Overmap" | "DeepDesert_1")
}

fn phase_component(
    label: &str,
    log_key: &str,
    category: &str,
    phase: &str,
    summary: String,
    details: Vec<String>,
) -> RemoteServerComponent {
    let normalized = phase.to_ascii_lowercase();
    let (state, tone) = match normalized.as_str() {
        "healthy" | "running" | "ready" | "available" => ("Ready", "green"),
        "stopped" | "suspended" => ("Stopped", "gray"),
        "initializing" | "reconciling" | "pending" | "starting" => ("Starting", "amber"),
        "failed" | "error" | "degraded" => ("Problem", "red"),
        _ => ("Unknown", "amber"),
    };
    component(label, log_key, category, state, tone, summary, details)
}

fn component(
    name: &str,
    log_key: &str,
    category: &str,
    state: &str,
    tone: &str,
    summary: impl Into<String>,
    details: Vec<String>,
) -> RemoteServerComponent {
    RemoteServerComponent {
        name: name.to_string(),
        log_key: log_key.to_string(),
        category: category.to_string(),
        state: state.to_string(),
        tone: tone.to_string(),
        summary: summary.into(),
        details,
    }
}

fn compact_details(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .filter(|value| !value.trim().is_empty())
        .collect()
}

fn is_bad_reason(reason: &str) -> bool {
    matches!(
        reason,
        "CrashLoopBackOff"
            | "ImagePullBackOff"
            | "ErrImagePull"
            | "CreateContainerConfigError"
            | "CreateContainerError"
            | "RunContainerError"
            | "OOMKilled"
            | "Error"
    )
}

fn friendly_map_name(map: &str, fallback_name: &str) -> String {
    let normalized = map.to_ascii_lowercase();
    if normalized == "survival_1" || fallback_name.contains("survival-1") {
        return "Hagga Basin".to_string();
    }
    if normalized == "overmap" || fallback_name.contains("overmap") {
        return "Overmap".to_string();
    }
    if normalized.contains("deepdesert") || fallback_name.contains("deepdesert") {
        return "Deep Desert".to_string();
    }
    if fallback_name.contains("sh-arrakeen") {
        return "Social Hub: Arrakeen".to_string();
    }
    if fallback_name.contains("sh-harkovillage") {
        return "Social Hub: Harko Village".to_string();
    }
    if !map.is_empty() {
        return map.replace('_', " ");
    }
    "Game Server".to_string()
}

fn serverset_log_key(name: &str, map: &str) -> String {
    let combined = format!("{name} {map}").to_ascii_lowercase();
    if map.eq_ignore_ascii_case("Survival_1") || combined.contains("survival-1") {
        return "map-survival-1".to_string();
    }
    if map.eq_ignore_ascii_case("Overmap") || combined.contains("overmap") {
        return "map-overmap".to_string();
    }
    if combined.contains("deepdesert") || combined.contains("deep-desert") {
        return "map-deepdesert".to_string();
    }
    if combined.contains("sh-arrakeen") {
        return "map-social-arrakeen".to_string();
    }
    if combined.contains("sh-harkovillage") {
        return "map-social-harkovillage".to_string();
    }
    format!("map-{}", sanitize_component_key(map))
}

fn sanitize_component_key(value: &str) -> String {
    let key = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    if key.is_empty() {
        "unknown".to_string()
    } else {
        key
    }
}

fn sh_single_quoted(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn apply_instance_layout<R>(
    request: &SetupRequest,
    namespace: &str,
    battlegroup_name: &str,
    runner: &R,
    sink: &mut TauriOperationSink,
) -> CommandResult<bool>
where
    R: RemoteCommandRunner + Clone,
{
    let battlegroup = dune_manager_core::orchestration::BattlegroupRef {
        namespace: namespace.to_string(),
        name: battlegroup_name.to_string(),
    };
    let orchestrator = MapInstanceOrchestrator::new(runner.clone());

    sink.info("layout", "Applying Hagga Basin instance count.");
    let survival = orchestrator.set_instances(&SetMapInstancesRequest::new(
        battlegroup.clone(),
        InstanceMap::Survival1,
        request.survival_instances,
    ))?;
    let mut changed = survival.restart_required;

    let deep_desert_total = request.deep_desert_pve_instances + request.deep_desert_pvp_instances;
    if deep_desert_total > 1 {
        return Err(dune_manager_core::errors::failure(
            "Only one Deep Desert instance is supported in this build.",
        ));
    }
    if deep_desert_total > 0 {
        sink.info("layout", "Applying Deep Desert instance count.");
        let mut deep_desert =
            SetMapInstancesRequest::new(battlegroup, InstanceMap::DeepDesert, deep_desert_total);
        deep_desert.pvp_instance_count = Some(request.deep_desert_pvp_instances);
        let deep_desert = orchestrator.set_instances(&deep_desert)?;
        changed |= deep_desert.restart_required;
    } else {
        sink.info(
            "layout",
            "Deep Desert is disabled; skipping Deep Desert instance patch.",
        );
    }

    if request.deep_desert_warm_servers > 0 {
        return Err(dune_manager_core::errors::failure(
            "Warm Deep Desert instances are not supported yet; set Warm Deep Desert Instances to 0 for this build.",
        ));
    }

    Ok(changed)
}

fn required_layout_memory_gib(survival_instances: usize, deep_desert_instances: usize) -> u64 {
    let survival = survival_instances.max(1) as u64 * 20;
    let social = if deep_desert_instances > 0 { 10 } else { 0 };
    let deep_desert = deep_desert_instances as u64 * 10;
    survival + social + deep_desert
}

fn recommended_ubuntu_swap_gib(
    preflight: &UbuntuSshPreflight,
    request: &RemoteSetupRequest,
) -> u64 {
    let required = required_layout_memory_gib(
        request.survival_instances,
        request.deep_desert_pve_instances + request.deep_desert_pvp_instances,
    );
    let required_bytes = required.saturating_mul(1024 * 1024 * 1024);
    let shortfall =
        bytes_to_gib_ceil(required_bytes.saturating_sub(preflight.available_memory_bytes));
    shortfall.clamp(2, 64)
}

fn bytes_to_gib_ceil(bytes: u64) -> u64 {
    const GIB: u64 = 1024 * 1024 * 1024;
    if bytes == 0 {
        0
    } else {
        bytes.div_ceil(GIB)
    }
}

fn is_empty_dir(path: &std::path::Path) -> bool {
    path.is_dir()
        && path
            .read_dir()
            .map(|mut entries| entries.next().is_none())
            .unwrap_or(false)
}

fn remove_switch_if_unused(switch_name: &str) -> CommandResult<bool> {
    if switch_name.trim().is_empty() {
        return Ok(false);
    }
    let output = run_powershell(&format!(
        r#"
$ErrorActionPreference = 'Stop'
$switchName = {switch_name}
$switch = Get-VMSwitch -Name $switchName -ErrorAction SilentlyContinue
if (-not $switch) {{
  [Console]::Out.WriteLine('false')
  exit 0
}}
$usedByVms = @(Get-VMNetworkAdapter -All -ErrorAction SilentlyContinue | Where-Object {{ $_.SwitchName -eq $switchName }})
if ($usedByVms.Count -eq 0) {{
  Remove-VMSwitch -Name $switchName -Force -ErrorAction Stop
  [Console]::Out.WriteLine('true')
}} else {{
  [Console]::Out.WriteLine('false')
}}
"#,
        switch_name = ps_single_quoted(switch_name),
    ))?;
    Ok(output.trim().eq_ignore_ascii_case("true"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(TunnelRegistry::default())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            detect_environment,
            default_vm_location,
            vm_destination_has_vm,
            remote_server_status,
            remote_server_components,
            local_hyperv_runtime,
            start_server_tunnel,
            stop_server_tunnel,
            server_tunnel_status,
            stop_all_tunnels,
            remote_component_log_tail,
            local_hyperv_component_log_tail,
            restart_remote_component,
            restart_local_hyperv_component,
            start_remote_battlegroup,
            stop_remote_battlegroup,
            start_local_hyperv_battlegroup,
            stop_local_hyperv_battlegroup,
            register_local_hyperv_server,
            start_local_hyperv_server,
            stop_local_hyperv_server,
            detect_remote_ubuntu_servers,
            detect_remote_alpine_servers,
            generate_ubuntu_ssh_key,
            preflight_remote_ubuntu,
            start_full_setup,
            start_remote_ubuntu_setup,
            rollback_setup
        ])
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                window.state::<TunnelRegistry>().stop_all();
            }
        })
        .run(tauri::generate_context!())
        .expect("failed to run Tauri application");
}
