use crate::{Engine, EngineError};
use nucleus::sync::{Activity, Direction, Instance, Outcome, Overview, Pending, Queue, Summary};
use std::{
    collections::BTreeMap,
    future::Future,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

type QueueReader = Box<dyn Fn() -> Queue + Send + Sync>;
type QueueReaders = Arc<Mutex<BTreeMap<String, QueueReader>>>;

pub struct QueueRegistration {
    queues: QueueReaders,
    id: String,
}

impl Drop for QueueRegistration {
    fn drop(&mut self) {
        self.queues.lock().unwrap().remove(&self.id);
    }
}

#[derive(Default)]
pub struct SyncService {
    save_history: AtomicBool,
    policy_lock: tokio::sync::Mutex<()>,
    running: Mutex<BTreeMap<String, Activity>>,
    history_error: Mutex<Option<String>>,
    queues: QueueReaders,
}

struct Running<'a> {
    service: &'a SyncService,
    id: String,
}

impl Drop for Running<'_> {
    fn drop(&mut self) {
        self.service.running.lock().unwrap().remove(&self.id);
    }
}

impl SyncService {
    pub(crate) fn new(save_history: bool) -> Self {
        Self {
            save_history: AtomicBool::new(save_history),
            ..Default::default()
        }
    }

    pub(crate) fn saves_history(&self) -> bool {
        self.save_history.load(Ordering::Acquire)
    }

    pub fn observe_queue(
        &self,
        read: impl Fn() -> Queue + Send + Sync + 'static,
    ) -> QueueRegistration {
        let id = nucleus::new_uid("sync-queue");
        self.queues
            .lock()
            .unwrap()
            .insert(id.clone(), Box::new(read));
        QueueRegistration {
            queues: self.queues.clone(),
            id,
        }
    }

    pub async fn run<T, E: std::fmt::Display>(
        &self,
        store: &store::Store,
        activity: Activity,
        work: impl Future<Output = Result<T, E>>,
        summarize: impl FnOnce(&T) -> Summary,
    ) -> Result<T, E> {
        let _service = crate::performance::Span::new("sync-service-await");
        let id = nucleus::new_uid("sync");
        self.running
            .lock()
            .unwrap()
            .insert(id.clone(), activity.clone());
        let _running = Running { service: self, id };
        let result = {
            let _work = crate::performance::Span::new("sync-work-await");
            work.await
        };
        if !self.saves_history() {
            return result;
        }
        let summary = {
            let _summary = crate::performance::Span::new("sync-summary");
            match &result {
                Ok(value) => summarize(value),
                Err(error) => Summary {
                    outcome: Outcome::Failed,
                    count: 0,
                    subjects: Vec::new(),
                    message: Some(error.to_string()),
                },
            }
        };
        if summary.count > 0
            || matches!(
                summary.outcome,
                Outcome::Failed | Outcome::Conflict | Outcome::Refreshed
            )
        {
            let _history = crate::performance::Span::new("sync-history-append-await");
            if let Err(error) = store::sync_activity::append(
                &store.pool,
                &activity,
                &summary,
                nucleus::execution::now().timestamp(),
            )
            .await
            {
                tracing::warn!(%error, "Could not save sync history");
                *self.history_error.lock().unwrap() = Some(error.to_string());
            } else {
                *self.history_error.lock().unwrap() = None;
            }
        }
        result
    }

    pub fn active(&self) -> Vec<Activity> {
        self.running.lock().unwrap().values().cloned().collect()
    }
}

impl Engine {
    pub async fn set_sync_history_policy(
        &self,
        policy: nucleus::sync::Retention,
    ) -> Result<(), EngineError> {
        let _guard = self.sync_service.policy_lock.lock().await;
        store::sync_activity::set_retention(
            &self.store.pool,
            policy,
            nucleus::execution::now().timestamp(),
        )
        .await?;
        self.sync_service
            .save_history
            .store(policy.enabled, Ordering::Release);
        if !policy.enabled {
            *self.sync_service.history_error.lock().unwrap() = None;
        }
        Ok(())
    }

    pub async fn sync_overview(&self, before: Option<i64>) -> Result<Overview, EngineError> {
        let now = nucleus::execution::now().timestamp();
        let retention = store::sync_activity::retention(&self.store.pool).await?;
        let history = store::sync_activity::recent(&self.store.pool, before, now).await?;
        let (mut pending, outgoing, mut held) =
            store::sync_activity::queues(&self.store.pool).await?;
        let (plugin_pending, plugin_outgoing, plugin_held) = self.plugin_sync_pending().await?;
        pending.extend(plugin_pending);
        held += plugin_held;
        let queues: Vec<_> = self
            .sync_service
            .queues
            .lock()
            .unwrap()
            .values()
            .map(|read| read())
            .collect();
        let incoming = queues.iter().map(|queue| queue.incoming).sum();
        let outgoing = outgoing
            + plugin_outgoing
            + queues.iter().map(|queue| queue.outgoing).sum::<u64>();
        {
            let conflicts = self.file_sync_conflicts.lock().unwrap();
            let mut organs: Vec<_> = conflicts.iter().collect();
            organs.sort_by_key(|(organ, _)| *organ);
            let mut remaining = 100usize;
            for (organ, conflicts) in organs {
                held += conflicts.len() as u64;
                for conflict in conflicts.iter().take(remaining) {
                    pending.push(Pending {
                        id: format!("file:{organ}:{}", conflict.path),
                        instance: Instance::files(
                            organ,
                            std::path::Path::new(&conflict.path)
                                .parent()
                                .unwrap_or(std::path::Path::new("")),
                        ),
                        direction: Direction::Both,
                        outcome: Outcome::Conflict,
                        subject: Some(conflict.path.clone()),
                        field: None,
                        attempts: 0,
                        message: Some(conflict.reason.clone()),
                    });
                }
                remaining = remaining.saturating_sub(conflicts.len());
            }
        }
        Ok(Overview {
            active: self.sync_service.active(),
            queues,
            history,
            pending,
            outgoing,
            incoming,
            held,
            retention,
            history_error: self.sync_service.history_error.lock().unwrap().clone(),
        })
    }
}
