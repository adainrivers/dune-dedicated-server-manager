use anyhow::{Context, Result};
use k8s_openapi::{
    api::core::v1::{Container, Event, PersistentVolumeClaim, Pod, Service},
    apimachinery::pkg::util::intstr::IntOrString,
};
use kube::{
    api::{ApiResource, DynamicObject, ListParams, Patch, PatchParams},
    Api,
};
use serde_json::{json, Value};
use std::collections::HashSet;

use crate::{
    config_files_domain::{read_deep_desert_pvp_partition_ids, write_deep_desert_pvp_settings},
    errors::ApiError,
    models::{
        BattleGroupDetail, BattleGroupSummary, ContainerResourceSummary, EventSummary,
        PersistentVolumeClaimSummary, PodSummary, ServerSetSummary, ServicePortSummary,
        ServiceSummary, WorldLayout, WorldLayoutUpdateRequest, WorldLayoutUpdateResponse,
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

    Ok(list.items.into_iter().map(pod_summary).collect())
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

pub async fn list_events(state: &AppState, limit: usize) -> Result<Vec<EventSummary>> {
    let events: Api<Event> = Api::namespaced(state.client.clone(), &state.namespace);
    let list = events
        .list(&ListParams::default())
        .await
        .context("failed to list events")?;

    let mut events = list
        .items
        .into_iter()
        .map(event_summary)
        .collect::<Vec<_>>();
    events.sort_by(|left, right| {
        event_sort_key(right)
            .cmp(&event_sort_key(left))
            .then_with(|| right.name.cmp(&left.name))
    });
    events.truncate(limit);
    Ok(events)
}

pub async fn list_persistent_volume_claims(
    state: &AppState,
) -> Result<Vec<PersistentVolumeClaimSummary>> {
    let claims: Api<PersistentVolumeClaim> =
        Api::namespaced(state.client.clone(), &state.namespace);
    let list = claims
        .list(&ListParams::default())
        .await
        .context("failed to list persistent volume claims")?;

    let mut claims = list
        .items
        .into_iter()
        .map(persistent_volume_claim_summary)
        .collect::<Vec<_>>();
    claims.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(claims)
}

fn pod_summary(pod: Pod) -> PodSummary {
    let status = pod.status.unwrap_or_default();
    let spec = pod.spec;
    let containers = spec
        .as_ref()
        .map(|spec| {
            spec.containers
                .iter()
                .map(|container| container.name.clone())
                .collect()
        })
        .unwrap_or_default();
    let container_resources = spec
        .as_ref()
        .map(|spec| spec.containers.iter().map(container_resources).collect())
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
        container_resources,
        node_name: spec.and_then(|spec| spec.node_name),
        created_at: pod
            .metadata
            .creation_timestamp
            .map(|time| time.0.to_rfc3339()),
    }
}

fn persistent_volume_claim_summary(claim: PersistentVolumeClaim) -> PersistentVolumeClaimSummary {
    let spec = claim.spec;
    let status = claim.status;
    let requested_storage = spec
        .as_ref()
        .and_then(|spec| spec.resources.as_ref())
        .and_then(|resources| resources.requests.as_ref())
        .and_then(|requests| requests.get("storage"))
        .map(|value| value.0.clone());
    let capacity_storage = status
        .as_ref()
        .and_then(|status| status.capacity.as_ref())
        .and_then(|capacity| capacity.get("storage"))
        .map(|value| value.0.clone());

    PersistentVolumeClaimSummary {
        name: claim.metadata.name.unwrap_or_default(),
        phase: status
            .as_ref()
            .and_then(|status| status.phase.clone())
            .unwrap_or_default(),
        requested_storage,
        capacity_storage,
        storage_class: spec
            .as_ref()
            .and_then(|spec| spec.storage_class_name.clone()),
        volume_name: spec.as_ref().and_then(|spec| spec.volume_name.clone()),
        access_modes: spec
            .as_ref()
            .and_then(|spec| spec.access_modes.clone())
            .unwrap_or_default(),
        created_at: claim
            .metadata
            .creation_timestamp
            .map(|time| time.0.to_rfc3339()),
    }
}

fn container_resources(container: &Container) -> ContainerResourceSummary {
    let requests = container
        .resources
        .as_ref()
        .and_then(|resources| resources.requests.as_ref());
    let limits = container
        .resources
        .as_ref()
        .and_then(|resources| resources.limits.as_ref());
    ContainerResourceSummary {
        name: container.name.clone(),
        image: container.image.clone(),
        cpu_request: requests
            .and_then(|items| items.get("cpu"))
            .map(|value| value.0.clone()),
        cpu_limit: limits
            .and_then(|items| items.get("cpu"))
            .map(|value| value.0.clone()),
        memory_request: requests
            .and_then(|items| items.get("memory"))
            .map(|value| value.0.clone()),
        memory_limit: limits
            .and_then(|items| items.get("memory"))
            .map(|value| value.0.clone()),
    }
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

pub async fn patch_battlegroup_title(
    state: &AppState,
    namespace: &str,
    name: &str,
    title: &str,
) -> Result<BattleGroupDetail, ApiError> {
    validate_namespace(state, namespace)?;
    validate_kube_name(name)?;
    let trimmed = title.trim();
    if trimmed.is_empty() || trimmed.len() > 80 {
        return Err(ApiError::bad_request(
            "server display name must be between 1 and 80 characters",
        ));
    }
    let api: Api<DynamicObject> = Api::namespaced_with(
        state.client.clone(),
        &state.namespace,
        &battlegroup_resource(),
    );
    let item = api
        .patch(
            name,
            &PatchParams::default(),
            &Patch::Merge(json!({ "spec": { "title": trimmed } })),
        )
        .await
        .with_context(|| format!("failed to patch battlegroup title {name}"))?;
    Ok(battlegroup_detail_from_object(&state.namespace, item))
}

pub async fn get_battlegroup_layout(
    state: &AppState,
    namespace: &str,
    name: &str,
) -> Result<WorldLayout, ApiError> {
    validate_namespace(state, namespace)?;
    validate_kube_name(name)?;
    let item = get_battlegroup_object(state, name).await?;
    let mut layout = world_layout_from_object(item, false, Vec::new());
    match read_deep_desert_pvp_partition_ids(state).await {
        Ok(ids) => apply_deep_desert_pvp_ids(&mut layout, &ids),
        Err(_) => layout.warnings.push(
            "Deep Desert PvP config could not be read from the filebrowser volume.".to_string(),
        ),
    }
    Ok(layout)
}

pub async fn patch_battlegroup_layout(
    state: &AppState,
    namespace: &str,
    name: &str,
    request: WorldLayoutUpdateRequest,
) -> Result<WorldLayoutUpdateResponse, ApiError> {
    validate_namespace(state, namespace)?;
    validate_kube_name(name)?;
    validate_world_layout_update(&request)?;
    let pve = request.deep_desert_pve_instances.unwrap_or(0);
    let pvp = request.deep_desert_pvp_instances.unwrap_or(0);
    let deep_desert_total = pve + pvp;

    let item = get_battlegroup_object(state, name).await?;
    let data = serde_json::to_value(item.data.clone()).unwrap_or_else(|_| json!({}));
    let mut operations = Vec::new();
    if let Some(count) = request.hagga_basin_instances {
        append_partition_patch(&data, "Survival_1", count, &mut operations)?;
    }
    if deep_desert_total > 0
        || request.deep_desert_pve_instances.is_some()
        || request.deep_desert_pvp_instances.is_some()
    {
        append_partition_patch(&data, "DeepDesert_1", deep_desert_total, &mut operations)?;
    }
    if let Some(enabled) = request.social_hubs_enabled {
        append_social_hubs_patch(&data, enabled, &mut operations)?;
    }

    let mut battlegroup_patched = false;
    let mut patched = item;
    if !operations.is_empty() {
        let patch: json_patch::Patch = serde_json::from_value(Value::Array(operations))
            .context("failed to build JSON patch for battlegroup layout")?;
        let api: Api<DynamicObject> = Api::namespaced_with(
            state.client.clone(),
            &state.namespace,
            &battlegroup_resource(),
        );
        patched = api
            .patch(name, &PatchParams::default(), &Patch::<()>::Json(patch))
            .await
            .with_context(|| format!("failed to patch battlegroup layout {name}"))?;
        battlegroup_patched = true;
    }

    let warnings = Vec::new();
    let mut pvp_config_updated = false;
    let mut selected_pvp_ids = Vec::new();
    if request.deep_desert_pvp_instances.is_some() {
        let patched_data = serde_json::to_value(patched.data.clone()).unwrap_or_else(|_| json!({}));
        let deep_desert_ids = partition_ids_for_map(&patched_data, "DeepDesert_1");
        selected_pvp_ids = deep_desert_pvp_ids(&deep_desert_ids, pvp);
        write_deep_desert_pvp_settings(state, &selected_pvp_ids).await?;
        pvp_config_updated = true;
    }
    let mut layout = world_layout_from_object(patched, battlegroup_patched, warnings.clone());
    if pvp_config_updated {
        apply_deep_desert_pvp_ids(&mut layout, &selected_pvp_ids);
        layout.restart_required = true;
    }
    Ok(WorldLayoutUpdateResponse {
        restart_required: layout.restart_required,
        layout,
        battlegroup_patched,
        pvp_config_updated,
        warnings,
    })
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
        database_phase: string_at_paths(
            &data,
            &[
                &["status", "database", "phase"],
                &["status", "databasePhase"],
            ],
        ),
        server_group_phase: string_at_paths(
            &data,
            &[
                &["status", "serverGroup", "phase"],
                &["status", "serverGroupPhase"],
            ],
        ),
        gateway_phase: string_at_paths(
            &data,
            &[
                &["status", "serverGateway", "phase"],
                &["status", "utilities", "serverGateway", "phase"],
            ],
        ),
        director_phase: string_at_paths(
            &data,
            &[
                &["status", "director", "phase"],
                &["status", "utilities", "director", "phase"],
            ],
        ),
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

fn event_summary(event: Event) -> EventSummary {
    let involved = event.involved_object;
    EventSummary {
        name: event.metadata.name.unwrap_or_default(),
        event_type: event.type_.unwrap_or_default(),
        reason: event.reason.unwrap_or_default(),
        message: event.message.unwrap_or_default(),
        involved_kind: involved.kind.unwrap_or_default(),
        involved_name: involved.name.unwrap_or_default(),
        count: event.count.unwrap_or(1),
        first_seen: event.first_timestamp.map(|time| time.0.to_rfc3339()),
        last_seen: event
            .last_timestamp
            .map(|time| time.0.to_rfc3339())
            .or_else(|| event.event_time.map(|time| time.0.to_rfc3339())),
    }
}

fn event_sort_key(event: &EventSummary) -> String {
    event
        .last_seen
        .as_deref()
        .or(event.first_seen.as_deref())
        .unwrap_or_default()
        .to_string()
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

fn world_layout_from_object(
    item: DynamicObject,
    restart_required: bool,
    warnings: Vec<String>,
) -> WorldLayout {
    let data = serde_json::to_value(item.data).unwrap_or_else(|_| json!({}));
    let survival_ids = partition_ids_for_map(&data, "Survival_1");
    let deep_desert_ids = partition_ids_for_map(&data, "DeepDesert_1");
    let server_sets = summarize_server_sets(&data);
    let social_hubs_enabled = server_sets
        .iter()
        .filter(|set| is_social_hub_map(&set.map))
        .any(|set| set.replicas > 0);

    WorldLayout {
        hagga_basin_instances: survival_ids.len().max(1),
        social_hubs_enabled,
        deep_desert_pve_instances: deep_desert_ids.len(),
        deep_desert_pvp_instances: 0,
        deep_desert_total_instances: deep_desert_ids.len(),
        deep_desert_partition_ids: deep_desert_ids,
        restart_required,
        warnings,
    }
}

fn apply_deep_desert_pvp_ids(layout: &mut WorldLayout, pvp_partition_ids: &[i64]) {
    let selected = pvp_partition_ids.iter().copied().collect::<HashSet<_>>();
    let pvp_count = layout
        .deep_desert_partition_ids
        .iter()
        .filter(|id| selected.contains(id))
        .count();
    layout.deep_desert_pvp_instances = pvp_count;
    layout.deep_desert_pve_instances = layout.deep_desert_total_instances.saturating_sub(pvp_count);
}

fn deep_desert_pvp_ids(partition_ids: &[i64], pvp_instance_count: usize) -> Vec<i64> {
    if pvp_instance_count == 0 {
        return Vec::new();
    }
    partition_ids
        .iter()
        .rev()
        .take(pvp_instance_count)
        .copied()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

fn append_social_hubs_patch(
    data: &Value,
    enabled: bool,
    operations: &mut Vec<Value>,
) -> Result<(), ApiError> {
    let sets = data["spec"]["serverGroup"]["template"]["spec"]["sets"]
        .as_array()
        .ok_or_else(|| ApiError::bad_request("BattleGroup server sets are not an array"))?;
    let desired_replicas = if enabled { 1 } else { 0 };
    let mut matched = 0;

    for (index, set) in sets.iter().enumerate() {
        let Some(map) = set["map"].as_str() else {
            continue;
        };
        if !is_social_hub_map(map) {
            continue;
        }
        matched += 1;
        if set["replicas"].as_u64().unwrap_or_default() != desired_replicas {
            operations.push(json!({
                "op": "replace",
                "path": format!("/spec/serverGroup/template/spec/sets/{index}/replicas"),
                "value": desired_replicas,
            }));
        }
    }

    if matched == 0 {
        return Err(ApiError::bad_request(
            "BattleGroup has no Social Hub server sets",
        ));
    }
    Ok(())
}

fn is_social_hub_map(map: &str) -> bool {
    matches!(map, "SH_Arrakeen" | "SH_HarkoVillage")
}

fn append_partition_patch(
    data: &Value,
    map_name: &str,
    count: usize,
    operations: &mut Vec<Value>,
) -> Result<(), ApiError> {
    let world_partitions = data["spec"]["database"]["template"]["spec"]["deployment"]["spec"]
        ["worldPartitions"]
        .as_array()
        .ok_or_else(|| ApiError::bad_request("BattleGroup worldPartitions is not an array"))?;
    let map_index = world_partitions
        .iter()
        .position(|item| item["map"].as_str() == Some(map_name))
        .ok_or_else(|| ApiError::bad_request(format!("BattleGroup has no {map_name} entry")))?;
    let current = world_partitions[map_index]["partitions"]
        .as_array()
        .ok_or_else(|| ApiError::bad_request(format!("{map_name} partitions is not an array")))?;
    if current.is_empty() && count > 0 {
        return Err(ApiError::bad_request(format!(
            "{map_name} has no template partition to clone"
        )));
    }
    let mut desired = current.clone();
    desired.sort_by_key(|item| {
        (
            item["dimension"].as_i64().unwrap_or(i64::MAX),
            item["id"].as_i64().unwrap_or(i64::MAX),
        )
    });
    let used_ids = collect_partition_ids(world_partitions);
    while desired.len() < count {
        let dimension = if map_name == "DeepDesert_1" {
            0
        } else {
            desired
                .iter()
                .filter_map(|item| item["dimension"].as_i64())
                .max()
                .unwrap_or(-1)
                + 1
        };
        let id = next_free_partition_id(&used_ids, &desired)?;
        let mut next = desired[0].clone();
        next["id"] = json!(id);
        next["dimension"] = json!(dimension);
        next["disable"] = json!(false);
        desired.push(next);
    }
    desired.truncate(count);
    if desired != *current {
        operations.push(json!({
            "op": "replace",
            "path": format!("/spec/database/template/spec/deployment/spec/worldPartitions/{map_index}/partitions"),
            "value": desired,
        }));
    }
    Ok(())
}

fn partition_ids_for_map(data: &Value, map_name: &str) -> Vec<i64> {
    data["spec"]["database"]["template"]["spec"]["deployment"]["spec"]["worldPartitions"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|item| item["map"].as_str() == Some(map_name))
        .and_then(|item| item["partitions"].as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item["id"].as_i64())
                .collect()
        })
        .unwrap_or_default()
}

fn validate_instance_count(count: usize, label: &str) -> Result<(), ApiError> {
    if count == 0 || count > 64 {
        Err(ApiError::bad_request(format!(
            "{label} instance count must be between 1 and 64"
        )))
    } else {
        Ok(())
    }
}

fn validate_world_layout_update(request: &WorldLayoutUpdateRequest) -> Result<(), ApiError> {
    let pve = request.deep_desert_pve_instances.unwrap_or(0);
    let pvp = request.deep_desert_pvp_instances.unwrap_or(0);
    let deep_desert_total = pve + pvp;
    if let Some(count) = request.hagga_basin_instances {
        validate_instance_count(count, "Hagga Basin")?;
    }
    if deep_desert_total > 0 {
        validate_instance_count(deep_desert_total, "Deep Desert")?;
    }
    if deep_desert_total > 1 {
        return Err(ApiError::bad_request(
            "Only one Deep Desert instance is supported in this build",
        ));
    }
    Ok(())
}

fn collect_partition_ids(world_partitions: &[Value]) -> Vec<i64> {
    world_partitions
        .iter()
        .flat_map(|entry| entry["partitions"].as_array().into_iter().flatten())
        .filter_map(|partition| partition["id"].as_i64())
        .collect()
}

fn next_free_partition_id(existing: &[i64], desired: &[Value]) -> Result<i64, ApiError> {
    let mut used = existing.to_vec();
    used.extend(desired.iter().filter_map(|item| item["id"].as_i64()));
    used.into_iter()
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(|| ApiError::bad_request("No free partition ID is available"))
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

fn string_at_paths(data: &Value, paths: &[&[&str]]) -> String {
    for path in paths {
        let mut current = data;
        for key in *path {
            current = &current[*key];
        }
        if let Some(value) = current.as_str().filter(|value| !value.is_empty()) {
            return value.to_string();
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use k8s_openapi::{
        api::core::v1::{
            ContainerStatus, PersistentVolumeClaim, PersistentVolumeClaimSpec,
            PersistentVolumeClaimStatus, PodSpec, PodStatus, ResourceRequirements,
            VolumeResourceRequirements,
        },
        apimachinery::pkg::{api::resource::Quantity, apis::meta::v1::ObjectMeta},
    };
    use std::collections::BTreeMap;

    fn sample_battlegroup_data() -> Value {
        json!({
            "spec": {
                "serverGroup": {
                    "template": {
                        "spec": {
                            "sets": [
                                { "map": "Survival_1", "replicas": 1 },
                                { "map": "SH_Arrakeen", "replicas": 0 },
                                { "map": "SH_HarkoVillage", "replicas": 0 }
                            ]
                        }
                    }
                },
                "database": {
                    "template": {
                        "spec": {
                            "deployment": {
                                "spec": {
                                    "worldPartitions": [
                                        { "map": "Survival_1", "partitions": [{ "id": 1, "dimension": 0, "disable": false }] },
                                        { "map": "DeepDesert_1", "partitions": [{ "id": 8, "dimension": 0, "disable": false }] }
                                    ]
                                }
                            }
                        }
                    }
                }
            }
        })
    }

    #[test]
    fn summarizes_pod_container_resources() {
        let mut requests = BTreeMap::new();
        requests.insert("memory".to_string(), Quantity("256Mi".to_string()));
        requests.insert("cpu".to_string(), Quantity("250m".to_string()));
        let mut limits = BTreeMap::new();
        limits.insert("memory".to_string(), Quantity("512Mi".to_string()));
        limits.insert("cpu".to_string(), Quantity("500m".to_string()));
        let pod = Pod {
            metadata: ObjectMeta {
                name: Some("runtime-pod".to_string()),
                ..ObjectMeta::default()
            },
            spec: Some(PodSpec {
                node_name: Some("node-a".to_string()),
                containers: vec![Container {
                    name: "server".to_string(),
                    image: Some("registry.example/server:v1".to_string()),
                    resources: Some(ResourceRequirements {
                        requests: Some(requests),
                        limits: Some(limits),
                        ..ResourceRequirements::default()
                    }),
                    ..Container::default()
                }],
                ..PodSpec::default()
            }),
            status: Some(PodStatus {
                phase: Some("Running".to_string()),
                container_statuses: Some(vec![ContainerStatus {
                    name: "server".to_string(),
                    ready: true,
                    restart_count: 2,
                    image: "registry.example/server:v1".to_string(),
                    image_id: "image-id".to_string(),
                    ..ContainerStatus::default()
                }]),
                ..PodStatus::default()
            }),
        };

        let summary = pod_summary(pod);

        assert_eq!(summary.name, "runtime-pod");
        assert!(summary.ready);
        assert_eq!(summary.restarts, 2);
        assert_eq!(
            summary.container_resources[0].memory_request.as_deref(),
            Some("256Mi")
        );
        assert_eq!(
            summary.container_resources[0].memory_limit.as_deref(),
            Some("512Mi")
        );
        assert_eq!(
            summary.container_resources[0].cpu_limit.as_deref(),
            Some("500m")
        );
    }

    #[test]
    fn summarizes_persistent_volume_claims() {
        let mut requests = BTreeMap::new();
        requests.insert("storage".to_string(), Quantity("100Gi".to_string()));
        let mut capacity = BTreeMap::new();
        capacity.insert("storage".to_string(), Quantity("100Gi".to_string()));
        let claim = PersistentVolumeClaim {
            metadata: ObjectMeta {
                name: Some("database-data".to_string()),
                ..ObjectMeta::default()
            },
            spec: Some(PersistentVolumeClaimSpec {
                access_modes: Some(vec!["ReadWriteOnce".to_string()]),
                resources: Some(VolumeResourceRequirements {
                    requests: Some(requests),
                    ..VolumeResourceRequirements::default()
                }),
                storage_class_name: Some("local-path".to_string()),
                volume_name: Some("pvc-volume".to_string()),
                ..PersistentVolumeClaimSpec::default()
            }),
            status: Some(PersistentVolumeClaimStatus {
                capacity: Some(capacity),
                phase: Some("Bound".to_string()),
                ..PersistentVolumeClaimStatus::default()
            }),
        };

        let summary = persistent_volume_claim_summary(claim);

        assert_eq!(summary.name, "database-data");
        assert_eq!(summary.phase, "Bound");
        assert_eq!(summary.requested_storage.as_deref(), Some("100Gi"));
        assert_eq!(summary.capacity_storage.as_deref(), Some("100Gi"));
        assert_eq!(summary.storage_class.as_deref(), Some("local-path"));
        assert_eq!(summary.access_modes, vec!["ReadWriteOnce"]);
    }

    #[test]
    fn rejects_multiple_deep_desert_instances() {
        let request = WorldLayoutUpdateRequest {
            hagga_basin_instances: None,
            social_hubs_enabled: None,
            deep_desert_pve_instances: Some(1),
            deep_desert_pvp_instances: Some(1),
        };

        let err = validate_world_layout_update(&request).unwrap_err();
        assert!(err.message.contains("Only one Deep Desert"));
    }

    #[test]
    fn builds_social_hub_replica_patch() {
        let data = sample_battlegroup_data();
        let mut operations = Vec::new();
        append_social_hubs_patch(&data, true, &mut operations).unwrap();

        assert_eq!(operations.len(), 2);
        assert_eq!(
            operations[0]["path"],
            "/spec/serverGroup/template/spec/sets/1/replicas"
        );
        assert_eq!(operations[0]["value"], 1);
        assert_eq!(
            operations[1]["path"],
            "/spec/serverGroup/template/spec/sets/2/replicas"
        );
    }

    #[test]
    fn builds_empty_deep_desert_partition_patch_when_disabled() {
        let data = sample_battlegroup_data();
        let mut operations = Vec::new();
        append_partition_patch(&data, "DeepDesert_1", 0, &mut operations).unwrap();

        assert_eq!(operations.len(), 1);
        assert_eq!(
            operations[0]["path"],
            "/spec/database/template/spec/deployment/spec/worldPartitions/1/partitions"
        );
        assert_eq!(operations[0]["value"], json!([]));
    }

    #[test]
    fn detects_social_hubs_only_when_replicas_are_enabled() {
        let object = DynamicObject {
            types: None,
            metadata: Default::default(),
            data: sample_battlegroup_data(),
        };

        let layout = world_layout_from_object(object, false, Vec::new());
        assert!(!layout.social_hubs_enabled);
    }

    #[test]
    fn derives_deep_desert_pvp_ids_from_selected_count() {
        assert_eq!(deep_desert_pvp_ids(&[8], 0), Vec::<i64>::new());
        assert_eq!(deep_desert_pvp_ids(&[8], 1), vec![8]);
        assert_eq!(deep_desert_pvp_ids(&[8, 29], 1), vec![29]);
    }

    #[test]
    fn applies_deep_desert_pvp_ids_to_layout() {
        let object = DynamicObject {
            types: None,
            metadata: Default::default(),
            data: sample_battlegroup_data(),
        };
        let mut layout = world_layout_from_object(object, false, Vec::new());

        apply_deep_desert_pvp_ids(&mut layout, &[8]);

        assert_eq!(layout.deep_desert_pve_instances, 0);
        assert_eq!(layout.deep_desert_pvp_instances, 1);
    }
}
