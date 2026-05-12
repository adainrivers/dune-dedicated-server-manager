use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    errors::failure,
    models::CommandResult,
    orchestration::{
        parse_single_json_document, CreatedWorld, GuestBootstrapProvider, RemoteCommandRunner,
        WorldManifestRequest,
    },
    validation::validate_kube_arg,
};

const DUNE_HOME: &str = "/home/dune/.dune";
const SERVER_APP_ID: &str = "3104830";

/// SSH-backed implementation of the guest bootstrap phases.
#[derive(Debug, Clone)]
pub struct SshGuestBootstrapProvider<R> {
    runner: R,
}

impl<R> SshGuestBootstrapProvider<R>
where
    R: RemoteCommandRunner,
{
    /// Creates a bootstrap provider around a remote command runner.
    pub fn new(runner: R) -> Self {
        Self { runner }
    }

    fn run_phase(&self, body: &str) -> CommandResult<String> {
        self.runner.run_script(&with_guest_path(body))
    }
}

impl<R> GuestBootstrapProvider for SshGuestBootstrapProvider<R>
where
    R: RemoteCommandRunner,
{
    fn validate_and_resize_root_disk(&self) -> CommandResult<()> {
        self.run_phase(DISK_SCRIPT)?;
        Ok(())
    }

    fn ensure_server_payload(&self) -> CommandResult<()> {
        self.run_phase(&download_script())?;
        Ok(())
    }

    fn start_k3s_and_wait(&self) -> CommandResult<()> {
        self.run_phase(START_K3S_SCRIPT)?;
        Ok(())
    }

    fn import_core_images(&self) -> CommandResult<()> {
        self.run_phase(&format!(
            "{}\n{}",
            CONTAINER_IMAGE_HELPERS, IMPORT_CORE_IMAGES_SCRIPT
        ))?;
        Ok(())
    }

    fn scale_core_deployments(&self) -> CommandResult<()> {
        self.run_phase(&format!("{}\n{}", KUBECTL_HELPERS, SCALE_CORE_SCRIPT))?;
        Ok(())
    }

    fn update_operator_crds(&self) -> CommandResult<()> {
        self.run_phase(&format!(
            "{}\n{}",
            KUBECTL_HELPERS, UPDATE_OPERATOR_CRDS_SCRIPT
        ))?;
        Ok(())
    }

    fn patch_operator_images(&self) -> CommandResult<()> {
        self.run_phase(&format!(
            "{}\n{}\n{}",
            KUBECTL_HELPERS, CONTAINER_IMAGE_HELPERS, PATCH_OPERATOR_IMAGES_SCRIPT
        ))?;
        Ok(())
    }

    fn scale_operator_deployments(&self) -> CommandResult<()> {
        self.run_phase(&format!("{}\n{}", KUBECTL_HELPERS, SCALE_OPERATOR_SCRIPT))?;
        Ok(())
    }

    fn install_battlegroup_helper(&self) -> CommandResult<()> {
        self.run_phase(INSTALL_HELPER_SCRIPT)?;
        Ok(())
    }

    fn create_world(&self, request: &WorldManifestRequest) -> CommandResult<CreatedWorld> {
        validate_world_manifest_request(request)?;
        let script = create_world_script(request);
        let output = self.run_phase(&script)?;
        let result: CreateWorldOutput = parse_single_json_document(&output, "create world")?;
        Ok(CreatedWorld {
            namespace: result.namespace,
            battlegroup_name: result.battlegroup_name,
        })
    }

    fn import_battlegroup_images(&self) -> CommandResult<()> {
        self.run_phase(&format!(
            "{}\n{}",
            CONTAINER_IMAGE_HELPERS, IMPORT_BATTLEGROUP_IMAGES_SCRIPT
        ))?;
        Ok(())
    }

    fn patch_battlegroup_images(
        &self,
        namespace: &str,
        battlegroup_name: &str,
    ) -> CommandResult<()> {
        validate_kube_arg(namespace, "namespace")?;
        validate_kube_arg(battlegroup_name, "battlegroup name")?;
        let new_version = self
            .run_phase(READ_BATTLEGROUP_VERSION_SCRIPT)?
            .trim()
            .to_string();
        if new_version.is_empty() {
            return Err(failure("Battlegroup image version file was empty"));
        }

        let command = format!(
            "sudo kubectl get battlegroup {} -n {} -o json",
            sh_single_quoted(battlegroup_name),
            sh_single_quoted(namespace),
        );
        let battlegroup_json = self
            .runner
            .run_json(&command, "battlegroup image patch source")?;
        let operations = battlegroup_image_patch_operations(&battlegroup_json, &new_version)?;
        let patch_command = format!(
            "sudo kubectl patch battlegroup {} -n {} --type=json -p {} -o json",
            sh_single_quoted(battlegroup_name),
            sh_single_quoted(namespace),
            sh_single_quoted(&serde_json::to_string(&operations).map_err(|err| {
                failure(format!(
                    "Failed to serialize battlegroup image patch: {err}"
                ))
            })?),
        );
        self.runner.run(&patch_command)?;
        Ok(())
    }

    fn apply_default_user_settings(
        &self,
        namespace: &str,
        battlegroup_name: &str,
    ) -> CommandResult<()> {
        validate_kube_arg(namespace, "namespace")?;
        validate_kube_arg(battlegroup_name, "battlegroup name")?;
        let mut script = String::new();
        script.push_str("set -eu\n");
        script.push_str(&shell_value("NS", namespace));
        script.push_str(APPLY_DEFAULT_SETTINGS_SCRIPT);
        self.run_phase(&script)?;
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateWorldOutput {
    namespace: String,
    battlegroup_name: String,
}

fn validate_world_manifest_request(request: &WorldManifestRequest) -> CommandResult<()> {
    validate_kube_arg(&request.world_unique_name, "world unique name")?;
    if request.world_name.trim().is_empty()
        || request.world_name.chars().count() > 50
        || request.world_name.contains('\n')
        || request.world_name.contains('\r')
    {
        return Err(failure(
            "World name must be 1-50 characters and single-line",
        ));
    }
    match request.world_region.as_str() {
        "Europe Test" | "North America Test" => {}
        _ => return Err(failure("Region must be Europe Test or North America Test")),
    }
    if request.self_host_token.trim().is_empty()
        || request.self_host_token.contains('\n')
        || request.self_host_token.contains('\r')
    {
        return Err(failure("Self-host token is required"));
    }
    Ok(())
}

fn with_guest_path(script: &str) -> String {
    format!(
        "export PATH=\"/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:$PATH\"\n{script}"
    )
}

fn download_script() -> String {
    format!(
        r#"
set -eu
DUNE_USER_PATH={dune_home}
DOWNLOAD_PATH="$DUNE_USER_PATH/download"
mkdir -p "$DOWNLOAD_PATH"
if [ -f "$DOWNLOAD_PATH/scripts/battlegroup.sh" ] && [ -f "$DOWNLOAD_PATH/scripts/setup.sh" ]; then
  exit 0
fi
steamcmd_update_once() {{
  if command -v timeout >/dev/null 2>&1; then
    timeout 45m steamcmd +@ShutdownOnFailedCommand 1 +@NoPromptForPassword 1 +set_spew_level 1 1 +force_install_dir "$DOWNLOAD_PATH" +login anonymous +app_update {app_id} validate +logoff +quit < /dev/null >&2
  else
    steamcmd +@ShutdownOnFailedCommand 1 +@NoPromptForPassword 1 +set_spew_level 1 1 +force_install_dir "$DOWNLOAD_PATH" +login anonymous +app_update {app_id} validate +logoff +quit < /dev/null >&2
  fi
}}
attempt=1
max_attempts=5
while [ "$attempt" -le "$max_attempts" ]; do
  echo "SteamCMD payload download attempt $attempt/$max_attempts." >&2
  if steamcmd_update_once; then
    break
  fi
  status=$?
  if [ "$attempt" -ge "$max_attempts" ]; then
    echo "SteamCMD payload download failed after $max_attempts attempts, last exit code $status." >&2
    exit "$status"
  fi
  sleep_seconds=$((attempt * 15))
  echo "SteamCMD payload download failed with exit code $status; retrying in ${{sleep_seconds}}s." >&2
  sleep "$sleep_seconds"
  attempt=$((attempt + 1))
done
test -f "$DOWNLOAD_PATH/scripts/battlegroup.sh"
test -f "$DOWNLOAD_PATH/scripts/setup.sh"
"#,
        dune_home = DUNE_HOME,
        app_id = SERVER_APP_ID
    )
}

const DISK_SCRIPT: &str = r#"
set -eu
required_gb=30
available_gb=$(df -B1G -P / | awk '$NF == "/" {print $(NF-2)+0}')
if [ "$available_gb" -le "$required_gb" ]; then
  sudo growpart /dev/sda 2 >&2 || true
  sudo pvresize /dev/sda2 >&2 || true
  sudo lvextend -l +100%FREE /dev/mapper/vg0-lv_root >&2 || true
  sudo resize2fs /dev/mapper/vg0-lv_root >&2 || true
fi
available_gb=$(df -B1G -P / | awk '$NF == "/" {print $(NF-2)+0}')
if [ "$available_gb" -le "$required_gb" ]; then
  echo "Not enough guest disk space after resize: ${available_gb}GB available, need more than ${required_gb}GB" >&2
  exit 1
fi
"#;

const START_K3S_SCRIPT: &str = r#"
set -eu
sudo rc-service k3s start >&2 || true
sudo rc-service k3s restart >&2
elapsed=0
while [ ! -S /run/k3s/containerd/containerd.sock ]; do
  sleep 2
  elapsed=$((elapsed + 2))
  if [ "$elapsed" -ge 60 ]; then echo "k3s containerd did not return in 60s" >&2; exit 1; fi
done
elapsed=0
until sudo kubectl get nodes >/dev/null 2>&1; do
  sleep 2
  elapsed=$((elapsed + 2))
  if [ "$elapsed" -ge 60 ]; then echo "k3s API did not return in 60s" >&2; exit 1; fi
done
sudo kubectl wait --for=condition=Ready node --all --timeout=180s >/dev/null || true
sudo rc-update add k3s >/dev/null
"#;

const CONTAINER_IMAGE_HELPERS: &str = r#"
set -eu
DOWNLOAD_PATH=/home/dune/.dune/download
load_image_from_file() {
  local file_name="$1"
  if [ ! -f "$DOWNLOAD_PATH/$file_name" ]; then
    echo "Image file $DOWNLOAD_PATH/$file_name does not exist" >&2
    exit 1
  fi
  local attempt=1
  while [ "$attempt" -le 3 ]; do
    if sudo ctr -n k8s.io images import "$DOWNLOAD_PATH/$file_name" >&2; then
      return 0
    fi
    sleep 5
    attempt=$((attempt + 1))
  done
  echo "Failed to import $file_name after 3 attempts" >&2
  exit 1
}
"#;

const IMPORT_CORE_IMAGES_SCRIPT: &str = r#"
load_image_from_file "images/prerequisites/coredns-coredns.tar"
load_image_from_file "images/prerequisites/local-path-provisioner.tar"
load_image_from_file "images/prerequisites/metrics-server.tar"
load_image_from_file "images/prerequisites/cert-manager-webhook.tar"
load_image_from_file "images/prerequisites/cert-manager-controller.tar"
load_image_from_file "images/prerequisites/cert-manager-cainjector.tar"
load_image_from_file "images/prerequisites/igw-postgres.tar"
"#;

const KUBECTL_HELPERS: &str = r#"
set -eu
kubectl_retry() {
  local attempt=1 out rc
  while [ "$attempt" -le 5 ]; do
    out=$(sudo kubectl "$@" 2>&1)
    rc=$?
    if [ "$rc" -eq 0 ]; then
      [ -n "$out" ] && printf '%s\n' "$out" >&2
      return 0
    fi
    if printf '%s' "$out" | grep -qiE 'connection refused|unable to connect to the server|i/o timeout|tls handshake|no route to host|EOF'; then
      sleep 5
      attempt=$((attempt + 1))
      continue
    fi
    printf '%s\n' "$out" >&2
    return "$rc"
  done
  echo "kubectl $* still failing after retries" >&2
  return 1
}
wait_for_deployment() {
  local ns="$1" name="$2" timeout="${3:-120}" elapsed=0
  until sudo kubectl get -n "$ns" deployment "$name" >/dev/null 2>&1; do
    sleep 2
    elapsed=$((elapsed + 2))
    if [ "$elapsed" -ge "$timeout" ]; then echo "deployment $ns/$name did not appear within ${timeout}s" >&2; return 1; fi
  done
}
scale_deployment() {
  local ns="$1" name="$2" replicas="$3"
  wait_for_deployment "$ns" "$name" 120
  kubectl_retry scale -n "$ns" "deployment/$name" "--replicas=$replicas"
}
operator_versions_differ() {
  local version_file="$DOWNLOAD_PATH/images/operators/version.txt"
  if [ ! -f "$version_file" ]; then
    echo "No operator version file found at $version_file" >&2
    return 1
  fi
  local current_version new_operator_version
  current_version=$(kubectl_retry get -n funcom-operators deployment/battlegroupoperator-controller-manager -o jsonpath='{.spec.template.spec.containers[0].image}' | sed 's/.*://')
  new_operator_version=$(cat "$version_file")
  [ "$current_version" != "$new_operator_version" ]
}
DOWNLOAD_PATH=/home/dune/.dune/download
"#;

const SCALE_CORE_SCRIPT: &str = r#"
scale_deployment kube-system coredns 1
scale_deployment kube-system local-path-provisioner 1
scale_deployment kube-system metrics-server 1
scale_deployment cert-manager cert-manager 1
scale_deployment cert-manager cert-manager-cainjector 1
scale_deployment cert-manager cert-manager-webhook 1
"#;

const UPDATE_OPERATOR_CRDS_SCRIPT: &str = r#"
if operator_versions_differ; then
  kubectl_retry replace -n funcom-operators -f "$DOWNLOAD_PATH/images/operators/crds/" || kubectl_retry apply -n funcom-operators -f "$DOWNLOAD_PATH/images/operators/crds/"
fi
"#;

const PATCH_OPERATOR_IMAGES_SCRIPT: &str = r#"
if operator_versions_differ; then
  new_operator_version=$(cat "$DOWNLOAD_PATH/images/operators/version.txt")
  load_image_from_file "images/operators/battlegroup-operator.tar"
  load_image_from_file "images/operators/database-operator.tar"
  load_image_from_file "images/operators/server-operator.tar"
  load_image_from_file "images/operators/utilities-operator.tar"
  kubectl_retry set -n funcom-operators image deployment/battlegroupoperator-controller-manager manager=registry.funcom.com/funcom/self-hosting/igw-k8s-battlegroup-operator:"$new_operator_version"
  kubectl_retry set -n funcom-operators image deployment/databaseoperator-controller-manager manager=registry.funcom.com/funcom/self-hosting/igw-k8s-database-operator:"$new_operator_version"
  kubectl_retry set -n funcom-operators image deployment/serveroperator-controller-manager manager=registry.funcom.com/funcom/self-hosting/igw-k8s-server-operator:"$new_operator_version"
  kubectl_retry set -n funcom-operators image deployment/utilitiesoperator-controller-manager manager=registry.funcom.com/funcom/self-hosting/igw-k8s-utilities-operator:"$new_operator_version"
fi
"#;

const SCALE_OPERATOR_SCRIPT: &str = r#"
scale_deployment funcom-operators battlegroupoperator-controller-manager 1
scale_deployment funcom-operators databaseoperator-controller-manager 1
scale_deployment funcom-operators serveroperator-controller-manager 1
scale_deployment funcom-operators utilitiesoperator-controller-manager 1
"#;

const INSTALL_HELPER_SCRIPT: &str = r#"
set -eu
mkdir -p /home/dune/.dune/bin
test -f /home/dune/.dune/download/scripts/battlegroup.sh
ln -sfn /home/dune/.dune/download/scripts/battlegroup.sh /home/dune/.dune/bin/battlegroup
chmod +x /home/dune/.dune/download/scripts/battlegroup.sh
"#;

fn create_world_script(request: &WorldManifestRequest) -> String {
    let namespace = format!("funcom-seabass-{}", request.world_unique_name);
    let title_patch = json!({
        "spec": {
            "title": request.world_name.trim(),
        }
    })
    .to_string();
    let mut script = String::from("set -eu\n");
    script.push_str("G_SPEC_PATH=/home/dune/.dune\n");
    script.push_str("G_SCRIPT_PATH=/home/dune/.dune/download/scripts/setup\n");
    script.push_str(&shell_value("WORLD_NAME", request.world_name.trim()));
    script.push_str(&shell_value("WORLD_REGION", request.world_region.trim()));
    script.push_str(&shell_value(
        "WORLD_UNIQUE_NAME",
        &request.world_unique_name,
    ));
    script.push_str(&shell_value("NS", &namespace));
    script.push_str(&shell_value("FLS_TOKEN", request.self_host_token.trim()));
    script.push_str(&shell_value("TITLE_PATCH", &title_patch));
    script.push_str(
        r#"
if sudo kubectl get ns "$NS" >/dev/null 2>&1; then
  echo "Battlegroup namespace already exists: $NS" >&2
  exit 1
fi
RMQ_SECRET=$(openssl rand -base64 64 | tr -d '\n')
escape_sed() { printf '%s' "$1" | sed -e 's/[\/&]/\\&/g'; }
escape_sed_pipe() { printf '%s' "$1" | sed -e 's/[|&]/\\&/g'; }
cp "$G_SCRIPT_PATH/templates/world-template.yaml" "$G_SPEC_PATH/$WORLD_UNIQUE_NAME.yaml"
cp "$G_SCRIPT_PATH/templates/fls-secret.yaml" "$G_SPEC_PATH/$WORLD_UNIQUE_NAME-fls-secret.yaml"
cp "$G_SCRIPT_PATH/templates/rmq-secret.yaml" "$G_SPEC_PATH/$WORLD_UNIQUE_NAME-rmq-secret.yaml"
sed -i "s/{WORLD_NAME}/$(escape_sed "$WORLD_NAME")/g" "$G_SPEC_PATH/$WORLD_UNIQUE_NAME.yaml"
sed -i "s/{WORLD_UNIQUE_NAME}/$(escape_sed "$WORLD_UNIQUE_NAME")/g" "$G_SPEC_PATH/$WORLD_UNIQUE_NAME.yaml"
sed -i "s/{WORLD_REGION}/$(escape_sed "$WORLD_REGION")/g" "$G_SPEC_PATH/$WORLD_UNIQUE_NAME.yaml"
sed -i "s/{WORLD_IMAGE_TAG}/0-0-shipping/g" "$G_SPEC_PATH/$WORLD_UNIQUE_NAME.yaml"
sed -i "s/{FLS_SECRET}/$(escape_sed "$FLS_TOKEN")/g" "$G_SPEC_PATH/$WORLD_UNIQUE_NAME.yaml"
sed -i "s/{FLS_SECRET}/$(escape_sed "$FLS_TOKEN")/g" "$G_SPEC_PATH/$WORLD_UNIQUE_NAME-fls-secret.yaml"
sed -i "s|{RMQ_SECRET}|$(escape_sed_pipe "$RMQ_SECRET")|g" "$G_SPEC_PATH/$WORLD_UNIQUE_NAME-rmq-secret.yaml"
elapsed=0
while [ "$elapsed" -lt 300 ]; do
  all_ready=true
  for op in battlegroupoperator-controller-manager databaseoperator-controller-manager serveroperator-controller-manager utilitiesoperator-controller-manager; do
    ready=$(sudo kubectl get -n funcom-operators deployment/"$op" -o jsonpath='{.status.readyReplicas}' 2>/dev/null || true)
    if [ "$ready" != "1" ]; then all_ready=false; break; fi
  done
  if $all_ready; then break; fi
  sleep 5
  elapsed=$((elapsed + 5))
done
if [ "$elapsed" -ge 300 ]; then
  echo "Timed out waiting for operators" >&2
  exit 1
fi
sudo kubectl create ns "$NS" >&2
sudo kubectl create -n "$NS" -f "$G_SPEC_PATH/$WORLD_UNIQUE_NAME-fls-secret.yaml" >&2
sudo kubectl create -n "$NS" -f "$G_SPEC_PATH/$WORLD_UNIQUE_NAME-rmq-secret.yaml" >&2
sudo kubectl create -n "$NS" -f "$G_SPEC_PATH/$WORLD_UNIQUE_NAME.yaml" >&2
sudo kubectl patch battlegroup "$WORLD_UNIQUE_NAME" -n "$NS" --type=merge -p "$TITLE_PATCH" >&2
printf '%s' "$WORLD_UNIQUE_NAME" > /home/dune/.dune/.manager-bootstrap-world-name
printf '{"namespace":"%s","battlegroupName":"%s"}\n' "$NS" "$WORLD_UNIQUE_NAME"
"#,
    );
    script
}

const IMPORT_BATTLEGROUP_IMAGES_SCRIPT: &str = r#"
load_image_from_file "images/battlegroup/server-rabbitmq.tar"
load_image_from_file "images/battlegroup/server-text-router.tar"
load_image_from_file "images/battlegroup/server-bg-director.tar"
load_image_from_file "images/battlegroup/server-gateway.tar"
load_image_from_file "images/battlegroup/server-db-utils.tar"
load_image_from_file "images/battlegroup/server.tar"
"#;

const READ_BATTLEGROUP_VERSION_SCRIPT: &str = r#"
DOWNLOAD_PATH=/home/dune/.dune/download
version_file="$DOWNLOAD_PATH/images/battlegroup/version.txt"
if [ ! -f "$version_file" ]; then
  echo "No battlegroup version file found at $version_file" >&2
  exit 1
fi
cat "$version_file"
"#;

const APPLY_DEFAULT_SETTINGS_SCRIPT: &str = r#"
DOWNLOAD_PATH=/home/dune/.dune/download
config_dir="$DOWNLOAD_PATH/scripts/setup/config"
if ! ls "$config_dir"/User*.ini >/dev/null 2>&1; then
  echo "No User*.ini files found in $config_dir" >&2
  exit 1
fi
elapsed=0
fb_pod=""
while [ "$elapsed" -lt 240 ]; do
  fb_pod=$(sudo kubectl get pods -n "$NS" -l role=igw-filebrowser --no-headers -o custom-columns=NAME:.metadata.name 2>/dev/null | head -n1 || true)
  if [ -n "$fb_pod" ]; then break; fi
  sleep 5
  elapsed=$((elapsed + 5))
done
if [ -z "$fb_pod" ]; then
  echo "No filebrowser pod became available in $NS" >&2
  exit 1
fi
sudo kubectl exec -n "$NS" "$fb_pod" -- mkdir -p /srv/UserSettings >&2
for config_file in "$config_dir"/User*.ini; do
  filename=$(basename "$config_file")
  sudo kubectl cp "$config_file" "$NS/$fb_pod:/srv/UserSettings/$filename" >&2
done
"#;

fn shell_value(name: &str, value: &str) -> String {
    let delimiter = format!("__DUNE_MANAGER_{name}__");
    format!("{name}=$(cat <<'{delimiter}'\n{value}\n{delimiter}\n)\n")
}

fn sh_single_quoted(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn battlegroup_image_patch_operations(
    value: &Value,
    new_version: &str,
) -> CommandResult<Vec<Value>> {
    let mut operations = Vec::new();
    collect_battlegroup_image_patch_operations(
        value,
        &mut Vec::new(),
        new_version,
        &mut operations,
    );
    if operations.is_empty() {
        return Err(failure("No battlegroup server images were found to patch"));
    }
    Ok(operations)
}

fn collect_battlegroup_image_patch_operations(
    value: &Value,
    path: &mut Vec<String>,
    new_version: &str,
    operations: &mut Vec<Value>,
) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                path.push(key.clone());
                if key == "image" {
                    if let Some(updated) = child
                        .as_str()
                        .and_then(|image| revised_seabass_server_image(image, new_version))
                    {
                        operations.push(replace_operation(path, json!(updated)));
                    }
                }
                collect_battlegroup_image_patch_operations(child, path, new_version, operations);
                path.pop();
            }
        }
        Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                path.push(index.to_string());
                collect_battlegroup_image_patch_operations(child, path, new_version, operations);
                path.pop();
            }
        }
        _ => {}
    }
}

fn revised_seabass_server_image(image: &str, new_version: &str) -> Option<String> {
    let file = image.rsplit('/').next().unwrap_or(image);
    if !file.starts_with("seabass-server") {
        return None;
    }
    let (prefix, _) = image.rsplit_once(':')?;
    Some(format!("{prefix}:{new_version}"))
}

fn replace_operation(path: &[String], value: Value) -> Value {
    json!({
        "op": "replace",
        "path": json_pointer(path),
        "value": value,
    })
}

fn json_pointer(path: &[String]) -> String {
    format!(
        "/{}",
        path.iter()
            .map(|item| item.replace('~', "~0").replace('/', "~1"))
            .collect::<Vec<_>>()
            .join("/")
    )
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, collections::VecDeque, rc::Rc};

    use super::*;

    #[derive(Clone, Default)]
    struct MockRemote {
        outputs: Rc<RefCell<VecDeque<String>>>,
        scripts: Rc<RefCell<Vec<String>>>,
    }

    impl MockRemote {
        fn with_outputs(outputs: impl IntoIterator<Item = impl Into<String>>) -> Self {
            Self {
                outputs: Rc::new(RefCell::new(outputs.into_iter().map(Into::into).collect())),
                scripts: Rc::new(RefCell::new(Vec::new())),
            }
        }
    }

    impl RemoteCommandRunner for MockRemote {
        fn run(&self, command: &str) -> CommandResult<String> {
            self.run_script(command)
        }

        fn run_script(&self, script: &str) -> CommandResult<String> {
            self.scripts.borrow_mut().push(script.to_string());
            Ok(self.outputs.borrow_mut().pop_front().unwrap_or_default())
        }
    }

    #[test]
    fn create_world_returns_structured_json_only() {
        let remote = MockRemote::with_outputs([
            r#"{"namespace":"funcom-seabass-sh-host-abcdef","battlegroupName":"sh-host-abcdef"}"#,
        ]);
        let scripts = remote.scripts.clone();
        let provider = SshGuestBootstrapProvider::new(remote);
        let world = provider
            .create_world(&WorldManifestRequest {
                world_name: "Adain".to_string(),
                world_region: "Europe Test".to_string(),
                world_unique_name: "sh-host-abcdef".to_string(),
                self_host_token: "header.payload.signature".to_string(),
            })
            .unwrap();

        assert_eq!(world.namespace, "funcom-seabass-sh-host-abcdef");
        let script = scripts.borrow().first().cloned().unwrap();
        assert!(script.contains("printf '{\"namespace\":\"%s\",\"battlegroupName\":\"%s\"}"));
        assert!(script.contains("kubectl create ns \"$NS\" >&2"));
        assert!(script.contains("s/{WORLD_IMAGE_TAG}/0-0-shipping/g"));
        assert!(!script.contains(
            "WORLD_IMAGE_TAG=$(cat \"$G_SPEC_PATH/download/images/battlegroup/version.txt\")"
        ));
    }

    #[test]
    fn create_world_patches_full_title_after_template_creation() {
        let remote = MockRemote::with_outputs([
            r#"{"namespace":"funcom-seabass-sh-host-abcdef","battlegroupName":"sh-host-abcdef"}"#,
        ]);
        let scripts = remote.scripts.clone();
        let provider = SshGuestBootstrapProvider::new(remote);
        provider
            .create_world(&WorldManifestRequest {
                world_name: "Great Banana".to_string(),
                world_region: "Europe Test".to_string(),
                world_unique_name: "sh-host-abcdef".to_string(),
                self_host_token: "header.payload.signature".to_string(),
            })
            .unwrap();

        let script = scripts.borrow().first().cloned().unwrap();
        assert!(script.contains("\"title\":\"Great Banana\""));
        assert!(script.contains("kubectl patch battlegroup \"$WORLD_UNIQUE_NAME\""));
    }

    #[test]
    fn provider_splits_vendor_k3s_work_into_explicit_phases() {
        let remote = MockRemote::default();
        let scripts = remote.scripts.clone();
        let provider = SshGuestBootstrapProvider::new(remote);

        provider.start_k3s_and_wait().unwrap();
        provider.import_core_images().unwrap();
        provider.scale_core_deployments().unwrap();

        let scripts = scripts.borrow();
        assert!(scripts[0].contains("rc-service k3s restart"));
        assert!(scripts[1].contains("coredns-coredns.tar"));
        assert!(scripts[2].contains("scale_deployment kube-system coredns 1"));
    }

    #[test]
    fn guest_download_uses_validating_app_update() {
        let script = download_script();
        assert!(script.contains("+app_update 3104830 validate"));
    }

    #[test]
    fn guest_download_retries_without_interactive_prompts() {
        let script = download_script();
        assert!(script.contains("+@ShutdownOnFailedCommand 1"));
        assert!(script.contains("+@NoPromptForPassword 1"));
        assert!(script.contains("< /dev/null"));
        assert!(script.contains("max_attempts=5"));
        assert!(script.contains("retrying in ${sleep_seconds}s"));
    }

    #[test]
    fn battlegroup_image_patch_uses_rust_built_json_patch_without_jq() {
        let remote = MockRemote::with_outputs([
            "1952287-0-shipping",
            r#"{
              "metadata":{"name":"sh-host-abcdef"},
              "spec":{
                "serverSets":[
                  {"image":"registry.funcom.com/funcom/self-hosting/seabass-server:old"},
                  {"image":"registry.funcom.com/funcom/self-hosting/other:old"}
                ],
                "nested":{"image":"registry.funcom.com/funcom/self-hosting/seabass-server-gateway:old"}
              }
            }"#,
            r#"{"metadata":{"name":"sh-host-abcdef"}}"#,
        ]);
        let scripts = remote.scripts.clone();
        let provider = SshGuestBootstrapProvider::new(remote);
        provider
            .patch_battlegroup_images("funcom-seabass-sh-host-abcdef", "sh-host-abcdef")
            .unwrap();

        let scripts = scripts.borrow();
        assert!(scripts[0].contains("version.txt"));
        assert!(scripts[1].contains("kubectl get battlegroup"));
        assert!(scripts[2].contains("kubectl patch battlegroup"));
        assert!(scripts[2].contains("--type=json"));
        assert!(scripts[2].contains("1952287-0-shipping"));
        assert!(scripts[2].contains("seabass-server-gateway"));
        assert!(!scripts.join("\n").contains("jq"));
    }

    #[test]
    fn rejects_invalid_world_manifest_before_script_execution() {
        let remote = MockRemote::default();
        let scripts = remote.scripts.clone();
        let provider = SshGuestBootstrapProvider::new(remote);
        let result = provider.create_world(&WorldManifestRequest {
            world_name: "Adain".to_string(),
            world_region: "Mars".to_string(),
            world_unique_name: "sh-host-abcdef".to_string(),
            self_host_token: "token".to_string(),
        });

        assert!(result.is_err());
        assert!(scripts.borrow().is_empty());
    }
}
