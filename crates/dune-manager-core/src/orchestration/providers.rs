//! Provider traits and shared data models for host, VM, guest, and Kubernetes operations.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{errors::failure, models::CommandResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
/// Normalized VM power state across host providers.
pub enum VmPowerState {
    /// No VM record was found.
    Missing,
    /// VM is powered off.
    Off,
    /// VM is transitioning to running.
    Starting,
    /// VM is running.
    Running,
    /// VM is transitioning to off.
    Stopping,
    /// VM is saved/checkpointed by Hyper-V.
    Saved,
    /// VM is paused.
    Paused,
    /// Hyper-V returned an unrecognized state.
    Other,
}

impl VmPowerState {
    /// Maps a raw Hyper-V state string into the normalized enum.
    pub fn from_hyperv_state(value: &str) -> Self {
        match value.to_ascii_lowercase().as_str() {
            "" => Self::Missing,
            "off" => Self::Off,
            "starting" => Self::Starting,
            "running" => Self::Running,
            "stopping" => Self::Stopping,
            "saved" => Self::Saved,
            "paused" => Self::Paused,
            _ => Self::Other,
        }
    }
}

/// Host readiness information required for Hyper-V setup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostReadiness {
    /// Whether the current process has elevated privileges.
    pub elevated: bool,
    /// Whether the Hyper-V PowerShell module is available.
    pub hyperv_available: bool,
    /// Whether the Hyper-V VM management service is running.
    pub vmms_running: bool,
    /// Whether firmware virtualization is enabled, when the host can report it.
    pub virtualization_firmware_enabled: Option<bool>,
    /// Total physical memory on the host in bytes.
    pub total_physical_memory_bytes: u64,
    /// Currently available physical memory on the host in bytes.
    pub available_physical_memory_bytes: u64,
    /// Logical processor count reported by the host.
    pub logical_processor_count: u32,
}

/// Host drive candidate suitable for placing VM files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveCandidate {
    /// Display name for the drive.
    pub name: String,
    /// Root path such as `C:\`.
    pub root: String,
    /// Available free space in bytes.
    pub free_bytes: u64,
}

/// Physical IPv4 network adapter candidate for a Hyper-V external switch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkAdapterCandidate {
    /// Adapter name used by Hyper-V commands.
    pub name: String,
    /// Adapter hardware/interface description.
    pub interface_description: String,
    /// Active IPv4 address on the adapter.
    pub ipv4_address: String,
    /// IPv4 CIDR prefix length.
    pub prefix_length: u8,
    /// Default gateway for the adapter.
    pub gateway: String,
    /// Suggested static IPv4 address for a VM on this adapter subnet.
    pub suggested_ipv4_address: String,
    /// Existing external switch bound to this adapter, if any.
    pub existing_external_switch: String,
}

/// Hyper-V external switch record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalSwitch {
    /// Switch name.
    pub name: String,
    /// Adapter description backing the switch.
    pub net_adapter_interface_description: String,
}

/// VM inventory snapshot from the host provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VmInventoryRecord {
    /// VM name.
    pub name: String,
    /// Normalized power state.
    pub state: VmPowerState,
    /// Raw provider-specific power state.
    pub raw_state: String,
    /// Hyper-V configuration location.
    pub configuration_location: String,
    /// VM path.
    pub path: String,
    /// Assigned memory in bytes.
    pub memory_assigned_bytes: u64,
    /// Virtual processor count assigned to the VM.
    pub processor_count: u32,
    /// Uptime in seconds.
    pub uptime_seconds: u64,
    /// IPv4 addresses reported for the VM.
    pub ipv4_addresses: Vec<String>,
    /// Attached virtual hard disk paths.
    pub hard_disk_paths: Vec<String>,
    /// Sum of attached virtual hard disk maximum sizes in bytes.
    pub disk_size_bytes: u64,
    /// Sum of attached virtual hard disk file sizes in bytes.
    pub disk_file_size_bytes: u64,
    /// Connected Hyper-V switch names.
    pub switch_names: Vec<String>,
}

/// Compatibility result for importing a packaged VM.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VmCompatibilityReport {
    /// Whether the VM can be imported.
    pub compatible: bool,
    /// Human-readable incompatibility reasons.
    pub incompatibilities: Vec<String>,
}

/// Result of importing a VM.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportedVm {
    /// Imported VM name.
    pub name: String,
    /// Hyper-V configuration location.
    pub configuration_location: String,
}

/// Request to import a packaged VM into a destination directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmImportRequest {
    /// Source `.vmcx` path.
    pub vmcx_path: String,
    /// Destination path for the imported VM files.
    pub destination_path: String,
}

/// Request to create or locate a Hyper-V external switch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnsureSwitchRequest {
    /// Desired switch name.
    pub switch_name: String,
    /// Host adapter to bind.
    pub adapter_name: String,
}

/// Static guest network configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuestNetworkConfig {
    /// Guest interface name.
    pub interface: String,
    /// Static address with CIDR prefix.
    pub address_cidr: String,
    /// Static gateway.
    pub gateway: String,
    /// DNS server list or value.
    pub dns: String,
}

/// Host-level discovery provider.
pub trait HostProvider {
    /// Returns host readiness information.
    fn readiness(&self) -> CommandResult<HostReadiness>;
    /// Lists drives with at least the requested free space.
    fn drives_with_minimum_free_space(
        &self,
        minimum_free_bytes: u64,
    ) -> CommandResult<Vec<DriveCandidate>>;
    /// Lists active physical IPv4 adapters suitable for setup.
    fn active_physical_adapters(&self) -> CommandResult<Vec<NetworkAdapterCandidate>>;
}

impl<T> HostProvider for &T
where
    T: HostProvider + ?Sized,
{
    fn readiness(&self) -> CommandResult<HostReadiness> {
        (*self).readiness()
    }

    fn drives_with_minimum_free_space(
        &self,
        minimum_free_bytes: u64,
    ) -> CommandResult<Vec<DriveCandidate>> {
        (*self).drives_with_minimum_free_space(minimum_free_bytes)
    }

    fn active_physical_adapters(&self) -> CommandResult<Vec<NetworkAdapterCandidate>> {
        (*self).active_physical_adapters()
    }
}

/// VM lifecycle and import provider.
pub trait VmProvider {
    /// Lists all VMs known to the provider.
    fn list_vms(&self) -> CommandResult<Vec<VmInventoryRecord>> {
        Err(failure("VM listing is not supported by this provider"))
    }
    /// Returns a VM inventory record by name, or `None` when absent.
    fn get_vm(&self, name: &str) -> CommandResult<Option<VmInventoryRecord>>;
    /// Checks whether a packaged VM import is compatible.
    fn compare_import(&self, request: &VmImportRequest) -> CommandResult<VmCompatibilityReport>;
    /// Imports a packaged VM.
    fn import_vm(&self, request: &VmImportRequest) -> CommandResult<ImportedVm>;
    /// Removes a VM registration.
    fn remove_vm(&self, name: &str) -> CommandResult<()>;
    /// Starts a VM.
    fn start_vm(&self, name: &str) -> CommandResult<()>;
    /// Stops a VM.
    fn stop_vm(&self, name: &str, turn_off: bool) -> CommandResult<()>;
    /// Connects the VM network adapter to a switch.
    fn connect_network_adapter(&self, vm_name: &str, switch_name: &str) -> CommandResult<()>;
    /// Ensures an external switch exists.
    fn ensure_external_switch(
        &self,
        request: &EnsureSwitchRequest,
    ) -> CommandResult<ExternalSwitch>;
    /// Resizes the first VHD attached to a VM.
    fn resize_first_vhd(&self, vm_name: &str, size_bytes: u64) -> CommandResult<()>;
    /// Sets the first disk as boot disk.
    fn set_first_boot_disk(&self, vm_name: &str) -> CommandResult<()>;
    /// Sets startup memory.
    fn set_startup_memory(&self, vm_name: &str, bytes: u64) -> CommandResult<()>;
    /// Sets virtual processor count.
    fn set_processor_count(&self, vm_name: &str, count: u32) -> CommandResult<()>;
}

impl<T> VmProvider for &T
where
    T: VmProvider + ?Sized,
{
    fn list_vms(&self) -> CommandResult<Vec<VmInventoryRecord>> {
        (*self).list_vms()
    }

    fn get_vm(&self, name: &str) -> CommandResult<Option<VmInventoryRecord>> {
        (*self).get_vm(name)
    }

    fn compare_import(&self, request: &VmImportRequest) -> CommandResult<VmCompatibilityReport> {
        (*self).compare_import(request)
    }

    fn import_vm(&self, request: &VmImportRequest) -> CommandResult<ImportedVm> {
        (*self).import_vm(request)
    }

    fn remove_vm(&self, name: &str) -> CommandResult<()> {
        (*self).remove_vm(name)
    }

    fn start_vm(&self, name: &str) -> CommandResult<()> {
        (*self).start_vm(name)
    }

    fn stop_vm(&self, name: &str, turn_off: bool) -> CommandResult<()> {
        (*self).stop_vm(name, turn_off)
    }

    fn connect_network_adapter(&self, vm_name: &str, switch_name: &str) -> CommandResult<()> {
        (*self).connect_network_adapter(vm_name, switch_name)
    }

    fn ensure_external_switch(
        &self,
        request: &EnsureSwitchRequest,
    ) -> CommandResult<ExternalSwitch> {
        (*self).ensure_external_switch(request)
    }

    fn resize_first_vhd(&self, vm_name: &str, size_bytes: u64) -> CommandResult<()> {
        (*self).resize_first_vhd(vm_name, size_bytes)
    }

    fn set_first_boot_disk(&self, vm_name: &str) -> CommandResult<()> {
        (*self).set_first_boot_disk(vm_name)
    }

    fn set_startup_memory(&self, vm_name: &str, bytes: u64) -> CommandResult<()> {
        (*self).set_startup_memory(vm_name, bytes)
    }

    fn set_processor_count(&self, vm_name: &str, count: u32) -> CommandResult<()> {
        (*self).set_processor_count(vm_name, count)
    }
}

/// Guest VM access provider.
pub trait GuestProvider {
    /// Waits for SSH to become reachable.
    fn wait_for_ssh(&self, ip: &str, timeout_seconds: u64) -> CommandResult<()>;
    /// Uploads bytes to a guest path with a file mode.
    fn upload_bytes(
        &self,
        ip: &str,
        remote_path: &str,
        bytes: &[u8],
        mode: u32,
    ) -> CommandResult<()>;
    /// Writes player-facing IP settings inside the guest.
    fn write_player_settings(&self, ip: &str, player_ip: &str) -> CommandResult<()>;
    /// Applies static guest networking.
    fn apply_static_network(&self, ip: &str, config: &GuestNetworkConfig) -> CommandResult<()>;
    /// Detects the guest's public egress IP, when possible.
    fn detect_public_ip(&self, ip: &str) -> CommandResult<Option<String>>;
}

impl<T> GuestProvider for &T
where
    T: GuestProvider + ?Sized,
{
    fn wait_for_ssh(&self, ip: &str, timeout_seconds: u64) -> CommandResult<()> {
        (*self).wait_for_ssh(ip, timeout_seconds)
    }

    fn upload_bytes(
        &self,
        ip: &str,
        remote_path: &str,
        bytes: &[u8],
        mode: u32,
    ) -> CommandResult<()> {
        (*self).upload_bytes(ip, remote_path, bytes, mode)
    }

    fn write_player_settings(&self, ip: &str, player_ip: &str) -> CommandResult<()> {
        (*self).write_player_settings(ip, player_ip)
    }

    fn apply_static_network(&self, ip: &str, config: &GuestNetworkConfig) -> CommandResult<()> {
        (*self).apply_static_network(ip, config)
    }

    fn detect_public_ip(&self, ip: &str) -> CommandResult<Option<String>> {
        (*self).detect_public_ip(ip)
    }
}

/// Kubernetes operations needed by battlegroup lifecycle orchestration.
pub trait KubernetesProvider {
    /// Lists battlegroup namespaces.
    fn list_battlegroup_namespaces(&self) -> CommandResult<Vec<String>>;
    /// Patches the battlegroup stop flag.
    fn patch_battlegroup_stop(&self, namespace: &str, name: &str, stop: bool) -> CommandResult<()>;
    /// Returns the Director NodePort for a namespace, when present.
    fn director_node_port(&self, namespace: &str) -> CommandResult<Option<u16>>;
}

/// Request to render and create a world manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldManifestRequest {
    /// User-facing world name.
    pub world_name: String,
    /// Vendor region label.
    pub world_region: String,
    /// Unique Kubernetes battlegroup/world name.
    pub world_unique_name: String,
    /// Self-host token used by the vendor manifest.
    pub self_host_token: String,
}

/// Result of creating world resources.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatedWorld {
    /// Created namespace.
    pub namespace: String,
    /// Created battlegroup resource name.
    pub battlegroup_name: String,
}

/// Provider for the guest bootstrap phases.
pub trait GuestBootstrapProvider {
    /// Validates and expands guest root disk if needed.
    fn validate_and_resize_root_disk(&self) -> CommandResult<()>;
    /// Ensures the server payload is downloaded inside the guest.
    fn ensure_server_payload(&self) -> CommandResult<()>;
    /// Starts k3s and waits until it is reachable.
    fn start_k3s_and_wait(&self) -> CommandResult<()>;
    /// Imports prerequisite k3s images.
    fn import_core_images(&self) -> CommandResult<()>;
    /// Starts core k3s deployments.
    fn scale_core_deployments(&self) -> CommandResult<()>;
    /// Updates operator CRDs and RBAC.
    fn update_operator_crds(&self) -> CommandResult<()>;
    /// Patches operator deployment images.
    fn patch_operator_images(&self) -> CommandResult<()>;
    /// Starts operator deployments.
    fn scale_operator_deployments(&self) -> CommandResult<()>;
    /// Installs the guest battlegroup helper script.
    fn install_battlegroup_helper(&self) -> CommandResult<()>;
    /// Creates the world namespace, secrets, and battlegroup resource.
    fn create_world(&self, request: &WorldManifestRequest) -> CommandResult<CreatedWorld>;
    /// Imports battlegroup container images.
    fn import_battlegroup_images(&self) -> CommandResult<()>;
    /// Patches battlegroup image tags to the downloaded version.
    fn patch_battlegroup_images(
        &self,
        namespace: &str,
        battlegroup_name: &str,
    ) -> CommandResult<()>;
    /// Applies default user settings files through the file browser pod.
    fn apply_default_user_settings(
        &self,
        namespace: &str,
        battlegroup_name: &str,
    ) -> CommandResult<()>;
}

impl<T> GuestBootstrapProvider for &T
where
    T: GuestBootstrapProvider + ?Sized,
{
    fn validate_and_resize_root_disk(&self) -> CommandResult<()> {
        (*self).validate_and_resize_root_disk()
    }

    fn ensure_server_payload(&self) -> CommandResult<()> {
        (*self).ensure_server_payload()
    }

    fn start_k3s_and_wait(&self) -> CommandResult<()> {
        (*self).start_k3s_and_wait()
    }

    fn import_core_images(&self) -> CommandResult<()> {
        (*self).import_core_images()
    }

    fn scale_core_deployments(&self) -> CommandResult<()> {
        (*self).scale_core_deployments()
    }

    fn update_operator_crds(&self) -> CommandResult<()> {
        (*self).update_operator_crds()
    }

    fn patch_operator_images(&self) -> CommandResult<()> {
        (*self).patch_operator_images()
    }

    fn scale_operator_deployments(&self) -> CommandResult<()> {
        (*self).scale_operator_deployments()
    }

    fn install_battlegroup_helper(&self) -> CommandResult<()> {
        (*self).install_battlegroup_helper()
    }

    fn create_world(&self, request: &WorldManifestRequest) -> CommandResult<CreatedWorld> {
        (*self).create_world(request)
    }

    fn import_battlegroup_images(&self) -> CommandResult<()> {
        (*self).import_battlegroup_images()
    }

    fn patch_battlegroup_images(
        &self,
        namespace: &str,
        battlegroup_name: &str,
    ) -> CommandResult<()> {
        (*self).patch_battlegroup_images(namespace, battlegroup_name)
    }

    fn apply_default_user_settings(
        &self,
        namespace: &str,
        battlegroup_name: &str,
    ) -> CommandResult<()> {
        (*self).apply_default_user_settings(namespace, battlegroup_name)
    }
}

/// Returns packaged `.vmcx` candidates under a server install path.
pub fn packaged_vmcx_candidates(install_path: &Path) -> CommandResult<Vec<String>> {
    let vm_dir = install_path.join("Virtual Machines");
    let entries = std::fs::read_dir(&vm_dir).map_err(|err| {
        crate::errors::failure(format!("Failed to read {}: {err}", vm_dir.display()))
    })?;
    let mut candidates = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("vmcx"))
        })
        .map(|path| path.to_string_lossy().to_string())
        .collect::<Vec<_>>();
    candidates.sort();
    Ok(candidates)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_hyperv_power_states() {
        assert_eq!(
            VmPowerState::from_hyperv_state("Running"),
            VmPowerState::Running
        );
        assert_eq!(
            VmPowerState::from_hyperv_state("Starting"),
            VmPowerState::Starting
        );
        assert_eq!(VmPowerState::from_hyperv_state("Off"), VmPowerState::Off);
        assert_eq!(
            VmPowerState::from_hyperv_state("SomethingElse"),
            VmPowerState::Other
        );
    }
}
