use anyhow::{Context, Result};
use axum::{
    body::Bytes,
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        OriginalUri, Path, Query, State,
    },
    http::{header, HeaderMap, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::{any, get, post},
    Json, Router,
};
use futures::{AsyncBufReadExt, SinkExt, StreamExt};
use k8s_openapi::api::core::v1::Pod;
use kube::{api::LogParams, Api};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{env, io::ErrorKind, path::PathBuf, sync::Arc, time::Duration};
use tokio::fs;
use tokio::time;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::{error, info};
use utoipa_swagger_ui::SwaggerUi;

use crate::{
    auth::authorize,
    clock::now_unix_ms,
    config_files_domain::*,
    database_domain::*,
    director_domain::*,
    director_proxy::*,
    errors::*,
    kubernetes_domain::*,
    models::*,
    openapi,
    security::{redact_json, redact_text},
    state::AppState,
    validation::{validate_kube_name, validate_namespace},
};

const MANAGER_API_VERSION: &str = env!("CARGO_PKG_VERSION");
const MANAGER_LOG_PATH: &str = "/var/log/dune-manager-api.log";
const MANAGER_LOG_MAX_BYTES: usize = 1024 * 1024;

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/auth/login", post(auth_login))
        .route("/api/auth/logout", post(auth_logout))
        .route("/api/auth/session", get(auth_session))
        .route("/api/status", get(status))
        .route("/api/overview", get(overview))
        .route("/api/manager/self", get(manager_self))
        .route("/api/manager/logs", get(manager_logs))
        .route("/api/battlegroups", get(battlegroups))
        .route(
            "/api/battlegroups/:namespace/:name",
            get(battlegroup_detail),
        )
        .route(
            "/api/battlegroups/:namespace/:name/raw",
            get(battlegroup_raw),
        )
        .route(
            "/api/battlegroups/:namespace/:name/start",
            post(start_battlegroup),
        )
        .route(
            "/api/battlegroups/:namespace/:name/stop",
            post(stop_battlegroup),
        )
        .route(
            "/api/battlegroups/:namespace/:name/restart",
            post(restart_battlegroup),
        )
        .route(
            "/api/battlegroups/:namespace/:name/layout",
            get(battlegroup_layout).put(update_battlegroup_layout),
        )
        .route(
            "/api/battlegroups/:namespace/:name/settings",
            axum::routing::patch(update_battlegroup_settings),
        )
        .route("/api/pods", get(pods))
        .route("/api/services", get(services))
        .route("/api/workloads", get(workloads))
        .route("/api/events", get(events))
        .route("/api/storage", get(storage))
        .route(
            "/api/database/world-partitions",
            get(database_world_partitions),
        )
        .route(
            "/api/database/world-partitions/:partition_id",
            axum::routing::patch(update_database_world_partition),
        )
        .route("/api/database/players", get(database_players))
        .route("/api/database-maintenance", get(database_maintenance))
        .route(
            "/api/database-maintenance/backups",
            post(create_database_backup_route),
        )
        .route(
            "/api/database-maintenance/physical-backups/enable",
            post(enable_database_backups_route),
        )
        .route("/api/logs", get(logs))
        .route("/api/logs/export", get(logs_export))
        .route("/api/logs/stream", get(logs_stream))
        .route(
            "/api/config/user-settings",
            get(user_settings_catalog_route),
        )
        .route(
            "/api/config/user-settings/:file",
            get(user_settings_file).put(update_user_settings_file),
        )
        .route(
            "/api/config/user-settings/:file/preview",
            post(preview_user_settings_file_route),
        )
        .route(
            "/api/config/user-settings/:file/backups",
            get(user_settings_backups).post(create_user_settings_backup_route),
        )
        .route(
            "/api/config/user-settings/:file/backups/:backup/restore",
            post(restore_user_settings_backup_route),
        )
        .route("/api/director/battlegroup", get(director_battlegroup))
        .route("/api/director/capabilities", get(director_capabilities))
        .route(
            "/api/director/players/summary",
            get(director_players_summary),
        )
        .route("/api/director/players", get(director_players))
        .route("/api/director/maps", get(director_maps))
        .route(
            "/api/director/config/fls",
            get(director_fls_config)
                .post(director_update_fls_config)
                .delete(director_clear_fls_config),
        )
        .route(
            "/api/director/config/character-transfer",
            get(director_character_transfer_config)
                .post(director_update_character_transfer_config)
                .delete(director_clear_character_transfer_config),
        )
        .route(
            "/api/director/config/maps/:map_name/override",
            get(director_map_override)
                .post(director_update_map_override)
                .delete(director_clear_map_override),
        )
        .route("/api/director/v0/*path", any(director_api_proxy))
        .route("/v0/*path", any(director_root_api_proxy))
        .route("/director", any(director_ui_proxy_root))
        .route("/director/*path", any(director_ui_proxy))
        .route("/Script/*path", any(director_script_proxy))
        .route("/Stylesheet/*path", any(director_stylesheet_proxy))
        .route("/Icons/*path", any(director_icons_proxy))
        .route("/api/telemetry", get(telemetry))
        .merge(
            SwaggerUi::new("/swagger-ui")
                .external_url_unchecked("/openapi.json", openapi::document()),
        )
        .route("/", get(spa_index))
        .route("/assets/*path", get(spa_asset))
        .fallback(spa_fallback)
        .with_state(state)
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
}

fn audit_action(action: &str, target: Option<&str>) {
    match target {
        Some(target) => info!(target: "audit", action, target, "mutating manager action"),
        None => info!(target: "audit", action, "mutating manager action"),
    }
}

async fn health(State(state): State<Arc<AppState>>) -> Json<HealthResponse> {
    Json(HealthResponse {
        ok: true,
        api_version: MANAGER_API_VERSION,
        namespace: state.namespace.clone(),
        auth_enabled: state.token.is_some(),
        director_configured: state.director_base_url.is_some(),
    })
}

async fn auth_login(
    State(state): State<Arc<AppState>>,
    Json(request): Json<LoginRequest>,
) -> Result<Response, ApiError> {
    let token = request.token.trim();
    if token.is_empty() {
        return Err(ApiError::unauthorized());
    }
    if let Some(expected) = state.token.as_deref() {
        if !crate::auth::token_matches(expected, token) {
            return Err(ApiError::unauthorized());
        }
    }
    Ok((
        [(header::SET_COOKIE, auth_cookie(token, 86_400))],
        Json(SessionResponse {
            authenticated: true,
            api_version: MANAGER_API_VERSION,
            namespace: state.namespace.clone(),
            auth_enabled: state.token.is_some(),
        }),
    )
        .into_response())
}

async fn auth_logout() -> Response {
    (
        [(header::SET_COOKIE, auth_cookie("", 0))],
        Json(json!({ "ok": true })),
    )
        .into_response()
}

async fn auth_session(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Json<SessionResponse> {
    let authenticated = authorize(&state, &headers, None).is_ok();
    Json(SessionResponse {
        authenticated,
        api_version: MANAGER_API_VERSION,
        namespace: state.namespace.clone(),
        auth_enabled: state.token.is_some(),
    })
}

fn auth_cookie(token: &str, max_age_seconds: u64) -> String {
    format!("dune_manager_token={token}; HttpOnly; SameSite=Lax; Path=/; Max-Age={max_age_seconds}")
}

async fn status(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> ApiResponse<StatusResponse> {
    authorize(&state, &headers, None)?;
    let (battlegroups, pods, services, director_configured) = tokio::join!(
        list_battlegroups(&state),
        list_pods(&state),
        list_services(&state),
        resolve_director_base_url(&state),
    );
    let battlegroups = battlegroups?.len();
    let pods = pods?.len();
    let services = services?.len();
    let director_configured = director_configured.is_ok();

    Ok(Json(StatusResponse {
        api_version: MANAGER_API_VERSION,
        namespace: state.namespace.clone(),
        auth_enabled: state.token.is_some(),
        director_configured,
        battlegroups,
        pods,
        services,
    }))
}

async fn overview(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> ApiResponse<OverviewResponse> {
    authorize(&state, &headers, None)?;
    let (battlegroups, pods, services, director_base) = tokio::join!(
        list_battlegroups(&state),
        list_pods(&state),
        list_services(&state),
        resolve_director_base_url(&state),
    );
    let battlegroups = battlegroups?;
    let pods = pods?;
    let services = services?;
    let director_available = director_base.is_ok();
    let (players, maps) = if director_available {
        match director_get_json(&state, "/v0/battlegroup").await {
            Ok(value) => (
                Some(director_player_summary(&value)),
                director_map_summaries(&value),
            ),
            Err(_) => (None, Vec::new()),
        }
    } else {
        (None, Vec::new())
    };
    Ok(Json(OverviewResponse {
        status: StatusResponse {
            api_version: MANAGER_API_VERSION,
            namespace: state.namespace.clone(),
            auth_enabled: state.token.is_some(),
            director_configured: director_available,
            battlegroups: battlegroups.len(),
            pods: pods.len(),
            services: services.len(),
        },
        battlegroups,
        workloads: WorkloadsResponse { pods, services },
        director_available,
        players,
        maps,
    }))
}

async fn manager_self(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> ApiResponse<ManagerSelfResponse> {
    authorize(&state, &headers, None)?;
    Ok(Json(ManagerSelfResponse {
        api_version: MANAGER_API_VERSION,
        started_unix_ms: state.started_unix_ms,
        uptime_seconds: ((now_unix_ms()).saturating_sub(state.started_unix_ms) / 1000) as u64,
        pid: std::process::id(),
        namespace: state.namespace.clone(),
        port: state.port,
        auth_enabled: state.token.is_some(),
        director_configured: resolve_director_base_url(&state).await.is_ok(),
        current_exe: env::current_exe()
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_default(),
        service_name: "dune-manager-api",
        binary_path: "/opt/dune-manager/dune-manager-api",
        env_path: "/etc/dune-manager-api.env",
        log_path: MANAGER_LOG_PATH,
    }))
}

async fn manager_logs(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<LogExportQuery>,
) -> ApiResponse<ManagerLogResponse> {
    authorize(&state, &headers, None)?;
    let tail_lines = query.tail.unwrap_or(300).clamp(1, 5000) as usize;
    let bytes = match fs::read(MANAGER_LOG_PATH).await {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == ErrorKind::NotFound => {
            return Ok(Json(ManagerLogResponse {
                path: MANAGER_LOG_PATH,
                available: false,
                truncated: false,
                tail_lines,
                lines: Vec::new(),
            }));
        }
        Err(err) => return Err(err).context("failed to read Manager API log")?,
    };
    let truncated = bytes.len() > MANAGER_LOG_MAX_BYTES;
    let slice = if truncated {
        &bytes[bytes.len() - MANAGER_LOG_MAX_BYTES..]
    } else {
        &bytes
    };
    let text = String::from_utf8_lossy(slice);
    Ok(Json(ManagerLogResponse {
        path: MANAGER_LOG_PATH,
        available: true,
        truncated,
        tail_lines,
        lines: tail_text_lines(&redact_text(&text), tail_lines),
    }))
}

async fn battlegroups(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> ApiResponse<Vec<BattleGroupSummary>> {
    authorize(&state, &headers, None)?;
    Ok(Json(list_battlegroups(&state).await?))
}

async fn battlegroup_detail(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((namespace, name)): Path<(String, String)>,
) -> ApiResponse<BattleGroupDetail> {
    authorize(&state, &headers, None)?;
    validate_namespace(&state, &namespace)?;
    validate_kube_name(&name)?;
    let item = get_battlegroup_object(&state, &name).await?;
    Ok(Json(battlegroup_detail_from_object(&state.namespace, item)))
}

async fn battlegroup_raw(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((namespace, name)): Path<(String, String)>,
) -> ApiResponse<Value> {
    authorize(&state, &headers, None)?;
    validate_namespace(&state, &namespace)?;
    validate_kube_name(&name)?;
    let item = get_battlegroup_object(&state, &name).await?;
    let value = serde_json::to_value(item).context("failed to serialize battlegroup")?;
    Ok(Json(redact_json(value)))
}

async fn start_battlegroup(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((namespace, name)): Path<(String, String)>,
) -> ApiResponse<BattleGroupDetail> {
    authorize(&state, &headers, None)?;
    audit_action("battlegroup.start", Some(&format!("{namespace}/{name}")));
    patch_battlegroup_stop(&state, &namespace, &name, false).await?;
    let item = get_battlegroup_object(&state, &name).await?;
    Ok(Json(battlegroup_detail_from_object(&state.namespace, item)))
}

async fn stop_battlegroup(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((namespace, name)): Path<(String, String)>,
) -> ApiResponse<BattleGroupDetail> {
    authorize(&state, &headers, None)?;
    audit_action("battlegroup.stop", Some(&format!("{namespace}/{name}")));
    patch_battlegroup_stop(&state, &namespace, &name, true).await?;
    let item = get_battlegroup_object(&state, &name).await?;
    Ok(Json(battlegroup_detail_from_object(&state.namespace, item)))
}

async fn restart_battlegroup(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((namespace, name)): Path<(String, String)>,
) -> ApiResponse<BattleGroupDetail> {
    authorize(&state, &headers, None)?;
    audit_action("battlegroup.restart", Some(&format!("{namespace}/{name}")));
    patch_battlegroup_stop(&state, &namespace, &name, true).await?;
    time::sleep(Duration::from_secs(5)).await;
    patch_battlegroup_stop(&state, &namespace, &name, false).await?;
    let item = get_battlegroup_object(&state, &name).await?;
    Ok(Json(battlegroup_detail_from_object(&state.namespace, item)))
}

async fn battlegroup_layout(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((namespace, name)): Path<(String, String)>,
) -> ApiResponse<WorldLayout> {
    authorize(&state, &headers, None)?;
    Ok(Json(
        get_battlegroup_layout(&state, &namespace, &name).await?,
    ))
}

async fn update_battlegroup_layout(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((namespace, name)): Path<(String, String)>,
    Json(request): Json<WorldLayoutUpdateRequest>,
) -> ApiResponse<WorldLayoutUpdateResponse> {
    authorize(&state, &headers, None)?;
    audit_action(
        "battlegroup.layout.update",
        Some(&format!("{namespace}/{name}")),
    );
    Ok(Json(
        patch_battlegroup_layout(&state, &namespace, &name, request).await?,
    ))
}

async fn update_battlegroup_settings(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((namespace, name)): Path<(String, String)>,
    Json(request): Json<BattleGroupSettingsRequest>,
) -> ApiResponse<BattleGroupDetail> {
    authorize(&state, &headers, None)?;
    if let Some(title) = request.title {
        audit_action(
            "battlegroup.settings.title",
            Some(&format!("{namespace}/{name}")),
        );
        return Ok(Json(
            patch_battlegroup_title(&state, &namespace, &name, &title).await?,
        ));
    }
    Err(ApiError::bad_request("no supported settings were provided"))
}

async fn user_settings_catalog_route(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> ApiResponse<UserSettingsCatalog> {
    authorize(&state, &headers, None)?;
    Ok(Json(user_settings_catalog()))
}

async fn user_settings_file(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(file): Path<String>,
) -> ApiResponse<UserSettingsFile> {
    authorize(&state, &headers, None)?;
    Ok(Json(read_user_settings_file(&state, &file).await?))
}

async fn update_user_settings_file(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(file): Path<String>,
    Json(request): Json<UserSettingsUpdateRequest>,
) -> ApiResponse<UserSettingsUpdateResponse> {
    authorize(&state, &headers, None)?;
    audit_action("config.user-settings.update", Some(&file));
    Ok(Json(
        write_user_settings_file(&state, &file, request.content).await?,
    ))
}

async fn preview_user_settings_file_route(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(file): Path<String>,
    Json(request): Json<UserSettingsPreviewRequest>,
) -> ApiResponse<UserSettingsPreviewResponse> {
    authorize(&state, &headers, None)?;
    Ok(Json(
        preview_user_settings_file(&state, &file, request.content).await?,
    ))
}

async fn user_settings_backups(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(file): Path<String>,
) -> ApiResponse<UserSettingsBackupsResponse> {
    authorize(&state, &headers, None)?;
    Ok(Json(list_user_settings_backups(&state, &file).await?))
}

async fn create_user_settings_backup_route(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(file): Path<String>,
) -> ApiResponse<UserSettingsBackupCreateResponse> {
    authorize(&state, &headers, None)?;
    audit_action("config.user-settings.backup", Some(&file));
    Ok(Json(create_user_settings_backup(&state, &file).await?))
}

async fn restore_user_settings_backup_route(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((file, backup)): Path<(String, String)>,
) -> ApiResponse<UserSettingsRestoreResponse> {
    authorize(&state, &headers, None)?;
    audit_action(
        "config.user-settings.restore",
        Some(&format!("{file}/{backup}")),
    );
    Ok(Json(
        restore_user_settings_backup(&state, &file, &backup).await?,
    ))
}

async fn pods(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> ApiResponse<Vec<PodSummary>> {
    authorize(&state, &headers, None)?;
    Ok(Json(list_pods(&state).await?))
}

async fn services(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> ApiResponse<Vec<ServiceSummary>> {
    authorize(&state, &headers, None)?;
    Ok(Json(list_services(&state).await?))
}

async fn workloads(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> ApiResponse<WorkloadsResponse> {
    authorize(&state, &headers, None)?;
    let (pods, services) = tokio::try_join!(list_pods(&state), list_services(&state))?;
    Ok(Json(WorkloadsResponse { pods, services }))
}

async fn events(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<LogExportQuery>,
) -> ApiResponse<EventsResponse> {
    authorize(&state, &headers, None)?;
    let limit = query.tail.unwrap_or(80).clamp(1, 500) as usize;
    Ok(Json(EventsResponse {
        namespace: state.namespace.clone(),
        events: list_events(&state, limit).await?,
    }))
}

async fn storage(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> ApiResponse<StorageResponse> {
    authorize(&state, &headers, None)?;
    Ok(Json(StorageResponse {
        namespace: state.namespace.clone(),
        claims: list_persistent_volume_claims(&state).await?,
    }))
}

async fn database_world_partitions(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> ApiResponse<DatabaseWorldPartitionsResponse> {
    authorize(&state, &headers, None)?;
    Ok(Json(DatabaseWorldPartitionsResponse {
        namespace: state.namespace.clone(),
        rows: list_world_partitions(&state).await?,
    }))
}

async fn update_database_world_partition(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(partition_id): Path<i64>,
    Json(request): Json<DatabaseWorldPartitionUpdateRequest>,
) -> ApiResponse<DatabaseWorldPartitionUpdateResponse> {
    authorize(&state, &headers, None)?;
    audit_action(
        "database.world_partition.update",
        Some(&partition_id.to_string()),
    );
    let row = update_world_partition(&state, partition_id, request)
        .await?
        .ok_or_else(|| ApiError::not_found("world partition was not found"))?;
    Ok(Json(DatabaseWorldPartitionUpdateResponse {
        namespace: state.namespace.clone(),
        row,
    }))
}

async fn database_players(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> ApiResponse<DatabasePlayersResponse> {
    authorize(&state, &headers, None)?;
    Ok(Json(DatabasePlayersResponse {
        namespace: state.namespace.clone(),
        rows: list_database_players(&state).await?,
    }))
}

async fn database_maintenance(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> ApiResponse<DatabaseMaintenanceResponse> {
    authorize(&state, &headers, None)?;
    Ok(Json(list_database_maintenance(&state).await?))
}

async fn create_database_backup_route(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(request): Json<CreateDatabaseBackupRequest>,
) -> ApiResponse<DatabaseMaintenanceItem> {
    authorize(&state, &headers, None)?;
    audit_action("database.backup.create", request.battle_group.as_deref());
    Ok(Json(
        create_database_backup(&state, request.battle_group, request.originator).await?,
    ))
}

async fn enable_database_backups_route(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(request): Json<EnableDatabaseBackupsRequest>,
) -> ApiResponse<DatabaseMaintenanceResponse> {
    authorize(&state, &headers, None)?;
    audit_action(
        "database.physical_backups.enable",
        request.battle_group.as_deref(),
    );
    Ok(Json(
        enable_database_physical_backups(&state, request.battle_group).await?,
    ))
}

async fn logs(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<LogQuery>,
) -> ApiResponse<Value> {
    authorize(&state, &headers, None)?;
    validate_kube_name(&query.pod)?;
    if let Some(container) = &query.container {
        validate_kube_name(container)?;
    }

    let pods: Api<Pod> = Api::namespaced(state.client.clone(), &state.namespace);
    let mut params = LogParams {
        tail_lines: Some(query.tail.unwrap_or(200).clamp(1, 2000)),
        ..Default::default()
    };
    params.container = query.container.clone();
    let text = pods
        .logs(&query.pod, &params)
        .await
        .with_context(|| format!("failed to read logs for pod {}", query.pod))?;

    Ok(Json(json!({
        "pod": query.pod,
        "container": query.container,
        "lines": redact_text(&text).lines().collect::<Vec<_>>()
    })))
}

async fn logs_export(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<LogExportQuery>,
) -> ApiResponse<LogExportResponse> {
    authorize(&state, &headers, None)?;
    let tail_lines = query.tail.unwrap_or(400).clamp(1, 5000);
    let pods_api: Api<Pod> = Api::namespaced(state.client.clone(), &state.namespace);
    let pod_list = pods_api
        .list(&Default::default())
        .await
        .context("failed to list pods for log export")?;
    let mut pods = Vec::new();
    let mut errors = Vec::new();

    for pod in pod_list {
        let name = pod.metadata.name.clone().unwrap_or_default();
        if name.is_empty() {
            continue;
        }
        let phase = pod
            .status
            .as_ref()
            .and_then(|status| status.phase.clone())
            .unwrap_or_else(|| "Unknown".to_string());
        let container_names = pod
            .spec
            .as_ref()
            .map(|spec| {
                spec.containers
                    .iter()
                    .map(|container| container.name.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let mut containers = Vec::new();

        for container in container_names {
            let params = LogParams {
                container: Some(container.clone()),
                tail_lines: Some(tail_lines),
                ..Default::default()
            };
            match pods_api.logs(&name, &params).await {
                Ok(text) => containers.push(ContainerLogExport {
                    name: container,
                    lines: redact_text(&text)
                        .lines()
                        .map(ToString::to_string)
                        .collect(),
                }),
                Err(err) => errors.push(LogExportError {
                    pod: name.clone(),
                    container: Some(container),
                    message: err.to_string(),
                }),
            }
        }

        pods.push(PodLogExport {
            name,
            phase,
            containers,
        });
    }

    Ok(Json(LogExportResponse {
        namespace: state.namespace.clone(),
        generated_at_unix_ms: now_unix_ms(),
        tail_lines,
        pods,
        errors,
    }))
}

async fn logs_stream(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<LogStreamQuery>,
) -> Result<Response, ApiError> {
    authorize(&state, &headers, query.token.as_deref())?;
    validate_kube_name(&query.pod)?;
    if let Some(container) = &query.container {
        validate_kube_name(container)?;
    }
    Ok(ws.on_upgrade(move |socket| logs_stream_socket(socket, state, query)))
}

async fn logs_stream_socket(socket: WebSocket, state: Arc<AppState>, query: LogStreamQuery) {
    let (mut sender, mut receiver) = socket.split();
    let pods: Api<Pod> = Api::namespaced(state.client.clone(), &state.namespace);
    let params = LogParams {
        follow: true,
        container: query.container.clone(),
        tail_lines: Some(query.tail.unwrap_or(100).clamp(1, 5000)),
        ..LogParams::default()
    };

    let mut lines = match pods.log_stream(&query.pod, &params).await {
        Ok(stream) => stream.lines(),
        Err(err) => {
            let payload = json!({ "type": "error", "message": err.to_string() }).to_string();
            let _ = sender.send(Message::Text(payload)).await;
            return;
        }
    };

    loop {
        tokio::select! {
            line = lines.next() => {
                match line {
                    Some(Ok(line)) => {
                        let payload = json!({ "type": "line", "line": redact_text(&line) }).to_string();
                        if sender.send(Message::Text(payload)).await.is_err() {
                            break;
                        }
                    }
                    Some(Err(err)) => {
                        let payload = json!({ "type": "error", "message": err.to_string() }).to_string();
                        let _ = sender.send(Message::Text(payload)).await;
                        break;
                    }
                    None => break,
                }
            }
            incoming = receiver.next() => {
                match incoming {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(_)) => {}
                    Some(Err(_)) => break,
                }
            }
        }
    }
}

async fn director_battlegroup(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> ApiResponse<Value> {
    authorize(&state, &headers, None)?;
    let value = director_get_json(&state, "/v0/battlegroup").await?;

    Ok(Json(redact_json(value)))
}

async fn director_capabilities(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> ApiResponse<DirectorCapabilities> {
    authorize(&state, &headers, None)?;
    Ok(Json(DirectorCapabilities {
        configured: resolve_director_base_url(&state).await.is_ok(),
        api_paths: director_capabilities_list(),
        ui_proxy_path: "/director",
    }))
}

async fn director_players_summary(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> ApiResponse<DirectorPlayerSummary> {
    authorize(&state, &headers, None)?;
    let value = director_get_json(&state, "/v0/battlegroup").await?;
    Ok(Json(director_player_summary(&value)))
}

async fn director_players(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<DirectorPlayersQuery>,
) -> ApiResponse<DirectorPlayerLists> {
    authorize(&state, &headers, None)?;
    let base_url = ensure_director_available(&state).await?;

    let (all, online, in_transit, grace_period, completion, queued) = if query.full.unwrap_or(false)
    {
        let (all, online, in_transit, grace_period, completion, queued) = tokio::try_join!(
            director_get_json_with_base(&state, &base_url, "/v0/players"),
            director_get_json_with_base(&state, &base_url, "/v0/players/online"),
            director_get_json_with_base(&state, &base_url, "/v0/players/intransit"),
            director_get_json_with_base(&state, &base_url, "/v0/players/graceperiod"),
            director_get_json_with_base(&state, &base_url, "/v0/players/completion"),
            director_get_json_with_base(&state, &base_url, "/v0/players/queued"),
        )?;
        (all, online, in_transit, grace_period, completion, queued)
    } else {
        let (all, online) = tokio::try_join!(
            director_get_json_with_base(&state, &base_url, "/v0/players"),
            director_get_json_with_base(&state, &base_url, "/v0/players/online"),
        )?;
        let empty = json!([]);
        (
            all,
            online,
            empty.clone(),
            empty.clone(),
            empty.clone(),
            empty,
        )
    };
    Ok(Json(director_player_lists(
        &all,
        &online,
        &in_transit,
        &grace_period,
        &completion,
        &queued,
    )))
}

#[derive(Debug, Deserialize)]
struct DirectorPlayersQuery {
    full: Option<bool>,
}

async fn director_maps(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> ApiResponse<Vec<DirectorMapSummary>> {
    authorize(&state, &headers, None)?;
    let value = director_get_json(&state, "/v0/battlegroup").await?;
    Ok(Json(director_map_summaries(&value)))
}

async fn director_map_override(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(map_name): Path<String>,
) -> ApiResponse<DirectorMapConfigDetail> {
    authorize(&state, &headers, None)?;
    validate_director_map_name(&map_name)?;
    let value = director_get_json(&state, "/v0/battlegroup").await?;
    Ok(Json(director_map_config_detail(&value, &map_name)?))
}

async fn director_fls_config(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> ApiResponse<Value> {
    authorize(&state, &headers, None)?;
    let value = director_get_json(&state, "/v0/BattlegroupFetchFlsReportSettings").await?;
    Ok(Json(redact_json(value)))
}

async fn director_update_fls_config(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> ApiResponse<Value> {
    authorize(&state, &headers, None)?;
    audit_action("director.fls.update", None);
    proxy_director_json(
        &state,
        Method::POST,
        "/v0/BattlegroupUpdateFlsReportSettings",
        None,
        body,
    )
    .await
}

async fn director_clear_fls_config(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> ApiResponse<Value> {
    authorize(&state, &headers, None)?;
    audit_action("director.fls.clear", None);
    proxy_director_json(
        &state,
        Method::POST,
        "/v0/BattlegroupClearFlsReportOverrides",
        None,
        Bytes::new(),
    )
    .await
}

async fn director_character_transfer_config(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> ApiResponse<Value> {
    authorize(&state, &headers, None)?;
    let value = director_get_json(&state, "/v0/BattlegroupFetchCharacterTransferRules").await?;
    Ok(Json(redact_json(value)))
}

async fn director_update_character_transfer_config(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> ApiResponse<Value> {
    authorize(&state, &headers, None)?;
    audit_action("director.character_transfer.update", None);
    proxy_director_json(
        &state,
        Method::POST,
        "/v0/BattlegroupUpdateCharacterTransferSettings",
        None,
        body,
    )
    .await
}

async fn director_clear_character_transfer_config(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> ApiResponse<Value> {
    authorize(&state, &headers, None)?;
    audit_action("director.character_transfer.clear", None);
    proxy_director_json(
        &state,
        Method::POST,
        "/v0/BattlegroupClearCharacterTransferOverrides",
        None,
        Bytes::new(),
    )
    .await
}

async fn director_update_map_override(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(map_name): Path<String>,
    body: Bytes,
) -> ApiResponse<Value> {
    authorize(&state, &headers, None)?;
    validate_director_map_name(&map_name)?;
    audit_action("director.map_override.update", Some(&map_name));
    proxy_director_json(
        &state,
        Method::POST,
        "/v0/BattlegroupUpdateServerGroupConfig",
        None,
        body,
    )
    .await
}

async fn director_clear_map_override(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(map_name): Path<String>,
) -> ApiResponse<Value> {
    authorize(&state, &headers, None)?;
    validate_director_map_name(&map_name)?;
    audit_action("director.map_override.clear", Some(&map_name));
    proxy_director_json(
        &state,
        Method::POST,
        "/v0/BattlegroupClearMapConfigOverrides",
        None,
        Bytes::from(map_name),
    )
    .await
}

async fn director_api_proxy(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(path): Path<String>,
    OriginalUri(uri): OriginalUri,
    method: Method,
    body: Bytes,
) -> ApiResponse<Value> {
    authorize(&state, &headers, query_token(uri.query()))?;
    let director_path = format!("/v0/{path}");
    if !is_allowed_director_api(method.as_str(), &director_path) {
        return Err(ApiError::bad_request(
            "Director API path is not allowlisted",
        ));
    }
    if method != Method::GET {
        audit_action(
            "director.proxy",
            Some(&format!("{} {}", method.as_str(), director_path)),
        );
    }
    proxy_director_json(
        &state,
        method,
        &director_path,
        director_query(uri.query()),
        body,
    )
    .await
}

async fn director_root_api_proxy(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(path): Path<String>,
    OriginalUri(uri): OriginalUri,
    method: Method,
    body: Bytes,
) -> Result<Response, ApiError> {
    authorize(&state, &headers, query_token(uri.query()))?;
    let director_path = format!("/v0/{path}");
    if !is_allowed_director_api(method.as_str(), &director_path) {
        return Err(ApiError::bad_request(
            "Director API path is not allowlisted",
        ));
    }
    if method != Method::GET {
        audit_action(
            "director.proxy",
            Some(&format!("{} {}", method.as_str(), director_path)),
        );
    }
    proxy_director_response(
        &state,
        method,
        &director_path,
        director_query(uri.query()),
        body,
        None,
    )
    .await
}

async fn director_ui_proxy_root(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OriginalUri(uri): OriginalUri,
    method: Method,
    body: Bytes,
) -> Result<Response, ApiError> {
    authorize(&state, &headers, query_token(uri.query()))?;
    proxy_director_response(
        &state,
        method,
        "/",
        director_query(uri.query()),
        body,
        query_token(uri.query()),
    )
    .await
}

async fn director_script_proxy(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(path): Path<String>,
    OriginalUri(uri): OriginalUri,
    method: Method,
    body: Bytes,
) -> Result<Response, ApiError> {
    director_static_proxy(state, headers, path, uri, method, body, "Script").await
}

async fn director_stylesheet_proxy(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(path): Path<String>,
    OriginalUri(uri): OriginalUri,
    method: Method,
    body: Bytes,
) -> Result<Response, ApiError> {
    director_static_proxy(state, headers, path, uri, method, body, "Stylesheet").await
}

async fn director_icons_proxy(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(path): Path<String>,
    OriginalUri(uri): OriginalUri,
    method: Method,
    body: Bytes,
) -> Result<Response, ApiError> {
    director_static_proxy(state, headers, path, uri, method, body, "Icons").await
}

async fn director_static_proxy(
    state: Arc<AppState>,
    headers: HeaderMap,
    path: String,
    uri: axum::http::Uri,
    method: Method,
    body: Bytes,
    prefix: &str,
) -> Result<Response, ApiError> {
    authorize(&state, &headers, query_token(uri.query()))?;
    if !is_safe_static_path(&path) {
        return Err(ApiError::bad_request("invalid Director static path"));
    }
    let director_path = format!("/{prefix}/{path}");
    proxy_director_response(
        &state,
        method,
        &director_path,
        director_query(uri.query()),
        body,
        None,
    )
    .await
}

async fn director_ui_proxy(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(path): Path<String>,
    OriginalUri(uri): OriginalUri,
    method: Method,
    body: Bytes,
) -> Result<Response, ApiError> {
    authorize(&state, &headers, query_token(uri.query()))?;
    let director_path = format!("/{path}");
    proxy_director_response(
        &state,
        method,
        &director_path,
        director_query(uri.query()),
        body,
        query_token(uri.query()),
    )
    .await
}

async fn telemetry(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<WsQuery>,
) -> Result<Response, ApiError> {
    authorize(&state, &headers, query.token.as_deref())?;
    Ok(ws.on_upgrade(move |socket| telemetry_socket(socket, state)))
}

async fn telemetry_socket(socket: WebSocket, state: Arc<AppState>) {
    let (mut sender, mut receiver) = socket.split();
    let mut interval = time::interval(Duration::from_secs(2));

    loop {
        tokio::select! {
            _ = interval.tick() => {
                match telemetry_snapshot(&state).await {
                    Ok(payload) => {
                        let envelope = TelemetryEnvelope {
                            event_type: "snapshot".to_string(),
                            time_unix_ms: now_unix_ms(),
                            payload,
                        };
                        match serde_json::to_string(&envelope) {
                            Ok(text) => {
                                if sender.send(Message::Text(text)).await.is_err() {
                                    break;
                                }
                            }
                            Err(err) => error!(?err, "failed to serialize telemetry event"),
                        }
                    }
                    Err(err) => {
                        let envelope = TelemetryEnvelope {
                            event_type: "error".to_string(),
                            time_unix_ms: now_unix_ms(),
                            payload: json!({ "message": err.to_string() }),
                        };
                        if let Ok(text) = serde_json::to_string(&envelope) {
                            let _ = sender.send(Message::Text(text)).await;
                        }
                    }
                }
            }
            incoming = receiver.next() => {
                match incoming {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(_)) => {}
                    Some(Err(_)) => break,
                }
            }
        }
    }
}

async fn spa_index(State(state): State<Arc<AppState>>) -> Response {
    serve_ui_file(state.ui_dir.join("index.html"), "text/html; charset=utf-8").await
}

async fn spa_asset(State(state): State<Arc<AppState>>, Path(path): Path<String>) -> Response {
    match safe_ui_path(&state.ui_dir.join("assets"), &path) {
        Some(path) => serve_ui_file(path.clone(), content_type_for(&path)).await,
        None => (StatusCode::BAD_REQUEST, "invalid asset path").into_response(),
    }
}

async fn spa_fallback(
    State(state): State<Arc<AppState>>,
    OriginalUri(uri): OriginalUri,
) -> Response {
    let path = uri.path();
    if path.starts_with("/api/")
        || path == "/health"
        || path == "/openapi.json"
        || path.starts_with("/swagger-ui")
        || path.starts_with("/director")
        || path.starts_with("/v0/")
        || path.starts_with("/Script/")
        || path.starts_with("/Stylesheet/")
        || path.starts_with("/Icons/")
    {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    }
    serve_ui_file(state.ui_dir.join("index.html"), "text/html; charset=utf-8").await
}

async fn serve_ui_file(path: PathBuf, content_type: &'static str) -> Response {
    match fs::read(&path).await {
        Ok(bytes) => ([(header::CONTENT_TYPE, content_type)], bytes).into_response(),
        Err(_) => (
            StatusCode::NOT_FOUND,
            format!(
                "Manager UI has not been built or installed at {}",
                path.display()
            ),
        )
            .into_response(),
    }
}

fn safe_ui_path(root: &std::path::Path, path: &str) -> Option<PathBuf> {
    if path.is_empty()
        || path.contains("..")
        || path.contains('\\')
        || !path
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '/' | '.' | '-' | '_'))
    {
        return None;
    }
    Some(root.join(path))
}

fn content_type_for(path: &std::path::Path) -> &'static str {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
    {
        "css" => "text/css; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "ico" => "image/x-icon",
        "html" => "text/html; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn tail_text_lines(text: &str, tail_lines: usize) -> Vec<String> {
    let mut lines = text
        .lines()
        .rev()
        .take(tail_lines)
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    lines.reverse();
    lines
}

async fn telemetry_snapshot(state: &AppState) -> Result<Value> {
    let (battlegroups, pods, services) = tokio::try_join!(
        list_battlegroups(state),
        list_pods(state),
        list_services(state)
    )?;

    Ok(json!({
        "namespace": state.namespace,
        "battlegroups": battlegroups,
        "pods": pods,
        "services": services
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tails_text_lines_in_original_order() {
        assert_eq!(
            tail_text_lines("one\ntwo\nthree\nfour\n", 2),
            vec!["three".to_string(), "four".to_string()]
        );
    }

    #[test]
    fn tails_all_lines_when_limit_is_larger() {
        assert_eq!(
            tail_text_lines("one\ntwo", 20),
            vec!["one".to_string(), "two".to_string()]
        );
    }
}
