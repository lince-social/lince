use engine::{
    Engine,
    actions::{Action, ActionOutcome},
};
use fiote::{
    config::{Request, Secret, Settings, Status},
    provider::{GenaiProvider, Message, Provider},
    tools::{CreateFile, Registry},
};
use nucleus::{MessageState, RecordKind};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::sync::{Mutex, watch};
use transport::fiote::Service;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Configuration {
    settings: Settings,
    key: Secret,
    author: String,
}

struct Running {
    record: String,
    stop: watch::Sender<bool>,
}

#[derive(Serialize, Deserialize)]
struct Pending {
    message: String,
}

pub struct Host {
    engine: Arc<Engine>,
    directory: PathBuf,
    running: Arc<Mutex<HashMap<String, Running>>>,
    provider: Option<Arc<dyn Provider>>,
}

impl Host {
    pub async fn open(engine: Arc<Engine>, directory: PathBuf) -> Result<Self, String> {
        std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        let host = Self {
            engine,
            directory,
            running: Default::default(),
            provider: None,
        };
        for entry in std::fs::read_dir(&host.directory).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            if entry.file_name().to_string_lossy().starts_with("turn-") {
                let pending: Pending = serde_json::from_slice(
                    &std::fs::read(entry.path()).map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())?;
                let state = store::records::get_extension(
                    &host.engine.store.pool,
                    &pending.message,
                    "lince.message",
                )
                .await
                .map_err(|e| e.to_string())?;
                if state.as_ref().and_then(|value| value["state"].as_str()) != Some("writing") {
                    std::fs::remove_file(entry.path()).map_err(|e| e.to_string())?;
                    continue;
                }
                host.engine.act(Action::ReviseMessage {
                    message: pending.message,
                    body: "Fiote was interrupted when the Cell stopped. File operations may have completed; check the configured folder before asking again.".into(),
                    state: MessageState::Interrupted,
                }, None).await.map_err(|e| e.to_string())?;
                std::fs::remove_file(entry.path()).map_err(|e| e.to_string())?;
            }
        }
        Ok(host)
    }

    fn path(&self, record: &str) -> Result<PathBuf, String> {
        if !nucleus::valid_uid(record, "r") {
            return Err("Fiote requires a Record UID.".into());
        }
        Ok(self.directory.join(format!("{record}.json")))
    }

    fn load(&self, record: &str) -> Result<Option<Configuration>, String> {
        match std::fs::read(self.path(record)?) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|_| "Cannot read this Fiote's settings.".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.to_string()),
        }
    }

    async fn record(&self, uid: &str) -> Result<store::records::RecordRow, String> {
        store::records::get(&self.engine.store.pool, uid)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "This Record no longer exists.".into())
    }

    async fn status(&self, record: &str) -> Result<Status, String> {
        self.record(record).await?;
        let config = self.load(record)?;
        let running = self
            .running
            .lock()
            .await
            .iter()
            .filter(|(_, run)| run.record == record)
            .map(|(thread, _)| thread.clone())
            .collect();
        Ok(Status {
            record: record.into(),
            has_key: config
                .as_ref()
                .is_some_and(|config| !config.key.0.is_empty()),
            settings: config.map(|config| config.settings).unwrap_or_default(),
            running,
        })
    }

    async fn configure(
        &self,
        record: &str,
        mut settings: Settings,
        key: Option<Secret>,
    ) -> Result<(), String> {
        let row = self.record(record).await?;
        let running = self.running.lock().await;
        if running.values().any(|run| run.record == record) {
            return Err("Stop this Fiote's running threads before changing its settings.".into());
        }
        let previous = self.load(record)?;
        if !settings.enabled {
            let Some(mut previous) = previous else {
                return Ok(());
            };
            previous.settings.enabled = false;
            return save(&self.path(record)?, &previous);
        }
        if settings.enabled {
            settings.validate()?;
        }
        let same_provider = previous.as_ref().is_some_and(|old| {
            old.settings.provider == settings.provider && old.settings.endpoint == settings.endpoint
        });
        let key = key
            .or_else(|| {
                previous
                    .as_ref()
                    .filter(|_| same_provider)
                    .map(|old| old.key.clone())
            })
            .unwrap_or_default();
        if key.0.len() > 16_384 {
            return Err("API key is too long.".into());
        }
        let author = if let Some(previous) = previous {
            previous.author
        } else if row.kind == RecordKind::Person.as_str() {
            row.uid
        } else {
            self.engine
                .act(
                    Action::CreateAgent {
                        head: format!("{} Fiote", row.head),
                        operated_by: None,
                    },
                    None,
                )
                .await
                .map_err(|e| e.to_string())?
                .created
                .ok_or("Could not create the Fiote author.")?
        };
        save(
            &self.path(record)?,
            &Configuration {
                settings,
                key,
                author,
            },
        )?;
        Ok(())
    }

    pub async fn stop_all(&self) {
        for run in self.running.lock().await.values() {
            let _ = run.stop.send(true);
        }
    }

    async fn history(&self, thread: &str, author: &str) -> Result<Vec<Message>, String> {
        let Some(predicate) = store::concepts::resolve(&self.engine.store.pool, "message-in")
            .await
            .map_err(|e| e.to_string())?
        else {
            return Ok(Vec::new());
        };
        let rows =
            store::assertions::recent_messages(&self.engine.store.pool, &predicate, thread, 257)
                .await
                .map_err(|e| e.to_string())?;
        if rows.len() > 256 {
            return Err("This session has reached 256 messages. Start a new thread.".into());
        }
        let mut messages = Vec::new();
        for row in rows.into_iter().rev() {
            let metadata =
                store::records::get_extension(&self.engine.store.pool, &row.uid, "lince.message")
                    .await
                    .map_err(|e| e.to_string())?;
            let assistant =
                metadata.as_ref().and_then(|value| value["author"].as_str()) == Some(author);
            let interrupted =
                metadata.as_ref().and_then(|value| value["state"].as_str()) == Some("interrupted");
            let body = if interrupted {
                format!("[Interrupted turn]\n{}", row.body)
            } else {
                row.body
            };
            messages.push(if assistant {
                Message::Assistant {
                    text: body,
                    calls: Vec::new(),
                }
            } else {
                Message::User(body)
            });
        }
        Ok(messages)
    }
}

fn save(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let mut file =
        tempfile::NamedTempFile::new_in(path.parent().ok_or("Settings directory is missing.")?)
            .map_err(|e| e.to_string())?;
    serde_json::to_writer(file.as_file_mut(), value).map_err(|e| e.to_string())?;
    file.flush()
        .and_then(|()| file.as_file().sync_all())
        .map_err(|e| e.to_string())?;
    file.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}

#[async_trait::async_trait]
impl Service for Host {
    async fn handle(&self, request: Request) -> Result<Status, String> {
        let record = match request {
            Request::Inspect { record } => record,
            Request::Configure {
                record,
                settings,
                api_key,
            } => {
                self.configure(&record, settings, api_key).await?;
                record
            }
            Request::Stop { thread } => {
                let running = self.running.lock().await;
                let run = running
                    .get(&thread)
                    .ok_or("No turn is running in this thread.")?;
                let _ = run.stop.send(true);
                run.record.clone()
            }
        };
        self.status(&record).await
    }

    async fn send(&self, thread: &str, body: &str) -> Result<Option<ActionOutcome>, String> {
        let mut running = self.running.lock().await;
        let Some(predicate) = store::concepts::resolve(&self.engine.store.pool, "thread-of")
            .await
            .map_err(|e| e.to_string())?
        else {
            return Ok(None);
        };
        let parents =
            store::assertions::objects_from_subject(&self.engine.store.pool, thread, &predicate)
                .await
                .map_err(|e| e.to_string())?;
        let mut selected = None;
        for record in parents {
            if let Some(config) = self.load(&record.uid)?
                && config.settings.enabled
            {
                if selected.is_some() {
                    return Err("This thread belongs to more than one enabled Fiote.".into());
                }
                selected = Some((record, config));
            }
        }
        let Some((record, config)) = selected else {
            return Ok(None);
        };
        if self.record(&config.author).await?.kind != RecordKind::Person.as_str() {
            return Err("Fiote's author must be a Person or Agent Record.".into());
        }
        if body.trim().is_empty() || body.len() > 65_536 {
            return Err("Write a message of at most 64 KiB.".into());
        }
        if running.contains_key(thread) {
            return Err(
                "Fiote is already working in this thread. Wait for its reply or press Stop.".into(),
            );
        }
        if running.len() >= 8 {
            return Err("Eight Fiote threads are already running.".into());
        }
        let mut messages = self.history(thread, &config.author).await?;
        messages.push(Message::User(body.into()));
        if record.body.len()
            + serde_json::to_vec(&messages)
                .map_err(|e| e.to_string())?
                .len()
            > fiote::runtime::MAX_CONTEXT_BYTES
        {
            return Err("This thread exceeds the context limit. Start a new thread.".into());
        }
        let mut tools = Registry::default();
        tools.register(CreateFile::new(&config.settings.directory)?);
        let provider: Arc<dyn Provider> = match &self.provider {
            Some(provider) => provider.clone(),
            None => Arc::new(GenaiProvider::new(&config.settings, &config.key)?),
        };
        let mut outcome = self
            .engine
            .act(
                Action::CreateMessage {
                    thread: thread.into(),
                    body: body.into(),
                    author: None,
                    state: MessageState::Finished,
                    parent: None,
                    references: Vec::new(),
                },
                None,
            )
            .await
            .map_err(|e| e.to_string())?;
        let reply = self
            .engine
            .act(
                Action::CreateMessage {
                    thread: thread.into(),
                    body: String::new(),
                    author: Some(config.author),
                    state: MessageState::Writing,
                    parent: outcome.created.clone(),
                    references: Vec::new(),
                },
                None,
            )
            .await;
        let reply = match reply {
            Ok(reply) => match reply.created {
                Some(uid) => uid,
                None => {
                    outcome
                        .warnings
                        .push("Message saved, but Fiote's reply could not be created.".into());
                    return Ok(Some(outcome));
                }
            },
            Err(error) => {
                outcome
                    .warnings
                    .push(format!("Message saved, but Fiote could not start: {error}"));
                return Ok(Some(outcome));
            }
        };
        let pending = self.directory.join(format!("turn-{thread}.json"));
        if let Err(error) = save(
            &pending,
            &Pending {
                message: reply.clone(),
            },
        ) {
            let body =
                format!("Fiote could not start because its session could not be saved: {error}");
            outcome.warnings.push(body.clone());
            if let Err(error) = self
                .engine
                .act(
                    Action::ReviseMessage {
                        message: reply,
                        body,
                        state: MessageState::Interrupted,
                    },
                    None,
                )
                .await
            {
                tracing::error!(%error, "Could not save Fiote's startup failure");
            }
            return Ok(Some(outcome));
        }
        let (stop, receiver) = watch::channel(false);
        running.insert(
            thread.into(),
            Running {
                record: record.uid,
                stop,
            },
        );
        let engine = self.engine.clone();
        let active = self.running.clone();
        let thread = thread.to_string();
        tokio::spawn(async move {
            let result =
                fiote::runtime::run(provider.as_ref(), &record.body, messages, &tools, receiver)
                    .await;
            let (body, state) = match result {
                Ok(body) => (body, MessageState::Finished),
                Err(error) => (format!("Fiote stopped: {error}"), MessageState::Interrupted),
            };
            match engine
                .act(
                    Action::ReviseMessage {
                        message: reply,
                        body,
                        state,
                    },
                    None,
                )
                .await
            {
                Ok(_) => {
                    let _ = std::fs::remove_file(pending);
                }
                Err(error) => tracing::error!(%error, "Could not save Fiote's reply"),
            }
            active.lock().await.remove(&thread);
        });
        Ok(Some(outcome))
    }
}
