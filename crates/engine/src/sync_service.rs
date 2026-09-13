use crate::{Engine, EngineError};
use nucleus::sync::{Activity, Direction, Instance, Outcome, Overview, Pending, Queue, Summary};
use std::{
    collections::BTreeMap,
    future::Future,
    sync::{Arc, Mutex},
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
        let id = nucleus::new_uid("sync");
        self.running
            .lock()
            .unwrap()
            .insert(id.clone(), activity.clone());
        let _running = Running { service: self, id };
        let result = work.await;
        let summary = match &result {
            Ok(value) => summarize(value),
            Err(error) => Summary {
                outcome: Outcome::Failed,
                count: 0,
                subjects: Vec::new(),
                message: Some(error.to_string()),
            },
        };
        if summary.count > 0
            || matches!(
                summary.outcome,
                Outcome::Failed | Outcome::Conflict | Outcome::Refreshed
            )
        {
            if let Err(error) = store::sync_activity::append(
                &store.pool,
                &activity,
                &summary,
                chrono::Utc::now().timestamp(),
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
    pub async fn sync_overview(&self, before: Option<i64>) -> Result<Overview, EngineError> {
        let now = chrono::Utc::now().timestamp();
        let retention = store::sync_activity::retention(&self.store.pool).await?;
        let history = store::sync_activity::recent(&self.store.pool, before, now).await?;
        let (mut pending, outgoing, mut held) =
            store::sync_activity::queues(&self.store.pool).await?;
        let queues: Vec<_> = self
            .sync_service
            .queues
            .lock()
            .unwrap()
            .values()
            .map(|read| read())
            .collect();
        let incoming = queues.iter().map(|queue| queue.incoming).sum();
        let outgoing = outgoing + queues.iter().map(|queue| queue.outgoing).sum::<u64>();
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
