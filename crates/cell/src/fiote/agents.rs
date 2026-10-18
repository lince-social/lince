use super::*;
use fiote::{acp, config::AgentActivity, provider::TextOutput};
use serde_json::{Value, json};
use tokio::sync::oneshot;

mod check;
mod live;
pub(super) mod login;
mod options;
pub(super) mod questions;

struct Runtime {
    record: String,
    connection: Arc<acp::Connection>,
    session: String,
    config: acp::Config,
    fresh: std::sync::atomic::AtomicBool,
    settings: Mutex<()>,
    tools: transport::native::NativeTools,
    _server: transport::mcp::Connection,
}

struct PendingPermission {
    record: String,
    thread: String,
    request: acp::Permission,
    response: oneshot::Sender<Option<String>>,
}

#[derive(Default)]
pub(super) struct Agents {
    terminals: Mutex<HashMap<String, (String, Arc<acp::terminal::LoginTerminal>)>>,
    sessions: Arc<Mutex<HashMap<String, Arc<Runtime>>>>,
    pub info: Arc<Mutex<HashMap<String, Value>>>,
    discovery: Mutex<HashMap<String, (acp::Config, Arc<acp::Connection>)>>,
    permissions: Arc<Mutex<HashMap<String, PendingPermission>>>,
    activity: Arc<Mutex<HashMap<String, (String, String)>>>,
    options: Mutex<HashMap<String, options::Preview>>,
}

impl Agents {
    pub async fn close_thread(&self, thread: &str) {
        if let Some(runtime) = self.sessions.lock().await.remove(thread) {
            runtime.connection.close();
            runtime._server.close().await;
        }
    }

    pub async fn info(&self, record: &str) -> Option<Value> {
        self.info.lock().await.get(record).cloned()
    }

    pub async fn activity(&self, record: &str) -> Vec<AgentActivity> {
        let mut activity: Vec<_> = self
            .activity
            .lock()
            .await
            .iter()
            .filter(|(_, (owner, _))| owner == record)
            .map(|(thread, (_, title))| AgentActivity {
                thread: thread.clone(),
                title: title.clone(),
                permission: None,
            })
            .collect();
        for pending in self
            .permissions
            .lock()
            .await
            .values()
            .filter(|pending| pending.record == record)
        {
            activity.push(AgentActivity {
                thread: pending.thread.clone(),
                title: pending.request.title.clone(),
                permission: Some(pending.request.clone()),
            });
        }
        activity
    }

    pub async fn answer(
        &self,
        record: &str,
        thread: &str,
        id: &str,
        option: Option<String>,
    ) -> Result<(), String> {
        let mut pending = self.permissions.lock().await;
        let request = pending
            .get(id)
            .ok_or("This permission request is no longer waiting.")?;
        if request.record != record
            || request.thread != thread
            || option.as_ref().is_some_and(|id| {
                !request
                    .request
                    .options
                    .iter()
                    .any(|choice| &choice.id == id)
            })
        {
            return Err("This choice does not belong to this thread's permission request.".into());
        }
        let request = pending.remove(id).unwrap();
        let _ = request.response.send(option);
        Ok(())
    }

    pub async fn close_record(&self, record: &str) {
        if let Some((_, terminal)) = self.terminals.lock().await.remove(record) {
            terminal.close();
        }
        self.clear_options(record).await;
        let removed: Vec<_> = {
            let mut sessions = self.sessions.lock().await;
            let keys: Vec<_> = sessions
                .iter()
                .filter(|(_, session)| session.record == record)
                .map(|(id, _)| id.clone())
                .collect();
            keys.into_iter()
                .filter_map(|id| sessions.remove(&id))
                .collect()
        };
        for runtime in removed {
            runtime.connection.close();
            runtime._server.close().await;
        }
        self.permissions
            .lock()
            .await
            .retain(|_, request| request.record != record);
        self.activity
            .lock()
            .await
            .retain(|_, (owner, _)| owner != record);
    }

    pub async fn close_all(&self) {
        for (_, (_, terminal)) in self.terminals.lock().await.drain() {
            terminal.close();
        }
        for (_, preview) in self.options.lock().await.drain() {
            preview.connection.close();
        }
        for info in self
            .info
            .lock()
            .await
            .values_mut()
            .filter_map(Value::as_object_mut)
        {
            info.remove("configOptions");
            info.remove("connectionCheck");
            info.remove("loginId");
            info.remove("terminalLogin");
            info.insert("loginPending".into(), false.into());
        }
        for (_, runtime) in self.sessions.lock().await.drain() {
            runtime.connection.close();
            runtime._server.close().await;
        }
        for (_, (_, connection)) in self.discovery.lock().await.drain() {
            connection.close();
        }
        self.permissions.lock().await.clear();
        self.activity.lock().await.clear();
    }
}

struct Output<'a> {
    timeline: timeline::Timeline<'a>,
    record: String,
    thread: String,
    permissions: Arc<Mutex<HashMap<String, PendingPermission>>>,
    activity: Arc<Mutex<HashMap<String, (String, String)>>>,
}

#[async_trait::async_trait]
impl TextOutput for Output<'_> {
    async fn update(&self, text: &str) -> Result<(), String> {
        self.timeline.update(text).await
    }
}

#[async_trait::async_trait]
impl acp::Output for Output<'_> {
    async fn question(&self, request: acp::QuestionRequest) -> Result<acp::Answer, String> {
        questions::ask(self, request).await
    }
    async fn content(&self, part: nucleus::message::MessagePart) -> Result<(), String> {
        let uid = self.timeline.open_content().await?;
        self.timeline.engine.access_scope(true, async {
            let mut parts = store::message_content::load(&self.timeline.engine.store.pool, &uid).await?;
            parts.push(part);
            store::message_content::save(&self.timeline.engine.store.pool, &uid, &parts).await?;
            self.timeline.engine.act(Action::SetExtension {
                target: uid, namespace: "lince.message-content".into(),
                fds: json!({"parts":nucleus::message::describe(&parts).map_err(engine::EngineError::Consequence)?}),
            }, None).await
        }).await.map_err(|error| error.to_string())?;
        Ok(())
    }

    async fn activity(&self, value: Value) -> Result<(), String> {
        self.timeline.activity(&value).await?;
        if matches!(
            value["sessionUpdate"].as_str(),
            Some("tool_call" | "tool_call_update")
        ) {
            let title = value["title"].as_str().unwrap_or("Agent tool");
            let status = value["status"].as_str().unwrap_or("working");
            let title: String = format!("{title} · {status}").chars().take(512).collect();
            self.activity
                .lock()
                .await
                .insert(self.thread.clone(), (self.record.clone(), title));
        }
        Ok(())
    }

    async fn permission(&self, mut request: acp::Permission) -> Result<Option<String>, String> {
        request.id = nucleus::new_uid("permission");
        request.details = request.details.chars().take(16_384).collect();
        let id = request.id.clone();
        let (response, received) = oneshot::channel();
        self.permissions.lock().await.insert(
            id.clone(),
            PendingPermission {
                record: self.record.clone(),
                thread: self.thread.clone(),
                request,
                response,
            },
        );
        self.timeline.progress(None, "waiting for input").await?;
        let answer = received.await.unwrap_or(None);
        self.timeline.progress(None, "running").await?;
        self.permissions.lock().await.remove(&id);
        Ok(answer)
    }
}

impl Host {
    pub(super) async fn discover_agent(
        &self,
        record: &str,
        mut config: acp::Config,
    ) -> Result<(), String> {
        self.options_available(record).await?;
        config.validate()?;
        {
            let discovery = self.agents.discovery.lock().await;
            if discovery.len() >= 16 && !discovery.contains_key(record) {
                return Err(
                    "Use /lock to close agent connections before discovering another.".into(),
                );
            }
        }
        let connection = acp::Connection::open(&config).await?;
        let info = serde_json::to_value(&connection.info).map_err(|error| error.to_string())?;
        self.agents.info.lock().await.insert(record.into(), info);
        if let Some((_, previous)) = self
            .agents
            .discovery
            .lock()
            .await
            .insert(record.into(), (config, connection))
        {
            previous.close();
        }
        Ok(())
    }

    pub(super) async fn configure_agent(
        &self,
        record: &str,
        config: acp::Config,
    ) -> Result<(), String> {
        self.engine
            .access_scope(true, async {
                self.configure_agent_inner(record, config)
                    .await
                    .map_err(engine::EngineError::Consequence)
            })
            .await
            .map_err(|e| e.to_string())
    }

    async fn configure_agent_inner(
        &self,
        record: &str,
        mut config: acp::Config,
    ) -> Result<(), String> {
        config.validate()?;
        let running = self.running.lock().await;
        if running.values().any(|run| run.record == record) {
            return Err("Stop this Fiote before changing its agent.".into());
        }
        if self
            .agents
            .info
            .lock()
            .await
            .get(record)
            .is_some_and(|info| info["loginPending"] == true)
        {
            return Err("Finish or cancel the current sign-in before changing settings.".into());
        }
        if !config.additional_directories.is_empty() {
            let connection = acp::Connection::open(&config).await?;
            let supported = connection.validate_directories(&config);
            connection.close();
            supported?;
        }
        self.prepare(record).await?;
        config.require_vault = false;
        if self.load(record)?.and_then(|saved| saved.agent).as_ref() != Some(&config) {
            if let Some(info) = self
                .agents
                .info
                .lock()
                .await
                .get_mut(record)
                .and_then(Value::as_object_mut)
            {
                info.remove("connectionCheck");
            }
        }
        let author = record.to_string();
        self.agents.close_record(record).await;
        let mut discovery = self.agents.discovery.lock().await;
        if discovery.get(record).is_some_and(|(previous, _)| {
            previous.command != config.command
                || previous.args != config.args
                || previous.directory != config.directory
                || previous.environment != config.environment
        }) {
            if let Some((_, connection)) = discovery.remove(record) {
                connection.close();
            }
            self.agents.info.lock().await.remove(record);
        }
        drop(discovery);
        save(
            &self.path(record)?,
            &Configuration {
                settings: Settings {
                    enabled: true,
                    directory: config.directory.clone(),
                    ..Default::default()
                },
                author,
                agent: Some(config),
            },
        )
    }

    pub(super) async fn authenticate_agent(
        &self,
        record: &str,
        method: &str,
    ) -> Result<(), String> {
        if self
            .agents
            .info
            .lock()
            .await
            .get(record)
            .is_some_and(|info| info["loginPending"] == true)
        {
            return Err("Finish the current agent login first.".into());
        }
        let connections = self.agents.discovery.lock().await;
        let (config, connection) = connections
            .get(record)
            .map(|(config, connection)| (config.clone(), connection.clone()))
            .ok_or("Discover the agent before signing in.")?;
        drop(connections);
        let method = connection
            .info
            .auth_methods
            .iter()
            .find(|candidate| candidate.id().to_string() == method)
            .ok_or("Choose one of this agent's authentication methods.")?;
        if let acp::AuthMethod::Terminal(method) = method {
            return self
                .start_terminal_login(record, config, method.clone())
                .await;
        }
        let method = method.id().to_string();
        let info = self.agents.info.clone();
        let login_id = login::start(&self.agents, record).await?;
        let record = record.to_string();
        tokio::spawn(async move {
            let result = tokio::time::timeout(
                std::time::Duration::from_secs(600),
                connection.authenticate(&method),
            )
            .await
            .map_err(|_| "Sign-in timed out. Try again.".to_string())
            .and_then(|result| result);
            login::finish(info, record, login_id, result).await;
        });
        Ok(())
    }

    pub(super) async fn start_agent_turn(
        &self,
        record: store::records::RecordRow,
        author: String,
        config: acp::Config,
        thread: &str,
        body: &str,
        content: &[nucleus::message::MessagePart],
        forwarded: Option<handoff::Forwarded>,
        running: &mut HashMap<String, Running>,
    ) -> Result<ActionOutcome, String> {
        let config = self.agent_working_directory(&record.uid, config).await?;
        let settings_path = self.directory.join(format!("agent-options-{thread}.json"));
        let config = live::configured(config, &settings_path)?;
        let evicted = {
            let mut sessions = self.agents.sessions.lock().await;
            if sessions.len() >= 8 && !sessions.contains_key(thread) {
                let idle = sessions
                    .keys()
                    .find(|thread| !running.contains_key(*thread))
                    .cloned()
                    .ok_or("Eight agent sessions are already working.")?;
                sessions.remove(&idle)
            } else {
                None
            }
        };
        if let Some(runtime) = evicted {
            runtime.connection.close();
            runtime._server.close().await;
        }
        let history = self.history(thread, &author).await?;
        let system = self.session_instructions(&record.uid, thread).await?;
        fiote::runtime::validate_context(&format!("{system}{body}"), &history)?;
        let saved_session = self.directory.join(format!("agent-session-{thread}.json"));
        let runtime = live::runtime(
            &self.agents.sessions,
            &self.engine,
            &record.uid,
            &author,
            thread,
            &config,
            &system,
            &saved_session,
        )
        .await?;
        runtime.connection.content(content)?;
        let command = if let Some(command) = body.trim().strip_prefix("/agent:") {
            let name = command
                .split_whitespace()
                .next()
                .ok_or("Choose an agent command.")?;
            if !content.is_empty() {
                return Err("Agent commands currently accept text arguments. Remove attached contents before invoking one.".into());
            }
            if !runtime
                .connection
                .state(&runtime.session)
                .await
                .commands
                .iter()
                .any(|command| command.name == name)
            {
                return Err("This command is no longer offered by the conversation's agent. Reload its command list.".into());
            }
            Some(format!("/{command}"))
        } else {
            None
        };
        let mut outcome = self
            .engine
            .act(
                Action::CreateMessage {
                    content: forwarded.as_ref().map(|source| source.content(content)).unwrap_or_else(|| content.to_vec()),
                    thread: thread.into(),
                    body: body.into(),
                    author: forwarded.as_ref().map(|source| source.author.clone()),
                    state: MessageState::Finished,
                    parent: None,
                    references: Vec::new(),
                },
                None,
            )
            .await
            .map_err(|error| error.to_string())?;
        let reply = match self
            .engine
            .act(
                Action::CreateMessage {
                    content: Vec::new(),
                    thread: thread.into(),
                    body: String::new(),
                    author: Some(author.clone()),
                    state: MessageState::Writing,
                    parent: outcome.created.clone(),
                    references: vec![],
                },
                None,
            )
            .await
        {
            Ok(result) => result.created.ok_or("Could not create the reply.")?,
            Err(error) => {
                outcome.warnings.push(format!(
                    "Message saved, but the agent could not start: {error}"
                ));
                return Ok(outcome);
            }
        };
        let native = transport::Session::local(
            self.engine.clone(),
            Arc::new(transport::LaneHub::new()),
            nucleus::new_uid("agent-output"),
        )
        .into_native_tools(transport::native::Context {
            agent: author.clone(),
            record: record.uid.clone(),
            thread: thread.into(),
        });
        native.set_source_message(outcome.created.as_deref());
        runtime.tools.set_source_message(outcome.created.as_deref());
        let mut supplied = if runtime.fresh.load(std::sync::atomic::Ordering::Acquire) {
            history.clone()
        } else {
            Vec::new()
        };
        supplied.push(if content.is_empty() {
            Message::User(body.into())
        } else {
            Message::RichUser {
                text: body.into(),
                content: content.to_vec(),
            }
        });
        let mut available = Registry::default();
        native.register(&mut available);
        self.save_context(thread, &record.uid, &supplied, &available)
            .await?;
        if let Err(error) = native.attach_message(&reply).await {
            let _ = self
                .engine
                .act(
                    Action::ReviseMessage {
                        message: reply,
                        body: format!("Fiote could not start: {error}"),
                        state: MessageState::Interrupted,
                    },
                    None,
                )
                .await;
            native.close().await;
            outcome.warnings.push(format!(
                "Message saved, but the agent could not start: {error}"
            ));
            return Ok(outcome);
        }
        let mut tools = Registry::default();
        native.register(&mut tools);
        let pending = self.directory.join(format!("turn-{thread}.json"));
        if let Err(error) = save(
            &pending,
            &Pending {
                root: Some(reply.clone()),
                message: reply.clone(),
            },
        ) {
            let output = output::Output {
                journal_path: None,
                summary_path: None,
                original_messages: Vec::new(),
                context_path: None,
                source_message: None,
                usage_path: None,
                tools: &tools,
                message: &reply,
                text: Default::default(),
            };
            let _ = output
                .finish(
                    &format!("Fiote could not start: {error}"),
                    MessageState::Interrupted,
                )
                .await;
            native.close().await;
            outcome.warnings.push(format!(
                "Message saved, but the agent session could not be saved: {error}"
            ));
            return Ok(outcome);
        }
        let (stop, receiver) = watch::channel(false);
        running.insert(
            thread.into(),
            Running {
                record: record.uid.clone(),
                stop,
            },
        );
        let thread = thread.to_string();
        let body = body.to_string();
        let content = content.to_vec();

        let engine = self.engine.clone();
        let active = self.running.clone();
        let sessions = self.agents.sessions.clone();
        let permissions = self.agents.permissions.clone();
        let activity = self.agents.activity.clone();
        let saved_session = self.directory.join(format!("agent-session-{thread}.json"));
        tokio::spawn(async move {
            let output = Output {
                timeline: timeline::Timeline {
                    root: reply.clone(),
                    engine: engine.clone(),
                    tools: &tools,
                    author: author.clone(),
                    thread: thread.clone(),
                    pending: pending.clone(),
                    state: Mutex::new(timeline::State {
                        message: reply.clone(),
                        text: String::new(),
                        offset: 0,
                        closed: false,
                        message_id: None,
                        calls: HashMap::new(),
                    }),
                },
                record: record.uid.clone(),
                thread: thread.clone(),
                permissions: permissions.clone(),
                activity: activity.clone(),
            };
            let mut cancellation = receiver.clone();
            let operation = async {
                let runtime = live::runtime(
                    &sessions,
                    &engine,
                    &record.uid,
                    &author,
                    &thread,
                    &config,
                    &system,
                    &saved_session,
                )
                .await?;
                live::apply_pending(&runtime, &settings_path, &saved_session).await?;
                if let Some(command) = command {
                    return runtime
                        .connection
                        .prompt(&runtime.session, command, &output, receiver)
                        .await;
                }
                let fresh = runtime
                    .fresh
                    .swap(false, std::sync::atomic::Ordering::AcqRel);
                let mut parts = Vec::new();
                if fresh {
                    for message in &history {
                        match message {
                            Message::User(text) | Message::Summary { text, .. } => {
                                parts.push(nucleus::message::MessagePart::Text {
                                    text: format!("Earlier user message:\n{text}"),
                                })
                            }
                            Message::RichUser { text, content } => {
                                parts.push(nucleus::message::MessagePart::Text {
                                    text: format!("Earlier user message:\n{text}"),
                                });
                                parts.extend(content.clone());
                            }
                            Message::RichAssistant { text, content } => {
                                parts.push(nucleus::message::MessagePart::Text {
                                    text: format!("Earlier assistant reply:\n{text}"),
                                });
                                parts.extend(content.clone());
                            }
                            Message::Assistant { text, .. } => {
                                parts.push(nucleus::message::MessagePart::Text {
                                    text: format!("Earlier assistant reply:\n{text}"),
                                })
                            }
                            Message::Tool { .. } | Message::Replay { .. } => {}
                        }
                    }
                }
                parts.push(nucleus::message::MessagePart::Text {
                    text: format!("Current user message:\n{body}"),
                });
                parts.extend(content);
                let prompt = format!(
                    "Fiote's current instructions from its Record:\n{system}\nUse the Lince MCP tools for Lince data. Your answer is already streamed into this thread; do not create a duplicate reply with lince_message. Use your code tools for the selected working folder."
                );
                runtime
                    .connection
                    .prompt_content(&runtime.session, prompt, &parts, &output, receiver)
                    .await
            };
            let result = tokio::select! {
                biased;
                _ = acp::cancelled(&mut cancellation) => Err("Stopped by you. Completed tool operations remain saved.".into()),
                result = operation => result,
            };
            if let Some(runtime) = sessions.lock().await.get(&thread) {
                let state = runtime.connection.state(&runtime.session).await;
                if let Some(report) = state.usage_report(
                    &runtime.session,
                    &format!(
                        "ACP · {}",
                        runtime
                            .connection
                            .info
                            .agent_info
                            .as_ref()
                            .map(|info| info.name.as_str())
                            .unwrap_or("agent")
                    ),
                ) {
                    if let Some(directory) = pending.parent() {
                        let _ = usage::record(&usage::path(directory, &thread), report);
                    }
                }
            }
            let progress_state = match &result {
                Ok(_) => "completed",
                Err(error) if error.starts_with("Stopped by you") => "interrupted",
                Err(error) if error.contains("connection") => "disconnected",
                Err(_) => "failed",
            };
            let _ = output.timeline.progress(None, progress_state).await;
            if result.is_err() {
                if let Some(runtime) = sessions.lock().await.remove(&thread) {
                    runtime.connection.close();
                    runtime._server.close().await;
                }
            }
            let (body, state) = match result {
                Ok(body) => (
                    if body.trim().is_empty() {
                        "The agent finished without a text reply. Check the tool results.".into()
                    } else {
                        body
                    },
                    MessageState::Finished,
                ),
                Err(error) => {
                    let partial = output.timeline.state.lock().await.text.clone();
                    (
                        format!("{partial}\n\nFiote stopped: {error}"),
                        MessageState::Interrupted,
                    )
                }
            };
            if output
                .timeline
                .finish(&body, state == MessageState::Interrupted)
                .await
                .is_ok()
            {
                let _ = std::fs::remove_file(pending);
            }
            native.close().await;
            permissions
                .lock()
                .await
                .retain(|_, pending| pending.thread != thread);
            activity.lock().await.remove(&thread);
            active.lock().await.remove(&thread);
        });
        Ok(outcome)
    }
}
