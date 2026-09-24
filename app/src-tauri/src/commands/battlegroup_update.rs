use dune_manager_core::models::CommandFailure;
use dune_manager_core::orchestration::{BattlegroupRef, RusshRunner};

use crate::commands::battlegroup::manager_from_runner;
use crate::commands::shared::{command_error_message, runner_for_remote_kind};
use crate::commands::status_data::read_remote_server_status;
use crate::dto::{RemoteServerActionRequest, RemoteServerPackageStatus, RemoteServerStatus};
use crate::logging::TauriOperationSink;

/// Line the vendor `battlegroup update` prints once the new images are
/// patched in. Everything after it is the non-idempotent symlink refresh.
const VENDOR_UPDATE_FINISHED_MARKER: &str = "Finished updating battlegroup to version";

#[tauri::command]
pub async fn update_remote_battlegroup(
    app: tauri::AppHandle,
    request: RemoteServerActionRequest,
) -> Result<RemoteServerStatus, String> {
    let worker_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut sink = TauriOperationSink::new(worker_app);
        sink.info("bg.update", "Running vendor wrapper update.");
        let runner = runner_for_remote_kind(
            request.server_type.as_deref(),
            request.host,
            request.user,
            request.key_path,
            Some(request.port),
        )?;
        run_battlegroup_update_with_runner(
            &runner,
            &mut sink,
            request.namespace,
            request.battlegroup_name,
        )
    })
    .await
    .map_err(|err| format!("Remote battlegroup update worker failed: {err}"))?
}

fn run_battlegroup_update_with_runner(
    runner: &RusshRunner,
    sink: &mut TauriOperationSink,
    namespace: String,
    battlegroup_name: String,
) -> Result<RemoteServerStatus, String> {
    let battlegroup = BattlegroupRef {
        namespace,
        name: battlegroup_name,
    };
    let manager = manager_from_runner(runner);
    sink.warn(
        "bg.update",
        "Running vendor `battlegroup update` (steamcmd + operators + maps + images).",
    );
    let stdout = match manager.update(&battlegroup, sink) {
        Ok(stdout) => stdout,
        Err(err) => accept_finished_update(runner, &battlegroup, sink, err)?,
    };
    if !stdout.trim().is_empty() {
        sink.info("bg.update", stdout.trim().to_string());
    }
    sink.info("bg.update", "Refreshing battlegroup state.");
    read_remote_server_status(runner, &battlegroup.namespace, &battlegroup.name)
        .map_err(command_error_message)
}

/// The vendor wrapper exits nonzero after a successful update because its
/// trailing `ln -s` symlink refresh is not idempotent (#24). Accept that exit
/// only when the wrapper printed its own completion line AND the live
/// BattleGroup is running the downloaded version; anything else stays a
/// failure.
fn accept_finished_update(
    runner: &RusshRunner,
    battlegroup: &BattlegroupRef,
    sink: &mut TauriOperationSink,
    err: CommandFailure,
) -> Result<String, String> {
    if !vendor_update_reported_finish(&err) {
        return Err(command_error_message(err));
    }
    let status = match read_remote_server_status(runner, &battlegroup.namespace, &battlegroup.name)
    {
        Ok(status) => status,
        Err(_) => return Err(command_error_message(err)),
    };
    if !live_matches_downloaded(&status.package) {
        return Err(command_error_message(err));
    }
    sink.warn(
        "bg.update",
        format!(
            "Vendor update exited with status {} after finishing; the live BattleGroup runs the downloaded version, so the update is treated as applied.",
            err.code.map_or_else(|| "unknown".to_string(), |code| code.to_string())
        ),
    );
    Ok(err.stdout)
}

fn vendor_update_reported_finish(err: &CommandFailure) -> bool {
    err.stdout.contains(VENDOR_UPDATE_FINISHED_MARKER)
        || err.stderr.contains(VENDOR_UPDATE_FINISHED_MARKER)
}

fn live_matches_downloaded(package: &RemoteServerPackageStatus) -> bool {
    match (
        package.battlegroup_version.as_deref().map(str::trim),
        package.live_battlegroup_version.as_deref().map(str::trim),
    ) {
        (Some(downloaded), Some(live)) => !downloaded.is_empty() && downloaded == live,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failure(stdout: &str, stderr: &str) -> CommandFailure {
        CommandFailure {
            message: "ssh remote command exited with status 1".to_string(),
            stdout: stdout.to_string(),
            stderr: stderr.to_string(),
            code: Some(1),
        }
    }

    fn package(downloaded: Option<&str>, live: Option<&str>) -> RemoteServerPackageStatus {
        RemoteServerPackageStatus {
            installed_build_id: None,
            battlegroup_version: downloaded.map(str::to_string),
            live_battlegroup_version: live.map(str::to_string),
            operator_version: None,
        }
    }

    #[test]
    fn finish_marker_detected_in_stdout_or_stderr() {
        // Shape reported live in #24: success line, then the ln failures.
        let out = "Finished updating battlegroup to version 1979201-0-shipping";
        let err = "ln: /home/dune/.dune/bin/battlegroup: File exists";
        assert!(vendor_update_reported_finish(&failure(out, err)));
        assert!(vendor_update_reported_finish(&failure("", out)));
    }

    #[test]
    fn missing_finish_marker_is_a_real_failure() {
        let err = "ERROR! Failed to install app '4754530' (Missing configuration)";
        assert!(!vendor_update_reported_finish(&failure("", err)));
    }

    #[test]
    fn live_must_match_downloaded_version() {
        let v = Some("1979201-0-shipping");
        assert!(live_matches_downloaded(&package(v, v)));
        assert!(!live_matches_downloaded(&package(
            Some("1979201-0-shipping"),
            Some("1973075-0-shipping")
        )));
        assert!(!live_matches_downloaded(&package(None, v)));
        assert!(!live_matches_downloaded(&package(v, None)));
        assert!(!live_matches_downloaded(&package(Some(" "), Some(" "))));
    }
}
