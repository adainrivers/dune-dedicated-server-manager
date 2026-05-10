use serde_json::Value;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::AppHandle;

mod config_store;
mod errors;
mod models;
mod security;
mod shell;
mod ssh;
mod validation;

use config_store::*;
use errors::*;
use models::*;
use security::*;
use shell::*;
use ssh::*;
use validation::*;

fn configured_vm_name(app: &AppHandle, vm_name: Option<String>) -> CommandResult<String> {
    let config = read_app_config(app)?;
    required_config_value(vm_name, &config.vm_name, "VM name")
}

fn resolve_connection(
    app: &AppHandle,
    install_path: Option<String>,
    ip: Option<String>,
    ssh_user: Option<String>,
) -> CommandResult<(String, String, String)> {
    let config = read_app_config(app)?;
    let install_path = required_config_value(install_path, &config.install_path, "Install path")?;
    let ip = ip
        .or_else(|| discover_ip_from_logs(&install_path))
        .unwrap_or_else(|| config.vm_ip.clone())
        .trim()
        .to_string();
    if ip.is_empty() {
        return Err(failure("VM IP is not configured"));
    }
    let ssh_user = required_config_value(ssh_user, &config.ssh_user, "SSH user")?;
    Ok((install_path, ip, ssh_user))
}

#[tauri::command]
fn get_app_config(app: AppHandle) -> CommandResult<AppConfig> {
    read_app_config(&app)
}

#[tauri::command]
fn save_app_config(app: AppHandle, config: AppConfig) -> CommandResult<AppConfig> {
    write_app_config(&app, config)
}

#[tauri::command]
fn detect_app_config(app: AppHandle) -> CommandResult<AppConfig> {
    let mut config = read_app_config(&app)?;
    let detected = detect_host_config();
    config.install_path = first_non_empty(config.install_path, detected.install_path);
    config.vm_name = first_non_empty(config.vm_name, detected.vm_name);
    config.vm_ip = first_non_empty(config.vm_ip, detected.vm_ip);
    config.ssh_path = first_non_empty(config.ssh_path, detected.ssh_path);
    config.ssh_user = first_non_empty(config.ssh_user, Some("dune".to_string()));
    config.manager_api_binary_path =
        first_non_empty(config.manager_api_binary_path, detect_manager_binary_path());
    if config.manager_api_url.is_empty() && !config.vm_ip.is_empty() {
        config.manager_api_url = format!("http://{}:8787", config.vm_ip);
    }
    write_app_config(&app, config)
}

#[tauri::command]
fn get_host_status(app: AppHandle) -> CommandResult<HostStatus> {
    let config = read_app_config(&app).unwrap_or_default();
    let script = format!(
        r#"
$principal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
$vmms = Get-Service -Name vmms -ErrorAction SilentlyContinue
[pscustomobject]@{{
  user = [Security.Principal.WindowsIdentity]::GetCurrent().Name
  isElevated = $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
  hypervAvailable = [bool](Get-Command Get-VM -ErrorAction SilentlyContinue)
  vmmsStatus = if ($vmms) {{ $vmms.Status.ToString() }} else {{ $null }}
  sshAvailable = Test-Path {ssh}
  defaultInstallPathExists = Test-Path {install}
  defaultInstallPath = {install}
}} | ConvertTo-Json -Compress
"#,
        ssh = ps_single_quoted(&config.ssh_path),
        install = ps_single_quoted(&config.install_path)
    );
    parse_json(&run_powershell(&script)?, "host status")
}

#[tauri::command]
fn get_vm_status(app: AppHandle, vm_name: Option<String>) -> CommandResult<VmStatus> {
    let vm_name = configured_vm_name(&app, vm_name)?;
    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
$vmName = {vm_name}
$vm = Get-VM -Name $vmName
$ips = @((Get-VMNetworkAdapter -VMName $vmName).IPAddresses | Where-Object {{ $_ -match '^\d+\.\d+\.\d+\.\d+$' }})
[pscustomobject]@{{
  name = $vm.Name
  state = $vm.State.ToString()
  status = $vm.Status
  memoryAssignedBytes = [uint64]$vm.MemoryAssigned
  uptime = $vm.Uptime.ToString()
  path = $vm.Path
  configurationLocation = $vm.ConfigurationLocation
  ipAddresses = $ips
}} | ConvertTo-Json -Compress
"#,
        vm_name = ps_single_quoted(&vm_name)
    );
    parse_json(&run_powershell(&script)?, "VM status")
}

#[tauri::command]
fn start_vm(app: AppHandle, vm_name: Option<String>) -> CommandResult<VmStatus> {
    let vm_name = configured_vm_name(&app, vm_name)?;
    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
$vmName = {vm_name}
Start-VM -Name $vmName -AsJob | Out-Null
"#,
        vm_name = ps_single_quoted(&vm_name)
    );
    run_powershell(&script)?;
    get_vm_status(app, Some(vm_name))
}

#[tauri::command]
fn stop_vm(app: AppHandle, vm_name: Option<String>) -> CommandResult<VmStatus> {
    let vm_name = configured_vm_name(&app, vm_name)?;
    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
$vmName = {vm_name}
Stop-VM -Name $vmName -Force -AsJob | Out-Null
"#,
        vm_name = ps_single_quoted(&vm_name)
    );
    run_powershell(&script)?;
    get_vm_status(app, Some(vm_name))
}

#[tauri::command]
fn connect_guest(
    app: AppHandle,
    install_path: Option<String>,
    ip: Option<String>,
    ssh_user: Option<String>,
) -> CommandResult<GuestConnection> {
    let (install_path, ip, ssh_user) = resolve_connection(&app, install_path, ip, ssh_user)?;
    let key = prepare_key(&app, &install_path)?;

    let identity = run_ssh(
        &app,
        &install_path,
        &ip,
        &ssh_user,
        "hostname; uname -r; sudo -n true && echo SUDO_OK; sudo kubectl version --client=true >/dev/null 2>&1 && echo KUBECTL_OK",
    )?;

    let mut lines = identity.lines();
    let hostname = lines.next().unwrap_or_default().to_string();
    let kernel = lines.next().unwrap_or_default().to_string();
    let sudo = identity.contains("SUDO_OK");
    let kubectl = identity.contains("KUBECTL_OK");

    Ok(GuestConnection {
        ip,
        ssh_user,
        key_path: key.to_string_lossy().to_string(),
        connected: true,
        sudo,
        hostname,
        kernel,
        kubectl,
    })
}

fn get_bg_json(
    app: &AppHandle,
    install_path: &str,
    ip: &str,
    ssh_user: &str,
) -> CommandResult<Value> {
    let command = "sudo kubectl get battlegroup -A -o json";
    let raw = run_ssh(app, install_path, ip, ssh_user, command)?;
    parse_json(&raw, "battlegroup list")
}

fn summarize_server_sets(item: &Value) -> Vec<ServerSetSummary> {
    item["spec"]["serverGroup"]["template"]["spec"]["sets"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|set| ServerSetSummary {
            map: set["map"].as_str().unwrap_or_default().to_string(),
            replicas: set["replicas"].as_u64().unwrap_or_default(),
            memory_limit: set["resources"]["limits"]["memory"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            dedicated_scaling: set["dedicatedScaling"].as_bool().unwrap_or(false),
            image: set["image"].as_str().unwrap_or_default().to_string(),
        })
        .collect()
}

fn unique_strings(values: impl Iterator<Item = String>) -> Vec<String> {
    let mut output = Vec::new();
    for value in values {
        if !value.is_empty() && !output.contains(&value) {
            output.push(value);
        }
    }
    output
}

fn detail_from_battlegroup(item: &Value) -> BattleGroupDetail {
    let namespace = item["metadata"]["namespace"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let name = item["metadata"]["name"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let server_sets = summarize_server_sets(item);
    let server_image = server_sets
        .first()
        .map(|set| set.image.clone())
        .unwrap_or_default();

    let mut utility_images = Vec::new();
    for path in [
        &item["spec"]["utilities"]["director"]["spec"]["image"],
        &item["spec"]["utilities"]["serverGateway"]["spec"]["image"],
        &item["spec"]["utilities"]["textRouter"]["spec"]["image"],
        &item["spec"]["utilities"]["fileBrowser"]["spec"]["image"],
    ] {
        if let Some(image) = path.as_str() {
            utility_images.push(image.to_string());
        }
    }
    for template in item["spec"]["utilities"]["messageQueues"]["templates"]
        .as_array()
        .cloned()
        .unwrap_or_default()
    {
        if let Some(image) = template["spec"]["image"].as_str() {
            utility_images.push(image.to_string());
        }
    }

    BattleGroupDetail {
        namespace,
        name,
        title: item["spec"]["title"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        phase: item["status"]["phase"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        stop: item["spec"]["stop"].as_bool().unwrap_or(false),
        database_phase: item["status"]["database"]["phase"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        server_group_phase: item["status"]["serverGroup"]["phase"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        gateway_phase: item["status"]["serverGateway"]["phase"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        director_phase: item["status"]["director"]["phase"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        server_image,
        utility_images: unique_strings(utility_images.into_iter()),
        server_sets,
    }
}

#[tauri::command]
fn get_battlegroups(
    app: AppHandle,
    install_path: Option<String>,
    ip: Option<String>,
    ssh_user: Option<String>,
) -> CommandResult<Vec<BattleGroupSummary>> {
    let (install_path, ip, ssh_user) = resolve_connection(&app, install_path, ip, ssh_user)?;

    let value = get_bg_json(&app, &install_path, &ip, &ssh_user)?;
    let mut groups = Vec::new();
    for item in value["items"].as_array().cloned().unwrap_or_default() {
        let namespace = item["metadata"]["namespace"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let name = item["metadata"]["name"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let title = item["spec"]["title"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let phase = item["status"]["phase"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let stop = item["spec"]["stop"].as_bool().unwrap_or(false);
        let server_sets = item["spec"]["serverGroup"]["template"]["spec"]["sets"]
            .as_array()
            .map(|sets| sets.len())
            .unwrap_or_default();
        let server_image = item["spec"]["serverGroup"]["template"]["spec"]["sets"]
            .as_array()
            .and_then(|sets| sets.first())
            .and_then(|set| set["image"].as_str())
            .unwrap_or_default()
            .to_string();

        validate_kube_arg(&namespace, "namespace")?;
        let services_raw = run_ssh(
            &app,
            &install_path,
            &ip,
            &ssh_user,
            &format!("sudo kubectl get svc -n {namespace} -o json"),
        )?;
        let services: Value = parse_json(&services_raw, "services")?;
        let mut file_browser_url = None;
        let mut director_url = None;
        for svc in services["items"].as_array().cloned().unwrap_or_default() {
            let svc_name = svc["metadata"]["name"].as_str().unwrap_or_default();
            for port in svc["spec"]["ports"].as_array().cloned().unwrap_or_default() {
                let port_number = port["port"].as_u64().unwrap_or_default();
                let node_port = port["nodePort"].as_u64();
                if svc_name.ends_with("-fb-svc") || port_number == 18888 {
                    file_browser_url = Some(format!("http://{ip}:18888/"));
                }
                if port_number == 11717 {
                    if let Some(node_port) = node_port {
                        director_url = Some(format!("http://{ip}:{node_port}/"));
                    }
                }
            }
        }

        groups.push(BattleGroupSummary {
            namespace,
            name,
            title,
            phase,
            stop,
            server_image,
            file_browser_url,
            director_url,
            server_sets,
        });
    }
    Ok(groups)
}

#[tauri::command]
fn get_battlegroup_detail(
    app: AppHandle,
    namespace: String,
    name: String,
    install_path: Option<String>,
    ip: Option<String>,
    ssh_user: Option<String>,
) -> CommandResult<BattleGroupDetail> {
    validate_kube_arg(&namespace, "namespace")?;
    validate_kube_arg(&name, "name")?;
    let (install_path, ip, ssh_user) = resolve_connection(&app, install_path, ip, ssh_user)?;
    let raw = run_ssh(
        &app,
        &install_path,
        &ip,
        &ssh_user,
        &format!("sudo kubectl get battlegroup {name} -n {namespace} -o json"),
    )?;
    let value: Value = parse_json(&raw, "live BattleGroup")?;
    Ok(detail_from_battlegroup(&value))
}

#[tauri::command]
fn get_workloads(
    app: AppHandle,
    namespace: String,
    install_path: Option<String>,
    ip: Option<String>,
    ssh_user: Option<String>,
) -> CommandResult<WorkloadList> {
    validate_kube_arg(&namespace, "namespace")?;
    let (install_path, ip, ssh_user) = resolve_connection(&app, install_path, ip, ssh_user)?;

    let pods = run_ssh(
        &app,
        &install_path,
        &ip,
        &ssh_user,
        &format!("sudo kubectl get pods -n {namespace} -o json"),
    )?;
    let services = run_ssh(
        &app,
        &install_path,
        &ip,
        &ssh_user,
        &format!("sudo kubectl get svc -n {namespace} -o json"),
    )?;

    Ok(WorkloadList {
        pods: parse_json(&pods, "pods")?,
        services: parse_json(&services, "services")?,
    })
}

fn patch_battlegroup_stop(
    app: &AppHandle,
    namespace: &str,
    name: &str,
    stop: bool,
    install_path: &str,
    ip: &str,
    ssh_user: &str,
) -> CommandResult<()> {
    validate_kube_arg(namespace, "namespace")?;
    validate_kube_arg(name, "name")?;
    let patch = if stop { "true" } else { "false" };
    let remote = format!(
        "sudo kubectl patch battlegroup {name} -n {namespace} --type=merge -p '{{\"spec\":{{\"stop\":{patch}}}}}'"
    );
    run_ssh(app, install_path, ip, ssh_user, &remote)?;
    Ok(())
}

#[tauri::command]
fn start_battlegroup(
    app: AppHandle,
    namespace: String,
    name: String,
    install_path: Option<String>,
    ip: Option<String>,
    ssh_user: Option<String>,
) -> CommandResult<()> {
    let (install_path, ip, ssh_user) = resolve_connection(&app, install_path, ip, ssh_user)?;
    patch_battlegroup_stop(
        &app,
        &namespace,
        &name,
        false,
        &install_path,
        &ip,
        &ssh_user,
    )
}

#[tauri::command]
fn stop_battlegroup(
    app: AppHandle,
    namespace: String,
    name: String,
    install_path: Option<String>,
    ip: Option<String>,
    ssh_user: Option<String>,
) -> CommandResult<()> {
    let (install_path, ip, ssh_user) = resolve_connection(&app, install_path, ip, ssh_user)?;
    patch_battlegroup_stop(&app, &namespace, &name, true, &install_path, &ip, &ssh_user)
}

#[tauri::command]
fn restart_battlegroup(
    app: AppHandle,
    namespace: String,
    name: String,
    install_path: Option<String>,
    ip: Option<String>,
    ssh_user: Option<String>,
) -> CommandResult<()> {
    let (install_path, ip, ssh_user) = resolve_connection(&app, install_path, ip, ssh_user)?;
    patch_battlegroup_stop(&app, &namespace, &name, true, &install_path, &ip, &ssh_user)?;
    std::thread::sleep(std::time::Duration::from_secs(5));
    patch_battlegroup_stop(
        &app,
        &namespace,
        &name,
        false,
        &install_path,
        &ip,
        &ssh_user,
    )
}

#[tauri::command]
fn export_live_config(
    app: AppHandle,
    namespace: String,
    name: String,
    install_path: Option<String>,
    ip: Option<String>,
    ssh_user: Option<String>,
) -> CommandResult<ConfigSnapshot> {
    validate_kube_arg(&namespace, "namespace")?;
    validate_kube_arg(&name, "name")?;
    let (install_path, ip, ssh_user) = resolve_connection(&app, install_path, ip, ssh_user)?;
    let raw = run_ssh(
        &app,
        &install_path,
        &ip,
        &ssh_user,
        &format!("sudo kubectl get battlegroup {name} -n {namespace} -o json"),
    )?;

    let snapshots = app_data_dir(&app)?.join("snapshots");
    fs::create_dir_all(&snapshots)
        .map_err(|err| failure(format!("Failed to create snapshots directory: {err}")))?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();
    let file_name = format!("{name}-live-{timestamp}.json");
    let path = snapshots.join(file_name);
    let mut value: Value = parse_json(&raw, "live BattleGroup")?;
    redact_json(&mut value);
    let snapshot = serde_json::to_string_pretty(&value)
        .map_err(|err| failure(format!("Failed to serialize snapshot: {err}")))?;
    fs::write(&path, snapshot)
        .map_err(|err| failure(format!("Failed to write snapshot: {err}")))?;

    Ok(ConfigSnapshot {
        file_path: path.to_string_lossy().to_string(),
    })
}

#[tauri::command]
fn install_manager_api(
    app: AppHandle,
    namespace: String,
    binary_path: String,
    token: String,
    director_base_url: String,
    install_path: Option<String>,
    ip: Option<String>,
    ssh_user: Option<String>,
) -> CommandResult<ManagerApiInstallResult> {
    let namespace = namespace.trim().to_string();
    let binary_path = binary_path.trim().to_string();
    let token = token.trim().to_string();
    let director_base_url = director_base_url.trim().trim_end_matches('/').to_string();
    validate_kube_arg(&namespace, "namespace")?;
    validate_plain_value(&binary_path, "Manager API binary")?;
    validate_plain_value(&token, "Manager API token")?;
    if !director_base_url.is_empty() {
        validate_plain_value(&director_base_url, "Director base URL")?;
    }

    let (install_path, ip, ssh_user) = resolve_connection(&app, install_path, ip, ssh_user)?;
    let upload_path = format!("/home/{ssh_user}/dune-manager-api");
    copy_to_guest(
        &app,
        &install_path,
        &ip,
        &ssh_user,
        &binary_path,
        &upload_path,
    )?;

    let install_script = format!(
        r#"set -eu
install -d -m 0755 /opt/dune-manager
install -m 0755 {upload_path} /opt/dune-manager/dune-manager-api
rm -f {upload_path}
cat > /etc/dune-manager-api.env <<'EOF'
MANAGER_API_TOKEN={token}
DUNE_NAMESPACE={namespace}
KUBECONFIG=/etc/rancher/k3s/k3s.yaml
DIRECTOR_BASE_URL={director_base_url}
PORT=8787
RUST_LOG=dune_manager_api=info,tower_http=info
EOF
chmod 0600 /etc/dune-manager-api.env
cat > /opt/dune-manager/run-manager-api <<'EOF'
#!/bin/sh
set -a
. /etc/dune-manager-api.env
set +a
exec /opt/dune-manager/dune-manager-api
EOF
chmod 0755 /opt/dune-manager/run-manager-api
cat > /etc/init.d/dune-manager-api <<'EOF'
#!/sbin/openrc-run
name="Dune Manager API"
description="Dune dedicated server manager guest service"
command="/opt/dune-manager/run-manager-api"
command_background="yes"
pidfile="/run/dune-manager-api.pid"
output_log="/var/log/dune-manager-api.log"
error_log="/var/log/dune-manager-api.log"
depend() {{
  need net
  after k3s
}}
EOF
chmod 0755 /etc/init.d/dune-manager-api
rc-update add dune-manager-api default >/dev/null 2>&1 || true
rc-service dune-manager-api restart
"#,
        upload_path = upload_path,
        token = token,
        namespace = namespace,
        director_base_url = director_base_url,
    );

    run_ssh_with_stdin(
        &app,
        &install_path,
        &ip,
        &ssh_user,
        "sudo sh -s",
        &install_script,
    )?;
    run_ssh(
        &app,
        &install_path,
        &ip,
        &ssh_user,
        "sudo rc-service dune-manager-api status",
    )?;

    Ok(ManagerApiInstallResult {
        namespace,
        deployment: "dune-manager-api".to_string(),
        service: "openrc".to_string(),
        binary_path,
        url: format!("http://{ip}:8787"),
    })
}

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_app_config,
            save_app_config,
            detect_app_config,
            get_host_status,
            get_vm_status,
            start_vm,
            stop_vm,
            connect_guest,
            get_battlegroups,
            get_battlegroup_detail,
            get_workloads,
            start_battlegroup,
            stop_battlegroup,
            restart_battlegroup,
            export_live_config,
            install_manager_api
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
