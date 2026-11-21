mod actions;
mod assistant;
mod catalog;
mod messages;
mod reads;
mod receipts;
mod text;

use std::{
    collections::BTreeMap,
    fmt,
    future::Future,
    sync::{
        Arc, PoisonError, RwLock,
        atomic::{AtomicBool, Ordering},
    },
};

use engine::{
    Engine,
    EngineError::{Consequence, Forbidden},
    actions::Action,
    operation_origin::{fiote as fiote_origin, fiote_from_message},
};
use fiote::{
    provider::ToolDefinition,
    tools::{Registry, Tool},
};
use nucleus::{json::encoded_len, new_uid};
use serde::{Deserialize, Serialize};
use serde_json::{Value, from_str, from_value, to_value};
use tokio::sync::Mutex;

use crate::{ClientMessage, ServerMessage, Session};

use self::{
    actions::ActionArguments,
    reads::{Read, Reads},
    receipts::Receipts,
};

const MAX_RESULT_BYTES: usize = 128 * 1024;

/// Identifiers that scope a Fiote's native tools to one conversation.
#[derive(Clone, Debug, Serialize)]
pub struct Context {
    /// Person or agent attributed as the operator.
    pub agent: String,
    /// Fiote Record that owns the conversation.
    pub record: String,
    /// Thread in which the tools can publish replies.
    pub thread: String,
}

struct State {
    context: Arc<Context>,
    session: Session,
    reads: Reads,
    receipts: Receipts,
    messages: BTreeMap<String, messages::Stream>,
    closed: bool,
}

/// A handle to one Fiote's scoped native tool session.
#[derive(Clone)]
pub struct NativeTools {
    engine: Arc<Engine>,
    state: Arc<Mutex<State>>,
    context: Arc<Context>,
    #[expect(
        clippy::rc_buffer,
        reason = "share the owned pinned instructions without copying their buffer"
    )]
    instructions: Option<Arc<String>>,
    closed: Arc<AtomicBool>,
    source_message: Arc<RwLock<Option<Arc<str>>>>,
    assistant: Option<Arc<dyn crate::fiote::Service>>,
}

impl fmt::Debug for NativeTools {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeTools").finish_non_exhaustive()
    }
}

impl NativeTools {
    pub(crate) fn new(engine: Arc<Engine>, session: Session, context: Context) -> Self {
        let context = Arc::new(context);
        Self {
            engine,
            state: Arc::new(Mutex::new(State {
                context: Arc::clone(&context),
                session,
                reads: Reads::default(),
                receipts: Receipts::default(),
                messages: BTreeMap::new(),
                closed: false,
            })),
            context,
            instructions: None,
            closed: Arc::new(AtomicBool::new(false)),
            source_message: Default::default(),
            assistant: None,
        }
    }

    /// Pins the complete instructions for this conversation.
    pub fn with_instructions(mut self, instructions: String) -> Self {
        self.instructions = Some(Arc::new(instructions));
        self
    }

    /// Enables assistant discovery and handoff tools through the given service.
    pub fn with_fiote_service(mut self, service: Arc<dyn crate::fiote::Service>) -> Self {
        self.assistant = Some(service);
        self
    }

    /// Updates the message attributed as the source of subsequent work.
    pub fn set_source_message(&self, uid: Option<&str>) {
        *self
            .source_message
            .write()
            .unwrap_or_else(PoisonError::into_inner) = uid.map(Arc::from);
    }

    async fn origin<F: Future>(&self, work: F) -> F::Output {
        let message = self
            .source_message
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        fiote_from_message(
            &self.context.agent,
            &self.context.thread,
            message.as_deref(),
            work,
        )
        .await
    }

    /// Adds this connection's native tools to the agent's registry.
    pub fn register(&self, registry: &mut Registry) {
        for kind in [
            Kind::Instructions,
            Kind::Describe,
            Kind::Query,
            Kind::Read,
            Kind::Edit,
            Kind::Action,
            Kind::Sync,
            Kind::Message,
            Kind::Release,
        ] {
            registry.register(NativeTool {
                native: self.clone(),
                kind,
            });
        }
        if self.assistant.is_some() {
            for kind in [Kind::Fiotes, Kind::Delegate, Kind::Handoffs] {
                registry.register(NativeTool {
                    native: self.clone(),
                    kind,
                });
            }
        }
    }

    /// Revokes this connection's tools and interrupts messages still being written.
    pub async fn close(&self) {
        self.closed.store(true, Ordering::Release);
        self.engine
            .revoke_canvas_requests(&self.context.agent, &self.context.thread);
        let result = fiote_origin(
            &self.context.agent,
            &self.context.thread,
            self.engine.access_scope(true, async {
                let mut state = self.state.lock().await;
                state.closed = true;
                state.interrupt_messages().await.map_err(Consequence)
            }),
        )
        .await;
        if let Err(error) = result {
            tracing::error!(%error, "Could not interrupt agent messages");
        }
    }

    /// Resumes a message that belongs to this connection's author and thread.
    ///
    /// # Errors
    /// Returns a diagnostic if access was removed or the message cannot be attached.
    pub async fn attach_message(&self, uid: &str) -> Result<(), String> {
        self.engine
            .access_scope(true, async {
                let mut state = self.state.lock().await;
                if state.closed || self.closed.load(Ordering::Acquire) {
                    return Err(Forbidden("This agent connection is closed.".into()));
                }
                state
                    .attach_message(uid, &self.context)
                    .await
                    .map_err(Consequence)
            })
            .await
            .map_err(error)
    }
}

#[derive(Clone, Copy)]
enum Kind {
    Fiotes,
    Delegate,
    Handoffs,
    Instructions,
    Describe,
    Query,
    Read,
    Edit,
    Action,
    Sync,
    Message,
    Release,
}

struct NativeTool {
    native: NativeTools,
    kind: Kind,
}

#[async_trait::async_trait]
impl Tool for NativeTool {
    fn definition(&self) -> ToolDefinition {
        self.kind
            .definition(self.native.instructions.as_deref().map(String::as_str))
    }

    async fn run(&self, arguments: Value) -> Result<Value, String> {
        if encoded_len(&arguments).map_err(error)? > MAX_RESULT_BYTES {
            return Err("Tool arguments exceed 128 KiB.".into());
        }
        if matches!(self.kind, Kind::Fiotes | Kind::Delegate | Kind::Handoffs) {
            return bounded(self.native.origin(self.run_assistant(arguments)).await?);
        }
        if matches!(self.kind, Kind::Action)
            && let Ok(args) = ActionArguments::deserialize(&arguments)
            && matches!(args.action, Action::PreviewKarmaProposal { .. })
        {
            let result = self
                .native
                .origin(self.run_preview(args, arguments))
                .await?;
            return bounded(result);
        }
        let write = matches!(self.kind, Kind::Action | Kind::Edit | Kind::Message);
        let result = self
            .native
            .origin(self.native.engine.access_scope(write, async {
                let mut state = self.native.state.lock().await;
                if state.closed || self.native.closed.load(Ordering::Acquire) {
                    return Err(Forbidden("This agent connection is closed.".into()));
                }
                if !state.session.subject_may_act().await {
                    return Err(Forbidden("Session access was removed.".into()));
                }
                let result = match self.kind {
                    Kind::Instructions => match from_value::<Empty>(arguments) {
                        Ok(_) => {
                            state
                                .instructions(
                                    &self.native.context,
                                    self.native.instructions.as_deref().map(String::as_str),
                                )
                                .await
                        }
                        Err(err) => Err(error(err)),
                    },
                    Kind::Describe => catalog::describe(arguments, &self.native.context),
                    Kind::Query => state.query(arguments).await,
                    Kind::Read => state.read(arguments).await,
                    Kind::Edit => state.edit(&arguments).await,
                    Kind::Message => state.message(&arguments, &self.native.context).await,
                    Kind::Release => state.release(arguments),
                    Kind::Action => state.action(&arguments).await,
                    Kind::Sync => match from_value::<Empty>(arguments) {
                        Ok(_) => response(
                            state
                                .session
                                .handle(ClientMessage::SyncInspect {
                                    id: new_uid("query"),
                                    before: None,
                                })
                                .await,
                        ),
                        Err(err) => Err(error(err)),
                    },
                    Kind::Fiotes | Kind::Delegate | Kind::Handoffs => {
                        Err("Assistant tools require their dedicated dispatch path.".into())
                    }
                };
                result.map_err(Consequence)
            }))
            .await
            .map_err(error)?;
        bounded(result)
    }
}

fn error(error: impl fmt::Display) -> String {
    error.to_string()
}

fn bounded(value: Value) -> Result<Value, String> {
    if encoded_len(&value).map_err(error)? > MAX_RESULT_BYTES {
        Err("Result exceeds 128 KiB. Narrow the query or select fewer fields. No partial data was returned.".into())
    } else {
        Ok(value)
    }
}

fn response(messages: Vec<ServerMessage>) -> Result<Value, String> {
    match messages.into_iter().next() {
        Some(ServerMessage::Error { message, code, .. }) => Err(format!(
            "{}: {message}",
            code.unwrap_or_else(|| "lince".into())
        )),
        Some(message) => to_value(message).map_err(error),
        None => Err("Lince returned no result.".into()),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Empty {}

fn json_argument<'de, D: serde::Deserializer<'de>, T: serde::de::DeserializeOwned>(
    deserializer: D,
) -> Result<T, D::Error> {
    let value = Value::deserialize(deserializer)?;
    match value {
        Value::String(text) => from_str(&text),
        value => from_value(value),
    }
    .map_err(serde::de::Error::custom)
}
