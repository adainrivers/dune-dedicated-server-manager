use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::{
    errors::failure,
    models::CommandResult,
    orchestration::{
        packaged_vmcx_candidates, EnsureSwitchRequest, HostProvider, ProviderKind, StepAction,
        StepDomain, VmImportRequest, VmPowerState, VmProvider,
    },
};

pub const DEFAULT_VM_DISK_BYTES: u64 = 100 * 1024 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum MemoryProfile {
    Sietch20Gb,
    SietchStory30Gb,
    SietchStoryDeepDesert40Gb,
    CustomBytes(u64),
}

impl MemoryProfile {
    pub fn bytes(self) -> u64 {
        match self {
            Self::Sietch20Gb => 20 * 1024 * 1024 * 1024,
            Self::SietchStory30Gb => 30 * 1024 * 1024 * 1024,
            Self::SietchStoryDeepDesert40Gb => 40 * 1024 * 1024 * 1024,
            Self::CustomBytes(bytes) => bytes,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HyperVVmSetupRequest {
    pub install_path: PathBuf,
    pub vm_name: String,
    pub destination_path: PathBuf,
    pub switch_name: String,
    pub adapter_name: String,
    pub memory: MemoryProfile,
    pub replace_existing_vm: bool,
    pub clear_destination: bool,
    pub disk_size_bytes: u64,
}

impl HyperVVmSetupRequest {
    pub fn validate(&self) -> CommandResult<()> {
        if self.vm_name.trim().is_empty() {
            return Err(failure("VM name is required"));
        }
        if self.switch_name.trim().is_empty() {
            return Err(failure("Hyper-V switch name is required"));
        }
        if self.adapter_name.trim().is_empty() {
            return Err(failure("Host network adapter name is required"));
        }
        if self.memory.bytes() == 0 {
            return Err(failure("VM memory must be greater than zero"));
        }
        if self.disk_size_bytes == 0 {
            return Err(failure("VM disk size must be greater than zero"));
        }
        validate_existing_dir(&self.install_path, "server install path")?;
        validate_destination_parent(&self.destination_path)?;
        Ok(())
    }
}

impl Default for HyperVVmSetupRequest {
    fn default() -> Self {
        Self {
            install_path: PathBuf::new(),
            vm_name: String::new(),
            destination_path: PathBuf::new(),
            switch_name: "DuneAwakeningServerSwitch".to_string(),
            adapter_name: String::new(),
            memory: MemoryProfile::Sietch20Gb,
            replace_existing_vm: false,
            clear_destination: false,
            disk_size_bytes: DEFAULT_VM_DISK_BYTES,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrchestrationEvent {
    pub step_id: &'static str,
    pub message: String,
    pub domain: StepDomain,
    pub action: StepAction,
    pub provider: ProviderKind,
}

pub trait OperationSink {
    fn emit(&mut self, event: OrchestrationEvent);
}

#[derive(Default)]
pub struct VecOperationSink {
    pub events: Vec<OrchestrationEvent>,
}

impl OperationSink for VecOperationSink {
    fn emit(&mut self, event: OrchestrationEvent) {
        self.events.push(event);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HyperVVmSetupResult {
    pub vm_name: String,
    pub destination_path: String,
    pub switch_name: String,
    pub vmcx_path: String,
}

pub struct HyperVVmSetupOrchestrator<H, V> {
    host: H,
    vm: V,
}

impl<H, V> HyperVVmSetupOrchestrator<H, V>
where
    H: HostProvider,
    V: VmProvider,
{
    pub fn new(host: H, vm: V) -> Self {
        Self { host, vm }
    }

    pub fn import_and_prepare_vm(
        &self,
        request: &HyperVVmSetupRequest,
        sink: &mut impl OperationSink,
    ) -> CommandResult<HyperVVmSetupResult> {
        request.validate()?;

        emit_hyperv_event(
            sink,
            "host.readiness",
            "Checking host virtualization readiness.",
            StepDomain::Host,
            StepAction::Check,
        );
        let readiness = self.host.readiness()?;
        if !readiness.elevated {
            return Err(failure("Hyper-V setup requires elevated host privileges"));
        }
        if !readiness.hyperv_available {
            return Err(failure("Hyper-V is not available on this host"));
        }
        if !readiness.vmms_running {
            return Err(failure("Hyper-V vmms service is not running"));
        }

        emit_hyperv_event(
            sink,
            "package.locate-vmcx",
            "Locating packaged VM configuration.",
            StepDomain::Files,
            StepAction::Detect,
        );
        let vmcx_path = single_vmcx(&request.install_path)?;

        emit_hyperv_event(
            sink,
            "hyperv.detect-existing-vm",
            "Checking for an existing VM.",
            StepDomain::HyperV,
            StepAction::Detect,
        );
        if let Some(existing) = self.vm.get_vm(&request.vm_name)? {
            if !request.replace_existing_vm {
                return Err(failure(format!(
                    "VM '{}' already exists and replacement was not requested",
                    existing.name
                )));
            }
            if existing.state == VmPowerState::Running {
                emit_hyperv_event(
                    sink,
                    "hyperv.stop-existing-vm",
                    "Stopping existing VM before replacement.",
                    StepDomain::HyperV,
                    StepAction::Stop,
                );
                self.vm.stop_vm(&request.vm_name, true)?;
            }
            emit_hyperv_event(
                sink,
                "hyperv.remove-existing-vm",
                "Removing existing VM registration.",
                StepDomain::HyperV,
                StepAction::Stop,
            );
            self.vm.remove_vm(&request.vm_name)?;
        }

        if request.destination_path.exists() {
            if !request.clear_destination {
                return Err(failure(format!(
                    "VM destination already exists: {}",
                    request.destination_path.display()
                )));
            }
            emit_hyperv_event(
                sink,
                "host.clear-vm-destination",
                "Clearing VM destination folder.",
                StepDomain::Files,
                StepAction::Configure,
            );
            clear_destination_dir(&request.destination_path)?;
        }

        let import_request = VmImportRequest {
            vmcx_path: vmcx_path.clone(),
            destination_path: request.destination_path.to_string_lossy().to_string(),
        };

        emit_hyperv_event(
            sink,
            "hyperv.compare-vm",
            "Checking VM import compatibility.",
            StepDomain::HyperV,
            StepAction::Check,
        );
        let compatibility = self.vm.compare_import(&import_request)?;
        if !compatibility.compatible {
            return Err(failure(format!(
                "VM import compatibility failed: {}",
                compatibility.incompatibilities.join("; ")
            )));
        }

        emit_hyperv_event(
            sink,
            "hyperv.import-vm",
            "Importing VM.",
            StepDomain::HyperV,
            StepAction::Import,
        );
        let imported = self.vm.import_vm(&import_request)?;

        emit_hyperv_event(
            sink,
            "hyperv.ensure-switch",
            "Preparing Hyper-V external switch.",
            StepDomain::HyperV,
            StepAction::Create,
        );
        let switch = self.vm.ensure_external_switch(&EnsureSwitchRequest {
            switch_name: request.switch_name.clone(),
            adapter_name: request.adapter_name.clone(),
        })?;

        emit_hyperv_event(
            sink,
            "hyperv.connect-switch",
            "Connecting VM network adapter.",
            StepDomain::HyperV,
            StepAction::Configure,
        );
        self.vm
            .connect_network_adapter(&imported.name, &switch.name)?;

        emit_hyperv_event(
            sink,
            "hyperv.resize-vhd",
            "Sizing VM virtual disk.",
            StepDomain::HyperV,
            StepAction::Configure,
        );
        self.vm
            .resize_first_vhd(&imported.name, request.disk_size_bytes)?;

        emit_hyperv_event(
            sink,
            "hyperv.set-first-boot",
            "Configuring VM boot disk.",
            StepDomain::HyperV,
            StepAction::Configure,
        );
        self.vm.set_first_boot_disk(&imported.name)?;

        emit_hyperv_event(
            sink,
            "hyperv.set-memory",
            "Configuring VM memory.",
            StepDomain::HyperV,
            StepAction::Configure,
        );
        self.vm
            .set_startup_memory(&imported.name, request.memory.bytes())?;

        emit_hyperv_event(
            sink,
            "hyperv.start-vm",
            "Starting VM.",
            StepDomain::HyperV,
            StepAction::Start,
        );
        self.vm.start_vm(&imported.name)?;

        Ok(HyperVVmSetupResult {
            vm_name: imported.name,
            destination_path: request.destination_path.to_string_lossy().to_string(),
            switch_name: switch.name,
            vmcx_path,
        })
    }
}

pub(crate) fn emit_hyperv_event(
    sink: &mut impl OperationSink,
    step_id: &'static str,
    message: impl Into<String>,
    domain: StepDomain,
    action: StepAction,
) {
    sink.emit(OrchestrationEvent {
        step_id,
        message: message.into(),
        domain,
        action,
        provider: ProviderKind::HyperV,
    });
}

fn validate_existing_dir(path: &Path, label: &str) -> CommandResult<()> {
    if !path.exists() {
        return Err(failure(format!(
            "{label} does not exist: {}",
            path.display()
        )));
    }
    if !path.is_dir() {
        return Err(failure(format!(
            "{label} is not a directory: {}",
            path.display()
        )));
    }
    Ok(())
}

fn validate_destination_parent(path: &Path) -> CommandResult<()> {
    let parent = path
        .parent()
        .filter(|value| !value.as_os_str().is_empty())
        .ok_or_else(|| failure("VM destination must have a parent directory"))?;
    if !parent.exists() {
        return Err(failure(format!(
            "VM destination parent does not exist: {}",
            parent.display()
        )));
    }
    Ok(())
}

fn single_vmcx(install_path: &Path) -> CommandResult<String> {
    let candidates = packaged_vmcx_candidates(install_path)?;
    match candidates.as_slice() {
        [path] => Ok(path.clone()),
        [] => Err(failure(format!(
            "No .vmcx file found under {}",
            install_path.join("Virtual Machines").display()
        ))),
        _ => Err(failure(format!(
            "Multiple .vmcx files found under {}",
            install_path.join("Virtual Machines").display()
        ))),
    }
}

fn clear_destination_dir(path: &Path) -> CommandResult<()> {
    if !path.exists() {
        return Ok(());
    }
    if path.parent().is_none() {
        return Err(failure(
            "Refusing to clear destination without a parent directory",
        ));
    }
    std::fs::remove_dir_all(path)
        .map_err(|err| failure(format!("Failed to clear {}: {err}", path.display())))
}

#[cfg(test)]
mod tests {
    use std::{
        cell::RefCell,
        fs,
        rc::Rc,
        time::{SystemTime, UNIX_EPOCH},
    };

    use crate::orchestration::{
        DriveCandidate, ExternalSwitch, HostReadiness, NetworkAdapterCandidate,
        VmCompatibilityReport, VmInventoryRecord,
    };

    use super::*;

    #[derive(Default)]
    struct MockHost;

    impl HostProvider for MockHost {
        fn readiness(&self) -> CommandResult<HostReadiness> {
            Ok(HostReadiness {
                elevated: true,
                hyperv_available: true,
                vmms_running: true,
                virtualization_firmware_enabled: Some(true),
            })
        }

        fn drives_with_minimum_free_space(
            &self,
            _minimum_free_bytes: u64,
        ) -> CommandResult<Vec<DriveCandidate>> {
            Ok(vec![])
        }

        fn active_physical_adapters(&self) -> CommandResult<Vec<NetworkAdapterCandidate>> {
            Ok(vec![])
        }
    }

    #[derive(Default)]
    struct MockVm {
        calls: Rc<RefCell<Vec<&'static str>>>,
        existing: Option<VmInventoryRecord>,
    }

    impl VmProvider for MockVm {
        fn get_vm(&self, _name: &str) -> CommandResult<Option<VmInventoryRecord>> {
            self.calls.borrow_mut().push("get_vm");
            Ok(self.existing.clone())
        }

        fn compare_import(
            &self,
            _request: &VmImportRequest,
        ) -> CommandResult<VmCompatibilityReport> {
            self.calls.borrow_mut().push("compare_import");
            Ok(VmCompatibilityReport {
                compatible: true,
                incompatibilities: vec![],
            })
        }

        fn import_vm(
            &self,
            _request: &VmImportRequest,
        ) -> CommandResult<crate::orchestration::ImportedVm> {
            self.calls.borrow_mut().push("import_vm");
            Ok(crate::orchestration::ImportedVm {
                name: "test-vm".to_string(),
                configuration_location: "dest".to_string(),
            })
        }

        fn remove_vm(&self, _name: &str) -> CommandResult<()> {
            self.calls.borrow_mut().push("remove_vm");
            Ok(())
        }

        fn start_vm(&self, _name: &str) -> CommandResult<()> {
            self.calls.borrow_mut().push("start_vm");
            Ok(())
        }

        fn stop_vm(&self, _name: &str, _turn_off: bool) -> CommandResult<()> {
            self.calls.borrow_mut().push("stop_vm");
            Ok(())
        }

        fn connect_network_adapter(&self, _vm_name: &str, _switch_name: &str) -> CommandResult<()> {
            self.calls.borrow_mut().push("connect_network_adapter");
            Ok(())
        }

        fn ensure_external_switch(
            &self,
            _request: &EnsureSwitchRequest,
        ) -> CommandResult<ExternalSwitch> {
            self.calls.borrow_mut().push("ensure_external_switch");
            Ok(ExternalSwitch {
                name: "switch".to_string(),
                net_adapter_interface_description: "adapter".to_string(),
            })
        }

        fn resize_first_vhd(&self, _vm_name: &str, _size_bytes: u64) -> CommandResult<()> {
            self.calls.borrow_mut().push("resize_first_vhd");
            Ok(())
        }

        fn set_first_boot_disk(&self, _vm_name: &str) -> CommandResult<()> {
            self.calls.borrow_mut().push("set_first_boot_disk");
            Ok(())
        }

        fn set_startup_memory(&self, _vm_name: &str, _bytes: u64) -> CommandResult<()> {
            self.calls.borrow_mut().push("set_startup_memory");
            Ok(())
        }
    }

    #[test]
    fn orchestrates_hyperv_vm_import_sequence() {
        let temp = test_dir();
        let install = temp.join("server");
        let vm_dir = install.join("Virtual Machines");
        fs::create_dir_all(&vm_dir).unwrap();
        fs::write(vm_dir.join("server.vmcx"), "").unwrap();
        let destination = temp.join("vm");

        let calls = Rc::new(RefCell::new(Vec::new()));
        let vm = MockVm {
            calls: calls.clone(),
            existing: None,
        };
        let orchestrator = HyperVVmSetupOrchestrator::new(MockHost, vm);
        let mut sink = VecOperationSink::default();
        let result = orchestrator
            .import_and_prepare_vm(
                &HyperVVmSetupRequest {
                    install_path: install,
                    vm_name: "test-vm".to_string(),
                    destination_path: destination,
                    switch_name: "switch".to_string(),
                    adapter_name: "Ethernet".to_string(),
                    memory: MemoryProfile::Sietch20Gb,
                    replace_existing_vm: false,
                    clear_destination: false,
                    disk_size_bytes: DEFAULT_VM_DISK_BYTES,
                },
                &mut sink,
            )
            .unwrap();

        assert_eq!(result.vm_name, "test-vm");
        assert_eq!(
            calls.borrow().as_slice(),
            &[
                "get_vm",
                "compare_import",
                "import_vm",
                "ensure_external_switch",
                "connect_network_adapter",
                "resize_first_vhd",
                "set_first_boot_disk",
                "set_startup_memory",
                "start_vm",
            ]
        );
        assert!(sink
            .events
            .iter()
            .any(|event| event.step_id == "hyperv.import-vm"));
    }

    #[test]
    fn refuses_existing_vm_without_replace_flag() {
        let temp = test_dir();
        let install = temp.join("server");
        let vm_dir = install.join("Virtual Machines");
        fs::create_dir_all(&vm_dir).unwrap();
        fs::write(vm_dir.join("server.vmcx"), "").unwrap();

        let vm = MockVm {
            calls: Rc::new(RefCell::new(Vec::new())),
            existing: Some(VmInventoryRecord {
                name: "test-vm".to_string(),
                state: VmPowerState::Off,
                raw_state: "Off".to_string(),
                configuration_location: String::new(),
                path: String::new(),
                memory_assigned_bytes: 0,
                uptime_seconds: 0,
                ipv4_addresses: vec![],
            }),
        };
        let orchestrator = HyperVVmSetupOrchestrator::new(MockHost, vm);
        let mut sink = VecOperationSink::default();
        let err = orchestrator
            .import_and_prepare_vm(
                &HyperVVmSetupRequest {
                    install_path: install,
                    vm_name: "test-vm".to_string(),
                    destination_path: temp.join("vm"),
                    switch_name: "switch".to_string(),
                    adapter_name: "Ethernet".to_string(),
                    memory: MemoryProfile::Sietch20Gb,
                    replace_existing_vm: false,
                    clear_destination: false,
                    disk_size_bytes: DEFAULT_VM_DISK_BYTES,
                },
                &mut sink,
            )
            .unwrap_err();
        assert!(err.message.contains("already exists"));
    }

    fn test_dir() -> PathBuf {
        let mut path = std::env::temp_dir();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        path.push(format!("dune-manager-orchestration-test-{nanos}"));
        fs::create_dir_all(&path).unwrap();
        path
    }
}
