use serde::Serialize;
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{
    errors::failure,
    models::CommandResult,
    orchestration::{
        GuestBootstrapProvider, OperationSink, OrchestrationEvent, ProviderKind, StepAction,
        StepDomain, WorldManifestRequest,
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuestBootstrapPlan {
    pub player_ip: String,
    pub world_name: String,
    pub world_region: String,
    pub self_host_token: String,
    pub host_id: String,
    pub world_suffix: String,
}

impl GuestBootstrapPlan {
    pub fn from_self_host_token(
        player_ip: impl Into<String>,
        world_name: impl Into<String>,
        world_region: impl Into<String>,
        self_host_token: impl Into<String>,
    ) -> CommandResult<Self> {
        let token = self_host_token.into();
        Ok(Self {
            player_ip: player_ip.into(),
            world_name: world_name.into(),
            world_region: world_region.into(),
            host_id: host_id_from_self_host_token(&token)?,
            world_suffix: random_lowercase_suffix(),
            self_host_token: token,
        })
    }

    pub fn world_unique_name(&self) -> String {
        format!("sh-{}-{}", self.host_id, self.world_suffix)
    }

    pub fn validate(&self) -> CommandResult<()> {
        validate_ipv4ish(&self.player_ip, "player-facing IP")?;
        validate_world_name(&self.world_name)?;
        validate_region(&self.world_region)?;
        validate_host_id(&self.host_id)?;
        validate_world_suffix(&self.world_suffix)?;
        if self.self_host_token.trim().is_empty()
            || self.self_host_token.contains('\n')
            || self.self_host_token.contains('\r')
        {
            return Err(failure("Self-host token is required"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuestBootstrapResult {
    pub namespace: String,
    pub battlegroup_name: String,
    pub world_unique_name: String,
}

pub struct GuestBootstrapOrchestrator<P> {
    provider: P,
}

impl<P> GuestBootstrapOrchestrator<P>
where
    P: GuestBootstrapProvider,
{
    pub fn new(provider: P) -> Self {
        Self { provider }
    }

    pub fn run(
        &self,
        plan: &GuestBootstrapPlan,
        sink: &mut impl OperationSink,
    ) -> CommandResult<GuestBootstrapResult> {
        plan.validate()?;

        emit(
            sink,
            "guest-settings",
            "Writing player-facing server address.",
            StepDomain::Guest,
            StepAction::Configure,
        );
        // The existing guest provider split still owns the actual settings file write.
        // This bootstrap provider starts at the vendor bootstrap/setup boundary.

        emit(
            sink,
            "guest-disk",
            "Checking guest disk capacity.",
            StepDomain::Guest,
            StepAction::Configure,
        );
        self.provider.validate_and_resize_root_disk()?;

        emit(
            sink,
            "guest-download",
            "Ensuring guest server payload is installed.",
            StepDomain::Steam,
            StepAction::Download,
        );
        self.provider.ensure_server_payload()?;

        emit(
            sink,
            "guest-k3s.start",
            "Starting k3s.",
            StepDomain::Guest,
            StepAction::Start,
        );
        self.provider.start_k3s_and_wait()?;

        emit(
            sink,
            "guest-k3s.import-core-images",
            "Importing k3s prerequisite images.",
            StepDomain::Guest,
            StepAction::Import,
        );
        self.provider.import_core_images()?;

        emit(
            sink,
            "guest-k3s.scale-core",
            "Starting k3s core deployments.",
            StepDomain::Kubernetes,
            StepAction::Configure,
        );
        self.provider.scale_core_deployments()?;

        emit(
            sink,
            "guest-operators.update-crds",
            "Updating operator resources.",
            StepDomain::Kubernetes,
            StepAction::Configure,
        );
        self.provider.update_operator_crds()?;

        emit(
            sink,
            "guest-operators.patch-images",
            "Updating operator images.",
            StepDomain::Kubernetes,
            StepAction::Patch,
        );
        self.provider.patch_operator_images()?;

        emit(
            sink,
            "guest-operators.scale",
            "Starting operator deployments.",
            StepDomain::Kubernetes,
            StepAction::Configure,
        );
        self.provider.scale_operator_deployments()?;

        emit(
            sink,
            "guest-system.install-helper",
            "Installing guest battlegroup helper.",
            StepDomain::Guest,
            StepAction::Configure,
        );
        self.provider.install_battlegroup_helper()?;

        let world_unique_name = plan.world_unique_name();
        emit(
            sink,
            "guest-world.create",
            "Creating battlegroup world resources.",
            StepDomain::Kubernetes,
            StepAction::Create,
        );
        let world = self.provider.create_world(&WorldManifestRequest {
            world_name: plan.world_name.clone(),
            world_region: plan.world_region.clone(),
            world_unique_name: world_unique_name.clone(),
            self_host_token: plan.self_host_token.clone(),
        })?;

        emit(
            sink,
            "guest-images.import",
            "Importing battlegroup images.",
            StepDomain::Guest,
            StepAction::Import,
        );
        self.provider.import_battlegroup_images()?;

        emit(
            sink,
            "guest-images.patch",
            "Patching battlegroup image revisions.",
            StepDomain::Kubernetes,
            StepAction::Patch,
        );
        self.provider
            .patch_battlegroup_images(&world.namespace, &world.battlegroup_name)?;

        emit(
            sink,
            "guest-defaults.apply",
            "Applying default user settings.",
            StepDomain::Kubernetes,
            StepAction::Configure,
        );
        self.provider
            .apply_default_user_settings(&world.namespace, &world.battlegroup_name)?;

        Ok(GuestBootstrapResult {
            namespace: world.namespace,
            battlegroup_name: world.battlegroup_name,
            world_unique_name,
        })
    }
}

pub fn validate_region(value: &str) -> CommandResult<()> {
    match value {
        "Europe Test" | "North America Test" => Ok(()),
        _ => Err(failure("Region must be Europe Test or North America Test")),
    }
}

pub fn validate_world_suffix(value: &str) -> CommandResult<()> {
    if value.len() == 6 && value.bytes().all(|byte| byte.is_ascii_lowercase()) {
        Ok(())
    } else {
        Err(failure(
            "World suffix must be exactly six lowercase letters",
        ))
    }
}

pub fn host_id_from_self_host_token(token: &str) -> CommandResult<String> {
    let payload = token
        .split('.')
        .nth(1)
        .ok_or_else(|| failure("Self-host token must be a JWT-like token"))?;
    let decoded = base64url_decode(payload)?;
    let value: Value = serde_json::from_slice(&decoded)
        .map_err(|err| failure(format!("Self-host token payload was not JSON: {err}")))?;
    let host_id = value["HostId"]
        .as_str()
        .ok_or_else(|| failure("Self-host token did not contain HostId"))?
        .to_ascii_lowercase();
    validate_host_id(&host_id)?;
    Ok(host_id)
}

pub fn random_lowercase_suffix() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(0);
    let mut state = nanos ^ ((std::process::id() as u64) << 32) ^ 0xa5a5_5a5a_d3c7_b901;
    let mut suffix = String::with_capacity(6);
    for _ in 0..6 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        suffix.push((b'a' + (state % 26) as u8) as char);
    }
    suffix
}

fn base64url_decode(value: &str) -> CommandResult<Vec<u8>> {
    let mut bits = 0u32;
    let mut bit_count = 0u8;
    let mut decoded = Vec::new();
    for byte in value.bytes() {
        if byte == b'=' {
            break;
        }
        let next = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'-' => 62,
            b'_' => 63,
            _ => return Err(failure("Self-host token payload is not base64url")),
        };
        bits = (bits << 6) | u32::from(next);
        bit_count += 6;
        while bit_count >= 8 {
            bit_count -= 8;
            decoded.push(((bits >> bit_count) & 0xff) as u8);
        }
    }
    Ok(decoded)
}

pub fn validate_host_id(value: &str) -> CommandResult<()> {
    if !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
    {
        Ok(())
    } else {
        Err(failure(
            "HostId must contain only lowercase letters and digits",
        ))
    }
}

pub fn validate_world_name(value: &str) -> CommandResult<()> {
    let trimmed = value.trim();
    if trimmed.is_empty()
        || trimmed.chars().count() > 50
        || trimmed.contains('\n')
        || trimmed.contains('\r')
    {
        Err(failure(
            "World name must be 1-50 characters and single-line",
        ))
    } else {
        Ok(())
    }
}

fn validate_ipv4ish(value: &str, label: &str) -> CommandResult<()> {
    let parts = value.split('.').collect::<Vec<_>>();
    if parts.len() == 4 && parts.iter().all(|part| part.parse::<u8>().is_ok()) {
        Ok(())
    } else {
        Err(failure(format!("{label} must be an IPv4 address")))
    }
}

fn emit(
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

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use crate::orchestration::{CreatedWorld, VecOperationSink, WorldManifestRequest};

    use super::*;

    #[derive(Default)]
    struct MockGuestBootstrap {
        calls: Rc<RefCell<Vec<&'static str>>>,
    }

    impl GuestBootstrapProvider for MockGuestBootstrap {
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
    fn orchestrates_guest_bootstrap_sequence() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let provider = MockGuestBootstrap {
            calls: calls.clone(),
        };
        let orchestrator = GuestBootstrapOrchestrator::new(provider);
        let mut sink = VecOperationSink::default();
        let result = orchestrator
            .run(
                &GuestBootstrapPlan {
                    player_ip: "10.0.0.4".to_string(),
                    world_name: "Adain".to_string(),
                    world_region: "Europe Test".to_string(),
                    self_host_token: "token".to_string(),
                    host_id: "abc123".to_string(),
                    world_suffix: "abcdef".to_string(),
                },
                &mut sink,
            )
            .unwrap();

        assert_eq!(result.namespace, "funcom-seabass-sh-abc123-abcdef");
        assert_eq!(
            calls.borrow().as_slice(),
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
            .any(|event| event.step_id == "guest-world.create"));
    }

    #[test]
    fn rejects_non_vendor_suffix_shape() {
        assert!(validate_world_suffix("52d16d").is_err());
        assert!(validate_world_suffix("abcdef").is_ok());
    }

    #[test]
    fn builds_plan_from_self_host_token_host_id() {
        let plan = GuestBootstrapPlan::from_self_host_token(
            "10.0.0.4",
            "Adain",
            "Europe Test",
            "e30.eyJIb3N0SWQiOiJBQkMxMjMifQ.sig",
        )
        .unwrap();

        assert_eq!(plan.host_id, "abc123");
        assert_eq!(plan.world_suffix.len(), 6);
        assert!(plan
            .world_suffix
            .bytes()
            .all(|byte| byte.is_ascii_lowercase()));
        assert!(plan.world_unique_name().starts_with("sh-abc123-"));
    }

    #[test]
    fn rejects_token_without_host_id() {
        assert!(host_id_from_self_host_token("e30.e30.sig").is_err());
    }
}
