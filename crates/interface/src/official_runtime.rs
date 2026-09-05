use crate::{
    domain_model::{
        DomainActionReceipt, DomainConversation, DomainDraftTiming, DomainMessageDraft,
        DomainRecord,
    },
    official_sands::official_sand_package,
    retained_ui::{RetainedPlacement, RetainedRect, RetainedScene},
    sand::{SandElement, SandValue},
};
use serde::Serialize;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

const DRAFT_PERSIST_DEBOUNCE: Duration = Duration::from_millis(250);

pub const SHARED_OFFICIAL_RUNTIME_COUNT: usize = 7;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SharedOfficialRoot {
    ShellEdit,
    ShellZoom,
    Record,
    Conversation,
    Table,
    Todo,
    Kanban,
}

impl SharedOfficialRoot {
    pub const ALL: [Self; SHARED_OFFICIAL_RUNTIME_COUNT] = [
        Self::ShellEdit,
        Self::ShellZoom,
        Self::Record,
        Self::Conversation,
        Self::Table,
        Self::Todo,
        Self::Kanban,
    ];

    pub fn uid(self) -> &'static str {
        match self {
            Self::ShellEdit => "shell-edit",
            Self::ShellZoom => "shell-zoom",
            Self::Record => "record",
            Self::Conversation => "conversation",
            Self::Table => "table",
            Self::Todo => "todo",
            Self::Kanban => "kanban",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::ShellEdit => "Edit controls",
            Self::ShellZoom => "Zoom controls",
            Self::Record => "Record",
            Self::Conversation => "Conversation",
            Self::Table => "Table",
            Self::Todo => "Todo",
            Self::Kanban => "Kanban",
        }
    }

    pub fn from_uid(uid: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|root| root.uid() == uid)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DraftSendTiming {
    Now,
    NextSafePoint,
    AfterTurn,
}

impl DraftSendTiming {
    fn next(self) -> Self {
        match self {
            Self::Now => Self::NextSafePoint,
            Self::NextSafePoint => Self::AfterTurn,
            Self::AfterTurn => Self::Now,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Now => "send now",
            Self::NextSafePoint => "next safe point",
            Self::AfterTurn => "after this turn",
        }
    }
}

impl From<DraftSendTiming> for DomainDraftTiming {
    fn from(value: DraftSendTiming) -> Self {
        match value {
            DraftSendTiming::Now => Self::Now,
            DraftSendTiming::NextSafePoint => Self::NextSafePoint,
            DraftSendTiming::AfterTurn => Self::AfterTurn,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MessageDraftSource {
    Record,
    Conversation(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum OfficialRuntimeIntent {
    SendMessage {
        thread: String,
        body: String,
        source: MessageDraftSource,
    },
    CreateDraft {
        local_uid: String,
        conversation: String,
        thread: String,
        body: String,
        pinned: bool,
        timing: DraftSendTiming,
        position: u32,
        send_after_create: bool,
    },
    ReviseDraft {
        draft: String,
        body: String,
        pinned: bool,
        timing: DraftSendTiming,
        position: u32,
        revision: u64,
        send_after_revise: bool,
    },
    DeleteDraft {
        draft: String,
    },
    SendDraft {
        draft: String,
        pinned: bool,
    },
    CreateRecord {
        title: String,
        description: String,
        quantity: f64,
        source: SharedOfficialRoot,
    },
    SetRecordQuantity {
        record: String,
        value: f64,
        source: SharedOfficialRoot,
    },
}

#[derive(Clone, Debug, PartialEq)]
enum PendingDomainAction {
    SendMessage(MessageDraftSource),
    CreateDraft {
        local_uid: String,
        revision: u64,
        send_after_create: bool,
    },
    ReviseDraft {
        draft: String,
        revision: u64,
        send_after_revise: bool,
    },
    DeleteDraft {
        draft: String,
    },
    SendDraft {
        draft: String,
        pinned: bool,
    },
    CreateRecord {
        source: SharedOfficialRoot,
    },
    SetRecordQuantity {
        record: String,
        value: f64,
        source: SharedOfficialRoot,
    },
}

#[derive(Clone, Debug)]
struct ConversationDraft {
    uid: String,
    conversation: Option<String>,
    thread: Option<String>,
    content: String,
    pinned: bool,
    timing: DraftSendTiming,
    created_at: Instant,
    persisted_created_at: Option<String>,
    persisted: bool,
    dirty: bool,
    revision: u64,
    changed_at: Instant,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PropertyPresentation {
    Filled,
    All,
    Hidden,
}

impl PropertyPresentation {
    fn next(self) -> Self {
        match self {
            Self::Filled => Self::All,
            Self::All => Self::Hidden,
            Self::Hidden => Self::Filled,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Filled => "filled properties",
            Self::All => "all properties",
            Self::Hidden => "properties hidden",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SharedOfficialRuntimeFacts {
    pub active_root: Option<SharedOfficialRoot>,
    pub focused_control: usize,
    pub retained_nodes: usize,
    pub interactive_nodes: usize,
    pub edit_mode: bool,
    pub group_locked: bool,
    pub castle_save_requests: u64,
    pub zoom_percent: u16,
    pub recenter_actions: u64,
    pub emitted_record_events: u64,
    pub message_send_requests: u64,
    pub message_send_acknowledgements: u64,
    pub message_send_failures: u64,
    pub property_presentation: PropertyPresentation,
    pub domain_records: usize,
    pub domain_conversations: usize,
    pub conversation_drafts: usize,
    pub persisted_conversation_drafts: usize,
    pub dirty_conversation_drafts: usize,
    pub pinned_conversation_drafts: usize,
    pub pending_message_action: bool,
    pub workflow_action_requests: u64,
    pub workflow_action_acknowledgements: u64,
    pub workflow_action_failures: u64,
    pub selected_workflow_record: Option<String>,
    pub workflow_page: usize,
    pub domain_status: String,
}

#[derive(Clone, Debug)]
pub struct SharedOfficialRuntimeState {
    active: Option<SharedOfficialRoot>,
    focused: usize,
    edit_mode: bool,
    group_locked: bool,
    castle_save_requests: u64,
    zoom_percent: u16,
    recenter_actions: u64,
    emitted_record_events: u64,
    message_send_requests: u64,
    message_send_acknowledgements: u64,
    message_send_failures: u64,
    workflow_action_requests: u64,
    workflow_action_acknowledgements: u64,
    workflow_action_failures: u64,
    property_presentation: PropertyPresentation,
    record_uid: String,
    record_title: String,
    record_description: String,
    record_quantity: f64,
    record_state: String,
    draft: String,
    conversation_drafts: Vec<ConversationDraft>,
    selected_conversation_draft: usize,
    next_draft_id: u64,
    pending_domain_action: Option<(String, PendingDomainAction)>,
    outbound_intents: Vec<OfficialRuntimeIntent>,
    last_result: String,
    domain_records: Vec<DomainRecord>,
    domain_conversations: Vec<DomainConversation>,
    selected_conversation: Option<String>,
    selected_thread: Option<String>,
    selected_workflow_record: Option<String>,
    workflow_page: usize,
    new_record_title: String,
    turn_in_flight: bool,
    domain_status: String,
}

impl SharedOfficialRuntimeState {
    pub fn new() -> Self {
        Self {
            active: None,
            focused: 0,
            edit_mode: false,
            group_locked: false,
            castle_save_requests: 0,
            zoom_percent: 100,
            recenter_actions: 0,
            emitted_record_events: 0,
            message_send_requests: 0,
            message_send_acknowledgements: 0,
            message_send_failures: 0,
            workflow_action_requests: 0,
            workflow_action_acknowledgements: 0,
            workflow_action_failures: 0,
            property_presentation: PropertyPresentation::Filled,
            record_uid: "record-interface-c4".into(),
            record_title: "Compose the native Interface".into(),
            record_description:
                "Protein fields are projected through the exact recursive Record definition.".into(),
            record_quantity: -1.0,
            record_state: "Need".into(),
            draft: String::new(),
            conversation_drafts: vec![ConversationDraft {
                uid: "native-draft-1".into(),
                conversation: None,
                thread: None,
                content: String::new(),
                pinned: false,
                timing: DraftSendTiming::Now,
                created_at: Instant::now(),
                persisted_created_at: None,
                persisted: false,
                dirty: false,
                revision: 0,
                changed_at: Instant::now(),
            }],
            selected_conversation_draft: 0,
            next_draft_id: 2,
            pending_domain_action: None,
            outbound_intents: Vec::new(),
            last_result: "Choose a shared official Sand and press Enter to operate it".into(),
            domain_records: Vec::new(),
            domain_conversations: Vec::new(),
            selected_conversation: None,
            selected_thread: None,
            selected_workflow_record: None,
            workflow_page: 0,
            new_record_title: String::new(),
            turn_in_flight: false,
            domain_status: "representative Protein-shaped data".into(),
        }
    }

    pub fn active(&self) -> Option<SharedOfficialRoot> {
        self.active
    }

    pub fn open(&mut self, uid: &str) -> bool {
        let Some(root) = SharedOfficialRoot::from_uid(uid) else {
            self.last_result = format!("{uid} is cataloged; its runtime Behavior is still pending");
            return false;
        };
        self.active = Some(root);
        self.focused = 0;
        self.last_result = format!("{} retained runtime opened", root.label());
        true
    }

    pub fn close(&mut self) {
        self.active = None;
        self.focused = 0;
        self.last_result = "Returned to the official Sand catalog".into();
    }

    pub fn focus_next(&mut self, reverse: bool, scene: &RetainedScene) {
        let count = scene.interactive_count();
        if count == 0 {
            self.focused = 0;
            return;
        }
        self.focused = if reverse {
            self.focused.checked_sub(1).unwrap_or(count - 1)
        } else {
            (self.focused + 1) % count
        };
    }

    pub fn focus_at(&mut self, index: usize, scene: &RetainedScene) {
        self.focused = index.min(scene.interactive_count().saturating_sub(1));
    }

    pub fn scene(&self, viewport: RetainedRect) -> Result<RetainedScene, String> {
        let root = self
            .active
            .ok_or_else(|| "no shared official runtime is active".to_string())?;
        if matches!(
            root,
            SharedOfficialRoot::Table | SharedOfficialRoot::Todo | SharedOfficialRoot::Kanban
        ) {
            return self.record_collection_scene(root, viewport);
        }
        let mut scene = RetainedScene::from_package(
            &official_sand_package(),
            root.uid(),
            self.inputs(root),
            viewport,
            self.focused,
        )?;
        if root == SharedOfficialRoot::Record
            && self.property_presentation == PropertyPresentation::Hidden
        {
            scene
                .nodes
                .retain(|node| !node.key.starts_with("record/properties"));
        }
        Ok(scene)
    }

    pub fn activate(&mut self, scene: &RetainedScene) -> Result<(), String> {
        let root = self
            .active
            .ok_or_else(|| "no shared official runtime is active".to_string())?;
        let focused = scene
            .focused()
            .ok_or_else(|| "the active Sand has no focused control".to_string())?;
        let key = focused.key.as_str();
        match root {
            SharedOfficialRoot::ShellEdit if key.ends_with("tools/primary/control") => {
                self.edit_mode = !self.edit_mode;
                self.last_result = if self.edit_mode {
                    "Edit mode enabled; semantic Sand structure is selectable".into()
                } else {
                    "Edit mode disabled".into()
                };
            }
            SharedOfficialRoot::ShellEdit if key.ends_with("tools/secondary/control") => {
                self.group_locked = !self.group_locked;
                self.last_result = if self.group_locked {
                    "Selected composition locked as one group".into()
                } else {
                    "Selected composition unlocked for child editing".into()
                };
            }
            SharedOfficialRoot::ShellEdit if key.ends_with("tools/tertiary/control") => {
                self.castle_save_requests = self.castle_save_requests.saturating_add(1);
                self.last_result =
                    format!("Castle save request {} emitted", self.castle_save_requests);
            }
            SharedOfficialRoot::ShellZoom if key.ends_with("zoom-out/control") => {
                self.zoom_percent = self.zoom_percent.saturating_sub(10).max(10);
                self.last_result = format!("Box zoom {}%", self.zoom_percent);
            }
            SharedOfficialRoot::ShellZoom if key.ends_with("zoom-in/control") => {
                self.zoom_percent = self.zoom_percent.saturating_add(10).min(800);
                self.last_result = format!("Box zoom {}%", self.zoom_percent);
            }
            SharedOfficialRoot::ShellZoom if key.ends_with("recenter/control") => {
                self.recenter_actions = self.recenter_actions.saturating_add(1);
                self.last_result = "Box camera recentered without changing Sand state".into();
            }
            SharedOfficialRoot::Record if key.ends_with("summary/open/control") => {
                self.emitted_record_events = self.emitted_record_events.saturating_add(1);
                self.last_result = format!("record-clicked emitted for {}", self.record_uid);
            }
            SharedOfficialRoot::Record if key.ends_with("properties/action/control") => {
                self.property_presentation = self.property_presentation.next();
                self.last_result =
                    format!("Record now shows {}", self.property_presentation.label());
            }
            SharedOfficialRoot::Record if key.ends_with("thread/send/control") => {
                self.queue_message(MessageDraftSource::Record);
            }
            SharedOfficialRoot::Record if focused.element == SandElement::Textarea => {
                self.last_result = "Record composer focused; type to edit its local draft".into();
            }
            SharedOfficialRoot::Conversation if key.ends_with("thread/send/control") => {
                let source = self
                    .selected_draft()
                    .map(|draft| MessageDraftSource::Conversation(draft.uid.clone()));
                if let Some(source) = source {
                    self.queue_message(source);
                } else {
                    self.last_result = "Nothing sent: add a private draft first".into();
                }
            }
            SharedOfficialRoot::Conversation if key.ends_with("drafts/draft/send/control") => {
                let source = self
                    .selected_draft()
                    .map(|draft| MessageDraftSource::Conversation(draft.uid.clone()));
                if let Some(source) = source {
                    self.queue_message(source);
                } else {
                    self.last_result = "Nothing sent: add a private draft first".into();
                }
            }
            SharedOfficialRoot::Conversation if key.ends_with("drafts/add/control") => {
                let uid = format!("native-draft-{}", self.next_draft_id);
                self.next_draft_id = self.next_draft_id.saturating_add(1);
                self.conversation_drafts.push(ConversationDraft {
                    uid,
                    conversation: self.selected_conversation.clone(),
                    thread: self.selected_thread.clone(),
                    content: String::new(),
                    pinned: false,
                    timing: DraftSendTiming::Now,
                    created_at: Instant::now(),
                    persisted_created_at: None,
                    persisted: false,
                    dirty: true,
                    revision: 1,
                    changed_at: Instant::now(),
                });
                self.selected_conversation_draft = self.conversation_drafts.len() - 1;
                self.last_result = format!(
                    "Private draft {} added; type before sending",
                    self.selected_conversation_draft + 1
                );
            }
            SharedOfficialRoot::Conversation if key.ends_with("drafts/draft/pinned") => {
                if let Some(draft) = self.selected_draft_mut() {
                    draft.pinned = !draft.pinned;
                    draft.mark_dirty();
                    self.last_result = if draft.pinned {
                        "Draft pinned as a reusable preset".into()
                    } else {
                        "Draft unpinned; it will be consumed after acknowledged send".into()
                    };
                }
            }
            SharedOfficialRoot::Conversation
                if key.ends_with("drafts/draft/timing-action/control") =>
            {
                if !self.turn_in_flight {
                    self.last_result = "No turn is running; this draft sends now".into();
                    return Ok(());
                }
                if let Some(draft) = self.selected_draft_mut() {
                    draft.timing = draft.timing.next();
                    draft.mark_dirty();
                    self.last_result = match draft.timing {
                        DraftSendTiming::Now => "Draft will send now".into(),
                        DraftSendTiming::NextSafePoint => {
                            "Draft waits for a running turn's next safe point".into()
                        }
                        DraftSendTiming::AfterTurn => {
                            "Draft waits until the running turn finishes".into()
                        }
                    };
                }
            }
            SharedOfficialRoot::Conversation if key.ends_with("drafts/draft/move-up/control") => {
                if self.selected_conversation_draft > 0
                    && self.selected_conversation_draft < self.conversation_drafts.len()
                {
                    self.conversation_drafts.swap(
                        self.selected_conversation_draft,
                        self.selected_conversation_draft - 1,
                    );
                    self.selected_conversation_draft -= 1;
                    let now = Instant::now();
                    for index in [
                        self.selected_conversation_draft,
                        self.selected_conversation_draft + 1,
                    ] {
                        let draft = &mut self.conversation_drafts[index];
                        draft.dirty = true;
                        draft.revision = draft.revision.saturating_add(1);
                        draft.changed_at = now;
                    }
                    self.last_result = "Draft moved one place earlier".into();
                } else {
                    self.last_result = "This draft is already first".into();
                }
            }
            SharedOfficialRoot::Conversation if key.ends_with("drafts/draft/delete/control") => {
                if self.selected_conversation_draft < self.conversation_drafts.len() {
                    let draft = self.conversation_drafts[self.selected_conversation_draft].clone();
                    if draft.persisted {
                        if self.pending_domain_action.is_some() {
                            self.last_result =
                                "Another draft Action is awaiting acknowledgement".into();
                        } else {
                            self.outbound_intents
                                .push(OfficialRuntimeIntent::DeleteDraft {
                                    draft: draft.uid.clone(),
                                });
                            self.last_result = format!(
                                "Draft {} deletion is ready for the domain Action boundary",
                                draft.uid
                            );
                        }
                    } else {
                        self.conversation_drafts
                            .remove(self.selected_conversation_draft);
                        self.selected_conversation_draft = self
                            .selected_conversation_draft
                            .min(self.conversation_drafts.len().saturating_sub(1));
                        self.last_result = format!("Private draft {} deleted", draft.uid);
                    }
                }
            }
            SharedOfficialRoot::Conversation if focused.element == SandElement::Textarea => {
                self.last_result =
                    "Private Conversation draft focused; type to edit it locally".into();
            }
            SharedOfficialRoot::Table if key.ends_with("tools/primary/control") => {
                self.change_workflow_page(false);
            }
            SharedOfficialRoot::Table if key.ends_with("tools/secondary/control") => {
                self.change_workflow_page(true);
            }
            SharedOfficialRoot::Table if key.ends_with("tools/tertiary/control") => {
                self.queue_create_record(
                    SharedOfficialRoot::Table,
                    "Untitled Record".into(),
                    String::new(),
                    0.0,
                );
            }
            SharedOfficialRoot::Table if key.ends_with("open/control") => {
                self.select_workflow_record(key);
            }
            SharedOfficialRoot::Todo if key.ends_with("new/create/control") => {
                let title = self.new_record_title.trim().to_string();
                if title.is_empty() {
                    self.last_result = "Type a task title before adding it".into();
                } else {
                    self.queue_create_record(SharedOfficialRoot::Todo, title, String::new(), -1.0);
                }
            }
            SharedOfficialRoot::Todo if key.ends_with("pager/primary/control") => {
                self.change_workflow_page(false);
            }
            SharedOfficialRoot::Todo if key.ends_with("pager/secondary/control") => {
                self.change_workflow_page(true);
            }
            SharedOfficialRoot::Todo if key.ends_with("pager/tertiary/control") => {
                self.open_selected_workflow_record();
            }
            SharedOfficialRoot::Todo if key.ends_with("complete/control") => {
                if let Some(record) = self.record_for_node(key).map(|record| record.uid.clone()) {
                    self.selected_workflow_record = Some(record.clone());
                    self.queue_set_record_quantity(SharedOfficialRoot::Todo, record, 0.0);
                }
            }
            SharedOfficialRoot::Todo if key.ends_with("open/control") => {
                self.select_workflow_record(key);
            }
            SharedOfficialRoot::Todo if focused.element == SandElement::Field => {
                self.last_result = "New task title focused; type to compose it locally".into();
            }
            SharedOfficialRoot::Kanban if key.ends_with("tools/primary/control") => {
                self.move_selected_kanban_record(-1);
            }
            SharedOfficialRoot::Kanban if key.ends_with("tools/secondary/control") => {
                self.move_selected_kanban_record(1);
            }
            SharedOfficialRoot::Kanban if key.ends_with("tools/tertiary/control") => {
                let quantity = self
                    .selected_workflow_record
                    .as_deref()
                    .and_then(|uid| {
                        self.workflow_records()
                            .into_iter()
                            .find(|row| row.uid == uid)
                    })
                    .map_or(0.0, |record| kanban_lane_for(record.quantity).quantity);
                self.queue_create_record(
                    SharedOfficialRoot::Kanban,
                    "Untitled Record".into(),
                    String::new(),
                    quantity,
                );
            }
            SharedOfficialRoot::Kanban if key.ends_with("open/control") => {
                self.select_workflow_record(key);
            }
            _ => {
                self.last_result =
                    format!("{} focused; no Behavior is assigned yet", focused.label);
            }
        }
        Ok(())
    }

    pub fn input_text(&mut self, value: &str, scene: &RetainedScene) {
        if scene
            .focused()
            .is_some_and(|node| matches!(node.element, SandElement::Textarea | SandElement::Field))
        {
            match self.active {
                Some(SharedOfficialRoot::Record) => {
                    self.draft.push_str(value);
                    self.last_result =
                        format!("Private local draft · {} characters", self.draft.len());
                }
                Some(SharedOfficialRoot::Conversation) => {
                    if let Some(draft) = self.selected_draft_mut() {
                        draft.content.push_str(value);
                        draft.mark_dirty();
                        self.last_result = format!(
                            "Private Conversation draft · {} characters",
                            draft.content.len()
                        );
                    }
                }
                Some(SharedOfficialRoot::Todo) => {
                    self.new_record_title.push_str(value);
                    self.last_result = format!(
                        "New task draft · {} characters",
                        self.new_record_title.len()
                    );
                }
                _ => {}
            }
        }
    }

    pub fn backspace(&mut self, scene: &RetainedScene) {
        if scene
            .focused()
            .is_some_and(|node| matches!(node.element, SandElement::Textarea | SandElement::Field))
        {
            match self.active {
                Some(SharedOfficialRoot::Record) => {
                    self.draft.pop();
                    self.last_result =
                        format!("Private local draft · {} characters", self.draft.len());
                }
                Some(SharedOfficialRoot::Conversation) => {
                    if let Some(draft) = self.selected_draft_mut() {
                        draft.content.pop();
                        draft.mark_dirty();
                        self.last_result = format!(
                            "Private Conversation draft · {} characters",
                            draft.content.len()
                        );
                    }
                }
                Some(SharedOfficialRoot::Todo) => {
                    self.new_record_title.pop();
                    self.last_result = format!(
                        "New task draft · {} characters",
                        self.new_record_title.len()
                    );
                }
                _ => {}
            }
        }
    }

    pub fn status_line(&self) -> &str {
        &self.last_result
    }

    pub fn bind_domain_records(&mut self, records: &[DomainRecord], status: String) {
        let selected_uid = self.record_uid.clone();
        self.domain_records = records.to_vec();
        if self
            .selected_workflow_record
            .as_ref()
            .is_some_and(|uid| !self.domain_records.iter().any(|record| &record.uid == uid))
        {
            self.selected_workflow_record = None;
        }
        self.domain_status = status;
        let selected = self
            .domain_records
            .iter()
            .find(|record| record.uid == selected_uid)
            .or_else(|| self.domain_records.first());
        match selected {
            Some(record) => {
                self.record_uid = record.uid.clone();
                self.record_title = record.title.clone();
                self.record_description = record.description.clone();
                self.record_quantity = record.quantity;
                self.record_state = record.kind.clone();
                self.last_result = format!("Live Protein selected {}", record.title);
            }
            None => {
                self.record_uid = "no-record".into();
                self.record_title = "No Records matched this Protein".into();
                self.record_description =
                    "The local Lince connection is live, but its current result is empty.".into();
                self.record_quantity = 0.0;
                self.record_state = "empty".into();
                self.last_result = "Live Protein returned no Records".into();
            }
        }
    }

    pub fn set_domain_status(&mut self, status: String) {
        self.domain_status = status;
    }

    pub fn set_turn_in_flight(&mut self, running: bool) {
        self.turn_in_flight = running;
    }

    pub fn bind_domain_conversations(&mut self, conversations: &[DomainConversation]) {
        let selected_conversation = self.selected_conversation.clone();
        let selected_thread = self.selected_thread.clone();
        self.domain_conversations = conversations.to_vec();
        let conversation = selected_conversation
            .as_deref()
            .and_then(|uid| {
                self.domain_conversations
                    .iter()
                    .find(|conversation| conversation.uid == uid)
            })
            .or_else(|| self.domain_conversations.first());
        self.selected_conversation = conversation.map(|conversation| conversation.uid.clone());
        self.selected_thread = conversation.and_then(|conversation| {
            selected_thread
                .as_deref()
                .and_then(|uid| conversation.threads.iter().find(|thread| thread.uid == uid))
                .or_else(|| conversation.threads.first())
                .map(|thread| thread.uid.clone())
        });
        for draft in self
            .conversation_drafts
            .iter_mut()
            .filter(|draft| !draft.persisted && draft.conversation.is_none())
        {
            draft.conversation = self.selected_conversation.clone();
            draft.thread = self.selected_thread.clone();
        }
    }

    pub fn bind_domain_message_drafts(&mut self, drafts: &[DomainMessageDraft]) {
        let selected_uid = self.selected_draft().map(|draft| draft.uid.clone());
        let selected_conversation = self.selected_conversation.clone();
        let mut existing = std::mem::take(&mut self.conversation_drafts)
            .into_iter()
            .map(|draft| (draft.uid.clone(), draft))
            .collect::<BTreeMap<_, _>>();
        let mut materialized = Vec::new();
        for domain in drafts
            .iter()
            .filter(|draft| Some(draft.conversation.as_str()) == selected_conversation.as_deref())
        {
            let local = existing.remove(&domain.uid);
            if let Some(local) = local
                && (local.dirty || self.pending_targets_draft(&domain.uid))
            {
                materialized.push(local);
            } else {
                materialized.push(ConversationDraft::from_domain(domain));
            }
        }
        let keep_pristine_local = materialized.is_empty();
        materialized.extend(existing.into_values().filter(|draft| {
            !draft.persisted && (draft.dirty || !draft.content.is_empty() || keep_pristine_local)
        }));
        if materialized.is_empty() {
            materialized.push(self.fresh_conversation_draft(false));
        }
        materialized.sort_by_key(|draft| {
            drafts
                .iter()
                .find(|domain| domain.uid == draft.uid)
                .map(|domain| domain.position as usize)
                .unwrap_or(usize::MAX)
        });
        self.conversation_drafts = materialized;
        self.selected_conversation_draft = selected_uid
            .as_deref()
            .and_then(|uid| {
                self.conversation_drafts
                    .iter()
                    .position(|draft| draft.uid == uid)
            })
            .unwrap_or_default();
    }

    pub fn take_intents(&mut self) -> Vec<OfficialRuntimeIntent> {
        self.queue_debounced_draft_persistence();
        std::mem::take(&mut self.outbound_intents)
    }

    pub fn domain_action_submitted(&mut self, id: String, intent: OfficialRuntimeIntent) {
        let action = match intent {
            OfficialRuntimeIntent::SendMessage { source, .. } => {
                PendingDomainAction::SendMessage(source)
            }
            OfficialRuntimeIntent::CreateDraft {
                local_uid,
                send_after_create,
                ..
            } => {
                let revision = self
                    .conversation_drafts
                    .iter()
                    .find(|draft| draft.uid == local_uid)
                    .map_or(0, |draft| draft.revision);
                PendingDomainAction::CreateDraft {
                    local_uid,
                    revision,
                    send_after_create,
                }
            }
            OfficialRuntimeIntent::ReviseDraft {
                draft,
                revision,
                send_after_revise,
                ..
            } => PendingDomainAction::ReviseDraft {
                draft,
                revision,
                send_after_revise,
            },
            OfficialRuntimeIntent::DeleteDraft { draft } => {
                PendingDomainAction::DeleteDraft { draft }
            }
            OfficialRuntimeIntent::SendDraft { draft, pinned } => {
                PendingDomainAction::SendDraft { draft, pinned }
            }
            OfficialRuntimeIntent::CreateRecord { source, .. } => {
                PendingDomainAction::CreateRecord { source }
            }
            OfficialRuntimeIntent::SetRecordQuantity {
                record,
                value,
                source,
            } => PendingDomainAction::SetRecordQuantity {
                record,
                value,
                source,
            },
        };
        self.pending_domain_action = Some((id.clone(), action));
        self.last_result = format!("Domain Action {id} submitted; local state awaits its reply");
    }

    pub fn domain_action_refused(&mut self, intent: &OfficialRuntimeIntent, error: String) {
        if intent_sends_message(intent) {
            self.message_send_failures = self.message_send_failures.saturating_add(1);
        }
        if intent_is_workflow_action(intent) {
            self.workflow_action_failures = self.workflow_action_failures.saturating_add(1);
        }
        if let Some(uid) = intent_draft_target(intent)
            && let Some(draft) = self
                .conversation_drafts
                .iter_mut()
                .find(|draft| draft.uid == uid)
        {
            draft.changed_at = Instant::now();
        }
        self.last_result = format!("Domain Action was not submitted: {error}; draft retained");
    }

    pub fn apply_action_receipt(&mut self, receipt: DomainActionReceipt) {
        let Some((pending_id, action)) = self.pending_domain_action.clone() else {
            return;
        };
        if pending_id != receipt.id {
            return;
        }
        self.pending_domain_action = None;
        if receipt.succeeded {
            match action {
                PendingDomainAction::SendMessage(source) => {
                    self.message_send_acknowledgements =
                        self.message_send_acknowledgements.saturating_add(1);
                    if source == MessageDraftSource::Record {
                        self.draft.clear();
                    }
                    self.last_result = acknowledged_message(receipt.created);
                }
                PendingDomainAction::CreateDraft {
                    local_uid,
                    revision,
                    send_after_create,
                } => {
                    let Some(created) = receipt.created else {
                        if let Some(draft) = self
                            .conversation_drafts
                            .iter_mut()
                            .find(|draft| draft.uid == local_uid)
                        {
                            draft.changed_at = Instant::now();
                        }
                        self.last_result =
                            "Draft creation was acknowledged without a created Record".into();
                        return;
                    };
                    if let Some(draft) = self
                        .conversation_drafts
                        .iter_mut()
                        .find(|draft| draft.uid == local_uid)
                    {
                        draft.uid = created.clone();
                        draft.persisted = true;
                        if draft.revision == revision {
                            draft.dirty = false;
                        }
                    }
                    self.last_result = format!("Private draft {created} persisted");
                    if send_after_create {
                        self.queue_message_stage(MessageDraftSource::Conversation(created), false);
                    }
                }
                PendingDomainAction::ReviseDraft {
                    draft,
                    revision,
                    send_after_revise,
                } => {
                    if let Some(local) = self
                        .conversation_drafts
                        .iter_mut()
                        .find(|local| local.uid == draft)
                        && local.revision == revision
                    {
                        local.dirty = false;
                    }
                    self.last_result = format!("Private draft {draft} persisted");
                    if send_after_revise {
                        self.queue_message_stage(MessageDraftSource::Conversation(draft), false);
                    }
                }
                PendingDomainAction::DeleteDraft { draft } => {
                    self.remove_conversation_draft(&draft);
                    self.last_result = format!("Private draft {draft} deleted");
                }
                PendingDomainAction::SendDraft { draft, pinned } => {
                    self.message_send_acknowledgements =
                        self.message_send_acknowledgements.saturating_add(1);
                    if !pinned {
                        self.remove_conversation_draft(&draft);
                    }
                    self.last_result = acknowledged_message(receipt.created);
                }
                PendingDomainAction::CreateRecord { source } => {
                    self.workflow_action_acknowledgements =
                        self.workflow_action_acknowledgements.saturating_add(1);
                    if source == SharedOfficialRoot::Todo {
                        self.new_record_title.clear();
                    }
                    self.selected_workflow_record = receipt.created.clone();
                    self.last_result = match receipt.created {
                        Some(created) => format!(
                            "{} created Record {created}; live Protein will place it",
                            source.label()
                        ),
                        None => format!(
                            "{} Record creation acknowledged; live Protein will update",
                            source.label()
                        ),
                    };
                }
                PendingDomainAction::SetRecordQuantity {
                    record,
                    value,
                    source,
                } => {
                    self.workflow_action_acknowledgements =
                        self.workflow_action_acknowledgements.saturating_add(1);
                    self.last_result = format!(
                        "{} placed {record} at quantity {value}; live Protein will reconcile",
                        source.label()
                    );
                }
            }
        } else {
            if pending_action_sends_message(&action) {
                self.message_send_failures = self.message_send_failures.saturating_add(1);
            }
            if pending_action_is_workflow(&action) {
                self.workflow_action_failures = self.workflow_action_failures.saturating_add(1);
            }
            if let Some(uid) = pending_draft_target(&action)
                && let Some(draft) = self
                    .conversation_drafts
                    .iter_mut()
                    .find(|draft| draft.uid == uid)
            {
                draft.changed_at = Instant::now();
            }
            self.last_result = format!(
                "Domain Action failed: {}; draft retained",
                receipt.error.unwrap_or_else(|| "unknown refusal".into())
            );
        }
    }

    fn selected_draft(&self) -> Option<&ConversationDraft> {
        self.conversation_drafts
            .get(self.selected_conversation_draft)
    }

    fn selected_draft_mut(&mut self) -> Option<&mut ConversationDraft> {
        self.conversation_drafts
            .get_mut(self.selected_conversation_draft)
    }

    fn fresh_conversation_draft(&mut self, dirty: bool) -> ConversationDraft {
        let uid = format!("native-draft-{}", self.next_draft_id);
        self.next_draft_id = self.next_draft_id.saturating_add(1);
        ConversationDraft {
            uid,
            conversation: self.selected_conversation.clone(),
            thread: self.selected_thread.clone(),
            content: String::new(),
            pinned: false,
            timing: DraftSendTiming::Now,
            created_at: Instant::now(),
            persisted_created_at: None,
            persisted: false,
            dirty,
            revision: u64::from(dirty),
            changed_at: Instant::now(),
        }
    }

    fn pending_targets_draft(&self, uid: &str) -> bool {
        self.pending_domain_action
            .as_ref()
            .is_some_and(|(_, action)| match action {
                PendingDomainAction::SendMessage(MessageDraftSource::Conversation(draft))
                | PendingDomainAction::CreateDraft {
                    local_uid: draft, ..
                }
                | PendingDomainAction::ReviseDraft { draft, .. }
                | PendingDomainAction::DeleteDraft { draft }
                | PendingDomainAction::SendDraft { draft, .. } => draft == uid,
                PendingDomainAction::SendMessage(MessageDraftSource::Record)
                | PendingDomainAction::CreateRecord { .. }
                | PendingDomainAction::SetRecordQuantity { .. } => false,
            })
    }

    fn remove_conversation_draft(&mut self, uid: &str) {
        if let Some(index) = self
            .conversation_drafts
            .iter()
            .position(|draft| draft.uid == uid)
        {
            self.conversation_drafts.remove(index);
            self.selected_conversation_draft = self
                .selected_conversation_draft
                .min(self.conversation_drafts.len().saturating_sub(1));
        }
    }

    fn selected_thread_uid(&self) -> Option<String> {
        self.selected_thread.clone().or_else(|| {
            (self.domain_status == "representative Protein-shaped data")
                .then(|| "thread-interface-c4".into())
        })
    }

    fn selected_conversation_uid(&self) -> Option<String> {
        self.selected_conversation.clone().or_else(|| {
            (self.domain_status == "representative Protein-shaped data")
                .then(|| "conversation-interface-c4".into())
        })
    }

    fn queue_message(&mut self, source: MessageDraftSource) {
        self.queue_message_stage(source, true);
    }

    fn queue_message_stage(&mut self, source: MessageDraftSource, count_request: bool) {
        if self.pending_domain_action.is_some() || !self.outbound_intents.is_empty() {
            self.last_result = "A domain Action is already awaiting acknowledgement".into();
            return;
        }
        let (body, timing, persisted, dirty, pinned, revision) = match &source {
            MessageDraftSource::Record => (
                self.draft.trim().to_string(),
                DraftSendTiming::Now,
                false,
                false,
                false,
                0,
            ),
            MessageDraftSource::Conversation(uid) => {
                let Some(draft) = self
                    .conversation_drafts
                    .iter()
                    .find(|draft| &draft.uid == uid)
                else {
                    self.last_result = "The selected private draft no longer exists".into();
                    return;
                };
                (
                    draft.content.trim().to_string(),
                    draft.timing,
                    draft.persisted,
                    draft.dirty,
                    draft.pinned,
                    draft.revision,
                )
            }
        };
        if body.is_empty() {
            self.last_result = "Nothing sent: the draft is empty".into();
            return;
        }
        if self.turn_in_flight && timing != DraftSendTiming::Now {
            self.last_result = format!(
                "Draft queued for {}; no running turn is attached to this Conversation",
                timing.label()
            );
            return;
        }
        let Some(thread) = self.selected_thread_uid() else {
            self.last_result = "Nothing sent: no live Conversation thread is selected".into();
            return;
        };
        if count_request {
            self.message_send_requests = self.message_send_requests.saturating_add(1);
        }
        match source {
            MessageDraftSource::Record => {
                self.outbound_intents
                    .push(OfficialRuntimeIntent::SendMessage {
                        thread,
                        body,
                        source: MessageDraftSource::Record,
                    });
            }
            MessageDraftSource::Conversation(uid) if !persisted => {
                let Some(conversation) = self.selected_conversation_uid() else {
                    self.last_result =
                        "Nothing sent: no live Conversation is selected for this draft".into();
                    return;
                };
                self.outbound_intents
                    .push(OfficialRuntimeIntent::CreateDraft {
                        local_uid: uid,
                        conversation,
                        thread,
                        body,
                        pinned,
                        timing,
                        position: self.selected_conversation_draft as u32,
                        send_after_create: true,
                    });
            }
            MessageDraftSource::Conversation(uid) if dirty => {
                self.outbound_intents
                    .push(OfficialRuntimeIntent::ReviseDraft {
                        draft: uid,
                        body,
                        pinned,
                        timing,
                        position: self.selected_conversation_draft as u32,
                        revision,
                        send_after_revise: true,
                    });
            }
            MessageDraftSource::Conversation(uid) => {
                self.outbound_intents
                    .push(OfficialRuntimeIntent::SendDraft { draft: uid, pinned });
            }
        }
        self.last_result = format!(
            "Message send request {} ready for the domain Action boundary",
            self.message_send_requests
        );
    }

    fn queue_debounced_draft_persistence(&mut self) {
        if self.pending_domain_action.is_some() || !self.outbound_intents.is_empty() {
            return;
        }
        let Some((index, draft)) = self
            .conversation_drafts
            .iter()
            .enumerate()
            .find(|(_, draft)| draft.dirty && draft.changed_at.elapsed() >= DRAFT_PERSIST_DEBOUNCE)
        else {
            return;
        };
        let Some(conversation) = draft
            .conversation
            .clone()
            .or_else(|| self.selected_conversation.clone())
        else {
            return;
        };
        let Some(thread) = draft
            .thread
            .clone()
            .or_else(|| self.selected_thread.clone())
        else {
            return;
        };
        let intent = if draft.persisted {
            OfficialRuntimeIntent::ReviseDraft {
                draft: draft.uid.clone(),
                body: draft.content.clone(),
                pinned: draft.pinned,
                timing: draft.timing,
                position: index as u32,
                revision: draft.revision,
                send_after_revise: false,
            }
        } else {
            OfficialRuntimeIntent::CreateDraft {
                local_uid: draft.uid.clone(),
                conversation,
                thread,
                body: draft.content.clone(),
                pinned: draft.pinned,
                timing: draft.timing,
                position: index as u32,
                send_after_create: false,
            }
        };
        self.outbound_intents.push(intent);
    }

    fn record_collection_scene(
        &self,
        root: SharedOfficialRoot,
        viewport: RetainedRect,
    ) -> Result<RetainedScene, String> {
        let placements = match root {
            SharedOfficialRoot::Table => self.table_placements(viewport),
            SharedOfficialRoot::Todo => self.todo_placements(viewport),
            SharedOfficialRoot::Kanban => self.kanban_placements(viewport),
            _ => return Err(format!("{} is not a Record collection", root.label())),
        };
        RetainedScene::from_placements(
            &official_sand_package(),
            root.uid(),
            placements,
            self.focused,
        )
    }

    fn table_placements(&self, viewport: RetainedRect) -> Vec<RetainedPlacement> {
        let x = viewport.x + 12.0;
        let width = (viewport.width - 24.0).clamp(240.0, 760.0);
        let mut placements = vec![retained_placement(
            "table/tools",
            "official-toolbar",
            BTreeMap::from([
                (
                    "primary-label".into(),
                    SandValue::Text("Previous page".into()),
                ),
                (
                    "secondary-label".into(),
                    SandValue::Text("Next page".into()),
                ),
                (
                    "tertiary-label".into(),
                    SandValue::Text("Add Record".into()),
                ),
            ]),
            RetainedRect {
                x,
                y: viewport.y + 12.0,
                width: width.min(420.0),
                height: 28.0,
            },
        )];
        let records = self.workflow_records();
        let page_size = (((viewport.height - 104.0).max(26.0) / 32.0) as usize).clamp(1, 20);
        let (start, end) = page_window(records.len(), self.workflow_page, page_size);
        for (row, record) in records[start..end].iter().enumerate() {
            placements.push(retained_placement(
                format!("table/rows/{}", record_key(record.uid.as_str())),
                "official-table-row",
                record_row_inputs(record),
                RetainedRect {
                    x,
                    y: viewport.y + 50.0 + row as f32 * 32.0,
                    width,
                    height: 26.0,
                },
            ));
        }
        if records.is_empty() {
            placements.push(retained_placement(
                "table/empty",
                "official-empty-state",
                BTreeMap::from([
                    ("title".into(), SandValue::Text("No Records".into())),
                    (
                        "detail".into(),
                        SandValue::Text("This Protein result is empty".into()),
                    ),
                ]),
                RetainedRect {
                    x,
                    y: viewport.y + 50.0,
                    width: width.min(340.0),
                    height: 130.0,
                },
            ));
        }
        placements.push(workflow_status_placement(
            "table/status",
            x,
            viewport.y + viewport.height - 36.0,
            width.min(620.0),
            "Protein",
            format!(
                "{} Records · rows {}-{} · {}",
                records.len(),
                if records.is_empty() { 0 } else { start + 1 },
                end,
                self.domain_status
            ),
        ));
        placements
    }

    fn todo_placements(&self, viewport: RetainedRect) -> Vec<RetainedPlacement> {
        let x = viewport.x + 12.0;
        let width = (viewport.width - 24.0).clamp(460.0, 760.0);
        let mut placements = vec![
            retained_placement(
                "todo/new/title",
                "official-labeled-field",
                BTreeMap::from([
                    ("label".into(), SandValue::Text("New task".into())),
                    (
                        "value".into(),
                        SandValue::Text(self.new_record_title.clone()),
                    ),
                ]),
                RetainedRect {
                    x,
                    y: viewport.y + 12.0,
                    width: 320.0,
                    height: 80.0,
                },
            ),
            retained_placement(
                "todo/new/create",
                "official-action",
                BTreeMap::from([
                    ("label".into(), SandValue::Text("Add task".into())),
                    (
                        "description".into(),
                        SandValue::Text("Create a plain Record with quantity -1".into()),
                    ),
                ]),
                RetainedRect {
                    x: x + 332.0,
                    y: viewport.y + 36.0,
                    width: 120.0,
                    height: 28.0,
                },
            ),
        ];
        let records = self
            .workflow_records()
            .into_iter()
            .filter(|record| record.kind == "plain" && record.quantity < 0.0)
            .collect::<Vec<_>>();
        let page_size = (((viewport.height - 190.0).max(156.0) / 164.0) as usize).clamp(1, 5);
        let (start, end) = page_window(records.len(), self.workflow_page, page_size);
        for (row, record) in records[start..end].iter().enumerate() {
            let y = viewport.y + 102.0 + row as f32 * 164.0;
            placements.push(retained_placement(
                format!("todo/tasks/{}", record_key(record.uid.as_str())),
                "official-record-summary",
                record_summary_inputs(record, self.selected_workflow_record.as_deref()),
                RetainedRect {
                    x,
                    y,
                    width: 340.0,
                    height: 156.0,
                },
            ));
            placements.push(retained_placement(
                format!("todo/tasks/{}/complete", record_key(record.uid.as_str())),
                "official-action",
                BTreeMap::from([
                    ("label".into(), SandValue::Text("Complete".into())),
                    (
                        "description".into(),
                        SandValue::Text("Set this Record quantity to zero".into()),
                    ),
                ]),
                RetainedRect {
                    x: x + 350.0,
                    y: y + 96.0,
                    width: 110.0,
                    height: 28.0,
                },
            ));
        }
        if records.is_empty() {
            placements.push(retained_placement(
                "todo/empty",
                "official-empty-state",
                BTreeMap::from([
                    (
                        "title".into(),
                        SandValue::Text("Nothing needs doing".into()),
                    ),
                    (
                        "detail".into(),
                        SandValue::Text("No plain negative-quantity Records matched".into()),
                    ),
                ]),
                RetainedRect {
                    x,
                    y: viewport.y + 102.0,
                    width: 340.0,
                    height: 130.0,
                },
            ));
        }
        placements.push(retained_placement(
            "todo/pager",
            "official-toolbar",
            BTreeMap::from([
                (
                    "primary-label".into(),
                    SandValue::Text("Previous page".into()),
                ),
                (
                    "secondary-label".into(),
                    SandValue::Text("Next page".into()),
                ),
                (
                    "tertiary-label".into(),
                    SandValue::Text("Open selected".into()),
                ),
            ]),
            RetainedRect {
                x: x + width - 320.0,
                y: viewport.y + viewport.height - 72.0,
                width: 320.0,
                height: 28.0,
            },
        ));
        placements.push(workflow_status_placement(
            "todo/status",
            x,
            viewport.y + viewport.height - 36.0,
            width.min(620.0),
            "Todo Protein",
            format!(
                "{} open tasks · rows {}-{} · {}",
                records.len(),
                if records.is_empty() { 0 } else { start + 1 },
                end,
                self.domain_status
            ),
        ));
        placements
    }

    fn kanban_placements(&self, viewport: RetainedRect) -> Vec<RetainedPlacement> {
        let x = viewport.x + 8.0;
        let width = (viewport.width - 16.0).max(500.0);
        let lane_gap = 6.0;
        let lane_width =
            (width - lane_gap * (KANBAN_LANES.len() - 1) as f32) / KANBAN_LANES.len() as f32;
        let mut placements = vec![retained_placement(
            "kanban/tools",
            "official-toolbar",
            BTreeMap::from([
                ("primary-label".into(), SandValue::Text("Move left".into())),
                (
                    "secondary-label".into(),
                    SandValue::Text("Move right".into()),
                ),
                (
                    "tertiary-label".into(),
                    SandValue::Text("Add Record".into()),
                ),
            ]),
            RetainedRect {
                x,
                y: viewport.y + 8.0,
                width: 420.0,
                height: 28.0,
            },
        )];
        let records = self.workflow_records();
        let card_height = 156.0;
        let card_gap = 8.0;
        let per_lane = (((viewport.height - 122.0).max(card_height) / (card_height + card_gap))
            as usize)
            .clamp(1, 4);
        let mut hidden = 0_usize;
        for (lane_index, lane) in KANBAN_LANES.iter().enumerate() {
            let lane_x = x + lane_index as f32 * (lane_width + lane_gap);
            let lane_records = records
                .iter()
                .filter(|record| kanban_lane_for(record.quantity).key == lane.key)
                .collect::<Vec<_>>();
            hidden = hidden.saturating_add(lane_records.len().saturating_sub(per_lane));
            placements.push(retained_placement(
                format!("kanban/lanes/{}/title", lane.key),
                "title",
                BTreeMap::from([(
                    "value".into(),
                    SandValue::Text(format!("{} · {}", lane.label, lane_records.len())),
                )]),
                RetainedRect {
                    x: lane_x,
                    y: viewport.y + 44.0,
                    width: lane_width,
                    height: 26.0,
                },
            ));
            for (row, record) in lane_records.into_iter().take(per_lane).enumerate() {
                placements.push(retained_placement(
                    format!(
                        "kanban/lanes/{}/{}",
                        lane.key,
                        record_key(record.uid.as_str())
                    ),
                    "official-record-summary",
                    record_summary_inputs(record, self.selected_workflow_record.as_deref()),
                    RetainedRect {
                        x: lane_x,
                        y: viewport.y + 76.0 + row as f32 * (card_height + card_gap),
                        width: lane_width,
                        height: card_height,
                    },
                ));
            }
        }
        placements.push(workflow_status_placement(
            "kanban/status",
            x,
            viewport.y + viewport.height - 36.0,
            width.min(720.0),
            "Kanban Protein",
            format!(
                "{} Records · {} outside the visible lane capacity · {}",
                records.len(),
                hidden,
                self.domain_status
            ),
        ));
        placements
    }

    fn workflow_records(&self) -> Vec<DomainRecord> {
        if !self.domain_records.is_empty()
            || self.domain_status != "representative Protein-shaped data"
        {
            return self.domain_records.clone();
        }
        representative_workflow_records()
    }

    fn record_for_node(&self, key: &str) -> Option<DomainRecord> {
        self.workflow_records().into_iter().find(|record| {
            key.split('/')
                .any(|segment| segment == record_key(record.uid.as_str()))
        })
    }

    fn select_workflow_record(&mut self, key: &str) {
        if let Some(record) = self.record_for_node(key) {
            self.emit_workflow_record(record);
        } else {
            self.last_result = "The selected Protein row is no longer present".into();
        }
    }

    fn open_selected_workflow_record(&mut self) {
        let selected = self.selected_workflow_record.clone();
        let record = selected.as_deref().and_then(|uid| {
            self.workflow_records()
                .into_iter()
                .find(|record| record.uid == uid)
        });
        if let Some(record) = record {
            self.emit_workflow_record(record);
        } else {
            self.last_result = "Select a Todo Record before opening it".into();
        }
    }

    fn emit_workflow_record(&mut self, record: DomainRecord) {
        self.selected_workflow_record = Some(record.uid.clone());
        self.record_uid = record.uid.clone();
        self.record_title = record.title;
        self.record_description = record.description;
        self.record_quantity = record.quantity;
        self.record_state = record.kind;
        self.emitted_record_events = self.emitted_record_events.saturating_add(1);
        self.last_result = format!("record-clicked emitted for {}", record.uid);
    }

    fn change_workflow_page(&mut self, forward: bool) {
        if forward {
            self.workflow_page = self.workflow_page.saturating_add(1);
        } else {
            self.workflow_page = self.workflow_page.saturating_sub(1);
        }
        self.focused = 0;
        self.last_result = format!("Protein result page {} selected", self.workflow_page + 1);
    }

    fn queue_create_record(
        &mut self,
        source: SharedOfficialRoot,
        title: String,
        description: String,
        quantity: f64,
    ) {
        if self.pending_domain_action.is_some() || !self.outbound_intents.is_empty() {
            self.last_result = "A domain Action is already awaiting acknowledgement".into();
            return;
        }
        self.workflow_action_requests = self.workflow_action_requests.saturating_add(1);
        self.outbound_intents
            .push(OfficialRuntimeIntent::CreateRecord {
                title,
                description,
                quantity,
                source,
            });
        self.last_result = format!(
            "{} Record creation is ready for the domain Action boundary",
            source.label()
        );
    }

    fn queue_set_record_quantity(
        &mut self,
        source: SharedOfficialRoot,
        record: String,
        value: f64,
    ) {
        if self.pending_domain_action.is_some() || !self.outbound_intents.is_empty() {
            self.last_result = "A domain Action is already awaiting acknowledgement".into();
            return;
        }
        self.workflow_action_requests = self.workflow_action_requests.saturating_add(1);
        self.outbound_intents
            .push(OfficialRuntimeIntent::SetRecordQuantity {
                record: record.clone(),
                value,
                source,
            });
        self.last_result = format!(
            "{} quantity change for {record} is ready for the domain Action boundary",
            source.label()
        );
    }

    fn move_selected_kanban_record(&mut self, offset: isize) {
        let records = self.workflow_records();
        let selected = self
            .selected_workflow_record
            .as_deref()
            .and_then(|uid| records.iter().find(|record| record.uid == uid))
            .or_else(|| records.first());
        let Some(record) = selected else {
            self.last_result = "No Protein Record is available to move".into();
            return;
        };
        let current = KANBAN_LANES
            .iter()
            .position(|lane| lane.key == kanban_lane_for(record.quantity).key)
            .unwrap_or_default();
        let target = current.saturating_add_signed(offset);
        if target >= KANBAN_LANES.len() || target == current {
            self.last_result = format!("{} is already in the edge lane", record.title);
            return;
        }
        let record_uid = record.uid.clone();
        let lane = KANBAN_LANES[target];
        self.selected_workflow_record = Some(record_uid.clone());
        self.queue_set_record_quantity(SharedOfficialRoot::Kanban, record_uid, lane.quantity);
    }

    pub fn facts(&self, scene: Option<&RetainedScene>) -> SharedOfficialRuntimeFacts {
        SharedOfficialRuntimeFacts {
            active_root: self.active,
            focused_control: self.focused,
            retained_nodes: scene.map_or(0, |scene| scene.nodes.len()),
            interactive_nodes: scene.map_or(0, RetainedScene::interactive_count),
            edit_mode: self.edit_mode,
            group_locked: self.group_locked,
            castle_save_requests: self.castle_save_requests,
            zoom_percent: self.zoom_percent,
            recenter_actions: self.recenter_actions,
            emitted_record_events: self.emitted_record_events,
            message_send_requests: self.message_send_requests,
            message_send_acknowledgements: self.message_send_acknowledgements,
            message_send_failures: self.message_send_failures,
            property_presentation: self.property_presentation,
            domain_records: self.domain_records.len(),
            domain_conversations: self.domain_conversations.len(),
            conversation_drafts: self.conversation_drafts.len(),
            persisted_conversation_drafts: self
                .conversation_drafts
                .iter()
                .filter(|draft| draft.persisted)
                .count(),
            dirty_conversation_drafts: self
                .conversation_drafts
                .iter()
                .filter(|draft| draft.dirty)
                .count(),
            pinned_conversation_drafts: self
                .conversation_drafts
                .iter()
                .filter(|draft| draft.pinned)
                .count(),
            pending_message_action: self.pending_domain_action.is_some(),
            workflow_action_requests: self.workflow_action_requests,
            workflow_action_acknowledgements: self.workflow_action_acknowledgements,
            workflow_action_failures: self.workflow_action_failures,
            selected_workflow_record: self.selected_workflow_record.clone(),
            workflow_page: self.workflow_page,
            domain_status: self.domain_status.clone(),
        }
    }

    pub fn panel_text(&self, scene: &RetainedScene) -> String {
        let root = self.active.expect("active runtime root");
        let focused = scene
            .focused()
            .map(|node| format!("{} · {}", node.label, node.key))
            .unwrap_or_else(|| "No interactive control".into());
        let facts = self.facts(Some(scene));
        format!(
            "LINCE · {} · NATIVE RETAINED RUNTIME\n\nACTIVE · Esc catalog · Tab/Shift-Tab focus · Enter/Space activate · pointer selects and activates\n\nFOCUS\n{}\n\n{}\n\nDOMAIN · {}\n\n{} retained semantic nodes · {} interactive\nedit {} · group locked {} · Castle save requests {}\nzoom {}% · recenters {}\nrecord events {} · message requests {} · acknowledged {} · failed {}\nworkflow requests {} · acknowledged {} · failed {} · selected {:?} · page {}\nConversations {} · private drafts {} · persisted {} · dirty {} · pinned {} · awaiting Action {} · {}\n\nThis is an active C4 slice. Unmigrated official Sands remain visibly pending.",
            root.label(),
            focused,
            self.last_result,
            self.domain_status,
            facts.retained_nodes,
            facts.interactive_nodes,
            facts.edit_mode,
            facts.group_locked,
            facts.castle_save_requests,
            facts.zoom_percent,
            facts.recenter_actions,
            facts.emitted_record_events,
            facts.message_send_requests,
            facts.message_send_acknowledgements,
            facts.message_send_failures,
            facts.workflow_action_requests,
            facts.workflow_action_acknowledgements,
            facts.workflow_action_failures,
            facts.selected_workflow_record,
            facts.workflow_page + 1,
            facts.domain_conversations,
            facts.conversation_drafts,
            facts.persisted_conversation_drafts,
            facts.dirty_conversation_drafts,
            facts.pinned_conversation_drafts,
            facts.pending_message_action,
            facts.property_presentation.label(),
        )
    }

    pub fn exercise(&mut self) -> Result<(), String> {
        for (root, keys) in [
            (
                SharedOfficialRoot::ShellEdit,
                [
                    "tools/primary/control",
                    "tools/secondary/control",
                    "tools/tertiary/control",
                ]
                .as_slice(),
            ),
            (
                SharedOfficialRoot::ShellZoom,
                ["zoom-out/control", "zoom-in/control", "recenter/control"].as_slice(),
            ),
            (
                SharedOfficialRoot::Record,
                ["summary/open/control", "properties/action/control"].as_slice(),
            ),
        ] {
            self.open(root.uid());
            for suffix in keys {
                self.activate_suffix(suffix)?;
            }
        }
        let mut scene = self.scene(fixture_viewport())?;
        let composer = scene
            .nodes
            .iter()
            .filter(|node| node.interactive)
            .position(|node| node.key.ends_with("thread/composer"))
            .ok_or_else(|| "Record composer is missing".to_string())?;
        self.focus_at(composer, &scene);
        scene = self.scene(fixture_viewport())?;
        self.input_text("Retained runtime message", &scene);
        self.activate_suffix("thread/send/control")?;
        self.outbound_intents.clear();
        self.open(SharedOfficialRoot::Conversation.uid());
        self.activate_suffix("drafts/add/control")?;
        self.activate_suffix("drafts/draft/pinned")?;
        let mut scene = self.scene(fixture_viewport())?;
        let composer = scene
            .nodes
            .iter()
            .filter(|node| node.interactive)
            .position(|node| node.key.ends_with("thread/composer"))
            .ok_or_else(|| "Conversation composer is missing".to_string())?;
        self.focus_at(composer, &scene);
        scene = self.scene(fixture_viewport())?;
        self.input_text("Acknowledged Conversation message", &scene);
        self.activate_suffix("thread/send/control")?;
        self.outbound_intents.clear();
        self.open(SharedOfficialRoot::Table.uid());
        self.activate_suffix("open/control")?;
        self.open(SharedOfficialRoot::Todo.uid());
        let mut scene = self.scene(fixture_viewport())?;
        let creator = scene
            .nodes
            .iter()
            .filter(|node| node.interactive)
            .position(|node| node.key.ends_with("new/title/control"))
            .ok_or_else(|| "Todo creator is missing".to_string())?;
        self.focus_at(creator, &scene);
        scene = self.scene(fixture_viewport())?;
        self.input_text("Native task", &scene);
        self.activate_suffix("new/create/control")?;
        self.outbound_intents.clear();
        self.open(SharedOfficialRoot::Kanban.uid());
        self.activate_suffix("open/control")?;
        self.activate_suffix("tools/secondary/control")?;
        self.outbound_intents.clear();
        self.close();
        Ok(())
    }

    fn activate_suffix(&mut self, suffix: &str) -> Result<(), String> {
        let scene = self.scene(fixture_viewport())?;
        let index = scene
            .nodes
            .iter()
            .filter(|node| node.interactive)
            .position(|node| node.key.ends_with(suffix))
            .ok_or_else(|| format!("retained control {suffix} is missing"))?;
        self.focus_at(index, &scene);
        let focused = self.scene(fixture_viewport())?;
        self.activate(&focused)
    }

    fn inputs(&self, root: SharedOfficialRoot) -> BTreeMap<String, SandValue> {
        match root {
            SharedOfficialRoot::ShellEdit => BTreeMap::from([
                (
                    "source".into(),
                    SandValue::Record("interface-selection".into()),
                ),
                (
                    "secondary-label".into(),
                    SandValue::Text(if self.group_locked {
                        "Unlock group".into()
                    } else {
                        "Lock group".into()
                    }),
                ),
            ]),
            SharedOfficialRoot::ShellZoom => BTreeMap::from([
                (
                    "source".into(),
                    SandValue::Record("interface-camera".into()),
                ),
                (
                    "level".into(),
                    SandValue::Number(f64::from(self.zoom_percent)),
                ),
            ]),
            SharedOfficialRoot::Record => BTreeMap::from([
                ("source".into(), SandValue::Record(self.record_uid.clone())),
                ("title".into(), SandValue::Text(self.record_title.clone())),
                (
                    "description".into(),
                    SandValue::Text(self.record_description.clone()),
                ),
                ("quantity".into(), SandValue::Number(self.record_quantity)),
                ("state".into(), SandValue::Text(self.record_state.clone())),
                (
                    "conversation".into(),
                    SandValue::Record("conversation-interface-c4".into()),
                ),
                ("draft".into(), SandValue::Text(self.draft.clone())),
            ]),
            SharedOfficialRoot::Conversation => {
                let conversation = self
                    .selected_conversation
                    .as_deref()
                    .and_then(|uid| {
                        self.domain_conversations
                            .iter()
                            .find(|conversation| conversation.uid == uid)
                    })
                    .or_else(|| self.domain_conversations.first());
                let thread = conversation.and_then(|conversation| {
                    self.selected_thread
                        .as_deref()
                        .and_then(|uid| {
                            conversation.threads.iter().find(|thread| thread.uid == uid)
                        })
                        .or_else(|| conversation.threads.first())
                });
                let message = thread.and_then(|thread| thread.messages.last());
                let draft = self.selected_draft();
                let timing = draft.map_or(DraftSendTiming::Now, |draft| draft.timing);
                let effective_timing: String = if self.turn_in_flight {
                    timing.label().into()
                } else {
                    "send now · no turn running".into()
                };
                BTreeMap::from([
                    (
                        "source".into(),
                        SandValue::Record(
                            conversation
                                .map(|conversation| conversation.uid.clone())
                                .unwrap_or_else(|| "conversation-interface-c4".into()),
                        ),
                    ),
                    (
                        "title".into(),
                        SandValue::Text(
                            thread
                                .map(|thread| thread.title.clone())
                                .or_else(|| conversation.map(|item| item.title.clone()))
                                .unwrap_or_else(|| "No Conversation selected".into()),
                        ),
                    ),
                    (
                        "author".into(),
                        SandValue::Record(
                            message
                                .map(|message| message.author.clone())
                                .unwrap_or_else(|| "unknown-author".into()),
                        ),
                    ),
                    (
                        "operator".into(),
                        SandValue::Record(
                            message
                                .map(|message| message.operator.clone())
                                .unwrap_or_else(|| "unknown-operator".into()),
                        ),
                    ),
                    (
                        "message-content".into(),
                        SandValue::Text(
                            message
                                .map(|message| message.content.clone())
                                .unwrap_or_else(|| "Nothing said yet".into()),
                        ),
                    ),
                    (
                        "message-state".into(),
                        SandValue::Text(
                            message
                                .map(|message| message.state.label().into())
                                .unwrap_or_else(|| "finished".into()),
                        ),
                    ),
                    (
                        "draft".into(),
                        SandValue::Text(
                            draft.map_or_else(String::new, |draft| draft.content.clone()),
                        ),
                    ),
                    (
                        "queued-draft".into(),
                        SandValue::Text(
                            draft.map_or_else(String::new, |draft| draft.content.clone()),
                        ),
                    ),
                    (
                        "draft-pinned".into(),
                        SandValue::Boolean(draft.is_some_and(|draft| draft.pinned)),
                    ),
                    (
                        "draft-age".into(),
                        SandValue::Text(
                            draft.map_or_else(|| "no draft".into(), conversation_draft_age),
                        ),
                    ),
                    (
                        "send-timing".into(),
                        SandValue::Text(effective_timing.clone()),
                    ),
                    (
                        "draft-record".into(),
                        SandValue::Record(
                            draft
                                .map(|draft| draft.uid.clone())
                                .unwrap_or_else(|| "unbound-draft".into()),
                        ),
                    ),
                    (
                        "send-label".into(),
                        SandValue::Text(effective_timing.clone()),
                    ),
                    (
                        "queued-send-label".into(),
                        SandValue::Text(effective_timing),
                    ),
                    ("status".into(), SandValue::Text(self.last_result.clone())),
                ])
            }
            SharedOfficialRoot::Table | SharedOfficialRoot::Todo | SharedOfficialRoot::Kanban => {
                BTreeMap::new()
            }
        }
    }
}

impl ConversationDraft {
    fn from_domain(draft: &DomainMessageDraft) -> Self {
        Self {
            uid: draft.uid.clone(),
            conversation: Some(draft.conversation.clone()),
            thread: Some(draft.thread.clone()),
            content: draft.content.clone(),
            pinned: draft.pinned,
            timing: match draft.timing {
                DomainDraftTiming::Now => DraftSendTiming::Now,
                DomainDraftTiming::NextSafePoint => DraftSendTiming::NextSafePoint,
                DomainDraftTiming::AfterTurn => DraftSendTiming::AfterTurn,
            },
            created_at: Instant::now(),
            persisted_created_at: (!draft.created_at.is_empty()).then(|| draft.created_at.clone()),
            persisted: true,
            dirty: false,
            revision: 0,
            changed_at: Instant::now(),
        }
    }

    fn mark_dirty(&mut self) {
        self.dirty = true;
        self.revision = self.revision.saturating_add(1);
        self.changed_at = Instant::now();
    }
}

#[derive(Clone, Copy)]
struct KanbanLane {
    key: &'static str,
    label: &'static str,
    quantity: f64,
}

const KANBAN_LANES: [KanbanLane; 5] = [
    KanbanLane {
        key: "backlog",
        label: "Backlog",
        quantity: 0.0,
    },
    KanbanLane {
        key: "next",
        label: "Next",
        quantity: -1.0,
    },
    KanbanLane {
        key: "wip",
        label: "WIP",
        quantity: -2.0,
    },
    KanbanLane {
        key: "review",
        label: "Review",
        quantity: -3.0,
    },
    KanbanLane {
        key: "done",
        label: "Done",
        quantity: 1.0,
    },
];

fn kanban_lane_for(quantity: f64) -> KanbanLane {
    if quantity > 0.0 {
        return KANBAN_LANES[4];
    }
    KANBAN_LANES
        .iter()
        .copied()
        .find(|lane| lane.quantity == quantity)
        .unwrap_or(KANBAN_LANES[0])
}

fn retained_placement(
    key: impl Into<String>,
    definition_uid: impl Into<String>,
    inputs: BTreeMap<String, SandValue>,
    rect: RetainedRect,
) -> RetainedPlacement {
    RetainedPlacement {
        key: key.into(),
        definition_uid: definition_uid.into(),
        inputs,
        rect,
    }
}

fn workflow_status_placement(
    key: impl Into<String>,
    x: f32,
    y: f32,
    width: f32,
    state: impl Into<String>,
    detail: impl Into<String>,
) -> RetainedPlacement {
    retained_placement(
        key,
        "official-status",
        BTreeMap::from([
            ("state".into(), SandValue::Text(state.into())),
            ("detail".into(), SandValue::Text(detail.into())),
        ]),
        RetainedRect {
            x,
            y,
            width,
            height: 24.0,
        },
    )
}

fn record_row_inputs(record: &DomainRecord) -> BTreeMap<String, SandValue> {
    BTreeMap::from([
        ("record".into(), SandValue::Record(record.uid.clone())),
        ("title".into(), SandValue::Text(record.title.clone())),
        ("quantity".into(), SandValue::Number(record.quantity)),
        ("state".into(), SandValue::Text(record.kind.clone())),
    ])
}

fn record_summary_inputs(
    record: &DomainRecord,
    selected: Option<&str>,
) -> BTreeMap<String, SandValue> {
    BTreeMap::from([
        ("record".into(), SandValue::Record(record.uid.clone())),
        ("title".into(), SandValue::Text(record.title.clone())),
        (
            "description".into(),
            SandValue::Text(record.description.clone()),
        ),
        ("quantity".into(), SandValue::Number(record.quantity)),
        ("state".into(), SandValue::Text(record.kind.clone())),
        (
            "open-label".into(),
            SandValue::Text(if selected == Some(record.uid.as_str()) {
                "Selected".into()
            } else {
                "Open Record".into()
            }),
        ),
        (
            "open-description".into(),
            SandValue::Text(format!("Open {} through record-clicked", record.title)),
        ),
    ])
}

fn page_window(total: usize, requested_page: usize, page_size: usize) -> (usize, usize) {
    if total == 0 {
        return (0, 0);
    }
    let last_page = total.saturating_sub(1) / page_size;
    let page = requested_page.min(last_page);
    let start = page.saturating_mul(page_size);
    (start, (start + page_size).min(total))
}

fn record_key(uid: &str) -> String {
    let mut key = String::with_capacity(7 + uid.len() * 2);
    key.push_str("record-");
    for byte in uid.as_bytes() {
        use std::fmt::Write;
        let _ = write!(key, "{byte:02x}");
    }
    key
}

fn representative_workflow_records() -> Vec<DomainRecord> {
    [
        (
            "r_backlog",
            "Map the Protein",
            "Read one stable result",
            0.0,
        ),
        (
            "r_next",
            "Compose the Sands",
            "Reuse one card definition",
            -1.0,
        ),
        (
            "r_wip",
            "Retain the scene",
            "Keep semantic identities",
            -2.0,
        ),
        (
            "r_review",
            "Verify the boundary",
            "Exercise typed Actions",
            -3.0,
        ),
        (
            "r_done",
            "Prove the frame",
            "Measure the joined runtime",
            1.0,
        ),
        (
            "r_next_two",
            "Preserve focus",
            "Reconcile without clobbering",
            -1.0,
        ),
        (
            "r_contribution",
            "Share the result",
            "Emit record-clicked",
            2.0,
        ),
    ]
    .into_iter()
    .map(|(uid, title, description, quantity)| DomainRecord {
        uid: uid.into(),
        title: title.into(),
        description: description.into(),
        quantity,
        kind: "plain".into(),
        slug: None,
    })
    .collect()
}

fn intent_sends_message(intent: &OfficialRuntimeIntent) -> bool {
    match intent {
        OfficialRuntimeIntent::SendMessage { .. } | OfficialRuntimeIntent::SendDraft { .. } => true,
        OfficialRuntimeIntent::CreateDraft {
            send_after_create, ..
        } => *send_after_create,
        OfficialRuntimeIntent::ReviseDraft {
            send_after_revise, ..
        } => *send_after_revise,
        OfficialRuntimeIntent::DeleteDraft { .. }
        | OfficialRuntimeIntent::CreateRecord { .. }
        | OfficialRuntimeIntent::SetRecordQuantity { .. } => false,
    }
}

fn pending_action_sends_message(action: &PendingDomainAction) -> bool {
    match action {
        PendingDomainAction::SendMessage(_) | PendingDomainAction::SendDraft { .. } => true,
        PendingDomainAction::CreateDraft {
            send_after_create, ..
        } => *send_after_create,
        PendingDomainAction::ReviseDraft {
            send_after_revise, ..
        } => *send_after_revise,
        PendingDomainAction::DeleteDraft { .. }
        | PendingDomainAction::CreateRecord { .. }
        | PendingDomainAction::SetRecordQuantity { .. } => false,
    }
}

fn intent_is_workflow_action(intent: &OfficialRuntimeIntent) -> bool {
    matches!(
        intent,
        OfficialRuntimeIntent::CreateRecord { .. }
            | OfficialRuntimeIntent::SetRecordQuantity { .. }
    )
}

fn pending_action_is_workflow(action: &PendingDomainAction) -> bool {
    matches!(
        action,
        PendingDomainAction::CreateRecord { .. } | PendingDomainAction::SetRecordQuantity { .. }
    )
}

fn intent_draft_target(intent: &OfficialRuntimeIntent) -> Option<&str> {
    match intent {
        OfficialRuntimeIntent::CreateDraft { local_uid, .. } => Some(local_uid),
        OfficialRuntimeIntent::ReviseDraft { draft, .. }
        | OfficialRuntimeIntent::DeleteDraft { draft }
        | OfficialRuntimeIntent::SendDraft { draft, .. } => Some(draft),
        OfficialRuntimeIntent::SendMessage { source, .. } => match source {
            MessageDraftSource::Conversation(draft) => Some(draft),
            MessageDraftSource::Record => None,
        },
        OfficialRuntimeIntent::CreateRecord { .. }
        | OfficialRuntimeIntent::SetRecordQuantity { .. } => None,
    }
}

fn pending_draft_target(action: &PendingDomainAction) -> Option<&str> {
    match action {
        PendingDomainAction::CreateDraft { local_uid, .. } => Some(local_uid),
        PendingDomainAction::ReviseDraft { draft, .. }
        | PendingDomainAction::DeleteDraft { draft }
        | PendingDomainAction::SendDraft { draft, .. } => Some(draft),
        PendingDomainAction::SendMessage(source) => match source {
            MessageDraftSource::Conversation(draft) => Some(draft),
            MessageDraftSource::Record => None,
        },
        PendingDomainAction::CreateRecord { .. }
        | PendingDomainAction::SetRecordQuantity { .. } => None,
    }
}

fn acknowledged_message(created: Option<String>) -> String {
    match created {
        Some(created) => format!("Message {created} acknowledged; live Protein will update"),
        None => "Message acknowledged; live Protein will update".into(),
    }
}

fn conversation_draft_age(draft: &ConversationDraft) -> String {
    if let Some(created_at) = &draft.persisted_created_at {
        return format!("saved {created_at}");
    }
    let seconds = draft.created_at.elapsed().as_secs();
    if seconds < 60 {
        format!("{seconds}s old")
    } else if seconds < 3_600 {
        format!("{}m old", seconds / 60)
    } else {
        format!("{}h old", seconds / 3_600)
    }
}

fn fixture_viewport() -> RetainedRect {
    RetainedRect {
        x: 0.0,
        y: 0.0,
        width: 900.0,
        height: 900.0,
    }
}

impl Default for SharedOfficialRuntimeState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn viewport() -> RetainedRect {
        RetainedRect {
            x: 0.0,
            y: 0.0,
            width: 900.0,
            height: 900.0,
        }
    }

    fn single_intent(state: &mut SharedOfficialRuntimeState) -> OfficialRuntimeIntent {
        let mut intents = state.take_intents();
        assert_eq!(intents.len(), 1);
        intents.pop().unwrap()
    }

    #[test]
    fn only_operational_roots_open_and_pending_roots_stay_honest() {
        let mut state = SharedOfficialRuntimeState::new();
        assert!(!state.open("relations"));
        assert!(state.status_line().contains("pending"));
        assert!(state.open("record"));
        assert_eq!(state.active(), Some(SharedOfficialRoot::Record));
        assert!(state.open("conversation"));
        assert_eq!(state.active(), Some(SharedOfficialRoot::Conversation));
        assert!(state.open("table"));
        assert!(state.open("todo"));
        assert!(state.open("kanban"));
    }

    #[test]
    fn shell_controls_change_semantic_state() {
        let mut state = SharedOfficialRuntimeState::new();
        state.open("shell-edit");
        let mut scene = state.scene(viewport()).unwrap();
        while !scene
            .focused()
            .is_some_and(|node| node.key.ends_with("tools/primary/control"))
        {
            state.focus_next(false, &scene);
            scene = state.scene(viewport()).unwrap();
        }
        state.activate(&scene).unwrap();
        assert!(state.facts(Some(&scene)).edit_mode);
    }

    #[test]
    fn record_open_emits_the_typed_record_identity() {
        let mut state = SharedOfficialRuntimeState::new();
        state.open("record");
        let mut scene = state.scene(viewport()).unwrap();
        while !scene
            .focused()
            .is_some_and(|node| node.key.ends_with("summary/open/control"))
        {
            state.focus_next(false, &scene);
            scene = state.scene(viewport()).unwrap();
        }
        state.activate(&scene).unwrap();
        assert_eq!(state.facts(Some(&scene)).emitted_record_events, 1);
        assert!(state.status_line().contains("record-interface-c4"));
    }

    #[test]
    fn diagnostic_exercise_operates_every_shared_root_and_returns_to_catalog() {
        let mut state = SharedOfficialRuntimeState::new();
        state.exercise().unwrap();
        let facts = state.facts(None);
        assert_eq!(facts.active_root, None);
        assert!(facts.edit_mode);
        assert!(facts.group_locked);
        assert_eq!(facts.castle_save_requests, 1);
        assert_eq!(facts.zoom_percent, 100);
        assert_eq!(facts.recenter_actions, 1);
        assert_eq!(facts.emitted_record_events, 3);
        assert_eq!(facts.message_send_requests, 2);
        assert_eq!(facts.conversation_drafts, 2);
        assert_eq!(facts.pinned_conversation_drafts, 1);
        assert_eq!(facts.workflow_action_requests, 2);
        assert!(facts.selected_workflow_record.is_some());
        assert_eq!(facts.property_presentation, PropertyPresentation::All);
        assert_eq!(facts.domain_records, 0);
        assert_eq!(facts.domain_status, "representative Protein-shaped data");
    }

    #[test]
    fn live_domain_records_replace_representative_record_without_clobbering_draft() {
        let mut state = SharedOfficialRuntimeState::new();
        state.draft = "private draft".into();
        state.bind_domain_records(
            &[DomainRecord {
                uid: "r_live".into(),
                title: "Live title".into(),
                description: "Live description".into(),
                quantity: 7.0,
                kind: "plain".into(),
                slug: Some("live".into()),
            }],
            "live Protein subscription · 1 Record".into(),
        );
        state.open("record");
        let inputs = state.inputs(SharedOfficialRoot::Record);
        assert_eq!(inputs["source"], SandValue::Record("r_live".into()));
        assert_eq!(inputs["title"], SandValue::Text("Live title".into()));
        assert_eq!(inputs["draft"], SandValue::Text("private draft".into()));
        assert_eq!(state.facts(None).domain_records, 1);
    }

    #[test]
    fn conversation_send_consumes_only_after_acknowledgement() {
        let mut state = SharedOfficialRuntimeState::new();
        state.bind_domain_conversations(&[DomainConversation {
            uid: "r_conversation".into(),
            title: "Interface".into(),
            threads: vec![crate::domain_model::DomainThread {
                uid: "r_thread".into(),
                title: "C4".into(),
                messages: vec![crate::domain_model::DomainMessage {
                    uid: "r_message".into(),
                    author: "r_author".into(),
                    operator: "r_operator".into(),
                    content: "Live body".into(),
                    state: crate::domain_model::DomainMessageState::Writing,
                    created_at: None,
                }],
            }],
        }]);
        let draft = &mut state.conversation_drafts[0];
        draft.content = "Do not lose this".into();
        draft.persisted = true;
        draft.uid = "r_draft".into();
        state.queue_message(MessageDraftSource::Conversation("r_draft".into()));
        let intent = single_intent(&mut state);
        assert!(matches!(
            &intent,
            OfficialRuntimeIntent::SendDraft { draft, .. } if draft == "r_draft"
        ));
        state.domain_action_submitted("native-message-1".into(), intent);
        assert_eq!(state.conversation_drafts[0].content, "Do not lose this");
        state.apply_action_receipt(DomainActionReceipt {
            id: "native-message-1".into(),
            succeeded: false,
            created: None,
            error: Some("refused".into()),
        });
        assert_eq!(state.conversation_drafts[0].content, "Do not lose this");
        state.queue_message(MessageDraftSource::Conversation("r_draft".into()));
        let intent = single_intent(&mut state);
        assert!(matches!(&intent, OfficialRuntimeIntent::SendDraft { .. }));
        state.domain_action_submitted("native-message-2".into(), intent);
        state.apply_action_receipt(DomainActionReceipt {
            id: "native-message-2".into(),
            succeeded: true,
            created: Some("r_created".into()),
            error: None,
        });
        assert!(state.conversation_drafts.is_empty());
        assert_eq!(state.facts(None).message_send_acknowledgements, 1);
        assert_eq!(state.facts(None).message_send_failures, 1);
    }

    #[test]
    fn pinned_conversation_preset_survives_acknowledged_send() {
        let mut state = SharedOfficialRuntimeState::new();
        state.selected_thread = Some("r_thread".into());
        state.conversation_drafts[0].content = "Reusable".into();
        state.conversation_drafts[0].pinned = true;
        state.conversation_drafts[0].persisted = true;
        state.conversation_drafts[0].uid = "r_draft".into();
        state.queue_message(MessageDraftSource::Conversation("r_draft".into()));
        let intent = single_intent(&mut state);
        assert!(matches!(&intent, OfficialRuntimeIntent::SendDraft { .. }));
        state.domain_action_submitted("native-message-1".into(), intent);
        state.apply_action_receipt(DomainActionReceipt {
            id: "native-message-1".into(),
            succeeded: true,
            created: Some("r_created".into()),
            error: None,
        });
        assert_eq!(state.conversation_drafts.len(), 1);
        assert_eq!(state.conversation_drafts[0].content, "Reusable");
    }

    #[test]
    fn live_private_drafts_replace_the_placeholder_and_keep_order() {
        let mut state = SharedOfficialRuntimeState::new();
        state.selected_conversation = Some("r_conversation".into());
        state.selected_thread = Some("r_thread".into());
        state.bind_domain_message_drafts(&[
            DomainMessageDraft {
                uid: "r_later".into(),
                conversation: "r_conversation".into(),
                thread: "r_thread".into(),
                author: "r_author".into(),
                operator: "r_author".into(),
                content: "Later".into(),
                pinned: false,
                timing: DomainDraftTiming::Now,
                position: 2,
                created_at: "2026-09-05T12:01:00Z".into(),
            },
            DomainMessageDraft {
                uid: "r_first".into(),
                conversation: "r_conversation".into(),
                thread: "r_thread".into(),
                author: "r_author".into(),
                operator: "r_author".into(),
                content: "First".into(),
                pinned: true,
                timing: DomainDraftTiming::AfterTurn,
                position: 0,
                created_at: "2026-09-05T12:00:00Z".into(),
            },
        ]);
        assert_eq!(state.conversation_drafts.len(), 2);
        assert_eq!(state.conversation_drafts[0].uid, "r_first");
        assert_eq!(state.conversation_drafts[1].uid, "r_later");
        assert!(state.conversation_drafts[0].pinned);
    }

    #[test]
    fn unpersisted_send_creates_then_sends_the_durable_draft() {
        let mut state = SharedOfficialRuntimeState::new();
        state.selected_conversation = Some("r_conversation".into());
        state.selected_thread = Some("r_thread".into());
        state.conversation_drafts[0].conversation = state.selected_conversation.clone();
        state.conversation_drafts[0].thread = state.selected_thread.clone();
        state.conversation_drafts[0].content = "Persist before send".into();
        state.conversation_drafts[0].mark_dirty();
        state.queue_message(MessageDraftSource::Conversation("native-draft-1".into()));
        let create = single_intent(&mut state);
        assert!(matches!(
            &create,
            OfficialRuntimeIntent::CreateDraft {
                send_after_create: true,
                ..
            }
        ));
        state.domain_action_submitted("create-1".into(), create);
        state.apply_action_receipt(DomainActionReceipt {
            id: "create-1".into(),
            succeeded: true,
            created: Some("r_draft".into()),
            error: None,
        });
        let send = single_intent(&mut state);
        assert!(matches!(
            &send,
            OfficialRuntimeIntent::SendDraft { draft, .. } if draft == "r_draft"
        ));
        state.domain_action_submitted("send-1".into(), send);
        state.apply_action_receipt(DomainActionReceipt {
            id: "send-1".into(),
            succeeded: true,
            created: Some("r_message".into()),
            error: None,
        });
        assert!(state.conversation_drafts.is_empty());
        assert_eq!(state.message_send_requests, 1);
        assert_eq!(state.message_send_acknowledgements, 1);
    }

    #[test]
    fn deferred_timing_is_honest_with_and_without_a_running_turn() {
        let mut state = SharedOfficialRuntimeState::new();
        state.selected_conversation = Some("r_conversation".into());
        state.selected_thread = Some("r_thread".into());
        let draft = &mut state.conversation_drafts[0];
        draft.uid = "r_draft".into();
        draft.content = "Deliver me".into();
        draft.timing = DraftSendTiming::AfterTurn;
        draft.persisted = true;
        let inputs = state.inputs(SharedOfficialRoot::Conversation);
        assert_eq!(
            inputs["send-label"],
            SandValue::Text("send now · no turn running".into())
        );
        state.queue_message(MessageDraftSource::Conversation("r_draft".into()));
        assert!(matches!(
            single_intent(&mut state),
            OfficialRuntimeIntent::SendDraft { .. }
        ));
        state.set_turn_in_flight(true);
        state.queue_message(MessageDraftSource::Conversation("r_draft".into()));
        assert!(state.take_intents().is_empty());
        assert!(state.status_line().contains("after this turn"));
    }

    #[test]
    fn table_repeats_one_row_definition_per_stable_protein_record() {
        let mut state = SharedOfficialRuntimeState::new();
        state.bind_domain_records(
            &[
                DomainRecord {
                    uid: "r/a".into(),
                    title: "Alpha".into(),
                    description: String::new(),
                    quantity: -2.0,
                    kind: "plain".into(),
                    slug: None,
                },
                DomainRecord {
                    uid: "r:b".into(),
                    title: "Beta".into(),
                    description: String::new(),
                    quantity: 1.0,
                    kind: "plain".into(),
                    slug: None,
                },
            ],
            "live Protein subscription".into(),
        );
        state.open("table");
        let scene = state.scene(viewport()).unwrap();
        assert!(scene.nodes.iter().any(|node| {
            node.key == format!("table/rows/{}/title", record_key("r/a")) && node.label == "Alpha"
        }));
        assert!(scene.nodes.iter().any(|node| {
            node.key == format!("table/rows/{}/title", record_key("r:b")) && node.label == "Beta"
        }));
    }

    #[test]
    fn todo_completion_waits_for_action_acknowledgement() {
        let mut state = SharedOfficialRuntimeState::new();
        state.bind_domain_records(
            &[DomainRecord {
                uid: "r_task".into(),
                title: "Task".into(),
                description: "Do it".into(),
                quantity: -1.0,
                kind: "plain".into(),
                slug: None,
            }],
            "live Protein subscription".into(),
        );
        state.open("todo");
        state.activate_suffix("complete/control").unwrap();
        let intent = single_intent(&mut state);
        assert!(matches!(
            &intent,
            OfficialRuntimeIntent::SetRecordQuantity {
                record,
                value: 0.0,
                source: SharedOfficialRoot::Todo,
            } if record == "r_task"
        ));
        assert_eq!(state.workflow_action_acknowledgements, 0);
        state.domain_action_submitted("todo-1".into(), intent);
        state.apply_action_receipt(DomainActionReceipt {
            id: "todo-1".into(),
            succeeded: true,
            created: None,
            error: None,
        });
        assert_eq!(state.workflow_action_acknowledgements, 1);
        assert!(state.status_line().contains("quantity 0"));
    }

    #[test]
    fn todo_open_selected_emits_the_selected_record_identity() {
        let mut state = SharedOfficialRuntimeState::new();
        state.bind_domain_records(
            &[DomainRecord {
                uid: "r_task".into(),
                title: "Task".into(),
                description: "Do it".into(),
                quantity: -1.0,
                kind: "plain".into(),
                slug: None,
            }],
            "live Protein subscription".into(),
        );
        state.selected_workflow_record = Some("r_task".into());
        state.open("todo");
        state.activate_suffix("pager/tertiary/control").unwrap();
        assert_eq!(state.emitted_record_events, 1);
        assert_eq!(state.record_uid, "r_task");
        assert!(state.status_line().contains("record-clicked"));
    }

    #[test]
    fn kanban_move_uses_default_lane_quantity_without_local_optimism() {
        let mut state = SharedOfficialRuntimeState::new();
        state.bind_domain_records(
            &[DomainRecord {
                uid: "r_card".into(),
                title: "Card".into(),
                description: String::new(),
                quantity: -1.0,
                kind: "plain".into(),
                slug: None,
            }],
            "live Protein subscription".into(),
        );
        state.selected_workflow_record = Some("r_card".into());
        state.open("kanban");
        state.activate_suffix("tools/secondary/control").unwrap();
        assert!(matches!(
            single_intent(&mut state),
            OfficialRuntimeIntent::SetRecordQuantity {
                record,
                value: -2.0,
                source: SharedOfficialRoot::Kanban,
            } if record == "r_card"
        ));
        assert_eq!(state.domain_records[0].quantity, -1.0);
    }
}
