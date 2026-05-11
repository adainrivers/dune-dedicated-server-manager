use std::path::PathBuf;

use dune_manager_core::environment::{detect_setup_environment, SetupEnvironment};
use dune_manager_core::models::CommandResult;
use dune_manager_core::orchestration::{
    BattlegroupManagementOrchestrator, BattlegroupRef, DuneVmCandidate, DuneVmDetector,
    ExperimentalSwapOrchestrator, ExperimentalSwapRequest, GuestBootstrapPlan, GuestNetworkConfig,
    GuestNetworkPlan, HyperVInitialSetupOrchestrator, HyperVInitialSetupRequest,
    HyperVVmSetupRequest, InstanceMap, MapInstanceOrchestrator, MemoryProfile,
    OpenSshGuestProvider, OpenSshRunner, OpenSshTarget, OperationSink, OrchestrationEvent,
    SetMapInstancesRequest, SshGuestBootstrapProvider, StrictPowerShellHyperV, StructuredKubectl,
    VmProvider,
};
use dune_manager_core::shell::{ps_single_quoted, run_powershell};
use dune_manager_core::toolchain::{
    default_server_package_dir, default_vm_destination, prepare_vendor_ssh_key, ManagedTool,
    Toolchain,
};
use serde::{Deserialize, Serialize};
use tauri::Emitter;

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
async fn detect_dune_vms() -> Result<Vec<DuneVmCandidate>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        DuneVmDetector::new(StrictPowerShellHyperV::new())
            .detect()
            .map_err(|err| err.message)
    })
    .await
    .map_err(|err| format!("Dune VM detection worker failed: {err}"))?
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
    namespace: String,
    battlegroup_name: String,
    world_unique_name: String,
    director_node_port: Option<u16>,
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
            "A Hyper-V VM named '{}' already exists. Remove it or choose a different VM Name before setup.",
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

    apply_instance_layout(
        &request,
        &result.bootstrap.namespace,
        &result.bootstrap.battlegroup_name,
        &bootstrap_runner,
        sink,
    )?;

    if request.enable_swap {
        sink.info("guest-swap", "Enabling experimental swap profile.");
        let mut swap = ExperimentalSwapRequest::new(
            result.bootstrap.namespace.clone(),
            result.bootstrap.battlegroup_name.clone(),
        );
        swap.restart_k3s = true;
        ExperimentalSwapOrchestrator::new(bootstrap_runner.clone()).enable(&swap, sink)?;
    }

    let battlegroup = BattlegroupRef {
        namespace: result.bootstrap.namespace.clone(),
        name: result.bootstrap.battlegroup_name.clone(),
    };
    sink.info("bg", "Starting battlegroup after setup.");
    let director_node_port =
        BattlegroupManagementOrchestrator::new(StructuredKubectl::new(bootstrap_runner))
            .start_and_wait_director(&battlegroup, 180, sink)?;
    if let Some(port) = director_node_port {
        sink.info(
            "director",
            format!("Director is available on NodePort {port}."),
        );
    }
    sink.warn(
        "manager-api",
        "Manager API install is not part of this setup run yet; configure a Manager admin token before exposing :8787.",
    );

    Ok(SetupRunResult {
        namespace: result.bootstrap.namespace,
        battlegroup_name: result.bootstrap.battlegroup_name,
        world_unique_name: result.bootstrap.world_unique_name,
        director_node_port,
    })
}

fn apply_instance_layout(
    request: &SetupRequest,
    namespace: &str,
    battlegroup_name: &str,
    runner: &OpenSshRunner,
    sink: &mut TauriOperationSink,
) -> CommandResult<()> {
    let battlegroup = dune_manager_core::orchestration::BattlegroupRef {
        namespace: namespace.to_string(),
        name: battlegroup_name.to_string(),
    };
    let orchestrator = MapInstanceOrchestrator::new(runner.clone());

    sink.info("layout", "Applying Hagga Basin instance count.");
    orchestrator.set_instances(&SetMapInstancesRequest::new(
        battlegroup.clone(),
        InstanceMap::Survival1,
        request.survival_instances,
    ))?;

    let deep_desert_total = request.deep_desert_pve_instances + request.deep_desert_pvp_instances;
    if deep_desert_total > 0 {
        sink.info("layout", "Applying Deep Desert instance count.");
        let mut deep_desert =
            SetMapInstancesRequest::new(battlegroup, InstanceMap::DeepDesert, deep_desert_total);
        deep_desert.pvp_instance_count = Some(request.deep_desert_pvp_instances);
        orchestrator.set_instances(&deep_desert)?;
    } else {
        sink.info(
            "layout",
            "Deep Desert is disabled; skipping Deep Desert instance patch.",
        );
    }

    if request.deep_desert_warm_servers > 0 {
        sink.info(
            "layout",
            "Warm Deep Desert instance selection is recorded in the setup plan; min-server wiring is pending.",
        );
    }

    Ok(())
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
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            detect_environment,
            default_vm_location,
            vm_destination_has_vm,
            detect_dune_vms,
            start_full_setup,
            rollback_setup
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Tauri application");
}
