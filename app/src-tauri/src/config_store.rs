use std::{
    fs,
    path::{Path, PathBuf},
};

use tauri::{AppHandle, Manager};

use crate::errors::{failure, parse_json};
use crate::models::{AppConfig, CommandResult, DetectedConfig};
use crate::shell::run_powershell;

pub fn first_non_empty(current: String, detected: Option<String>) -> String {
    if current.trim().is_empty() {
        detected.unwrap_or_default().trim().to_string()
    } else {
        current
    }
}

pub fn default_key_path(install_path: &str) -> PathBuf {
    Path::new(install_path)
        .join("internal-scripts")
        .join("ssh")
        .join("sshKey")
}

pub fn app_data_dir(app: &AppHandle) -> CommandResult<PathBuf> {
    app.path()
        .app_data_dir()
        .map_err(|err| failure(format!("Failed to resolve app data directory: {err}")))
}

pub fn read_app_config(app: &AppHandle) -> CommandResult<AppConfig> {
    let path = config_path(app)?;
    if !path.exists() {
        let config = read_local_seed_config().unwrap_or_default();
        return write_app_config(app, config);
    }

    read_config_file(&path)
}

pub fn write_app_config(app: &AppHandle, config: AppConfig) -> CommandResult<AppConfig> {
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

pub fn detect_host_config() -> DetectedConfig {
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

pub fn detect_manager_binary_path() -> Option<String> {
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
