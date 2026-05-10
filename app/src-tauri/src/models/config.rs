use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
#[serde(default)]
pub struct AppConfig {
    pub install_path: String,
    pub vm_name: String,
    pub vm_ip: String,
    pub ssh_user: String,
    pub ssh_path: String,
    pub manager_api_url: String,
    pub manager_api_token: String,
    pub manager_api_namespace: String,
    pub manager_api_image: String,
    pub manager_api_binary_path: String,
    pub manager_api_director_url: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            install_path: String::new(),
            vm_name: String::new(),
            vm_ip: String::new(),
            ssh_user: String::new(),
            ssh_path: String::new(),
            manager_api_url: String::new(),
            manager_api_token: String::new(),
            manager_api_namespace: String::new(),
            manager_api_image: String::new(),
            manager_api_binary_path: String::new(),
            manager_api_director_url: String::new(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DetectedConfig {
    pub install_path: Option<String>,
    pub vm_name: Option<String>,
    pub vm_ip: Option<String>,
    pub ssh_path: Option<String>,
}
