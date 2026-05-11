use serde::Serialize;
use serde_json::{json, Value};

use crate::orchestration::{GuestProvider, HostProvider};
use crate::{
    database::{DuneDatabase, DuneDatabaseConfig, DEFAULT_DUNE_DATABASE_PORT},
    environment::detect_setup_environment,
    errors::failure,
    models::{CommandFailure, CommandResult},
    orchestration::{
        battlegroup_command_catalog, detect_player_address_candidates, hyperv_initial_setup_flow,
        BattlegroupManagementOrchestrator, BattlegroupRef, BattlegroupUpdateOrchestrator,
        DuneVmDetector, ExperimentalSwapOrchestrator, ExperimentalSwapRequest,
        GuestBootstrapOrchestrator, GuestBootstrapPlan, GuestNetworkConfig,
        HyperVVmLifecycleOrchestrator, HyperVVmSetupOrchestrator, HyperVVmSetupRequest,
        InstanceMap, ManagerApiInstallRequest, ManagerApiInstaller, MapInstanceOrchestrator,
        MemoryProfile, OpenSshGuestProvider, OpenSshRunner, OpenSshTarget, OrchestrationEvent,
        SetMapInstancesRequest, SshGuestBootstrapProvider, StrictPowerShellHyperV,
        StructuredBattlegroupOps, StructuredKubectl, VecOperationSink, VmProvider,
        DEFAULT_VM_DISK_BYTES,
    },
    toolchain::{ManagedTool, Toolchain},
};

/// Runs the CLI using process arguments and returns a process exit code.
///
/// Successful commands print pretty JSON to stdout. Failures print a redacted
/// JSON error envelope to stderr.
pub fn run_cli_from_env() -> i32 {
    match run_cli(std::env::args().skip(1).collect()) {
        Ok(value) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".to_string())
            );
            0
        }
        Err(err) => {
            eprintln!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "ok": false,
                    "error": err.message,
                    "stdout": err.stdout,
                    "stderr": err.stderr,
                    "code": err.code,
                }))
                .unwrap_or_else(|_| "{\"ok\":false}".to_string())
            );
            err.code.unwrap_or(1)
        }
    }
}

fn run_cli(args: Vec<String>) -> CommandResult<Value> {
    let args = CliArgs::new(args);
    if args.is_empty() || args.has_flag("--help") || args.has_flag("-h") {
        return Ok(json!({
            "ok": true,
            "usage": usage(),
        }));
    }

    let positional = args.positional_slice();
    match positional.as_slice() {
        ["flow", "initial"] => to_json(hyperv_initial_setup_flow()),
        ["flow", "battlegroup"] => to_json(battlegroup_command_catalog()),
        ["host", "environment"] => to_json(detect_setup_environment()?),
        ["host", "readiness"] => to_json(StrictPowerShellHyperV::new().readiness()?),
        ["host", "drives"] => {
            let min_gb = args.optional_u64("--min-gb")?.unwrap_or(100);
            to_json(
                StrictPowerShellHyperV::new()
                    .drives_with_minimum_free_space(min_gb * 1024 * 1024 * 1024)?,
            )
        }
        ["host", "adapters"] => to_json(StrictPowerShellHyperV::new().active_physical_adapters()?),
        ["db", "ping"] => to_json(DuneDatabase::new(db_config(&args)?).health()?),
        ["db", "world-partitions"] => {
            let map = args.optional("--map");
            to_json(DuneDatabase::new(db_config(&args)?).world_partitions(map.as_deref())?)
        }
        ["tools", "status"] => {
            let toolchain = toolchain(&args)?;
            if let Some(tool) = args.optional("--tool") {
                to_json(toolchain.status(ManagedTool::parse(&tool)?))
            } else {
                to_json(toolchain.status_all())
            }
        }
        ["tools", "path"] => {
            let tool = ManagedTool::parse(&args.required("--tool")?)?;
            let status = toolchain(&args)?.status(tool);
            Ok(json!({
                "ok": true,
                "tool": tool,
                "installed": status.installed,
                "path": status.executable,
            }))
        }
        ["tools", "install"] => {
            let tool_name = args.required("--tool")?;
            let toolchain = toolchain(&args)?;
            let force = args.has_flag("--force");
            let source_url = args.optional("--source-url");
            if tool_name.eq_ignore_ascii_case("all") {
                if source_url.is_some() {
                    return Err(failure(
                        "--source-url can only be used when installing one tool",
                    ));
                }
                let mut results = Vec::new();
                for tool in [ManagedTool::SteamCmd, ManagedTool::OpenSsh] {
                    results.push(toolchain.install(tool, force, None)?);
                }
                to_json(results)
            } else {
                to_json(toolchain.install(ManagedTool::parse(&tool_name)?, force, source_url)?)
            }
        }
        ["vm", "get"] => {
            let name = args.required("--name")?;
            to_json(StrictPowerShellHyperV::new().get_vm(&name)?)
        }
        ["vm", "list"] => to_json(StrictPowerShellHyperV::new().list_vms()?),
        ["vm", "detect-dune"] => {
            to_json(DuneVmDetector::new(StrictPowerShellHyperV::new()).detect()?)
        }
        ["vm", "start"] => {
            let name = args.required("--name")?;
            let mut sink = VecOperationSink::default();
            HyperVVmLifecycleOrchestrator::new(StrictPowerShellHyperV::new())
                .start(&name, &mut sink)?;
            operation_ok(sink)
        }
        ["vm", "stop"] => {
            let name = args.required("--name")?;
            let mut sink = VecOperationSink::default();
            HyperVVmLifecycleOrchestrator::new(StrictPowerShellHyperV::new())
                .stop(&name, &mut sink)?;
            operation_ok(sink)
        }
        ["setup", "create-vm"] => {
            let memory_gb = args.optional_u64("--memory-gb")?.unwrap_or(20);
            let processor_count = args.optional_u64("--processors")?.unwrap_or(4);
            let disk_gb = args
                .optional_u64("--disk-gb")?
                .unwrap_or(DEFAULT_VM_DISK_BYTES / 1024 / 1024 / 1024);
            let request = HyperVVmSetupRequest {
                install_path: args.required("--install-path")?.into(),
                vm_name: args.required("--vm-name")?,
                destination_path: args.required("--destination")?.into(),
                switch_name: args.required("--switch")?,
                adapter_name: args.required("--adapter")?,
                memory: memory_profile(memory_gb)?,
                processor_count: u32::try_from(processor_count)
                    .map_err(|_| failure("--processors must fit in a 32-bit value"))?,
                replace_existing_vm: args.has_flag("--replace-existing"),
                clear_destination: args.has_flag("--clear-destination"),
                disk_size_bytes: disk_gb.saturating_mul(1024 * 1024 * 1024),
            };
            if request.disk_size_bytes == 0 {
                return Err(failure("--disk-gb must be greater than zero"));
            }
            if request.processor_count == 0 {
                return Err(failure("--processors must be greater than zero"));
            }
            let provider = StrictPowerShellHyperV::new();
            let mut sink = VecOperationSink::default();
            let result = HyperVVmSetupOrchestrator::new(&provider, &provider)
                .import_and_prepare_vm(&request, &mut sink)?;
            to_json(OperationOutput {
                ok: true,
                result,
                events: sink.events,
            })
        }
        ["guest", "player-candidates"] => {
            let host = args.required("--host")?;
            let guest = ssh_guest_provider(&args)?;
            let mut sink = VecOperationSink::default();
            let candidates = detect_player_address_candidates(&guest, &host, &mut sink)?;
            to_json(OperationOutput {
                ok: true,
                result: candidates,
                events: sink.events,
            })
        }
        ["guest", "write-player-settings"] => {
            let host = args.required("--host")?;
            let player_ip = args.required("--player-ip")?;
            let guest = ssh_guest_provider(&args)?;
            guest.write_player_settings(&host, &player_ip)?;
            Ok(json!({ "ok": true }))
        }
        ["guest", "apply-static-network"] => {
            let host = args.required("--host")?;
            let guest = ssh_guest_provider(&args)?;
            guest.apply_static_network(
                &host,
                &GuestNetworkConfig {
                    interface: args
                        .optional("--interface")
                        .unwrap_or_else(|| "eth0".to_string()),
                    address_cidr: args.required("--address-cidr")?,
                    gateway: args.required("--gateway")?,
                    dns: args.required("--dns")?,
                },
            )?;
            Ok(json!({ "ok": true }))
        }
        ["guest", "bootstrap"] => {
            let token = args.token()?;
            let plan = GuestBootstrapPlan::from_self_host_token(
                args.required("--player-ip")?,
                args.required("--world-name")?,
                args.optional("--region")
                    .unwrap_or_else(|| "Europe Test".to_string()),
                token,
            )?;
            let runner = ssh_runner(&args)?;
            let provider = SshGuestBootstrapProvider::new(runner.clone());
            let mut sink = VecOperationSink::default();
            let result = GuestBootstrapOrchestrator::new(provider).run(&plan, &mut sink)?;
            if args.has_flag("--enable-experimental-swap") {
                let mut request = ExperimentalSwapRequest::new(
                    result.namespace.clone(),
                    result.battlegroup_name.clone(),
                );
                request.swap_size_gib = args
                    .optional_u64("--experimental-swap-size-gib")?
                    .unwrap_or(30);
                request.restart_k3s = !args.has_flag("--experimental-swap-no-restart-k3s");
                let swap = ExperimentalSwapOrchestrator::new(runner).enable(&request, &mut sink)?;
                return to_json(OperationOutput {
                    ok: true,
                    result: json!({
                        "bootstrap": result,
                        "experimentalSwap": swap,
                    }),
                    events: sink.events,
                });
            }
            to_json(OperationOutput {
                ok: true,
                result,
                events: sink.events,
            })
        }
        ["guest", "experimental-swap", "status"] => {
            let battlegroup = optional_battlegroup_ref(&args)?;
            let orchestrator = ExperimentalSwapOrchestrator::new(ssh_runner(&args)?);
            let status = orchestrator.status(
                battlegroup
                    .as_ref()
                    .map(|bg| (bg.namespace.as_str(), bg.name.as_str())),
            )?;
            to_json(status)
        }
        ["guest", "experimental-swap", "enable"] => {
            let mut request = ExperimentalSwapRequest::new(
                args.required("--namespace")?,
                args.required("--name")?,
            );
            request.swap_size_gib = args.optional_u64("--swap-size-gib")?.unwrap_or(30);
            request.restart_k3s = !args.has_flag("--no-restart-k3s");
            let mut sink = VecOperationSink::default();
            let result = ExperimentalSwapOrchestrator::new(ssh_runner(&args)?)
                .enable(&request, &mut sink)?;
            to_json(OperationOutput {
                ok: true,
                result,
                events: sink.events,
            })
        }
        ["token", "plan"] => {
            let token = args.token()?;
            let plan = GuestBootstrapPlan::from_self_host_token(
                args.required("--player-ip")?,
                args.required("--world-name")?,
                args.optional("--region")
                    .unwrap_or_else(|| "Europe Test".to_string()),
                token,
            )?;
            Ok(json!({
                "ok": true,
                "worldUniqueName": plan.world_unique_name(),
                "hostId": plan.host_id,
                "worldSuffix": plan.world_suffix,
                "playerIp": plan.player_ip,
                "worldName": plan.world_name,
                "worldRegion": plan.world_region,
            }))
        }
        ["ssh", "shell-spec"] => {
            let spec = ssh_runner(&args)?.interactive_shell_spec()?;
            to_json(spec)
        }
        ["bg", "list"] => to_json(bg_ops(&args)?.list()?),
        ["bg", "status"] => {
            let bg = battlegroup_ref(&args)?;
            to_json(bg_ops(&args)?.status(&bg)?)
        }
        ["bg", "patch-region"] => {
            let bg = battlegroup_ref(&args)?;
            let region = args.required("--region")?;
            bg_ops(&args)?.patch_region(&bg, &region)?;
            Ok(json!({ "ok": true }))
        }
        ["bg", "instances", "set"] => {
            let bg = battlegroup_ref(&args)?;
            let map = InstanceMap::parse(&args.required("--map")?)?;
            let count = usize::try_from(args.required_u64("--count")?)
                .map_err(|_| failure("--count is too large"))?;
            let mut request = SetMapInstancesRequest::new(bg, map, count);
            request.pvp_instance_count = args
                .optional_u64("--pvp-count")?
                .map(|value| {
                    usize::try_from(value).map_err(|_| failure("--pvp-count is too large"))
                })
                .transpose()?;
            let result =
                MapInstanceOrchestrator::new(ssh_runner(&args)?).set_instances(&request)?;
            let restart = if args.has_flag("--restart") {
                Some(bg_lifecycle(&args, "restart")?)
            } else {
                None
            };
            Ok(json!({
                "ok": true,
                "result": result,
                "restart": restart,
            }))
        }
        ["bg", "pods"] => {
            let namespace = args.required("--namespace")?;
            to_json(bg_ops(&args)?.list_pods(&namespace)?)
        }
        ["bg", "pod-shell-spec"] => {
            let namespace = args.required("--namespace")?;
            let pod = args.required("--pod")?;
            to_json(bg_ops(&args)?.pod_shell_spec(&namespace, &pod)?)
        }
        ["bg", "export-logs"] => {
            let namespace = args.required("--namespace")?;
            to_json(bg_ops(&args)?.export_namespace_logs(&namespace)?)
        }
        ["bg", "export-operator-logs"] => to_json(bg_ops(&args)?.export_operator_logs()?),
        ["bg", "file-browser-url"] => {
            let vm_ip = args.required("--vm-ip")?;
            let kube = StructuredKubectl::new(ssh_runner(&args)?);
            to_json(BattlegroupManagementOrchestrator::new(kube).file_browser_url(&vm_ip)?)
        }
        ["bg", "director-url"] => {
            let bg = battlegroup_ref(&args)?;
            let vm_ip = args.required("--vm-ip")?;
            let kube = StructuredKubectl::new(ssh_runner(&args)?);
            to_json(BattlegroupManagementOrchestrator::new(kube).director_url(&bg, &vm_ip)?)
        }
        ["bg", "start"] => bg_lifecycle(&args, "start"),
        ["bg", "stop"] => bg_lifecycle(&args, "stop"),
        ["bg", "restart"] => bg_lifecycle(&args, "restart"),
        ["bg", "update"] => {
            let bg = battlegroup_ref(&args)?;
            let provider = SshGuestBootstrapProvider::new(ssh_runner(&args)?);
            let mut sink = VecOperationSink::default();
            BattlegroupUpdateOrchestrator::new(provider).update_from_downloads(&bg, &mut sink)?;
            operation_ok(sink)
        }
        ["manager", "install"] => {
            let token = args.token()?;
            let mut request = ManagerApiInstallRequest::new(
                args.required("--binary")?,
                token,
                args.required("--namespace")?,
            );
            request.port = optional_port(&args, "--port")?.unwrap_or(request.port);
            request.director_base_url = args.optional("--director-base-url");
            if let Some(path) = args.optional("--remote-binary") {
                request.remote_binary_path = path;
            }
            if let Some(path) = args.optional("--env-path") {
                request.env_path = path;
            }
            if let Some(path) = args.optional("--log-path") {
                request.log_path = path;
            }
            if let Some(path) = args.optional("--kubeconfig-path") {
                request.kubeconfig_path = path;
            }
            let mut sink = VecOperationSink::default();
            let result =
                ManagerApiInstaller::new(ssh_runner(&args)?).install(&request, &mut sink)?;
            to_json(OperationOutput {
                ok: true,
                result,
                events: sink.events,
            })
        }
        ["manager", "status"] => {
            let port = optional_port(&args, "--port")?.unwrap_or(8787);
            to_json(ManagerApiInstaller::new(ssh_runner(&args)?).status(port)?)
        }
        other => Err(failure(format!(
            "Unknown command: {}",
            if other.is_empty() {
                "<none>".to_string()
            } else {
                other.join(" ")
            }
        ))),
    }
}

fn bg_lifecycle(args: &CliArgs, action: &str) -> CommandResult<Value> {
    let bg = battlegroup_ref(args)?;
    let timeout = args.optional_u64("--director-timeout")?.unwrap_or(60);
    let kube = StructuredKubectl::new(ssh_runner(args)?);
    let orchestrator = BattlegroupManagementOrchestrator::new(kube);
    let mut sink = VecOperationSink::default();
    let director_port = match action {
        "start" => orchestrator.start_and_wait_director(&bg, timeout, &mut sink)?,
        "stop" => {
            orchestrator.stop(&bg, &mut sink)?;
            None
        }
        "restart" => orchestrator.restart_and_wait_director(&bg, timeout, &mut sink)?,
        _ => unreachable!("validated by caller"),
    };
    Ok(json!({
        "ok": true,
        "directorNodePort": director_port,
        "events": sink.events,
    }))
}

fn bg_ops(args: &CliArgs) -> CommandResult<StructuredBattlegroupOps<OpenSshRunner>> {
    Ok(StructuredBattlegroupOps::new(ssh_runner(args)?))
}

fn toolchain(args: &CliArgs) -> CommandResult<Toolchain> {
    if let Some(root) = args.optional("--tools-root") {
        Ok(Toolchain::new(root))
    } else {
        Toolchain::from_default_root()
    }
}

fn optional_port(args: &CliArgs, name: &str) -> CommandResult<Option<u16>> {
    args.optional_u64(name)?
        .map(|value| {
            u16::try_from(value).map_err(|_| failure(format!("{name} must fit in a TCP port")))
        })
        .transpose()
}

fn db_config(args: &CliArgs) -> CommandResult<DuneDatabaseConfig> {
    let host = args.required("--db-host")?;
    let port = optional_port(args, "--db-port")?.unwrap_or(DEFAULT_DUNE_DATABASE_PORT);
    let database = args
        .optional("--db-name")
        .unwrap_or_else(|| "dune".to_string());
    let user = args
        .optional("--db-user")
        .unwrap_or_else(|| "dune".to_string());
    let password = db_password(args)?.unwrap_or_else(|| "dune".to_string());
    Ok(DuneDatabaseConfig {
        host,
        port,
        database,
        user,
        password,
    })
}

fn db_password(args: &CliArgs) -> CommandResult<Option<String>> {
    if let Some(value) = args.optional("--db-password") {
        return Ok(Some(value));
    }
    if let Some(path) = args.optional("--db-password-file") {
        let text = std::fs::read_to_string(&path).map_err(|err| {
            failure(format!(
                "Failed to read database password file {path}: {err}"
            ))
        })?;
        let password = text.trim_end_matches(['\r', '\n']).to_string();
        if password.is_empty() {
            return Err(failure("Database password file is empty"));
        }
        return Ok(Some(password));
    }
    if let Some(name) = args.optional("--db-password-env") {
        let password = std::env::var(&name)
            .map_err(|_| failure(format!("Environment variable {name} is not set")))?;
        if password.is_empty() {
            return Err(failure(format!("Environment variable {name} is empty")));
        }
        return Ok(Some(password));
    }
    Ok(None)
}

fn battlegroup_ref(args: &CliArgs) -> CommandResult<BattlegroupRef> {
    Ok(BattlegroupRef {
        namespace: args.required("--namespace")?,
        name: args.required("--name")?,
    })
}

fn optional_battlegroup_ref(args: &CliArgs) -> CommandResult<Option<BattlegroupRef>> {
    match (args.optional("--namespace"), args.optional("--name")) {
        (Some(namespace), Some(name))
            if !namespace.trim().is_empty() && !name.trim().is_empty() =>
        {
            Ok(Some(BattlegroupRef { namespace, name }))
        }
        (None, None) => Ok(None),
        _ => Err(failure("--namespace and --name must be provided together")),
    }
}

fn ssh_guest_provider(args: &CliArgs) -> CommandResult<OpenSshGuestProvider> {
    Ok(OpenSshGuestProvider::new(
        args.required("--ssh")?,
        args.required("--key")?,
        args.optional("--user")
            .unwrap_or_else(|| "dune".to_string()),
    ))
}

fn ssh_runner(args: &CliArgs) -> CommandResult<OpenSshRunner> {
    Ok(OpenSshRunner::new(OpenSshTarget::new(
        args.required("--ssh")?,
        args.required("--key")?,
        args.optional("--user")
            .unwrap_or_else(|| "dune".to_string()),
        args.required("--host")?,
    )))
}

fn operation_ok(sink: VecOperationSink) -> CommandResult<Value> {
    Ok(json!({
        "ok": true,
        "events": sink.events,
    }))
}

fn to_json(value: impl Serialize) -> CommandResult<Value> {
    serde_json::to_value(value).map_err(|err| failure(format!("Failed to serialize output: {err}")))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct OperationOutput<T> {
    ok: bool,
    result: T,
    events: Vec<OrchestrationEvent>,
}

#[derive(Debug, Clone)]
struct CliArgs {
    args: Vec<String>,
}

impl CliArgs {
    fn new(args: Vec<String>) -> Self {
        Self { args }
    }

    fn is_empty(&self) -> bool {
        self.args.is_empty()
    }

    fn positional_slice(&self) -> Vec<&str> {
        let mut values = Vec::new();
        let mut index = 0;
        while index < self.args.len() {
            let arg = &self.args[index];
            if arg.starts_with("--") {
                if self
                    .args
                    .get(index + 1)
                    .is_some_and(|next| !next.starts_with("--"))
                {
                    index += 2;
                } else {
                    index += 1;
                }
            } else {
                values.push(arg.as_str());
                index += 1;
            }
        }
        values
    }

    fn has_flag(&self, name: &str) -> bool {
        self.args.iter().any(|arg| arg == name)
    }

    fn required(&self, name: &str) -> CommandResult<String> {
        self.optional(name)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| failure(format!("Missing required argument {name}")))
    }

    fn optional(&self, name: &str) -> Option<String> {
        self.args
            .windows(2)
            .find(|pair| pair[0] == name)
            .map(|pair| pair[1].clone())
    }

    fn optional_u64(&self, name: &str) -> CommandResult<Option<u64>> {
        self.optional(name)
            .map(|value| {
                value
                    .parse::<u64>()
                    .map_err(|_| failure(format!("{name} must be an unsigned integer")))
            })
            .transpose()
    }

    fn required_u64(&self, name: &str) -> CommandResult<u64> {
        self.optional_u64(name)?
            .ok_or_else(|| failure(format!("Missing required argument {name}")))
    }

    fn token(&self) -> CommandResult<String> {
        if let Some(value) = self.optional("--token") {
            return Ok(value);
        }
        if let Some(path) = self.optional("--token-file") {
            let text = std::fs::read_to_string(&path)
                .map_err(|err| failure(format!("Failed to read token file {path}: {err}")))?;
            let token = text.trim().to_string();
            if token.is_empty() {
                return Err(failure("Token file is empty"));
            }
            return Ok(token);
        }
        if let Some(name) = self.optional("--token-env") {
            let token = std::env::var(&name)
                .map_err(|_| failure(format!("Environment variable {name} is not set")))?;
            let token = token.trim().to_string();
            if token.is_empty() {
                return Err(failure(format!("Environment variable {name} is empty")));
            }
            return Ok(token);
        }
        Err(failure(
            "Missing token; use --token, --token-file, or --token-env",
        ))
    }
}

fn usage() -> Vec<&'static str> {
    vec![
        "dune-manager-cli flow initial",
        "dune-manager-cli flow battlegroup",
        "dune-manager-cli host environment",
        "dune-manager-cli host readiness",
        "dune-manager-cli host drives [--min-gb 100]",
        "dune-manager-cli host adapters",
        "dune-manager-cli db ping --db-host IP [--db-port 15432] [--db-name dune] [--db-user dune] [--db-password PASSWORD | --db-password-file PATH | --db-password-env NAME]",
        "dune-manager-cli db world-partitions --db-host IP [--map MAP] [--db-port 15432] [--db-name dune] [--db-user dune] [--db-password PASSWORD | --db-password-file PATH | --db-password-env NAME]",
        "dune-manager-cli tools status [--tool steamcmd|openssh] [--tools-root PATH]",
        "dune-manager-cli tools install --tool steamcmd|openssh|all [--tools-root PATH] [--force] [--source-url URL]",
        "dune-manager-cli tools path --tool steamcmd|openssh [--tools-root PATH]",
        "dune-manager-cli vm get --name NAME",
        "dune-manager-cli vm list",
        "dune-manager-cli vm detect-dune",
        "dune-manager-cli vm start --name NAME",
        "dune-manager-cli vm stop --name NAME",
        "dune-manager-cli setup create-vm --install-path PATH --destination PATH --vm-name NAME --switch NAME --adapter NAME [--memory-gb 20] [--processors 4] [--disk-gb 100] [--replace-existing] [--clear-destination]",
        "dune-manager-cli ssh shell-spec --ssh PATH --key PATH --host IP [--user dune]",
        "dune-manager-cli token plan (--token JWT | --token-file PATH | --token-env NAME) --player-ip IP --world-name NAME [--region \"Europe Test\"]",
        "dune-manager-cli guest player-candidates --ssh PATH --key PATH --host IP [--user dune]",
        "dune-manager-cli guest write-player-settings --ssh PATH --key PATH --host IP --player-ip IP [--user dune]",
        "dune-manager-cli guest apply-static-network --ssh PATH --key PATH --host IP --address-cidr IP/PREFIX --gateway IP --dns IP [--interface eth0] [--user dune]",
        "dune-manager-cli guest bootstrap --ssh PATH --key PATH --host IP (--token JWT | --token-file PATH | --token-env NAME) --player-ip IP --world-name NAME [--region \"Europe Test\"] [--enable-experimental-swap] [--experimental-swap-size-gib 30] [--experimental-swap-no-restart-k3s] [--user dune]",
        "dune-manager-cli guest experimental-swap status --ssh PATH --key PATH --host IP [--namespace NS --name BG] [--user dune]",
        "dune-manager-cli guest experimental-swap enable --ssh PATH --key PATH --host IP --namespace NS --name BG [--swap-size-gib 30] [--no-restart-k3s] [--user dune]",
        "dune-manager-cli bg list --ssh PATH --key PATH --host IP [--user dune]",
        "dune-manager-cli bg status --ssh PATH --key PATH --host IP --namespace NS --name BG [--user dune]",
        "dune-manager-cli bg start|stop|restart --ssh PATH --key PATH --host IP --namespace NS --name BG [--director-timeout 60]",
        "dune-manager-cli bg patch-region --ssh PATH --key PATH --host IP --namespace NS --name BG --region \"Europe Test\"",
        "dune-manager-cli bg instances set --ssh PATH --key PATH --host IP --namespace NS --name BG --map survival-1|deep-desert --count N [--pvp-count N] [--restart]",
        "dune-manager-cli bg pods --ssh PATH --key PATH --host IP --namespace NS",
        "dune-manager-cli bg pod-shell-spec --ssh PATH --key PATH --host IP --namespace NS --pod POD",
        "dune-manager-cli bg export-logs --ssh PATH --key PATH --host IP --namespace NS",
        "dune-manager-cli bg export-operator-logs --ssh PATH --key PATH --host IP",
        "dune-manager-cli bg update --ssh PATH --key PATH --host IP --namespace NS --name BG",
        "dune-manager-cli bg file-browser-url --ssh PATH --key PATH --host IP --vm-ip IP",
        "dune-manager-cli bg director-url --ssh PATH --key PATH --host IP --namespace NS --name BG --vm-ip IP",
        "dune-manager-cli manager install --ssh PATH --key PATH --host IP --binary PATH --namespace NS (--token TOKEN | --token-file PATH | --token-env NAME) [--port 8787] [--director-base-url URL] [--kubeconfig-path /etc/rancher/k3s/k3s.yaml] [--user dune]",
        "dune-manager-cli manager status --ssh PATH --key PATH --host IP [--port 8787] [--user dune]",
    ]
}

fn memory_profile(memory_gb: u64) -> CommandResult<MemoryProfile> {
    match memory_gb {
        20 => Ok(MemoryProfile::Sietch20Gb),
        30 => Ok(MemoryProfile::SietchStory30Gb),
        40 => Ok(MemoryProfile::SietchStoryDeepDesert40Gb),
        custom => Ok(MemoryProfile::CustomBytes(
            custom.saturating_mul(1024 * 1024 * 1024),
        )),
    }
}

impl From<CommandFailure> for Value {
    fn from(value: CommandFailure) -> Self {
        json!({
            "ok": false,
            "error": value.message,
            "stdout": value.stdout,
            "stderr": value.stderr,
            "code": value.code,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prints_usage_when_no_args_are_supplied() {
        let value = run_cli(vec![]).unwrap();
        assert_eq!(value["ok"], true);
        assert!(value["usage"].as_array().unwrap().len() > 5);
    }

    #[test]
    fn token_plan_redacts_original_token_from_output() {
        let value = run_cli(vec![
            "token".into(),
            "plan".into(),
            "--token".into(),
            "e30.eyJIb3N0SWQiOiJBQkMxMjMifQ.sig".into(),
            "--player-ip".into(),
            "10.0.0.4".into(),
            "--world-name".into(),
            "Adain".into(),
        ])
        .unwrap();

        let text = serde_json::to_string(&value).unwrap();
        assert_eq!(value["hostId"], "abc123");
        assert!(!text.contains("eyJIb3N0SWQi"));
    }

    #[test]
    fn token_plan_can_read_token_from_file() {
        let path = std::env::temp_dir().join("dune-manager-token-plan-test.jwt");
        std::fs::write(&path, "e30.eyJIb3N0SWQiOiJBQkMxMjMifQ.sig\n").unwrap();
        let value = run_cli(vec![
            "token".into(),
            "plan".into(),
            "--token-file".into(),
            path.to_string_lossy().to_string(),
            "--player-ip".into(),
            "10.0.0.4".into(),
            "--world-name".into(),
            "Adain".into(),
        ])
        .unwrap();
        let _ = std::fs::remove_file(path);

        assert_eq!(value["hostId"], "abc123");
    }

    #[test]
    fn missing_required_arg_fails_cleanly() {
        let err = run_cli(vec!["vm".into(), "get".into()]).unwrap_err();
        assert!(err.message.contains("--name"));
    }

    #[test]
    fn memory_profile_maps_vendor_sizes() {
        assert_eq!(memory_profile(20).unwrap().bytes(), 20 * 1024 * 1024 * 1024);
        assert_eq!(memory_profile(30).unwrap().bytes(), 30 * 1024 * 1024 * 1024);
        assert_eq!(memory_profile(40).unwrap().bytes(), 40 * 1024 * 1024 * 1024);
    }
}
