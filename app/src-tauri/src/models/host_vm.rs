use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostStatus {
    pub user: String,
    pub is_elevated: bool,
    pub hyperv_available: bool,
    pub vmms_status: Option<String>,
    pub ssh_available: bool,
    pub default_install_path_exists: bool,
    pub default_install_path: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VmStatus {
    pub name: String,
    pub state: String,
    pub status: String,
    pub memory_assigned_bytes: u64,
    pub uptime: String,
    pub path: String,
    pub configuration_location: String,
    pub ip_addresses: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuestConnection {
    pub ip: String,
    pub ssh_user: String,
    pub key_path: String,
    pub connected: bool,
    pub sudo: bool,
    pub hostname: String,
    pub kernel: String,
    pub kubectl: bool,
}
