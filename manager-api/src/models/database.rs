use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "snake_case"))]
pub struct DatabaseWorldPartition {
    pub partition_id: i64,
    pub server_id: Option<String>,
    pub map: String,
    pub partition_definition: String,
    pub dimension_index: i32,
    pub blocked: bool,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseWorldPartitionsResponse {
    pub namespace: String,
    pub rows: Vec<DatabaseWorldPartition>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseWorldPartitionUpdateRequest {
    pub blocked: bool,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseWorldPartitionUpdateResponse {
    pub namespace: String,
    pub row: DatabaseWorldPartition,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "snake_case"))]
pub struct DatabasePlayerSummary {
    pub account_id: i64,
    pub character_name: Option<String>,
    pub online_status: Option<String>,
    pub life_state: Option<String>,
    pub server_id: Option<String>,
    pub player_controller_id: Option<i64>,
    pub player_state_id: Option<i64>,
    pub previous_server_partition_id: Option<i64>,
    pub home_dimension_index: Option<i32>,
    pub last_login_time: Option<String>,
    pub last_avatar_activity: Option<String>,
    pub guild_id: Option<i64>,
    pub guild_name: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabasePlayersResponse {
    pub namespace: String,
    pub rows: Vec<DatabasePlayerSummary>,
}
