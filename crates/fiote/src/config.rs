//! Configuration commands and status snapshots exchanged with Fiote's host.

mod settings;

use std::{collections::BTreeMap, path::PathBuf};

use nucleus::operation::Usage;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{acp, adapters, assistant, connection};

pub use settings::{ProviderKind, Secret, Settings};

/// An operation sent to the Fiote configuration service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Request {
    Connections {
        record: String,
        request: connection::Request,
    },
    Activate {
        record: String,
        value: String,
        request_id: String,
    },
    CancelActivations {
        record: String,
    },
    SessionReset {
        thread: String,
    },
    AgentLogout {
        record: String,
    },
    Directory,
    AssistantDirectory {
        record: String,
        offset: usize,
        limit: usize,
    },
    AssistantSend {
        request: assistant::EntryRequest,
    },
    AssistantContinue {
        conversation: assistant::Conversation,
        request: assistant::EntryRequest,
    },
    AssistantInspect {
        request_id: String,
    },
    Delete {
        record: String,
    },
    Create {
        head: String,
    },
    InspectThread {
        thread: String,
    },
    Behavior {
        record: String,
        prompt_parent: Option<String>,
        run_assigned: bool,
    },
    WorkingDirectory {
        record: String,
        directory: PathBuf,
    },
    RefreshInstructions {
        record: String,
        thread: String,
    },
    RetryAssignment {
        record: String,
        thread: String,
    },
    AgentConfigure {
        record: String,
        config: acp::Config,
    },
    AgentOptions {
        record: String,
        config: acp::Config,
    },
    AgentSetOption {
        record: String,
        option: String,
        value: Value,
    },
    SessionOptions {
        thread: String,
    },
    SessionDirectories {
        thread: String,
        directories: Vec<PathBuf>,
    },
    SessionSetOption {
        thread: String,
        option: String,
        value: Value,
    },
    AgentDiscover {
        record: String,
        config: acp::Config,
    },
    AgentCheck {
        record: String,
        config: acp::Config,
    },
    AgentCancelLogin {
        record: String,
    },
    AgentAuthenticate {
        record: String,
        method: String,
    },
    AgentQuestionAnswer {
        record: String,
        request: String,
        answer: acp::Answer,
    },
    AgentPermission {
        record: String,
        thread: String,
        request: String,
        option: Option<String>,
    },
    OpenTools {
        record: String,
        thread: String,
    },
    CloseTools {
        record: String,
        thread: String,
    },
    Prepare {
        record: String,
    },
    BrowserStart {
        record: String,
        settings: Settings,
    },
    BrowserPoll {
        record: String,
    },
    BrowserCancel {
        record: String,
    },
    Inspect {
        record: String,
    },
    InspectDashboard {
        record: String,
    },
    InspectToolHistory {
        thread: String,
        message: String,
    },
    Configure {
        record: String,
        settings: Settings,
        api_key: Option<Secret>,
    },
    Stop {
        thread: String,
    },
    StopChild {
        record: String,
        thread: String,
    },
}

/// Fiote's settings, connections, and running activity as reported to the host.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Status {
    #[serde(default)]
    pub assistant_reply: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_history: Option<String>,
    pub assistant_directory: Option<assistant::Directory>,
    pub assistant_request: Option<assistant::EntryReceipt>,
    #[serde(default)]
    pub intake: Option<Intake>,
    #[serde(default)]
    pub handoffs: Vec<assistant::Handoff>,
    #[serde(default)]
    pub working_directory: WorkingDirectory,
    #[serde(default)]
    pub connections: connection::Status,
    #[serde(default)]
    pub activations: Vec<Value>,
    #[serde(default)]
    pub questions: Vec<PrivateQuestion>,
    #[serde(default)]
    pub usage: Vec<Usage>,
    pub agent_session: Option<AgentSession>,
    pub session: Option<PromptSession>,
    pub behavior: Behavior,
    pub instructions: Vec<PromptSource>,
    pub instruction_error: Option<String>,
    pub fiotes: Vec<FioteChoice>,
    pub tasks: Vec<TaskSession>,
    pub agent: Option<acp::Config>,
    pub agent_info: Option<Value>,
    pub agent_activity: Vec<AgentActivity>,
    pub record: String,
    pub settings: Settings,
    pub has_key: bool,
    pub running: Vec<String>,
    pub login_url: Option<String>,
    pub login_pending: bool,
    pub providers: Vec<adapters::Descriptor>,
    pub requires_credential: bool,
    #[serde(default)]
    pub provider_diagnostics: Vec<String>,
    pub tool_connections: Vec<ToolConnection>,
}

/// The configured and resolved directory, including any resolution error.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkingDirectory {
    pub configured: PathBuf,
    pub effective: Option<PathBuf>,
    pub source: Option<String>,
    pub error: Option<String>,
}

/// Session state and options negotiated with an ACP agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSession {
    #[serde(default)]
    pub prompt_capabilities: Value,
    pub directories: Vec<PathBuf>,
    pub supports_directories: bool,
    pub thread: String,
    pub state: acp::SessionState,
    pub pending: BTreeMap<String, Value>,
    pub connected: bool,
}

/// Activity and any outstanding permission request for a thread.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentActivity {
    pub thread: String,
    pub title: String,
    pub permission: Option<acp::Permission>,
}

/// Tool endpoint credentials scoped to one thread.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolConnection {
    pub thread: String,
    pub url: String,
    pub token: Secret,
}

/// Inherited prompt configuration and assigned-work behavior.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Behavior {
    pub prompt_parent: Option<String>,
    pub run_assigned: bool,
}

/// A specific revision of a Record included in the session prompt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptSource {
    pub record: String,
    pub title: String,
    pub revision: String,
    pub body: String,
}

/// A Fiote offered for selection by the host.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FioteChoice {
    pub record: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dashboard: Option<DashboardState>,
}

/// Connection and activity shown in the local Fiote roster.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DashboardState {
    pub ready: bool,
    pub model: Option<String>,
    pub working: bool,
}

/// The designated intake Fiote and the state of its routing queue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Intake {
    pub record: String,
    pub title: String,
    pub children: usize,
    pub created_at: String,
    pub enabled: bool,
}

/// The progress of assigned work in a particular thread.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSession {
    pub thread: String,
    pub task: String,
    pub title: String,
    pub state: String,
    pub detail: String,
}

/// The sources pinned to a session and its optional continuation context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptSession {
    pub thread: String,
    pub sources: Vec<PromptSource>,
    pub changed: bool,
    #[serde(default)]
    pub context: Option<Value>,
}

/// An ACP question addressed to the owner, optionally scoped to a thread.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivateQuestion {
    pub thread: Option<String>,
    pub request: acp::QuestionRequest,
}
