use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::scheduler::Task;
use crate::store::TaskTrigger;

use super::{AppState, VERSION};

pub async fn root() -> impl IntoResponse {
    Json(serde_json::json!({
        "name": "dune-server-service",
        "version": VERSION,
    }))
}

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub ok: bool,
    pub version: &'static str,
    pub now: String,
}

pub async fn health() -> impl IntoResponse {
    Json(HealthResponse {
        ok: true,
        version: VERSION,
        now: Utc::now().to_rfc3339(),
    })
}

#[derive(Debug, Deserialize)]
pub struct RunsQuery {
    pub limit: Option<u32>,
    pub task: Option<String>,
}

pub async fn list_runs(
    State(state): State<AppState>,
    Query(q): Query<RunsQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let runs = state
        .store
        .list_runs(q.limit.unwrap_or(50), q.task.as_deref())?;
    Ok(Json(runs))
}

#[derive(Debug, Deserialize)]
pub struct LogsQuery {
    pub limit: Option<u32>,
    #[serde(rename = "runId")]
    pub run_id: Option<i64>,
}

pub async fn list_logs(
    State(state): State<AppState>,
    Query(q): Query<LogsQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let logs = state.store.list_logs(q.limit.unwrap_or(200), q.run_id)?;
    Ok(Json(logs))
}

#[derive(Debug, Deserialize)]
pub struct TriggerRequest {
    pub task: String,
    #[serde(default)]
    pub options: Option<serde_json::Value>,
}

pub async fn trigger(
    State(state): State<AppState>,
    Json(req): Json<TriggerRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let tasks: Vec<Arc<dyn Task>> = crate::tasks::build_all(state.env.clone());
    let task = tasks
        .into_iter()
        .find(|t| t.id() == req.task)
        .ok_or_else(|| ApiError::not_found(format!("unknown task: {}", req.task)))?;

    let runner = state.runner.clone();
    let options = req.options.clone();
    tokio::spawn(async move {
        if let Err(err) = runner.run(task, TaskTrigger::Manual, false, options).await {
            tracing::error!(error = %err, "manual trigger failed");
        }
    });

    Ok(Json(serde_json::json!({"ok": true, "task": req.task})))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigResponse {
    pub restart_hour: u32,
    pub restart_minute: u32,
    pub restart_warning_frequency_secs: u64,
    pub restart_warning_duration_secs: u64,
    pub update_lead_secs: i64,
    pub restart_tz: String,
    /// Master switches for the daily restart, update loop, and scheduled
    /// backups. Default true; backups also require `backup_cron`.
    pub restart_enabled: bool,
    pub update_enabled: bool,
    pub backup_enabled: bool,
    /// `None` means scheduled backups are disabled — manual triggers still
    /// work. When set, it is the exact 5-field cron string the operator typed,
    /// evaluated in `restart_tz`.
    pub backup_cron: Option<String>,
    pub welcome_message_enabled: bool,
    pub welcome_package_enabled: bool,
    pub welcome_package_version: String,
    pub welcome_package_actions_json: String,
    pub welcome_package_items_json: String,
    pub welcome_whisper_source_player: String,
    pub welcome_message: String,
    /// True if any saved override differs from the active TaskEnv values —
    /// signals to the UI that a service restart is needed to pick them up.
    pub restart_required: bool,
}

pub async fn get_config(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let env = &state.env;
    let stored_hour = state
        .store
        .get_config_i64("restart_hour")?
        .map(|v| v as u32);
    let stored_minute = state
        .store
        .get_config_i64("restart_minute")?
        .map(|v| v as u32);
    let stored_freq = state
        .store
        .get_config_i64("restart_warning_frequency_secs")?
        .map(|v| v as u64);
    let stored_dur = state
        .store
        .get_config_i64("restart_warning_duration_secs")?
        .map(|v| v as u64);
    let stored_lead = state.store.get_config_i64("update_lead_secs")?;
    let stored_restart_enabled = state
        .store
        .get_config_i64("restart_enabled")?
        .map(|v| v != 0);
    let stored_update_enabled = state
        .store
        .get_config_i64("update_enabled")?
        .map(|v| v != 0);
    let stored_backup_enabled = state
        .store
        .get_config_i64("backup_enabled")?
        .map(|v| v != 0);
    let stored_backup_cron = state
        .store
        .get_config("backup_cron")?
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());

    let stored_tz = state.store.get_config("restart_tz")?;
    let stored_welcome_enabled = state
        .store
        .get_config_i64("welcome_package_enabled")?
        .map(|v| v != 0);
    let stored_welcome_message_enabled = state
        .store
        .get_config_i64("welcome_message_enabled")?
        .map(|v| v != 0);
    let stored_welcome_actions_json = state.store.get_config("welcome_package_actions_json")?;
    let stored_welcome_items_json = state.store.get_config("welcome_package_items_json")?;
    let stored_welcome_whisper_source = state.store.get_config("welcome_whisper_source_player")?;
    let stored_welcome_message = state.store.get_config("welcome_message")?;

    let restart_required = stored_hour.map(|v| v != env.restart_hour).unwrap_or(false)
        || stored_minute
            .map(|v| v != env.restart_minute)
            .unwrap_or(false)
        || stored_freq
            .map(|v| v != env.restart_warning_frequency_secs)
            .unwrap_or(false)
        || stored_dur
            .map(|v| v != env.restart_warning_duration_secs)
            .unwrap_or(false)
        || stored_lead
            .map(|v| v != env.update_lead_secs)
            .unwrap_or(false)
        || stored_restart_enabled
            .map(|v| v != env.restart_enabled)
            .unwrap_or(false)
        || stored_update_enabled
            .map(|v| v != env.update_enabled)
            .unwrap_or(false)
        || stored_backup_enabled
            .map(|v| v != env.backup_enabled)
            .unwrap_or(false)
        || stored_backup_cron.as_deref() != env.backup_cron_raw.as_deref()
        || stored_tz
            .as_deref()
            .map(|v| v != env.restart_tz.name())
            .unwrap_or(false)
        || stored_welcome_enabled
            .map(|v| v != env.welcome_package_enabled)
            .unwrap_or(false)
        || stored_welcome_message_enabled
            .map(|v| v != env.welcome_message_enabled)
            .unwrap_or(false)
        || stored_welcome_actions_json
            .as_deref()
            .map(|v| v != env.welcome_package_actions_json)
            .unwrap_or_else(|| {
                stored_welcome_items_json
                    .as_deref()
                    .map(|v| v != env.welcome_package_actions_json)
                    .unwrap_or(false)
            })
        || stored_welcome_whisper_source
            .as_deref()
            .map(|v| v != env.welcome_whisper_source_player)
            .unwrap_or(false)
        || stored_welcome_message
            .as_deref()
            .map(|v| v != env.welcome_message)
            .unwrap_or(false);

    // Return the SAVED (desired) config, falling back to the running `env`
    // snapshot only for fields the operator has never set. Previously every
    // field was read straight from `env` (frozen at service start), so a saved
    // change — e.g. turning Auto Update off — visually reverted on the next
    // load until a restart (#25). `restart_required` above still tells the UI
    // the running scheduler hasn't picked the change up yet.
    let welcome_actions_response = stored_welcome_actions_json
        .clone()
        .or_else(|| stored_welcome_items_json.clone())
        .unwrap_or_else(|| env.welcome_package_actions_json.clone());
    // backup_cron needs care: a present-but-empty row means "explicitly cleared"
    // (backups disabled), which must NOT fall back to the running value. The
    // filtered `stored_backup_cron` collapses cleared and never-set together, so
    // re-read the raw row to tell them apart.
    let backup_cron_response = match state.store.get_config("backup_cron")? {
        Some(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        None => env.backup_cron_raw.clone(),
    };

    Ok(Json(ConfigResponse {
        restart_hour: stored_hour.unwrap_or(env.restart_hour),
        restart_minute: stored_minute.unwrap_or(env.restart_minute),
        restart_warning_frequency_secs: stored_freq
            .unwrap_or(env.restart_warning_frequency_secs),
        restart_warning_duration_secs: stored_dur.unwrap_or(env.restart_warning_duration_secs),
        update_lead_secs: stored_lead.unwrap_or(env.update_lead_secs),
        restart_tz: stored_tz
            .clone()
            .unwrap_or_else(|| env.restart_tz.name().to_string()),
        restart_enabled: stored_restart_enabled.unwrap_or(env.restart_enabled),
        update_enabled: stored_update_enabled.unwrap_or(env.update_enabled),
        backup_enabled: stored_backup_enabled.unwrap_or(env.backup_enabled),
        backup_cron: backup_cron_response,
        welcome_message_enabled: stored_welcome_message_enabled
            .unwrap_or(env.welcome_message_enabled),
        welcome_package_enabled: stored_welcome_enabled.unwrap_or(env.welcome_package_enabled),
        welcome_package_version: env.welcome_package_version.clone(),
        welcome_package_actions_json: welcome_actions_response.clone(),
        welcome_package_items_json: welcome_actions_response,
        welcome_whisper_source_player: stored_welcome_whisper_source
            .clone()
            .unwrap_or_else(|| env.welcome_whisper_source_player.clone()),
        welcome_message: stored_welcome_message
            .clone()
            .unwrap_or_else(|| env.welcome_message.clone()),
        restart_required,
    }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigUpdate {
    pub restart_hour: Option<u32>,
    pub restart_minute: Option<u32>,
    pub restart_warning_frequency_secs: Option<u64>,
    pub restart_warning_duration_secs: Option<u64>,
    pub update_lead_secs: Option<i64>,
    pub restart_tz: Option<String>,
    pub restart_enabled: Option<bool>,
    pub update_enabled: Option<bool>,
    pub backup_enabled: Option<bool>,
    /// Empty string clears the cron schedule (= disabled); non-empty strings
    /// are validated by `parse_cron` before being persisted.
    pub backup_cron: Option<String>,
    pub welcome_message_enabled: Option<bool>,
    pub welcome_package_enabled: Option<bool>,
    pub welcome_package_version: Option<String>,
    pub welcome_package_actions_json: Option<String>,
    pub welcome_package_items_json: Option<String>,
    pub welcome_whisper_source_player: Option<String>,
    pub welcome_message: Option<String>,
}

pub async fn set_config(
    State(state): State<AppState>,
    Json(req): Json<ConfigUpdate>,
) -> Result<impl IntoResponse, ApiError> {
    if let Some(h) = req.restart_hour {
        if h > 23 {
            return Err(ApiError::bad_request("restart_hour must be 0..=23"));
        }
        state.store.set_config("restart_hour", &h.to_string())?;
    }
    if let Some(m) = req.restart_minute {
        if m > 59 {
            return Err(ApiError::bad_request("restart_minute must be 0..=59"));
        }
        state.store.set_config("restart_minute", &m.to_string())?;
    }
    if let Some(s) = req.restart_warning_frequency_secs {
        if s == 0 {
            return Err(ApiError::bad_request(
                "restart_warning_frequency_secs must be greater than 0",
            ));
        }
        state
            .store
            .set_config("restart_warning_frequency_secs", &s.to_string())?;
    }
    if let Some(s) = req.restart_warning_duration_secs {
        if s == 0 {
            return Err(ApiError::bad_request(
                "restart_warning_duration_secs must be greater than 0",
            ));
        }
        state
            .store
            .set_config("restart_warning_duration_secs", &s.to_string())?;
    }
    if let Some(s) = req.update_lead_secs {
        if s < 0 {
            return Err(ApiError::bad_request("update_lead_secs must be >= 0"));
        }
        state.store.set_config("update_lead_secs", &s.to_string())?;
    }
    if let Some(tz) = req.restart_tz.as_deref() {
        if tz.parse::<chrono_tz::Tz>().is_err() {
            return Err(ApiError::bad_request(format!(
                "invalid IANA timezone: {tz}"
            )));
        }
        state.store.set_config("restart_tz", tz)?;
    }
    if let Some(enabled) = req.restart_enabled {
        state
            .store
            .set_config("restart_enabled", if enabled { "1" } else { "0" })?;
    }
    if let Some(enabled) = req.update_enabled {
        state
            .store
            .set_config("update_enabled", if enabled { "1" } else { "0" })?;
    }
    if let Some(enabled) = req.backup_enabled {
        state
            .store
            .set_config("backup_enabled", if enabled { "1" } else { "0" })?;
    }
    if let Some(expr) = req.backup_cron.as_deref() {
        let trimmed = expr.trim();
        if trimmed.is_empty() {
            // Empty -> clear the row so the service treats backups as disabled.
            state.store.set_config("backup_cron", "")?;
        } else {
            crate::scheduler::schedule::parse_cron(trimmed)
                .map_err(|err| ApiError::bad_request(err.to_string()))?;
            state.store.set_config("backup_cron", trimmed)?;
        }
    }
    if let Some(enabled) = req.welcome_package_enabled {
        state
            .store
            .set_config("welcome_package_enabled", if enabled { "1" } else { "0" })?;
    }
    if let Some(enabled) = req.welcome_message_enabled {
        state
            .store
            .set_config("welcome_message_enabled", if enabled { "1" } else { "0" })?;
    }
    if let Some(version) = req.welcome_package_version.as_deref() {
        let trimmed = version.trim();
        if trimmed.is_empty() {
            return Err(ApiError::bad_request(
                "welcome_package_version must not be empty",
            ));
        }
        // Pinned to the daemon's current env value while the feature is
        // experimental. A no-op when the client echoes the same value back
        // (e.g. from a prior GET); a 400 on mismatch so silent drift is
        // visible instead of looking like a successful save.
        if trimmed != state.env.welcome_package_version {
            return Err(ApiError::bad_request(format!(
                "welcome_package_version is currently fixed to {} and cannot be changed",
                state.env.welcome_package_version
            )));
        }
    }
    if let Some(raw) = req.welcome_package_actions_json.as_deref() {
        let trimmed = raw.trim();
        crate::tasks::welcome_package::parse_welcome_actions(trimmed)
            .map_err(|err| ApiError::bad_request(err.to_string()))?;
        state
            .store
            .set_config("welcome_package_actions_json", trimmed)?;
    } else if let Some(raw) = req.welcome_package_items_json.as_deref() {
        // Legacy alias: only applied when actions_json is absent so a mixed
        // payload from a stale client can't clobber the canonical field.
        let trimmed = raw.trim();
        crate::tasks::welcome_package::parse_welcome_actions(trimmed)
            .map_err(|err| ApiError::bad_request(err.to_string()))?;
        state
            .store
            .set_config("welcome_package_actions_json", trimmed)?;
    }
    if let Some(source) = req.welcome_whisper_source_player.as_deref() {
        state
            .store
            .set_config("welcome_whisper_source_player", source.trim())?;
    }
    if let Some(message) = req.welcome_message.as_deref() {
        if message.len() > 1000 {
            return Err(ApiError::bad_request(
                "welcome_message must be <= 1000 characters",
            ));
        }
        state.store.set_config("welcome_message", message)?;
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}

#[derive(Debug, Deserialize)]
pub struct CronPreviewQuery {
    pub expr: String,
    /// Number of upcoming fire times to return. Capped at 20.
    pub count: Option<u32>,
}

/// Validates a cron expression and returns the next few upcoming fire times
/// (in the service's configured `restart_tz`) so the operator gets a sanity
/// check while typing it. Returns `{ok: false, error}` on parse failure.
pub async fn cron_preview(
    State(state): State<AppState>,
    Query(q): Query<CronPreviewQuery>,
) -> impl IntoResponse {
    let count = q.count.unwrap_or(5).clamp(1, 20) as usize;
    match crate::scheduler::schedule::parse_cron(&q.expr) {
        Ok(schedule) => {
            let tz = state.env.restart_tz;
            // Pre-format in the operator's tz so the UI doesn't have to
            // figure out timezone conversion. RFC3339 with the tz offset gets
            // accidentally rendered as UTC by `Date.toISOString()` callers.
            let next: Vec<String> = schedule
                .upcoming(tz)
                .take(count)
                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S %Z").to_string())
                .collect();
            Json(serde_json::json!({
                "ok": true,
                "tz": tz.name(),
                "next": next,
            }))
        }
        Err(err) => Json(serde_json::json!({
            "ok": false,
            "error": err.to_string(),
        })),
    }
}

pub async fn list_timezones() -> impl IntoResponse {
    let names: Vec<&'static str> = chrono_tz::TZ_VARIANTS.iter().map(|tz| tz.name()).collect();
    Json(names)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DumpPruneItem {
    pub namespace: String,
    pub name: String,
    pub action: String,
    pub backup: Option<String>,
    pub phase: String,
    pub created_at: String,
    pub age_days: i64,
}

/// Phase values the prune endpoint considers eligible for deletion. We
/// allow Succeeded (the artifact, if any, is already on disk; the CR is
/// just bookkeeping) and Failed (no artifact produced; pure cluster
/// clutter). In-progress / Pending / unknown phases are kept so we never
/// race the operator.
fn is_prunable_phase(phase: &str) -> bool {
    matches!(phase, "Succeeded" | "Failed")
}

/// Actions the prune endpoint considers eligible. Both `dump` and `import`
/// CRs are pure historical records once they reach a terminal phase —
/// deleting them does not undo the database state they produced. Unknown
/// action strings are excluded by default so future Funcom additions don't
/// get reaped accidentally.
fn is_prunable_action(action: &str) -> bool {
    matches!(action, "dump" | "import")
}

/// Lists `DatabaseOperation` CRs across all namespaces that are safe to
/// delete: `status.phase` is terminal (Succeeded/Failed) AND `spec.action`
/// is one of the known actions (dump/import). The on-disk `.backup` files
/// are not affected by deleting the CR.
async fn list_prunable_dumps(state: &AppState) -> Result<Vec<DumpPruneItem>, ApiError> {
    let result = state
        .env
        .kubectl
        .run(&["get", "databaseoperations", "-A", "-o", "json"])
        .await
        .map_err(|err| ApiError::internal(format!("listing database operations: {err}")))?;
    if !result.ok() {
        return Err(ApiError::internal(format!(
            "kubectl get databaseoperations exited {}: {}",
            result.exit_code,
            result.stderr.trim()
        )));
    }
    let value: serde_json::Value = serde_json::from_str(&result.stdout)
        .map_err(|err| ApiError::internal(format!("parsing operations json: {err}")))?;
    let items = value
        .get("items")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let now = chrono::Utc::now();
    let mut out = Vec::new();
    for item in items {
        let phase = item["status"]["phase"].as_str().unwrap_or_default();
        let action = item["spec"]["action"].as_str().unwrap_or_default();
        if !is_prunable_action(action) || !is_prunable_phase(phase) {
            continue;
        }
        let namespace = item["metadata"]["namespace"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let name = item["metadata"]["name"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        if namespace.is_empty() || name.is_empty() {
            continue;
        }
        let backup = item["spec"]["backup"].as_str().map(|s| s.to_string());
        let created_at_raw = item["metadata"]["creationTimestamp"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let age_days = chrono::DateTime::parse_from_rfc3339(&created_at_raw)
            .map(|dt| (now - dt.with_timezone(&chrono::Utc)).num_days())
            .unwrap_or(0);
        out.push(DumpPruneItem {
            namespace,
            name,
            action: action.to_string(),
            backup,
            phase: phase.to_string(),
            created_at: created_at_raw,
            age_days,
        });
    }
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(out)
}

pub async fn dump_prune_preview(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    let items = list_prunable_dumps(&state).await?;
    Ok(Json(items))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DumpPruneTarget {
    pub namespace: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DumpPruneRequest {
    pub items: Vec<DumpPruneTarget>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DumpPruneSkip {
    pub namespace: String,
    pub name: String,
    pub reason: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DumpPruneResult {
    pub deleted: Vec<String>,
    pub skipped: Vec<DumpPruneSkip>,
}

/// Deletes the requested DatabaseOperation CRs after re-validating each one
/// against the same Succeeded+dump filter — never trust the client. The
/// Funcom operator garbage-collects the owned pod via ownerReferences once
/// the operation CR is gone. The `.backup` files on disk are NOT touched.
pub async fn dump_prune_execute(
    State(state): State<AppState>,
    Json(req): Json<DumpPruneRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let current = list_prunable_dumps(&state).await?;
    let mut deleted = Vec::new();
    let mut skipped = Vec::new();
    for target in req.items {
        let eligible = current
            .iter()
            .any(|item| item.namespace == target.namespace && item.name == target.name);
        if !eligible {
            skipped.push(DumpPruneSkip {
                namespace: target.namespace,
                name: target.name,
                reason: "no longer eligible (not a Succeeded dump, or already removed)".to_string(),
            });
            continue;
        }
        let result = state
            .env
            .kubectl
            .run(&[
                "delete",
                "databaseoperation",
                &target.name,
                "-n",
                &target.namespace,
                "--ignore-not-found",
            ])
            .await;
        match result {
            Ok(r) if r.ok() => {
                tracing::info!(
                    namespace = %target.namespace,
                    name = %target.name,
                    "deleted DatabaseOperation"
                );
                deleted.push(format!("{}/{}", target.namespace, target.name));
            }
            Ok(r) => skipped.push(DumpPruneSkip {
                namespace: target.namespace.clone(),
                name: target.name.clone(),
                reason: format!("kubectl exit {}: {}", r.exit_code, r.stderr.trim()),
            }),
            Err(err) => skipped.push(DumpPruneSkip {
                namespace: target.namespace.clone(),
                name: target.name.clone(),
                reason: format!("kubectl error: {err}"),
            }),
        }
    }
    Ok(Json(DumpPruneResult { deleted, skipped }))
}

#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub message: String,
}

impl ApiError {
    pub fn internal(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: msg.into(),
        }
    }
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: msg.into(),
        }
    }
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: msg.into(),
        }
    }
    pub fn not_implemented(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_IMPLEMENTED,
            message: msg.into(),
        }
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(err: anyhow::Error) -> Self {
        let scrubbed = crate::logger::redact(&format!("{err:#}")).into_owned();
        Self::internal(scrubbed)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        (
            self.status,
            Json(serde_json::json!({"error": self.message})),
        )
            .into_response()
    }
}
