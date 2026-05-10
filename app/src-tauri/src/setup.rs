use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    thread,
};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use dune_manager_core::operation_log::{
    classify_command_output, OperationLogEvent, StreamLogCapture,
};

use crate::{
    config_store::{app_data_dir, read_app_config, write_app_config},
    errors::{failure, parse_json},
    models::{
        CommandResult, GuestBootstrapRequest, SetupCommandResult, SetupPersistedState, SetupState,
        SteamCmdDetection, VmDestinationStatus, VmImportOptions,
    },
    security::redact_text,
    shell::{ps_single_quoted, run_powershell},
    ssh::{prepare_key, run_ssh},
};

const STEAMCMD_DOWNLOAD_URL: &str = "https://steamcdn-a.akamaihd.net/client/installer/steamcmd.zip";
const SERVER_APP_ID: &str = "3104830";
const DEFAULT_BOOTSTRAP_PROFILE_ID: &str = "vendor-default";
const WORLD_REGIONS: &[&str] = &["Europe Test", "North America Test"];

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SetupOutputEvent {
    stage: String,
    level: dune_manager_core::operation_log::LogLevel,
    message: String,
}

#[tauri::command]
pub fn detect_steamcmd(app: AppHandle) -> CommandResult<SteamCmdDetection> {
    let config = read_app_config(&app).unwrap_or_default();
    let local_candidates = steamcmd_candidate_paths()
        .into_iter()
        .map(|path| ps_single_quoted(&path.to_string_lossy()))
        .collect::<Vec<_>>()
        .join(", ");
    let script = format!(
        r#"
$ErrorActionPreference = 'SilentlyContinue'
$candidates = @()
$configured = {configured}
if ($configured) {{ $candidates += $configured }}
$candidates += @({local_candidates})
$cmd = (Get-Command steamcmd.exe).Source
if ($cmd) {{ $candidates += $cmd }}
$steamInstallPaths = @()
$steamInstallPaths += (Get-ItemProperty -Path 'HKCU:\Software\Valve\Steam').SteamPath
$steamInstallPaths += (Get-ItemProperty -Path 'HKLM:\Software\WOW6432Node\Valve\Steam').InstallPath
$steamInstallPaths += (Get-ItemProperty -Path 'HKLM:\Software\Valve\Steam').InstallPath
foreach ($steamPath in $steamInstallPaths | Where-Object {{ $_ }}) {{
  $candidates += (Join-Path $steamPath 'steamcmd\steamcmd.exe')
  $candidates += (Join-Path $steamPath 'steamcmd.exe')
}}
$candidates += (Join-Path $env:ProgramFiles 'SteamCMD\steamcmd.exe')
$candidates += (Join-Path ${{env:ProgramFiles(x86)}} 'SteamCMD\steamcmd.exe')
$candidates += (Join-Path $env:LOCALAPPDATA 'SteamCMD\steamcmd.exe')
$candidates += 'C:\steamcmd\steamcmd.exe'
$resolved = @()
$found = $null
foreach ($candidate in $candidates | Where-Object {{ $_ }} | Select-Object -Unique) {{
  if (Test-Path $candidate) {{
    $path = (Resolve-Path $candidate).Path
    $resolved += $path
    if (-not $found) {{ $found = $path }}
  }}
}}
[pscustomobject]@{{
  found = [bool]$found
  path = if ($found) {{ $found }} else {{ '' }}
  candidates = $resolved
}} | ConvertTo-Json -Compress
"#,
        configured = ps_single_quoted(&config.steamcmd_path),
        local_candidates = local_candidates
    );
    parse_json(&run_powershell(&script)?, "SteamCMD detection")
}

#[tauri::command]
pub fn detect_setup_state(app: AppHandle) -> CommandResult<SetupState> {
    let config = read_app_config(&app).unwrap_or_default();
    let persisted = read_setup_state(&app).unwrap_or_default();
    let steamcmd = detect_steamcmd(app.clone())?;
    let script = format!(
        r#"
$ErrorActionPreference = 'SilentlyContinue'
$principal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
$vmms = Get-Service -Name vmms -ErrorAction SilentlyContinue
$install = {install}
$configuredVmName = {vm_name}
$vm = $null
if ($configuredVmName) {{ $vm = Get-VM -Name $configuredVmName -ErrorAction SilentlyContinue }}
if (-not $vm) {{
  $vm = Get-VM -ErrorAction SilentlyContinue | Where-Object {{ $_.Name -match 'dune|awakening' -or $_.Path -match 'DuneAwakeningServer' }} | Select-Object -First 1
}}
$ip = ''
if ($vm) {{
  $ip = @((Get-VMNetworkAdapter -VMName $vm.Name).IPAddresses | Where-Object {{ $_ -match '^\d+\.\d+\.\d+\.\d+$' }} | Select-Object -First 1)
}}
[pscustomobject]@{{
  serverInstalled = if ($install) {{ Test-Path (Join-Path $install 'initial-setup.bat') }} else {{ $false }}
  serverInstallPath = $install
  vmExists = [bool]$vm
  vmState = if ($vm) {{ $vm.State.ToString() }} else {{ '' }}
  vmIp = if ($ip) {{ [string]$ip }} else {{ '' }}
  elevated = $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
  hypervAvailable = [bool](Get-Command Get-VM -ErrorAction SilentlyContinue)
  vmmsRunning = if ($vmms) {{ $vmms.Status.ToString() -eq 'Running' }} else {{ $false }}
}} | ConvertTo-Json -Compress
"#,
        install = ps_single_quoted(&config.install_path),
        vm_name = ps_single_quoted(&config.vm_name)
    );
    let mut state: SetupState = parse_json(&run_powershell(&script)?, "setup state")?;
    state.persisted = persisted;
    state.steamcmd = steamcmd;
    state.suggested_steamcmd_install_dir = suggested_steamcmd_install_dir()
        .to_string_lossy()
        .to_string();
    state.suggested_server_install_dir =
        suggested_server_install_dir().to_string_lossy().to_string();
    Ok(state)
}

#[tauri::command]
pub async fn install_steamcmd(
    app: AppHandle,
    install_dir: String,
) -> CommandResult<SteamCmdDetection> {
    run_setup_task(move || install_steamcmd_sync(app, install_dir)).await
}

fn install_steamcmd_sync(app: AppHandle, install_dir: String) -> CommandResult<SteamCmdDetection> {
    let install_dir = install_dir.trim();
    if install_dir.is_empty() {
        return Err(failure("SteamCMD install directory is required"));
    }
    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
$dir = {dir}
New-Item -ItemType Directory -Force -Path $dir | Out-Null
$zip = Join-Path $dir 'steamcmd.zip'
Invoke-WebRequest -Uri {url} -OutFile $zip
Expand-Archive -Path $zip -DestinationPath $dir -Force
Remove-Item $zip -Force
$exe = Join-Path $dir 'steamcmd.exe'
if (-not (Test-Path $exe)) {{ throw "steamcmd.exe was not found after extraction" }}
[pscustomobject]@{{ path = (Resolve-Path $exe).Path }} | ConvertTo-Json -Compress
"#,
        dir = ps_single_quoted(install_dir),
        url = ps_single_quoted(STEAMCMD_DOWNLOAD_URL)
    );
    let value: serde_json::Value = parse_json(&run_powershell(&script)?, "SteamCMD install")?;
    let path = value["path"].as_str().unwrap_or_default().to_string();
    let mut config = read_app_config(&app).unwrap_or_default();
    config.steamcmd_path = path;
    let _ = write_app_config(&app, config)?;
    detect_steamcmd(app)
}

#[tauri::command]
pub async fn install_server_app(
    app: AppHandle,
    steamcmd_path: String,
    install_dir: String,
) -> CommandResult<SetupCommandResult> {
    run_setup_task(move || install_server_app_sync(app, steamcmd_path, install_dir)).await
}

fn install_server_app_sync(
    app: AppHandle,
    steamcmd_path: String,
    install_dir: String,
) -> CommandResult<SetupCommandResult> {
    let steamcmd_path = steamcmd_path.trim();
    let install_dir = install_dir.trim();
    if steamcmd_path.is_empty() {
        return Err(failure("SteamCMD path is required"));
    }
    if install_dir.is_empty() {
        return Err(failure("Server install directory is required"));
    }

    if !Path::new(steamcmd_path).exists() {
        return Err(failure(format!("SteamCMD was not found: {steamcmd_path}")));
    }
    fs::create_dir_all(install_dir)
        .map_err(|err| failure(format!("Failed to create server install directory: {err}")))?;

    emit_setup_line(&app, "server-app", "Starting SteamCMD app update...");
    let args = vec![
        "+force_install_dir".to_string(),
        install_dir.to_string(),
        "+login".to_string(),
        "anonymous".to_string(),
        "+app_update".to_string(),
        SERVER_APP_ID.to_string(),
        "validate".to_string(),
        "+quit".to_string(),
    ];
    let (mut capture, exit_code) = run_program_streaming(&app, "server-app", steamcmd_path, &args)?;
    let installed = capture
        .raw
        .contains(&format!("Success! App '{SERVER_APP_ID}' fully installed."));
    if exit_code != 0 && installed {
        let line = format!(
            "SteamCMD returned exit code {exit_code} after reporting success; treating install as successful."
        );
        emit_setup_line(&app, "server-app", &line);
        capture.push_controlled("server-app", &line);
    }
    if exit_code != 0 && !installed {
        return Err(failure(format!(
            "SteamCMD failed with exit code {exit_code}\n{}",
            redact_text(&capture.controlled)
        )));
    }

    let mut config = read_app_config(&app).unwrap_or_default();
    config.steamcmd_path = steamcmd_path.to_string();
    config.install_path = install_dir.to_string();
    let _ = write_app_config(&app, config)?;
    mark_stage(&app, "server-app", None)?;
    Ok(stage_ok(
        "server-app",
        "Server app install/update completed",
        capture.controlled,
    ))
}

#[tauri::command]
pub fn detect_vm_import_options(
    app: AppHandle,
    install_path: Option<String>,
) -> CommandResult<VmImportOptions> {
    let config = read_app_config(&app).unwrap_or_default();
    let install = install_path
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(config.install_path);
    if install.trim().is_empty() {
        return Err(failure("Server install path is required"));
    }
    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
$root = {install}
$vmcx = Get-Item (Join-Path $root 'Virtual Machines\*.vmcx') -ErrorAction SilentlyContinue | Select-Object -First 1
$configuredVmName = {vm_name}
$existing = $null
if ($configuredVmName) {{ $existing = Get-VM -Name $configuredVmName -ErrorAction SilentlyContinue }}
if (-not $existing) {{
  $existing = Get-VM -ErrorAction SilentlyContinue | Where-Object {{ $_.Name -match 'dune|awakening' -or $_.Path -match 'DuneAwakeningServer' }} | Select-Object -First 1
}}
$drives = @(Get-PSDrive -PSProvider FileSystem | Where-Object {{ $_.Free -gt 100GB }} | ForEach-Object {{
  [pscustomobject]@{{ name = $_.Name; root = ($_.Name + ':\'); freeGb = [math]::Round($_.Free / 1GB, 2) }}
}})
$unsupportedAdapterPattern = 'Hyper-V|Virtual|Loopback|Bluetooth|TAP|TUN|VPN|WireGuard|Wintun|Npcap|Docker|WSL'
$adapters = @(Get-NetAdapter | Where-Object {{
  $_.Status -eq 'Up' -and
  $_.HardwareInterface -eq $true -and
  $_.InterfaceDescription -notmatch $unsupportedAdapterPattern
}} | ForEach-Object {{
  $adapter = $_
  $boundSwitch = Get-VMSwitch -SwitchType External -ErrorAction SilentlyContinue |
    Where-Object {{ $_.NetAdapterInterfaceDescription -eq $adapter.InterfaceDescription }} |
    Select-Object -First 1
  $ipInterfaceIndex = $adapter.ifIndex
  if ($boundSwitch) {{
    $managementAdapter = Get-NetAdapter -Name ("vEthernet (" + $boundSwitch.Name + ")") -ErrorAction SilentlyContinue
    if ($managementAdapter) {{ $ipInterfaceIndex = $managementAdapter.ifIndex }}
  }}
  $ip = Get-NetIPAddress -AddressFamily IPv4 -InterfaceIndex $ipInterfaceIndex -ErrorAction SilentlyContinue |
    Where-Object {{ $_.IPAddress -notlike '169.254.*' }} |
    Sort-Object PrefixOrigin |
    Select-Object -First 1
  $gateway = (Get-NetRoute -DestinationPrefix '0.0.0.0/0' -InterfaceIndex $ipInterfaceIndex -ErrorAction SilentlyContinue |
    Sort-Object RouteMetric |
    Select-Object -First 1).NextHop
  [pscustomobject]@{{
    name = $adapter.Name
    interfaceDescription = $adapter.InterfaceDescription
    ipv4Address = if ($ip) {{ $ip.IPAddress }} else {{ '' }}
    prefixLength = if ($ip) {{ [int]$ip.PrefixLength }} else {{ 0 }}
    cidr = if ($ip) {{ "$($ip.IPAddress)/$($ip.PrefixLength)" }} else {{ '' }}
    gateway = if ($gateway) {{ [string]$gateway }} else {{ '' }}
    boundSwitchName = if ($boundSwitch) {{ $boundSwitch.Name }} else {{ '' }}
  }}
}} | Where-Object {{ $_.ipv4Address }} | Sort-Object @{{ Expression = {{ if ($_.gateway) {{ 0 }} else {{ 1 }} }} }}, name)
$switches = @(Get-VMSwitch -ErrorAction SilentlyContinue | ForEach-Object {{
  [pscustomobject]@{{ name = $_.Name; switchType = $_.SwitchType.ToString(); netAdapterInterfaceDescription = [string]$_.NetAdapterInterfaceDescription }}
}})
[pscustomobject]@{{
  vmcxPath = if ($vmcx) {{ $vmcx.FullName }} else {{ '' }}
  existingVm = [bool]$existing
  existingVmState = if ($existing) {{ $existing.State.ToString() }} else {{ '' }}
  drives = $drives
  networkAdapters = $adapters
  switches = $switches
  suggestedDestination = {suggested_destination}
}} | ConvertTo-Json -Depth 5 -Compress
"#,
        install = ps_single_quoted(&install),
        vm_name = ps_single_quoted(&config.vm_name),
        suggested_destination = ps_single_quoted(&suggested_vm_destination_dir().to_string_lossy())
    );
    parse_json(&run_powershell(&script)?, "VM import options")
}

#[tauri::command]
pub fn inspect_vm_destination(destination_path: String) -> CommandResult<VmDestinationStatus> {
    let destination = destination_path.trim();
    if destination.is_empty() {
        return Err(failure("VM destination path is required"));
    }
    let path = Path::new(destination);
    if !path.exists() {
        return Ok(VmDestinationStatus {
            exists: false,
            is_empty: true,
        });
    }
    if !path.is_dir() {
        return Err(failure("VM destination exists but is not a folder"));
    }
    let mut entries = fs::read_dir(path)
        .map_err(|err| failure(format!("Failed to inspect VM destination: {err}")))?;
    Ok(VmDestinationStatus {
        exists: true,
        is_empty: entries.next().is_none(),
    })
}

#[tauri::command]
pub async fn run_vm_import_stage(
    app: AppHandle,
    install_path: String,
    destination_path: String,
    memory_gb: u32,
    switch_name: String,
    physical_adapter_name: String,
    clear_destination: bool,
) -> CommandResult<SetupCommandResult> {
    run_setup_task(move || {
        run_vm_import_stage_sync(
            app,
            install_path,
            destination_path,
            memory_gb,
            switch_name,
            physical_adapter_name,
            clear_destination,
        )
    })
    .await
}

fn run_vm_import_stage_sync(
    app: AppHandle,
    install_path: String,
    destination_path: String,
    memory_gb: u32,
    switch_name: String,
    physical_adapter_name: String,
    clear_destination: bool,
) -> CommandResult<SetupCommandResult> {
    if !(20..=64).contains(&memory_gb) {
        return Err(failure("Memory must be between 20GB and 64GB"));
    }
    if install_path.trim().is_empty() || destination_path.trim().is_empty() {
        return Err(failure("Install path and VM destination are required"));
    }
    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
$root = {install}
$dest = {dest}
$memory = [uint64]{memory}GB
$switchName = {switch_name}
$adapterName = {adapter}
$configuredVmName = {vm_name}
$clearDestination = ${clear_destination}
$vmcx = Get-Item (Join-Path $root 'Virtual Machines\*.vmcx') -ErrorAction Stop | Select-Object -First 1
if (-not $vmcx) {{ throw "No .vmcx file found under $root\Virtual Machines" }}
$existing = $null
if ($configuredVmName) {{ $existing = Get-VM -Name $configuredVmName -ErrorAction SilentlyContinue }}
if (-not $existing) {{
  $existing = Get-VM -ErrorAction SilentlyContinue | Where-Object {{ $_.Name -match 'dune|awakening' -or $_.Path -match 'DuneAwakeningServer' }} | Select-Object -First 1
}}
if ($existing) {{ throw "A matching server VM already exists. Reuse it or use an explicit reinstall flow." }}
if (Test-Path $dest) {{
  $existingItems = @(Get-ChildItem -LiteralPath $dest -Force -ErrorAction SilentlyContinue | Select-Object -First 1)
  if ($existingItems.Count -gt 0) {{
    if (-not $clearDestination) {{ throw "Destination already exists and is not empty: $dest" }}
    $resolvedDest = (Resolve-Path -LiteralPath $dest).Path
    $root = [System.IO.Path]::GetPathRoot($resolvedDest)
    if ($resolvedDest.TrimEnd('\') -eq $root.TrimEnd('\')) {{ throw "Refusing to clear drive root: $resolvedDest" }}
    if ($resolvedDest.Length -lt 8) {{ throw "Refusing to clear suspiciously short destination: $resolvedDest" }}
    Get-ChildItem -LiteralPath $resolvedDest -Force | Remove-Item -Recurse -Force -ErrorAction Stop
  }}
}} else {{
  New-Item -ItemType Directory -Force -Path $dest | Out-Null
}}
$compat = Compare-VM -Path $vmcx.FullName -Copy -VirtualMachinePath $dest -VhdDestinationPath (Join-Path $dest 'Virtual Hard Disks') -ErrorAction Stop
if ($compat.Incompatibilities.Count -gt 0) {{
  $messages = ($compat.Incompatibilities | ForEach-Object {{ $_.Message }}) -join '; '
  throw "VM compatibility issues: $messages"
}}
$imported = Import-VM -CompatibilityReport $compat -ErrorAction Stop
$vmName = $imported.Name
if ([string]::IsNullOrWhiteSpace($switchName)) {{ $switchName = 'DuneAwakeningServerSwitch' }}
$existingSwitch = Get-VMSwitch -Name $switchName -ErrorAction SilentlyContinue
if (-not $existingSwitch) {{
  if ([string]::IsNullOrWhiteSpace($adapterName)) {{ throw "Network adapter is required to create switch $switchName" }}
  New-VMSwitch -Name $switchName -NetAdapterName $adapterName -AllowManagementOS $true -ErrorAction Stop | Out-Null
}}
Connect-VMNetworkAdapter -VMName $vmName -SwitchName $switchName -ErrorAction Stop
$vhdx = Get-Item (Join-Path $dest 'Virtual Hard Disks\*.vhdx') -ErrorAction SilentlyContinue | Select-Object -First 1
if ($vhdx) {{ Resize-VHD -Path $vhdx.FullName -SizeBytes 100GB -ErrorAction Stop }}
$boot = Get-VMHardDiskDrive -VMName $vmName | Select-Object -First 1
if ($boot) {{ Set-VMFirmware -VMName $vmName -FirstBootDevice $boot }}
Set-VMMemory -VMName $vmName -StartupBytes $memory
Start-VM -Name $vmName -ErrorAction Stop
[pscustomobject]@{{ vmName = $vmName; destination = $dest; switchName = $switchName }} | ConvertTo-Json -Compress
"#,
        install = ps_single_quoted(&install_path),
        dest = ps_single_quoted(&destination_path),
        memory = memory_gb,
        switch_name = ps_single_quoted(&switch_name),
        adapter = ps_single_quoted(&physical_adapter_name),
        vm_name = ps_single_quoted(&read_app_config(&app).unwrap_or_default().vm_name),
        clear_destination = if clear_destination { "true" } else { "false" },
    );
    let stdout = run_powershell(&script)?;
    let value: serde_json::Value = parse_json(&stdout, "VM import result")?;
    let mut config = read_app_config(&app).unwrap_or_default();
    config.install_path = install_path;
    if let Some(vm_name) = value["vmName"].as_str() {
        config.vm_name = vm_name.to_string();
    }
    let _ = write_app_config(&app, config)?;
    mark_stage(&app, "vm-import", None)?;
    Ok(stage_ok(
        "vm-import",
        "VM imported and started",
        "VM imported, configured, and started.".to_string(),
    ))
}

#[tauri::command]
pub async fn run_guest_bootstrap_stage(
    app: AppHandle,
    request: GuestBootstrapRequest,
) -> CommandResult<SetupCommandResult> {
    run_setup_task(move || run_guest_bootstrap_stage_sync(app, request)).await
}

fn run_guest_bootstrap_stage_sync(
    app: AppHandle,
    request: GuestBootstrapRequest,
) -> CommandResult<SetupCommandResult> {
    validate_guest_bootstrap_request(&request)?;
    let install_path = request.install_path.trim().to_string();
    let start_ip = request.ip.trim().to_string();
    prepare_key(&app, &install_path)?;

    let mut all_output = String::new();
    all_output.push_str(&run_guest_phase(
        &app,
        &install_path,
        &start_ip,
        "guest-preflight",
        "Checking guest prerequisites",
        &guest_preflight_script(),
    )?);

    let target_ip = run_guest_network_phase(&app, &install_path, &request, &mut all_output)?;

    all_output.push_str(&run_guest_phase(
        &app,
        &install_path,
        &target_ip,
        "guest-disk",
        "Preparing guest disk",
        &guest_disk_script(),
    )?);
    all_output.push_str(&run_guest_phase(
        &app,
        &install_path,
        &target_ip,
        "guest-download",
        "Downloading guest server payload",
        &guest_download_script(),
    )?);
    all_output.push_str(&run_guest_phase(
        &app,
        &install_path,
        &target_ip,
        "guest-k3s",
        "Starting k3s and operators",
        &guest_k3s_script(),
    )?);
    all_output.push_str(&run_guest_phase(
        &app,
        &install_path,
        &target_ip,
        "guest-system",
        "Installing guest helper scripts",
        &guest_system_script(),
    )?);
    all_output.push_str(&run_guest_phase(
        &app,
        &install_path,
        &target_ip,
        "guest-world",
        "Creating battlegroup world",
        &guest_world_script(&request),
    )?);

    let world_unique_name = run_ssh(
        &app,
        &install_path,
        &target_ip,
        "dune",
        "cat /home/dune/.dune/.manager-bootstrap-world-name",
    )?
    .trim()
    .to_string();
    if world_unique_name.is_empty() {
        return Err(failure(
            "Guest bootstrap did not produce a battlegroup name",
        ));
    }

    all_output.push_str(&run_guest_phase(
        &app,
        &install_path,
        &target_ip,
        "guest-images",
        "Loading battlegroup images",
        &guest_images_script(&world_unique_name),
    )?);
    all_output.push_str(&run_guest_phase(
        &app,
        &install_path,
        &target_ip,
        "guest-default-settings",
        "Applying default user settings",
        &guest_default_settings_script(&world_unique_name),
    )?);

    let mut config = read_app_config(&app).unwrap_or_default();
    config.vm_ip = target_ip;
    config.ssh_user = non_empty(&config.ssh_user, "dune");
    let _ = write_app_config(&app, config)?;
    mark_stage(&app, "guest-bootstrap", None)?;
    Ok(stage_ok(
        "guest-bootstrap",
        "Guest bootstrap completed",
        "Guest bootstrap completed.".to_string(),
    ))
}

fn validate_guest_bootstrap_request(request: &GuestBootstrapRequest) -> CommandResult<()> {
    if request.install_path.trim().is_empty()
        || request.ip.trim().is_empty()
        || request.player_ip.trim().is_empty()
    {
        return Err(failure(
            "Install path, VM IP, and player-facing IP are required",
        ));
    }
    let world_name = request.world_name.trim();
    if world_name.is_empty() || world_name.chars().count() > 50 {
        return Err(failure("World name must be 1-50 characters"));
    }
    if has_multiline_value(world_name) {
        return Err(failure("World name cannot contain newlines"));
    }
    if !WORLD_REGIONS.contains(&request.region.trim()) {
        return Err(failure("Region must be Europe Test or North America Test"));
    }
    if request.self_host_token.trim().is_empty() || has_multiline_value(&request.self_host_token) {
        return Err(failure("Self-host token is required"));
    }
    let profile = non_empty(&request.profile_id, DEFAULT_BOOTSTRAP_PROFILE_ID);
    if profile != DEFAULT_BOOTSTRAP_PROFILE_ID {
        return Err(failure(
            "Only the vendor-default bootstrap profile is available right now",
        ));
    }
    Ok(())
}

fn run_guest_phase(
    app: &AppHandle,
    install_path: &str,
    ip: &str,
    stage: &str,
    title: &str,
    script: &str,
) -> CommandResult<String> {
    set_current_stage(app, stage)?;
    emit_setup_line(app, stage, title);
    let script = guest_script_with_system_path(script);
    match run_ssh_script_streaming(app, install_path, ip, "dune", stage, &script) {
        Ok(output) => {
            mark_stage(app, stage, None)?;
            Ok(output)
        }
        Err(err) => {
            let _ = mark_stage(app, stage, Some(err.message.clone()));
            Err(err)
        }
    }
}

fn guest_script_with_system_path(script: &str) -> String {
    format!(
        "export PATH=\"/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:$PATH\"\n{script}"
    )
}

fn run_guest_network_phase(
    app: &AppHandle,
    install_path: &str,
    request: &GuestBootstrapRequest,
    all_output: &mut String,
) -> CommandResult<String> {
    let start_ip = request.ip.trim();
    let target_ip = if request.static_ip.trim().is_empty() {
        start_ip.to_string()
    } else {
        request.static_ip.trim().to_string()
    };

    if !request.static_ip.trim().is_empty() {
        let script = guest_static_network_script(request);
        all_output.push_str(&run_guest_phase(
            app,
            install_path,
            start_ip,
            "guest-network",
            "Applying static guest network",
            &script,
        )?);
        emit_setup_line(
            app,
            "guest-network",
            &format!("Waiting for guest SSH on {target_ip}..."),
        );
        wait_for_ssh(app, install_path, &target_ip, "dune", 90)?;
    } else {
        set_current_stage(app, "guest-network")?;
        emit_setup_line(app, "guest-network", "Using DHCP guest network");
        mark_stage(app, "guest-network", None)?;
    }

    let script = guest_player_ip_script(&request.player_ip);
    all_output.push_str(&run_guest_phase(
        app,
        install_path,
        &target_ip,
        "guest-settings",
        "Writing player-facing IP",
        &script,
    )?);
    Ok(target_ip)
}

fn wait_for_ssh(
    app: &AppHandle,
    install_path: &str,
    ip: &str,
    ssh_user: &str,
    timeout_seconds: u64,
) -> CommandResult<()> {
    let started = std::time::Instant::now();
    while started.elapsed().as_secs() < timeout_seconds {
        if run_ssh(app, install_path, ip, ssh_user, "true").is_ok() {
            return Ok(());
        }
        thread::sleep(std::time::Duration::from_secs(2));
    }
    Err(failure(format!(
        "Guest did not become reachable on {ip} within {timeout_seconds} seconds"
    )))
}

fn guest_preflight_script() -> String {
    r#"
set -euo pipefail
echo "Checking /home/dune/.dune and passwordless sudo..."
test -d /home/dune/.dune
sudo -n true
missing=""
for cmd in sudo steamcmd jq openssl growpart bc base64 wget fdisk pvresize lvextend resize2fs rc-service k3s kubectl ctr; do
  if ! command -v "$cmd" >/dev/null 2>&1; then
    missing="$missing $cmd"
  fi
done
if [ -n "$missing" ]; then
  echo "Missing guest commands:$missing" >&2
  exit 1
fi
echo "Guest preflight passed."
"#
    .to_string()
}

fn guest_static_network_script(request: &GuestBootstrapRequest) -> String {
    let cidr = non_empty(&request.static_cidr, "/24");
    let dns = non_empty(&request.static_dns, "1.1.1.1");
    let mut script = String::from("set -euo pipefail\n");
    script.push_str(&shell_value("STATIC_IP", request.static_ip.trim()));
    script.push_str(&shell_value("STATIC_CIDR", &cidr));
    script.push_str(&shell_value(
        "STATIC_GATEWAY",
        request.static_gateway.trim(),
    ));
    script.push_str(&shell_value("STATIC_DNS", &dns));
    script.push_str(
        r#"
if [ -z "$STATIC_GATEWAY" ]; then
  echo "Static gateway is required" >&2
  exit 1
fi
cat > /tmp/dune-interfaces <<EOF
auto lo
iface lo inet loopback

auto eth0
iface eth0 inet static
    address ${STATIC_IP}${STATIC_CIDR}
    gateway ${STATIC_GATEWAY}
EOF
printf 'nameserver %s\n' "$STATIC_DNS" > /tmp/dune-resolv
sudo -n cp /tmp/dune-interfaces /etc/network/interfaces
sudo -n cp /tmp/dune-resolv /etc/resolv.conf
echo "Static network config written for ${STATIC_IP}${STATIC_CIDR}."
nohup sudo -n sh -c 'sleep 2; rc-service networking restart' </dev/null >/dev/null 2>&1 &
"#,
    );
    script
}

fn guest_player_ip_script(player_ip: &str) -> String {
    let mut script = String::from("set -euo pipefail\n");
    script.push_str(&shell_value("PLAYER_IP", player_ip.trim()));
    script.push_str(
        r#"
printf '\n\n\n%s\n' "$PLAYER_IP" > /home/dune/.dune/settings.conf
echo "Player-facing IP saved to guest settings."
"#,
    );
    script
}

fn guest_disk_script() -> String {
    r#"
set -euo pipefail
required_gb=30
available_gb=$(df -B1G -P / | awk '$NF == "/" {print $(NF-2)+0}')
echo "Root filesystem has ${available_gb}GB available."
if [ "$available_gb" -le "$required_gb" ]; then
  echo "Expanding root volume if the virtual disk has free space..."
  sudo growpart /dev/sda 2 || true
  sudo pvresize /dev/sda2 || true
  sudo lvextend -l +100%FREE /dev/mapper/vg0-lv_root || true
  sudo resize2fs /dev/mapper/vg0-lv_root || true
fi
available_gb=$(df -B1G -P / | awk '$NF == "/" {print $(NF-2)+0}')
if [ "$available_gb" -le "$required_gb" ]; then
  echo "Not enough guest disk space after resize: ${available_gb}GB available, need more than ${required_gb}GB" >&2
  exit 1
fi
echo "Guest disk has enough free space."
"#
    .to_string()
}

fn guest_download_script() -> String {
    format!(
        r#"
set -euo pipefail
DUNE_USER_PATH=/home/dune/.dune
DOWNLOAD_PATH="$DUNE_USER_PATH/download"
mkdir -p "$DOWNLOAD_PATH"
if [ -f "$DOWNLOAD_PATH/scripts/battlegroup.sh" ] && [ -f "$DOWNLOAD_PATH/scripts/setup.sh" ]; then
  echo "Guest server payload already exists."
  exit 0
fi
echo "Downloading server payload inside the VM with SteamCMD..."
steamcmd +set_spew_level 1 1 +force_install_dir "$DOWNLOAD_PATH" +login anonymous +app_update {app_id} +logoff +quit
if [ ! -f "$DOWNLOAD_PATH/scripts/battlegroup.sh" ] || [ ! -f "$DOWNLOAD_PATH/scripts/setup.sh" ]; then
  echo "Steam download completed, but expected setup scripts are missing." >&2
  exit 1
fi
echo "Guest server payload is ready."
"#,
        app_id = SERVER_APP_ID
    )
}

fn guest_k3s_script() -> String {
    r#"
set -euo pipefail
DUNE_USER_PATH=/home/dune/.dune
DOWNLOAD_PATH="$DUNE_USER_PATH/download"

restart_k3s_and_wait_until_ready() {
  local max_wait=60 elapsed=0
  sudo rc-service k3s restart
  echo "Waiting for k3s containerd socket..."
  while [ ! -S /run/k3s/containerd/containerd.sock ]; do
    sleep 2
    elapsed=$((elapsed + 2))
    if [ "$elapsed" -ge "$max_wait" ]; then echo "k3s containerd did not return in ${max_wait}s" >&2; return 1; fi
  done
  echo "Waiting for k3s API server..."
  elapsed=0
  until sudo kubectl get nodes >/dev/null 2>&1; do
    sleep 2
    elapsed=$((elapsed + 2))
    if [ "$elapsed" -ge "$max_wait" ]; then echo "k3s API did not return in ${max_wait}s" >&2; return 1; fi
  done
}

load_image_from_file() {
  local file_name="$1"
  if [ ! -f "$DOWNLOAD_PATH/$file_name" ]; then
    echo "Image file $DOWNLOAD_PATH/$file_name does not exist" >&2
    exit 1
  fi
  local attempt=1
  while [ "$attempt" -le 3 ]; do
    if sudo ctr -n k8s.io images import "$DOWNLOAD_PATH/$file_name"; then
      return 0
    fi
    echo "Import of $file_name failed (attempt $attempt/3)."
    if ! sudo ctr -n k8s.io version >/dev/null 2>&1; then
      echo "k3s/containerd is not responding; restarting."
      restart_k3s_and_wait_until_ready
    else
      sleep 5
    fi
    attempt=$((attempt + 1))
  done
  echo "Failed to import $file_name after 3 attempts" >&2
  exit 1
}

kubectl_retry() {
  local attempt=1 out rc
  while [ "$attempt" -le 5 ]; do
    out=$(sudo kubectl "$@" 2>&1)
    rc=$?
    if [ "$rc" -eq 0 ]; then
      [ -n "$out" ] && printf '%s\n' "$out"
      return 0
    fi
    if printf '%s' "$out" | grep -qiE 'connection refused|unable to connect to the server|i/o timeout|tls handshake|no route to host|EOF'; then
      echo "kubectl $* failed because the API is unavailable (attempt $attempt/5); retrying" >&2
      if ! sudo ctr -n k8s.io version >/dev/null 2>&1; then
        restart_k3s_and_wait_until_ready >&2
      else
        sleep 5
      fi
      attempt=$((attempt + 1))
      continue
    fi
    printf '%s\n' "$out" >&2
    return "$rc"
  done
  echo "kubectl $* still failing after retries" >&2
  return 1
}

wait_for_deployment() {
  local ns="$1" name="$2" timeout="${3:-120}" elapsed=0
  until sudo kubectl get -n "$ns" deployment "$name" >/dev/null 2>&1; do
    sleep 2
    elapsed=$((elapsed + 2))
    if [ "$elapsed" -ge "$timeout" ]; then echo "deployment $ns/$name did not appear within ${timeout}s" >&2; return 1; fi
  done
}

scale_deployment() {
  local ns="$1" name="$2" replicas="$3"
  wait_for_deployment "$ns" "$name" 120
  kubectl_retry scale -n "$ns" "deployment/$name" "--replicas=$replicas"
}

echo "Starting k3s..."
sudo rc-service k3s start
restart_k3s_and_wait_until_ready
sudo kubectl wait --for=condition=Ready node --all --timeout=180s >/dev/null || echo "Node did not reach Ready within 180s; continuing."
sudo rc-update add k3s >/dev/null

echo "Loading core images..."
load_image_from_file "images/prerequisites/coredns-coredns.tar"
load_image_from_file "images/prerequisites/local-path-provisioner.tar"
load_image_from_file "images/prerequisites/metrics-server.tar"
load_image_from_file "images/prerequisites/cert-manager-webhook.tar"
load_image_from_file "images/prerequisites/cert-manager-controller.tar"
load_image_from_file "images/prerequisites/cert-manager-cainjector.tar"
load_image_from_file "images/prerequisites/igw-postgres.tar"

echo "Starting core deployments..."
scale_deployment kube-system coredns 1
scale_deployment kube-system local-path-provisioner 1
scale_deployment kube-system metrics-server 1
scale_deployment cert-manager cert-manager 1
scale_deployment cert-manager cert-manager-cainjector 1
scale_deployment cert-manager cert-manager-webhook 1

version_file="$DOWNLOAD_PATH/images/operators/version.txt"
if [ ! -f "$version_file" ]; then
  echo "No operator version file found at $version_file" >&2
  exit 1
fi
current_version=$(kubectl_retry get -n funcom-operators deployment/battlegroupoperator-controller-manager -o jsonpath='{.spec.template.spec.containers[0].image}' | sed 's/.*://')
new_operator_version=$(cat "$version_file")
echo "Current operator version: $current_version"
echo "Downloaded operator version: $new_operator_version"
if [ "$current_version" != "$new_operator_version" ]; then
  echo "Loading operator images..."
  load_image_from_file "images/operators/battlegroup-operator.tar"
  load_image_from_file "images/operators/database-operator.tar"
  load_image_from_file "images/operators/server-operator.tar"
  load_image_from_file "images/operators/utilities-operator.tar"
  echo "Updating operator CRDs..."
  kubectl_retry replace -n funcom-operators -f "$DOWNLOAD_PATH/images/operators/crds/" || kubectl_retry apply -n funcom-operators -f "$DOWNLOAD_PATH/images/operators/crds/"
  echo "Patching operator images..."
  kubectl_retry set -n funcom-operators image deployment/battlegroupoperator-controller-manager manager=registry.funcom.com/funcom/self-hosting/igw-k8s-battlegroup-operator:"$new_operator_version"
  kubectl_retry set -n funcom-operators image deployment/databaseoperator-controller-manager manager=registry.funcom.com/funcom/self-hosting/igw-k8s-database-operator:"$new_operator_version"
  kubectl_retry set -n funcom-operators image deployment/serveroperator-controller-manager manager=registry.funcom.com/funcom/self-hosting/igw-k8s-server-operator:"$new_operator_version"
  kubectl_retry set -n funcom-operators image deployment/utilitiesoperator-controller-manager manager=registry.funcom.com/funcom/self-hosting/igw-k8s-utilities-operator:"$new_operator_version"
else
  echo "Operator version is already current."
fi

echo "Starting operator deployments..."
scale_deployment funcom-operators battlegroupoperator-controller-manager 1
scale_deployment funcom-operators databaseoperator-controller-manager 1
scale_deployment funcom-operators serveroperator-controller-manager 1
scale_deployment funcom-operators utilitiesoperator-controller-manager 1
echo "k3s and operators are ready."
"#
    .to_string()
}

fn guest_system_script() -> String {
    r#"
set -euo pipefail
mkdir -p /home/dune/.dune/bin
if [ ! -f /home/dune/.dune/download/scripts/battlegroup.sh ]; then
  echo "Missing downloaded battlegroup helper script" >&2
  exit 1
fi
ln -sfn /home/dune/.dune/download/scripts/battlegroup.sh /home/dune/.dune/bin/battlegroup
chmod +x /home/dune/.dune/download/scripts/battlegroup.sh
echo "Battlegroup helper installed."
"#
    .to_string()
}

fn guest_world_script(request: &GuestBootstrapRequest) -> String {
    let mut script = String::from("set -euo pipefail\n");
    script.push_str("G_SPEC_PATH=/home/dune/.dune\n");
    script.push_str("G_SCRIPT_PATH=/home/dune/.dune/download/scripts/setup\n");
    script.push_str(&shell_value("WORLD_NAME", request.world_name.trim()));
    script.push_str(&shell_value("WORLD_REGION", request.region.trim()));
    script.push_str(&shell_value("FLS_TOKEN", request.self_host_token.trim()));
    script.push_str(&shell_value(
        "PROFILE_ID",
        &non_empty(&request.profile_id, DEFAULT_BOOTSTRAP_PROFILE_ID),
    ));
    script.push_str(
        r#"
if [ "$PROFILE_ID" != "vendor-default" ]; then
  echo "Unsupported bootstrap profile: $PROFILE_ID" >&2
  exit 1
fi
existing_ns=$(sudo kubectl get ns --no-headers -o custom-columns=NAME:.metadata.name | grep '^funcom-seabass-' || true)
if [ -n "$existing_ns" ]; then
  echo "A battlegroup namespace already exists; native bootstrap is for first-time world creation." >&2
  printf '%s\n' "$existing_ns" >&2
  exit 1
fi
token_data=$(printf '%s' "$FLS_TOKEN" | jq -R 'split(".") | .[0:2] | map(@base64d) | map(fromjson)')
FLS_PLAYER_ID=$(printf '%s' "$token_data" | jq -r '.[1].HostId' | tr '[:upper:]' '[:lower:]')
if [ -z "$FLS_PLAYER_ID" ] || [ "$FLS_PLAYER_ID" = "null" ]; then
  echo "Self-host token did not contain a HostId" >&2
  exit 1
fi
if ! printf '%s' "$FLS_PLAYER_ID" | grep -Eq '^[a-z0-9]+$'; then
  echo "Self-host token HostId contained unexpected characters" >&2
  exit 1
fi
RMQ_SECRET=$(openssl rand -base64 64 | tr -d '\n')
WORLD_SUFFIX=$(od -An -N12 -tu1 /dev/urandom | awk '{ for (i = 1; i <= NF && length(out) < 6; i++) out = out sprintf("%c", 97 + ($i % 26)); print out }')
WORLD_UNIQUE_NAME="sh-${FLS_PLAYER_ID}-${WORLD_SUFFIX}"
NS="funcom-seabass-${WORLD_UNIQUE_NAME}"

escape_sed() {
  printf '%s' "$1" | sed -e 's/[\/&]/\\&/g'
}
escape_sed_pipe() {
  printf '%s' "$1" | sed -e 's/[|&]/\\&/g'
}

cp "$G_SCRIPT_PATH/templates/world-template.yaml" "$G_SPEC_PATH/$WORLD_UNIQUE_NAME.yaml"
cp "$G_SCRIPT_PATH/templates/fls-secret.yaml" "$G_SPEC_PATH/$WORLD_UNIQUE_NAME-fls-secret.yaml"
cp "$G_SCRIPT_PATH/templates/rmq-secret.yaml" "$G_SPEC_PATH/$WORLD_UNIQUE_NAME-rmq-secret.yaml"

world_name_e=$(escape_sed "$WORLD_NAME")
world_unique_e=$(escape_sed "$WORLD_UNIQUE_NAME")
world_region_e=$(escape_sed "$WORLD_REGION")
fls_secret_e=$(escape_sed "$FLS_TOKEN")
rmq_secret_e=$(escape_sed_pipe "$RMQ_SECRET")

sed -i "s/{WORLD_NAME}/$world_name_e/g" "$G_SPEC_PATH/$WORLD_UNIQUE_NAME.yaml"
sed -i "s/{WORLD_UNIQUE_NAME}/$world_unique_e/g" "$G_SPEC_PATH/$WORLD_UNIQUE_NAME.yaml"
sed -i "s/{WORLD_REGION}/$world_region_e/g" "$G_SPEC_PATH/$WORLD_UNIQUE_NAME.yaml"
sed -i "s/{WORLD_IMAGE_TAG}/0-0-shipping/g" "$G_SPEC_PATH/$WORLD_UNIQUE_NAME.yaml"
sed -i "s/{FLS_SECRET}/$fls_secret_e/g" "$G_SPEC_PATH/$WORLD_UNIQUE_NAME.yaml"
sed -i "s/{FLS_SECRET}/$fls_secret_e/g" "$G_SPEC_PATH/$WORLD_UNIQUE_NAME-fls-secret.yaml"
sed -i "s|{RMQ_SECRET}|$rmq_secret_e|g" "$G_SPEC_PATH/$WORLD_UNIQUE_NAME-rmq-secret.yaml"

echo "Waiting for operators before creating the world..."
elapsed=0
while [ "$elapsed" -lt 300 ]; do
  all_ready=true
  for op in battlegroupoperator-controller-manager databaseoperator-controller-manager serveroperator-controller-manager utilitiesoperator-controller-manager; do
    ready=$(sudo kubectl get -n funcom-operators deployment/"$op" -o jsonpath='{.status.readyReplicas}' 2>/dev/null || true)
    if [ "$ready" != "1" ]; then all_ready=false; break; fi
  done
  if $all_ready; then break; fi
  sleep 5
  elapsed=$((elapsed + 5))
  echo "Still waiting for operators... (${elapsed}s / 300s)"
done
if [ "$elapsed" -ge 300 ]; then
  echo "Timed out waiting for operators" >&2
  exit 1
fi

sudo kubectl create ns "$NS"
sudo kubectl create -n "$NS" -f "$G_SPEC_PATH/$WORLD_UNIQUE_NAME-fls-secret.yaml"
sudo kubectl create -n "$NS" -f "$G_SPEC_PATH/$WORLD_UNIQUE_NAME-rmq-secret.yaml"
sudo kubectl create -n "$NS" -f "$G_SPEC_PATH/$WORLD_UNIQUE_NAME.yaml"
printf '%s' "$WORLD_UNIQUE_NAME" > /home/dune/.dune/.manager-bootstrap-world-name
echo "Battlegroup world resource created."
"#,
    );
    script
}

fn guest_images_script(world_unique_name: &str) -> String {
    let mut script = String::from("set -euo pipefail\n");
    script.push_str("DOWNLOAD_PATH=/home/dune/.dune/download\n");
    script.push_str(&shell_value("WORLD_UNIQUE_NAME", world_unique_name));
    script.push_str(
        r#"
NS="funcom-seabass-${WORLD_UNIQUE_NAME}"
version_file="$DOWNLOAD_PATH/images/battlegroup/version.txt"
if [ ! -f "$version_file" ]; then
  echo "No battlegroup version file found at $version_file" >&2
  exit 1
fi
new_version=$(cat "$version_file")
echo "Downloaded battlegroup version: $new_version"
load_image_from_file() {
  local file_path="$1"
  if [ ! -f "$DOWNLOAD_PATH/$file_path" ]; then
    echo "Image file $DOWNLOAD_PATH/$file_path does not exist" >&2
    exit 1
  fi
  sudo ctr -n k8s.io images import "$DOWNLOAD_PATH/$file_path"
}
echo "Loading battlegroup images..."
load_image_from_file "images/battlegroup/server-rabbitmq.tar"
load_image_from_file "images/battlegroup/server-text-router.tar"
load_image_from_file "images/battlegroup/server-bg-director.tar"
load_image_from_file "images/battlegroup/server-gateway.tar"
load_image_from_file "images/battlegroup/server-db-utils.tar"
load_image_from_file "images/battlegroup/server.tar"
IMAGE_PATTERN='(?<prefix>.*/seabass-server[^:]*:)(?<tag>[0-9]+-0-[a-zA-Z0-9_-]+)'
patch_operations=$(sudo kubectl get battlegroup "$WORLD_UNIQUE_NAME" -n "$NS" -o json | jq --arg image_pattern "$IMAGE_PATTERN" --arg new_revision "$new_version" '
  [
    paths as $p |
    select(getpath($p) | type == "string" and test($image_pattern)) |
    select($p[-1] == "image") |
    {
      op: "replace",
      path: ("/" + ($p | map(tostring) | join("/"))),
      value: ((getpath($p) | capture($image_pattern).prefix) + $new_revision)
    }
  ]
')
sudo kubectl patch battlegroup "$WORLD_UNIQUE_NAME" -n "$NS" --type=json -p "$patch_operations"
echo "Battlegroup images loaded and resource patched."
"#,
    );
    script
}

fn guest_default_settings_script(world_unique_name: &str) -> String {
    let mut script = String::from("set -euo pipefail\n");
    script.push_str("DOWNLOAD_PATH=/home/dune/.dune/download\n");
    script.push_str(&shell_value("WORLD_UNIQUE_NAME", world_unique_name));
    script.push_str(
        r#"
NS="funcom-seabass-${WORLD_UNIQUE_NAME}"
config_dir="$DOWNLOAD_PATH/scripts/setup/config"
if ! ls "$config_dir"/User*.ini >/dev/null 2>&1; then
  echo "No User*.ini files found in $config_dir" >&2
  exit 1
fi
echo "Waiting for filebrowser pod..."
elapsed=0
fb_pod=""
while [ "$elapsed" -lt 240 ]; do
  fb_pod=$(sudo kubectl get pods -n "$NS" -l role=igw-filebrowser --no-headers -o custom-columns=NAME:.metadata.name 2>/dev/null | head -n1 || true)
  if [ -n "$fb_pod" ]; then break; fi
  sleep 5
  elapsed=$((elapsed + 5))
  echo "Still waiting for filebrowser pod... (${elapsed}s / 240s)"
done
if [ -z "$fb_pod" ]; then
  echo "No filebrowser pod became available in $NS" >&2
  exit 1
fi
sudo kubectl exec -n "$NS" "$fb_pod" -- mkdir -p /srv/UserSettings
for config_file in "$config_dir"/User*.ini; do
  filename=$(basename "$config_file")
  echo "Deploying $filename."
  sudo kubectl cp "$config_file" "$NS/$fb_pod:/srv/UserSettings/$filename"
done
echo "Default user settings deployed."
"#,
    );
    script
}

#[tauri::command]
pub fn save_setup_state(
    app: AppHandle,
    state: SetupPersistedState,
) -> CommandResult<SetupPersistedState> {
    write_setup_state(&app, &state)?;
    Ok(state)
}

#[tauri::command]
pub fn clear_setup_state(app: AppHandle) -> CommandResult<SetupPersistedState> {
    let path = setup_state_path(&app)?;
    if path.exists() {
        fs::remove_file(&path).map_err(|err| {
            failure(format!(
                "Failed to remove setup state {}: {err}",
                path.display()
            ))
        })?;
    }
    Ok(SetupPersistedState::default())
}

fn set_current_stage(app: &AppHandle, stage: &str) -> CommandResult<()> {
    let mut state = read_setup_state(app).unwrap_or_default();
    state.current_stage = stage.to_string();
    state.last_error.clear();
    write_setup_state(app, &state)
}

fn mark_stage(app: &AppHandle, stage: &str, error: Option<String>) -> CommandResult<()> {
    let mut state = read_setup_state(app).unwrap_or_default();
    state.current_stage = stage.to_string();
    if error.is_none() && !state.completed_stages.contains(&stage.to_string()) {
        state.completed_stages.push(stage.to_string());
    }
    state.last_error = error.unwrap_or_default();
    write_setup_state(app, &state)
}

fn read_setup_state(app: &AppHandle) -> CommandResult<SetupPersistedState> {
    let path = setup_state_path(app)?;
    if !path.exists() {
        return Ok(SetupPersistedState::default());
    }
    let text = fs::read_to_string(&path).map_err(|err| {
        failure(format!(
            "Failed to read setup state {}: {err}",
            path.display()
        ))
    })?;
    parse_json(&text, "setup state")
}

fn write_setup_state(app: &AppHandle, state: &SetupPersistedState) -> CommandResult<()> {
    let path = setup_state_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| failure(format!("Failed to create setup state directory: {err}")))?;
    }
    let text = serde_json::to_string_pretty(state)
        .map_err(|err| failure(format!("Failed to serialize setup state: {err}")))?;
    fs::write(&path, text).map_err(|err| {
        failure(format!(
            "Failed to write setup state {}: {err}",
            path.display()
        ))
    })?;
    Ok(())
}

fn setup_state_path(app: &AppHandle) -> CommandResult<PathBuf> {
    Ok(app_data_dir(app)?.join("setup-state.json"))
}

fn run_ssh_script_streaming(
    app: &AppHandle,
    install_path: &str,
    ip: &str,
    ssh_user: &str,
    stage: &str,
    script: &str,
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
            "LogLevel=ERROR",
            "-o",
            "ConnectTimeout=6",
            "-i",
            &key_str,
            &destination,
            "bash -s",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| failure(format!("Failed to run SSH: {err}")))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(script.as_bytes())
            .map_err(|err| failure(format!("Failed to send SSH script: {err}")))?;
    }

    let capture = Arc::new(Mutex::new(StreamLogCapture::default()));
    let mut readers = Vec::new();
    if let Some(stdout) = child.stdout.take() {
        readers.push(spawn_stream_reader(
            app.clone(),
            stage.to_string(),
            capture.clone(),
            stdout,
        ));
    }
    if let Some(stderr) = child.stderr.take() {
        readers.push(spawn_stream_reader(
            app.clone(),
            stage.to_string(),
            capture.clone(),
            stderr,
        ));
    }

    let status = child
        .wait()
        .map_err(|err| failure(format!("Failed to wait for SSH: {err}")))?;
    for reader in readers {
        let _ = reader.join();
    }
    let capture = capture
        .lock()
        .map(|value| value.clone())
        .unwrap_or_default();

    if !status.success() {
        return Err(failure(format!(
            "SSH bootstrap phase failed with exit code {}\n{}",
            status.code().unwrap_or(-1),
            redact_text(&capture.controlled)
        )));
    }

    Ok(capture.controlled)
}

fn emit_setup_line(app: &AppHandle, stage: &str, line: &str) {
    emit_setup_event(app, OperationLogEvent::info(stage, line));
}

fn emit_setup_event(app: &AppHandle, event: OperationLogEvent) {
    let line = redact_text(&event.message).trim_end().to_string();
    if line.is_empty() {
        return;
    }
    let _ = app.emit(
        "setup-output",
        SetupOutputEvent {
            stage: event.stage,
            level: event.level,
            message: line,
        },
    );
}

fn run_program_streaming(
    app: &AppHandle,
    stage: &str,
    program: &str,
    args: &[String],
) -> CommandResult<(StreamLogCapture, i32)> {
    let mut child = Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| failure(format!("Failed to run {program}: {err}")))?;

    let capture = Arc::new(Mutex::new(StreamLogCapture::default()));
    let mut readers = Vec::new();

    if let Some(stdout) = child.stdout.take() {
        readers.push(spawn_stream_reader(
            app.clone(),
            stage.to_string(),
            capture.clone(),
            stdout,
        ));
    }
    if let Some(stderr) = child.stderr.take() {
        readers.push(spawn_stream_reader(
            app.clone(),
            stage.to_string(),
            capture.clone(),
            stderr,
        ));
    }

    let status = child
        .wait()
        .map_err(|err| failure(format!("Failed to wait for {program}: {err}")))?;

    for reader in readers {
        let _ = reader.join();
    }

    let capture = capture
        .lock()
        .map(|value| value.clone())
        .unwrap_or_default();
    Ok((capture, status.code().unwrap_or(-1)))
}

fn spawn_stream_reader<R>(
    app: AppHandle,
    stage: String,
    capture: Arc<Mutex<StreamLogCapture>>,
    stream: R,
) -> thread::JoinHandle<()>
where
    R: std::io::Read + Send + 'static,
{
    thread::spawn(move || {
        let reader = BufReader::new(stream);
        for line in reader.lines().map_while(Result::ok) {
            if let Ok(mut capture) = capture.lock() {
                capture.push_raw(&line);
                if let Some(event) = classify_command_output(&stage, &line) {
                    capture.push_event(event.clone());
                    emit_setup_event(&app, event);
                }
            }
        }
    })
}

async fn run_setup_task<T, F>(work: F) -> CommandResult<T>
where
    T: Send + 'static,
    F: FnOnce() -> CommandResult<T> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|err| failure(format!("Setup task failed: {err}")))?
}

fn suggested_steamcmd_install_dir() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("steamcmd")
}

fn suggested_server_install_dir() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("dune-server")
}

fn suggested_vm_destination_dir() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("vm")
}

fn steamcmd_candidate_paths() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(current_dir) = std::env::current_dir() {
        roots.push(current_dir);
    }
    if let Ok(exe) = std::env::current_exe() {
        roots.extend(exe.ancestors().map(Path::to_path_buf));
    }

    let mut candidates = Vec::new();
    for root in roots {
        candidates.push(root.join("SteamCMD").join("steamcmd.exe"));
        candidates.push(root.join("steamcmd").join("steamcmd.exe"));
        candidates.push(root.join("steamcmd.exe"));
    }
    candidates.push(suggested_steamcmd_install_dir().join("steamcmd.exe"));
    candidates
}

fn stage_ok(stage: &str, message: &str, stdout: String) -> SetupCommandResult {
    SetupCommandResult {
        ok: true,
        stage: stage.to_string(),
        message: message.to_string(),
        stdout: redact_text(&stdout),
    }
}

fn non_empty(value: &str, fallback: &str) -> String {
    if value.trim().is_empty() {
        fallback.to_string()
    } else {
        value.trim().to_string()
    }
}

fn shell_value(name: &str, value: &str) -> String {
    let delimiter = format!("__DUNE_MANAGER_{name}__");
    format!("{name}=$(cat <<'{delimiter}'\n{value}\n{delimiter}\n)\n")
}

fn has_multiline_value(value: &str) -> bool {
    value.contains('\n') || value.contains('\r')
}
