use agent_client_protocol::schema::{ProtocolVersion, v1::*};
use agent_client_protocol::{
    AcpAgent, AcpAgentConfig, Agent, Client, ConnectionTo, UntypedMessage,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::sync::{Mutex, mpsc, oneshot, watch};

mod check;
mod content;
mod launch;
mod options;
mod questions;
mod state;
pub mod terminal;
pub use agent_client_protocol::schema::v1::{AuthMethod, AuthMethodTerminal};
pub use check::ConnectionCheck;
pub use launch::resolve;
pub use options::SessionOptions;
pub use questions::{Answer, QuestionRequest};
pub use state::SessionState;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub require_vault: bool,
    pub command: PathBuf,
    pub args: Vec<String>,
    pub directory: PathBuf,
    #[serde(default)]
    pub additional_directories: Vec<PathBuf>,
    #[serde(default)]
    pub environment: BTreeMap<String, String>,
    #[serde(default)]
    pub session_meta: serde_json::Map<String, Value>,
    #[serde(default)]
    pub options: BTreeMap<String, Value>,
}

impl Config {
    pub fn requires_node(&self) -> bool {
        launch::requires_node(self)
    }

    pub fn validate(&mut self) -> Result<(), String> {
        if self.requires_node() {
            return Err("Node.js agent launchers are not supported. Open AI connections and choose a native Fiote subscription, API key, or local model connection.".into());
        }
        if self.command.as_os_str().is_empty()
            || self.args.len() > 32
            || self.environment.len() > 32
        {
            return Err(
                "Choose an agent executable and at most 32 arguments and environment entries."
                    .into(),
            );
        }
        if self.options.len() > 64 {
            return Err("Save at most 64 agent options.".into());
        }
        if self.options.iter().any(|(id, value)| {
            id.is_empty() || id.len() > 256 || !(value.is_string() || value.is_boolean())
        }) {
            return Err(
                "Agent options must have an identifier and a choice or toggle value.".into(),
            );
        }
        if !self.directory.is_absolute() || !self.directory.is_dir() {
            return Err("Choose an existing working folder using its full path.".into());
        }
        self.directory = self
            .directory
            .canonicalize()
            .map_err(|error| error.to_string())?;
        if self.additional_directories.len() > 16 {
            return Err("Choose at most 16 additional directories.".into());
        }
        let mut directories = Vec::new();
        for path in &self.additional_directories {
            if !path.is_absolute() || !path.is_dir() {
                return Err(
                    "Additional directories must exist on the agent machine and use full paths."
                        .into(),
                );
            }
            let path = path.canonicalize().map_err(|error| error.to_string())?;
            if path != self.directory && !directories.contains(&path) {
                directories.push(path);
            }
        }
        self.additional_directories = directories;
        if serde_json::to_vec(self)
            .map_err(|error| error.to_string())?
            .len()
            > 16_384
        {
            return Err("Agent configuration exceeds 16 KiB.".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Permission {
    pub id: String,
    pub title: String,
    pub details: String,
    pub options: Vec<Choice>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Choice {
    pub id: String,
    pub label: String,
}

#[async_trait::async_trait]
pub trait Output: crate::provider::TextOutput {
    async fn question(&self, _question: QuestionRequest) -> Result<Answer, String> {
        Ok(Answer::Cancel)
    }
    async fn activity(&self, value: Value) -> Result<(), String>;
    async fn permission(&self, request: Permission) -> Result<Option<String>, String>;
    async fn content(&self, _part: nucleus::message::MessagePart) -> Result<(), String> {
        Err("This output does not support rich agent replies.".into())
    }
}

#[derive(Default)]
struct Buffer {
    text: String,
    dirty: bool,
    message_id: Option<String>,
}

enum Event {
    Question(QuestionRequest, oneshot::Sender<CreateElicitationResponse>),
    Update(SessionNotification),
    Permission(
        RequestPermissionRequest,
        oneshot::Sender<RequestPermissionResponse>,
    ),
}

pub struct Connection {
    questions: questions::Questions,
    peer: ConnectionTo<Agent>,
    pub info: InitializeResponse,
    events: Mutex<mpsc::Receiver<Event>>,
    task: Task,
    closed: watch::Sender<bool>,
    active: Arc<AtomicBool>,
    pub login_notice: Arc<Mutex<Option<Value>>>,
    states: Arc<Mutex<BTreeMap<String, SessionState>>>,
}

struct Task(tokio::task::JoinHandle<()>);

impl Drop for Task {
    fn drop(&mut self) {
        self.0.abort();
    }
}

struct Active(Arc<AtomicBool>);

impl Drop for Active {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

pub async fn cancelled(stop: &mut watch::Receiver<bool>) {
    while !*stop.borrow_and_update() {
        if stop.changed().await.is_err() {
            break;
        }
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        let _ = self.closed.send(true);
        self.task.0.abort();
    }
}

fn failure(error: impl std::fmt::Display) -> String {
    format!("Agent connection failed: {error}")
}

impl Connection {
    pub async fn logout(&self) -> Result<(), String> {
        if self.info.agent_capabilities.auth.logout.is_none() {
            return Err(
                "This agent does not advertise logout. Use its account settings to sign out."
                    .into(),
            );
        }
        tokio::time::timeout(
            Duration::from_secs(30),
            self.peer.send_request(LogoutRequest::new()).block_task(),
        )
        .await
        .map_err(|_| "Agent logout timed out.".to_string())?
        .map_err(failure)?;
        Ok(())
    }
    pub async fn open(config: &Config) -> Result<Arc<Self>, String> {
        let mut config = config.clone();
        config.validate()?;
        let command = launch::resolve(&config)?;
        let agent = AcpAgent::new(
            AcpAgentConfig::new(command)
                .args(config.args.clone())
                .envs(launch::environment(&config)),
        );
        let (events, receiver) = mpsc::channel(256);
        let notices = events.clone();
        let question_events = events.clone();
        let (ready, initialized) = oneshot::channel();
        let (closed, mut closing) = watch::channel(false);
        let active = Arc::new(AtomicBool::new(false));
        let receive_updates = active.clone();
        let receive_permissions = active.clone();
        let receive_questions = active.clone();
        let questions = questions::Questions::default();
        let question_updates = questions.clone();
        let question_closed = closed.subscribe();
        let login_notice = Arc::new(Mutex::new(None));
        let login_updates = login_notice.clone();
        let states = Arc::new(Mutex::new(BTreeMap::<String, SessionState>::new()));
        let state_updates = states.clone();
        let task = Task(tokio::spawn(async move {
            let ready = Arc::new(std::sync::Mutex::new(Some(ready)));
            let setup = ready.clone();
            let result = Client
                .builder()
                .on_receive_notification(
                    async move |message: UntypedMessage, _cx| {
                        if message.method()
                            == "_goose/unstable/providers/authentication/device-code"
                        {
                            if message.params().to_string().len() <= 8192 {
                                *login_updates.lock().await = Some(message.params().clone());
                            }
                            return Ok(());
                        }
                        if message.method() != "session/update" {
                            return Ok(());
                        }
                        if message.params().to_string().len() > 8 * 1024 * 1024 {
                            return Err(agent_client_protocol::Error::invalid_params());
                        }
                        let notice: SessionNotification =
                            serde_json::from_value(message.params().clone())
                                .map_err(|_| agent_client_protocol::Error::invalid_params())?;
                        {
                            let mut states = state_updates.lock().await;
                            let id = notice.session_id.to_string();
                            if states.len() < 16 || states.contains_key(&id) {
                                states.entry(id).or_default().update(&notice.update);
                            }
                        }
                        if !receive_updates.load(Ordering::Acquire) {
                            return Ok(());
                        }
                        notices
                            .send(Event::Update(notice))
                            .await
                            .map_err(|_| agent_client_protocol::Error::internal_error())
                    },
                    agent_client_protocol::on_receive_notification!(),
                )
                .on_receive_request(
                    async move |request: RequestPermissionRequest, responder, _cx| {
                        let (reply, received) = oneshot::channel();
                        let response = if receive_permissions.load(Ordering::Acquire)
                            && events.send(Event::Permission(request, reply)).await.is_ok()
                        {
                            received.await.unwrap_or_else(|_| {
                                RequestPermissionResponse::new(RequestPermissionOutcome::Cancelled)
                            })
                        } else {
                            RequestPermissionResponse::new(RequestPermissionOutcome::Cancelled)
                        };
                        responder.respond(response)
                    },
                    agent_client_protocol::on_receive_request!(),
                )
                .on_receive_request(
                    async move |request: CreateElicitationRequest, responder, _cx| {
                        let response = questions::receive(
                            request,
                            &question_events,
                            &receive_questions,
                            &question_updates,
                            question_closed.clone(),
                        )
                        .await?;
                        responder.respond(response)
                    },
                    agent_client_protocol::on_receive_request!(),
                )
                .connect_with(agent, move |peer: ConnectionTo<Agent>| async move {
                    let info =
                        peer
                            .send_request(
                                InitializeRequest::new(ProtocolVersion::V1)
                                    .client_capabilities(
                                        ClientCapabilities::new()
                                            .auth(AuthCapabilities::new().terminal(true))
                                            .elicitation(
                                                ElicitationCapabilities::new()
                                                    .form(ElicitationFormCapabilities::new())
                                                    .url(ElicitationUrlCapabilities::new()),
                                            )
                                            .session(
                                                ClientSessionCapabilities::new().config_options(
                                                    SessionConfigOptionsCapabilities::new()
                                                        .boolean(
                                                            BooleanConfigOptionCapabilities::new(),
                                                        ),
                                                ),
                                            )
                                            .meta(
                                                serde_json::from_value::<
                                                    serde_json::Map<String, Value>,
                                                >(
                                                    json!({"goose":{"customNotifications":true}})
                                                )
                                                .unwrap(),
                                            ),
                                    )
                                    .client_info(Implementation::new(
                                        "lince",
                                        env!("CARGO_PKG_VERSION"),
                                    )),
                            )
                            .block_task()
                            .await?;
                    if let Some(ready) = setup.lock().unwrap().take() {
                        let _ = ready.send(Ok((peer, info)));
                    }
                    let _ = closing.changed().await;
                    Ok(())
                })
                .await;
            if let Some(ready) = ready.lock().unwrap().take() {
                let _ = ready
                    .send(Err(result.err().map(failure).unwrap_or_else(|| {
                        "The agent stopped before initialization.".into()
                    })));
            }
        }));
        match tokio::time::timeout(Duration::from_secs(30), initialized).await {
            Ok(Ok(Ok((peer, info)))) => Ok(Arc::new(Self {
                peer,
                questions,
                info,
                events: Mutex::new(receiver),
                task,
                closed,
                active,
                login_notice,
                states,
            })),
            result => {
                task.0.abort();
                Err(match result {
                    Ok(Ok(Err(error))) => error,
                    _ => "The agent did not initialize within 30 seconds. Check its executable and installation.".into(),
                })
            }
        }
    }

    pub fn close(&self) {
        let _ = self.closed.send(true);
        self.task.0.abort();
    }

    pub fn is_closed(&self) -> bool {
        self.task.0.is_finished()
    }

    pub async fn extension(&self, method: &str, params: Value) -> Result<Value, String> {
        tokio::time::timeout(Duration::from_secs(30), self.extension_wait(method, params))
            .await
            .map_err(|_| "The agent setup request timed out.".to_string())?
    }

    pub async fn extension_wait(&self, method: &str, params: Value) -> Result<Value, String> {
        self.peer
            .send_request(UntypedMessage::new(method, params).map_err(failure)?)
            .block_task()
            .await
            .map_err(failure)
    }

    pub async fn authenticate(&self, method: &str) -> Result<(), String> {
        self.peer
            .send_request(AuthenticateRequest::new(method.to_string()))
            .block_task()
            .await
            .map_err(failure)?;
        Ok(())
    }

    pub async fn session(
        &self,
        config: &Config,
        server: crate::config::ToolConnection,
        previous: Option<&str>,
    ) -> Result<String, String> {
        self.validate_directories(config)?;
        let mcp = if self.info.agent_capabilities.mcp_capabilities.http {
            vec![McpServer::Http(
                McpServerHttp::new("lince", server.url).headers(vec![HttpHeader::new(
                    "Authorization",
                    format!("Bearer {}", server.token.0),
                )]),
            )]
        } else if let Some(command) = crate::adapters::bundled_executable() {
            vec![McpServer::Stdio(
                McpServerStdio::new("lince", command)
                    .args(vec!["--mcp-stdio".into()])
                    .env(vec![
                        EnvVariable::new("LINCE_MCP_URL", server.url),
                        EnvVariable::new("LINCE_MCP_TOKEN", server.token.0.clone()),
                    ]),
            )]
        } else {
            return Err("Lince's stdio tool bridge is unavailable in this executable. Chat-only session checks still work.".into());
        };
        let (id, options) = if let Some(previous) =
            previous.filter(|_| self.info.agent_capabilities.load_session)
        {
            let response = self
                .peer
                .send_request(
                    LoadSessionRequest::new(previous.to_string(), &config.directory)
                        .additional_directories(config.additional_directories.clone())
                        .mcp_servers(mcp),
                )
                .block_task()
                .await
                .map_err(failure)?;
            (
                previous.to_string(),
                response.config_options.unwrap_or_default(),
            )
        } else {
            let response = self
                .peer
                .send_request(
                    NewSessionRequest::new(&config.directory)
                        .additional_directories(config.additional_directories.clone())
                        .mcp_servers(mcp)
                        .meta(config.session_meta.clone()),
                )
                .block_task()
                .await
                .map_err(failure)?;
            (
                response.session_id.to_string(),
                response.config_options.unwrap_or_default(),
            )
        };
        let mut session_options = SessionOptions {
            session: id.clone(),
            options,
        };
        self.apply_options(&mut session_options, &config.options)
            .await?;
        self.remember_options(&session_options).await;
        let mut events = self.events.lock().await;
        while let Ok(event) = events.try_recv() {
            if let Event::Permission(_, reply) = event {
                let _ = reply.send(RequestPermissionResponse::new(
                    RequestPermissionOutcome::Cancelled,
                ));
            }
        }
        Ok(id)
    }

    pub async fn prompt(
        &self,
        session: &str,
        prompt: String,
        output: &dyn Output,
        mut stop: watch::Receiver<bool>,
    ) -> Result<String, String> {
        if prompt.len() > 512 * 1024 {
            return Err(
                "This conversation exceeds the agent context limit. Start a new thread.".into(),
            );
        }
        let result = self
            .run_prompt(
                session,
                vec![ContentBlock::Text(TextContent::new(prompt))],
                output,
                &mut stop,
            )
            .await;
        if result.is_err() {
            self.close();
        }
        result
    }

    async fn run_prompt(
        &self,
        session: &str,
        prompt: Vec<ContentBlock>,
        output: &dyn Output,
        stop: &mut watch::Receiver<bool>,
    ) -> Result<String, String> {
        let mut events = self.events.lock().await;
        if *stop.borrow() {
            return Err("Stopped by you.".into());
        }
        self.active.store(true, Ordering::Release);
        let _active = Active(self.active.clone());
        let response = self
            .peer
            .send_request(PromptRequest::new(session.to_string(), prompt))
            .block_task();
        tokio::pin!(response);
        let mut buffer = Buffer::default();
        let mut flush = tokio::time::interval(Duration::from_millis(60));
        flush.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                biased;
                _ = cancelled(stop) => {
                    let _ = self.peer.send_notification(CancelNotification::new(session.to_string()));
                    if tokio::time::timeout(Duration::from_secs(5), &mut response).await.is_err() {
                        self.close();
                    }
                    if buffer.dirty { output.update(&buffer.text).await?; }
                    return Err("Stopped by you. Completed tool operations remain saved.".into());
                }
                event = events.recv() => {
                    let Some(event) = event else { return Err("The agent connection closed.".into()) };
                    if let Err(error) = self.apply(event, session, &mut buffer, output, stop).await {
                        if buffer.dirty { output.update(&buffer.text).await?; }
                        return Err(error);
                    }
                }
                _ = flush.tick(), if buffer.dirty => {
                    output.update(&buffer.text).await?;
                    buffer.dirty = false;
                }
                result = &mut response => {
                    while let Ok(event) = events.try_recv() {
                        self.apply(event, session, &mut buffer, output, stop).await?;
                    }
                    if buffer.dirty { output.update(&buffer.text).await?; }
                    let reply = result.map_err(failure)?;
                    if reply.stop_reason != StopReason::EndTurn {
                        return Err(format!("The agent stopped with {:?}.", reply.stop_reason));
                    }
                    return Ok(buffer.text);
                }
            }
        }
    }

    async fn apply(
        &self,
        event: Event,
        session: &str,
        buffer: &mut Buffer,
        output: &dyn Output,
        stop: &mut watch::Receiver<bool>,
    ) -> Result<(), String> {
        match event {
            Event::Update(notice) if notice.session_id.to_string() == session => {
                match notice.update {
                    SessionUpdate::AgentMessageChunk(chunk) => {
                        if chunk.message_id.as_ref().is_some_and(|id| {
                            buffer.message_id.as_deref() != Some(id.to_string().as_str())
                        }) {
                            if buffer.dirty {
                                output.update(&buffer.text).await?;
                                buffer.dirty = false;
                            }
                            buffer.message_id = chunk.message_id.as_ref().map(ToString::to_string);
                            output.activity(json!({"sessionUpdate":"message_boundary","messageId":chunk.message_id})).await?;
                        }
                        if let ContentBlock::Text(chunk) = chunk.content {
                            if buffer.text.len() + chunk.text.len() > 60_000 {
                                self.close();
                                return Err(
                                    "The agent reply reached Lince's message size limit.".into()
                                );
                            }
                            buffer.text.push_str(&chunk.text);
                            buffer.dirty = true;
                        } else {
                            output.content(content::from_agent(chunk.content)?).await?;
                        }
                    }
                    update => {
                        if buffer.dirty {
                            output.update(&buffer.text).await?;
                            buffer.dirty = false;
                        }
                        output
                            .activity(serde_json::to_value(update).map_err(failure)?)
                            .await?
                    }
                }
            }
            Event::Question(request, reply) => {
                if request.session.as_deref() != Some(session) {
                    let _ = reply.send(CreateElicitationResponse::new(ElicitationAction::Cancel));
                    return Ok(());
                }
                if buffer.dirty {
                    output.update(&buffer.text).await?;
                    buffer.dirty = false;
                }
                let answer = tokio::select! {
                    _ = cancelled(stop) => Answer::Cancel,
                    result = output.question(request.clone()) => result?,
                };
                let _ = reply.send(request.response(answer)?);
            }
            Event::Permission(request, reply) => {
                if request.session_id.to_string() != session {
                    let _ = reply.send(RequestPermissionResponse::new(
                        RequestPermissionOutcome::Cancelled,
                    ));
                    return Ok(());
                }
                let permission = Permission {
                    id: request.tool_call.tool_call_id.to_string(),
                    title: request.tool_call.fields.title.clone().unwrap_or_else(|| "Agent tool request".into()),
                    details: json!({"input":request.tool_call.fields.raw_input,"locations":request.tool_call.fields.locations,"content":request.tool_call.fields.content}).to_string(),
                    options: request.options.iter().map(|choice| Choice { id: choice.option_id.to_string(), label: choice.name.clone() }).collect(),
                };
                let chosen = tokio::select! {
                    _ = cancelled(stop) => None,
                    result = output.permission(permission) => result?,
                };
                let outcome = chosen
                    .filter(|id| {
                        request
                            .options
                            .iter()
                            .any(|choice| choice.option_id.to_string() == *id)
                    })
                    .map(|id| {
                        RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(id))
                    })
                    .unwrap_or(RequestPermissionOutcome::Cancelled);
                let _ = reply.send(RequestPermissionResponse::new(outcome));
            }
            _ => {}
        }
        Ok(())
    }
}
