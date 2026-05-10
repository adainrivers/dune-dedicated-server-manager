use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogQuery {
    pub pod: String,
    pub container: Option<String>,
    pub tail: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct WsQuery {
    pub token: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthResponse {
    pub ok: bool,
    pub namespace: String,
    pub auth_enabled: bool,
    pub director_configured: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusResponse {
    pub namespace: String,
    pub auth_enabled: bool,
    pub director_configured: bool,
    pub battlegroups: usize,
    pub pods: usize,
    pub services: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PodSummary {
    pub name: String,
    pub phase: String,
    pub ready: bool,
    pub restarts: i32,
    pub node_name: Option<String>,
    pub created_at: Option<String>,
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
pub struct BattleGroupSummary {
    pub namespace: String,
    pub name: String,
    pub title: String,
    pub phase: String,
    pub stop: bool,
    pub server_sets: usize,
    pub server_image: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerSetSummary {
    pub map: String,
    pub replicas: u64,
    pub memory_limit: String,
    pub dedicated_scaling: bool,
    pub image: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BattleGroupDetail {
    pub namespace: String,
    pub name: String,
    pub title: String,
    pub phase: String,
    pub stop: bool,
    pub database_phase: String,
    pub server_group_phase: String,
    pub gateway_phase: String,
    pub director_phase: String,
    pub server_image: String,
    pub utility_images: Vec<String>,
    pub server_sets: Vec<ServerSetSummary>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkloadsResponse {
    pub pods: Vec<PodSummary>,
    pub services: Vec<ServiceSummary>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectorCapabilities {
    pub configured: bool,
    pub api_paths: Vec<DirectorPathCapability>,
    pub ui_proxy_path: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectorPathCapability {
    pub method: &'static str,
    pub path: &'static str,
}

#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DirectorPlayerSummary {
    pub active: i64,
    pub online: i64,
    pub in_transit: i64,
    pub grace_period: i64,
    pub completion: i64,
    pub queued: i64,
    pub login_requests_total: i64,
    pub travel_requests_total: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectorMapSummary {
    pub name: String,
    pub kind: String,
    pub players: i64,
    pub online: i64,
    pub queued: i64,
    pub servers: Vec<DirectorServerSummary>,
    pub has_override: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectorServerSummary {
    pub label: String,
    pub server_id: String,
    pub partition_id: Option<i64>,
    pub dimension_index: Option<i64>,
    pub players: i64,
    pub online: i64,
    pub queued: Option<i64>,
    pub status: String,
    pub heartbeat_seconds_ago: Option<i64>,
    pub has_override: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TelemetryEnvelope {
    pub event_type: String,
    pub time_unix_ms: u128,
    pub payload: Value,
}
