use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use chrono::Utc;

use crate::kubectl::battlegroup as bg;
use crate::kubectl::{run_process, KubectlClient};
use crate::scheduler::{Schedule, Task, TaskCtx, TaskOutcome};
use crate::tasks::TaskEnv;

/// Replaces `scripts/cron-battlegroup-backup`. Runs the vendor backup helper,
/// emits a per-run log line referencing the dump path, and lets the operator
/// handle stale dump cleanup out-of-band (we do not invoke `sudo find -delete`
/// from the daemon — too easy to widen the blast radius).
pub struct BackupTask {
    env: Arc<TaskEnv>,
}

impl BackupTask {
    pub fn new(env: Arc<TaskEnv>) -> Self {
        Self { env }
    }
}

#[async_trait]
impl Task for BackupTask {
    fn id(&self) -> &'static str {
        "backup"
    }

    fn schedule(&self) -> Schedule {
        // Two gates: the `backup_enabled` master switch and a parsed cron.
        // Vendor backups block server I/O for the whole dump, so a cron must be
        // set explicitly (see seb851's report of in-play perf hits with the old
        // 2h default). The switch lets an operator pause the cadence without
        // discarding their cron. Either gate off -> Disabled (manual still runs).
        match (self.env.backup_enabled, self.env.backup_cron.as_ref()) {
            (true, Some(schedule)) => Schedule::Cron(Box::new(schedule.clone())),
            _ => Schedule::Disabled,
        }
    }

    async fn run(&self, ctx: &TaskCtx) -> Result<TaskOutcome> {
        let cluster = ctx.env.cluster.get().await?;
        let bg_name = bg::bg_name(&ctx.env.kubectl, &cluster.namespace).await?;
        let stamp = Utc::now().format("%Y%m%d-%H%M%S").to_string();
        let backup_name = format!("{}-{}.backup", bg_name, stamp);

        if ctx.dry_run {
            ctx.log_info(&format!(
                "[dry-run] would invoke battlegroup backup name={backup_name}"
            ))?;
            return Ok(TaskOutcome::Done);
        }

        ctx.log_info(&format!(
            "starting backup bg={bg_name} ns={} name={backup_name}",
            cluster.namespace
        ))?;
        run_backup_and_verify(ctx, &bg_name, &backup_name).await?;
        ctx.log_info(&format!(
            "backup complete path=/funcom/artifacts/database-dumps/{bg_name}/{backup_name}"
        ))?;
        Ok(TaskOutcome::Done)
    }
}

pub async fn run_backup_and_verify(ctx: &TaskCtx, bg_name: &str, backup_name: &str) -> Result<()> {
    if let Err(err) = ctx.env.bg_cli.backup(backup_name).await {
        // #7: the vendor `battlegroup backup` runs the dump in a separate pod, so
        // its failure cause (OOMKilled, or pg_dump aborting on a non-`dune`-owned
        // table) isn't in the wrapper's output — leaving an opaque "backup failed"
        // and a "go run kubectl describe yourself" dead end. Best-effort: attach
        // the latest dump pod's state + log tail so the real cause is surfaced.
        let diag = match ctx.env.cluster.get().await {
            Ok(cluster) => dump_pod_diagnostics(&ctx.env.kubectl, &cluster.namespace).await,
            Err(_) => None,
        };
        return match diag {
            Some(detail) => Err(err.context(detail)),
            None => Err(err),
        };
    }

    let backup_path = format!("/funcom/artifacts/database-dumps/{bg_name}/{backup_name}");
    let stat = run_process("sudo", &["-n", "stat", "-c", "%s", &backup_path], None, 30)
        .await
        .with_context(|| format!("checking backup output {backup_path}"))?;
    stat.require_ok(&format!("stat backup {backup_path}"))?;

    let size = stat.stdout.trim().parse::<u64>().unwrap_or(0);
    if size == 0 {
        return Err(anyhow!("backup output is empty: {backup_path}"));
    }
    ctx.log_info(&format!("backup verified path={backup_path} bytes={size}"))?;

    let spec_path = format!("{backup_path}.yaml");
    let spec = run_process("sudo", &["-n", "test", "-f", &spec_path], None, 30)
        .await
        .with_context(|| format!("checking backup companion spec {spec_path}"))?;
    if spec.ok() {
        ctx.log_info(&format!("backup companion spec present path={spec_path}"))?;
    } else {
        ctx.log_warn(&format!("backup companion spec missing path={spec_path}"))?;
    }

    Ok(())
}

/// Best-effort diagnostics for a failed vendor backup (#7): summarize the most
/// recently-created dump pod's phase + container termination (reason/exitCode)
/// and append a short log tail. Returns the formatted detail, or None if no
/// dump pod or kubectl output is available. Never errors — diagnostics must not
/// mask the original backup failure.
async fn dump_pod_diagnostics(kubectl: &KubectlClient, namespace: &str) -> Option<String> {
    let pods = match kubectl
        .run(&["get", "pods", "-n", namespace, "-o", "json"])
        .await
    {
        Ok(out) if out.ok() => out,
        _ => return None,
    };
    let json: serde_json::Value = serde_json::from_str(&pods.stdout).ok()?;
    let (name, mut detail) = summarize_latest_dump_pod(&json)?;

    if let Ok(logs) = kubectl
        .run(&["logs", "-n", namespace, &name, "--tail=30"])
        .await
    {
        let tail = logs.stdout.trim();
        if !tail.is_empty() {
            detail.push_str("\n--- dump pod log tail ---\n");
            detail.push_str(tail);
        }
    }
    Some(detail)
}

/// Pure parse of `kubectl get pods -o json`: pick the most recently-created pod
/// whose name marks it as a dump pod (`-dump-`) and summarize its phase +
/// container termination. Returns `(pod_name, summary)`. Split out for testing.
fn summarize_latest_dump_pod(json: &serde_json::Value) -> Option<(String, String)> {
    let items = json.get("items")?.as_array()?;
    let mut latest_ts = "";
    let mut chosen: Option<&serde_json::Value> = None;
    let mut chosen_name = "";
    for pod in items {
        let name = pod
            .pointer("/metadata/name")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if !name.contains("-dump-") {
            continue;
        }
        let ts = pod
            .pointer("/metadata/creationTimestamp")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if chosen.is_none() || ts > latest_ts {
            latest_ts = ts;
            chosen = Some(pod);
            chosen_name = name;
        }
    }
    let pod = chosen?;
    let phase = pod
        .pointer("/status/phase")
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown");
    let mut detail = format!("latest dump pod {chosen_name} (phase={phase})");
    if let Some(statuses) = pod
        .pointer("/status/containerStatuses")
        .and_then(|v| v.as_array())
    {
        for cs in statuses {
            if let Some(term) = cs
                .pointer("/state/terminated")
                .or_else(|| cs.pointer("/lastState/terminated"))
            {
                let reason = term.get("reason").and_then(|v| v.as_str()).unwrap_or("");
                let code = term.get("exitCode").and_then(|v| v.as_i64()).unwrap_or(-1);
                detail.push_str(&format!(" terminated(reason={reason}, exitCode={code})"));
            }
        }
    }
    Some((chosen_name.to_string(), detail))
}

#[cfg(test)]
mod tests {
    use super::summarize_latest_dump_pod;

    #[test]
    fn picks_most_recent_dump_pod_and_surfaces_termination() {
        let json: serde_json::Value = serde_json::from_str(
            r#"{"items":[
              {"metadata":{"name":"bg-dump-20260609-pod","creationTimestamp":"2026-06-09T19:00:00Z"},
               "status":{"phase":"Failed","containerStatuses":[{"lastState":{"terminated":{"reason":"OOMKilled","exitCode":137}}}]}},
              {"metadata":{"name":"bg-dump-20260610-pod","creationTimestamp":"2026-06-10T03:00:00Z"},
               "status":{"phase":"Failed","containerStatuses":[{"state":{"terminated":{"reason":"Error","exitCode":1}}}]}},
              {"metadata":{"name":"bg-mq-game-sts-0","creationTimestamp":"2026-06-10T05:00:00Z"},
               "status":{"phase":"Running"}}
            ]}"#,
        )
        .unwrap();
        let (name, summary) = summarize_latest_dump_pod(&json).expect("a dump pod summary");
        // Most recent DUMP pod (03:00), not the newer non-dump mq pod.
        assert_eq!(name, "bg-dump-20260610-pod");
        assert!(summary.contains("exitCode=1"), "{summary}");
        assert!(summary.contains("Error"), "{summary}");
    }

    #[test]
    fn none_when_no_dump_pod_present() {
        let json: serde_json::Value = serde_json::from_str(
            r#"{"items":[{"metadata":{"name":"bg-mq-game-sts-0"},"status":{"phase":"Running"}}]}"#,
        )
        .unwrap();
        assert!(summarize_latest_dump_pod(&json).is_none());
    }
}
