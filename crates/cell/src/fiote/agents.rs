use super::*;
use fiote::{acp, config::AgentActivity, provider::TextOutput};
use serde_json::{Value, json};
use tokio::sync::oneshot;

struct Runtime {
    record: String,
    connection: Arc<acp::Connection>,
    session: String,
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
    sessions: Arc<Mutex<HashMap<String, Arc<Runtime>>>>,
    pub info: Arc<Mutex<HashMap<String, Value>>>,
    discovery: Mutex<HashMap<String, (acp::Config, Arc<acp::Connection>)>>,
    permissions: Arc<Mutex<HashMap<String, PendingPermission>>>,
    activity: Arc<Mutex<HashMap<String, (String, String)>>>,
}

impl Agents {
    pub async fn close_thread(&self, thread: &str) {
        if let Some(runtime) = self.sessions.lock().await.remove(thread) {
            runtime.connection.close();
            runtime._server.close().await;
        }
    }

    pub async fn info(&self, record: &str) -> Option<Value> {
        let mut info = self.info.lock().await.get(record).cloned()?;
        if info["loginPending"] == true {
            let connection = self
                .discovery
                .lock()
                .await
                .get(record)
                .map(|(_, connection)| connection.clone());
            if let Some(connection) = connection {
                info["deviceCode"] = connection
                    .login_notice
                    .lock()
                    .await
                    .clone()
                    .unwrap_or(Value::Null);
            }
        }
        Some(info)
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
        let answer = received.await.unwrap_or(None);
        self.permissions.lock().await.remove(&id);
        Ok(answer)
    }
}

impl Host {
    pub(super) async fn agent_provider_login(
        &self,
        record: &str,
        fields: Secret,
        _password: Option<Secret>,
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
        let entry = self
            .agents
            .info
            .lock()
            .await
            .get(record)
            .and_then(|info| info.get("selectedProvider"))
            .cloned()
            .ok_or("Choose an agent provider first.")?;
        let provider = entry["providerId"]
            .as_str()
            .ok_or("The provider has no identifier.")?
            .to_string();
        let values: std::collections::BTreeMap<String, String> =
            serde_json::from_str(&fields.0).map_err(|_| "Invalid provider fields.")?;
        if values.len() > 32 || fields.0.len() > 65_536 {
            return Err("Provider credentials exceed their size limit.".into());
        }
        let allowed = entry["fields"]
            .as_array()
            .ok_or("The provider has no field definitions.")?;
        if values
            .keys()
            .any(|key| !allowed.iter().any(|field| field["key"] == *key))
        {
            return Err("Only this provider's discovered fields may be saved.".into());
        }
        if allowed.iter().any(|field| {
            field["required"] == true
                && values
                    .get(field["key"].as_str().unwrap_or(""))
                    .is_none_or(|value| value.is_empty())
        }) {
            return Err("Fill in this provider's required fields.".into());
        }
        let connection = self
            .agents
            .discovery
            .lock()
            .await
            .get(record)
            .map(|(_, connection)| connection.clone())
            .ok_or("Discover the agent first.")?;
        if !values.is_empty() {
            let updates: Vec<_> = values
                .into_iter()
                .map(|(key, value)| json!({"key":key,"value":value}))
                .collect();
            connection
                .extension(
                    "_goose/unstable/providers/config/save",
                    json!({"providerId":provider,"fields":updates}),
                )
                .await?;
        }
        if matches!(
            entry["setupMethod"].as_str(),
            Some("oauth_browser" | "oauth_device_code" | "host_with_oauth_fallback")
        ) {
            let info = self.agents.info.clone();
            let record = record.to_string();
            *connection.login_notice.lock().await = None;
            info.lock()
                .await
                .get_mut(&record)
                .ok_or("Agent discovery expired.")?["loginPending"] = true.into();
            tokio::spawn(async move {
                let result = connection
                    .extension_wait(
                        "_goose/unstable/providers/config/authenticate",
                        json!({"providerId":provider}),
                    )
                    .await;
                if let Some(info) = info.lock().await.get_mut(&record) {
                    info["loginPending"] = false.into();
                    info["loginResult"] = match result {
                        Ok(_) => "The agent completed its login flow.".into(),
                        Err(error) => error.into(),
                    };
                }
            });
        }
        Ok(())
    }

    pub(super) async fn select_agent_provider(
        &self,
        record: &str,
        provider: &str,
    ) -> Result<(), String> {
        if self
            .running
            .lock()
            .await
            .values()
            .any(|run| run.record == record)
        {
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
            return Err("Finish the current agent login first.".into());
        }
        let entry = self
            .agents
            .info
            .lock()
            .await
            .get(record)
            .and_then(|info| info["providers"].as_array())
            .and_then(|providers| {
                providers
                    .iter()
                    .find(|entry| entry["providerId"] == provider)
            })
            .cloned()
            .ok_or("Choose a connection from the agent's discovered providers.")?;
        let (mut config, connection) = self
            .agents
            .discovery
            .lock()
            .await
            .get(record)
            .cloned()
            .ok_or("Discover the agent first.")?;
        config
            .session_meta
            .insert("provider".into(), provider.into());
        config.require_vault = false;
        if let Some((_, previous)) = self
            .agents
            .discovery
            .lock()
            .await
            .remove(&format!("login:{record}"))
        {
            previous.close();
        }
        if let Some(info) = self
            .agents
            .info
            .lock()
            .await
            .get_mut(record)
            .and_then(Value::as_object_mut)
        {
            info.remove("loginAgent");
            info.remove("loginResult");
        }
        if entry["acp"] == true {
            let binary = entry["binaryName"]
                .as_str()
                .ok_or("This provider does not report its companion executable.")?;
            let auth_config = acp::Config {
                require_vault: false,
                command: binary.into(),
                args: vec![],
                directory: config.directory.clone(),
                environment: Default::default(),
                session_meta: Default::default(),
            };
            let auth = acp::Connection::open(&auth_config).await?;
            let info = serde_json::to_value(&auth.info).map_err(|error| error.to_string())?;
            self.agents
                .info
                .lock()
                .await
                .get_mut(record)
                .ok_or("Agent discovery expired.")?["loginAgent"] = info;
            self.agents
                .discovery
                .lock()
                .await
                .insert(format!("login:{record}"), (auth_config, auth));
        }
        self.agents
            .info
            .lock()
            .await
            .get_mut(record)
            .ok_or("Agent discovery expired.")?["selectedProvider"] = entry;
        self.agents
            .discovery
            .lock()
            .await
            .insert(record.into(), (config.clone(), connection));
        self.configure_agent(record, config).await
    }

    pub(super) async fn discover_agent(
        &self,
        record: &str,
        mut config: acp::Config,
    ) -> Result<(), String> {
        self.record(record).await?;
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
        let mut info = serde_json::to_value(&connection.info).map_err(|error| error.to_string())?;
        if info["agentInfo"]["name"] == "goose" {
            match connection
                .extension("_goose/unstable/providers/setup/catalog/list", json!({}))
                .await
            {
                Ok(catalog) if catalog.to_string().len() <= 262_144 => {
                    info["providers"] = catalog["providers"].clone()
                }
                Ok(_) => {
                    info["setupError"] = "The agent provider catalog exceeds its size limit.".into()
                }
                Err(error) => info["setupError"] = error.into(),
            }
        }
        self.agents.info.lock().await.insert(record.into(), info);
        if let Some((_, previous)) = self
            .agents
            .discovery
            .lock()
            .await
            .remove(&format!("login:{record}"))
        {
            previous.close();
        }
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
        self.prepare(record).await?;
        config.require_vault = false;
        let author = record.to_string();
        self.agents.close_record(record).await;
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
        let connection = connections
            .get(&format!("login:{record}"))
            .or_else(|| connections.get(record))
            .map(|(_, connection)| connection.clone())
            .ok_or("Discover the agent before signing in.")?;
        drop(connections);
        let method = connection
            .info
            .auth_methods
            .iter()
            .find(|candidate| candidate.id().to_string() == method)
            .ok_or("Choose one of this agent's authentication methods.")?;
        if connection
            .info
            .agent_info
            .as_ref()
            .is_some_and(|info| info.name == "goose")
        {
            return Err("Choose a provider from Goose's connections. Goose's generic authenticate method does not perform login.".into());
        }
        if serde_json::to_value(method).map_err(|error| error.to_string())?["type"] == "terminal" {
            return Err("This agent requires interactive terminal login. Run its documented login command, then connect again.".into());
        }
        let method = method.id().to_string();
        let info = self.agents.info.clone();
        let record = record.to_string();
        info.lock()
            .await
            .get_mut(&record)
            .ok_or("Agent discovery expired.")?["loginPending"] = true.into();
        tokio::spawn(async move {
            let result = connection.authenticate(&method).await;
            if let Some(info) = info.lock().await.get_mut(&record) {
                info["loginPending"] = false.into();
                info["loginResult"] = match result {
                    Ok(()) => "The agent completed its login flow.".into(),
                    Err(error) => error.into(),
                };
            }
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
        running: &mut HashMap<String, Running>,
    ) -> Result<ActionOutcome, String> {
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
        if serde_json::to_vec(&history)
            .map_err(|error| error.to_string())?
            .len()
            + body.len()
            + record.body.len()
            > 500 * 1024
        {
            return Err(
                "This conversation exceeds the agent context limit. Start a new thread.".into(),
            );
        }
        let mut outcome = self
            .engine
            .act(
                Action::CreateMessage {
                    thread: thread.into(),
                    body: body.into(),
                    author: None,
                    state: MessageState::Finished,
                    parent: None,
                    references: vec![],
                },
                None,
            )
            .await
            .map_err(|error| error.to_string())?;
        let reply = match self
            .engine
            .act(
                Action::CreateMessage {
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
                message: reply.clone(),
            },
        ) {
            let output = output::Output {
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

        let engine = self.engine.clone();
        let active = self.running.clone();
        let sessions = self.agents.sessions.clone();
        let permissions = self.agents.permissions.clone();
        let activity = self.agents.activity.clone();
        let saved_session = self.directory.join(format!("agent-session-{thread}.json"));
        tokio::spawn(async move {
            let output = Output {
                timeline: timeline::Timeline {
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
                let existing = sessions
                    .lock()
                    .await
                    .get(&thread)
                    .filter(|runtime| !runtime.connection.is_closed())
                    .cloned();
                let (runtime, fresh) = if let Some(existing) = existing {
                    (existing, false)
                } else {
                    let connection = acp::Connection::open(&config).await?;
                    let external = transport::Session::local(
                        engine,
                        Arc::new(transport::LaneHub::new()),
                        nucleus::new_uid("agent-tools"),
                    )
                    .into_native_tools(transport::native::Context {
                        agent: author,
                        record: record.uid.clone(),
                        thread: thread.clone(),
                    })
                    .with_instructions(system.clone());
                    let server = transport::mcp::Connection::open(external).await?;
                    let saved: Option<Value> = std::fs::read(&saved_session)
                        .ok()
                        .and_then(|bytes| serde_json::from_slice(&bytes).ok());
                    let configured =
                        serde_json::to_value(&config).map_err(|error| error.to_string())?;
                    let previous = saved
                        .as_ref()
                        .filter(|saved| saved["config"] == configured)
                        .and_then(|saved| saved["session"].as_str());
                    let tool_connection = fiote::config::ToolConnection {
                        thread: thread.clone(),
                        url: server.url.clone(),
                        token: server.token.clone(),
                    };
                    let session = tokio::time::timeout(
                        std::time::Duration::from_secs(90),
                        connection.session(&config, tool_connection, previous),
                    )
                    .await
                    .map_err(|_| {
                        "The agent did not open its session within 90 seconds.".to_string()
                    })??;
                    let fresh =
                        previous.is_none() || !connection.info.agent_capabilities.load_session;
                    save(
                        &saved_session,
                        &json!({"config":configured,"session":session}),
                    )?;
                    let runtime = Arc::new(Runtime {
                        record: record.uid.clone(),
                        connection,
                        session,
                        _server: server,
                    });
                    let mut sessions = sessions.lock().await;
                    if *receiver.borrow() {
                        return Err("Stopped by you.".into());
                    }
                    sessions.insert(thread.clone(), runtime.clone());
                    (runtime, fresh)
                };
                let context = if fresh && !history.is_empty() {
                    format!(
                        "\nSaved conversation:\n{}\n",
                        serde_json::to_string(&history).map_err(|error| error.to_string())?
                    )
                } else {
                    String::new()
                };
                let prompt = format!(
                    "Fiote's current instructions from its Record:\n{system}\nUse the Lince MCP tools for Lince data. Your answer is already streamed into this thread; do not create a duplicate reply with lince_message. Use your code tools for the selected working folder.\n{context}\nUser message:\n{body}"
                );
                runtime
                    .connection
                    .prompt(&runtime.session, prompt, &output, receiver)
                    .await
            };
            let result = tokio::select! {
                biased;
                _ = acp::cancelled(&mut cancellation) => Err("Stopped by you. Completed tool operations remain saved.".into()),
                result = operation => result,
            };
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
