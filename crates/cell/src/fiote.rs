use engine::{
    Engine,
    actions::{Action, ActionOutcome},
};
use fiote::{
    config::{Request, Secret, Settings, Status},
    provider::{Message, Provider},
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

mod activations;
mod agents;
mod assignments;
mod behavior;
mod connections;
mod context;
mod history;
mod mentions;
mod output;
mod profiles;
#[cfg(test)]
mod tests;
mod timeline;
mod usage;
pub(crate) mod vault;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Configuration {
    #[serde(default)]
    agent: Option<fiote::acp::Config>,
    settings: Settings,
    author: String,
}

struct Running {
    record: String,
    stop: watch::Sender<bool>,
}

#[derive(Serialize, Deserialize)]
struct Pending {
    #[serde(default)]
    root: Option<String>,
    message: String,
}

struct Login {
    record: String,
    url: String,
    settings: Settings,
    task: tokio::task::JoinHandle<Result<Secret, String>>,
}

pub struct Host {
    agents: agents::Agents,
    preparation: Mutex<()>,
    profile_lock: Mutex<()>,
    assignments: Mutex<assignments::Queue>,
    activation_dispatch: Mutex<()>,
    engine: Arc<Engine>,
    directory: PathBuf,
    running: Arc<Mutex<HashMap<String, Running>>>,
    provider: Option<Arc<dyn Provider>>,
    vault: Arc<vault::Vault>,
    login: Mutex<Option<Login>>,
    refresh_locks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
    catalog: fiote::adapters::Catalog,
    connections: Mutex<HashMap<String, connections::Opened>>,
}

impl Host {
    pub(crate) fn shared_vault(&self) -> Arc<vault::Vault> {
        self.vault.clone()
    }

    async fn cancel_login(&self) {
        if let Some(login) = self.login.lock().await.take() {
            login.task.abort();
        }
    }

    pub async fn open(engine: Arc<Engine>, directory: PathBuf) -> Result<Self, String> {
        std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        let host = Self {
            agents: Default::default(),
            preparation: Mutex::new(()),
            profile_lock: Mutex::new(()),
            assignments: Mutex::new(assignments::Queue::load(&directory)?),
            activation_dispatch: Mutex::new(()),
            vault: Arc::new(vault::Vault::new(engine.clone())),
            login: Mutex::new(None),
            refresh_locks: Default::default(),
            catalog: fiote::adapters::Catalog::load(&directory).await?,
            connections: Default::default(),
            engine,
            directory,
            running: Default::default(),
            provider: None,
        };
        for entry in std::fs::read_dir(&host.directory).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            if entry.file_name().to_string_lossy().starts_with("question-") {
                agents::questions::recover(&host.engine, &entry.path()).await?;
            }
            if entry.file_name().to_string_lossy().starts_with("turn-") {
                let pending: Pending = serde_json::from_slice(
                    &std::fs::read(entry.path()).map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())?;
                if let Some(root) = &pending.root {
                    if let Some(mut progress) = store::records::get_extension(
                        &host.engine.store.pool,
                        root,
                        "lince.message-progress",
                    )
                    .await
                    .map_err(|error| error.to_string())?
                    {
                        progress["state"] = "interrupted".into();
                        progress["updated_ms"] = nucleus::operation::now_ms().into();
                        host.engine
                            .act(
                                Action::SetExtension {
                                    target: root.clone(),
                                    namespace: "lince.message-progress".into(),
                                    fds: progress,
                                },
                                None,
                            )
                            .await
                            .map_err(|error| error.to_string())?;
                    }
                }
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
                host.engine.access_scope(true, async {
                    let row = store::records::get(&host.engine.store.pool, &pending.message).await?
                        .ok_or_else(|| engine::EngineError::UnknownRecord(pending.message.clone()))?;
                    let note = "Fiote was interrupted when the Cell stopped. Record and file operations may have completed; check their current state before asking again.";
                    host.engine.act(Action::ReviseMessage {
                        message: pending.message,
                        body: if row.body.is_empty() { note.into() } else { format!("{}\n\n{note}", row.body) },
                        state: MessageState::Interrupted,
                    }, None).await
                }).await.map_err(|e| e.to_string())?;
                std::fs::remove_file(entry.path()).map_err(|e| e.to_string())?;
            }
        }
        store::sqlx::query("UPDATE fiote_activation SET state = 'interrupted', detail = 'The Cell stopped. Inspect completed effects before requesting another run.' WHERE state = 'running'").execute(&host.engine.store.pool).await.map_err(|e| e.to_string())?;
        let records: Vec<String> = store::sqlx::query_scalar(
            "SELECT record_uid FROM record_extension WHERE namespace = 'lince.fiote'",
        )
        .fetch_all(&host.engine.store.pool)
        .await
        .map_err(|e| e.to_string())?;
        for record in records {
            host.engine.register_fiote_availability(
                &record,
                host.load(&record)?
                    .is_some_and(|config| config.settings.enabled),
            );
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
        self.engine.register_fiote_availability(
            record,
            config
                .as_ref()
                .is_some_and(|config| config.settings.enabled),
        );
        if config
            .as_ref()
            .is_none_or(|config| !config.settings.enabled)
        {
            store::sqlx::query("UPDATE fiote_activation SET state = 'cancelled', detail = 'Fiote disabled' WHERE fiote_uid = ? AND state IN ('queued','waiting')").bind(record).execute(&self.engine.store.pool).await.map_err(|e| e.to_string())?;
        }
        let running = self
            .running
            .lock()
            .await
            .iter()
            .filter(|(_, run)| run.record == record)
            .map(|(thread, _)| thread.clone())
            .collect();
        let (vault_exists, locked) = self.vault.status().await?;
        let (login_url, login_pending) = {
            let login = self.login.lock().await;
            (
                login
                    .as_ref()
                    .filter(|login| login.record == record)
                    .map(|login| login.url.clone()),
                login.as_ref().is_some_and(|login| login.record == record),
            )
        };
        let requires_credential = config.as_ref().is_some_and(|config| {
            config.agent.is_none()
                && config.settings.enabled
                && self
                    .catalog
                    .method(&config.settings)
                    .map_or(true, |method| {
                        method.kind != fiote::adapters::AuthKind::None
                    })
        });
        let instructions = self.prompt_sources(record).await;
        Ok(Status {
            connections: self.profile_status(record)?,
            activations: self
                .engine
                .fiote_activations(record)
                .await
                .map_err(|e| e.to_string())?
                .into_iter()
                .map(|activation| serde_json::to_value(activation).map_err(|e| e.to_string()))
                .collect::<Result<_, _>>()?,
            questions: self.agents.questions(record).await,
            usage: Vec::new(),
            agent_session: None,
            session: None,
            behavior: self.behavior(record).await?,
            instruction_error: instructions.as_ref().err().cloned(),
            instructions: instructions.unwrap_or_default(),
            fiotes: self.fiote_choices().await?,
            tasks: self.task_sessions(record).await?,
            agent: config.as_ref().and_then(|config| config.agent.clone()),
            agent_info: self.agents.info(record).await,
            agent_activity: self.agents.activity(record).await,
            tool_connections: self.tool_connections(record).await,
            record: record.into(),
            has_key: requires_credential && vault_exists,
            requires_credential,
            provider_diagnostics: self.catalog.diagnostics.clone(),
            providers: self.catalog.descriptors.clone(),
            vault_exists,
            locked,
            login_url,
            login_pending,
            settings: config.map(|config| config.settings).unwrap_or_default(),
            running,
        })
    }

    async fn configure(
        &self,
        record: &str,
        settings: Settings,
        key: Option<Secret>,
        password: Option<Secret>,
    ) -> Result<(), String> {
        self.engine
            .access_scope(true, async {
                self.configure_inner(record, settings, key, password)
                    .await
                    .map_err(engine::EngineError::Consequence)
            })
            .await
            .map_err(|e| e.to_string())
    }

    async fn configure_inner(
        &self,
        record: &str,
        mut settings: Settings,
        key: Option<Secret>,
        password: Option<Secret>,
    ) -> Result<(), String> {
        self.record(record).await?;
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
            self.catalog.validate(&mut settings)?;
        }
        self.prepare(record).await?;
        if let Some(password) = password {
            self.vault.unlock(password).await?;
        }
        if let Some(key) = key {
            if key.0.is_empty() || key.0.len() > 65_536 {
                return Err("Enter a credential of at most 64 KiB.".into());
            }
            self.vault.put(vault::slot(&settings), key).await?;
        }
        if self.catalog.method(&settings)?.kind != fiote::adapters::AuthKind::None
            && self.vault.key(&vault::slot(&settings)).await?.is_none()
        {
            return Err("Use /login to save a credential for this provider.".into());
        }
        let author = record.to_string();
        self.agents.close_record(record).await;
        save(
            &self.path(record)?,
            &Configuration {
                settings,
                author,
                agent: None,
            },
        )?;
        Ok(())
    }

    pub async fn stop_all(&self) {
        let result = self
            .engine
            .access_scope(true, async {
                self.stop_all_inner().await;
                Ok(())
            })
            .await;
        if let Err(error) = result {
            tracing::warn!(%error, "Could not stop Fiote");
        }
    }

    async fn stop_all_inner(&self) {
        if let Err(error) = store::sqlx::query("UPDATE fiote_activation SET state = 'cancelled', detail = 'Stopped by the person' WHERE state IN ('queued','waiting')").execute(&self.engine.store.pool).await {
            tracing::warn!(%error, "Could not stop pending activations");
        }
        if let Err(error) = self.stop_queued_assignments().await {
            tracing::warn!(%error, "Could not stop queued assignments");
        }
        let running = self.running.lock().await;
        for run in running.values() {
            let _ = run.stop.send(true);
        }
        self.agents.close_all().await;
        for (_, open) in self.connections.lock().await.drain() {
            open.connection.close().await;
        }
        self.cancel_login().await;
        self.vault.lock().await;
        drop(running);
    }

    async fn history(&self, thread: &str, author: &str) -> Result<Vec<Message>, String> {
        let Some(predicate) = store::concepts::resolve(&self.engine.store.pool, "message-in")
            .await
            .map_err(|e| e.to_string())?
        else {
            return Ok(Vec::new());
        };
        let rows =
            store::assertions::recent_messages(&self.engine.store.pool, &predicate, thread, 100001)
                .await
                .map_err(|e| e.to_string())?;
        if rows.len() > 100000 {
            return Err("This conversation exceeds 100,000 Messages. Start another conversation; its original history remains saved.".into());
        }
        let mut messages = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for row in rows.into_iter().rev() {
            seen.insert(row.uid.clone());
            let metadata =
                store::records::get_extension(&self.engine.store.pool, &row.uid, "lince.message")
                    .await
                    .map_err(|e| e.to_string())?;
            let assistant =
                metadata.as_ref().and_then(|value| value["author"].as_str()) == Some(author);
            let interrupted =
                metadata.as_ref().and_then(|value| value["state"].as_str()) == Some("interrupted");
            if assistant
                && let Some(frames) = self
                    .restore_frames(&row.uid, &row.body, &messages, &seen)
                    .await?
            {
                messages.extend(frames);
                continue;
            }
            let body = if interrupted {
                format!("[Interrupted turn]\n{}", row.body)
            } else {
                row.body
            };
            let content = store::message_content::load(&self.engine.store.pool, &row.uid)
                .await
                .map_err(|error| error.to_string())?;
            messages.push(if assistant && !content.is_empty() {
                Message::RichAssistant {
                    text: body,
                    content,
                }
            } else if assistant {
                Message::Assistant {
                    text: body,
                    calls: Vec::new(),
                }
            } else {
                if content.is_empty() {
                    Message::User(body)
                } else {
                    Message::RichUser {
                        text: body,
                        content,
                    }
                }
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
    async fn terminal(
        &self,
        request: fiote::acp::terminal::TerminalRequest,
    ) -> Result<fiote::acp::terminal::TerminalFrame, String> {
        self.login_terminal(request).await
    }
    async fn handle(&self, request: Request) -> Result<Status, String> {
        let inspected = match &request {
            Request::InspectThread { thread }
            | Request::RefreshInstructions { thread, .. }
            | Request::SessionOptions { thread }
            | Request::SessionReset { thread }
            | Request::SessionSetOption { thread, .. }
            | Request::SessionDirectories { thread, .. } => Some(thread.clone()),
            _ => None,
        };
        let record = match request {
            Request::Delete { record } => {
                if !self
                    .running
                    .lock()
                    .await
                    .values()
                    .all(|run| run.record != record)
                {
                    return Err("Stop this Fiote before deleting it.".into());
                }
                let choices = self.fiote_choices().await?;
                if choices.len() <= 1 {
                    return Err("Create another Fiote before deleting the last one.".into());
                }
                for choice in &choices {
                    if self
                        .engine
                        .fiote_parent(&choice.record)
                        .await
                        .map_err(|error| error.to_string())?
                        .as_deref()
                        == Some(&record)
                    {
                        return Err("Choose another parent for this Fiote's descendants before deleting it.".into());
                    }
                }
                self.agents.close_record(&record).await;
                self.engine
                    .act(Action::DeleteRecord { target: record }, None)
                    .await
                    .map_err(|error| error.to_string())?;
                self.fiote_choices()
                    .await?
                    .first()
                    .ok_or("No Fiotes remain. Create a Fiote to continue.")?
                    .record
                    .clone()
            }
            Request::Directory => self.directory().await?,
            Request::Create { head } => {
                let record = self
                    .engine
                    .act(
                        Action::CreateAgent {
                            head,
                            operated_by: None,
                        },
                        None,
                    )
                    .await
                    .map_err(|error| error.to_string())?
                    .created
                    .ok_or("Could not create Fiote.")?;
                self.prepare(&record).await?;
                record
            }

            Request::InspectThread { thread } => self.thread_fiote(&thread).await?,
            Request::Activate {
                record,
                value,
                request_id,
            } => {
                self.engine
                    .act(
                        Action::ActivateFiote {
                            target: record.clone(),
                            value,
                            request_id,
                        },
                        None,
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                record
            }
            Request::CancelActivations { record } => {
                store::sqlx::query("UPDATE fiote_activation SET state = 'cancelled', detail = 'Stopped before starting' WHERE fiote_uid = ? AND state IN ('queued','waiting')").bind(&record).execute(&self.engine.store.pool).await.map_err(|e| e.to_string())?;
                for run in self
                    .running
                    .lock()
                    .await
                    .values()
                    .filter(|run| run.record == record)
                {
                    let _ = run.stop.send(true);
                }
                record
            }
            Request::Behavior {
                record,
                prompt_parent,
                run_assigned,
            } => {
                self.prepare(&record).await?;
                self.engine
                    .act(
                        Action::ConfigureFiote {
                            target: record.clone(),
                            prompt_parent,
                            run_assigned,
                        },
                        None,
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                record
            }
            Request::RefreshInstructions { record, thread } => {
                let record = self.session_owner(&record, &thread).await?;
                self.refresh_instructions(&record, &thread).await?;
                record
            }
            Request::RetryAssignment { record, thread } => {
                self.retry_assignment(&record, &thread).await?;
                record
            }
            Request::SessionReset { thread } => self.reset_session(&thread).await?,
            Request::SessionOptions { thread } => self.open_session_options(&thread).await?,
            Request::SessionDirectories {
                thread,
                directories,
            } => self.set_session_directories(&thread, directories).await?,
            Request::SessionSetOption {
                thread,
                option,
                value,
            } => self.set_session_option(&thread, &option, &value).await?,
            Request::AgentDiscover { record, config } => {
                self.discover_agent(&record, config).await?;
                record
            }
            Request::AgentConfigure { record, config } => {
                self.configure_agent(&record, config).await?;
                self.remember_connection_configuration(&record).await?;
                record
            }
            Request::AgentCheck { record, config } => {
                self.check_agent(&record, config).await?;
                record
            }
            Request::AgentCancelLogin { record } => {
                self.cancel_agent_login(&record).await?;
                record
            }
            Request::AgentOptions { record, config } => {
                self.agent_options(&record, config).await?;
                self.remember_connection_configuration(&record).await?;
                record
            }
            Request::AgentSetOption {
                record,
                option,
                value,
            } => {
                self.set_agent_option(&record, &option, &value).await?;
                self.remember_connection_configuration(&record).await?;
                record
            }
            Request::AgentLogout { record } => {
                self.logout_agent(&record).await?;
                record
            }
            Request::AgentAuthenticate { record, method } => {
                self.authenticate_agent(&record, &method).await?;
                record
            }
            Request::AgentProvider { record, provider } => {
                self.select_agent_provider(&record, &provider).await?;
                self.remember_connection_configuration(&record).await?;
                record
            }
            Request::AgentProviderLogin {
                record,
                fields,
                password,
            } => {
                self.agent_provider_login(&record, fields, password).await?;
                record
            }
            Request::AgentQuestionAnswer {
                record,
                request,
                answer,
            } => {
                self.record(&record).await?;
                self.agents
                    .answer_question(&record, &request, answer)
                    .await?;
                record
            }
            Request::AgentPermission {
                record,
                thread,
                request,
                option,
            } => {
                self.agents
                    .answer(&record, &thread, &request, option)
                    .await?;
                record
            }
            Request::OpenTools { record, thread } => {
                let record = self.session_owner(&record, &thread).await?;
                self.open_tools(&record, &thread).await?;
                record
            }
            Request::CloseTools { record, thread } => {
                let record = self.session_owner(&record, &thread).await?;
                let open = {
                    let mut connections = self.connections.lock().await;
                    if connections
                        .get(&thread)
                        .is_some_and(|connection| connection.record == record)
                    {
                        connections.remove(&thread)
                    } else {
                        None
                    }
                };
                if let Some(open) = open {
                    open.connection.close().await;
                }
                record
            }
            Request::Prepare { record } => {
                self.prepare(&record).await?;
                record
            }
            Request::Unlock { record, password } => {
                self.record(&record).await?;
                self.vault.unlock(password).await?;
                record
            }
            Request::Lock { record } => {
                self.stop_all().await;
                self.cancel_login().await;
                self.vault.lock().await;
                record
            }
            Request::BrowserStart {
                record,
                password,
                mut settings,
            } => {
                self.record(&record).await?;
                settings.enabled = true;
                self.catalog.validate(&mut settings)?;
                if self.catalog.method(&settings)?.kind != fiote::adapters::AuthKind::Browser {
                    return Err("This provider does not advertise browser login.".into());
                }
                let mut login = self.login.lock().await;
                if login.is_some() {
                    return Err(
                        "A browser login is already pending. Cancel it or finish it first.".into(),
                    );
                }
                self.vault.unlock(password).await?;
                let (url, task) = if fiote::communication::native::has_native_login(&settings) {
                    let previous = if settings.account.is_empty() {
                        None
                    } else {
                        self.vault.key(&vault::slot(&settings)).await?
                    };
                    let attempt = fiote::communication::native::login(
                        &mut settings,
                        &self.directory,
                        previous.as_ref(),
                    )
                    .await?;
                    (attempt.url.clone(), tokio::spawn(attempt.finish()))
                } else {
                    let mut connection = self
                        .catalog
                        .driver(&settings.provider)
                        .ok_or("This provider has no browser login adapter.")?
                        .connect()
                        .await?;
                    let url = connection.login(&settings).await?;
                    (url, tokio::spawn(connection.finish_login()))
                };
                *login = Some(Login {
                    record: record.clone(),
                    url,
                    settings,
                    task,
                });
                record
            }
            Request::BrowserPoll { record } => {
                let pending = {
                    let mut login = self.login.lock().await;
                    if login
                        .as_ref()
                        .is_some_and(|login| login.record == record && login.task.is_finished())
                    {
                        login.take()
                    } else {
                        None
                    }
                };
                if let Some(pending) = pending {
                    let key = pending.task.await.map_err(|_| "Browser login stopped.")??;
                    let mut settings = pending.settings;
                    if fiote::communication::native::has_native_login(&settings) {
                        self.configure(&record, settings.clone(), Some(key.clone()), None)
                            .await?;
                        self.save_native_profile(&record, &settings).await?;
                        if !fiote::communication::native::account_ready(&settings, &key)? {
                            return Err("Signed in to ChatGPT; the registration is saved, but plan usage was not granted. Choose Sign in again to enable it.".into());
                        }
                    }
                    if fiote::communication::native::has_native_login(&settings)
                        && settings.model.is_empty()
                    {
                        let provider = fiote::communication::native::provider(
                            &settings,
                            &key,
                            Some(Arc::new(vault::Credential {
                                vault: self.vault.clone(),
                                directory: self.directory.clone(),
                                slot: vault::slot(&settings),
                            })),
                            None,
                        )?;
                        let model = provider
                            .models()
                            .await?
                            .into_iter()
                            .next()
                            .ok_or("No model is available to this ChatGPT account.")?;
                        settings.model = model.id;
                        settings.context_budget_bytes = model.context_budget_bytes;
                    }
                    self.configure(&record, settings.clone(), Some(key), None)
                        .await?;
                    self.save_native_profile(&record, &settings).await?;
                }
                record
            }
            Request::BrowserCancel { record } => {
                self.cancel_login().await;
                record
            }
            Request::Connections { record, request } => {
                return self.connection_request(&record, request).await;
            }
            Request::Inspect { record } => record,
            Request::Configure {
                record,
                settings,
                api_key,
                password,
            } => {
                self.configure(&record, settings, api_key, password).await?;
                self.remember_connection_configuration(&record).await?;
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
        let mut status = self.status(&record).await?;
        if let Some(thread) = inspected {
            status.agent_session = self.agent_session_status(&thread).await?;
            status.usage = usage::read(&usage::path(&self.directory, &thread))?;
            if let Some(snapshot) = self.instruction_snapshot(&thread)? {
                let sources: Vec<fiote::config::PromptSource> =
                    serde_json::from_value(snapshot["sources"].clone())
                        .map_err(|e| e.to_string())?;
                status.session = Some(fiote::config::PromptSession {
                    context: self.inspect_context(&thread).await?,
                    thread,
                    changed: sources != status.instructions,
                    sources,
                });
            }
        }
        Ok(status)
    }

    async fn send(&self, thread: &str, body: &str) -> Result<Option<ActionOutcome>, String> {
        self.engine
            .access_scope(true, async {
                Box::pin(self.start_turn(thread, body, &[]))
                    .await
                    .map_err(engine::EngineError::Consequence)
            })
            .await
            .map_err(|error| error.to_string())
    }
    async fn send_content(
        &self,
        thread: &str,
        body: &str,
        content: &[nucleus::message::MessagePart],
    ) -> Result<Option<ActionOutcome>, String> {
        self.engine
            .access_scope(true, async {
                Box::pin(self.start_turn(thread, body, content))
                    .await
                    .map_err(engine::EngineError::Consequence)
            })
            .await
            .map_err(|error| error.to_string())
    }
}

impl Host {
    async fn start_turn(
        &self,
        thread: &str,
        body: &str,
        content: &[nucleus::message::MessagePart],
    ) -> Result<Option<ActionOutcome>, String> {
        if !nucleus::valid_uid(thread, "r") {
            return Err("Choose a valid thread identifier.".into());
        }
        nucleus::message::validate(content)?;
        if body.len() > 65_536 {
            return Err("Write a message of at most 64 KiB.".into());
        }
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
        let task =
            store::records::get_extension(&self.engine.store.pool, thread, "lince.fiote-task")
                .await
                .map_err(|e| e.to_string())?;
        let mentioned = self.mentioned_fiote(body).await?;
        let is_mention = mentioned.is_some();
        let mut selected = mentioned;
        for record in parents {
            if is_mention {
                break;
            }
            if task
                .as_ref()
                .and_then(|task| task["fiote"].as_str())
                .is_some_and(|uid| uid != record.uid)
            {
                continue;
            }
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
        if self.record(thread).await?.kind != RecordKind::Thread.as_str() {
            return Err("Choose a thread for this conversation.".into());
        }
        if self.record(&config.author).await?.kind != RecordKind::Person.as_str() {
            return Err("Fiote's author must be a Person or Agent Record.".into());
        }
        if body.trim_start().starts_with("/login") {
            return Err(
                "Use the local /login picker; credentials must never be sent as messages.".into(),
            );
        }
        if (body.trim().is_empty() && content.is_empty()) || body.len() > 65_536 {
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
        self.prepare_mentioned_session(&record.uid, thread, is_mention)
            .await?;
        if let Some(agent) = config.agent {
            return Box::pin(self.start_agent_turn(
                record,
                config.author,
                agent,
                thread,
                body,
                content,
                &mut running,
            ))
            .await
            .map(Some);
        }
        let mut messages = self.history(thread, &config.author).await?;
        messages.push(if content.is_empty() {
            Message::User(body.into())
        } else {
            Message::RichUser {
                text: body.into(),
                content: content.to_vec(),
            }
        });
        let system = self.session_instructions(&record.uid, thread).await?;
        let mut tools = Registry::default();
        if !config.settings.directory.as_os_str().is_empty() {
            tools.register(CreateFile::new(&config.settings.directory)?);
        }
        let native = transport::Session::local(
            self.engine.clone(),
            Arc::new(transport::LaneHub::new()),
            format!("fiote-{thread}"),
        )
        .into_native_tools(transport::native::Context {
            agent: config.author.clone(),
            record: record.uid.clone(),
            thread: thread.into(),
        })
        .with_instructions(system.clone());
        native.register(&mut tools);
        self.save_context(thread, &record.uid, &[], &tools).await?;
        let driver = self.catalog.driver(&config.settings.provider).cloned();
        let key = if self.catalog.method(&config.settings)?.kind == fiote::adapters::AuthKind::None
        {
            Secret::default()
        } else {
            self.vault
                .key(&vault::slot(&config.settings))
                .await?
                .ok_or("Use /login to configure this provider.")?
        };
        let adapter = driver.map(|driver| {
            Arc::new(fiote::driver::DriverProvider::new(
                driver,
                config.settings.clone(),
                key.clone(),
            ))
        });
        let provider: Arc<dyn Provider> = match (&self.provider, &adapter) {
            (Some(provider), _) => provider.clone(),
            (_, Some(provider)) => provider.clone(),
            _ => {
                let slot = vault::slot(&config.settings);
                let lock = self
                    .refresh_locks
                    .lock()
                    .await
                    .entry(slot.clone())
                    .or_default()
                    .clone();
                fiote::communication::native::provider(
                    &config.settings,
                    &key,
                    Some(Arc::new(vault::Credential {
                        vault: self.vault.clone(),
                        directory: self.directory.clone(),
                        slot,
                    })),
                    Some(lock),
                )?
            }
        };
        for message in &messages {
            if let Message::RichUser { content, .. } | Message::RichAssistant { content, .. } =
                message
            {
                provider.validate_content(content)?;
            }
        }
        let original_messages = messages.clone();
        let messages = self.summarized_history(thread, &messages)?;
        let mut outcome = self
            .engine
            .act(
                Action::CreateMessage {
                    content: content.to_vec(),
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
                    content: Vec::new(),
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
                root: None,
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
        let source_message = outcome.created.clone();
        let journal_path = history::path(&self.directory, &reply);
        if let Err(error) = save(
            &journal_path,
            &history::Journal {
                source: source_message.clone(),
                ..Default::default()
            },
        ) {
            let body = format!("Fiote could not start its native journal: {error}");
            outcome.warnings.push(body.clone());
            self.engine
                .act(
                    Action::ReviseMessage {
                        message: reply,
                        body,
                        state: MessageState::Interrupted,
                    },
                    None,
                )
                .await
                .map_err(|e| e.to_string())?;
            return Ok(Some(outcome));
        }
        let summary_path = self.directory.join(format!("summary-{thread}.json"));
        let (stop, receiver) = watch::channel(false);
        running.insert(
            thread.into(),
            Running {
                record: record.uid,
                stop,
            },
        );
        let active = self.running.clone();
        let thread = thread.to_string();
        let vault = self.vault.clone();
        let slot = vault::slot(&config.settings);
        let attached = native.attach_message(&reply).await;
        let usage_path = usage::path(&self.directory, &thread);
        let context_path = self.directory.join(format!("context-{thread}.json"));
        native.set_source_message(outcome.created.as_deref());
        tokio::spawn(async move {
            let output = output::Output {
                journal_path: Some(journal_path),
                summary_path: Some(summary_path),
                original_messages,
                context_path: Some(context_path),
                source_message,
                usage_path: Some(usage_path),
                tools: &tools,
                message: &reply,
                text: Default::default(),
            };
            let result = match attached {
                Ok(()) => {
                    fiote::runtime::run_streamed(
                        provider.as_ref(),
                        &system,
                        messages,
                        &tools,
                        receiver,
                        &output,
                    )
                    .await
                }
                Err(error) => Err(error),
            };
            let refreshed = if let Some(adapter) = adapter {
                vault.refreshed(slot, adapter.credential().await, key).await
            } else {
                Ok(())
            };
            let (mut body, state) = match result {
                Ok(body) => (body, MessageState::Finished),
                Err(error) => {
                    let partial = output.text.lock().await.clone();
                    (
                        if partial.is_empty() {
                            format!("Fiote stopped: {error}")
                        } else {
                            format!("{partial}\n\nFiote stopped: {error}")
                        },
                        MessageState::Interrupted,
                    )
                }
            };
            if let Err(error) = refreshed {
                body.push_str(&format!("\n\nLogin could not be saved: {error}"));
            }
            match output.finish(&body, state).await {
                Ok(_) => {
                    let _ = std::fs::remove_file(pending);
                }
                Err(error) => tracing::error!(%error, "Could not save Fiote's reply"),
            }
            native.close().await;
            active.lock().await.remove(&thread);
        });
        Ok(Some(outcome))
    }
}
