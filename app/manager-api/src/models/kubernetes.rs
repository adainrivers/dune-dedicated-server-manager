use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PodSummary {
    pub name: String,
    pub phase: String,
    pub ready: bool,
    pub restarts: i32,
    pub containers: Vec<String>,
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
pub struct WorkloadsResponse {
    pub pods: Vec<PodSummary>,
    pub services: Vec<ServiceSummary>,
}
