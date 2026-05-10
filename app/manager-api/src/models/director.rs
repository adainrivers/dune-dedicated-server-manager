use serde::Serialize;

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

#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DirectorPlayerLists {
    pub all: Vec<String>,
    pub online: Vec<String>,
    pub in_transit: Vec<String>,
    pub grace_period: Vec<String>,
    pub completion: Vec<String>,
    pub queued: Vec<String>,
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
