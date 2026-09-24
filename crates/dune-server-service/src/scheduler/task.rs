use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

use crate::store::{LogLevel, NewLogEntry, Store, TaskTrigger};
use crate::tasks::TaskEnv;

use super::schedule::Schedule;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskOutcome {
    /// Task ran successfully and the run row should be retained.
    Done,
    /// Task decided there was nothing to do; the run row will be deleted so
    /// history is not polluted. Mirrors the exit-code-75 semantics of the
    /// original shell scripts.
    Noop,
}

#[async_trait]
pub trait Task: Send + Sync + 'static {
    fn id(&self) -> &'static str;
    fn schedule(&self) -> Schedule;
    async fn run(&self, ctx: &TaskCtx) -> Result<TaskOutcome>;

    /// True for tasks that disrupt the BattleGroup (restart, backup, update
    /// apply). The runner lets only one of them run at a time (#37).
    fn exclusive(&self) -> bool {
        false
    }
}

/// Context handed to each task invocation. Holds run-scoped state (run_id,
/// dry_run, trigger), a handle to the shared store, and the `TaskEnv` bundle
/// of external resources (kubectl, postgres, mq publisher, etc.).
#[derive(Clone)]
pub struct TaskCtx {
    pub run_id: i64,
    pub dry_run: bool,
    pub trigger: TaskTrigger,
    pub store: Store,
    pub env: Arc<TaskEnv>,
    /// Per-trigger overrides, populated from `POST /api/runs/trigger`'s
    /// optional `options` body. Tasks may inspect this to override their
    /// schedule-defaults; scheduled fires get `None`.
    pub options: Option<Value>,
}

impl TaskCtx {
    pub fn log_info(&self, message: &str) -> Result<()> {
        tracing::info!(task_run = self.run_id, "{}", message);
        self.store.log(&NewLogEntry {
            level: LogLevel::Info,
            message,
            task_id: None,
            run_id: Some(self.run_id),
        })
    }

    pub fn log_warn(&self, message: &str) -> Result<()> {
        tracing::warn!(task_run = self.run_id, "{}", message);
        self.store.log(&NewLogEntry {
            level: LogLevel::Warn,
            message,
            task_id: None,
            run_id: Some(self.run_id),
        })
    }

    pub fn log_error(&self, message: &str) -> Result<()> {
        tracing::error!(task_run = self.run_id, "{}", message);
        self.store.log(&NewLogEntry {
            level: LogLevel::Error,
            message,
            task_id: None,
            run_id: Some(self.run_id),
        })
    }
}
