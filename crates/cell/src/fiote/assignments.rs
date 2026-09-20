use super::*;
use fiote::config::TaskSession;
use serde_json::json;

#[derive(Default, Serialize, Deserialize)]
pub(super) struct Queue {
    cursor: i64,
    entries: Vec<Entry>,
}

#[derive(Clone, Serialize, Deserialize)]
struct Entry {
    assertion: String,
    fiote: String,
    task: String,
    thread: Option<String>,
    state: String,
    detail: String,
}

impl Queue {
    pub fn load(directory: &Path) -> Result<Self, String> {
        let mut queue: Self = match std::fs::read(directory.join("assignments.json")) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| e.to_string())?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(error) => return Err(error.to_string()),
        };
        for entry in &mut queue.entries {
            if entry.state == "running" {
                entry.state = "interrupted".into();
                entry.detail =
                    "The Cell stopped. Inspect completed changes before retrying.".into();
            }
        }
        Ok(queue)
    }
}

impl Host {
    pub(super) async fn stop_queued_assignments(&self) -> Result<(), String> {
        let mut queue = self.assignments.lock().await;
        for entry in &mut queue.entries {
            if matches!(entry.state.as_str(), "queued" | "waiting") {
                entry.state = "interrupted".into();
                entry.detail = "Stopped by you before starting.".into();
            }
        }
        save(&self.directory.join("assignments.json"), &*queue)
    }

    pub fn spawn_assignments(self: &Arc<Self>) -> tokio::task::JoinHandle<()> {
        let weak = Arc::downgrade(self);
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(std::time::Duration::from_secs(1));
            loop {
                tick.tick().await;
                let Some(host) = weak.upgrade() else { break };
                if let Err(error) = host.assignment_tick().await {
                    tracing::warn!(%error, "Fiote assignment dispatch failed");
                }
            }
        })
    }

    pub(super) async fn assignment_tick(&self) -> Result<(), String> {
        self.engine
            .access_scope(true, async {
                self.assignment_tick_inner()
                    .await
                    .map_err(engine::EngineError::Consequence)
            })
            .await
            .map_err(|e| e.to_string())
    }

    async fn assignment_tick_inner(&self) -> Result<(), String> {
        let mut queue = self.assignments.lock().await;
        let facts: Vec<(i64, String)> = store::sqlx::query_as(
            "SELECT rowid, uid FROM fact WHERE rowid > ? ORDER BY rowid LIMIT 256",
        )
        .bind(queue.cursor)
        .fetch_all(&self.engine.store.pool)
        .await
        .map_err(|e| e.to_string())?;
        let predicate = store::concepts::resolve(&self.engine.store.pool, "assigned-to")
            .await
            .map_err(|e| e.to_string())?;
        for (cursor, uid) in facts {
            if let Some(fact) = store::facts::get(&self.engine.store.pool, &uid)
                .await
                .map_err(|e| e.to_string())?
            {
                if matches!(
                    fact.cause.kind,
                    nucleus::CauseKind::UserEdit | nucleus::CauseKind::Action
                ) {
                    let value = fact
                        .payload
                        .as_deref()
                        .and_then(|v| serde_json::from_str::<serde_json::Value>(v).ok())
                        .unwrap_or_default();
                    if value["changed"] == true && value["operation"] == "add" {
                        if let Some(assertion) = value["assertion"].as_str() {
                            if !queue
                                .entries
                                .iter()
                                .any(|entry| entry.assertion == assertion)
                            {
                                if let Some(row) =
                                    store::assertions::get(&self.engine.store.pool, assertion)
                                        .await
                                        .map_err(|e| e.to_string())?
                                {
                                    if row.retracted_at.is_none()
                                        && predicate.as_deref() == Some(&row.predicate_uid)
                                    {
                                        if let Some(fiote) = row.object_uid {
                                            if self.behavior(&fiote).await?.run_assigned {
                                                queue.entries.push(Entry {
                                                    assertion: assertion.into(),
                                                    fiote,
                                                    task: row.subject_uid,
                                                    thread: None,
                                                    state: "queued".into(),
                                                    detail: String::new(),
                                                });
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            queue.cursor = cursor;
        }
        save(&self.directory.join("assignments.json"), &*queue)?;
        for index in 0..queue.entries.len() {
            let entry = queue.entries[index].clone();
            if entry.state == "running" {
                if let Some(thread) = entry.thread.as_ref() {
                    let assigned =
                        store::assertions::get(&self.engine.store.pool, &entry.assertion)
                            .await
                            .map_err(|e| e.to_string())?
                            .is_some_and(|row| row.retracted_at.is_none());
                    if !assigned || !self.behavior(&entry.fiote).await?.run_assigned {
                        if let Some(run) = self.running.lock().await.get(thread) {
                            let _ = run.stop.send(true);
                        }
                    }
                    if !self.running.lock().await.contains_key(thread) {
                        let messages = self.history(thread, &entry.fiote).await?;
                        let interrupted = messages
                            .iter()
                            .rev()
                            .find_map(|message| match message {
                                Message::Assistant { text, .. } => {
                                    Some(text.starts_with("[Interrupted turn]"))
                                }
                                _ => None,
                            })
                            .unwrap_or(false);
                        queue.entries[index].state = if interrupted {
                            "interrupted"
                        } else {
                            "finished"
                        }
                        .into();
                        queue.entries[index].detail = if interrupted {
                            "The turn stopped; inspect its messages before retrying."
                        } else {
                            "The model finished its turn. See the thread for its report."
                        }
                        .into();
                    }
                }
                continue;
            }
            if entry.state != "queued" && entry.state != "waiting" {
                continue;
            }
            let assertion = store::assertions::get(&self.engine.store.pool, &entry.assertion)
                .await
                .map_err(|e| e.to_string())?;
            if assertion.is_none_or(|row| row.retracted_at.is_some())
                || !self.behavior(&entry.fiote).await?.run_assigned
            {
                queue.entries[index].state = "cancelled".into();
                queue.entries[index].detail =
                    "Assignment removed or automatic work disabled.".into();
                continue;
            }
            let thread = if let Some(thread) = entry.thread {
                thread
            } else {
                let thread = self
                    .engine
                    .act(
                        Action::CreateThread {
                            target: entry.task.clone(),
                            head: String::new(),
                        },
                        None,
                    )
                    .await
                    .map_err(|e| e.to_string())?
                    .created
                    .ok_or("Could not create task thread.")?;
                self.engine
                    .act(
                        Action::AssertRecord {
                            subject: thread.clone(),
                            predicate: "thread-of".into(),
                            object: Some(entry.fiote.clone()),
                            quantity: None,
                            unit: None,
                        },
                        None,
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                self.engine.act(Action::SetExtension { target: thread.clone(), namespace: "lince.fiote-task".into(), fds: json!({"fiote":entry.fiote,"task":entry.task,"assignment":entry.assertion}) }, None).await.map_err(|e| e.to_string())?;
                queue.entries[index].thread = Some(thread.clone());
                save(&self.directory.join("assignments.json"), &*queue)?;
                thread
            };
            let config = self.load(&entry.fiote)?;
            let unavailable = match config {
                None => Some("Use /login on this Fiote to configure its connection."),
                Some(config) if !config.settings.enabled => {
                    Some("This Fiote's connection is disabled.")
                }
                Some(config) => {
                    let requires = config.agent.as_ref().map_or_else(
                        || {
                            self.catalog
                                .method(&config.settings)
                                .map_or(true, |method| {
                                    method.kind != fiote::adapters::AuthKind::None
                                })
                        },
                        |_| false,
                    );
                    if requires && self.vault.status().await?.1 {
                        Some("Unlock the provider vault to start this assignment.")
                    } else {
                        None
                    }
                }
            };
            if let Some(reason) = unavailable {
                if entry.detail != reason {
                    self.engine
                        .act(
                            Action::CreateMessage {
                                thread: thread.clone(),
                                body: format!("Waiting to start: {reason}"),
                                author: Some(entry.fiote.clone()),
                                state: MessageState::Finished,
                                parent: None,
                                references: Vec::new(),
                            },
                            None,
                        )
                        .await
                        .map_err(|e| e.to_string())?;
                }
                queue.entries[index].state = "waiting".into();
                queue.entries[index].detail = reason.into();
                continue;
            }
            if self.running.lock().await.len() >= 8 {
                continue;
            }
            queue.entries[index].state = "running".into();
            queue.entries[index].detail.clear();
            save(&self.directory.join("assignments.json"), &*queue)?;
            let message = format!(
                "Work on assigned task Record {}. Read its current description and properties, follow your inherited instructions, and report your progress and result in this thread.",
                entry.task
            );
            if let Err(error) = self
                .send(&thread, &message)
                .await
                .and_then(|outcome| outcome.ok_or("No enabled Fiote owns this session.".into()))
            {
                queue.entries[index].state = "failed".into();
                queue.entries[index].detail = error.clone();
                self.engine
                    .act(
                        Action::CreateMessage {
                            thread,
                            body: format!("Fiote could not start this assignment: {error}"),
                            author: Some(entry.fiote),
                            state: MessageState::Finished,
                            parent: None,
                            references: vec![],
                        },
                        None,
                    )
                    .await
                    .map_err(|e| e.to_string())?;
            }
        }
        save(&self.directory.join("assignments.json"), &*queue)
    }

    pub(super) async fn retry_assignment(&self, record: &str, thread: &str) -> Result<(), String> {
        let mut queue = self.assignments.lock().await;
        if self.running.lock().await.contains_key(thread) {
            return Err("This task is already running.".into());
        }
        let entry = queue
            .entries
            .iter_mut()
            .find(|entry| entry.fiote == record && entry.thread.as_deref() == Some(thread))
            .ok_or("No assignment owns this thread.")?;
        entry.state = "queued".into();
        entry.detail.clear();
        save(&self.directory.join("assignments.json"), &*queue)
    }

    pub(super) async fn task_sessions(&self, record: &str) -> Result<Vec<TaskSession>, String> {
        let entries = self.assignments.lock().await.entries.clone();
        let mut result = Vec::new();
        for entry in entries.into_iter().filter(|entry| entry.fiote == record) {
            if let Some(thread) = entry.thread {
                let title = self
                    .record(&entry.task)
                    .await
                    .map(|row| row.head)
                    .unwrap_or_else(|_| entry.task.clone());
                result.push(TaskSession {
                    thread,
                    task: entry.task,
                    title,
                    state: entry.state,
                    detail: entry.detail,
                });
            }
        }
        Ok(result)
    }
}
