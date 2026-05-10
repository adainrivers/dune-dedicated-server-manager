use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::models::CommandResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VmPowerState {
    Missing,
    Off,
    Starting,
    Running,
    Stopping,
    Saved,
    Paused,
    Other,
}

impl VmPowerState {
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostReadiness {
    pub elevated: bool,
    pub hyperv_available: bool,
    pub vmms_running: bool,
    pub virtualization_firmware_enabled: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveCandidate {
    pub name: String,
    pub root: String,
    pub free_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkAdapterCandidate {
    pub name: String,
    pub interface_description: String,
    pub ipv4_address: String,
    pub prefix_length: u8,
    pub gateway: String,
    pub existing_external_switch: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalSwitch {
    pub name: String,
    pub net_adapter_interface_description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VmInventoryRecord {
    pub name: String,
    pub state: VmPowerState,
    pub raw_state: String,
    pub configuration_location: String,
    pub path: String,
    pub memory_assigned_bytes: u64,
    pub uptime_seconds: u64,
    pub ipv4_addresses: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VmCompatibilityReport {
    pub compatible: bool,
    pub incompatibilities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportedVm {
    pub name: String,
    pub configuration_location: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmImportRequest {
    pub vmcx_path: String,
    pub destination_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnsureSwitchRequest {
    pub switch_name: String,
    pub adapter_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuestNetworkConfig {
    pub interface: String,
    pub address_cidr: String,
    pub gateway: String,
    pub dns: String,
}

pub trait HostProvider {
    fn readiness(&self) -> CommandResult<HostReadiness>;
    fn drives_with_minimum_free_space(
        &self,
        minimum_free_bytes: u64,
    ) -> CommandResult<Vec<DriveCandidate>>;
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

pub trait VmProvider {
    fn get_vm(&self, name: &str) -> CommandResult<Option<VmInventoryRecord>>;
    fn compare_import(&self, request: &VmImportRequest) -> CommandResult<VmCompatibilityReport>;
    fn import_vm(&self, request: &VmImportRequest) -> CommandResult<ImportedVm>;
    fn remove_vm(&self, name: &str) -> CommandResult<()>;
    fn start_vm(&self, name: &str) -> CommandResult<()>;
    fn stop_vm(&self, name: &str, turn_off: bool) -> CommandResult<()>;
    fn connect_network_adapter(&self, vm_name: &str, switch_name: &str) -> CommandResult<()>;
    fn ensure_external_switch(
        &self,
        request: &EnsureSwitchRequest,
    ) -> CommandResult<ExternalSwitch>;
    fn resize_first_vhd(&self, vm_name: &str, size_bytes: u64) -> CommandResult<()>;
    fn set_first_boot_disk(&self, vm_name: &str) -> CommandResult<()>;
    fn set_startup_memory(&self, vm_name: &str, bytes: u64) -> CommandResult<()>;
}

impl<T> VmProvider for &T
where
    T: VmProvider + ?Sized,
{
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
}

pub trait GuestProvider {
    fn wait_for_ssh(&self, ip: &str, timeout_seconds: u64) -> CommandResult<()>;
    fn upload_bytes(
        &self,
        ip: &str,
        remote_path: &str,
        bytes: &[u8],
        mode: u32,
    ) -> CommandResult<()>;
    fn write_player_settings(&self, ip: &str, player_ip: &str) -> CommandResult<()>;
    fn apply_static_network(&self, ip: &str, config: &GuestNetworkConfig) -> CommandResult<()>;
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

pub trait KubernetesProvider {
    fn list_battlegroup_namespaces(&self) -> CommandResult<Vec<String>>;
    fn patch_battlegroup_stop(&self, namespace: &str, name: &str, stop: bool) -> CommandResult<()>;
    fn director_node_port(&self, namespace: &str) -> CommandResult<Option<u16>>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldManifestRequest {
    pub world_name: String,
    pub world_region: String,
    pub world_unique_name: String,
    pub self_host_token: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatedWorld {
    pub namespace: String,
    pub battlegroup_name: String,
}

pub trait GuestBootstrapProvider {
    fn validate_and_resize_root_disk(&self) -> CommandResult<()>;
    fn ensure_server_payload(&self) -> CommandResult<()>;
    fn start_k3s_and_wait(&self) -> CommandResult<()>;
    fn import_core_images(&self) -> CommandResult<()>;
    fn scale_core_deployments(&self) -> CommandResult<()>;
    fn update_operator_crds(&self) -> CommandResult<()>;
    fn patch_operator_images(&self) -> CommandResult<()>;
    fn scale_operator_deployments(&self) -> CommandResult<()>;
    fn install_battlegroup_helper(&self) -> CommandResult<()>;
    fn create_world(&self, request: &WorldManifestRequest) -> CommandResult<CreatedWorld>;
    fn import_battlegroup_images(&self) -> CommandResult<()>;
    fn patch_battlegroup_images(
        &self,
        namespace: &str,
        battlegroup_name: &str,
    ) -> CommandResult<()>;
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
