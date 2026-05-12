use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PodSummary {
    pub name: String,
    pub phase: String,
    pub ready: bool,
    pub restarts: i32,
    pub containers: Vec<String>,
    pub container_resources: Vec<ContainerResourceSummary>,
    pub node_name: Option<String>,
    pub created_at: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContainerResourceSummary {
    pub name: String,
    pub image: Option<String>,
    pub cpu_request: Option<String>,
    pub cpu_limit: Option<String>,
    pub memory_request: Option<String>,
    pub memory_limit: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServicePortSummary {
    pub name: Option<String>,
    pub port: i32,
    pub target_port: Option<String>,
    pub node_port: Option<i32>,
    pub protocol: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceSummary {
    pub name: String,
    pub service_type: Option<String>,
    pub cluster_ip: Option<String>,
    pub external_ips: Vec<String>,
    pub ports: Vec<ServicePortSummary>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkloadsResponse {
    pub pods: Vec<PodSummary>,
    pub services: Vec<ServiceSummary>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventSummary {
    pub name: String,
    pub event_type: String,
    pub reason: String,
    pub message: String,
    pub involved_kind: String,
    pub involved_name: String,
    pub count: i32,
    pub first_seen: Option<String>,
    pub last_seen: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventsResponse {
    pub namespace: String,
    pub events: Vec<EventSummary>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistentVolumeClaimSummary {
    pub name: String,
    pub phase: String,
    pub requested_storage: Option<String>,
    pub capacity_storage: Option<String>,
    pub storage_class: Option<String>,
    pub volume_name: Option<String>,
    pub access_modes: Vec<String>,
    pub created_at: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageResponse {
    pub namespace: String,
    pub claims: Vec<PersistentVolumeClaimSummary>,
}
