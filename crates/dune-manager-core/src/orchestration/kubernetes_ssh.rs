use serde_json::{json, Value};

use crate::{
    errors::{failure, parse_json},
    models::CommandResult,
    orchestration::KubernetesProvider,
    validation::validate_kube_arg,
};

pub trait RemoteCommandRunner {
    fn run(&self, command: &str) -> CommandResult<String>;
    fn run_script(&self, script: &str) -> CommandResult<String>;

    fn run_json(&self, command: &str, label: &str) -> CommandResult<Value> {
        parse_json(&self.run(command)?, label)
    }
}

#[derive(Debug, Clone)]
pub struct StructuredKubectl<R> {
    runner: R,
}

impl<R> StructuredKubectl<R>
where
    R: RemoteCommandRunner,
{
    pub fn new(runner: R) -> Self {
        Self { runner }
    }
}

impl<R> KubernetesProvider for StructuredKubectl<R>
where
    R: RemoteCommandRunner,
{
    fn list_battlegroup_namespaces(&self) -> CommandResult<Vec<String>> {
        let value = self
            .runner
            .run_json("sudo kubectl get ns -o json", "namespace list")?;
        let mut namespaces = value["items"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|item| item["metadata"]["name"].as_str().map(str::to_string))
            .filter(|name| name.starts_with("funcom-seabass-"))
            .collect::<Vec<_>>();
        namespaces.sort();
        Ok(namespaces)
    }

    fn patch_battlegroup_stop(&self, namespace: &str, name: &str, stop: bool) -> CommandResult<()> {
        validate_kube_arg(namespace, "namespace")?;
        validate_kube_arg(name, "battlegroup name")?;
        let patch = json!({ "spec": { "stop": stop } }).to_string();
        let command = format!(
            "sudo kubectl patch battlegroup {} -n {} --type=merge -p {} -o json",
            sh_single_quoted(name),
            sh_single_quoted(namespace),
            sh_single_quoted(&patch)
        );
        let value = self.runner.run_json(&command, "battlegroup patch")?;
        let patched_name = value["metadata"]["name"].as_str().unwrap_or_default();
        if patched_name != name {
            return Err(failure(
                "Battlegroup patch did not return the expected resource",
            ));
        }
        Ok(())
    }

    fn director_node_port(&self, namespace: &str) -> CommandResult<Option<u16>> {
        validate_kube_arg(namespace, "namespace")?;
        let command = format!(
            "sudo kubectl get svc -n {} -o json",
            sh_single_quoted(namespace)
        );
        let value = self.runner.run_json(&command, "service list")?;
        for service in value["items"].as_array().cloned().unwrap_or_default() {
            for port in service["spec"]["ports"]
                .as_array()
                .cloned()
                .unwrap_or_default()
            {
                if port["port"].as_u64() == Some(11717) {
                    return Ok(port["nodePort"]
                        .as_u64()
                        .and_then(|value| u16::try_from(value).ok()));
                }
            }
        }
        Ok(None)
    }
}

fn sh_single_quoted(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, collections::VecDeque, rc::Rc};

    use super::*;

    #[derive(Clone, Default)]
    struct MockRemote {
        outputs: Rc<RefCell<VecDeque<String>>>,
        commands: Rc<RefCell<Vec<String>>>,
    }

    impl MockRemote {
        fn with_outputs(outputs: impl IntoIterator<Item = impl Into<String>>) -> Self {
            Self {
                outputs: Rc::new(RefCell::new(outputs.into_iter().map(Into::into).collect())),
                commands: Rc::new(RefCell::new(Vec::new())),
            }
        }
    }

    impl RemoteCommandRunner for MockRemote {
        fn run(&self, command: &str) -> CommandResult<String> {
            self.commands.borrow_mut().push(command.to_string());
            self.outputs
                .borrow_mut()
                .pop_front()
                .ok_or_else(|| failure("no mock output queued"))
        }

        fn run_script(&self, script: &str) -> CommandResult<String> {
            self.run(script)
        }
    }

    #[test]
    fn lists_battlegroup_namespaces_from_json_only() {
        let remote = MockRemote::with_outputs([r#"{
          "items": [
            {"metadata":{"name":"default"}},
            {"metadata":{"name":"funcom-seabass-sh-host-bbbbbb"}},
            {"metadata":{"name":"funcom-seabass-sh-host-aaaaaa"}}
          ]
        }"#]);
        let provider = StructuredKubectl::new(remote);
        assert_eq!(
            provider.list_battlegroup_namespaces().unwrap(),
            vec![
                "funcom-seabass-sh-host-aaaaaa".to_string(),
                "funcom-seabass-sh-host-bbbbbb".to_string(),
            ]
        );
    }

    #[test]
    fn discovers_director_node_port_from_services_json() {
        let remote = MockRemote::with_outputs([r#"{
          "items": [
            {"spec":{"ports":[{"port":18888,"nodePort":30000}]}},
            {"spec":{"ports":[{"port":11717,"nodePort":32527}]}}
          ]
        }"#]);
        let provider = StructuredKubectl::new(remote);
        assert_eq!(
            provider
                .director_node_port("funcom-seabass-sh-host-abcdef")
                .unwrap(),
            Some(32527)
        );
    }

    #[test]
    fn patch_uses_json_merge_patch_and_validates_returned_resource() {
        let remote = MockRemote::with_outputs([r#"{"metadata":{"name":"sh-host-abcdef"}}"#]);
        let commands = remote.commands.clone();
        let provider = StructuredKubectl::new(remote);
        provider
            .patch_battlegroup_stop("funcom-seabass-sh-host-abcdef", "sh-host-abcdef", true)
            .unwrap();
        let command = commands.borrow().first().cloned().unwrap();
        assert!(command.contains("--type=merge"));
        assert!(command.contains("\"stop\":true"));
        assert!(command.contains("-o json"));
    }
}
