use std::sync::Arc;

use chrono::Utc;
use chrono_tz::Tz;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

pub mod maintenance;
pub mod runner;
pub mod schedule;
pub mod task;
pub mod timezone;

pub use runner::TaskRunner;
pub use schedule::Schedule;
pub use task::{Task, TaskCtx, TaskOutcome};

use crate::store::TaskTrigger;

/// Owns the per-task tick loops. Each task gets one tokio task that sleeps
/// until its next scheduled fire and dispatches into [`TaskRunner::run`].
pub struct Scheduler {
    runner: Arc<TaskRunner>,
    tz: Tz,
    tasks: Vec<Arc<dyn Task>>,
    handles: Vec<JoinHandle<()>>,
    cancel: CancellationToken,
}

impl Scheduler {
    pub fn new(runner: Arc<TaskRunner>, tz: Tz) -> Self {
        Self {
            runner,
            tz,
            tasks: Vec::new(),
            handles: Vec::new(),
            cancel: CancellationToken::new(),
        }
    }

    pub fn add(&mut self, task: Arc<dyn Task>) {
        self.tasks.push(task);
    }

    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel.clone()
    }

    /// Spawn one loop per task. Returns once spawn is complete; the loops run
    /// in the background until [`Scheduler::shutdown`] is called.
    pub fn start(&mut self) {
        for task in self.tasks.clone() {
            let runner = self.runner.clone();
            let tz = self.tz;
            let cancel = self.cancel.clone();

            tracing::info!(
                task = task.id(),
                schedule = %task.schedule().describe(tz),
                "scheduling task"
            );

            if task.schedule().is_disabled() {
                // No tick loop. Manual triggers still resolve via build_all().
                continue;
            }

            let handle = tokio::spawn(async move {
                loop {
                    let next = task.schedule().next_fire(tz, Utc::now());
                    let wait = (next - Utc::now())
                        .to_std()
                        .unwrap_or(std::time::Duration::ZERO);
                    tracing::debug!(task = task.id(), next = %next, wait_ms = wait.as_millis() as u64, "next fire");

                    tokio::select! {
                        _ = cancel.cancelled() => {
                            tracing::info!(task = task.id(), "scheduler loop cancelled");
                            return;
                        }
                        _ = tokio::time::sleep(wait) => {}
                    }

                    if let Err(err) = runner
                        .run(task.clone(), TaskTrigger::Scheduled, false, None)
                        .await
                    {
                        tracing::error!(task = task.id(), error = %err, "scheduled run failed");
                    }
                }
            });

            self.handles.push(handle);
        }
    }

    pub async fn shutdown(&mut self) {
        self.cancel.cancel();
        for handle in self.handles.drain(..) {
            let _ = handle.await;
        }
    }
}
