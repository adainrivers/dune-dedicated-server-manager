//! App-owned external tool installation and discovery.

use std::{
    env, fs,
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
const QEMU_IMG_URL: &str = "https://cloudbase.it/downloads/qemu-img-win-x64-2_3_0.zip";
/// Steam app id for the Dune Awakening dedicated server package.
pub const SERVER_APP_ID: &str = "3104830";

const SERVER_MANIFEST_PATH: &str = "steamapps/appmanifest_3104830.acf";

/// External command-line tool managed under the app-owned tools directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ManagedTool {
    /// Valve SteamCMD client.
    #[serde(rename = "steamcmd")]
    SteamCmd,
    /// Windows OpenSSH client distribution.
    #[serde(rename = "openssh")]
    OpenSsh,
    /// QEMU disk image conversion tool.
    #[serde(rename = "qemu-img")]
    QemuImg,
}

impl ManagedTool {
    /// Parses a CLI/user-facing tool name.
    pub fn parse(value: &str) -> CommandResult<Self> {
        match value.to_ascii_lowercase().as_str() {
            "steamcmd" | "steam-cmd" => Ok(Self::SteamCmd),
            "openssh" | "open-ssh" | "ssh" => Ok(Self::OpenSsh),
            "qemu-img" | "qemuimg" | "qemu" => Ok(Self::QemuImg),
            _ => Err(failure(format!(
                "Unknown managed tool {value}; expected steamcmd, openssh, or qemu-img"
            ))),
        }
    }

    /// Stable identifier used in tool paths and JSON output.
    pub fn id(self) -> &'static str {
        match self {
            Self::SteamCmd => "steamcmd",
            Self::OpenSsh => "openssh",
            Self::QemuImg => "qemu-img",
        }
    }

    /// Executable filename expected after installation.
    pub fn executable_name(self) -> &'static str {
        match self {
            Self::SteamCmd => "steamcmd.exe",
            Self::OpenSsh => "ssh.exe",
            Self::QemuImg => "qemu-img.exe",
        }
    }

    /// Default archive URL used for installation.
    pub fn default_url(self) -> &'static str {
        match self {
            Self::SteamCmd => STEAMCMD_URL,
            Self::OpenSsh => OPENSSH_URL,
            Self::QemuImg => QEMU_IMG_URL,
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

/// Vendor package layout detected on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ServerPackageLayout {
    /// Original package layout using `internal-scripts`.
    LegacyInternalScripts,
    /// Current package layout using `battlegroup-management`.
    BattlegroupManagement,
}

/// Required paths discovered for a host-side Dune server package.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerPackageLayoutInfo {
    /// Package root directory.
    pub package_dir: PathBuf,
    /// Detected vendor layout.
    pub layout: ServerPackageLayout,
    /// Host-side batch entrypoint.
    pub battlegroup_bat: PathBuf,
    /// Vendor SSH private key used for first guest contact.
    pub ssh_key: PathBuf,
    /// Host-side bootstrap helper uploaded into the guest.
    pub bootstrap_setup: PathBuf,
    /// Packaged Hyper-V VM configuration.
    pub vmcx_path: PathBuf,
}

/// Version and completeness status for the host-side Dune server package.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerPackageStatus {
    /// Package root directory.
    pub package_dir: PathBuf,
    /// Steam app id used for the package.
    pub app_id: String,
    /// Installed Steam build id from the local manifest.
    pub installed_build_id: Option<String>,
    /// Latest Steam public branch build id when SteamCMD could report it.
    pub latest_build_id: Option<String>,
    /// Whether the local build is older than the latest known build.
    pub update_available: bool,
    /// Whether the app recognized all required package assets.
    pub complete: bool,
    /// Detected vendor layout, when complete enough to identify.
    pub layout: Option<ServerPackageLayout>,
    /// Human-readable status details or recovery hint.
    pub message: String,
}

/// Result of rotating a fresh imported VM from the public bootstrap key to a host-local key.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VendorSshKeyRotationResult {
    /// Private key path to use for the rest of the current setup flow.
    pub key_path: PathBuf,
    /// Public key path when a new key was generated and stored.
    pub public_key_path: Option<PathBuf>,
    /// Whether a new public key was installed into the guest.
    pub rotated: bool,
    /// Human-readable status or fallback reason.
    pub message: String,
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
        [
            ManagedTool::SteamCmd,
            ManagedTool::OpenSsh,
            ManagedTool::QemuImg,
        ]
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

    /// Reads host-side server package status and optionally the latest Steam build id.
    pub fn server_package_status(
        &self,
        install_dir: impl AsRef<Path>,
    ) -> CommandResult<ServerPackageStatus> {
        let install_dir = install_dir.as_ref();
        let layout = detect_server_package_layout(install_dir).ok();
        let installed_build_id = read_installed_server_build_id(install_dir);
        let latest_build_id = if self.status(ManagedTool::SteamCmd).installed {
            query_latest_server_build_id(&self.status(ManagedTool::SteamCmd).executable).ok()
        } else {
            None
        };
        let update_available = installed_build_id
            .as_deref()
            .zip(latest_build_id.as_deref())
            .is_some_and(|(installed, latest)| installed != latest);
        let complete = layout.is_some();
        let message = match (&layout, &installed_build_id, &latest_build_id) {
            (Some(info), Some(installed), Some(latest)) if installed == latest => {
                format!("{:?} package is current at build {installed}.", info.layout)
            }
            (Some(info), Some(installed), Some(latest)) => {
                format!(
                    "{:?} package build {installed} is older than latest build {latest}.",
                    info.layout
                )
            }
            (Some(info), Some(installed), None) => {
                format!(
                    "{:?} package build {installed} is installed; latest build is unknown.",
                    info.layout
                )
            }
            (Some(info), None, _) => format!(
                "{:?} package assets are present but the Steam manifest build id was not found.",
                info.layout
            ),
            (None, _, _) => {
                "Server package is missing required VM, SSH key, or bootstrap assets.".to_string()
            }
        };
        Ok(ServerPackageStatus {
            package_dir: install_dir.to_path_buf(),
            app_id: SERVER_APP_ID.to_string(),
            installed_build_id,
            latest_build_id,
            update_available,
            complete,
            layout: layout.map(|info| info.layout),
            message,
        })
    }
}

fn server_package_exists(install_dir: &Path) -> bool {
    detect_server_package_layout(install_dir).is_ok()
}

/// Detects a complete server package layout and returns its required paths.
pub fn detect_server_package_layout(
    install_dir: impl AsRef<Path>,
) -> CommandResult<ServerPackageLayoutInfo> {
    let install_dir = install_dir.as_ref();
    let battlegroup_bat = install_dir.join("battlegroup.bat");
    if !battlegroup_bat.is_file() {
        return Err(failure(format!(
            "Vendor battlegroup entrypoint was not found: {}",
            battlegroup_bat.display()
        )));
    }
    let vmcx_path = find_packaged_vmcx(install_dir).ok_or_else(|| {
        failure(format!(
            "Packaged VM configuration was not found under {}",
            install_dir.join("Virtual Machines").display()
        ))
    })?;
    let candidates = [
        (
            ServerPackageLayout::BattlegroupManagement,
            install_dir
                .join("battlegroup-management")
                .join("ssh")
                .join("bundledSshKey"),
            install_dir
                .join("battlegroup-management")
                .join("bootstrap")
                .join("setup"),
        ),
        (
            ServerPackageLayout::LegacyInternalScripts,
            install_dir
                .join("internal-scripts")
                .join("ssh")
                .join("sshKey"),
            install_dir
                .join("internal-scripts")
                .join("bootstrap")
                .join("setup"),
        ),
    ];
    for (layout, ssh_key, bootstrap_setup) in candidates {
        if ssh_key.is_file() && bootstrap_setup.is_file() {
            return Ok(ServerPackageLayoutInfo {
                package_dir: install_dir.to_path_buf(),
                layout,
                battlegroup_bat,
                ssh_key,
                bootstrap_setup,
                vmcx_path,
            });
        }
    }
    Err(failure(format!(
        "Vendor SSH key/bootstrap files were not found in supported layouts under {}",
        install_dir.display()
    )))
}

fn find_packaged_vmcx(install_dir: &Path) -> Option<PathBuf> {
    install_dir
        .join("Virtual Machines")
        .read_dir()
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            path.extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("vmcx"))
        })
}

/// Reads the installed Steam build id from the package manifest.
pub fn read_installed_server_build_id(install_dir: impl AsRef<Path>) -> Option<String> {
    let manifest = fs::read_to_string(install_dir.as_ref().join(SERVER_MANIFEST_PATH)).ok()?;
    parse_vdf_value(&manifest, "buildid")
}

fn query_latest_server_build_id(steamcmd: &Path) -> CommandResult<String> {
    let mut command = Command::new(steamcmd);
    suppress_console_window(&mut command);
    let output = command
        .args([
            "+login",
            "anonymous",
            "+app_info_update",
            "1",
            "+app_info_print",
            SERVER_APP_ID,
            "+quit",
        ])
        .output()
        .map_err(|err| failure(format!("Failed to run SteamCMD app info query: {err}")))?;
    if !output.status.success() {
        return Err(command_failure("SteamCMD app info query failed", output));
    }
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    parse_public_branch_build_id(&text)
        .or_else(|| parse_vdf_value(&text, "buildid"))
        .ok_or_else(|| failure("SteamCMD app info did not contain a public build id"))
}

fn parse_public_branch_build_id(text: &str) -> Option<String> {
    let mut in_branches = false;
    let mut in_public = false;
    let mut depth = 0i32;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('"') && trimmed.contains("\"branches\"") {
            in_branches = true;
            depth = 0;
            continue;
        }
        if in_branches && trimmed.starts_with('"') && trimmed.contains("\"public\"") {
            in_public = true;
            depth = 0;
            continue;
        }
        if in_public {
            if let Some(value) = parse_vdf_line_value(trimmed, "buildid") {
                return Some(value);
            }
            depth += trimmed.matches('{').count() as i32;
            depth -= trimmed.matches('}').count() as i32;
            if depth < 0 || trimmed == "}" {
                in_public = false;
            }
        } else if in_branches && trimmed == "}" {
            in_branches = false;
        }
    }
    None
}

fn parse_vdf_value(text: &str, key: &str) -> Option<String> {
    text.lines()
        .find_map(|line| parse_vdf_line_value(line.trim(), key))
}

fn parse_vdf_line_value(line: &str, key: &str) -> Option<String> {
    let mut parts = line.split('"').filter(|part| !part.trim().is_empty());
    let found_key = parts.next()?.trim();
    if found_key != key {
        return None;
    }
    Some(parts.next()?.trim().to_string())
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

/// Copies the packaged bootstrap SSH key to a temporary path and restricts its ACL for OpenSSH.
pub fn prepare_vendor_ssh_key(server_package_dir: impl AsRef<Path>) -> CommandResult<PathBuf> {
    let layout = detect_server_package_layout(server_package_dir)?;
    prepare_restricted_ssh_key_copy(&layout.ssh_key)
}

/// Copies usable vendor SSH key candidates to temporary paths with OpenSSH-compatible ACLs.
///
/// Current `battlegroup-management` packages rotate the public bootstrap key into
/// `%LOCALAPPDATA%\DuneAwakeningServer\sshKey` during vendor setup. Existing VMs
/// may therefore authenticate with that active key, while fresh imported VMs
/// still authenticate with the packaged bootstrap key.
pub fn prepare_vendor_ssh_key_candidates(
    server_package_dir: impl AsRef<Path>,
) -> CommandResult<Vec<PathBuf>> {
    let layout = detect_server_package_layout(server_package_dir)?;
    let mut sources = Vec::new();
    if layout.layout == ServerPackageLayout::BattlegroupManagement {
        if let Some(active_key) = vendor_active_ssh_key_path().filter(|path| path.is_file()) {
            sources.push(active_key);
        }
    }
    sources.push(layout.ssh_key);
    sources.dedup();

    let mut candidates = Vec::with_capacity(sources.len());
    for source in sources {
        candidates.push(prepare_restricted_ssh_key_copy(&source)?);
    }
    Ok(candidates)
}

/// Generates a fresh host-local SSH key and installs its public key into the guest.
///
/// Current vendor setup seeds first contact with the packaged `bundledSshKey`, then rotates
/// the VM to `%LOCALAPPDATA%\DuneAwakeningServer\sshKey`. This mirrors that behavior while
/// returning the currently usable key so native setup can continue even if rotation falls back.
pub fn rotate_vendor_guest_ssh_key(
    server_package_dir: impl AsRef<Path>,
    ssh_path: impl AsRef<Path>,
    bootstrap_key_path: impl AsRef<Path>,
    host: &str,
) -> CommandResult<VendorSshKeyRotationResult> {
    let layout = detect_server_package_layout(server_package_dir)?;
    let bootstrap_key_path = bootstrap_key_path.as_ref();
    if layout.layout != ServerPackageLayout::BattlegroupManagement {
        return Ok(VendorSshKeyRotationResult {
            key_path: bootstrap_key_path.to_path_buf(),
            public_key_path: None,
            rotated: false,
            message: "Legacy server package layout keeps using the packaged SSH key.".to_string(),
        });
    }

    let ssh_path = ssh_path.as_ref();
    let keygen = ssh_path
        .parent()
        .map(|dir| dir.join("ssh-keygen.exe"))
        .ok_or_else(|| failure("Failed to resolve OpenSSH tool directory"))?;
    if !keygen.is_file() {
        return Ok(VendorSshKeyRotationResult {
            key_path: bootstrap_key_path.to_path_buf(),
            public_key_path: None,
            rotated: false,
            message: format!(
                "OpenSSH key generator was not found at {}; continuing with the bootstrap key.",
                keygen.display()
            ),
        });
    }

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let temp_stem = env::temp_dir().join(format!(
        "dune-manager-vm-generated-sshKey-{}-{unique}",
        std::process::id()
    ));
    let temp_public = PathBuf::from(format!("{}.pub", temp_stem.to_string_lossy()));
    let active_key = vendor_active_ssh_key_path()
        .ok_or_else(|| failure("LOCALAPPDATA is required to store the active VM SSH key"))?;
    let active_public = PathBuf::from(format!("{}.pub", active_key.to_string_lossy()));

    let generate_output = Command::new(&keygen)
        .args(["-t", "ed25519", "-f"])
        .arg(&temp_stem)
        .args(["-N", "", "-q", "-C", "dune-manager-hyperv"])
        .output()
        .map_err(|err| failure(format!("Failed to run ssh-keygen: {err}")))?;
    if !generate_output.status.success() || !temp_stem.is_file() || !temp_public.is_file() {
        let _ = fs::remove_file(&temp_stem);
        let _ = fs::remove_file(&temp_public);
        return Ok(VendorSshKeyRotationResult {
            key_path: bootstrap_key_path.to_path_buf(),
            public_key_path: None,
            rotated: false,
            message: command_failure(
                "ssh-keygen failed; continuing with the bootstrap key",
                generate_output,
            )
            .message,
        });
    }

    let public_key = fs::read_to_string(&temp_public)
        .map_err(|err| failure(format!("Failed to read generated public key: {err}")))?;
    if let Err(err) = install_guest_public_key(ssh_path, bootstrap_key_path, host, &public_key) {
        let _ = fs::remove_file(&temp_stem);
        let _ = fs::remove_file(&temp_public);
        return Ok(VendorSshKeyRotationResult {
            key_path: bootstrap_key_path.to_path_buf(),
            public_key_path: None,
            rotated: false,
            message: format!(
                "Failed to install the generated SSH key; continuing with the bootstrap key. {}",
                err.message
            ),
        });
    }

    if let Err(err) = verify_guest_key(ssh_path, &temp_stem, host) {
        let _ = fs::remove_file(&temp_stem);
        let _ = fs::remove_file(&temp_public);
        return Ok(VendorSshKeyRotationResult {
            key_path: bootstrap_key_path.to_path_buf(),
            public_key_path: None,
            rotated: false,
            message: format!(
                "The generated SSH key was installed but did not authenticate; continuing with the bootstrap key. {}",
                err.message
            ),
        });
    }

    store_active_vendor_ssh_key(&temp_stem, &temp_public, &active_key, &active_public)?;
    Ok(VendorSshKeyRotationResult {
        key_path: active_key,
        public_key_path: Some(active_public),
        rotated: true,
        message: "Generated and installed a fresh VM SSH key.".to_string(),
    })
}

fn vendor_active_ssh_key_path() -> Option<PathBuf> {
    let local_app_data = env::var_os("LOCALAPPDATA")?;
    Some(
        PathBuf::from(local_app_data)
            .join("DuneAwakeningServer")
            .join("sshKey"),
    )
}

fn prepare_restricted_ssh_key_copy(source: &Path) -> CommandResult<PathBuf> {
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

fn install_guest_public_key(
    ssh_path: &Path,
    bootstrap_key_path: &Path,
    host: &str,
    public_key: &str,
) -> CommandResult<()> {
    let public_key_b64 = base64_encode(format!("{}\n", public_key.trim()).as_bytes());
    let remote_script = format!(
        r#"
set -eu
mkdir -p "$HOME/.ssh"
chmod 700 "$HOME/.ssh"
printf '%s' '{public_key_b64}' | base64 -d > "$HOME/.ssh/authorized_keys.new"
chmod 600 "$HOME/.ssh/authorized_keys.new"
mv "$HOME/.ssh/authorized_keys.new" "$HOME/.ssh/authorized_keys"
echo ROTATE_OK
"#
    );
    let remote_command = format!(
        "printf '%s' '{}' | base64 -d | sh",
        base64_encode(remote_script.as_bytes())
    );
    let output = Command::new(ssh_path)
        .args(openssh_key_rotation_args(bootstrap_key_path, host))
        .arg(remote_command)
        .output()
        .map_err(|err| failure(format!("Failed to run ssh for key rotation: {err}")))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !output.status.success() || !stdout.contains("ROTATE_OK") {
        return Err(command_failure(
            "Failed to install generated SSH public key in the guest",
            output,
        ));
    }
    Ok(())
}

fn verify_guest_key(ssh_path: &Path, key_path: &Path, host: &str) -> CommandResult<()> {
    let output = Command::new(ssh_path)
        .args(openssh_key_rotation_args(key_path, host))
        .arg("true")
        .output()
        .map_err(|err| failure(format!("Failed to verify generated SSH key: {err}")))?;
    if !output.status.success() {
        return Err(command_failure(
            "Generated SSH key did not authenticate to the guest",
            output,
        ));
    }
    Ok(())
}

fn openssh_key_rotation_args(key_path: &Path, host: &str) -> Vec<String> {
    vec![
        "-o".to_string(),
        "BatchMode=yes".to_string(),
        "-o".to_string(),
        "IdentitiesOnly=yes".to_string(),
        "-o".to_string(),
        "PreferredAuthentications=publickey".to_string(),
        "-o".to_string(),
        "StrictHostKeyChecking=no".to_string(),
        "-o".to_string(),
        "UserKnownHostsFile=NUL".to_string(),
        "-o".to_string(),
        "LogLevel=ERROR".to_string(),
        "-o".to_string(),
        "ConnectTimeout=8".to_string(),
        "-i".to_string(),
        key_path.to_string_lossy().to_string(),
        format!("dune@{host}"),
    ]
}

fn store_active_vendor_ssh_key(
    private_source: &Path,
    public_source: &Path,
    private_destination: &Path,
    public_destination: &Path,
) -> CommandResult<()> {
    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
$privateSource = {private_source}
$publicSource = {public_source}
$privateDestination = {private_destination}
$publicDestination = {public_destination}
$keyDir = Split-Path -Parent $privateDestination
New-Item -ItemType Directory -Force -Path $keyDir | Out-Null
foreach ($path in @($privateDestination, $publicDestination)) {{
  if (Test-Path -LiteralPath $path) {{
    takeown /f $path 2>&1 | Out-Null
    icacls $path /reset 2>&1 | Out-Null
    Remove-Item -LiteralPath $path -Force
  }}
}}
Move-Item -LiteralPath $privateSource -Destination $privateDestination -Force
Move-Item -LiteralPath $publicSource -Destination $publicDestination -Force
icacls $privateDestination /inheritance:r | Out-Null
icacls $privateDestination /grant:r "$($env:USERNAME):(R)" | Out-Null
"#,
        private_source = ps_single_quoted(&private_source.to_string_lossy()),
        public_source = ps_single_quoted(&public_source.to_string_lossy()),
        private_destination = ps_single_quoted(&private_destination.to_string_lossy()),
        public_destination = ps_single_quoted(&public_destination.to_string_lossy()),
    );
    run_powershell(&script).map(|_| ())
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = *chunk.get(1).unwrap_or(&0);
        let b2 = *chunk.get(2).unwrap_or(&0);
        encoded.push(TABLE[(b0 >> 2) as usize] as char);
        encoded.push(TABLE[(((b0 & 0b0000_0011) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            encoded.push(TABLE[(((b1 & 0b0000_1111) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            encoded.push('=');
        }
        if chunk.len() > 2 {
            encoded.push(TABLE[(b2 & 0b0011_1111) as usize] as char);
        } else {
            encoded.push('=');
        }
    }
    encoded
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

    #[test]
    fn detects_new_battlegroup_management_layout() {
        let root = temp_package_root("new-layout");
        fs::create_dir_all(root.join("Virtual Machines")).unwrap();
        fs::create_dir_all(root.join("battlegroup-management/ssh")).unwrap();
        fs::create_dir_all(root.join("battlegroup-management/bootstrap")).unwrap();
        fs::write(root.join("battlegroup.bat"), "").unwrap();
        fs::write(root.join("Virtual Machines/test.vmcx"), "").unwrap();
        fs::write(root.join("battlegroup-management/ssh/bundledSshKey"), "key").unwrap();
        fs::write(root.join("battlegroup-management/bootstrap/setup"), "setup").unwrap();

        let layout = detect_server_package_layout(&root).unwrap();

        assert_eq!(layout.layout, ServerPackageLayout::BattlegroupManagement);
        assert!(layout.ssh_key.ends_with("bundledSshKey"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn detects_legacy_internal_scripts_layout() {
        let root = temp_package_root("old-layout");
        fs::create_dir_all(root.join("Virtual Machines")).unwrap();
        fs::create_dir_all(root.join("internal-scripts/ssh")).unwrap();
        fs::create_dir_all(root.join("internal-scripts/bootstrap")).unwrap();
        fs::write(root.join("battlegroup.bat"), "").unwrap();
        fs::write(root.join("Virtual Machines/test.vmcx"), "").unwrap();
        fs::write(root.join("internal-scripts/ssh/sshKey"), "key").unwrap();
        fs::write(root.join("internal-scripts/bootstrap/setup"), "setup").unwrap();

        let layout = detect_server_package_layout(&root).unwrap();

        assert_eq!(layout.layout, ServerPackageLayout::LegacyInternalScripts);
        assert!(layout.ssh_key.ends_with("sshKey"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn parses_manifest_and_public_branch_build_ids() {
        let manifest = r#"
"AppState"
{
  "appid" "3104830"
  "buildid" "23216207"
}
"#;
        assert_eq!(
            parse_vdf_value(manifest, "buildid").as_deref(),
            Some("23216207")
        );

        let app_info = r#"
"depots"
{
  "branches"
  {
    "beta" { "buildid" "1" }
    "public"
    {
      "buildid" "23299999"
    }
  }
}
"#;
        assert_eq!(
            parse_public_branch_build_id(app_info).as_deref(),
            Some("23299999")
        );
    }

    fn temp_package_root(name: &str) -> PathBuf {
        env::temp_dir().join(format!(
            "dune-manager-toolchain-test-{name}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
}
