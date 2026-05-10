use anyhow::{Context, Result};
use k8s_openapi::{
    api::core::v1::{Pod, Service},
    apimachinery::pkg::util::intstr::IntOrString,
};
use kube::{
    api::{ApiResource, DynamicObject, ListParams, Patch, PatchParams},
    Api,
};
use serde_json::{json, Value};

use crate::{
    errors::ApiError,
    models::{
        BattleGroupDetail, BattleGroupSummary, PodSummary, ServerSetSummary, ServicePortSummary,
        ServiceSummary,
    },
    state::AppState,
    validation::{validate_kube_name, validate_namespace},
};

pub async fn list_pods(state: &AppState) -> Result<Vec<PodSummary>> {
    let pods: Api<Pod> = Api::namespaced(state.client.clone(), &state.namespace);
    let list = pods
        .list(&ListParams::default())
        .await
        .context("failed to list pods")?;

    Ok(list
        .items
        .into_iter()
        .map(|pod| {
            let status = pod.status.unwrap_or_default();
            let containers = pod
                .spec
                .as_ref()
                .map(|spec| {
                    spec.containers
                        .iter()
                        .map(|container| container.name.clone())
                        .collect()
                })
                .unwrap_or_default();
            let container_statuses = status.container_statuses.unwrap_or_default();
            PodSummary {
                name: pod.metadata.name.unwrap_or_default(),
                phase: status.phase.unwrap_or_default(),
                ready: !container_statuses.is_empty()
                    && container_statuses.iter().all(|container| container.ready),
                restarts: container_statuses
                    .iter()
                    .map(|container| container.restart_count)
                    .sum(),
                containers,
                node_name: pod.spec.and_then(|spec| spec.node_name),
                created_at: pod
                    .metadata
                    .creation_timestamp
                    .map(|time| time.0.to_rfc3339()),
            }
        })
        .collect())
}

pub async fn list_services(state: &AppState) -> Result<Vec<ServiceSummary>> {
    let services: Api<Service> = Api::namespaced(state.client.clone(), &state.namespace);
    let list = services
        .list(&ListParams::default())
        .await
        .context("failed to list services")?;

    Ok(list
        .items
        .into_iter()
        .map(|service| {
            let spec = service.spec.unwrap_or_default();
            ServiceSummary {
                name: service.metadata.name.unwrap_or_default(),
                service_type: spec.type_,
                cluster_ip: spec.cluster_ip,
                external_ips: spec.external_ips.unwrap_or_default(),
                ports: spec
                    .ports
                    .unwrap_or_default()
                    .into_iter()
                    .map(|port| ServicePortSummary {
                        name: port.name,
                        port: port.port,
                        target_port: port.target_port.map(int_or_string_to_string),
                        node_port: port.node_port,
                        protocol: port.protocol,
                    })
                    .collect(),
            }
        })
        .collect())
}

pub async fn list_battlegroups(state: &AppState) -> Result<Vec<BattleGroupSummary>> {
    let api: Api<DynamicObject> = Api::namespaced_with(
        state.client.clone(),
        &state.namespace,
        &battlegroup_resource(),
    );
    let list = api
        .list(&ListParams::default())
        .await
        .context("failed to list battlegroups")?;

    Ok(list
        .items
        .into_iter()
        .map(|item| battlegroup_summary(&state.namespace, item))
        .collect())
}

pub async fn get_battlegroup_object(state: &AppState, name: &str) -> Result<DynamicObject> {
    let api: Api<DynamicObject> = Api::namespaced_with(
        state.client.clone(),
        &state.namespace,
        &battlegroup_resource(),
    );
    api.get(name)
        .await
        .with_context(|| format!("failed to get battlegroup {name}"))
}

pub async fn patch_battlegroup_stop(
    state: &AppState,
    namespace: &str,
    name: &str,
    stop: bool,
) -> Result<(), ApiError> {
    validate_namespace(state, namespace)?;
    validate_kube_name(name)?;
    let api: Api<DynamicObject> = Api::namespaced_with(
        state.client.clone(),
        &state.namespace,
        &battlegroup_resource(),
    );
    api.patch(
        name,
        &PatchParams::default(),
        &Patch::Merge(json!({ "spec": { "stop": stop } })),
    )
    .await
    .with_context(|| format!("failed to patch battlegroup {name}"))?;
    Ok(())
}

pub fn battlegroup_detail_from_object(
    default_namespace: &str,
    item: DynamicObject,
) -> BattleGroupDetail {
    let namespace = item
        .metadata
        .namespace
        .unwrap_or_else(|| default_namespace.to_string());
    let name = item.metadata.name.unwrap_or_default();
    let data = serde_json::to_value(item.data).unwrap_or_else(|_| json!({}));
    let server_sets = summarize_server_sets(&data);
    let server_image = server_sets
        .first()
        .map(|set| set.image.clone())
        .unwrap_or_default();
    let mut utility_images = Vec::new();

    for path in [
        &data["spec"]["utilities"]["director"]["spec"]["image"],
        &data["spec"]["utilities"]["serverGateway"]["spec"]["image"],
        &data["spec"]["utilities"]["textRouter"]["spec"]["image"],
        &data["spec"]["utilities"]["fileBrowser"]["spec"]["image"],
    ] {
        if let Some(image) = path.as_str() {
            utility_images.push(image.to_string());
        }
    }

    for template in data["spec"]["utilities"]["messageQueues"]["templates"]
        .as_array()
        .cloned()
        .unwrap_or_default()
    {
        if let Some(image) = template["spec"]["image"].as_str() {
            utility_images.push(image.to_string());
        }
    }

    BattleGroupDetail {
        namespace,
        name,
        title: data["spec"]["title"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        phase: data["status"]["phase"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        stop: data["spec"]["stop"].as_bool().unwrap_or(false),
        database_phase: data["status"]["database"]["phase"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        server_group_phase: data["status"]["serverGroup"]["phase"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        gateway_phase: data["status"]["serverGateway"]["phase"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        director_phase: data["status"]["director"]["phase"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        server_image,
        utility_images: unique_strings(utility_images.into_iter()),
        server_sets,
    }
}

fn battlegroup_resource() -> ApiResource {
    ApiResource {
        group: "igw.funcom.com".to_string(),
        version: "v1".to_string(),
        api_version: "igw.funcom.com/v1".to_string(),
        kind: "BattleGroup".to_string(),
        plural: "battlegroups".to_string(),
    }
}

fn int_or_string_to_string(value: IntOrString) -> String {
    match value {
        IntOrString::Int(value) => value.to_string(),
        IntOrString::String(value) => value,
    }
}

fn battlegroup_summary(default_namespace: &str, item: DynamicObject) -> BattleGroupSummary {
    let namespace = item
        .metadata
        .namespace
        .unwrap_or_else(|| default_namespace.to_string());
    let name = item.metadata.name.unwrap_or_default();
    let data = serde_json::to_value(item.data).unwrap_or_else(|_| json!({}));
    let sets = data["spec"]["serverGroup"]["template"]["spec"]["sets"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let server_image = sets
        .first()
        .and_then(|set| set["image"].as_str())
        .unwrap_or_default()
        .to_string();

    BattleGroupSummary {
        namespace,
        name,
        title: data["spec"]["title"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        phase: data["status"]["phase"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        stop: data["spec"]["stop"].as_bool().unwrap_or(false),
        server_sets: sets.len(),
        server_image,
    }
}

fn summarize_server_sets(data: &Value) -> Vec<ServerSetSummary> {
    data["spec"]["serverGroup"]["template"]["spec"]["sets"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|set| ServerSetSummary {
            map: set["map"].as_str().unwrap_or_default().to_string(),
            replicas: set["replicas"].as_u64().unwrap_or_default(),
            memory_limit: set["resources"]["limits"]["memory"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            dedicated_scaling: set["dedicatedScaling"].as_bool().unwrap_or(false),
            image: set["image"].as_str().unwrap_or_default().to_string(),
        })
        .collect()
}

fn unique_strings(values: impl Iterator<Item = String>) -> Vec<String> {
    let mut output = Vec::new();
    for value in values {
        if !value.is_empty() && !output.contains(&value) {
            output.push(value);
        }
    }
    output
}
