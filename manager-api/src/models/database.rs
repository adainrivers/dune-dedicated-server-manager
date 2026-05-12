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

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabasePlayerTagRequest {
    pub tag: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "snake_case"))]
pub struct DatabasePlayerTagsUpdate {
    pub account_id: i64,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabasePlayerTagsUpdateResponse {
    pub namespace: String,
    pub result: DatabasePlayerTagsUpdate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "snake_case"))]
pub struct DatabaseNamedCount {
    pub name: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "snake_case"))]
pub struct DatabaseRecentPlayer {
    pub account_id: i64,
    pub character_name: Option<String>,
    pub online_status: Option<String>,
    pub last_login_time: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "snake_case"))]
pub struct DatabasePlayerStatistics {
    pub total_accounts: i64,
    pub total_players: i64,
    pub guilds: i64,
    pub guild_members: i64,
    pub tagged_players: i64,
    pub online_statuses: Vec<DatabaseNamedCount>,
    pub life_states: Vec<DatabaseNamedCount>,
    pub recent_players: Vec<DatabaseRecentPlayer>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabasePlayerStatisticsResponse {
    pub namespace: String,
    pub statistics: DatabasePlayerStatistics,
}
