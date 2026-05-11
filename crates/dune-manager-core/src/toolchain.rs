//! App-owned external tool installation and discovery.

use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::{
    errors::{command_failure, failure},
    models::CommandResult,
    shell::{ps_single_quoted, run_powershell, suppress_console_window},
};

const STEAMCMD_URL: &str = "https://steamcdn-a.akamaihd.net/client/installer/steamcmd.zip";
const OPENSSH_URL: &str =
    "https://github.com/PowerShell/Win32-OpenSSH/releases/latest/download/OpenSSH-Win64.zip";
const SERVER_APP_ID: &str = "3104830";

/// External command-line tool managed under the app-owned tools directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ManagedTool {
    /// Valve SteamCMD client.
    #[serde(rename = "steamcmd")]
    SteamCmd,
    /// Windows OpenSSH client distribution.
    #[serde(rename = "openssh")]
    OpenSsh,
}

impl ManagedTool {
    /// Parses a CLI/user-facing tool name.
    pub fn parse(value: &str) -> CommandResult<Self> {
        match value.to_ascii_lowercase().as_str() {
            "steamcmd" | "steam-cmd" => Ok(Self::SteamCmd),
            "openssh" | "open-ssh" | "ssh" => Ok(Self::OpenSsh),
            _ => Err(failure(format!(
                "Unknown managed tool {value}; expected steamcmd or openssh"
            ))),
        }
    }

    /// Stable identifier used in tool paths and JSON output.
    pub fn id(self) -> &'static str {
        match self {
            Self::SteamCmd => "steamcmd",
            Self::OpenSsh => "openssh",
        }
    }

    /// Executable filename expected after installation.
    pub fn executable_name(self) -> &'static str {
        match self {
            Self::SteamCmd => "steamcmd.exe",
            Self::OpenSsh => "ssh.exe",
        }
    }

    /// Default archive URL used for installation.
    pub fn default_url(self) -> &'static str {
        match self {
            Self::SteamCmd => STEAMCMD_URL,
            Self::OpenSsh => OPENSSH_URL,
        }
    }
}

/// Installation status for one managed tool.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolStatus {
    /// Managed tool.
    pub tool: ManagedTool,
    /// Whether the expected executable exists.
    pub installed: bool,
    /// Root directory for manager-owned data.
    pub tools_root: PathBuf,
    /// Directory where the tool is installed.
    pub install_dir: PathBuf,
    /// Expected executable path.
    pub executable: PathBuf,
}

/// Result of installing or reusing a managed tool.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInstallResult {
    /// Status after the install attempt.
    pub status: ToolStatus,
    /// Source archive URL used or selected.
    pub source_url: String,
    /// Whether the installer performed work in this call.
    pub installed_now: bool,
}

/// Result of installing or validating the host-side server package.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerPackageInstallResult {
    /// Directory where the server package was installed.
    pub install_dir: PathBuf,
    /// Steam app id used for the package.
    pub app_id: String,
    /// Whether SteamCMD reported the app fully installed.
    pub installed: bool,
}

/// Manager for app-owned external command-line tools.
#[derive(Debug, Clone)]
pub struct Toolchain {
    root: PathBuf,
}

impl Toolchain {
    /// Creates a toolchain rooted at a caller-provided directory.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Creates a toolchain using the default root resolution.
    pub fn from_default_root() -> CommandResult<Self> {
        Ok(Self::new(default_tools_root()?))
    }

    /// Returns the root directory used for manager-owned data.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Returns status for one tool.
    pub fn status(&self, tool: ManagedTool) -> ToolStatus {
        let install_dir = self.install_dir(tool);
        let executable = install_dir.join(tool.executable_name());
        ToolStatus {
            tool,
            installed: executable.is_file(),
            tools_root: self.root.clone(),
            install_dir,
            executable,
        }
    }

    /// Returns status for all supported tools.
    pub fn status_all(&self) -> Vec<ToolStatus> {
        [ManagedTool::SteamCmd, ManagedTool::OpenSsh]
            .into_iter()
            .map(|tool| self.status(tool))
            .collect()
    }

    /// Installs one tool from its default URL or a caller-provided archive URL.
    pub fn install(
        &self,
        tool: ManagedTool,
        force: bool,
        source_url: Option<String>,
    ) -> CommandResult<ToolInstallResult> {
        let before = self.status(tool);
        let source_url = source_url.unwrap_or_else(|| tool.default_url().to_string());
        if before.installed && !force {
            return Ok(ToolInstallResult {
                status: before,
                source_url,
                installed_now: false,
            });
        }

        let script = install_zip_tool_script(&self.root, tool, &source_url, force);
        run_powershell(&script)?;
        let status = self.status(tool);
        if !status.installed {
            return Err(failure(format!(
                "{} installation did not produce {}",
                tool.id(),
                status.executable.display()
            )));
        }
        Ok(ToolInstallResult {
            status,
            source_url,
            installed_now: true,
        })
    }

    fn install_dir(&self, tool: ManagedTool) -> PathBuf {
        self.root.join("tools").join(tool.id())
    }

    /// Installs or validates the host-side Dune server package with SteamCMD.
    pub fn install_server_package(
        &self,
        install_dir: impl AsRef<Path>,
    ) -> CommandResult<ServerPackageInstallResult> {
        let steamcmd = self.status(ManagedTool::SteamCmd);
        if !steamcmd.installed {
            return Err(failure("SteamCMD is not installed"));
        }
        let install_dir = install_dir.as_ref();
        let mut command = Command::new(&steamcmd.executable);
        suppress_console_window(&mut command);
        let output = command
            .args([
                "+@ShutdownOnFailedCommand",
                "1",
                "+@NoPromptForPassword",
                "1",
                "+force_install_dir",
            ])
            .arg(install_dir)
            .args([
                "+login",
                "anonymous",
                "+app_update",
                SERVER_APP_ID,
                "validate",
                "+quit",
            ])
            .output()
            .map_err(|err| failure(format!("Failed to run SteamCMD: {err}")))?;
        let combined_output = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let installed = combined_output
            .contains(&format!("Success! App '{SERVER_APP_ID}' fully installed."))
            || server_package_exists(install_dir);
        if !output.status.success() && !installed {
            return Err(command_failure(
                "SteamCMD server package install failed",
                output,
            ));
        }
        Ok(ServerPackageInstallResult {
            install_dir: install_dir.to_path_buf(),
            app_id: SERVER_APP_ID.to_string(),
            installed,
        })
    }
}

fn server_package_exists(install_dir: &Path) -> bool {
    let vm_dir = install_dir.join("Virtual Machines");
    install_dir.join("initial-setup.bat").is_file()
        && install_dir.join("battlegroup.bat").is_file()
        && vm_dir
            .read_dir()
            .ok()
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .any(|entry| {
                entry
                    .path()
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("vmcx"))
            })
}

/// Resolves the default manager data root for owned tools and downloads.
pub fn default_tools_root() -> CommandResult<PathBuf> {
    if let Ok(value) = env::var("DUNE_MANAGER_HOME") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Ok(PathBuf::from(trimmed));
        }
    }
    if let Ok(value) = env::var("LOCALAPPDATA") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Ok(PathBuf::from(trimmed).join("DuneDedicatedServerManager"));
        }
    }
    Ok(env::current_dir()
        .map_err(|err| failure(format!("Failed to determine current directory: {err}")))?
        .join(".dune-manager"))
}

/// Resolves the default directory for Hyper-V VM files managed by the app.
pub fn default_vm_destination() -> CommandResult<PathBuf> {
    Ok(default_runtime_root()?.join("vm"))
}

/// Resolves the default host-side server package directory.
pub fn default_server_package_dir() -> CommandResult<PathBuf> {
    Ok(default_runtime_root()?.join("dune-server"))
}

fn default_runtime_root() -> CommandResult<PathBuf> {
    let current = env::current_dir()
        .map_err(|err| failure(format!("Failed to determine current directory: {err}")))?;
    if current
        .file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case("src-tauri"))
    {
        return current
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| failure("Failed to resolve app runtime root"));
    }
    Ok(current)
}

/// Copies the vendor SSH key to a temporary path and restricts its ACL for OpenSSH.
pub fn prepare_vendor_ssh_key(server_package_dir: impl AsRef<Path>) -> CommandResult<PathBuf> {
    let source = server_package_dir
        .as_ref()
        .join("internal-scripts")
        .join("ssh")
        .join("sshKey");
    if !source.is_file() {
        return Err(failure(format!(
            "Vendor SSH key was not found: {}",
            source.display()
        )));
    }
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let destination = env::temp_dir().join(format!(
        "dune-manager-vm-sshKey-{}-{unique}",
        std::process::id()
    ));
    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
$source = {source}
$destination = {destination}
Copy-Item -LiteralPath $source -Destination $destination -Force
icacls $destination /inheritance:r | Out-Null
icacls $destination /grant:r "$($env:USERNAME):(R)" | Out-Null
[Console]::Out.WriteLine($destination)
"#,
        source = ps_single_quoted(&source.to_string_lossy()),
        destination = ps_single_quoted(&destination.to_string_lossy()),
    );
    run_powershell(&script)?;
    Ok(destination)
}

fn install_zip_tool_script(
    root: &Path,
    tool: ManagedTool,
    source_url: &str,
    force: bool,
) -> String {
    let install_dir = root.join("tools").join(tool.id());
    let downloads_dir = root.join("downloads");
    let staging_dir = root.join("staging").join(tool.id());
    let archive_path = downloads_dir.join(format!("{}.zip", tool.id()));
    let executable = install_dir.join(tool.executable_name());
    format!(
        r#"
$ErrorActionPreference = 'Stop'
$url = {url}
$installDir = {install_dir}
$downloadsDir = {downloads_dir}
$stagingDir = {staging_dir}
$archivePath = {archive_path}
$executable = {executable}
$executableName = {executable_name}
$force = {force}

if ((Test-Path -LiteralPath $executable) -and (-not $force)) {{
  [Console]::Out.WriteLine("already-installed")
  exit 0
}}

New-Item -ItemType Directory -Force -Path $downloadsDir | Out-Null
New-Item -ItemType Directory -Force -Path (Split-Path -Parent $stagingDir) | Out-Null
if (Test-Path -LiteralPath $stagingDir) {{ Remove-Item -LiteralPath $stagingDir -Recurse -Force }}
if ((Test-Path -LiteralPath $installDir) -and $force) {{
  Remove-Item -LiteralPath $installDir -Recurse -Force
}}

Invoke-WebRequest -Uri $url -OutFile $archivePath
Expand-Archive -LiteralPath $archivePath -DestinationPath $stagingDir -Force

$found = Get-ChildItem -LiteralPath $stagingDir -Recurse -File -Filter $executableName | Select-Object -First 1
if (-not $found) {{ throw "Archive did not contain $executableName" }}

New-Item -ItemType Directory -Force -Path $installDir | Out-Null
Copy-Item -Path (Join-Path $found.DirectoryName '*') -Destination $installDir -Recurse -Force
if (-not (Test-Path -LiteralPath $executable)) {{
  throw "Expected executable was not installed: $executable"
}}
[Console]::Out.WriteLine($executable)
"#,
        url = ps_single_quoted(source_url),
        install_dir = ps_single_quoted(&install_dir.to_string_lossy()),
        downloads_dir = ps_single_quoted(&downloads_dir.to_string_lossy()),
        staging_dir = ps_single_quoted(&staging_dir.to_string_lossy()),
        archive_path = ps_single_quoted(&archive_path.to_string_lossy()),
        executable = ps_single_quoted(&executable.to_string_lossy()),
        executable_name = ps_single_quoted(tool.executable_name()),
        force = if force { "$true" } else { "$false" },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_tool_paths_are_app_owned() {
        let root = PathBuf::from(r"C:\Users\Example\AppData\Local\DuneDedicatedServerManager");
        let toolchain = Toolchain::new(root.clone());
        let status = toolchain.status(ManagedTool::SteamCmd);
        assert_eq!(status.tools_root, root);
        assert!(status.executable.ends_with(r"tools\steamcmd\steamcmd.exe"));
    }

    #[test]
    fn install_script_downloads_and_expands_without_global_path_changes() {
        let script = install_zip_tool_script(
            Path::new(r"C:\DuneTools"),
            ManagedTool::OpenSsh,
            ManagedTool::OpenSsh.default_url(),
            false,
        );
        assert!(script.contains("Invoke-WebRequest"));
        assert!(script.contains("Expand-Archive"));
        assert!(script.contains("OpenSSH-Win64.zip"));
        assert!(!script.contains("setx"));
        assert!(!script.contains("$env:Path"));
    }
}
