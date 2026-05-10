use serde::Serialize;
use std::{thread, time::Duration};

use crate::{
    errors::failure,
    models::CommandResult,
    orchestration::{
        GuestBootstrapProvider, KubernetesProvider, OperationSink, OrchestrationEvent,
        ProviderKind, StepAction, StepDomain,
    },
    validation::validate_kube_arg,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BattlegroupRef {
    pub namespace: String,
    pub name: String,
}

impl BattlegroupRef {
    pub fn validate(&self) -> CommandResult<()> {
        validate_kube_arg(&self.namespace, "namespace")?;
        validate_kube_arg(&self.name, "battlegroup name")?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceUrl {
    pub url: String,
}

pub struct BattlegroupManagementOrchestrator<K> {
    kubernetes: K,
}

impl<K> BattlegroupManagementOrchestrator<K>
where
    K: KubernetesProvider,
{
    pub fn new(kubernetes: K) -> Self {
        Self { kubernetes }
    }

    pub fn start(
        &self,
        battlegroup: &BattlegroupRef,
        sink: &mut impl OperationSink,
    ) -> CommandResult<()> {
        battlegroup.validate()?;
        emit(sink, "bg.start", "Starting battlegroup.", StepAction::Start);
        self.kubernetes
            .patch_battlegroup_stop(&battlegroup.namespace, &battlegroup.name, false)
    }

    pub fn start_and_wait_director(
        &self,
        battlegroup: &BattlegroupRef,
        timeout_seconds: u64,
        sink: &mut impl OperationSink,
    ) -> CommandResult<Option<u16>> {
        self.start(battlegroup, sink)?;
        self.wait_for_director_node_port(battlegroup, timeout_seconds, sink)
    }

    pub fn stop(
        &self,
        battlegroup: &BattlegroupRef,
        sink: &mut impl OperationSink,
    ) -> CommandResult<()> {
        battlegroup.validate()?;
        emit(sink, "bg.stop", "Stopping battlegroup.", StepAction::Stop);
        self.kubernetes
            .patch_battlegroup_stop(&battlegroup.namespace, &battlegroup.name, true)
    }

    pub fn restart(
        &self,
        battlegroup: &BattlegroupRef,
        sink: &mut impl OperationSink,
    ) -> CommandResult<()> {
        battlegroup.validate()?;
        emit(
            sink,
            "bg.restart.stop",
            "Stopping battlegroup for restart.",
            StepAction::Stop,
        );
        self.kubernetes
            .patch_battlegroup_stop(&battlegroup.namespace, &battlegroup.name, true)?;
        emit(
            sink,
            "bg.restart.start",
            "Starting battlegroup after restart.",
            StepAction::Start,
        );
        self.kubernetes
            .patch_battlegroup_stop(&battlegroup.namespace, &battlegroup.name, false)
    }

    pub fn restart_and_wait_director(
        &self,
        battlegroup: &BattlegroupRef,
        timeout_seconds: u64,
        sink: &mut impl OperationSink,
    ) -> CommandResult<Option<u16>> {
        self.restart(battlegroup, sink)?;
        self.wait_for_director_node_port(battlegroup, timeout_seconds, sink)
    }

    pub fn file_browser_url(&self, vm_ip: &str) -> CommandResult<ServiceUrl> {
        validate_ipv4ish(vm_ip, "VM IP")?;
        Ok(ServiceUrl {
            url: format!("http://{vm_ip}:18888/"),
        })
    }

    pub fn director_url(
        &self,
        battlegroup: &BattlegroupRef,
        vm_ip: &str,
    ) -> CommandResult<Option<ServiceUrl>> {
        battlegroup.validate()?;
        validate_ipv4ish(vm_ip, "VM IP")?;
        let Some(port) = self.kubernetes.director_node_port(&battlegroup.namespace)? else {
            return Ok(None);
        };
        Ok(Some(ServiceUrl {
            url: format!("http://{vm_ip}:{port}/"),
        }))
    }

    pub fn discover_single_battlegroup_namespace(&self) -> CommandResult<Option<String>> {
        let namespaces = self.kubernetes.list_battlegroup_namespaces()?;
        match namespaces.as_slice() {
            [] => Ok(None),
            [namespace] => Ok(Some(namespace.clone())),
            _ => Err(failure("Multiple battlegroup namespaces were found")),
        }
    }

    pub fn wait_for_director_node_port(
        &self,
        battlegroup: &BattlegroupRef,
        timeout_seconds: u64,
        sink: &mut impl OperationSink,
    ) -> CommandResult<Option<u16>> {
        battlegroup.validate()?;
        emit(
            sink,
            "bg.director.wait-port",
            "Waiting for Director service port.",
            StepAction::Wait,
        );
        let mut elapsed = 0;
        while elapsed <= timeout_seconds {
            if let Some(port) = self.kubernetes.director_node_port(&battlegroup.namespace)? {
                return Ok(Some(port));
            }
            thread::sleep(Duration::from_secs(2));
            elapsed += 2;
        }
        Ok(None)
    }
}

pub struct BattlegroupUpdateOrchestrator<B> {
    bootstrap: B,
}

impl<B> BattlegroupUpdateOrchestrator<B>
where
    B: GuestBootstrapProvider,
{
    pub fn new(bootstrap: B) -> Self {
        Self { bootstrap }
    }

    pub fn update_from_downloads(
        &self,
        battlegroup: &BattlegroupRef,
        sink: &mut impl OperationSink,
    ) -> CommandResult<()> {
        battlegroup.validate()?;
        emit(
            sink,
            "bg.update.import-images",
            "Importing downloaded battlegroup images.",
            StepAction::Import,
        );
        self.bootstrap.import_battlegroup_images()?;
        emit(
            sink,
            "bg.update.patch-images",
            "Patching battlegroup image revisions.",
            StepAction::Patch,
        );
        self.bootstrap
            .patch_battlegroup_images(&battlegroup.namespace, &battlegroup.name)
    }
}

fn emit(
    sink: &mut impl OperationSink,
    step_id: &'static str,
    message: impl Into<String>,
    action: StepAction,
) {
    sink.emit(OrchestrationEvent {
        step_id,
        message: message.into(),
        domain: StepDomain::Kubernetes,
        action,
        provider: ProviderKind::HyperV,
    });
}

fn validate_ipv4ish(value: &str, label: &str) -> CommandResult<()> {
    let parts = value.split('.').collect::<Vec<_>>();
    if parts.len() == 4 && parts.iter().all(|part| part.parse::<u8>().is_ok()) {
        Ok(())
    } else {
        Err(failure(format!("{label} must be an IPv4 address")))
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use crate::orchestration::VecOperationSink;

    use super::*;

    #[derive(Default)]
    struct MockKubernetes {
        calls: Rc<RefCell<Vec<String>>>,
        namespaces: Vec<String>,
        director_ports: Rc<RefCell<Vec<Option<u16>>>>,
    }

    impl KubernetesProvider for MockKubernetes {
        fn list_battlegroup_namespaces(&self) -> CommandResult<Vec<String>> {
            Ok(self.namespaces.clone())
        }

        fn patch_battlegroup_stop(
            &self,
            namespace: &str,
            name: &str,
            stop: bool,
        ) -> CommandResult<()> {
            self.calls
                .borrow_mut()
                .push(format!("{namespace}/{name}:{stop}"));
            Ok(())
        }

        fn director_node_port(&self, _namespace: &str) -> CommandResult<Option<u16>> {
            Ok(self.director_ports.borrow_mut().pop().unwrap_or(None))
        }
    }

    #[derive(Default)]
    struct MockBootstrap {
        calls: Rc<RefCell<Vec<&'static str>>>,
    }

    impl GuestBootstrapProvider for MockBootstrap {
        fn validate_and_resize_root_disk(&self) -> CommandResult<()> {
            Ok(())
        }

        fn ensure_server_payload(&self) -> CommandResult<()> {
            Ok(())
        }

        fn start_k3s_and_wait(&self) -> CommandResult<()> {
            Ok(())
        }

        fn import_core_images(&self) -> CommandResult<()> {
            Ok(())
        }

        fn scale_core_deployments(&self) -> CommandResult<()> {
            Ok(())
        }

        fn update_operator_crds(&self) -> CommandResult<()> {
            Ok(())
        }

        fn patch_operator_images(&self) -> CommandResult<()> {
            Ok(())
        }

        fn scale_operator_deployments(&self) -> CommandResult<()> {
            Ok(())
        }

        fn install_battlegroup_helper(&self) -> CommandResult<()> {
            Ok(())
        }

        fn create_world(
            &self,
            _request: &crate::orchestration::WorldManifestRequest,
        ) -> CommandResult<crate::orchestration::CreatedWorld> {
            unreachable!("update does not create worlds")
        }

        fn import_battlegroup_images(&self) -> CommandResult<()> {
            self.calls.borrow_mut().push("import_battlegroup_images");
            Ok(())
        }

        fn patch_battlegroup_images(
            &self,
            _namespace: &str,
            _battlegroup_name: &str,
        ) -> CommandResult<()> {
            self.calls.borrow_mut().push("patch_battlegroup_images");
            Ok(())
        }

        fn apply_default_user_settings(
            &self,
            _namespace: &str,
            _battlegroup_name: &str,
        ) -> CommandResult<()> {
            Ok(())
        }
    }

    #[test]
    fn restart_patches_stop_then_start() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let orchestrator = BattlegroupManagementOrchestrator::new(MockKubernetes {
            calls: calls.clone(),
            namespaces: vec![],
            director_ports: Rc::new(RefCell::new(vec![])),
        });
        let mut sink = VecOperationSink::default();
        orchestrator
            .restart(
                &BattlegroupRef {
                    namespace: "funcom-seabass-sh-host-abcdef".to_string(),
                    name: "sh-host-abcdef".to_string(),
                },
                &mut sink,
            )
            .unwrap();
        assert_eq!(
            calls.borrow().as_slice(),
            &[
                "funcom-seabass-sh-host-abcdef/sh-host-abcdef:true",
                "funcom-seabass-sh-host-abcdef/sh-host-abcdef:false",
            ]
        );
    }

    #[test]
    fn builds_service_urls_without_shelling_out() {
        let orchestrator = BattlegroupManagementOrchestrator::new(MockKubernetes {
            calls: Rc::new(RefCell::new(Vec::new())),
            namespaces: vec![],
            director_ports: Rc::new(RefCell::new(vec![Some(32527)])),
        });
        let bg = BattlegroupRef {
            namespace: "funcom-seabass-sh-host-abcdef".to_string(),
            name: "sh-host-abcdef".to_string(),
        };
        assert_eq!(
            orchestrator.file_browser_url("10.0.0.4").unwrap().url,
            "http://10.0.0.4:18888/"
        );
        assert_eq!(
            orchestrator
                .director_url(&bg, "10.0.0.4")
                .unwrap()
                .unwrap()
                .url,
            "http://10.0.0.4:32527/"
        );
    }

    #[test]
    fn start_waits_for_director_node_port_after_unstop_patch() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let orchestrator = BattlegroupManagementOrchestrator::new(MockKubernetes {
            calls: calls.clone(),
            namespaces: vec![],
            director_ports: Rc::new(RefCell::new(vec![Some(32527)])),
        });
        let mut sink = VecOperationSink::default();
        let port = orchestrator
            .start_and_wait_director(
                &BattlegroupRef {
                    namespace: "funcom-seabass-sh-host-abcdef".to_string(),
                    name: "sh-host-abcdef".to_string(),
                },
                0,
                &mut sink,
            )
            .unwrap();

        assert_eq!(port, Some(32527));
        assert_eq!(
            calls.borrow().as_slice(),
            &["funcom-seabass-sh-host-abcdef/sh-host-abcdef:false"]
        );
        assert!(sink
            .events
            .iter()
            .any(|event| event.step_id == "bg.director.wait-port"));
    }

    #[test]
    fn update_imports_and_patches_battlegroup_images() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let updater = BattlegroupUpdateOrchestrator::new(MockBootstrap {
            calls: calls.clone(),
        });
        let mut sink = VecOperationSink::default();
        updater
            .update_from_downloads(
                &BattlegroupRef {
                    namespace: "funcom-seabass-sh-host-abcdef".to_string(),
                    name: "sh-host-abcdef".to_string(),
                },
                &mut sink,
            )
            .unwrap();

        assert_eq!(
            calls.borrow().as_slice(),
            &["import_battlegroup_images", "patch_battlegroup_images"]
        );
        assert!(sink
            .events
            .iter()
            .any(|event| event.step_id == "bg.update.patch-images"));
    }
}
