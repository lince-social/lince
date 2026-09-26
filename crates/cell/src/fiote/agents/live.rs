use super::*;
use std::collections::BTreeMap;

#[derive(Default, Serialize, Deserialize)]
struct Choices {
    #[serde(default)]
    directories: Option<Vec<PathBuf>>,
    values: BTreeMap<String, Value>,
    pending: BTreeMap<String, Value>,
}

fn choices(path: &Path) -> Result<Choices, String> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|error| error.to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Choices::default()),
        Err(error) => Err(error.to_string()),
    }
}

pub(super) fn configured(mut config: acp::Config, path: &Path) -> Result<acp::Config, String> {
    let saved = choices(path)?;
    config.options.extend(saved.values);
    if let Some(directories) = saved.directories {
        config.additional_directories = directories;
    }
    config.validate()?;
    Ok(config)
}

pub(super) async fn runtime(
    sessions: &Mutex<HashMap<String, Arc<Runtime>>>,
    engine: &Arc<Engine>,
    record: &str,
    author: &str,
    thread: &str,
    config: &acp::Config,
    system: &str,
    saved_session: &Path,
) -> Result<Arc<Runtime>, String> {
    if let Some(runtime) = sessions
        .lock()
        .await
        .get(thread)
        .filter(|runtime| !runtime.connection.is_closed())
        .cloned()
    {
        if runtime.record != record {
            return Err("This session belongs to another Fiote.".into());
        }
        return Ok(runtime);
    }
    sessions.lock().await.remove(thread);
    if sessions.lock().await.len() >= 8 {
        return Err("Close an agent session before opening another.".into());
    }
    let connection = acp::Connection::open(config).await?;
    let external = transport::Session::local(
        engine.clone(),
        Arc::new(transport::LaneHub::new()),
        nucleus::new_uid("agent-tools"),
    )
    .into_native_tools(transport::native::Context {
        agent: author.into(),
        record: record.into(),
        thread: thread.into(),
    })
    .with_instructions(system.to_string());
    let server = transport::mcp::Connection::open(external).await?;
    let saved: Option<Value> = std::fs::read(saved_session)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok());
    let configured = serde_json::to_value(config).map_err(|error| error.to_string())?;
    let previous = saved
        .as_ref()
        .filter(|saved| saved["config"] == configured)
        .and_then(|saved| saved["session"].as_str());
    let tools = fiote::config::ToolConnection {
        thread: thread.into(),
        url: server.url.clone(),
        token: server.token.clone(),
    };
    let opened = tokio::time::timeout(
        std::time::Duration::from_secs(90),
        connection.session(config, tools, previous),
    )
    .await;
    let session = match opened {
        Ok(Ok(session)) => session,
        result => {
            connection.close();
            server.close().await;
            return Err(match result {
                Ok(Err(error)) => error,
                _ => "The agent did not open its session within 90 seconds.".into(),
            });
        }
    };
    let fresh = previous.is_none() || !connection.info.agent_capabilities.load_session;
    if let Err(error) = save(
        saved_session,
        &json!({"config":configured,"session":session}),
    ) {
        connection.close();
        server.close().await;
        return Err(error);
    }
    let runtime = Arc::new(Runtime {
        record: record.into(),
        connection,
        session,
        config: config.clone(),
        fresh: std::sync::atomic::AtomicBool::new(fresh),
        settings: Mutex::new(()),
        _server: server,
    });
    sessions.lock().await.insert(thread.into(), runtime.clone());
    Ok(runtime)
}

async fn apply(
    runtime: &Runtime,
    saved: &mut Choices,
    option: &str,
    value: &Value,
    session_path: &Path,
) -> Result<(), String> {
    runtime
        .connection
        .change_option(&runtime.session, option, value)
        .await?;
    let state = runtime.connection.state(&runtime.session).await;
    let values = acp::SessionOptions {
        session: runtime.session.clone(),
        options: state.options,
    }
    .values();
    saved.values = values.into_iter().collect();
    saved.pending.remove(option);
    let mut config = runtime.config.clone();
    config.options.extend(saved.values.clone());
    save(
        session_path,
        &json!({"config":config,"session":runtime.session}),
    )
}

pub(super) async fn apply_pending(
    runtime: &Runtime,
    path: &Path,
    session_path: &Path,
) -> Result<(), String> {
    let _settings = runtime.settings.lock().await;
    let mut saved = choices(path)?;
    let mut pending: Vec<_> = saved.pending.clone().into_iter().collect();
    pending.sort_by_key(|(id, _)| {
        if id == "provider" {
            0
        } else if id == "model" {
            1
        } else {
            2
        }
    });
    for (option, value) in pending {
        let result = apply(runtime, &mut saved, &option, &value, session_path).await;
        saved.pending.remove(&option);
        save(path, &saved)?;
        result?;
    }
    Ok(())
}

impl Host {
    pub(in crate::fiote) async fn reset_session(&self, thread: &str) -> Result<String, String> {
        let record = self.thread_fiote(thread).await?;
        let running = self.running.lock().await;
        if running.contains_key(thread) {
            return Err("Stop the current reply before resetting conversation choices.".into());
        }
        self.agents.close_thread(thread).await;
        for prefix in ["agent-options", "agent-session", "agent-state"] {
            match std::fs::remove_file(self.directory.join(format!("{prefix}-{thread}.json"))) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.to_string()),
            }
        }
        drop(running);
        self.open_session_options(thread).await?;
        Ok(record)
    }
    pub(in crate::fiote) async fn set_session_directories(
        &self,
        thread: &str,
        directories: Vec<PathBuf>,
    ) -> Result<String, String> {
        let record = self.thread_fiote(thread).await?;
        let running = self.running.lock().await;
        if running.contains_key(thread) {
            return Err("Stop the current reply before changing directories.".into());
        }
        let runtime = self
            .agents
            .sessions
            .lock()
            .await
            .get(thread)
            .cloned()
            .ok_or("Load this conversation first.")?;
        let mut config = runtime.config.clone();
        config.additional_directories = directories;
        config.validate()?;
        runtime.connection.validate_directories(&config)?;
        if !runtime.connection.info.agent_capabilities.load_session {
            return Err("This agent cannot reload the conversation with changed directories. Set defaults before starting a new conversation.".into());
        }
        let path = self.directory.join(format!("agent-options-{thread}.json"));
        let mut saved = choices(&path)?;
        saved.directories = Some(config.additional_directories.clone());
        config.options.extend(saved.values.clone());
        save(&path, &saved)?;
        save(
            &self.directory.join(format!("agent-session-{thread}.json")),
            &json!({"config":config,"session":runtime.session}),
        )?;
        self.agents.sessions.lock().await.remove(thread);
        runtime.connection.close();
        runtime._server.close().await;
        drop(running);
        self.open_session_options(thread).await?;
        Ok(record)
    }

    pub(in crate::fiote) async fn agent_session_status(
        &self,
        thread: &str,
    ) -> Result<Option<fiote::config::AgentSession>, String> {
        let runtime = self.agents.sessions.lock().await.get(thread).cloned();
        let state_path = self.directory.join(format!("agent-state-{thread}.json"));
        let Some(runtime) = runtime else {
            let saved = std::fs::read(state_path)
                .ok()
                .and_then(|bytes| {
                    serde_json::from_slice::<fiote::config::AgentSession>(&bytes).ok()
                })
                .map(|mut state| {
                    state.connected = false;
                    state
                });
            return Ok(saved);
        };
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
            usage::record(&usage::path(&self.directory, thread), report)?;
        }
        let session = fiote::config::AgentSession {
            prompt_capabilities: serde_json::to_value(
                &runtime
                    .connection
                    .info
                    .agent_capabilities
                    .prompt_capabilities,
            )
            .map_err(|error| error.to_string())?,
            directories: runtime.config.additional_directories.clone(),
            supports_directories: runtime
                .connection
                .info
                .agent_capabilities
                .session_capabilities
                .additional_directories
                .is_some(),
            thread: thread.into(),
            state,
            pending: choices(&self.directory.join(format!("agent-options-{thread}.json")))?.pending,
            connected: !runtime.connection.is_closed(),
        };
        save(&state_path, &session)?;
        Ok(Some(session))
    }

    pub(in crate::fiote) async fn open_session_options(
        &self,
        thread: &str,
    ) -> Result<String, String> {
        let record = self.thread_fiote(thread).await?;
        let running = self.running.lock().await;
        if running.contains_key(thread) {
            return Ok(record);
        }
        let saved = self.load(&record)?.ok_or("Configure this Fiote first.")?;
        let config = configured(
            saved.agent.ok_or("This Fiote does not use an ACP agent.")?,
            &self.directory.join(format!("agent-options-{thread}.json")),
        )?;
        let system = self.session_instructions(&record, thread).await?;
        runtime(
            &self.agents.sessions,
            &self.engine,
            &record,
            &saved.author,
            thread,
            &config,
            &system,
            &self.directory.join(format!("agent-session-{thread}.json")),
        )
        .await?;
        Ok(record)
    }

    pub(in crate::fiote) async fn set_session_option(
        &self,
        thread: &str,
        option: &str,
        value: &Value,
    ) -> Result<String, String> {
        let record = self.thread_fiote(thread).await?;
        let running = self.running.lock().await;
        let runtime = self
            .agents
            .sessions
            .lock()
            .await
            .get(thread)
            .filter(|runtime| runtime.record == record && !runtime.connection.is_closed())
            .cloned()
            .ok_or("Load this conversation's choices first.")?;
        let session = acp::SessionOptions {
            session: runtime.session.clone(),
            options: runtime.connection.state(&runtime.session).await.options,
        };
        session.option_value(option, value)?;
        let path = self.directory.join(format!("agent-options-{thread}.json"));
        let _settings = runtime.settings.lock().await;
        let mut saved = choices(&path)?;
        if running.contains_key(thread) {
            saved.pending.insert(option.into(), value.clone());
        } else {
            apply(
                &runtime,
                &mut saved,
                option,
                value,
                &self.directory.join(format!("agent-session-{thread}.json")),
            )
            .await?;
        }
        save(&path, &saved)?;
        Ok(record)
    }
}
