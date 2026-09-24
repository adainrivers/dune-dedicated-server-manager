use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::kubectl::battlegroup as bg;
use crate::kubectl::battlegroup_cli;
use crate::scheduler::{Schedule, Task, TaskCtx, TaskOutcome};
use crate::store::{TaskRun, TaskRunStatus, TaskTrigger};
use crate::tasks::TaskEnv;

/// Replaces `scripts/daily-battlegroup-restart`. Delegates the restart itself
/// to the vendor `battlegroup restart` helper, then waits for full readiness.
/// Schedule fires at the configured wall-clock hour:minute in the IANA timezone
/// supplied by `TaskEnv` (default 05:00 Europe/Amsterdam).
///
/// A scheduled restart is skipped when an automatic update was applied within
/// `RECENT_UPDATE_WINDOW_SECS`: the update already restarted every server, and
/// a restart queued behind it on the maintenance gate would otherwise kick
/// players a second time with no countdown (#37).
pub struct RestartTask {
    env: Arc<TaskEnv>,
}

impl RestartTask {
    pub fn new(env: Arc<TaskEnv>) -> Self {
        Self { env }
    }
}

#[async_trait]
impl Task for RestartTask {
    fn id(&self) -> &'static str {
        "restart"
    }

    fn exclusive(&self) -> bool {
        true
    }

    fn schedule(&self) -> Schedule {
        if self.env.restart_enabled {
            Schedule::daily(self.env.restart_hour, self.env.restart_minute)
        } else {
            Schedule::Disabled
        }
    }

    async fn run(&self, ctx: &TaskCtx) -> Result<TaskOutcome> {
        let cluster = ctx.env.cluster.get().await?;
        let bg_name = bg::bg_name(&ctx.env.kubectl, &cluster.namespace).await?;
        if ctx.trigger == TaskTrigger::Scheduled {
            let stop_value = bg::bg_field(
                &ctx.env.kubectl,
                &cluster.namespace,
                &bg_name,
                "{.spec.stop}",
            )
            .await
            .unwrap_or_default();
            if stop_value == "true" {
                ctx.log_info(&format!(
                    "battlegroup bg={bg_name} is stopped; skipping scheduled restart"
                ))?;
                return Ok(TaskOutcome::Noop);
            }
            let update_runs = ctx.store.list_runs(5, Some("update-apply"))?;
            if update_applied_recently(&update_runs, Utc::now()) {
                ctx.log_info(&format!(
                    "battlegroup bg={bg_name} was updated in the last {} minutes; skipping scheduled restart",
                    RECENT_UPDATE_WINDOW_SECS / 60
                ))?;
                return Ok(TaskOutcome::Done);
            }
        }

        ctx.log_info(&format!(
            "restarting battlegroup bg={bg_name} ns={}",
            cluster.namespace
        ))?;

        if ctx.dry_run {
            ctx.log_info("[dry-run] would invoke battlegroup restart")?;
            return Ok(TaskOutcome::Done);
        }

        ctx.env.bg_cli.restart().await?;
        let summary = battlegroup_cli::wait_until_running(
            &ctx.env.kubectl,
            &cluster.namespace,
            &bg_name,
            Duration::from_secs(1200),
        )
        .await?;
        ctx.log_info(&format!(
            "battlegroup restart complete phase={} serverGroupPhase={} ready={}/{}",
            summary.phase, summary.server_group_phase, summary.ready, summary.size
        ))?;
        Ok(TaskOutcome::Done)
    }
}

const RECENT_UPDATE_WINDOW_SECS: i64 = 30 * 60;

/// True when a real (non dry-run) update-apply run succeeded within the
/// recent-update window.
fn update_applied_recently(runs: &[TaskRun], now: DateTime<Utc>) -> bool {
    runs.iter().any(|run| {
        run.status == TaskRunStatus::Success
            && !run.dry_run
            && run
                .finished_at
                .as_deref()
                .and_then(|ts| DateTime::parse_from_rfc3339(ts).ok())
                .is_some_and(|finished| {
                    let age = now.signed_duration_since(finished.with_timezone(&Utc));
                    age.num_seconds() >= 0 && age.num_seconds() <= RECENT_UPDATE_WINDOW_SECS
                })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(status: TaskRunStatus, dry_run: bool, finished_mins_ago: Option<i64>) -> TaskRun {
        let now = Utc::now();
        TaskRun {
            id: 1,
            task_id: "update-apply".to_string(),
            trigger: TaskTrigger::Scheduled,
            dry_run,
            status,
            started_at: now.to_rfc3339(),
            finished_at: finished_mins_ago
                .map(|mins| (now - chrono::Duration::minutes(mins)).to_rfc3339()),
            duration_ms: None,
            error: None,
        }
    }

    #[test]
    fn recent_successful_update_skips_restart() {
        let runs = [run(TaskRunStatus::Success, false, Some(10))];
        assert!(update_applied_recently(&runs, Utc::now()));
    }

    #[test]
    fn old_failed_dry_or_unfinished_updates_do_not_skip_restart() {
        let now = Utc::now();
        assert!(!update_applied_recently(
            &[run(TaskRunStatus::Success, false, Some(45))],
            now
        ));
        assert!(!update_applied_recently(
            &[run(TaskRunStatus::Failed, false, Some(5))],
            now
        ));
        assert!(!update_applied_recently(
            &[run(TaskRunStatus::Success, true, Some(5))],
            now
        ));
        assert!(!update_applied_recently(
            &[run(TaskRunStatus::Running, false, None)],
            now
        ));
        assert!(!update_applied_recently(&[], now));
    }
}
