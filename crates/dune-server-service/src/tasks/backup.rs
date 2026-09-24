use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use chrono::Utc;

use crate::kubectl::battlegroup as bg;
use crate::kubectl::run_process;
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

    fn exclusive(&self) -> bool {
        true
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
        let backup_path = run_backup_and_verify(ctx, &bg_name, &backup_name).await?;
        ctx.log_info(&format!("backup complete path={}", backup_path.display()))?;
        Ok(TaskOutcome::Done)
    }
}

pub async fn run_backup_and_verify(
    ctx: &TaskCtx,
    bg_name: &str,
    backup_name: &str,
) -> Result<PathBuf> {
    validate_backup_name(backup_name)?;
    let cluster = ctx.env.cluster.get().await?;
    let mut candidates = backup_candidates(bg_name, backup_name, None);
    match server_pvc_path(ctx, &cluster.namespace).await {
        Ok(Some(pvc_path)) => candidates = backup_candidates(bg_name, backup_name, Some(&pvc_path)),
        Ok(None) => ctx.log_warn("server PVC path unavailable; using legacy backup path")?,
        Err(err) => ctx.log_warn(&format!(
            "server PVC path lookup failed; using legacy backup path: {err:#}"
        ))?,
    }

    if let Some(path) = verify_candidates(ctx, &candidates).await? {
        ctx.log_info(&format!(
            "reusing existing verified backup path={}",
            path.display()
        ))?;
        return Ok(path);
    }

    ctx.env.bg_cli.backup(backup_name).await?;
    let backup_path = verify_candidates(ctx, &candidates).await?.ok_or_else(|| {
        anyhow!(
            "backup output not found in expected paths: {}",
            candidates
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )
    })?;

    verify_companion_spec(ctx, &candidates).await?;
    Ok(backup_path)
}

fn validate_backup_name(backup_name: &str) -> Result<()> {
    let path = Path::new(backup_name);
    if backup_name.is_empty()
        || backup_name.contains(['/', '\\'])
        || path.file_name().and_then(|name| name.to_str()) != Some(backup_name)
    {
        return Err(anyhow!("backup name must be a single file name"));
    }
    Ok(())
}

fn backup_candidates(bg_name: &str, backup_name: &str, pvc_path: Option<&Path>) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(pvc_path) = pvc_path {
        candidates.push(
            pvc_path
                .join("Saved")
                .join("DatabaseDumps")
                .join(backup_name),
        );
    }
    candidates.push(
        Path::new("/funcom/artifacts/database-dumps")
            .join(bg_name)
            .join(backup_name),
    );
    candidates
}

async fn server_pvc_path(ctx: &TaskCtx, namespace: &str) -> Result<Option<PathBuf>> {
    let pvc_result = ctx
        .env
        .kubectl
        .run(&[
            "get",
            "pvc",
            "-n",
            namespace,
            "-l",
            "role=igw-server",
            "-o",
            "json",
        ])
        .await?;
    pvc_result.require_ok("kubectl get server PVC")?;
    let pvc_doc: serde_json::Value =
        serde_json::from_str(&pvc_result.stdout).context("parsing server PVC JSON")?;
    let Some(pv_name) = pvc_doc
        .pointer("/items/0/spec/volumeName")
        .and_then(|value| value.as_str())
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };

    let pv_result = ctx
        .env
        .kubectl
        .run(&["get", "pv", pv_name, "-o", "json"])
        .await?;
    pv_result.require_ok(&format!("kubectl get PV {pv_name}"))?;
    let pv_doc: serde_json::Value =
        serde_json::from_str(&pv_result.stdout).context("parsing server PV JSON")?;
    Ok(extract_pv_path(&pv_doc))
}

fn extract_pv_path(pv_doc: &serde_json::Value) -> Option<PathBuf> {
    ["/spec/local/path", "/spec/hostPath/path"]
        .into_iter()
        .find_map(|pointer| {
            pv_doc
                .pointer(pointer)
                .and_then(|value| value.as_str())
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
        })
}

async fn verify_candidates(ctx: &TaskCtx, candidates: &[PathBuf]) -> Result<Option<PathBuf>> {
    for backup_path in candidates {
        let path = backup_path.to_string_lossy().into_owned();
        let stat = run_process("sudo", &["-n", "stat", "-c", "%s", &path], None, 30)
            .await
            .with_context(|| format!("checking backup output {}", backup_path.display()))?;
        if !stat.ok() {
            continue;
        }

        let size = stat.stdout.trim().parse::<u64>().unwrap_or(0);
        if size == 0 {
            return Err(anyhow!("backup output is empty: {}", backup_path.display()));
        }
        ctx.log_info(&format!(
            "backup verified path={} bytes={size}",
            backup_path.display()
        ))?;
        return Ok(Some(backup_path.clone()));
    }
    Ok(None)
}

async fn verify_companion_spec(ctx: &TaskCtx, candidates: &[PathBuf]) -> Result<()> {
    for backup_path in candidates {
        let spec_path = PathBuf::from(format!("{}.yaml", backup_path.display()));
        let path = spec_path.to_string_lossy().into_owned();
        let spec = run_process("sudo", &["-n", "test", "-f", &path], None, 30)
            .await
            .with_context(|| format!("checking backup companion spec {}", spec_path.display()))?;
        if spec.ok() {
            ctx.log_info(&format!(
                "backup companion spec present path={}",
                spec_path.display()
            ))?;
            return Ok(());
        }
    }

    ctx.log_warn("backup companion spec missing from all expected paths")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_local_or_host_path_from_pv() {
        let local = serde_json::json!({"spec": {"local": {"path": "/var/lib/data"}}});
        let host = serde_json::json!({"spec": {"hostPath": {"path": "/srv/data"}}});
        assert_eq!(
            extract_pv_path(&local),
            Some(PathBuf::from("/var/lib/data"))
        );
        assert_eq!(extract_pv_path(&host), Some(PathBuf::from("/srv/data")));
    }

    #[test]
    fn prefers_pvc_dump_path_and_keeps_legacy_fallback() {
        let paths = backup_candidates("world", "snapshot.backup", Some(Path::new("/var/lib/pvc")));
        assert_eq!(
            paths,
            [
                PathBuf::from("/var/lib/pvc/Saved/DatabaseDumps/snapshot.backup"),
                PathBuf::from("/funcom/artifacts/database-dumps/world/snapshot.backup"),
            ]
        );
    }

    #[test]
    fn rejects_backup_path_traversal() {
        assert!(validate_backup_name("snapshot.backup").is_ok());
        assert!(validate_backup_name("../snapshot.backup").is_err());
        assert!(validate_backup_name("nested/snapshot.backup").is_err());
    }
}
