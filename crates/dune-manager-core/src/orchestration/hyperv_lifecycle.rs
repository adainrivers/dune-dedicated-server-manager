use crate::{
    models::CommandResult,
    orchestration::{emit_hyperv_event, OperationSink, StepAction, StepDomain, VmProvider},
};

/// Starts and stops an existing Hyper-V VM.
pub struct HyperVVmLifecycleOrchestrator<V> {
    vm: V,
}

impl<V> HyperVVmLifecycleOrchestrator<V>
where
    V: VmProvider,
{
    /// Creates a lifecycle orchestrator around a VM provider.
    pub fn new(vm: V) -> Self {
        Self { vm }
    }

    /// Starts the named VM.
    pub fn start(&self, vm_name: &str, sink: &mut impl OperationSink) -> CommandResult<()> {
        emit_hyperv_event(
            sink,
            "hyperv.lifecycle.start-vm",
            "Starting VM.",
            StepDomain::HyperV,
            StepAction::Start,
        );
        self.vm.start_vm(vm_name)
    }

    /// Turns off the named VM.
    pub fn stop(&self, vm_name: &str, sink: &mut impl OperationSink) -> CommandResult<()> {
        emit_hyperv_event(
            sink,
            "hyperv.lifecycle.stop-vm",
            "Stopping VM.",
            StepDomain::HyperV,
            StepAction::Stop,
        );
        self.vm.stop_vm(vm_name, true)
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use crate::orchestration::{
        EnsureSwitchRequest, ExternalSwitch, ImportedVm, VecOperationSink, VmCompatibilityReport,
        VmImportRequest, VmInventoryRecord,
    };

    use super::*;

    #[derive(Default)]
    struct MockVm {
        calls: Rc<RefCell<Vec<String>>>,
    }

    impl VmProvider for MockVm {
        fn get_vm(&self, _name: &str) -> CommandResult<Option<VmInventoryRecord>> {
            Ok(None)
        }

        fn compare_import(
            &self,
            _request: &VmImportRequest,
        ) -> CommandResult<VmCompatibilityReport> {
            unreachable!("lifecycle does not import")
        }

        fn import_vm(&self, _request: &VmImportRequest) -> CommandResult<ImportedVm> {
            unreachable!("lifecycle does not import")
        }

        fn remove_vm(&self, _name: &str) -> CommandResult<()> {
            Ok(())
        }

        fn start_vm(&self, name: &str) -> CommandResult<()> {
            self.calls.borrow_mut().push(format!("start:{name}"));
            Ok(())
        }

        fn stop_vm(&self, name: &str, turn_off: bool) -> CommandResult<()> {
            self.calls
                .borrow_mut()
                .push(format!("stop:{name}:{turn_off}"));
            Ok(())
        }

        fn connect_network_adapter(&self, _vm_name: &str, _switch_name: &str) -> CommandResult<()> {
            Ok(())
        }

        fn ensure_external_switch(
            &self,
            _request: &EnsureSwitchRequest,
        ) -> CommandResult<ExternalSwitch> {
            unreachable!("lifecycle does not create switches")
        }

        fn resize_first_vhd(&self, _vm_name: &str, _size_bytes: u64) -> CommandResult<()> {
            Ok(())
        }

        fn set_first_boot_disk(&self, _vm_name: &str) -> CommandResult<()> {
            Ok(())
        }

        fn set_startup_memory(&self, _vm_name: &str, _bytes: u64) -> CommandResult<()> {
            Ok(())
        }

        fn set_processor_count(&self, _vm_name: &str, _count: u32) -> CommandResult<()> {
            Ok(())
        }
    }

    #[test]
    fn lifecycle_orchestrator_starts_and_stops_vm() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let orchestrator = HyperVVmLifecycleOrchestrator::new(MockVm {
            calls: calls.clone(),
        });
        let mut sink = VecOperationSink::default();

        orchestrator.start("test-vm", &mut sink).unwrap();
        orchestrator.stop("test-vm", &mut sink).unwrap();

        assert_eq!(
            calls.borrow().as_slice(),
            &["start:test-vm", "stop:test-vm:true"]
        );
        assert!(sink
            .events
            .iter()
            .any(|event| event.step_id == "hyperv.lifecycle.stop-vm"));
    }
}
