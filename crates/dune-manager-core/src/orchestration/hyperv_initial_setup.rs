use std::{thread, time::Duration};

use serde::Serialize;

use crate::{
    errors::failure,
    models::CommandResult,
    orchestration::{
        emit_hyperv_event, GuestBootstrapOrchestrator, GuestBootstrapPlan, GuestBootstrapProvider,
        GuestBootstrapResult, GuestNetworkConfig, GuestProvider, HyperVVmSetupOrchestrator,
        HyperVVmSetupRequest, HyperVVmSetupResult, OperationSink, StepAction, StepDomain,
        VmPowerState, VmProvider,
    },
};

/// Guest network mode applied during initial setup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuestNetworkPlan {
    /// Keep the VM on DHCP after first boot.
    Dhcp,
    /// Reconfigure the guest to use a static address.
    Static(GuestNetworkConfig),
}

/// Full request for creating the Hyper-V VM and bootstrapping the guest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HyperVInitialSetupRequest {
    /// Host-side Hyper-V import and VM preparation request.
    pub vm: HyperVVmSetupRequest,
    /// Network configuration to apply after the first DHCP boot.
    pub guest_network: GuestNetworkPlan,
    /// Guest bootstrap plan used after SSH becomes available.
    pub guest_bootstrap: GuestBootstrapPlan,
    /// Seconds to wait for Hyper-V to report a guest IPv4 address.
    pub vm_ip_timeout_seconds: u64,
    /// Seconds to wait for SSH reachability.
    pub ssh_timeout_seconds: u64,
}

impl HyperVInitialSetupRequest {
    /// Validates the complete initial setup request.
    pub fn validate(&self) -> CommandResult<()> {
        self.vm.validate()?;
        self.guest_bootstrap.validate()?;
        if self.vm_ip_timeout_seconds == 0 {
            return Err(failure("VM IP timeout must be greater than zero"));
        }
        if self.ssh_timeout_seconds == 0 {
            return Err(failure("SSH timeout must be greater than zero"));
        }
        Ok(())
    }
}

/// Result of a completed Hyper-V initial setup flow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HyperVInitialSetupResult {
    /// Host-side VM setup result.
    pub vm: HyperVVmSetupResult,
    /// Final guest IP used for bootstrap.
    pub guest_ip: String,
    /// Guest bootstrap result.
    pub bootstrap: GuestBootstrapResult,
}

/// Suggested addresses for player-facing server configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerAddressCandidates {
    /// LAN address currently assigned to the guest.
    pub guest_lan_ip: String,
    /// Public address detected from inside the guest, when reachable.
    pub public_ip: Option<String>,
}

/// Orchestrates full first-time setup across host, VM, guest, and bootstrap providers.
pub struct HyperVInitialSetupOrchestrator<H, V, G, B> {
    host: H,
    vm: V,
    guest: G,
    bootstrap: B,
}

impl<H, V, G, B> HyperVInitialSetupOrchestrator<H, V, G, B>
where
    H: crate::orchestration::HostProvider,
    V: VmProvider,
    G: GuestProvider,
    B: GuestBootstrapProvider,
{
    /// Creates an initial setup orchestrator from its provider boundaries.
    pub fn new(host: H, vm: V, guest: G, bootstrap: B) -> Self {
        Self {
            host,
            vm,
            guest,
            bootstrap,
        }
    }

    /// Imports and starts the VM, waits for SSH, applies network settings, and bootstraps k3s.
    pub fn run(
        &self,
        request: &HyperVInitialSetupRequest,
        sink: &mut impl OperationSink,
    ) -> CommandResult<HyperVInitialSetupResult> {
        request.validate()?;

        let vm_setup = HyperVVmSetupOrchestrator::new(&self.host, &self.vm);
        let vm = vm_setup.import_and_prepare_vm(&request.vm, sink)?;

        emit_hyperv_event(
            sink,
            "hyperv.wait-for-ip",
            "Waiting for VM IPv4 address.",
            StepDomain::HyperV,
            StepAction::Detect,
        );
        let first_ip = wait_for_vm_ipv4(&self.vm, &vm.vm_name, request.vm_ip_timeout_seconds)?;

        emit_hyperv_event(
            sink,
            "guest.wait-for-ssh",
            "Waiting for guest SSH.",
            StepDomain::Guest,
            StepAction::Check,
        );
        self.guest
            .wait_for_ssh(&first_ip, request.ssh_timeout_seconds)?;

        let guest_ip = match &request.guest_network {
            GuestNetworkPlan::Dhcp => first_ip,
            GuestNetworkPlan::Static(config) => {
                emit_hyperv_event(
                    sink,
                    "guest.apply-static-network",
                    "Applying static guest network.",
                    StepDomain::Guest,
                    StepAction::Configure,
                );
                self.guest.apply_static_network(&first_ip, config)?;
                let static_ip = config
                    .address_cidr
                    .split_once('/')
                    .map(|(ip, _)| ip.to_string())
                    .unwrap_or_else(|| config.address_cidr.clone());
                self.guest
                    .wait_for_ssh(&static_ip, request.ssh_timeout_seconds)?;
                static_ip
            }
        };

        emit_hyperv_event(
            sink,
            "guest.write-player-settings",
            "Writing player-facing server address.",
            StepDomain::Guest,
            StepAction::Configure,
        );
        self.guest
            .write_player_settings(&guest_ip, &request.guest_bootstrap.player_ip)?;

        let bootstrap =
            GuestBootstrapOrchestrator::new(&self.bootstrap).run(&request.guest_bootstrap, sink)?;

        Ok(HyperVInitialSetupResult {
            vm,
            guest_ip,
            bootstrap,
        })
    }
}

/// Detects LAN and optional public player-facing address candidates.
pub fn detect_player_address_candidates(
    guest: &impl GuestProvider,
    guest_ip: &str,
    sink: &mut impl OperationSink,
) -> CommandResult<PlayerAddressCandidates> {
    emit_hyperv_event(
        sink,
        "guest.detect-public-ip",
        "Detecting public player-facing IP.",
        StepDomain::Guest,
        StepAction::Detect,
    );
    Ok(PlayerAddressCandidates {
        guest_lan_ip: guest_ip.to_string(),
        public_ip: guest.detect_public_ip(guest_ip)?,
    })
}

/// Waits for a running Hyper-V VM to report a non-link-local IPv4 address.
pub fn wait_for_vm_ipv4(
    provider: &impl VmProvider,
    vm_name: &str,
    timeout_seconds: u64,
) -> CommandResult<String> {
    let mut elapsed = 0;
    while elapsed <= timeout_seconds {
        if let Some(vm) = provider.get_vm(vm_name)? {
            if vm.state == VmPowerState::Running {
                if let Some(ip) = vm
                    .ipv4_addresses
                    .iter()
                    .find(|ip| !ip.starts_with("169.254.") && !ip.trim().is_empty())
                {
                    return Ok(ip.clone());
                }
            }
        }
        thread::sleep(Duration::from_secs(2));
        elapsed += 2;
    }
    Err(failure(format!(
        "VM {vm_name} did not report an IPv4 address within {timeout_seconds} seconds"
    )))
}

#[cfg(test)]
mod tests {
    use std::{
        cell::RefCell,
        fs,
        path::PathBuf,
        rc::Rc,
        time::{SystemTime, UNIX_EPOCH},
    };

    use crate::orchestration::{
        CreatedWorld, DriveCandidate, ExternalSwitch, HostProvider, HostReadiness, ImportedVm,
        MemoryProfile, NetworkAdapterCandidate, VecOperationSink, VmCompatibilityReport,
        VmImportRequest, VmInventoryRecord, WorldManifestRequest, DEFAULT_VM_DISK_BYTES,
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
                total_physical_memory_bytes: 64 * 1024 * 1024 * 1024,
                available_physical_memory_bytes: 48 * 1024 * 1024 * 1024,
                logical_processor_count: 16,
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
        get_vm_calls: RefCell<usize>,
    }

    impl VmProvider for MockVm {
        fn get_vm(&self, _name: &str) -> CommandResult<Option<VmInventoryRecord>> {
            self.calls.borrow_mut().push("get_vm");
            let mut get_vm_calls = self.get_vm_calls.borrow_mut();
            *get_vm_calls += 1;
            if *get_vm_calls == 1 {
                return Ok(None);
            }
            Ok(Some(VmInventoryRecord {
                name: "test-vm".to_string(),
                state: VmPowerState::Running,
                raw_state: "Running".to_string(),
                configuration_location: String::new(),
                path: String::new(),
                memory_assigned_bytes: 0,
                processor_count: 0,
                uptime_seconds: 0,
                ipv4_addresses: vec!["10.0.0.4".to_string()],
                hard_disk_paths: vec![],
                disk_size_bytes: 0,
                disk_file_size_bytes: 0,
                switch_names: vec![],
            }))
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

        fn import_vm(&self, _request: &VmImportRequest) -> CommandResult<ImportedVm> {
            self.calls.borrow_mut().push("import_vm");
            Ok(ImportedVm {
                name: "test-vm".to_string(),
                configuration_location: String::new(),
            })
        }

        fn remove_vm(&self, _name: &str) -> CommandResult<()> {
            Ok(())
        }

        fn start_vm(&self, _name: &str) -> CommandResult<()> {
            self.calls.borrow_mut().push("start_vm");
            Ok(())
        }

        fn stop_vm(&self, _name: &str, _turn_off: bool) -> CommandResult<()> {
            Ok(())
        }

        fn connect_network_adapter(&self, _vm_name: &str, _switch_name: &str) -> CommandResult<()> {
            self.calls.borrow_mut().push("connect_network_adapter");
            Ok(())
        }

        fn ensure_external_switch(
            &self,
            _request: &crate::orchestration::EnsureSwitchRequest,
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

        fn set_processor_count(&self, _vm_name: &str, _count: u32) -> CommandResult<()> {
            self.calls.borrow_mut().push("set_processor_count");
            Ok(())
        }
    }

    #[derive(Default)]
    struct MockGuest {
        calls: Rc<RefCell<Vec<&'static str>>>,
    }

    impl GuestProvider for MockGuest {
        fn wait_for_ssh(&self, _ip: &str, _timeout_seconds: u64) -> CommandResult<()> {
            self.calls.borrow_mut().push("wait_for_ssh");
            Ok(())
        }

        fn upload_bytes(
            &self,
            _ip: &str,
            _remote_path: &str,
            _bytes: &[u8],
            _mode: u32,
        ) -> CommandResult<()> {
            Ok(())
        }

        fn write_player_settings(&self, _ip: &str, _player_ip: &str) -> CommandResult<()> {
            self.calls.borrow_mut().push("write_player_settings");
            Ok(())
        }

        fn apply_static_network(
            &self,
            _ip: &str,
            _config: &GuestNetworkConfig,
        ) -> CommandResult<()> {
            self.calls.borrow_mut().push("apply_static_network");
            Ok(())
        }

        fn detect_public_ip(&self, _ip: &str) -> CommandResult<Option<String>> {
            self.calls.borrow_mut().push("detect_public_ip");
            Ok(Some("203.0.113.10".to_string()))
        }
    }

    #[derive(Default)]
    struct MockBootstrap {
        calls: Rc<RefCell<Vec<&'static str>>>,
    }

    impl GuestBootstrapProvider for MockBootstrap {
        fn validate_and_resize_root_disk(&self) -> CommandResult<()> {
            self.calls.borrow_mut().push("disk");
            Ok(())
        }
        fn ensure_server_payload(&self) -> CommandResult<()> {
            self.calls.borrow_mut().push("payload");
            Ok(())
        }
        fn start_k3s_and_wait(&self) -> CommandResult<()> {
            self.calls.borrow_mut().push("k3s");
            Ok(())
        }
        fn import_core_images(&self) -> CommandResult<()> {
            self.calls.borrow_mut().push("core_images");
            Ok(())
        }
        fn scale_core_deployments(&self) -> CommandResult<()> {
            self.calls.borrow_mut().push("core_scale");
            Ok(())
        }
        fn update_operator_crds(&self) -> CommandResult<()> {
            self.calls.borrow_mut().push("operator_crds");
            Ok(())
        }
        fn patch_operator_images(&self) -> CommandResult<()> {
            self.calls.borrow_mut().push("operator_images");
            Ok(())
        }
        fn scale_operator_deployments(&self) -> CommandResult<()> {
            self.calls.borrow_mut().push("operator_scale");
            Ok(())
        }
        fn install_battlegroup_helper(&self) -> CommandResult<()> {
            self.calls.borrow_mut().push("helper");
            Ok(())
        }
        fn create_world(&self, request: &WorldManifestRequest) -> CommandResult<CreatedWorld> {
            self.calls.borrow_mut().push("world");
            Ok(CreatedWorld {
                namespace: format!("funcom-seabass-{}", request.world_unique_name),
                battlegroup_name: request.world_unique_name.clone(),
            })
        }
        fn import_battlegroup_images(&self) -> CommandResult<()> {
            self.calls.borrow_mut().push("bg_images");
            Ok(())
        }
        fn patch_battlegroup_images(
            &self,
            _namespace: &str,
            _battlegroup_name: &str,
        ) -> CommandResult<()> {
            self.calls.borrow_mut().push("bg_patch");
            Ok(())
        }
        fn apply_default_user_settings(
            &self,
            _namespace: &str,
            _battlegroup_name: &str,
        ) -> CommandResult<()> {
            self.calls.borrow_mut().push("defaults");
            Ok(())
        }
    }

    #[test]
    fn orchestrates_hyperv_initial_setup_across_provider_boundaries() {
        let temp = test_dir();
        let install = temp.join("server");
        let vm_dir = install.join("Virtual Machines");
        fs::create_dir_all(&vm_dir).unwrap();
        fs::write(vm_dir.join("server.vmcx"), "").unwrap();

        let guest_calls = Rc::new(RefCell::new(Vec::new()));
        let bootstrap_calls = Rc::new(RefCell::new(Vec::new()));
        let orchestrator = HyperVInitialSetupOrchestrator::new(
            MockHost,
            MockVm::default(),
            MockGuest {
                calls: guest_calls.clone(),
            },
            MockBootstrap {
                calls: bootstrap_calls.clone(),
            },
        );
        let mut sink = VecOperationSink::default();
        let result = orchestrator
            .run(
                &HyperVInitialSetupRequest {
                    vm: HyperVVmSetupRequest {
                        install_path: install,
                        vm_name: "test-vm".to_string(),
                        destination_path: temp.join("vm"),
                        switch_name: "switch".to_string(),
                        adapter_name: "Ethernet".to_string(),
                        memory: MemoryProfile::Sietch20Gb,
                        processor_count: 4,
                        replace_existing_vm: false,
                        clear_destination: false,
                        disk_size_bytes: DEFAULT_VM_DISK_BYTES,
                    },
                    guest_network: GuestNetworkPlan::Dhcp,
                    guest_bootstrap: GuestBootstrapPlan {
                        player_ip: "10.0.0.4".to_string(),
                        world_name: "Adain".to_string(),
                        world_region: "Europe Test".to_string(),
                        self_host_token: "token".to_string(),
                        host_id: "host123".to_string(),
                        world_suffix: "abcdef".to_string(),
                    },
                    vm_ip_timeout_seconds: 2,
                    ssh_timeout_seconds: 2,
                },
                &mut sink,
            )
            .unwrap();

        assert_eq!(result.guest_ip, "10.0.0.4");
        assert_eq!(
            guest_calls.borrow().as_slice(),
            &["wait_for_ssh", "write_player_settings"]
        );
        assert_eq!(
            bootstrap_calls.borrow().as_slice(),
            &[
                "disk",
                "payload",
                "k3s",
                "core_images",
                "core_scale",
                "operator_crds",
                "operator_images",
                "operator_scale",
                "helper",
                "world",
                "bg_images",
                "bg_patch",
                "defaults",
            ]
        );
        assert!(sink
            .events
            .iter()
            .any(|event| event.step_id == "guest.write-player-settings"));
    }

    #[test]
    fn detects_player_address_candidates_as_a_distinct_setup_step() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let guest = MockGuest {
            calls: calls.clone(),
        };
        let mut sink = VecOperationSink::default();
        let candidates = detect_player_address_candidates(&guest, "10.0.0.4", &mut sink).unwrap();

        assert_eq!(candidates.guest_lan_ip, "10.0.0.4");
        assert_eq!(candidates.public_ip, Some("203.0.113.10".to_string()));
        assert_eq!(calls.borrow().as_slice(), &["detect_public_ip"]);
        assert!(sink
            .events
            .iter()
            .any(|event| event.step_id == "guest.detect-public-ip"));
    }

    fn test_dir() -> PathBuf {
        let mut path = std::env::temp_dir();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        path.push(format!("dune-manager-initial-setup-test-{nanos}"));
        fs::create_dir_all(&path).unwrap();
        path
    }
}
