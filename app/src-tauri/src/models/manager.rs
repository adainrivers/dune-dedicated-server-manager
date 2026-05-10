use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagerApiInstallResult {
    pub namespace: String,
    pub deployment: String,
    pub service: String,
    pub binary_path: String,
    pub url: String,
}
