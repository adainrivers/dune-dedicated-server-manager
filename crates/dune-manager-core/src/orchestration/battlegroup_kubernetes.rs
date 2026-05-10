use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    errors::{failure, parse_json},
    models::CommandResult,
    orchestration::{BattlegroupRef, RemoteCommandRunner},
    validation::validate_kube_arg,
};

const BATTLEGROUP_NAMESPACE_PREFIX: &str = "funcom-seabass-";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PodContainerRef {
    pub pod: String,
    pub container: String,
    pub role: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PodShellSpec {
    pub namespace: String,
    pub pod: String,
    pub commands: Vec<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogFile {
    pub relative_path: String,
    pub contents: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BattlegroupStatusSnapshot {
    pub battlegroup: Value,
    pub pods: Vec<PodContainerRef>,
    pub director_node_port: Option<u16>,
}

#[derive(Debug, Clone)]
pub struct StructuredBattlegroupOps<R> {
    runner: R,
}

impl<R> StructuredBattlegroupOps<R>
where
    R: RemoteCommandRunner,
{
    pub fn new(runner: R) -> Self {
        Self { runner }
    }

    pub fn list(&self) -> CommandResult<Vec<BattlegroupRef>> {
        let value = self.runner.run_json(
            "sudo kubectl get battlegroups -A -o json",
            "battlegroup list",
        )?;
        let mut refs = value["items"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|item| {
                let namespace = item["metadata"]["namespace"].as_str()?.to_string();
                let name = item["metadata"]["name"].as_str()?.to_string();
                Some(BattlegroupRef { namespace, name })
            })
            .filter(|item| item.namespace.starts_with(BATTLEGROUP_NAMESPACE_PREFIX))
            .collect::<Vec<_>>();
        refs.sort_by(|left, right| left.namespace.cmp(&right.namespace));
        Ok(refs)
    }

    pub fn status(&self, battlegroup: &BattlegroupRef) -> CommandResult<BattlegroupStatusSnapshot> {
        battlegroup.validate()?;
        let bg_command = format!(
            "sudo kubectl get battlegroup {} -n {} -o json",
            sh_single_quoted(&battlegroup.name),
            sh_single_quoted(&battlegroup.namespace)
        );
        let battlegroup_json = self.runner.run_json(&bg_command, "battlegroup status")?;
        Ok(BattlegroupStatusSnapshot {
            battlegroup: battlegroup_json,
            pods: self.list_pods(&battlegroup.namespace)?,
            director_node_port: self.director_node_port(&battlegroup.namespace)?,
        })
    }

    pub fn patch_region(&self, battlegroup: &BattlegroupRef, region: &str) -> CommandResult<()> {
        battlegroup.validate()?;
        validate_region(region)?;
        let mut script = String::from("set -euo pipefail\n");
        script.push_str(&shell_value("NS", &battlegroup.namespace));
        script.push_str(&shell_value("BG", &battlegroup.name));
        script.push_str(&shell_value("REGION", region));
        script.push_str(
            r#"
sudo kubectl get battlegroup "$BG" -n "$NS" -o json |
jq --arg region "$REGION" '
  def patch_region:
    if type == "object" then
      with_entries(
        if .key == "dataCenter" and (.value | type == "string") then
          .value = $region
        else
          .
        end
      )
      | if .name? == "BATTLEGROUP_REGION_NAME" and has("value") then .value = $region else . end
      | with_entries(.value |= patch_region)
    elif type == "array" then
      map(if type == "string" and startswith("-FarmRegion=") then "-FarmRegion=" + $region else patch_region end)
    else
      .
    end;
  patch_region
' |
sudo kubectl replace -f - -o json
"#,
        );
        let output = self.runner.run_script(&script)?;
        let value: Value = parse_json(&output, "patched battlegroup")?;
        let patched_name = value["metadata"]["name"].as_str().unwrap_or_default();
        if patched_name != battlegroup.name {
            return Err(failure(
                "Region patch did not return the expected battlegroup",
            ));
        }
        Ok(())
    }

    pub fn list_pods(&self, namespace: &str) -> CommandResult<Vec<PodContainerRef>> {
        validate_kube_arg(namespace, "namespace")?;
        let command = format!(
            "sudo kubectl get pods -n {} -o json",
            sh_single_quoted(namespace)
        );
        let value = self.runner.run_json(&command, "pod list")?;
        let mut pods = Vec::new();
        for item in value["items"].as_array().cloned().unwrap_or_default() {
            let pod = item["metadata"]["name"].as_str().unwrap_or_default();
            if pod.is_empty() {
                continue;
            }
            let role = item["metadata"]["labels"]["role"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            for container in item["spec"]["containers"]
                .as_array()
                .cloned()
                .unwrap_or_default()
            {
                let container_name = container["name"].as_str().unwrap_or_default();
                if !container_name.is_empty() {
                    pods.push(PodContainerRef {
                        pod: pod.to_string(),
                        container: container_name.to_string(),
                        role: role.clone(),
                    });
                }
            }
        }
        pods.sort_by(|left, right| {
            left.pod
                .cmp(&right.pod)
                .then(left.container.cmp(&right.container))
        });
        Ok(pods)
    }

    pub fn pod_shell_spec(&self, namespace: &str, pod: &str) -> CommandResult<PodShellSpec> {
        validate_kube_arg(namespace, "namespace")?;
        validate_kube_arg(pod, "pod")?;
        Ok(PodShellSpec {
            namespace: namespace.to_string(),
            pod: pod.to_string(),
            commands: vec![
                vec![
                    "sudo".into(),
                    "kubectl".into(),
                    "exec".into(),
                    "-it".into(),
                    pod.into(),
                    "-n".into(),
                    namespace.into(),
                    "--".into(),
                    "/bin/bash".into(),
                ],
                vec![
                    "sudo".into(),
                    "kubectl".into(),
                    "exec".into(),
                    "-it".into(),
                    pod.into(),
                    "-n".into(),
                    namespace.into(),
                    "--".into(),
                    "/bin/sh".into(),
                ],
            ],
        })
    }

    pub fn export_namespace_logs(&self, namespace: &str) -> CommandResult<Vec<LogFile>> {
        let pods = self.list_pods(namespace)?;
        self.collect_logs(namespace, &pods)
    }

    pub fn export_operator_logs(&self) -> CommandResult<Vec<LogFile>> {
        let pods = self.list_pods("funcom-operators")?;
        self.collect_logs("funcom-operators", &pods)
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

    fn collect_logs(
        &self,
        namespace: &str,
        pods: &[PodContainerRef],
    ) -> CommandResult<Vec<LogFile>> {
        let mut files = Vec::new();
        for item in pods {
            validate_kube_arg(&item.pod, "pod")?;
            validate_kube_arg(&item.container, "container")?;
            let command = format!(
                "sudo kubectl logs -n {} {} -c {} --timestamps --tail=-1",
                sh_single_quoted(namespace),
                sh_single_quoted(&item.pod),
                sh_single_quoted(&item.container),
            );
            let contents = self.runner.run(&command)?;
            files.push(LogFile {
                relative_path: format!("{}/{}.log", item.pod, item.container),
                contents,
            });
        }
        Ok(files)
    }
}

fn validate_region(region: &str) -> CommandResult<()> {
    match region {
        "Europe Test" | "North America Test" => Ok(()),
        _ => Err(failure("Region must be Europe Test or North America Test")),
    }
}

fn sh_single_quoted(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn shell_value(name: &str, value: &str) -> String {
    let delimiter = format!("__DUNE_MANAGER_{name}__");
    format!("{name}=$(cat <<'{delimiter}'\n{value}\n{delimiter}\n)\n")
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
    fn lists_battlegroups_from_cluster_json() {
        let remote = MockRemote::with_outputs([r#"{
          "items": [
            {"metadata":{"namespace":"default","name":"ignored"}},
            {"metadata":{"namespace":"funcom-seabass-sh-host-bbbbbb","name":"sh-host-bbbbbb"}},
            {"metadata":{"namespace":"funcom-seabass-sh-host-aaaaaa","name":"sh-host-aaaaaa"}}
          ]
        }"#]);
        let ops = StructuredBattlegroupOps::new(remote);
        assert_eq!(
            ops.list().unwrap(),
            vec![
                BattlegroupRef {
                    namespace: "funcom-seabass-sh-host-aaaaaa".to_string(),
                    name: "sh-host-aaaaaa".to_string(),
                },
                BattlegroupRef {
                    namespace: "funcom-seabass-sh-host-bbbbbb".to_string(),
                    name: "sh-host-bbbbbb".to_string(),
                }
            ]
        );
    }

    #[test]
    fn region_patch_uses_structured_jq_transform_not_sed() {
        let remote = MockRemote::with_outputs([r#"{"metadata":{"name":"sh-host-abcdef"}}"#]);
        let commands = remote.commands.clone();
        let ops = StructuredBattlegroupOps::new(remote);
        ops.patch_region(
            &BattlegroupRef {
                namespace: "funcom-seabass-sh-host-abcdef".to_string(),
                name: "sh-host-abcdef".to_string(),
            },
            "Europe Test",
        )
        .unwrap();
        let script = commands.borrow().first().cloned().unwrap();
        assert!(script.contains("jq --arg region"));
        assert!(script.contains("BATTLEGROUP_REGION_NAME"));
        assert!(script.contains("dataCenter"));
        assert!(script.contains("startsWith") || script.contains("startswith"));
        assert!(!script.contains(" sed "));
        assert!(script.contains("kubectl replace -f - -o json"));
    }

    #[test]
    fn exports_logs_by_enumerating_pods_and_containers() {
        let remote = MockRemote::with_outputs([
            r#"{
              "items": [{
                "metadata":{"name":"pod-a","labels":{"role":"gateway"}},
                "spec":{"containers":[{"name":"main"},{"name":"sidecar"}]}
              }]
            }"#,
            "main log",
            "sidecar log",
        ]);
        let commands = remote.commands.clone();
        let ops = StructuredBattlegroupOps::new(remote);
        let files = ops
            .export_namespace_logs("funcom-seabass-sh-host-abcdef")
            .unwrap();
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].relative_path, "pod-a/main.log");
        let commands = commands.borrow();
        assert!(commands[0].contains("kubectl get pods"));
        assert!(commands[1].contains("kubectl logs"));
        assert!(commands[1].contains("--timestamps"));
    }

    #[test]
    fn builds_pod_shell_command_candidates() {
        let ops = StructuredBattlegroupOps::new(MockRemote::default());
        let spec = ops
            .pod_shell_spec("funcom-seabass-sh-host-abcdef", "pod-a")
            .unwrap();
        assert_eq!(spec.commands[0].last().unwrap(), "/bin/bash");
        assert_eq!(spec.commands[1].last().unwrap(), "/bin/sh");
    }
}
