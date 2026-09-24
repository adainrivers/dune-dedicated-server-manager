use std::sync::Mutex as StdMutex;

use tokio::sync::{Mutex, MutexGuard};

/// One-at-a-time gate for tasks that disrupt the BattleGroup (restart,
/// backup, update apply). Without it a scheduled restart could land in the
/// middle of an update, or a backup could run while every map server boots
/// (#37). Tasks queue on the gate instead of being skipped, so nothing that
/// was scheduled is silently lost.
pub struct MaintenanceGate {
    lock: Mutex<()>,
    holder: StdMutex<Option<&'static str>>,
}

/// Held while an exclusive task runs; releases the gate on drop.
pub struct MaintenanceSlot<'a> {
    _guard: MutexGuard<'a, ()>,
    holder: &'a StdMutex<Option<&'static str>>,
}

impl Drop for MaintenanceSlot<'_> {
    fn drop(&mut self) {
        *self.holder.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

impl MaintenanceGate {
    pub fn new() -> Self {
        Self {
            lock: Mutex::new(()),
            holder: StdMutex::new(None),
        }
    }

    /// Takes the gate if it is free; otherwise returns the id of the task
    /// currently holding it (when known).
    pub fn try_acquire(
        &self,
        task_id: &'static str,
    ) -> Result<MaintenanceSlot<'_>, Option<&'static str>> {
        match self.lock.try_lock() {
            Ok(guard) => Ok(self.slot(guard, task_id)),
            Err(_) => Err(*self.holder.lock().unwrap_or_else(|e| e.into_inner())),
        }
    }

    /// Waits until the gate is free, then takes it.
    pub async fn acquire(&self, task_id: &'static str) -> MaintenanceSlot<'_> {
        let guard = self.lock.lock().await;
        self.slot(guard, task_id)
    }

    fn slot<'a>(&'a self, guard: MutexGuard<'a, ()>, task_id: &'static str) -> MaintenanceSlot<'a> {
        *self.holder.lock().unwrap_or_else(|e| e.into_inner()) = Some(task_id);
        MaintenanceSlot {
            _guard: guard,
            holder: &self.holder,
        }
    }
}

impl Default for MaintenanceGate {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn second_task_sees_holder_then_gets_gate_after_release() {
        let gate = MaintenanceGate::new();
        let first = gate.try_acquire("update-apply").expect("gate free");
        assert_eq!(
            gate.try_acquire("restart").err(),
            Some(Some("update-apply"))
        );
        drop(first);
        let second = gate.try_acquire("restart").expect("gate released");
        assert_eq!(gate.try_acquire("backup").err(), Some(Some("restart")));
        drop(second);
    }

    #[tokio::test]
    async fn waiting_task_runs_after_holder_finishes() {
        let gate = std::sync::Arc::new(MaintenanceGate::new());
        let held = gate.try_acquire("backup").expect("gate free");
        let waiter = {
            let gate = gate.clone();
            tokio::spawn(async move {
                let _slot = gate.acquire("restart").await;
            })
        };
        tokio::task::yield_now().await;
        assert!(!waiter.is_finished());
        drop(held);
        waiter
            .await
            .expect("waiter completes once the gate is released");
    }
}
