use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;

use crate::kubectl::{battlegroup as bg, steam};
use crate::scheduler::{Schedule, Task, TaskCtx, TaskOutcome};
use crate::store::PendingUpdateRecord;
use crate::tasks::TaskEnv;

const UPDATE_RETRY_DELAY_SECS: i64 = 15 * 60;

/// Replaces `scripts/apply-pending-battlegroup-update`. Every minute, checks
/// whether the `pending_update` row's `due_ts` has arrived; when it has,
/// broadcasts, runs a pre-update backup, then delegates the full
/// check/download/apply/restart flow to the vendor `battlegroup update`.
pub struct UpdateApplyTask {
    env: Arc<TaskEnv>,
}

impl UpdateApplyTask {
    pub fn new(env: Arc<TaskEnv>) -> Self {
        Self { env }
    }
}

#[async_trait]
impl Task for UpdateApplyTask {
    fn id(&self) -> &'static str {
        "update-apply"
    }

    fn schedule(&self) -> Schedule {
        if self.env.update_enabled {
            Schedule::interval_secs(60)
        } else {
            Schedule::Disabled
        }
    }

    async fn run(&self, ctx: &TaskCtx) -> Result<TaskOutcome> {
        let Some(pending) = ctx.store.load_pending_update()? else {
            return Ok(TaskOutcome::Noop);
        };
        if Utc::now().timestamp() < pending.due_ts {
            return Ok(TaskOutcome::Noop);
        }

        let result = apply_due_update(ctx, pending).await;
        if let Err(err) = &result {
            if !ctx.dry_run {
                let _ = ctx.store.defer_pending_update(UPDATE_RETRY_DELAY_SECS);
                let _ = ctx.log_warn(&format!(
                    "deferred pending update retry by {} seconds after failure: {err:#}",
                    UPDATE_RETRY_DELAY_SECS
                ));
            }
        }
        result
    }
}

async fn apply_due_update(ctx: &TaskCtx, pending: PendingUpdateRecord) -> Result<TaskOutcome> {
    ctx.log_info(&format!(
        "applying pending update bg={} latest_steam_build={} live={}",
        pending.battlegroup,
        pending.downloaded_version,
        pending.live_version.as_deref().unwrap_or("unknown")
    ))?;

    if pending_update_is_applied(ctx, &pending).await? {
        let downloaded_version = ctx.env.steamcmd.downloaded_version().await?;
        let applied = downloaded_version.as_deref().unwrap_or("unknown");
        ctx.log_info(&format!(
            "pending update already applied to live BattleGroup version {applied}; clearing pending update"
        ))?;
        if !ctx.dry_run {
            ctx.store.clear_pending_update()?;
        }
        return Ok(TaskOutcome::Done);
    }

    if ctx.dry_run {
        ctx.log_info("[dry-run] would run pre-update backup and vendor battlegroup update")?;
        return Ok(TaskOutcome::Done);
    }

    if let Err(err) = ctx
        .env
        .mq
        .publish_service_broadcast(
            "Server update",
            "Server update is starting now. The server will restart.",
            60,
        )
        .await
    {
        ctx.log_warn(&format!("pre-update broadcast failed: {err:#}"))?;
    }

    ctx.log_info("taking pre-update database backup")?;
    backup_one(ctx, &pending).await?;

    ctx.log_info("running vendor battlegroup update")?;
    if let Err(err) = ctx.env.bg_cli.update().await {
        if pending_update_is_applied(ctx, &pending).await? {
            ctx.log_warn(&format!(
                "vendor battlegroup update exited with an error after applying the requested build; verified live state and continuing: {err:#}"
            ))?;
        } else {
            return Err(err);
        }
    }
    ctx.log_info("vendor battlegroup update completed")?;
    ctx.store.clear_pending_update()?;

    if let Err(err) = ctx
        .env
        .mq
        .publish_service_broadcast(
            "Server update",
            "Server update is complete and the server is back online.",
            60,
        )
        .await
    {
        ctx.log_warn(&format!("post-update broadcast failed: {err:#}"))?;
    }
    Ok(TaskOutcome::Done)
}

async fn pending_update_is_applied(ctx: &TaskCtx, pending: &PendingUpdateRecord) -> Result<bool> {
    let bg_doc = bg::bg_json(&ctx.env.kubectl, &pending.namespace, &pending.battlegroup).await?;
    let current_live_version = steam::extract_live_version(&bg_doc);
    let downloaded_version = ctx.env.steamcmd.downloaded_version().await?;
    let local_build = ctx.env.steamcmd.local_build().await?;
    Ok(update_already_applied(
        local_build.as_deref(),
        pending.latest_steam_build.as_deref(),
        current_live_version.as_deref(),
        downloaded_version.as_deref(),
    ))
}

async fn backup_one(ctx: &TaskCtx, pending: &PendingUpdateRecord) -> Result<()> {
    let created = chrono::DateTime::from_timestamp(pending.created_ts, 0).unwrap_or_else(Utc::now);
    let stamp = created.format("%Y%m%d-%H%M%S");
    let backup_name = format!("{}-pre-update-{stamp}.backup", pending.battlegroup);
    crate::tasks::backup::run_backup_and_verify(ctx, &pending.battlegroup, &backup_name)
        .await
        .map(|_| ())
}

/// Decide whether a pending update is already satisfied and can be cleared
/// without running the vendor update.
///
/// Both conditions are required:
/// 1. the on-disk Steam download has advanced to the latest build
///    (`local_build == latest_steam_build`), and
/// 2. the live BattleGroup is already running that downloaded version
///    (`live_version == downloaded_version`).
///
/// Condition 1 is the one that was missing and caused auto-updates to silently
/// no-op: `downloaded_version` is read from `images/battlegroup/version.txt`,
/// which is only rewritten by the vendor `battlegroup update` download step that
/// runs *after* this check. Before that step runs, `version.txt` still holds the
/// old live version, so `live == downloaded` is trivially true on every fresh
/// pending update. Without first confirming the download actually advanced to
/// the latest build, the guard would clear the pending row and return early,
/// never downloading or restarting. `update-check` would then re-create the
/// pending row on its next pass and the cycle would repeat indefinitely.
fn update_already_applied(
    local_build: Option<&str>,
    latest_steam_build: Option<&str>,
    live_version: Option<&str>,
    downloaded_version: Option<&str>,
) -> bool {
    let download_is_latest = matches!(
        (local_build, latest_steam_build),
        (Some(local), Some(latest)) if local == latest
    );
    if !download_is_latest {
        return false;
    }
    matches!(
        (live_version, downloaded_version),
        (Some(live), Some(downloaded)) if live == downloaded
    )
}

#[cfg(test)]
mod tests {
    use super::update_already_applied;

    // Regression for the silent auto-update no-op: a fresh pending update has the
    // on-disk download still behind latest, while live == downloaded (both the old
    // version). The guard must NOT treat this as applied, so the vendor update runs.
    #[test]
    fn not_applied_when_download_still_behind_latest() {
        assert!(!update_already_applied(
            Some("23510000"),           // local build still old
            Some("23528481"),           // latest steam build
            Some("1973075-0-shipping"), // live == downloaded because version.txt
            Some("1973075-0-shipping"), // hasn't been refreshed by the download step yet
        ));
    }

    // After the download advanced and the BattleGroup restarted onto it (e.g. a
    // manual update), the pending update is genuinely applied and can be cleared.
    #[test]
    fn applied_when_download_latest_and_live_matches() {
        assert!(update_already_applied(
            Some("23528481"),
            Some("23528481"),
            Some("1979201-0-shipping"),
            Some("1979201-0-shipping"),
        ));
    }

    // Download advanced to latest but the BattleGroup is still on the old image
    // (download done, restart pending): not yet applied, let the vendor flow run.
    #[test]
    fn not_applied_when_downloaded_but_not_yet_live() {
        assert!(!update_already_applied(
            Some("23528481"),
            Some("23528481"),
            Some("1973075-0-shipping"),
            Some("1979201-0-shipping"),
        ));
    }

    // Missing build/version data is never treated as applied.
    #[test]
    fn not_applied_when_data_missing() {
        assert!(!update_already_applied(
            None,
            Some("23528481"),
            Some("x"),
            Some("x")
        ));
        assert!(!update_already_applied(
            Some("23528481"),
            None,
            Some("x"),
            Some("x")
        ));
        assert!(!update_already_applied(
            Some("23528481"),
            Some("23528481"),
            None,
            Some("1979201-0-shipping")
        ));
    }
}
