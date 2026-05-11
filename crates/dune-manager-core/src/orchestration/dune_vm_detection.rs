//! Host-side detection of Dune Awakening Hyper-V VMs.

use serde::{Deserialize, Serialize};

use crate::{
    models::CommandResult,
    orchestration::{VmInventoryRecord, VmProvider},
};

/// Canonical virtual disk file name used by the vendor Dune server VM.
pub const DUNE_SERVER_VHDX_NAME: &str = "dune-server.vhdx";

/// Confidence level for host-only Dune VM detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DuneVmConfidence {
    /// Strong host-side fingerprint, usually the canonical Dune VHD name.
    High,
    /// Several soft host-side hints matched, but no canonical disk fingerprint.
    Medium,
    /// A single weak host-side hint matched.
    Low,
}

/// A Hyper-V VM that appears to be a Dune Awakening dedicated server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DuneVmCandidate {
    /// Host VM inventory record.
    pub vm: VmInventoryRecord,
    /// Host-only confidence level.
    pub confidence: DuneVmConfidence,
    /// Human-readable detection reasons.
    pub reasons: Vec<String>,
}

/// Detects Dune Awakening VMs from host-side Hyper-V inventory.
pub struct DuneVmDetector<V> {
    vm: V,
}

impl<V> DuneVmDetector<V>
where
    V: VmProvider,
{
    /// Creates a detector from a VM provider.
    pub fn new(vm: V) -> Self {
        Self { vm }
    }

    /// Lists VMs that match Dune host-side fingerprints.
    pub fn detect(&self) -> CommandResult<Vec<DuneVmCandidate>> {
        Ok(self
            .vm
            .list_vms()?
            .into_iter()
            .filter_map(classify_dune_vm)
            .collect())
    }
}

/// Classifies a single VM inventory record using host-side Dune fingerprints.
pub fn classify_dune_vm(vm: VmInventoryRecord) -> Option<DuneVmCandidate> {
    let mut reasons = Vec::new();
    let mut strong = false;

    if vm
        .hard_disk_paths
        .iter()
        .any(|path| path_file_name_eq(path, DUNE_SERVER_VHDX_NAME))
    {
        strong = true;
        reasons.push(format!("attached virtual disk is {DUNE_SERVER_VHDX_NAME}"));
    }

    let name = vm.name.to_ascii_lowercase();
    if name.contains("dune") {
        reasons.push("VM name contains dune".to_string());
    }
    if name.contains("awakening") {
        reasons.push("VM name contains awakening".to_string());
    }

    let path_text = format!("{} {}", vm.path, vm.configuration_location).to_ascii_lowercase();
    if path_text.contains("dune") {
        reasons.push("VM path contains dune".to_string());
    }
    if path_text.contains("awakening") {
        reasons.push("VM path contains awakening".to_string());
    }

    if vm
        .switch_names
        .iter()
        .any(|switch| switch.to_ascii_lowercase().contains("dune"))
    {
        reasons.push("connected switch name contains dune".to_string());
    }

    if reasons.is_empty() {
        return None;
    }

    let confidence = if strong {
        DuneVmConfidence::High
    } else if reasons.len() >= 2 {
        DuneVmConfidence::Medium
    } else {
        DuneVmConfidence::Low
    };

    Some(DuneVmCandidate {
        vm,
        confidence,
        reasons,
    })
}

fn path_file_name_eq(path: &str, expected: &str) -> bool {
    path.replace('/', "\\")
        .rsplit('\\')
        .next()
        .is_some_and(|name| name.eq_ignore_ascii_case(expected))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::{VmPowerState, VmProvider};

    #[test]
    fn canonical_dune_vhd_is_high_confidence() {
        let candidate = classify_dune_vm(sample_vm(
            "renamed",
            vec!["D:\\VMs\\Virtual Hard Disks\\dune-server.vhdx"],
            vec![],
        ))
        .unwrap();

        assert_eq!(candidate.confidence, DuneVmConfidence::High);
        assert!(candidate
            .reasons
            .iter()
            .any(|reason| reason.contains(DUNE_SERVER_VHDX_NAME)));
    }

    #[test]
    fn soft_host_hints_are_medium_confidence() {
        let candidate = classify_dune_vm(sample_vm(
            "dune-test",
            vec!["D:\\VMs\\disk.vhdx"],
            vec!["DuneAwakeningServerSwitch"],
        ))
        .unwrap();

        assert_eq!(candidate.confidence, DuneVmConfidence::Medium);
    }

    #[test]
    fn unrelated_vm_is_not_a_candidate() {
        assert!(
            classify_dune_vm(sample_vm("linux-test", vec!["D:\\VMs\\disk.vhdx"], vec![])).is_none()
        );
    }

    #[test]
    fn detector_filters_inventory() {
        struct MockVmProvider;

        impl VmProvider for MockVmProvider {
            fn list_vms(&self) -> CommandResult<Vec<VmInventoryRecord>> {
                Ok(vec![
                    sample_vm("linux-test", vec!["D:\\VMs\\disk.vhdx"], vec![]),
                    sample_vm(
                        "server",
                        vec!["D:\\VMs\\Virtual Hard Disks\\dune-server.vhdx"],
                        vec![],
                    ),
                ])
            }

            fn get_vm(&self, _name: &str) -> CommandResult<Option<VmInventoryRecord>> {
                unreachable!()
            }

            fn compare_import(
                &self,
                _request: &crate::orchestration::VmImportRequest,
            ) -> CommandResult<crate::orchestration::VmCompatibilityReport> {
                unreachable!()
            }

            fn import_vm(
                &self,
                _request: &crate::orchestration::VmImportRequest,
            ) -> CommandResult<crate::orchestration::ImportedVm> {
                unreachable!()
            }

            fn remove_vm(&self, _name: &str) -> CommandResult<()> {
                unreachable!()
            }

            fn start_vm(&self, _name: &str) -> CommandResult<()> {
                unreachable!()
            }

            fn stop_vm(&self, _name: &str, _turn_off: bool) -> CommandResult<()> {
                unreachable!()
            }

            fn connect_network_adapter(
                &self,
                _vm_name: &str,
                _switch_name: &str,
            ) -> CommandResult<()> {
                unreachable!()
            }

            fn ensure_external_switch(
                &self,
                _request: &crate::orchestration::EnsureSwitchRequest,
            ) -> CommandResult<crate::orchestration::ExternalSwitch> {
                unreachable!()
            }

            fn resize_first_vhd(&self, _vm_name: &str, _size_bytes: u64) -> CommandResult<()> {
                unreachable!()
            }

            fn set_first_boot_disk(&self, _vm_name: &str) -> CommandResult<()> {
                unreachable!()
            }

            fn set_startup_memory(&self, _vm_name: &str, _bytes: u64) -> CommandResult<()> {
                unreachable!()
            }

            fn set_processor_count(&self, _vm_name: &str, _count: u32) -> CommandResult<()> {
                unreachable!()
            }
        }

        let candidates = DuneVmDetector::new(MockVmProvider).detect().unwrap();

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].confidence, DuneVmConfidence::High);
    }

    fn sample_vm(
        name: &str,
        hard_disk_paths: Vec<&str>,
        switch_names: Vec<&str>,
    ) -> VmInventoryRecord {
        VmInventoryRecord {
            name: name.to_string(),
            state: VmPowerState::Off,
            raw_state: "Off".to_string(),
            configuration_location: "D:\\VMs".to_string(),
            path: "D:\\VMs".to_string(),
            memory_assigned_bytes: 0,
            processor_count: 0,
            uptime_seconds: 0,
            ipv4_addresses: vec![],
            hard_disk_paths: hard_disk_paths.into_iter().map(str::to_string).collect(),
            disk_size_bytes: 0,
            disk_file_size_bytes: 0,
            switch_names: switch_names.into_iter().map(str::to_string).collect(),
        }
    }
}
