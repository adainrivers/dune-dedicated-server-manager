use dune_manager_core::orchestration::{
    is_started_state, BattlegroupManagementOrchestrator, BattlegroupRef, BattlegroupState,
    RusshRunner, StructuredKubectl, VendorBattlegroupWrapper,
};

use crate::commands::shared::{command_error_message, runner_for_remote_kind};
use crate::commands::status_data::read_remote_server_status;
use crate::dto::{RemoteBattlegroupStatus, RemoteServerActionRequest, RemoteServerStatus};
use crate::logging::TauriOperationSink;

type Manager = BattlegroupManagementOrchestrator<
    StructuredKubectl<RusshRunner>,
    VendorBattlegroupWrapper<RusshRunner>,
>;

pub(super) fn manager_from_runner(runner: &RusshRunner) -> Manager {
    let kubernetes = StructuredKubectl::new(runner.clone());
    // Pass the actual SSH login user so the wrapper knows when to insert
    // `sudo -n -u dune -H bash -lc ...`. Defaulting to "dune" here was a
    // silent root-style fallback: when the operator registered the server
    // under e.g. `ubuntu`, the wrapper skipped impersonation and the script
    // tried to read/write /home/dune as ubuntu, which fails noisily.
    let ssh_user = runner.target().user.clone();
    let wrapper = VendorBattlegroupWrapper::with_ssh_user(runner.clone(), ssh_user);
    BattlegroupManagementOrchestrator::new(kubernetes, wrapper)
}

#[tauri::command]
pub async fn start_remote_battlegroup(
    app: tauri::AppHandle,
    request: RemoteServerActionRequest,
) -> Result<RemoteServerStatus, String> {
    run_remote_battlegroup_action(app, request, false).await
}

#[tauri::command]
pub async fn stop_remote_battlegroup(
    app: tauri::AppHandle,
    request: RemoteServerActionRequest,
) -> Result<RemoteServerStatus, String> {
    run_remote_battlegroup_action(app, request, true).await
}

#[tauri::command]
pub async fn restart_remote_battlegroup(
    app: tauri::AppHandle,
    request: RemoteServerActionRequest,
) -> Result<RemoteServerStatus, String> {
    let worker_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut sink = TauriOperationSink::new(worker_app);
        sink.info("bg.restart", "Restarting remote battlegroup.");
        let runner = runner_for_remote_kind(
            request.server_type.as_deref(),
            request.host,
            request.user,
            request.key_path,
            Some(request.port),
        )?;
        let battlegroup = BattlegroupRef {
            namespace: request.namespace,
            name: request.battlegroup_name,
        };
        let manager = manager_from_runner(&runner);
        manager
            .restart_and_wait_director(&battlegroup, 240, &mut sink)
            .map_err(command_error_message)?;
        sink.info("bg.restart", "Refreshing battlegroup state.");
        read_remote_server_status(&runner, &battlegroup.namespace, &battlegroup.name)
            .map_err(command_error_message)
    })
    .await
    .map_err(|err| format!("Remote battlegroup restart worker failed: {err}"))?
}

pub async fn run_remote_battlegroup_action(
    app: tauri::AppHandle,
    request: RemoteServerActionRequest,
    stop: bool,
) -> Result<RemoteServerStatus, String> {
    let worker_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut sink = TauriOperationSink::new(worker_app);
        sink.info("bg.check", "Checking remote battlegroup state.");
        let runner = runner_for_remote_kind(
            request.server_type.as_deref(),
            request.host,
            request.user,
            request.key_path,
            Some(request.port),
        )?;
        run_battlegroup_action_with_runner(
            &runner,
            &mut sink,
            request.namespace,
            request.battlegroup_name,
            stop,
        )
    })
    .await
    .map_err(|err| format!("Remote battlegroup action worker failed: {err}"))?
}

fn run_battlegroup_action_with_runner(
    runner: &RusshRunner,
    sink: &mut TauriOperationSink,
    namespace: String,
    battlegroup_name: String,
    stop: bool,
) -> Result<RemoteServerStatus, String> {
    let battlegroup = BattlegroupRef {
        namespace,
        name: battlegroup_name,
    };
    let manager = manager_from_runner(runner);
    // Pre-flight no-op guard. Read the BattleGroup state from the stable
    // kubectl JSON schema (same source as the dashboard) rather than the
    // vendor wrapper's `status` text: that text layout drifts across Funcom
    // releases and was being misparsed into bogus phases (e.g. status="World",
    // director="2/2"), which made `is_started_state` wrongly report the BG as
    // not running and refuse a perfectly valid Stop (#19).
    let before = read_remote_server_status(runner, &battlegroup.namespace, &battlegroup.name)
        .map_err(command_error_message)?;
    let before_bg = &before.battlegroup;
    let before_started = is_started_state(&battlegroup_state_from_status(before_bg));
    if stop && !before_started {
        return Err(format!(
            "Battlegroup is not running (status={}, stop={}, database={}, gateway={}, director={}).",
            before_bg.phase,
            before_bg.stop,
            before_bg.database_phase,
            before_bg.server_group_phase,
            before_bg.director_phase
        ));
    }
    if !stop && before_started {
        return Err("Battlegroup is already started.".to_string());
    }
    if stop {
        manager
            .stop(&battlegroup, sink)
            .map_err(command_error_message)?;
    } else {
        manager
            .start_and_wait_director(&battlegroup, 180, sink)
            .map_err(command_error_message)?;
    }
    sink.info("bg.check", "Refreshing battlegroup state.");
    read_remote_server_status(runner, &battlegroup.namespace, &battlegroup.name)
        .map_err(command_error_message)
}

/// Adapts the structured `RemoteBattlegroupStatus` (read from the BattleGroup
/// CR JSON) into the core `BattlegroupState` so the shared `is_started_state`
/// phase vocabulary stays the single source of truth. `server_stats` is not
/// consulted by `is_started_state`, so it is left empty.
fn battlegroup_state_from_status(status: &RemoteBattlegroupStatus) -> BattlegroupState {
    BattlegroupState {
        stop: status.stop,
        phase: status.phase.clone(),
        database_phase: status.database_phase.clone(),
        server_group_phase: status.server_group_phase.clone(),
        director_phase: status.director_phase.clone(),
        uptime: status.uptime.clone(),
        server_stats: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(phase: &str, sgp: &str, director: &str, stop: bool) -> RemoteBattlegroupStatus {
        RemoteBattlegroupStatus {
            stop,
            phase: phase.to_string(),
            database_phase: "Ready".to_string(),
            server_group_phase: sgp.to_string(),
            gateway_phase: String::new(),
            director_phase: director.to_string(),
            uptime: "8h45m".to_string(),
            server_stats: Vec::new(),
        }
    }

    #[test]
    fn reconciling_bg_counts_as_started_so_stop_is_allowed() {
        // #19: the structured kubectl read reports phase=Reconciling,
        // serverGroupPhase=Running, directorPhase=Healthy while the BG is up.
        // The stop guard must treat this as started (previously the wrapper
        // text-parse produced status="World"/director="2/2" and refused).
        let s = status("Reconciling", "Running", "Healthy", false);
        assert!(is_started_state(&battlegroup_state_from_status(&s)));
    }

    #[test]
    fn stopped_bg_is_not_started() {
        assert!(!is_started_state(&battlegroup_state_from_status(&status(
            "Stopped", "Stopped", "", true
        ))));
    }
}
