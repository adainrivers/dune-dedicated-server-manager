use serde::Serialize;

/// Execution provider family used by a documented vendor-flow step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderKind {
    /// Windows Hyper-V host provider.
    HyperV,
    /// Proxmox VE API provider.
    Proxmox,
    /// OpenSSH guest provider.
    Ssh,
    /// Container runtime provider.
    Docker,
    /// Kubernetes API provider.
    Kubernetes,
}

/// Functional domain for a setup or management step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum StepDomain {
    /// Windows host checks and filesystem operations.
    Host,
    /// Hyper-V VM and switch operations.
    HyperV,
    /// Proxmox VM, storage, and network operations.
    Proxmox,
    /// SteamCMD package installation or update.
    Steam,
    /// SSH connectivity and transfer work.
    Ssh,
    /// Guest OS configuration.
    Guest,
    /// Kubernetes resource operations.
    Kubernetes,
    /// Filesystem reads, writes, and template rendering.
    Files,
    /// Browser-opened URLs.
    Browser,
    /// User choice or terminal interaction.
    Interactive,
}

/// Action category for a flow step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum StepAction {
    /// Verify a prerequisite.
    Check,
    /// Discover existing state.
    Detect,
    /// Ask the caller to select an option.
    Choose,
    /// Create a new resource.
    Create,
    /// Import packaged data or resources.
    Import,
    /// Configure an existing resource.
    Configure,
    /// Start a service, VM, or workload.
    Start,
    /// Stop a service, VM, or workload.
    Stop,
    /// Restart a service or workload.
    Restart,
    /// Wait for readiness.
    Wait,
    /// Upload local data to a remote target.
    Upload,
    /// Download package data.
    Download,
    /// Patch a live resource.
    Patch,
    /// Export data to the caller.
    Export,
    /// Open a URL or shell.
    Open,
    /// Start an interactive shell.
    Shell,
    /// Finish a flow.
    Complete,
}

/// One documented step from a vendor flow and its native replacement strategy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowStep {
    /// Stable step id.
    pub id: &'static str,
    /// Human-readable step label.
    pub label: &'static str,
    /// Step domain.
    pub domain: StepDomain,
    /// Step action category.
    pub action: StepAction,
    /// Provider boundary responsible for the native implementation.
    pub provider: ProviderKind,
    /// Vendor script or behavior this step replaces.
    pub source: &'static str,
    /// Native library strategy for the replacement.
    pub native_strategy: &'static str,
    /// Whether this step requires elevated host privileges.
    pub requires_admin: bool,
    /// Whether this step is optional or conditional.
    pub optional: bool,
}

/// Documented end-to-end vendor flow with native replacement steps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowSpec {
    /// Stable flow id.
    pub id: &'static str,
    /// Human-readable flow title.
    pub title: &'static str,
    /// Primary provider family.
    pub provider: ProviderKind,
    /// Vendor scripts used as source references.
    pub source_scripts: &'static [&'static str],
    /// Ordered replacement steps.
    pub steps: Vec<FlowStep>,
}

/// Vendor battlegroup menu command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BattlegroupCommand {
    /// List all battlegroups.
    List,
    /// Show status for a battlegroup.
    Status,
    /// Start a battlegroup.
    Start,
    /// Restart a battlegroup.
    Restart,
    /// Stop a battlegroup.
    Stop,
    /// Apply a downloaded update.
    Update,
    /// Edit battlegroup settings.
    EditBattlegroup,
    /// Advanced manual battlegroup YAML editing.
    EditBattlegroupAdvanced,
    /// Enable vendor experimental swap profile.
    EnableExperimentalSwap,
    /// Back up the battlegroup database.
    BackupDatabase,
    /// Import a battlegroup database backup.
    ImportDatabase,
    /// Export battlegroup logs.
    LogsExport,
    /// Export operator logs.
    OperatorLogsExport,
    /// Open the file browser service.
    OpenFileBrowser,
    /// Open the Director service.
    OpenDirector,
    /// Open an SSH shell on the VM.
    ShellVm,
    /// Open a shell inside a pod.
    ShellPod,
    /// Start the VM.
    StartVm,
    /// Stop the VM.
    StopVm,
    /// Exit the menu.
    Quit,
}

/// Native replacement plan for a vendor battlegroup command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BattlegroupCommandSpec {
    /// Command enum value.
    pub command: BattlegroupCommand,
    /// Vendor menu name.
    pub menu_name: &'static str,
    /// Human-readable command description.
    pub description: &'static str,
    /// Steps used to implement the command natively.
    pub steps: Vec<FlowStep>,
}

impl BattlegroupCommandSpec {
    fn new(
        command: BattlegroupCommand,
        menu_name: &'static str,
        description: &'static str,
        steps: Vec<FlowStep>,
    ) -> Self {
        Self {
            command,
            menu_name,
            description,
            steps,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct StepFlags {
    requires_admin: bool,
    optional: bool,
}

impl StepFlags {
    const fn new(requires_admin: bool, optional: bool) -> Self {
        Self {
            requires_admin,
            optional,
        }
    }
}

fn step(
    id: &'static str,
    label: &'static str,
    domain: StepDomain,
    action: StepAction,
    source: &'static str,
    native_strategy: &'static str,
    flags: StepFlags,
) -> FlowStep {
    FlowStep {
        id,
        label,
        domain,
        action,
        provider: ProviderKind::HyperV,
        source,
        native_strategy,
        requires_admin: flags.requires_admin,
        optional: flags.optional,
    }
}

/// Returns the native replacement map for the vendor initial setup scripts.
pub fn hyperv_initial_setup_flow() -> FlowSpec {
    FlowSpec {
        id: "vendor.initial-setup.hyperv",
        title: "Initial Hyper-V setup",
        provider: ProviderKind::HyperV,
        source_scripts: &[
            "initial-setup.bat",
            "internal-scripts/initial-setup.ps1",
            "internal-scripts/bootstrap/setup",
            "battlegroup-management/initial-setup.ps1",
            "battlegroup-management/bootstrap/setup",
            "download/scripts/setup.sh",
            "download/scripts/setup/k3s.sh",
            "download/scripts/setup/system.sh",
            "download/scripts/setup/world.sh",
            "download/scripts/battlegroup.sh update-from-downloads",
            "download/scripts/battlegroup.sh apply-default-usersettings",
        ],
        steps: vec![
            step(
                "host.require-admin",
                "Require elevated host privileges",
                StepDomain::Host,
                StepAction::Check,
                "initial-setup.ps1 #Requires -RunAsAdministrator",
                "Windows token elevation check",
                StepFlags::new(true, false),
            ),
            step(
                "host.check-hyperv-module",
                "Check Hyper-V module",
                StepDomain::Host,
                StepAction::Check,
                "Get-Module -ListAvailable -Name Hyper-V",
                "Windows capability/provider check; strict JSON fallback is allowed",
                StepFlags::new(true, false),
            ),
            step(
                "host.check-vmms",
                "Check vmms service",
                StepDomain::Host,
                StepAction::Check,
                "Get-Service vmms",
                "Windows service query; strict JSON fallback is allowed",
                StepFlags::new(true, false),
            ),
            step(
                "package.locate-vmcx",
                "Locate packaged VM configuration",
                StepDomain::Files,
                StepAction::Detect,
                "Virtual Machines/*.vmcx",
                "Rust filesystem glob",
                StepFlags::new(false, false),
            ),
            step(
                "host.select-destination",
                "Select VM files destination",
                StepDomain::Files,
                StepAction::Choose,
                "Get-PSDrive free > 100GB",
                "Rust drive/disk discovery plus caller-selected destination",
                StepFlags::new(false, false),
            ),
            step(
                "hyperv.detect-existing-vm",
                "Detect existing vendor VM",
                StepDomain::HyperV,
                StepAction::Detect,
                "Get-VM -Name <vendor-vm-name>",
                "Hyper-V provider get_vm",
                StepFlags::new(true, false),
            ),
            step(
                "hyperv.stop-existing-vm",
                "Stop existing VM before destructive replacement",
                StepDomain::HyperV,
                StepAction::Stop,
                "Stop-VM -TurnOff -Force",
                "Hyper-V provider stop_vm(turn_off=true)",
                StepFlags::new(true, true),
            ),
            step(
                "hyperv.remove-existing-vm",
                "Remove existing VM",
                StepDomain::HyperV,
                StepAction::Stop,
                "Remove-VM -Force",
                "Hyper-V provider remove_vm",
                StepFlags::new(true, true),
            ),
            step(
                "host.clear-vm-destination",
                "Clear selected VM destination",
                StepDomain::Files,
                StepAction::Configure,
                "Remove-Item destination",
                "Rust filesystem delete guarded by resolved-path containment and confirmation",
                StepFlags::new(false, true),
            ),
            step(
                "hyperv.compare-vm",
                "Compare packaged VM compatibility",
                StepDomain::HyperV,
                StepAction::Check,
                "Compare-VM -Copy",
                "Hyper-V provider compare_import",
                StepFlags::new(true, false),
            ),
            step(
                "hyperv.import-vm",
                "Import VM copy",
                StepDomain::HyperV,
                StepAction::Import,
                "Import-VM -CompatibilityReport",
                "Hyper-V provider import_vm_copy",
                StepFlags::new(true, false),
            ),
            step(
                "host.detect-physical-nics",
                "Detect active physical network adapters",
                StepDomain::Host,
                StepAction::Detect,
                "Get-NetAdapter Status Up excluding Hyper-V/Virtual",
                "Network adapter provider with IPv4/range metadata",
                StepFlags::new(true, false),
            ),
            step(
                "hyperv.choose-or-create-switch",
                "Choose or create external switch",
                StepDomain::HyperV,
                StepAction::Create,
                "Get-VMSwitch/New-VMSwitch",
                "Hyper-V provider ensure_external_switch",
                StepFlags::new(true, false),
            ),
            step(
                "hyperv.connect-switch",
                "Connect VM adapter to switch",
                StepDomain::HyperV,
                StepAction::Configure,
                "Connect-VMNetworkAdapter",
                "Hyper-V provider connect_network_adapter",
                StepFlags::new(true, false),
            ),
            step(
                "hyperv.resize-vhd",
                "Resize virtual disk to 100GB",
                StepDomain::HyperV,
                StepAction::Configure,
                "Resize-VHD -SizeBytes 100GB",
                "Hyper-V provider resize_vhd",
                StepFlags::new(true, false),
            ),
            step(
                "hyperv.set-first-boot",
                "Set VM first boot device",
                StepDomain::HyperV,
                StepAction::Configure,
                "Set-VMFirmware -FirstBootDevice",
                "Hyper-V provider set_first_boot_disk",
                StepFlags::new(true, false),
            ),
            step(
                "hyperv.choose-memory",
                "Choose VM memory profile",
                StepDomain::HyperV,
                StepAction::Choose,
                "20/30/40GB Read-Host menu",
                "Caller-selected memory profile enum",
                StepFlags::new(false, false),
            ),
            step(
                "hyperv.set-memory",
                "Set VM startup memory",
                StepDomain::HyperV,
                StepAction::Configure,
                "Set-VMMemory",
                "Hyper-V provider set_startup_memory",
                StepFlags::new(true, false),
            ),
            step(
                "hyperv.start-vm",
                "Start VM",
                StepDomain::HyperV,
                StepAction::Start,
                "Start-VM",
                "Hyper-V provider start_vm",
                StepFlags::new(true, false),
            ),
            step(
                "hyperv.wait-ip",
                "Wait for VM IPv4 address",
                StepDomain::HyperV,
                StepAction::Wait,
                "Get-VMNetworkAdapter IPAddresses loop",
                "Hyper-V provider wait_ipv4",
                StepFlags::new(true, false),
            ),
            step(
                "ssh.prepare-key",
                "Seed and rotate VM SSH key",
                StepDomain::Ssh,
                StepAction::Configure,
                "Update-SshKey -UseBundledKey",
                "Rust key rotation into host-local active VM key",
                StepFlags::new(false, false),
            ),
            step(
                "guest.choose-ip-mode",
                "Choose DHCP or static guest networking",
                StepDomain::Guest,
                StepAction::Choose,
                "Read-Host DHCP/static menu",
                "Caller-selected network mode",
                StepFlags::new(false, false),
            ),
            step(
                "guest.apply-static-network",
                "Apply static Alpine network config",
                StepDomain::Guest,
                StepAction::Configure,
                "/etc/network/interfaces and /etc/resolv.conf via SSH",
                "SSH script with structured success marker",
                StepFlags::new(false, true),
            ),
            step(
                "guest.wait-static-ssh",
                "Wait for SSH after network restart",
                StepDomain::Ssh,
                StepAction::Wait,
                "ssh true loop",
                "SSH executor wait_ready",
                StepFlags::new(false, true),
            ),
            step(
                "guest.detect-public-ip",
                "Detect public IP from VM",
                StepDomain::Guest,
                StepAction::Detect,
                "wget api.ipify.org",
                "detect_player_address_candidates via GuestProvider::detect_public_ip",
                StepFlags::new(false, true),
            ),
            step(
                "guest.select-player-ip",
                "Select player-facing IP",
                StepDomain::Guest,
                StepAction::Choose,
                "public/private/manual Read-Host menu",
                "Caller selects from PlayerAddressCandidates or supplies manual player-facing IP",
                StepFlags::new(false, false),
            ),
            step(
                "guest.write-settings-conf",
                "Write player-facing IP settings",
                StepDomain::Guest,
                StepAction::Configure,
                "printf '\\n\\n\\n$selectedIP\\n' > settings.conf",
                "SSH file write with exact settings format",
                StepFlags::new(false, false),
            ),
            step(
                "guest.upload-bootstrap",
                "Upload bootstrap setup helper",
                StepDomain::Ssh,
                StepAction::Upload,
                "base64 tee /home/dune/.dune/bin/setup",
                "SSH stdin upload and chmod",
                StepFlags::new(false, false),
            ),
            step(
                "guest.validate-disk",
                "Validate and grow root disk",
                StepDomain::Guest,
                StepAction::Configure,
                "bootstrap/setup validate_disk_space",
                "Guest disk provider using df/growpart/lvm/resize2fs",
                StepFlags::new(false, false),
            ),
            step(
                "guest.download-payload",
                "Download guest server payload",
                StepDomain::Steam,
                StepAction::Download,
                "steamcmd app_update 3104830",
                "Guest SteamCMD executor with retry policy",
                StepFlags::new(false, false),
            ),
            step(
                "guest.k3s.start",
                "Start k3s",
                StepDomain::Guest,
                StepAction::Start,
                "setup/k3s.sh rc-service k3s start",
                "OpenRC service provider start/wait",
                StepFlags::new(false, false),
            ),
            step(
                "guest.k3s.import-core-images",
                "Import k3s prerequisite images",
                StepDomain::Guest,
                StepAction::Import,
                "ctr images import prerequisites",
                "Container image import step with retry/restart policy",
                StepFlags::new(false, false),
            ),
            step(
                "guest.k3s.scale-core",
                "Scale core k3s deployments",
                StepDomain::Kubernetes,
                StepAction::Configure,
                "kubectl scale coredns/local-path/metrics/cert-manager",
                "Kubernetes client scale deployments",
                StepFlags::new(false, false),
            ),
            step(
                "guest.operators.update-crds",
                "Replace/apply operator CRDs",
                StepDomain::Kubernetes,
                StepAction::Configure,
                "kubectl replace/apply images/operators/crds",
                "Kubernetes apply server-side manifests",
                StepFlags::new(false, false),
            ),
            step(
                "guest.operators.patch-images",
                "Patch operator deployment images",
                StepDomain::Kubernetes,
                StepAction::Patch,
                "kubectl set image operators",
                "Kubernetes patch deployments",
                StepFlags::new(false, false),
            ),
            step(
                "guest.operators.scale",
                "Scale operator deployments",
                StepDomain::Kubernetes,
                StepAction::Configure,
                "kubectl scale funcom-operators",
                "Kubernetes client scale deployments",
                StepFlags::new(false, false),
            ),
            step(
                "guest.system.install-battlegroup-helper",
                "Install battlegroup helper symlink",
                StepDomain::Guest,
                StepAction::Configure,
                "setup/system.sh ln -s battlegroup.sh",
                "Guest filesystem symlink creation",
                StepFlags::new(false, false),
            ),
            step(
                "guest.world.read-region",
                "Choose world region",
                StepDomain::Interactive,
                StepAction::Choose,
                "world.sh Europe Test/North America Test",
                "Caller-selected region enum",
                StepFlags::new(false, false),
            ),
            step(
                "guest.world.read-name",
                "Choose world name",
                StepDomain::Interactive,
                StepAction::Choose,
                "world.sh world name prompt",
                "Caller-selected validated world name",
                StepFlags::new(false, false),
            ),
            step(
                "guest.world.read-token",
                "Read self-host token",
                StepDomain::Interactive,
                StepAction::Choose,
                "world.sh self-host token prompt",
                "Secret input never persisted",
                StepFlags::new(false, false),
            ),
            step(
                "guest.world.derive-host-id",
                "Derive HostId from token",
                StepDomain::Guest,
                StepAction::Configure,
                "Rust JWT payload decode HostId lowercase",
                "GuestBootstrapPlan::from_self_host_token extracts HostId with secret redaction",
                StepFlags::new(false, false),
            ),
            step(
                "guest.world.generate-name",
                "Generate world unique name",
                StepDomain::Guest,
                StepAction::Create,
                "sh-$HostId- six lowercase random letters",
                "GuestBootstrapPlan::from_self_host_token generates lowercase-only vendor suffix",
                StepFlags::new(false, false),
            ),
            step(
                "guest.world.render-manifests",
                "Render world and secret manifests",
                StepDomain::Files,
                StepAction::Configure,
                "sed substitutions on templates",
                "Structured template renderer preserving vendor YAML",
                StepFlags::new(false, false),
            ),
            step(
                "guest.world.wait-operators",
                "Wait for four operators ready",
                StepDomain::Kubernetes,
                StepAction::Wait,
                "kubectl deployment ready loop",
                "Kubernetes wait deployments",
                StepFlags::new(false, false),
            ),
            step(
                "guest.world.create-resources",
                "Create namespace, secrets, and BattleGroup",
                StepDomain::Kubernetes,
                StepAction::Create,
                "kubectl create ns/secret/battlegroup",
                "Kubernetes create resources",
                StepFlags::new(false, false),
            ),
            step(
                "guest.images.import-battlegroup",
                "Import battlegroup container images",
                StepDomain::Guest,
                StepAction::Import,
                "battlegroup.sh update-from-downloads image imports",
                "Container image import by manifest version",
                StepFlags::new(false, false),
            ),
            step(
                "guest.images.patch-battlegroup",
                "Patch BattleGroup image tags",
                StepDomain::Kubernetes,
                StepAction::Patch,
                "Rust JSON patch generation + kubectl patch",
                "Kubernetes JSON patch generated from live resource",
                StepFlags::new(false, false),
            ),
            step(
                "guest.defaults.wait-filebrowser",
                "Wait for filebrowser pod",
                StepDomain::Kubernetes,
                StepAction::Wait,
                "battlegroup.sh apply-default-usersettings",
                "Kubernetes pod wait by label",
                StepFlags::new(false, false),
            ),
            step(
                "guest.defaults.copy-user-settings",
                "Copy default UserSettings files",
                StepDomain::Kubernetes,
                StepAction::Configure,
                "kubectl cp UserEngine.ini/UserGame.ini",
                "Kubernetes copy/exec to mounted filebrowser data",
                StepFlags::new(false, false),
            ),
            step(
                "setup.complete",
                "Initial setup complete",
                StepDomain::Host,
                StepAction::Complete,
                "initial-setup.ps1 final message",
                "Flow result",
                StepFlags::new(false, false),
            ),
        ],
    }
}

/// Returns the native replacement map for the vendor battlegroup menu shell.
pub fn battlegroup_management_flow() -> FlowSpec {
    FlowSpec {
        id: "vendor.battlegroup.hyperv",
        title: "Battlegroup management",
        provider: ProviderKind::HyperV,
        source_scripts: &[
            "battlegroup.bat",
            "internal-scripts/battlegroup.ps1",
            "battlegroup-management/battlegroup.ps1",
            "download/scripts/battlegroup.sh",
        ],
        steps: vec![
            step(
                "bg.host.require-admin",
                "Require elevated host privileges",
                StepDomain::Host,
                StepAction::Check,
                "battlegroup.ps1 #Requires -RunAsAdministrator",
                "Split into admin VM operations and non-admin guest operations",
                StepFlags::new(true, false),
            ),
            step(
                "bg.hyperv.get-vm",
                "Load vendor VM",
                StepDomain::HyperV,
                StepAction::Detect,
                "Get-VM -Name <vendor-vm-name>",
                "Hyper-V provider get_vm",
                StepFlags::new(true, false),
            ),
            step(
                "bg.ssh.prepare-key",
                "Prepare active or bootstrap SSH key",
                StepDomain::Ssh,
                StepAction::Configure,
                "%LOCALAPPDATA% active key, falling back to bundled bootstrap key",
                "Rust key candidate manager with Windows ACL helper",
                StepFlags::new(false, false),
            ),
            step(
                "bg.hyperv.get-ip-if-running",
                "Read VM IPv4 when running",
                StepDomain::HyperV,
                StepAction::Detect,
                "Get-VMNetworkAdapter IPAddresses",
                "Hyper-V provider vm_ipv4",
                StepFlags::new(true, true),
            ),
            step(
                "bg.menu.dispatch",
                "Dispatch selected command",
                StepDomain::Interactive,
                StepAction::Choose,
                "Read-Host menu",
                "Typed command enum",
                StepFlags::new(false, false),
            ),
        ],
    }
}

fn battlegroup_kubernetes_step(
    id: &'static str,
    description: &'static str,
    action: StepAction,
    source: &'static str,
    native_strategy: &'static str,
) -> FlowStep {
    let mut flow_step = step(
        id,
        description,
        StepDomain::Kubernetes,
        action,
        source,
        native_strategy,
        StepFlags::new(false, false),
    );
    flow_step.provider = ProviderKind::Kubernetes;
    flow_step
}

/// Returns the catalog of supported battlegroup management commands.
pub fn battlegroup_command_catalog() -> Vec<BattlegroupCommandSpec> {
    vec![
        BattlegroupCommandSpec::new(
            BattlegroupCommand::List,
            "list",
            "Lists all available battlegroups",
            vec![battlegroup_kubernetes_step(
                "bg.command.list",
                "List battlegroups",
                StepAction::Detect,
                "kubectl get battlegroups -A -o json",
                "StructuredBattlegroupOps::list",
            )],
        ),
        BattlegroupCommandSpec::new(
            BattlegroupCommand::Status,
            "status",
            "Shows the status of the selected battlegroup",
            vec![battlegroup_kubernetes_step(
                "bg.command.status",
                "Read battlegroup status snapshot",
                StepAction::Detect,
                "kubectl get battlegroup/pods/services -o json",
                "StructuredBattlegroupOps::status",
            )],
        ),
        BattlegroupCommandSpec::new(
            BattlegroupCommand::Start,
            "start",
            "Starts the selected battlegroup",
            vec![
                battlegroup_kubernetes_step(
                    "bg.command.start",
                    "Start battlegroup",
                    StepAction::Start,
                    "kubectl patch battlegroup spec.stop=false",
                    "BattlegroupManagementOrchestrator::start_and_wait_director",
                ),
                step(
                    "bg.director.wait-port-after-start",
                    "Wait for Director NodePort",
                    StepDomain::Kubernetes,
                    StepAction::Wait,
                    "kubectl get svc port 11717 nodePort",
                    "Kubernetes service discovery",
                    StepFlags::new(false, false),
                ),
            ],
        ),
        BattlegroupCommandSpec::new(
            BattlegroupCommand::Restart,
            "restart",
            "Restarts the selected battlegroup",
            vec![
                battlegroup_kubernetes_step(
                    "bg.command.restart.stop",
                    "Stop battlegroup for restart",
                    StepAction::Stop,
                    "kubectl patch battlegroup spec.stop=true",
                    "BattlegroupManagementOrchestrator::restart_and_wait_director",
                ),
                battlegroup_kubernetes_step(
                    "bg.command.restart.start",
                    "Start battlegroup after restart",
                    StepAction::Start,
                    "kubectl patch battlegroup spec.stop=false",
                    "BattlegroupManagementOrchestrator::restart_and_wait_director",
                ),
                step(
                    "bg.director.wait-port-after-restart",
                    "Wait for Director NodePort",
                    StepDomain::Kubernetes,
                    StepAction::Wait,
                    "kubectl get svc port 11717 nodePort",
                    "Kubernetes service discovery",
                    StepFlags::new(false, false),
                ),
            ],
        ),
        BattlegroupCommandSpec::new(
            BattlegroupCommand::Stop,
            "stop",
            "Stops the selected battlegroup",
            vec![battlegroup_kubernetes_step(
                "bg.command.stop",
                "Stop battlegroup",
                StepAction::Stop,
                "kubectl patch battlegroup spec.stop=true",
                "BattlegroupManagementOrchestrator::stop",
            )],
        ),
        BattlegroupCommandSpec::new(
            BattlegroupCommand::Update,
            "update",
            "Checks for new versions and applies them",
            vec![
                step(
                    "bg.command.update.import-images",
                    "Import downloaded battlegroup images",
                    StepDomain::Guest,
                    StepAction::Import,
                    "ctr -n k8s.io images import downloaded battlegroup tars",
                    "BattlegroupUpdateOrchestrator::update_from_downloads",
                    StepFlags::new(false, false),
                ),
                battlegroup_kubernetes_step(
                    "bg.command.update.patch-images",
                    "Patch battlegroup image revisions",
                    StepAction::Patch,
                    "kubectl patch battlegroup image tags",
                    "BattlegroupUpdateOrchestrator::update_from_downloads",
                ),
            ],
        ),
        BattlegroupCommandSpec::new(
            BattlegroupCommand::EditBattlegroup,
            "edit-battlegroup",
            "Edit settings of the battlegroup",
            vec![
                step(
                    "bg.edit.discover-namespace",
                    "Discover battlegroup namespace",
                    StepDomain::Kubernetes,
                    StepAction::Detect,
                    "kubectl get ns grep funcom-seabass",
                    "Kubernetes namespace list",
                    StepFlags::new(false, false),
                ),
                step(
                    "bg.edit.region",
                    "Patch region settings",
                    StepDomain::Kubernetes,
                    StepAction::Patch,
                    "kubectl get battlegroup -o json; Rust JSON patch; kubectl patch",
                    "StructuredBattlegroupOps::patch_region",
                    StepFlags::new(false, false),
                ),
            ],
        ),
        BattlegroupCommandSpec::new(
            BattlegroupCommand::EditBattlegroupAdvanced,
            "edit-battlegroup-advanced",
            "Manually edit the live battlegroup YAML",
            vec![step(
                "bg.edit-advanced.open",
                "Open advanced battlegroup YAML editor",
                StepDomain::Kubernetes,
                StepAction::Shell,
                "kubectl edit battlegroup",
                "Future guarded native YAML/diff editor; currently vendor capability metadata",
                StepFlags::new(false, true),
            )],
        ),
        BattlegroupCommandSpec::new(
            BattlegroupCommand::EnableExperimentalSwap,
            "enable-experimental-swap",
            "Enable experimental swap memory profile",
            vec![step(
                "bg.swap.enable",
                "Enable guest swap and patch BattleGroup memory",
                StepDomain::Guest,
                StepAction::Configure,
                "/home/dune/.dune/bin/battlegroup enable-experimental-swap",
                "ExperimentalSwapOrchestrator::enable",
                StepFlags::new(false, true),
            )],
        ),
        BattlegroupCommandSpec::new(
            BattlegroupCommand::BackupDatabase,
            "backup",
            "Take a database backup",
            vec![step(
                "bg.database.backup",
                "Take database backup",
                StepDomain::Kubernetes,
                StepAction::Export,
                "/home/dune/.dune/bin/battlegroup backup",
                "Future native database backup workflow",
                StepFlags::new(false, true),
            )],
        ),
        BattlegroupCommandSpec::new(
            BattlegroupCommand::ImportDatabase,
            "import",
            "Import a database backup",
            vec![step(
                "bg.database.import",
                "Import database backup",
                StepDomain::Kubernetes,
                StepAction::Import,
                "/home/dune/.dune/bin/battlegroup import",
                "Future guarded native database import workflow",
                StepFlags::new(false, true),
            )],
        ),
        BattlegroupCommandSpec::new(
            BattlegroupCommand::LogsExport,
            "logs-export",
            "Retrieves logs from all pods in the selected battlegroup",
            vec![battlegroup_kubernetes_step(
                "bg.command.logs-export",
                "Collect battlegroup pod logs",
                StepAction::Export,
                "kubectl get pods -o json; kubectl logs per container",
                "StructuredBattlegroupOps::export_namespace_logs",
            )],
        ),
        BattlegroupCommandSpec::new(
            BattlegroupCommand::OperatorLogsExport,
            "operator-logs-export",
            "Retrieves logs from all operator pods",
            vec![battlegroup_kubernetes_step(
                "bg.command.operator-logs-export",
                "Collect operator pod logs",
                StepAction::Export,
                "kubectl get pods -n funcom-operators -o json; kubectl logs per container",
                "StructuredBattlegroupOps::export_operator_logs",
            )],
        ),
        BattlegroupCommandSpec::new(
            BattlegroupCommand::OpenFileBrowser,
            "open-file-browser",
            "Open the file browser in a web browser",
            vec![step(
                "bg.filebrowser.open",
                "Open File Browser URL",
                StepDomain::Browser,
                StepAction::Open,
                "Start-Process http://ip:18888",
                "Return URL to caller; UI opens it",
                StepFlags::new(false, false),
            )],
        ),
        BattlegroupCommandSpec::new(
            BattlegroupCommand::OpenDirector,
            "open-director",
            "Open the director web app in a web browser",
            vec![
                step(
                    "bg.director.discover-port",
                    "Discover Director NodePort",
                    StepDomain::Kubernetes,
                    StepAction::Detect,
                    "kubectl get svc port 11717 nodePort",
                    "Kubernetes service discovery",
                    StepFlags::new(false, false),
                ),
                step(
                    "bg.director.open",
                    "Open Director URL",
                    StepDomain::Browser,
                    StepAction::Open,
                    "Start-Process http://ip:nodePort",
                    "Return URL to caller; UI opens it",
                    StepFlags::new(false, false),
                ),
            ],
        ),
        BattlegroupCommandSpec::new(
            BattlegroupCommand::ShellVm,
            "shell-vm",
            "Open an SSH shell on the VM",
            vec![step(
                "bg.shell.vm",
                "Open VM shell",
                StepDomain::Ssh,
                StepAction::Shell,
                "ssh -t dune@ip",
                "Interactive terminal adapter",
                StepFlags::new(false, false),
            )],
        ),
        BattlegroupCommandSpec::new(
            BattlegroupCommand::ShellPod,
            "shell-pod",
            "Open a shell inside a pod of the selected battlegroup",
            vec![
                step(
                    "bg.shell-pod.discover",
                    "Discover namespace and pods",
                    StepDomain::Kubernetes,
                    StepAction::Detect,
                    "kubectl get pods -n ns",
                    "Kubernetes pod list",
                    StepFlags::new(false, false),
                ),
                step(
                    "bg.shell-pod.open",
                    "Open pod shell",
                    StepDomain::Kubernetes,
                    StepAction::Shell,
                    "kubectl exec -it pod -- bash/sh",
                    "Interactive Kubernetes exec adapter",
                    StepFlags::new(false, false),
                ),
            ],
        ),
        BattlegroupCommandSpec::new(
            BattlegroupCommand::StartVm,
            "start-vm",
            "Start the VM",
            vec![
                step(
                    "bg.vm.start",
                    "Start VM",
                    StepDomain::HyperV,
                    StepAction::Start,
                    "Start-VM",
                    "Hyper-V provider start_vm",
                    StepFlags::new(true, false),
                ),
                step(
                    "bg.vm.wait-ip",
                    "Wait for VM IPv4",
                    StepDomain::HyperV,
                    StepAction::Wait,
                    "Get-VMNetworkAdapter IPAddresses loop",
                    "Hyper-V provider wait_ipv4",
                    StepFlags::new(true, false),
                ),
            ],
        ),
        BattlegroupCommandSpec::new(
            BattlegroupCommand::StopVm,
            "stop-vm",
            "Stop the VM",
            vec![step(
                "bg.vm.stop",
                "Stop VM",
                StepDomain::HyperV,
                StepAction::Stop,
                "Stop-VM -Force",
                "Hyper-V provider stop_vm",
                StepFlags::new(true, false),
            )],
        ),
        BattlegroupCommandSpec::new(
            BattlegroupCommand::Quit,
            "quit",
            "Exit this script",
            vec![step(
                "bg.quit",
                "Quit menu",
                StepDomain::Interactive,
                StepAction::Complete,
                "break loop",
                "Return control to caller",
                StepFlags::new(false, false),
            )],
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_setup_flow_covers_vendor_milestones() {
        let flow = hyperv_initial_setup_flow();
        let ids = flow.steps.iter().map(|step| step.id).collect::<Vec<_>>();
        for required in [
            "host.check-hyperv-module",
            "hyperv.import-vm",
            "hyperv.choose-or-create-switch",
            "guest.write-settings-conf",
            "guest.upload-bootstrap",
            "guest.k3s.import-core-images",
            "guest.system.install-battlegroup-helper",
            "guest.world.generate-name",
            "guest.images.patch-battlegroup",
            "guest.defaults.copy-user-settings",
        ] {
            assert!(ids.contains(&required), "missing step {required}");
        }
    }

    #[test]
    fn battlegroup_catalog_matches_vendor_menu_names() {
        let names = battlegroup_command_catalog()
            .into_iter()
            .map(|command| command.menu_name)
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![
                "list",
                "status",
                "start",
                "restart",
                "stop",
                "update",
                "edit-battlegroup",
                "edit-battlegroup-advanced",
                "enable-experimental-swap",
                "backup",
                "import",
                "logs-export",
                "operator-logs-export",
                "open-file-browser",
                "open-director",
                "shell-vm",
                "shell-pod",
                "start-vm",
                "stop-vm",
                "quit",
            ]
        );
    }

    #[test]
    fn battlegroup_catalog_points_to_native_replacements() {
        let catalog = battlegroup_command_catalog();
        let text = catalog
            .iter()
            .flat_map(|command| command.steps.iter())
            .map(|step| format!("{} {}", step.source, step.native_strategy))
            .collect::<Vec<_>>()
            .join("\n");

        assert!(!text.contains("/home/dune/.dune/bin/battlegroup command"));
        assert!(!text.contains("until replaced"));
        assert!(!text.contains("sed | replace"));
        for required in [
            "StructuredBattlegroupOps::list",
            "StructuredBattlegroupOps::status",
            "BattlegroupManagementOrchestrator::start_and_wait_director",
            "BattlegroupUpdateOrchestrator::update_from_downloads",
            "StructuredBattlegroupOps::patch_region",
            "StructuredBattlegroupOps::export_namespace_logs",
            "StructuredBattlegroupOps::export_operator_logs",
            "Hyper-V provider start_vm",
            "Hyper-V provider stop_vm",
        ] {
            assert!(
                text.contains(required),
                "missing native strategy {required}"
            );
        }
    }

    #[test]
    fn world_name_generation_preserves_vendor_lowercase_suffix_requirement() {
        let flow = hyperv_initial_setup_flow();
        let step = flow
            .steps
            .iter()
            .find(|step| step.id == "guest.world.generate-name")
            .expect("world generation step");
        assert!(step.native_strategy.contains("lowercase-only"));
        assert!(step.source.contains("six lowercase random letters"));
    }
}
