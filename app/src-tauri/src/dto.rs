use serde::{Deserialize, Serialize};

fn default_ssh_port() -> u16 {
    22
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteConnectionRequest {
    pub host: String,
    pub key_path: Option<String>,
    pub server_type: Option<String>,
    pub user: Option<String>,
    #[serde(default = "default_ssh_port")]
    pub port: u16,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteServerActionRequest {
    pub server_type: Option<String>,
    pub host: String,
    pub user: String,
    pub key_path: Option<String>,
    #[serde(default = "default_ssh_port")]
    pub port: u16,
    pub namespace: String,
    pub battlegroup_name: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerTunnelStartRequest {
    pub tunnel_id: String,
    pub server_kind: String,
    pub service: String,
    pub host: String,
    pub user: String,
    pub key_path: Option<String>,
    #[serde(default = "default_ssh_port")]
    pub port: u16,
    pub namespace: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerTunnelStopRequest {
    pub tunnel_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomTunnelStartRequest {
    pub tunnel_id: String,
    pub server_kind: String,
    pub host: String,
    pub user: String,
    pub key_path: Option<String>,
    #[serde(default = "default_ssh_port")]
    pub port: u16,
    pub protocol: String,
    pub remote_port: u16,
    pub local_port: u16,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerTunnelStatus {
    pub tunnel_id: String,
    pub service: String,
    pub local_port: u16,
    pub remote_port: u16,
    pub url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteBattlegroupStatus {
    pub stop: bool,
    pub phase: String,
    #[serde(default)]
    pub database_phase: String,
    /// `status.serverGroupPhase`: aggregate phase of the map servers.
    pub server_group_phase: String,
    /// `status.utilities.serverGateway.phase`: the ServerGateway's own health
    /// (e.g. `Healthy`), which is what Funcom's tooling shows as "Gateway".
    #[serde(default)]
    pub gateway_phase: String,
    pub director_phase: String,
    #[serde(default)]
    pub uptime: String,
    #[serde(default)]
    pub server_stats: Vec<RemoteBattlegroupServerStat>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteBattlegroupServerStat {
    pub map: String,
    pub phase: String,
    pub ready: String,
    pub players: String,
    pub age: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteServerStatus {
    pub battlegroup: RemoteBattlegroupStatus,
    pub package: RemoteServerPackageStatus,
    /// Root and k3s storage filesystems; empty when `df` could not be read.
    pub disks: Vec<RemoteDiskUsage>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteDiskUsage {
    pub mount: String,
    pub total_kb: u64,
    pub used_kb: u64,
    pub available_kb: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteServerPackageStatus {
    pub installed_build_id: Option<String>,
    pub battlegroup_version: Option<String>,
    pub live_battlegroup_version: Option<String>,
    pub operator_version: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteServerComponent {
    pub name: String,
    pub log_key: String,
    pub category: String,
    pub state: String,
    pub tone: String,
    pub summary: String,
    pub details: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteComponentLogRequest {
    pub server_type: Option<String>,
    pub host: String,
    pub user: String,
    pub key_path: Option<String>,
    #[serde(default = "default_ssh_port")]
    pub port: u16,
    pub namespace: String,
    pub component: String,
    pub tail: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteComponentLogResult {
    pub component: String,
    pub output: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteComponentRestartRequest {
    pub server_type: Option<String>,
    pub host: String,
    pub user: String,
    pub key_path: Option<String>,
    #[serde(default = "default_ssh_port")]
    pub port: u16,
    pub namespace: String,
    pub component: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteComponentRestartResult {
    pub component: String,
    pub output: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteServerRecord {
    #[serde(rename = "type")]
    pub server_type: String,
    pub id: String,
    pub name: String,
    pub host: String,
    pub user: String,
    pub key_path: String,
    pub port: u16,
    pub namespace: String,
    pub battlegroup_name: String,
    pub world_unique_name: String,
    pub phase: String,
}
