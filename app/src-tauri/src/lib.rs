use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::Write;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Manager};

#[derive(Debug, Serialize)]
struct CommandFailure {
    message: String,
    stdout: String,
    stderr: String,
    code: Option<i32>,
}

type CommandResult<T> = Result<T, CommandFailure>;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HostStatus {
    user: String,
    is_elevated: bool,
    hyperv_available: bool,
    vmms_status: Option<String>,
    ssh_available: bool,
    default_install_path_exists: bool,
    default_install_path: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
#[serde(default)]
struct AppConfig {
    install_path: String,
    vm_name: String,
    vm_ip: String,
    ssh_user: String,
    ssh_path: String,
    manager_api_url: String,
    manager_api_token: String,
    manager_api_namespace: String,
    manager_api_image: String,
    manager_api_binary_path: String,
    manager_api_director_url: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            install_path: String::new(),
            vm_name: String::new(),
            vm_ip: String::new(),
            ssh_user: String::new(),
            ssh_path: String::new(),
            manager_api_url: String::new(),
            manager_api_token: String::new(),
            manager_api_namespace: String::new(),
            manager_api_image: String::new(),
            manager_api_binary_path: String::new(),
            manager_api_director_url: String::new(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VmStatus {
    name: String,
    state: String,
    status: String,
    memory_assigned_bytes: u64,
    uptime: String,
    path: String,
    configuration_location: String,
    ip_addresses: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GuestConnection {
    ip: String,
    ssh_user: String,
    key_path: String,
    connected: bool,
    sudo: bool,
    hostname: String,
    kernel: String,
    kubectl: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BattleGroupSummary {
    namespace: String,
    name: String,
    title: String,
    phase: String,
    stop: bool,
    server_image: String,
    file_browser_url: Option<String>,
    director_url: Option<String>,
    server_sets: usize,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ServerSetSummary {
    map: String,
    replicas: u64,
    memory_limit: String,
    dedicated_scaling: bool,
    image: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BattleGroupDetail {
    namespace: String,
    name: String,
    title: String,
    phase: String,
    stop: bool,
    database_phase: String,
    server_group_phase: String,
    gateway_phase: String,
    director_phase: String,
    server_image: String,
    utility_images: Vec<String>,
    server_sets: Vec<ServerSetSummary>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkloadList {
    pods: Value,
    services: Value,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConfigSnapshot {
    file_path: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManagerApiInstallResult {
    namespace: String,
    deployment: String,
    service: String,
    binary_path: String,
    url: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct DetectedConfig {
    install_path: Option<String>,
    vm_name: Option<String>,
    vm_ip: Option<String>,
    ssh_path: Option<String>,
}

fn failure(message: impl Into<String>) -> CommandFailure {
    CommandFailure {
        message: message.into(),
        stdout: String::new(),
        stderr: String::new(),
        code: None,
    }
}

fn command_failure(message: impl Into<String>, output: std::process::Output) -> CommandFailure {
    CommandFailure {
        message: message.into(),
        stdout: redact_text(&String::from_utf8_lossy(&output.stdout))
            .trim()
            .to_string(),
        stderr: redact_text(&String::from_utf8_lossy(&output.stderr))
            .trim()
            .to_string(),
        code: output.status.code(),
    }
}

fn redact_text(input: &str) -> String {
    let mut output = Vec::new();
    for line in input.lines() {
        let lower = line.to_ascii_lowercase();
        if lower.contains("token")
            || lower.contains("secret")
            || lower.contains("password")
            || lower.contains("apikey")
            || lower.contains("api_key")
            || lower.contains("serviceauth")
        {
            output.push("<redacted>".to_string());
        } else {
            output.push(line.to_string());
        }
    }
    output.join("\n")
}

fn is_sensitive_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    lower.contains("token")
        || lower.contains("secret")
        || lower.contains("password")
        || lower.contains("apikey")
        || lower.contains("api_key")
        || lower.contains("auth")
        || lower == "key"
}

fn looks_like_jwt(value: &str) -> bool {
    let mut parts = value.split('.');
    matches!(
        (parts.next(), parts.next(), parts.next(), parts.next()),
        (Some(a), Some(b), Some(c), None) if a.len() > 8 && b.len() > 8 && c.len() > 8
    )
}

fn redact_json(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                if is_sensitive_key(key) {
                    *child = Value::String("<redacted>".to_string());
                } else {
                    redact_json(child);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                redact_json(item);
            }
        }
        Value::String(text) if looks_like_jwt(text) => {
            *text = "<redacted>".to_string();
        }
        Value::String(text) => {
            if text.contains("ServiceAuthToken=") {
                *text = "<redacted>".to_string();
            }
        }
        _ => {}
    }
}

fn run_program(program: &str, args: &[&str]) -> CommandResult<String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|err| failure(format!("Failed to run {program}: {err}")))?;

    if !output.status.success() {
        return Err(command_failure(
            format!("{program} exited with an error"),
            output,
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn run_powershell(script: &str) -> CommandResult<String> {
    run_program(
        "powershell",
        &[
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ],
    )
}

fn parse_json<T: for<'de> Deserialize<'de>>(text: &str, label: &str) -> CommandResult<T> {
    serde_json::from_str(text).map_err(|err| failure(format!("Failed to parse {label}: {err}")))
}

fn validate_kube_arg(value: &str, label: &str) -> CommandResult<()> {
    if value.is_empty()
        || !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '.')
    {
        return Err(failure(format!("Invalid Kubernetes {label}: {value}")));
    }
    Ok(())
}

fn validate_plain_value(value: &str, label: &str) -> CommandResult<()> {
    if value.is_empty() || value.chars().any(|ch| ch.is_control()) {
        return Err(failure(format!("{label} is not configured")));
    }
    Ok(())
}

fn ps_single_quoted(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn first_non_empty(current: String, detected: Option<String>) -> String {
    if current.trim().is_empty() {
        detected.unwrap_or_default().trim().to_string()
    } else {
        current
    }
}

fn default_key_path(install_path: &str) -> PathBuf {
    Path::new(install_path)
        .join("internal-scripts")
        .join("ssh")
        .join("sshKey")
}

fn app_data_dir(app: &AppHandle) -> CommandResult<PathBuf> {
    app.path()
        .app_data_dir()
        .map_err(|err| failure(format!("Failed to resolve app data directory: {err}")))
}

fn config_path(app: &AppHandle) -> CommandResult<PathBuf> {
    Ok(app_data_dir(app)?.join("config.json"))
}

fn normalize_config(mut config: AppConfig) -> AppConfig {
    config.install_path = config.install_path.trim().to_string();
    config.vm_name = config.vm_name.trim().to_string();
    config.vm_ip = config.vm_ip.trim().to_string();
    config.ssh_user = config.ssh_user.trim().to_string();
    config.ssh_path = config.ssh_path.trim().to_string();
    config.manager_api_url = config
        .manager_api_url
        .trim()
        .trim_end_matches('/')
        .to_string();
    config.manager_api_token = config.manager_api_token.trim().to_string();
    config.manager_api_namespace = config.manager_api_namespace.trim().to_string();
    config.manager_api_image = config.manager_api_image.trim().to_string();
    config.manager_api_binary_path = config.manager_api_binary_path.trim().to_string();
    config.manager_api_director_url = config
        .manager_api_director_url
        .trim()
        .trim_end_matches('/')
        .to_string();
    config
}

fn read_config_file(path: &Path) -> CommandResult<AppConfig> {
    let text = fs::read_to_string(path)
        .map_err(|err| failure(format!("Failed to read config {}: {err}", path.display())))?;
    let config: AppConfig = parse_json(&text, "app config")?;
    Ok(normalize_config(config))
}

fn local_seed_config_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(current_dir) = std::env::current_dir() {
        candidates.push(current_dir.join("default-config.json"));
        candidates.push(current_dir.join("app").join("default-config.json"));
    }
    if let Ok(exe) = std::env::current_exe() {
        for ancestor in exe.ancestors() {
            candidates.push(ancestor.join("default-config.json"));
        }
    }
    candidates
}

fn read_local_seed_config() -> Option<AppConfig> {
    for path in local_seed_config_candidates() {
        if path.exists() {
            if let Ok(config) = read_config_file(&path) {
                return Some(config);
            }
        }
    }
    None
}

fn write_app_config(app: &AppHandle, config: AppConfig) -> CommandResult<AppConfig> {
    let config = normalize_config(config);
    let path = config_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| failure(format!("Failed to create app data directory: {err}")))?;
    }
    let text = serde_json::to_string_pretty(&config)
        .map_err(|err| failure(format!("Failed to serialize app config: {err}")))?;
    fs::write(&path, text)
        .map_err(|err| failure(format!("Failed to write config {}: {err}", path.display())))?;
    Ok(config)
}

fn read_app_config(app: &AppHandle) -> CommandResult<AppConfig> {
    let path = config_path(app)?;
    if !path.exists() {
        let config = read_local_seed_config().unwrap_or_default();
        return write_app_config(app, config);
    }

    read_config_file(&path)
}

fn detect_host_config() -> DetectedConfig {
    let script = r#"
$ErrorActionPreference = 'SilentlyContinue'
$steamInstallPaths = @()
$steamInstallPaths += (Get-ItemProperty -Path 'HKCU:\Software\Valve\Steam').SteamPath
$steamInstallPaths += (Get-ItemProperty -Path 'HKLM:\Software\WOW6432Node\Valve\Steam').InstallPath
$steamInstallPaths += (Get-ItemProperty -Path 'HKLM:\Software\Valve\Steam').InstallPath
$libraryRoots = @()
foreach ($steamPath in $steamInstallPaths | Where-Object { $_ }) {
  $libraryRoots += $steamPath
  $libraryFile = Join-Path $steamPath 'steamapps\libraryfolders.vdf'
  if (Test-Path $libraryFile) {
    Get-Content $libraryFile | ForEach-Object {
      if ($_ -match '"path"\s+"([^"]+)"') {
        $libraryRoots += ($Matches[1] -replace '\\\\', '\')
      }
    }
  }
}
$installPath = $null
foreach ($root in $libraryRoots | Select-Object -Unique) {
  $candidate = Join-Path $root 'steamapps\common\Dune Awakening Playtest Server'
  if (Test-Path $candidate) {
    $installPath = (Resolve-Path $candidate).Path
    break
  }
}
$ssh = (Get-Command ssh.exe).Source
$vmName = $null
$vmIp = $null
if (Get-Command Get-VM) {
  $vm = Get-VM | Where-Object { $_.Name -match 'dune|awakening' } | Select-Object -First 1
  if ($vm) {
    $vmName = $vm.Name
    $ips = @((Get-VMNetworkAdapter -VMName $vm.Name).IPAddresses | Where-Object { $_ -match '^\d+\.\d+\.\d+\.\d+$' })
    $vmIp = $ips | Select-Object -First 1
  }
}
[pscustomobject]@{
  installPath = $installPath
  vmName = $vmName
  vmIp = $vmIp
  sshPath = $ssh
} | ConvertTo-Json -Compress
"#;
    run_powershell(script)
        .ok()
        .and_then(|text| parse_json::<DetectedConfig>(&text, "detected host config").ok())
        .unwrap_or_default()
}

fn detect_manager_binary_path() -> Option<String> {
    let relative = Path::new("app")
        .join("manager-api")
        .join("target")
        .join("x86_64-unknown-linux-musl")
        .join("release")
        .join("dune-manager-api");

    let mut candidates = Vec::new();
    if let Ok(current_dir) = std::env::current_dir() {
        candidates.push(current_dir.join(&relative));
        candidates.push(
            current_dir
                .join("manager-api")
                .join("target")
                .join("x86_64-unknown-linux-musl")
                .join("release")
                .join("dune-manager-api"),
        );
    }
    if let Ok(exe) = std::env::current_exe() {
        for ancestor in exe.ancestors() {
            candidates.push(ancestor.join(&relative));
        }
    }

    candidates
        .into_iter()
        .find(|path| path.exists())
        .map(|path| path.to_string_lossy().to_string())
}

fn required_config_value(
    value: Option<String>,
    fallback: &str,
    label: &str,
) -> CommandResult<String> {
    let value = value
        .unwrap_or_else(|| fallback.to_string())
        .trim()
        .to_string();
    if value.is_empty() {
        return Err(failure(format!("{label} is not configured")));
    }
    Ok(value)
}

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

fn prepare_key(app: &AppHandle, install_path: &str) -> CommandResult<PathBuf> {
    let source = default_key_path(install_path);
    if !source.exists() {
        return Err(failure(format!(
            "Bundled SSH key was not found at {}",
            source.display()
        )));
    }

    let key_dir = app_data_dir(app)?.join("keys");
    fs::create_dir_all(&key_dir)
        .map_err(|err| failure(format!("Failed to create key directory: {err}")))?;
    let target = key_dir.join("server-manager-ssh-key");

    let script = format!(
        r#"
$src = {src}
$dst = {dst}
if (Test-Path $dst) {{
  takeown /f $dst 2>&1 | Out-Null
  icacls $dst /reset 2>&1 | Out-Null
  Remove-Item -Path $dst -Force -ErrorAction SilentlyContinue
}}
Copy-Item -Path $src -Destination $dst -Force
icacls $dst /inheritance:r /grant:r "${{env:USERNAME}}:(R)" | Out-Null
[pscustomobject]@{{ path = $dst }} | ConvertTo-Json -Compress
"#,
        src = ps_single_quoted(&source.to_string_lossy()),
        dst = ps_single_quoted(&target.to_string_lossy())
    );

    run_powershell(&script)?;
    Ok(target)
}

fn run_ssh(
    app: &AppHandle,
    install_path: &str,
    ip: &str,
    ssh_user: &str,
    remote_command: &str,
) -> CommandResult<String> {
    let key = prepare_key(app, install_path)?;
    let destination = format!("{ssh_user}@{ip}");
    let key_str = key.to_string_lossy().to_string();
    let ssh_path = read_app_config(app)
        .map(|config| config.ssh_path)
        .unwrap_or_default();
    if ssh_path.is_empty() {
        return Err(failure("SSH path is not configured"));
    }
    run_program(
        &ssh_path,
        &[
            "-o",
            "BatchMode=yes",
            "-o",
            "PreferredAuthentications=publickey",
            "-o",
            "StrictHostKeyChecking=no",
            "-o",
            "UserKnownHostsFile=NUL",
            "-o",
            "ConnectTimeout=6",
            "-i",
            &key_str,
            &destination,
            remote_command,
        ],
    )
}

fn run_ssh_with_stdin(
    app: &AppHandle,
    install_path: &str,
    ip: &str,
    ssh_user: &str,
    remote_command: &str,
    stdin_text: &str,
) -> CommandResult<String> {
    let key = prepare_key(app, install_path)?;
    let destination = format!("{ssh_user}@{ip}");
    let key_str = key.to_string_lossy().to_string();
    let ssh_path = read_app_config(app)
        .map(|config| config.ssh_path)
        .unwrap_or_default();
    if ssh_path.is_empty() {
        return Err(failure("SSH path is not configured"));
    }

    let mut child = Command::new(&ssh_path)
        .args([
            "-o",
            "BatchMode=yes",
            "-o",
            "PreferredAuthentications=publickey",
            "-o",
            "StrictHostKeyChecking=no",
            "-o",
            "UserKnownHostsFile=NUL",
            "-o",
            "ConnectTimeout=6",
            "-i",
            &key_str,
            &destination,
            remote_command,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| failure(format!("Failed to run SSH: {err}")))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(stdin_text.as_bytes())
            .map_err(|err| failure(format!("Failed to send SSH stdin: {err}")))?;
    }

    let output = child
        .wait_with_output()
        .map_err(|err| failure(format!("Failed to wait for SSH: {err}")))?;

    if !output.status.success() {
        return Err(command_failure("SSH command exited with an error", output));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn scp_path(app: &AppHandle) -> CommandResult<String> {
    let ssh_path = read_app_config(app)
        .map(|config| config.ssh_path)
        .unwrap_or_default();
    if ssh_path.is_empty() {
        return Err(failure("SSH path is not configured"));
    }

    let path = Path::new(&ssh_path);
    let scp = path
        .parent()
        .map(|parent| parent.join("scp.exe"))
        .filter(|candidate| candidate.exists())
        .unwrap_or_else(|| PathBuf::from("scp.exe"));
    Ok(scp.to_string_lossy().to_string())
}

fn copy_to_guest(
    app: &AppHandle,
    install_path: &str,
    ip: &str,
    ssh_user: &str,
    source: &str,
    destination: &str,
) -> CommandResult<()> {
    let source_path = Path::new(source);
    if !source_path.exists() {
        return Err(failure(format!(
            "Manager API binary was not found at {}",
            source_path.display()
        )));
    }

    let key = prepare_key(app, install_path)?;
    let key_str = key.to_string_lossy().to_string();
    let target = format!("{ssh_user}@{ip}:{destination}");
    let scp = scp_path(app)?;
    run_program(
        &scp,
        &[
            "-o",
            "BatchMode=yes",
            "-o",
            "PreferredAuthentications=publickey",
            "-o",
            "StrictHostKeyChecking=no",
            "-o",
            "UserKnownHostsFile=NUL",
            "-o",
            "ConnectTimeout=6",
            "-i",
            &key_str,
            source,
            &target,
        ],
    )?;
    Ok(())
}

fn discover_ip_from_logs(install_path: &str) -> Option<String> {
    let log_dir = Path::new(install_path).join(".logs");
    let mut logs = fs::read_dir(log_dir)
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "log"))
        .collect::<Vec<_>>();
    logs.sort_by_key(|entry| entry.metadata().and_then(|meta| meta.modified()).ok());
    logs.reverse();

    for entry in logs {
        if let Ok(text) = fs::read_to_string(entry.path()) {
            for line in text.lines().rev() {
                if let Some(value) = line.split("VM IP address:").nth(1) {
                    let ip = value.trim().to_string();
                    if !ip.is_empty() {
                        return Some(ip);
                    }
                }
            }
        }
    }

    None
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
