mod catalog;
mod messages;
mod text;

use std::{collections::BTreeMap, sync::Arc};

use engine::{Engine, actions::Action};
use fiote::{
    provider::ToolDefinition,
    tools::{Registry, Tool},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

use crate::{ClientMessage, ServerMessage, Session};

const MAX_RESULT_BYTES: usize = 128 * 1024;
const MAX_READS: usize = 32;
const MAX_SNAPSHOT_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Serialize)]
pub struct Context {
    pub agent: String,
    pub record: String,
    pub thread: String,
}

struct Read {
    record: Value,
    snapshot: Vec<u8>,
    writable: Vec<String>,
}

struct State {
    session: Session,
    reads: BTreeMap<String, Read>,
    receipts: BTreeMap<String, ([u8; 32], Result<Value, String>)>,
    receipt_bytes: usize,
    messages: BTreeMap<String, messages::Stream>,
    closed: bool,
}

#[derive(Clone)]
pub struct NativeTools {
    engine: Arc<Engine>,
    state: Arc<Mutex<State>>,
    context: Context,
    instructions: Option<Arc<String>>,
}

impl NativeTools {
    pub(crate) fn new(engine: Arc<Engine>, session: Session, context: Context) -> Self {
        Self {
            engine,
            state: Arc::new(Mutex::new(State {
                session,
                reads: BTreeMap::new(),
                receipts: BTreeMap::new(),
                receipt_bytes: 0,
                messages: BTreeMap::new(),
                closed: false,
            })),
            context,
            instructions: None,
        }
    }

    pub fn with_instructions(mut self, instructions: String) -> Self {
        self.instructions = Some(Arc::new(instructions));
        self
    }

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
    }

    pub async fn close(&self) {
        let result = engine::operation_origin::fiote(
            &self.context.agent,
            &self.context.thread,
            self.engine.access_scope(true, async {
                let mut state = self.state.lock().await;
                state.closed = true;
                state
                    .interrupt_messages()
                    .await
                    .map_err(engine::EngineError::Consequence)
            }),
        )
        .await;
        if let Err(error) = result {
            tracing::error!(%error, "Could not interrupt agent messages");
        }
    }

    pub async fn attach_message(&self, uid: &str) -> Result<(), String> {
        self.engine
            .access_scope(true, async {
                let mut state = self.state.lock().await;
                if state.closed {
                    return Err(engine::EngineError::Forbidden(
                        "This agent connection is closed.".into(),
                    ));
                }
                state
                    .attach_message(uid, &self.context)
                    .await
                    .map_err(engine::EngineError::Consequence)
            })
            .await
            .map_err(error)
    }
}

#[derive(Clone, Copy)]
enum Kind {
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
        let (name, description, schema) = match self.kind {
            Kind::Message => (
                "lince_message",
                "Write an attributed reply in this connection's thread. Start creates a writing message and returns its UID. Update supplies the complete text generated so far; only changes to your own text are applied through CRDT, preserving concurrent user edits. Finish or interrupt saves the final text and state. Reuse the same request_id to retry. Message UIDs must have been started by this connection.",
                json!({"type":"object","properties":{"request_id":{"type":"string"},"operation":{"type":"string","enum":["start","update","finish","interrupt"]},"message_uid":{"type":"string"},"text":{"type":"string"}},"required":["request_id","operation","text"],"additionalProperties":false}),
            ),
            Kind::Release => (
                "lince_release_reads",
                "Release collaborative read snapshots you no longer need. Released read_ids cannot be used for later changes. Existing write retry receipts are retained. At most 32 snapshots may be open at once.",
                json!({"type":"object","properties":{"read_ids":{"type":"array","items":{"type":"string"},"maxItems":32}},"required":["read_ids"],"additionalProperties":false}),
            ),
            Kind::Describe => (
                "lince_describe",
                "Discover Lince's native API. With no action, returns the Record fields, action names and usage. With an action name, returns its exact JSON schema, including referenced types. Protein and Actions are the same API the interface uses.",
                json!({"type":"object","properties":{"action":{"type":"string"}},"additionalProperties":false}),
            ),
            Kind::Query => (
                "lince_query",
                "Run a Protein query with this session's access. Read saved Proteins, Records, assertions, people, threads, facts and other sources. Results are bounded; narrow the query or select fewer fields if too large. Use lince_read_record before editing. Use lince_describe for the Protein schema.",
                json!({"type":"object","properties":{"protein":{"type":"string","description":"A Protein query encoded as JSON, using the schema from lince_describe."},"saved":{"type":"string"}},"additionalProperties":false}),
            ),
            Kind::Read => (
                "lince_read_record",
                "Read a Record through Protein and open its collaborative document. Returns a read_id for subsequent text or property changes. Read IDs are scoped to this turn. Use stable Record UIDs. Concurrent text edits merge; reread after each committed edit to see the merged result.",
                json!({"type":"object","properties":{"record_uid":{"type":"string"},"extensions":{"type":"array","items":{"type":"string"},"maxItems":8,"description":"Extension namespaces to read and guard, required before set-extension."}},"required":["record_uid"],"additionalProperties":false}),
            ),
            Kind::Edit => (
                "lince_edit_text",
                "Apply precise replacements to the head or body from a lince_read_record snapshot through the normal collaborative Action. Each before passage must appear exactly once in that snapshot; use an empty before only for an empty field. Edits in one call refer to the same snapshot and must not overlap. Concurrent user insertions are preserved. Use a stable request_id to retry the same operation.",
                json!({"type":"object","properties":{"read_id":{"type":"string"},"request_id":{"type":"string"},"edits":{"type":"array","minItems":1,"maxItems":32,"items":{"type":"object","properties":{"field":{"type":"string","enum":["head","body"]},"before":{"type":"string"},"after":{"type":"string"}},"required":["field","before","after"],"additionalProperties":false}}},"required":["read_id","request_id","edits"],"additionalProperties":false}),
            ),
            Kind::Action => (
                "lince_action",
                "Execute a native Lince Action under the same session checks as the interface. Discover its schema first. Supply read_ids for existing Records you change; scalar/property changes reject stale reads. Use lince_edit_text for existing titles/descriptions. Use preview-area-transition then apply-area-transition for atomic state/quantity changes. Retries with the same request_id and arguments return the original result within this turn. Never retry an uncertain operation with a new ID without checking its effect.",
                json!({"type":"object","properties":{"request_id":{"type":"string"},"action":{"type":"string","description":"A native Action encoded as JSON, using the schema from lince_describe."},"read_ids":{"type":"array","items":{"type":"string"},"maxItems":32}},"required":["request_id","action","read_ids"],"additionalProperties":false}),
            ),
            Kind::Instructions => (
                "lince_instructions",
                "Reload the complete pinned Fiote and ancestor instructions after context compaction. These persist outside conversation summaries. Call this before continuing work after compaction.",
                json!({"type":"object","properties":{},"additionalProperties":false}),
            ),
            Kind::Sync => (
                "lince_sync_status",
                "Inspect Lince's existing sync activity. To discover configured file sync, query visible Organ Records with include.extension.namespace = lince.file_sync and use the file-sync-status Action. A saved database change does not imply its file export or remote delivery has completed.",
                json!({"type":"object","properties":{},"additionalProperties":false}),
            ),
        };
        ToolDefinition {
            name: name.into(),
            description: if matches!(self.kind, Kind::Instructions) {
                self.native.instructions.as_ref().map_or_else(|| description.into(), |instructions| format!("{description}\n\nPersistent session instructions (retain across compaction):\n{instructions}"))
            } else {
                description.into()
            },
            schema,
        }
    }

    async fn run(&self, arguments: Value) -> Result<Value, String> {
        if serde_json::to_vec(&arguments).map_err(error)?.len() > MAX_RESULT_BYTES {
            return Err("Tool arguments exceed 128 KiB.".into());
        }
        let write = matches!(self.kind, Kind::Action | Kind::Edit | Kind::Message);
        let result = engine::operation_origin::fiote(
            &self.native.context.agent,
            &self.native.context.thread,
            self.native.engine.access_scope(write, async {
                let mut state = self.native.state.lock().await;
                if state.closed {
                    return Err(engine::EngineError::Forbidden(
                        "This agent connection is closed.".into(),
                    ));
                }
                if !state.session.subject_may_act().await {
                    return Err(engine::EngineError::Forbidden(
                        "Session access was removed.".into(),
                    ));
                }
                let result = match self.kind {
                    Kind::Instructions => match serde_json::from_value::<Empty>(arguments) {
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
                    Kind::Edit => state.edit(arguments).await,
                    Kind::Message => state.message(arguments, &self.native.context).await,
                    Kind::Release => state.release(arguments),
                    Kind::Action => state.action(arguments).await,
                    Kind::Sync => match serde_json::from_value::<Empty>(arguments) {
                        Ok(_) => response(
                            state
                                .session
                                .handle(ClientMessage::SyncInspect {
                                    id: nucleus::new_uid("query"),
                                    before: None,
                                })
                                .await,
                        ),
                        Err(err) => Err(error(err)),
                    },
                };
                result.map_err(engine::EngineError::Consequence)
            }),
        )
        .await
        .map_err(error)?;
        bounded(result)
    }
}

fn error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn bounded(value: Value) -> Result<Value, String> {
    if serde_json::to_vec(&value).map_err(error)?.len() > MAX_RESULT_BYTES {
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
        Some(message) => serde_json::to_value(message).map_err(error),
        None => Err("Lince returned no result.".into()),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Empty {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct QueryArguments {
    #[serde(default, deserialize_with = "json_argument")]
    protein: Option<protein::Protein>,
    saved: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadArguments {
    record_uid: String,
    #[serde(default)]
    extensions: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ActionArguments {
    request_id: String,
    #[serde(deserialize_with = "json_argument")]
    action: Action,
    read_ids: Vec<String>,
}

fn json_argument<'de, D: serde::Deserializer<'de>, T: serde::de::DeserializeOwned>(
    deserializer: D,
) -> Result<T, D::Error> {
    let value = Value::deserialize(deserializer)?;
    match value {
        Value::String(text) => serde_json::from_str(&text),
        value => serde_json::from_value(value),
    }
    .map_err(serde::de::Error::custom)
}

impl State {
    async fn instructions(
        &mut self,
        context: &Context,
        pinned: Option<&str>,
    ) -> Result<Value, String> {
        let mut value = self
            .extension(&context.thread, "lince.fiote-session")
            .await?;
        if value["fiote"] != context.record {
            return Err("No instructions are pinned to this Fiote session.".into());
        }
        for source in value["sources"]
            .as_array()
            .ok_or("Missing instruction sources.")?
        {
            self.record(
                source["record"]
                    .as_str()
                    .ok_or("Missing instruction Record.")?,
            )
            .await?;
        }
        value["system"] = pinned
            .ok_or("Reconnect this session to restore its pinned instructions.")?
            .into();
        Ok(value)
    }

    async fn query(&mut self, arguments: Value) -> Result<Value, String> {
        let args: QueryArguments = serde_json::from_value(arguments).map_err(error)?;
        let protein = match (args.protein, args.saved) {
            (Some(protein), None) => protein,
            (None, Some(name)) => {
                let selector = if nucleus::valid_uid(&name, "r") {
                    json!({"uid_eq":name})
                } else {
                    json!({"slug_eq":name})
                };
                let lookup = serde_json::from_value(json!({
                    "source":"record","where":[selector,{"kind_eq":"protein"}],
                    "fields":["uid","extension"],"include":{"extension":{"namespace":"lince.protein"}},"limit":1
                }))
                .map_err(error)?;
                let result = self.subscribe(lookup).await?;
                let ast = result["rows"]
                    .as_array()
                    .and_then(|rows| rows.first())
                    .map(|record| record["extension"].clone())
                    .ok_or("The saved Protein is missing or is not visible to this session.")?;
                serde_json::from_value(ast).map_err(error)?
            }
            _ => return Err("Supply either protein or saved.".into()),
        };
        self.subscribe(protein).await
    }

    async fn subscribe(&mut self, mut protein: protein::Protein) -> Result<Value, String> {
        let id = nucleus::new_uid("query");
        let limit = protein.limit.unwrap_or(50).min(100);
        protein.limit = Some(limit);
        let result = response(
            self.session
                .handle(ClientMessage::Subscribe {
                    id: id.clone(),
                    protein,
                })
                .await,
        );
        self.session.handle(ClientMessage::Unsubscribe { id }).await;
        let mut result = result?;
        result["limit"] = json!(limit);
        result["may_have_more"] = json!(
            result["rows"]
                .as_array()
                .is_some_and(|rows| rows.len() == limit)
        );
        bounded(result)
    }

    async fn record(&mut self, uid: &str) -> Result<Value, String> {
        if !nucleus::valid_uid(uid, "r") {
            return Err("Use a stable Record UID from a Protein result.".into());
        }
        let result = self
            .query(json!({"protein":{"source":"record","where":[{"uid_eq":uid}],"limit":1}}))
            .await?;
        result["rows"]
            .as_array()
            .and_then(|rows| rows.first())
            .cloned()
            .ok_or_else(|| "The Record is missing or is not visible to this session.".into())
    }

    async fn read(&mut self, arguments: Value) -> Result<Value, String> {
        use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
        let args: ReadArguments = serde_json::from_value(arguments).map_err(error)?;
        if self.reads.len() >= MAX_READS {
            return Err(
                "This turn has reached its 32 Record snapshots. Continue in another turn.".into(),
            );
        }
        if args.extensions.len() > 8
            || args
                .extensions
                .iter()
                .any(|name| name.is_empty() || name.len() > 128)
        {
            return Err("Read at most eight extension namespaces of 1–128 bytes.".into());
        }
        let mut record = self.record(&args.record_uid).await?;
        for namespace in args.extensions {
            record["extensions"][&namespace] = self.extension(&args.record_uid, &namespace).await?;
        }
        let state = response(
            self.session
                .handle(ClientMessage::CollabJoin {
                    id: nucleus::new_uid("read"),
                    record_uid: args.record_uid.clone(),
                })
                .await,
        );
        self.session
            .handle(ClientMessage::CollabLeave {
                record_uid: args.record_uid,
            })
            .await;
        let state = state?;
        let snapshot = B64
            .decode(
                state["snapshot_base64"]
                    .as_str()
                    .ok_or("Missing collaborative snapshot.")?,
            )
            .map_err(error)?;
        if snapshot.len()
            + self
                .reads
                .values()
                .map(|read| read.snapshot.len())
                .sum::<usize>()
            > MAX_SNAPSHOT_BYTES
        {
            return Err("Collaborative snapshots exceed this turn's memory limit.".into());
        }
        let writable = serde_json::from_value(state["writable"].clone()).map_err(error)?;
        let read_id = nucleus::new_uid("read");
        let result = bounded(
            json!({"record":record,"read_id":read_id,"change_id":nucleus::new_uid("op"),"text_version":state["version"],"writable_text":writable}),
        )?;
        self.reads.insert(
            read_id,
            Read {
                record,
                snapshot,
                writable,
            },
        );
        Ok(result)
    }

    async fn extension(&mut self, uid: &str, namespace: &str) -> Result<Value, String> {
        let result = self
            .query(json!({"protein":{
            "source":"record","where":[{"uid_eq":uid}],"fields":["uid","extension"],
                "include":{"extension":{"namespace":namespace}},"limit":1
            }}))
            .await?;
        result["rows"]
            .as_array()
            .and_then(|rows| rows.first())
            .map(|row| row["extension"].clone())
            .ok_or_else(|| "The Record is no longer visible.".into())
    }

    fn previous(&self, id: &str, args: &Value) -> Result<Option<Value>, String> {
        if id.is_empty()
            || id.len() > 128
            || !id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err(
                "Use a request_id of 1–128 letters, digits, hyphens or underscores.".into(),
            );
        }
        if let Some((original, result)) = self.receipts.get(id) {
            if *original != fingerprint(args) {
                return Err("This request_id was already used for a different operation.".into());
            }
            return result.clone().map(Some);
        }
        if self.receipts.len() >= 4096 || self.receipt_bytes >= 16 * 1024 * 1024 {
            return Err("This connection has reached its operation budget. Close it and open a new connection. Inspect uncertain effects before retrying.".into());
        }
        Ok(None)
    }

    fn remember(&mut self, id: String, arguments: &Value, result: Result<Value, String>) {
        self.receipt_bytes += id.len()
            + 32
            + match &result {
                Ok(value) => {
                    serde_json::to_vec(value).map_or(MAX_RESULT_BYTES, |bytes| bytes.len())
                }
                Err(error) => error.len(),
            };
        self.receipts.insert(id, (fingerprint(arguments), result));
    }

    async fn action(&mut self, arguments: Value) -> Result<Value, String> {
        let args: ActionArguments = serde_json::from_value(arguments.clone()).map_err(error)?;
        if matches!(&args.action, Action::ConfigureFiote { .. })
            || matches!(&args.action, Action::SetExtension { namespace, .. } if namespace.starts_with("lince.fiote"))
        {
            return Err("Fiote identity, session instructions and dispatch settings are changed by the operator in Fiote settings.".into());
        }
        if let Some(previous) = self.previous(&args.request_id, &arguments)? {
            return Ok(previous);
        }
        if matches!(
            args.action,
            Action::EditRecordText { .. }
                | Action::ChangeRecord {
                    request: engine::record_change::Request {
                        mutation: engine::record_change::Mutation::Text { .. },
                        ..
                    }
                }
        ) {
            return Err(
                "Use lince_read_record and lince_edit_text for collaborative text edits.".into(),
            );
        }
        if args.read_ids.len() > MAX_READS {
            return Err("Too many read_ids.".into());
        }
        let mut observed = Vec::new();
        for id in &args.read_ids {
            let read = self
                .reads
                .get(id)
                .ok_or("Unknown read_id. Read the Record in this turn first.")?;
            observed.push(read.record.clone());
        }
        for record in &observed {
            let mut current = self
                .record(record["uid"].as_str().ok_or("Missing Record UID.")?)
                .await?;
            if let Some(extensions) = record["extensions"].as_object() {
                for namespace in extensions.keys() {
                    current["extensions"][namespace] = self
                        .extension(record["uid"].as_str().unwrap(), namespace)
                        .await?;
                }
            }
            let replaces_text = matches!(
                args.action,
                Action::DeleteRecord { .. }
                    | Action::ReviseMessage { .. }
                    | Action::ReviseMessageDraft { .. }
                    | Action::SaveProtein { .. }
            );
            let unchanged =
                properties(record.clone(), replaces_text) == properties(current, replaces_text);
            if !unchanged {
                return Err("Record properties changed since this read. Read it again and reconsider the operation.".into());
            }
        }
        if let Some(target) = record_target(&args.action) {
            if !observed
                .iter()
                .any(|record| record["uid"].as_str() == Some(target))
            {
                return Err("Supply a read_id for the Record being changed and use its stable UID as the target.".into());
            }
        }
        if let Action::RetractAssertion { assertion } = &args.action {
            if !observed.iter().any(|record| {
                record["assertions"].as_array().is_some_and(|assertions| {
                    assertions.iter().any(|value| value["uid"] == *assertion)
                })
            }) {
                return Err("Read the assertion's subject Record and supply its read_id before retracting it.".into());
            }
        }
        if let Action::SetExtension {
            target, namespace, ..
        } = &args.action
        {
            if !observed.iter().any(|record| {
                record["uid"] == *target && record["extensions"].get(namespace).is_some()
            }) {
                return Err(
                    "Read this extension namespace with lince_read_record before changing it."
                        .into(),
                );
            }
        }
        if let Action::SaveProtein { slug, .. } = &args.action {
            let existing = self.query(json!({"protein":{"source":"record","where":[{"slug_eq":slug}],"fields":["uid"],"limit":1}})).await?;
            if let Some(record) = existing["rows"].as_array().and_then(|rows| rows.first()) {
                if !observed.iter().any(|read| {
                    read["uid"] == record["uid"]
                        && read["extensions"].get("lince.protein").is_some()
                }) {
                    return Err("Read the existing Protein Record and its lince.protein extension before replacing its query.".into());
                }
            }
        }
        let result = response(
            self.session
                .handle(ClientMessage::Act {
                    id: args.request_id.clone(),
                    action: args.action,
                })
                .await,
        );
        self.remember(args.request_id, &arguments, result.clone());
        result
    }
}

fn fingerprint(arguments: &Value) -> [u8; 32] {
    let mut value = arguments.clone();
    value.sort_all_objects();
    Sha256::digest(serde_json::to_vec(&value).expect("JSON value serialization")).into()
}

fn properties(mut record: Value, include_text: bool) -> Value {
    if let Some(object) = record.as_object_mut() {
        for key in ["updated_at", "spent_seconds"] {
            object.remove(key);
        }
        if !include_text {
            object.remove("head");
            object.remove("body");
        }
    }
    record
}

fn record_target(action: &Action) -> Option<&str> {
    match action {
        Action::ChangeRecord { request } => Some(&request.record_uid),
        Action::ReviseMessage { message, .. } => Some(message),
        Action::ReviseMessageDraft { draft, .. } => Some(draft),
        Action::SetQuantity { target, .. }
        | Action::SetQuantityExact { target, .. }
        | Action::AddQuantity { target, .. }
        | Action::Activate { target }
        | Action::Deactivate { target }
        | Action::DeleteRecord { target }
        | Action::SetSlug { target, .. }
        | Action::SetUnit { target, .. }
        | Action::SetExtension { target, .. }
        | Action::SetPlace { target, .. } => Some(target),
        Action::AssertRecord { subject, .. }
        | Action::RetractRecord { subject, .. }
        | Action::RefineAssertion { subject, .. }
        | Action::SetIdentity { subject, .. }
        | Action::TransitionRecord { subject, .. } => Some(subject),
        _ => None,
    }
}
