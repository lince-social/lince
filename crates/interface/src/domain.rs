pub use crate::domain_model::{
    DomainActionReceipt, DomainConversation, DomainDraftTiming, DomainMessage, DomainMessageDraft,
    DomainMessageState, DomainRecord, DomainThread,
};
use engine::actions::Action;
use futures::{SinkExt, StreamExt};
use protein::{ExtensionInclude, Include, Order, Predicate, Protein, Source, ThreadsInclude};
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use transport::{ClientMessage, ServerMessage};
use url::Url;

pub const OFFICIAL_RECORDS_SUBSCRIPTION: &str = "native-official-records";
pub const OFFICIAL_CONVERSATIONS_SUBSCRIPTION: &str = "native-official-conversations";
pub const OFFICIAL_MESSAGE_DRAFTS_SUBSCRIPTION: &str = "native-official-message-drafts";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DomainConnectionState {
    NotConfigured,
    Connecting,
    Live,
    Unavailable,
}

impl DomainConnectionState {
    pub fn label(self) -> &'static str {
        match self {
            Self::NotConfigured => "domain source not configured",
            Self::Connecting => "connecting to local Lince",
            Self::Live => "live Protein subscription",
            Self::Unavailable => "local Lince unavailable",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct NativeDomainFacts {
    pub connection: DomainConnectionState,
    pub endpoint: Option<String>,
    pub records: usize,
    pub conversations: usize,
    pub message_drafts: usize,
    pub snapshots: u64,
    pub updates: u64,
    pub action_requests: u64,
    pub action_acknowledgements: u64,
    pub action_failures: u64,
    pub pending_actions: usize,
    pub refusals: u64,
    pub last_error: Option<String>,
}

enum DomainEvent {
    Connecting,
    Connected,
    Message(ServerMessage),
    Unavailable(String),
}

pub struct NativeDomainClient {
    connection: DomainConnectionState,
    endpoint: Option<String>,
    records: Vec<DomainRecord>,
    conversations: Vec<DomainConversation>,
    message_drafts: Vec<DomainMessageDraft>,
    snapshots: u64,
    updates: u64,
    action_requests: u64,
    action_acknowledgements: u64,
    action_failures: u64,
    next_action_id: u64,
    pending_actions: BTreeMap<String, Action>,
    action_receipts: Vec<DomainActionReceipt>,
    refusals: u64,
    last_error: Option<String>,
    events: Option<mpsc::Receiver<DomainEvent>>,
    commands: Option<mpsc::Sender<ClientMessage>>,
}

impl NativeDomainClient {
    pub fn connect(base_url: Option<String>) -> Self {
        let Some(base_url) = base_url else {
            return Self::not_configured();
        };
        let endpoint = match transport_endpoint(base_url.as_str()) {
            Ok(endpoint) => endpoint,
            Err(error) => return Self::unavailable(Some(base_url), error),
        };
        let handle = match tokio::runtime::Handle::try_current() {
            Ok(handle) => handle,
            Err(error) => {
                return Self::unavailable(
                    Some(endpoint),
                    format!("native domain runtime is unavailable: {error}"),
                );
            }
        };
        let (events_tx, events_rx) = mpsc::channel(16);
        let (commands_tx, commands_rx) = mpsc::channel(16);
        handle.spawn(run_subscription(endpoint.clone(), events_tx, commands_rx));
        Self {
            connection: DomainConnectionState::Connecting,
            endpoint: Some(endpoint),
            records: Vec::new(),
            conversations: Vec::new(),
            message_drafts: Vec::new(),
            snapshots: 0,
            updates: 0,
            action_requests: 0,
            action_acknowledgements: 0,
            action_failures: 0,
            next_action_id: 1,
            pending_actions: BTreeMap::new(),
            action_receipts: Vec::new(),
            refusals: 0,
            last_error: None,
            events: Some(events_rx),
            commands: Some(commands_tx),
        }
    }

    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        let mut pending = Vec::new();
        if let Some(events) = self.events.as_mut() {
            for _ in 0..8 {
                let Ok(event) = events.try_recv() else {
                    break;
                };
                pending.push(event);
            }
        }
        for event in pending {
            changed |= self.apply(event);
        }
        changed
    }

    pub fn records(&self) -> &[DomainRecord] {
        self.records.as_slice()
    }

    pub fn conversations(&self) -> &[DomainConversation] {
        self.conversations.as_slice()
    }

    pub fn message_drafts(&self) -> &[DomainMessageDraft] {
        self.message_drafts.as_slice()
    }

    pub fn request_message(&mut self, thread: &str, body: &str) -> Result<String, String> {
        if self.connection != DomainConnectionState::Live {
            return Err("the local Lince Action connection is not live".into());
        }
        let thread = thread.trim();
        let body = body.trim();
        if thread.is_empty() || body.is_empty() {
            return Err("a live thread and non-empty message are required".into());
        }
        let action = Action::CreateMessage {
            thread: thread.into(),
            body: body.into(),
            author: None,
            state: nucleus::MessageState::Finished,
            parent: None,
            references: Vec::new(),
        };
        self.request_action("message", action)
    }

    pub fn request_create_draft(
        &mut self,
        conversation: &str,
        thread: &str,
        body: &str,
        pinned: bool,
        timing: DomainDraftTiming,
        position: u32,
    ) -> Result<String, String> {
        let timing = match timing {
            DomainDraftTiming::Now => nucleus::MessageDraftTiming::Now,
            DomainDraftTiming::NextSafePoint => nucleus::MessageDraftTiming::NextSafePoint,
            DomainDraftTiming::AfterTurn => nucleus::MessageDraftTiming::AfterTurn,
        };
        self.request_action(
            "draft-create",
            Action::CreateMessageDraft {
                conversation: conversation.into(),
                thread: thread.into(),
                body: body.into(),
                pinned,
                timing,
                position,
            },
        )
    }

    pub fn request_revise_draft(
        &mut self,
        draft: &str,
        body: &str,
        pinned: bool,
        timing: DomainDraftTiming,
        position: u32,
    ) -> Result<String, String> {
        let timing = match timing {
            DomainDraftTiming::Now => nucleus::MessageDraftTiming::Now,
            DomainDraftTiming::NextSafePoint => nucleus::MessageDraftTiming::NextSafePoint,
            DomainDraftTiming::AfterTurn => nucleus::MessageDraftTiming::AfterTurn,
        };
        self.request_action(
            "draft-revise",
            Action::ReviseMessageDraft {
                draft: draft.into(),
                body: body.into(),
                pinned,
                timing,
                position,
            },
        )
    }

    pub fn request_delete_draft(&mut self, draft: &str) -> Result<String, String> {
        self.request_action(
            "draft-delete",
            Action::DeleteMessageDraft {
                draft: draft.into(),
            },
        )
    }

    pub fn request_send_draft(&mut self, draft: &str) -> Result<String, String> {
        self.request_action(
            "draft-send",
            Action::SendMessageDraft {
                draft: draft.into(),
            },
        )
    }

    pub fn request_create_record(
        &mut self,
        title: &str,
        description: &str,
        quantity: f64,
    ) -> Result<String, String> {
        let title = title.trim();
        if title.is_empty() || !quantity.is_finite() {
            return Err("a non-empty title and finite quantity are required".into());
        }
        self.request_action(
            "record-create",
            Action::CreateRecord {
                slug: None,
                kind: nucleus::RecordKind::Plain,
                head: title.into(),
                body: description.into(),
                quantity,
            },
        )
    }

    pub fn request_set_record_quantity(
        &mut self,
        record: &str,
        value: f64,
    ) -> Result<String, String> {
        if record.trim().is_empty() || !value.is_finite() {
            return Err("a Record uid and finite quantity are required".into());
        }
        self.request_action(
            "record-quantity",
            Action::SetQuantity {
                target: record.into(),
                value,
            },
        )
    }

    fn request_action(&mut self, prefix: &str, action: Action) -> Result<String, String> {
        if self.connection != DomainConnectionState::Live {
            return Err("the local Lince Action connection is not live".into());
        }
        let id = format!("native-{prefix}-{}", self.next_action_id);
        self.next_action_id = self.next_action_id.saturating_add(1);
        let command = ClientMessage::Act {
            id: id.clone(),
            action: action.clone(),
        };
        let commands = self
            .commands
            .as_ref()
            .ok_or_else(|| "the local Lince Action channel is unavailable".to_string())?;
        commands.try_send(command).map_err(|error| {
            format!("the local Lince Action queue refused the request: {error}")
        })?;
        self.pending_actions.insert(id.clone(), action);
        self.action_requests = self.action_requests.saturating_add(1);
        Ok(id)
    }

    pub fn take_action_receipts(&mut self) -> Vec<DomainActionReceipt> {
        std::mem::take(&mut self.action_receipts)
    }

    pub fn status_line(&self) -> String {
        match self.last_error.as_deref() {
            Some(error) => format!("{} · {error}", self.connection.label()),
            None if self.connection == DomainConnectionState::Live => format!(
                "{} · {} Record{} · {} Conversation{} · {} private draft{}",
                self.connection.label(),
                self.records.len(),
                if self.records.len() == 1 { "" } else { "s" },
                self.conversations.len(),
                if self.conversations.len() == 1 {
                    ""
                } else {
                    "s"
                },
                self.message_drafts.len(),
                if self.message_drafts.len() == 1 {
                    ""
                } else {
                    "s"
                },
            ),
            None => self.connection.label().into(),
        }
    }

    pub fn facts(&self) -> NativeDomainFacts {
        NativeDomainFacts {
            connection: self.connection,
            endpoint: self.endpoint.clone(),
            records: self.records.len(),
            conversations: self.conversations.len(),
            message_drafts: self.message_drafts.len(),
            snapshots: self.snapshots,
            updates: self.updates,
            action_requests: self.action_requests,
            action_acknowledgements: self.action_acknowledgements,
            action_failures: self.action_failures,
            pending_actions: self.pending_actions.len(),
            refusals: self.refusals,
            last_error: self.last_error.clone(),
        }
    }

    fn not_configured() -> Self {
        Self {
            connection: DomainConnectionState::NotConfigured,
            endpoint: None,
            records: Vec::new(),
            conversations: Vec::new(),
            message_drafts: Vec::new(),
            snapshots: 0,
            updates: 0,
            action_requests: 0,
            action_acknowledgements: 0,
            action_failures: 0,
            next_action_id: 1,
            pending_actions: BTreeMap::new(),
            action_receipts: Vec::new(),
            refusals: 0,
            last_error: None,
            events: None,
            commands: None,
        }
    }

    fn unavailable(endpoint: Option<String>, error: String) -> Self {
        Self {
            connection: DomainConnectionState::Unavailable,
            endpoint,
            records: Vec::new(),
            conversations: Vec::new(),
            message_drafts: Vec::new(),
            snapshots: 0,
            updates: 0,
            action_requests: 0,
            action_acknowledgements: 0,
            action_failures: 0,
            next_action_id: 1,
            pending_actions: BTreeMap::new(),
            action_receipts: Vec::new(),
            refusals: 1,
            last_error: Some(error),
            events: None,
            commands: None,
        }
    }

    fn apply(&mut self, event: DomainEvent) -> bool {
        match event {
            DomainEvent::Connecting => {
                self.connection = DomainConnectionState::Connecting;
                true
            }
            DomainEvent::Connected => {
                self.connection = DomainConnectionState::Live;
                self.last_error = None;
                true
            }
            DomainEvent::Message(ServerMessage::Snapshot { id, rows })
                if id == OFFICIAL_RECORDS_SUBSCRIPTION =>
            {
                self.connection = DomainConnectionState::Live;
                self.last_error = None;
                self.replace_records(rows);
                self.snapshots = self.snapshots.saturating_add(1);
                true
            }
            DomainEvent::Message(ServerMessage::Snapshot { id, rows })
                if id == OFFICIAL_CONVERSATIONS_SUBSCRIPTION =>
            {
                self.connection = DomainConnectionState::Live;
                self.last_error = None;
                self.replace_conversations(rows);
                self.snapshots = self.snapshots.saturating_add(1);
                true
            }
            DomainEvent::Message(ServerMessage::Snapshot { id, rows })
                if id == OFFICIAL_MESSAGE_DRAFTS_SUBSCRIPTION =>
            {
                self.connection = DomainConnectionState::Live;
                self.last_error = None;
                self.replace_message_drafts(rows);
                self.snapshots = self.snapshots.saturating_add(1);
                true
            }
            DomainEvent::Message(ServerMessage::Update { id, rows })
                if id == OFFICIAL_RECORDS_SUBSCRIPTION =>
            {
                self.connection = DomainConnectionState::Live;
                self.last_error = None;
                self.replace_records(rows);
                self.updates = self.updates.saturating_add(1);
                true
            }
            DomainEvent::Message(ServerMessage::Update { id, rows })
                if id == OFFICIAL_CONVERSATIONS_SUBSCRIPTION =>
            {
                self.connection = DomainConnectionState::Live;
                self.last_error = None;
                self.replace_conversations(rows);
                self.updates = self.updates.saturating_add(1);
                true
            }
            DomainEvent::Message(ServerMessage::Update { id, rows })
                if id == OFFICIAL_MESSAGE_DRAFTS_SUBSCRIPTION =>
            {
                self.connection = DomainConnectionState::Live;
                self.last_error = None;
                self.replace_message_drafts(rows);
                self.updates = self.updates.saturating_add(1);
                true
            }
            DomainEvent::Message(ServerMessage::ActionOk { id, created, .. })
                if self.pending_actions.remove(&id).is_some() =>
            {
                self.action_acknowledgements = self.action_acknowledgements.saturating_add(1);
                self.action_receipts.push(DomainActionReceipt {
                    id,
                    succeeded: true,
                    created,
                    error: None,
                });
                true
            }
            DomainEvent::Message(ServerMessage::Error { id, message, .. })
                if id == OFFICIAL_RECORDS_SUBSCRIPTION || id == "-" =>
            {
                self.connection = DomainConnectionState::Unavailable;
                self.refusals = self.refusals.saturating_add(1);
                self.last_error = Some(message);
                true
            }
            DomainEvent::Message(ServerMessage::Error { id, message, .. })
                if id == OFFICIAL_CONVERSATIONS_SUBSCRIPTION
                    || id == OFFICIAL_MESSAGE_DRAFTS_SUBSCRIPTION =>
            {
                self.connection = DomainConnectionState::Unavailable;
                self.refusals = self.refusals.saturating_add(1);
                self.last_error = Some(message);
                true
            }
            DomainEvent::Message(ServerMessage::Error { id, message, .. })
                if self.pending_actions.remove(&id).is_some() =>
            {
                self.action_failures = self.action_failures.saturating_add(1);
                self.action_receipts.push(DomainActionReceipt {
                    id,
                    succeeded: false,
                    created: None,
                    error: Some(message),
                });
                true
            }
            DomainEvent::Message(_) => false,
            DomainEvent::Unavailable(error) => {
                self.connection = DomainConnectionState::Unavailable;
                self.refusals = self.refusals.saturating_add(1);
                self.last_error = Some(error.clone());
                let pending = std::mem::take(&mut self.pending_actions);
                self.action_failures = self.action_failures.saturating_add(pending.len() as u64);
                self.action_receipts
                    .extend(pending.into_keys().map(|id| DomainActionReceipt {
                        id,
                        succeeded: false,
                        created: None,
                        error: Some(error.clone()),
                    }));
                true
            }
        }
    }

    fn replace_records(&mut self, rows: Vec<Value>) {
        let mut refused = 0_u64;
        self.records = rows
            .into_iter()
            .filter_map(|row| match DomainRecord::try_from(row) {
                Ok(record) => Some(record),
                Err(_) => {
                    refused = refused.saturating_add(1);
                    None
                }
            })
            .collect();
        self.refusals = self.refusals.saturating_add(refused);
    }

    fn replace_conversations(&mut self, rows: Vec<Value>) {
        let mut refused = 0_u64;
        self.conversations = rows
            .into_iter()
            .filter_map(|row| match DomainConversation::try_from(row) {
                Ok(conversation) => Some(conversation),
                Err(_) => {
                    refused = refused.saturating_add(1);
                    None
                }
            })
            .collect();
        self.refusals = self.refusals.saturating_add(refused);
    }

    fn replace_message_drafts(&mut self, rows: Vec<Value>) {
        let mut refused = 0_u64;
        self.message_drafts = rows
            .into_iter()
            .filter_map(|row| match DomainMessageDraft::try_from(row) {
                Ok(draft) => Some(draft),
                Err(_) => {
                    refused = refused.saturating_add(1);
                    None
                }
            })
            .collect();
        self.message_drafts
            .sort_by_key(|draft| (draft.position, draft.created_at.clone(), draft.uid.clone()));
        self.refusals = self.refusals.saturating_add(refused);
    }
}

impl TryFrom<Value> for DomainRecord {
    type Error = String;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        let uid = required_text(&value, "uid")?;
        let title = optional_text(&value, "title")
            .or_else(|| optional_text(&value, "head"))
            .unwrap_or_else(|| "Untitled".into());
        let description = optional_text(&value, "description")
            .or_else(|| optional_text(&value, "body"))
            .unwrap_or_default();
        let quantity = value
            .get("quantity")
            .and_then(Value::as_f64)
            .or_else(|| {
                value
                    .get("quantity")
                    .and_then(Value::as_str)
                    .and_then(|quantity| quantity.parse().ok())
            })
            .unwrap_or_default();
        Ok(Self {
            uid,
            title,
            description,
            quantity,
            kind: optional_text(&value, "kind").unwrap_or_else(|| "record".into()),
            slug: optional_text(&value, "slug"),
        })
    }
}

impl TryFrom<Value> for DomainConversation {
    type Error = String;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        let uid = required_text(&value, "uid")?;
        let title = optional_text(&value, "title")
            .or_else(|| optional_text(&value, "head"))
            .unwrap_or_else(|| "Untitled conversation".into());
        let threads = value
            .get("threads")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .cloned()
            .map(DomainThread::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            uid,
            title,
            threads,
        })
    }
}

impl TryFrom<Value> for DomainThread {
    type Error = String;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        let uid = required_text(&value, "uid")?;
        let title = optional_text(&value, "title")
            .or_else(|| optional_text(&value, "head"))
            .unwrap_or_else(|| "Untitled topic".into());
        let messages = value
            .get("messages")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .cloned()
            .map(DomainMessage::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            uid,
            title,
            messages,
        })
    }
}

impl TryFrom<Value> for DomainMessage {
    type Error = String;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        let uid = required_text(&value, "uid")?;
        let author = optional_text(&value, "author")
            .or_else(|| optional_text(&value, "sender"))
            .or_else(|| optional_text(&value, "created_by"))
            .or_else(|| optional_text(&value, "organ_uid"))
            .unwrap_or_else(|| "unknown author".into());
        let operator = optional_text(&value, "operator").unwrap_or_else(|| author.clone());
        let content = optional_text(&value, "content")
            .or_else(|| optional_text(&value, "body"))
            .or_else(|| optional_text(&value, "head"))
            .unwrap_or_default();
        let state = match optional_text(&value, "message_state")
            .or_else(|| optional_text(&value, "state"))
            .as_deref()
        {
            Some("writing") => DomainMessageState::Writing,
            Some("interrupted") => DomainMessageState::Interrupted,
            Some("finished") | None => DomainMessageState::Finished,
            Some(state) => return Err(format!("unknown Message state {state}")),
        };
        Ok(Self {
            uid,
            author,
            operator,
            content,
            state,
            created_at: optional_text(&value, "created_at"),
        })
    }
}

impl TryFrom<Value> for DomainMessageDraft {
    type Error = String;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        let uid = required_text(&value, "uid")?;
        let extension = value
            .get("extension")
            .and_then(Value::as_object)
            .ok_or_else(|| "message draft Protein row lacks its extension".to_string())?;
        let timing = match extension.get("timing").and_then(Value::as_str) {
            Some("now") | None => DomainDraftTiming::Now,
            Some("next_safe_point") => DomainDraftTiming::NextSafePoint,
            Some("after_turn") => DomainDraftTiming::AfterTurn,
            Some(timing) => return Err(format!("unknown message draft timing {timing}")),
        };
        let position = extension
            .get("position")
            .and_then(Value::as_u64)
            .unwrap_or_default()
            .try_into()
            .map_err(|_| "message draft position exceeds u32".to_string())?;
        Ok(Self {
            uid,
            conversation: required_object_text(extension, "conversation")?,
            thread: required_object_text(extension, "thread")?,
            author: required_object_text(extension, "author")?,
            operator: required_object_text(extension, "operator")?,
            content: optional_text(&value, "body").unwrap_or_default(),
            pinned: extension
                .get("pinned")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            timing,
            position,
            created_at: optional_text(&value, "created_at").unwrap_or_default(),
        })
    }
}

fn required_text(value: &Value, field: &str) -> Result<String, String> {
    optional_text(value, field).ok_or_else(|| format!("Protein row lacks string {field}"))
}

fn optional_text(value: &Value, field: &str) -> Option<String> {
    value.get(field).and_then(Value::as_str).map(str::to_owned)
}

fn required_object_text(
    value: &serde_json::Map<String, Value>,
    field: &str,
) -> Result<String, String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("message draft extension lacks string {field}"))
}

fn transport_endpoint(base_url: &str) -> Result<String, String> {
    let mut endpoint = Url::parse(base_url).map_err(|error| error.to_string())?;
    let scheme = match endpoint.scheme() {
        "http" => "ws",
        "https" => "wss",
        "ws" => "ws",
        "wss" => "wss",
        scheme => return Err(format!("unsupported local Lince scheme {scheme}")),
    };
    endpoint
        .set_scheme(scheme)
        .map_err(|_| "local Lince transport scheme cannot be changed".to_string())?;
    endpoint.set_path("/host/transport/ws");
    endpoint.set_query(None);
    endpoint.set_fragment(None);
    Ok(endpoint.into())
}

fn records_protein() -> Protein {
    Protein {
        source: Source::Record,
        filter: Vec::new(),
        fields: Some(vec![
            "uid".into(),
            "head".into(),
            "body".into(),
            "quantity".into(),
            "kind".into(),
            "slug".into(),
        ]),
        include: Include::default(),
        aggregate: None,
        order: vec![Order::Asc("head".into())],
        limit: Some(64),
    }
}

fn conversations_protein() -> Protein {
    Protein {
        source: Source::Record,
        filter: vec![Predicate::KindEq("conversation".into())],
        fields: None,
        include: Include {
            threads: Some(ThreadsInclude { messages_limit: 64 }),
            ..Include::default()
        },
        aggregate: None,
        order: vec![Order::Desc("created_at".into())],
        limit: Some(32),
    }
}

fn message_drafts_protein() -> Protein {
    Protein {
        source: Source::Record,
        filter: vec![Predicate::KindEq("message_draft".into())],
        fields: None,
        include: Include {
            extension: Some(ExtensionInclude {
                namespace: "lince.message-draft".into(),
            }),
            ..Include::default()
        },
        aggregate: None,
        order: vec![Order::Asc("created_at".into())],
        limit: Some(128),
    }
}

async fn run_subscription(
    endpoint: String,
    events: mpsc::Sender<DomainEvent>,
    mut commands: mpsc::Receiver<ClientMessage>,
) {
    loop {
        if events.is_closed() {
            return;
        }
        let _ = events.send(DomainEvent::Connecting).await;
        match connect_async(endpoint.as_str()).await {
            Ok((mut stream, _)) => {
                let subscriptions = [
                    ClientMessage::Subscribe {
                        id: OFFICIAL_RECORDS_SUBSCRIPTION.into(),
                        protein: records_protein(),
                    },
                    ClientMessage::Subscribe {
                        id: OFFICIAL_CONVERSATIONS_SUBSCRIPTION.into(),
                        protein: conversations_protein(),
                    },
                    ClientMessage::Subscribe {
                        id: OFFICIAL_MESSAGE_DRAFTS_SUBSCRIPTION.into(),
                        protein: message_drafts_protein(),
                    },
                ];
                let mut subscribed = true;
                for request in subscriptions {
                    let serialized = match serde_json::to_string(&request) {
                        Ok(serialized) => serialized,
                        Err(error) => {
                            let _ = events
                                .send(DomainEvent::Unavailable(error.to_string()))
                                .await;
                            return;
                        }
                    };
                    if stream.send(Message::Text(serialized.into())).await.is_err() {
                        subscribed = false;
                        break;
                    }
                }
                if subscribed {
                    let _ = events.send(DomainEvent::Connected).await;
                    loop {
                        tokio::select! {
                            incoming = stream.next() => {
                                let Some(message) = incoming else {
                                    break;
                                };
                                match message {
                                    Ok(Message::Text(text)) => {
                                        match serde_json::from_str::<ServerMessage>(text.as_str()) {
                                            Ok(message) => {
                                                if events.send(DomainEvent::Message(message)).await.is_err() {
                                                    return;
                                                }
                                            }
                                            Err(error) => {
                                                let _ = events
                                                    .send(DomainEvent::Unavailable(error.to_string()))
                                                    .await;
                                            }
                                        }
                                    }
                                    Ok(Message::Ping(bytes)) => {
                                        if stream.send(Message::Pong(bytes)).await.is_err() {
                                            break;
                                        }
                                    }
                                    Ok(Message::Close(_)) | Err(_) => break,
                                    _ => {}
                                }
                            }
                            command = commands.recv() => {
                                let Some(command) = command else {
                                    return;
                                };
                                let action_id = client_message_id(&command);
                                let serialized = match serde_json::to_string(&command) {
                                    Ok(serialized) => serialized,
                                    Err(error) => {
                                        if let Some(id) = action_id {
                                            let _ = events.send(DomainEvent::Message(ServerMessage::Error {
                                                id,
                                                message: error.to_string(),
                                                code: None,
                                            })).await;
                                        }
                                        continue;
                                    }
                                };
                                if stream.send(Message::Text(serialized.into())).await.is_err() {
                                    if let Some(id) = action_id {
                                        let _ = events.send(DomainEvent::Message(ServerMessage::Error {
                                            id,
                                            message: "local Lince transport disconnected before sending the Action".into(),
                                            code: None,
                                        })).await;
                                    }
                                    break;
                                }
                            }
                        }
                    }
                }
                let _ = events
                    .send(DomainEvent::Unavailable(
                        "local Lince transport disconnected".into(),
                    ))
                    .await;
            }
            Err(error) => {
                let _ = events
                    .send(DomainEvent::Unavailable(error.to_string()))
                    .await;
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
}

fn client_message_id(message: &ClientMessage) -> Option<String> {
    match message {
        ClientMessage::Act { id, .. } | ClientMessage::SignedAct { id, .. } => Some(id.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_http_urls_become_exact_websocket_transport_urls() {
        assert_eq!(
            transport_endpoint("http://127.0.0.1:6174").unwrap(),
            "ws://127.0.0.1:6174/host/transport/ws"
        );
        assert_eq!(
            transport_endpoint("https://lince.example/board?old=true").unwrap(),
            "wss://lince.example/host/transport/ws"
        );
        assert!(transport_endpoint("file:///tmp/lince").is_err());
    }

    #[test]
    fn record_rows_accept_current_and_interface_field_names() {
        let current = DomainRecord::try_from(serde_json::json!({
            "uid": "r_current",
            "head": "Current title",
            "body": "Current description",
            "quantity": "-2.5",
            "kind": "plain",
            "slug": "current"
        }))
        .unwrap();
        assert_eq!(current.title, "Current title");
        assert_eq!(current.description, "Current description");
        assert_eq!(current.quantity, -2.5);
        let interface = DomainRecord::try_from(serde_json::json!({
            "uid": "r_interface",
            "title": "Interface title",
            "description": "Interface description",
            "quantity": 3
        }))
        .unwrap();
        assert_eq!(interface.title, "Interface title");
        assert_eq!(interface.description, "Interface description");
        assert_eq!(interface.quantity, 3.0);
    }

    #[test]
    fn subscription_snapshots_replace_rows_and_keep_failures_visible() {
        let mut client = NativeDomainClient::not_configured();
        assert!(client.apply(DomainEvent::Connected));
        assert!(client.apply(DomainEvent::Message(ServerMessage::Snapshot {
            id: OFFICIAL_RECORDS_SUBSCRIPTION.into(),
            rows: vec![
                serde_json::json!({ "uid": "r_one", "head": "One" }),
                serde_json::json!({ "head": "Invalid" }),
            ],
        })));
        assert_eq!(client.records().len(), 1);
        assert_eq!(client.facts().snapshots, 1);
        assert_eq!(client.facts().refusals, 1);
        assert!(client.status_line().contains("1 Record"));
    }

    #[test]
    fn conversation_rows_preserve_authorship_operator_and_live_state() {
        let conversation = DomainConversation::try_from(serde_json::json!({
            "uid": "r_conversation",
            "head": "Interface",
            "threads": [{
                "uid": "r_thread",
                "head": "C4",
                "messages": [{
                    "uid": "r_message",
                    "body": "Still arriving",
                    "sender": "Human author",
                    "operator": "Delegated Fiote",
                    "message_state": "writing",
                    "created_at": "2026-09-05T12:00:00Z"
                }]
            }]
        }))
        .unwrap();
        let message = &conversation.threads[0].messages[0];
        assert_eq!(message.author, "Human author");
        assert_eq!(message.operator, "Delegated Fiote");
        assert_eq!(message.state, DomainMessageState::Writing);
    }

    #[test]
    fn message_draft_rows_require_private_routing_metadata() {
        let draft = DomainMessageDraft::try_from(serde_json::json!({
            "uid": "r_draft",
            "body": "Restart-safe words",
            "created_at": "2026-09-05T12:00:00Z",
            "extension": {
                "conversation": "r_conversation",
                "thread": "r_thread",
                "author": "r_author",
                "operator": "r_operator",
                "pinned": true,
                "timing": "after_turn",
                "position": 7
            }
        }))
        .unwrap();
        assert_eq!(draft.content, "Restart-safe words");
        assert!(draft.pinned);
        assert_eq!(draft.timing, DomainDraftTiming::AfterTurn);
        assert_eq!(draft.position, 7);
        assert!(
            DomainMessageDraft::try_from(serde_json::json!({
                "uid": "r_invalid",
                "body": "No routing metadata"
            }))
            .is_err()
        );
    }

    #[tokio::test]
    async fn message_actions_are_bounded_and_acknowledged_before_consumption() {
        let (commands_tx, mut commands_rx) = mpsc::channel(1);
        let mut client = NativeDomainClient::not_configured();
        client.connection = DomainConnectionState::Live;
        client.commands = Some(commands_tx);
        let id = client
            .request_message("r_thread", "Keep this draft")
            .unwrap();
        let command = commands_rx.try_recv().unwrap();
        let ClientMessage::Act {
            id: sent_id,
            action:
                Action::CreateMessage {
                    thread,
                    body,
                    author: _,
                    state: _,
                    parent,
                    references,
                },
        } = command
        else {
            panic!("expected a CreateMessage Action");
        };
        assert_eq!(sent_id, id);
        assert_eq!(thread, "r_thread");
        assert_eq!(body, "Keep this draft");
        assert_eq!(parent, None);
        assert!(references.is_empty());
        assert_eq!(client.facts().pending_actions, 1);
        assert!(client.apply(DomainEvent::Message(ServerMessage::ActionOk {
            id: id.clone(),
            created: Some("r_message".into()),
            facts: 0,
            warnings: Vec::new(),
            data: None,
        })));
        assert_eq!(client.facts().pending_actions, 0);
        assert_eq!(client.facts().action_acknowledgements, 1);
        assert_eq!(
            client.take_action_receipts(),
            vec![DomainActionReceipt {
                id,
                succeeded: true,
                created: Some("r_message".into()),
                error: None,
            }]
        );
    }

    #[tokio::test]
    async fn record_collection_actions_use_existing_typed_engine_actions() {
        let (commands_tx, mut commands_rx) = mpsc::channel(2);
        let mut client = NativeDomainClient::not_configured();
        client.connection = DomainConnectionState::Live;
        client.commands = Some(commands_tx);
        client
            .request_create_record("New task", "Description", -1.0)
            .unwrap();
        client.request_set_record_quantity("r_task", 0.0).unwrap();
        assert!(matches!(
            commands_rx.try_recv().unwrap(),
            ClientMessage::Act {
                action: Action::CreateRecord {
                    kind: nucleus::RecordKind::Plain,
                    ref head,
                    quantity: -1.0,
                    ..
                },
                ..
            } if head == "New task"
        ));
        assert!(matches!(
            commands_rx.try_recv().unwrap(),
            ClientMessage::Act {
                action: Action::SetQuantity {
                    ref target,
                    value: 0.0,
                },
                ..
            } if target == "r_task"
        ));
        assert_eq!(client.facts().action_requests, 2);
        assert_eq!(client.facts().pending_actions, 2);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn live_socket_materializes_conversation_and_acknowledges_action() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_async(socket).await.unwrap();
            for _ in 0..3 {
                let Message::Text(text) = socket.next().await.unwrap().unwrap() else {
                    panic!("expected subscription text");
                };
                let ClientMessage::Subscribe { id, .. } =
                    serde_json::from_str::<ClientMessage>(text.as_str()).unwrap()
                else {
                    panic!("expected subscription");
                };
                let rows = match id.as_str() {
                    OFFICIAL_RECORDS_SUBSCRIPTION => {
                        vec![serde_json::json!({"uid": "r_record", "head": "Record"})]
                    }
                    OFFICIAL_CONVERSATIONS_SUBSCRIPTION => vec![serde_json::json!({
                        "uid": "r_conversation",
                        "head": "Conversation",
                        "threads": [{
                            "uid": "r_thread",
                            "head": "Topic",
                            "messages": []
                        }]
                    })],
                    OFFICIAL_MESSAGE_DRAFTS_SUBSCRIPTION => vec![serde_json::json!({
                        "uid": "r_draft",
                        "body": "Durable draft",
                        "created_at": "2026-09-05T12:00:00Z",
                        "extension": {
                            "conversation": "r_conversation",
                            "thread": "r_thread",
                            "author": "r_author",
                            "operator": "r_author",
                            "pinned": false,
                            "timing": "now",
                            "position": 0
                        }
                    })],
                    subscription => panic!("unexpected subscription {subscription}"),
                };
                socket
                    .send(Message::Text(
                        serde_json::to_string(&ServerMessage::Snapshot { id, rows })
                            .unwrap()
                            .into(),
                    ))
                    .await
                    .unwrap();
            }
            let Message::Text(text) = socket.next().await.unwrap().unwrap() else {
                panic!("expected Action text");
            };
            let ClientMessage::Act { id, action } =
                serde_json::from_str::<ClientMessage>(text.as_str()).unwrap()
            else {
                panic!("expected Action");
            };
            assert!(matches!(
                action,
                Action::CreateMessage { ref thread, ref body, .. }
                    if thread == "r_thread" && body == "Hello"
            ));
            socket
                .send(Message::Text(
                    serde_json::to_string(&ServerMessage::ActionOk {
                        id,
                        created: Some("r_created".into()),
                        facts: 0,
                        warnings: Vec::new(),
                        data: None,
                    })
                    .unwrap()
                    .into(),
                ))
                .await
                .unwrap();
        });
        let mut client = NativeDomainClient::connect(Some(format!("http://{address}")));
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                client.poll();
                if client.facts().snapshots >= 3 {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(client.records().len(), 1);
        assert_eq!(client.conversations().len(), 1);
        assert_eq!(client.message_drafts().len(), 1);
        let id = client.request_message("r_thread", "Hello").unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                client.poll();
                if client.facts().action_acknowledgements == 1 {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(client.take_action_receipts()[0].id, id);
        server.await.unwrap();
    }
}
