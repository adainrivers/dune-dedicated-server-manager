use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthResponse {
    pub ok: bool,
    pub api_version: &'static str,
    pub namespace: String,
    pub auth_enabled: bool,
    pub director_configured: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusResponse {
    pub api_version: &'static str,
    pub namespace: String,
    pub auth_enabled: bool,
    pub director_configured: bool,
    pub battlegroups: usize,
    pub pods: usize,
    pub services: usize,
}
