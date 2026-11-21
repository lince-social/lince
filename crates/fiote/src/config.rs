// non-lean-yet
// Scope: struct Secret, struct ProviderKind, struct Settings, enum Request.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::exhaustive_enums] Review whether public API evolution requires non_exhaustive.
//   Sites 99.
// - [Clippy: clippy::manual_range_contains] manual `!RangeInclusive::contains` implementation.
//   Sites 48.
// - [Clippy: clippy::map_err_ignore] Preserve relevant causes and context at the boundary. Sites
//   69.
// - [Clippy: clippy::missing_errors_doc] State the observable failure cases for exported functions.
//   Sites 45.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 7, 23,
//   26, 27, 28, 30, 31, 32, 33, 35, 37, 39, 41, 45, 99, 100, 101, 102, 104, 105, 106, 107, 109,
//   110, 112, 113, 115, 116, 118, 119, 120, 121, 122, 124, 125, 127, 128, 129, 131, 132, 134, 135,
//   137, 138, 140, 141, 143, 144, 145, 146, 148, 149, 150, 152, 153, 154, 156, 157, 158, 160, 161,
//   162, 164, 165, 166, 168, 169, 170, 171, 173, 174, 176, 177, 178, 180, 181, 182, 183, 185, 186,
//   187, 189, 190, 191, 193, 194, 196, 197, 198, 200, 201, 202, 203, 205, 206, 207, 208, 209, 211,
//   212, 213, 215, 216, 217, 219, 220, 222, 223, 224, 226, 227, 229, 230, 232, 233, 235, 236, 237,
//   238, 240, 241, 243, 244, 245, 250, 252, 253, 254, 256, 258, 260, 262, 264, 266, 268, 269, 270,
//   271, 272, 273, 274, 275, 276, 277, 278, 279, 280, 281, 282, 283, 284, 285, 286, 288, 289, 293,
//   294, 295, 296, 297, 301, 303, 304, 305, 306, 307, 308, 309, 313, 314, 315, 316, 320, 321, 322,
//   323, 327, 328, 329, 333, 334, 335, 336, 337, 341, 342, 343, 347, 348, 349, 350, 351, 352, 356,
//   357, 358, 359, 360, 361, 365, 366, 367, 368, 370, 374, 375, 376.
// - [review: §5 Errors] If exported, use a typed error with preserved causes. Sites validate:
//   String (45).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Secret(pub String);

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret([redacted])")
    }
}

impl Drop for Secret {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProviderKind(pub String);

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Settings {
    pub enabled: bool,
    pub provider: ProviderKind,
    #[serde(default)]
    pub auth_method: String,
    pub model: String,
    pub endpoint: String,
    pub directory: PathBuf,
    #[serde(default)]
    pub account: String,
    #[serde(default)]
    pub reasoning: Option<String>,
    #[serde(default)]
    pub fast: bool,
    #[serde(default)]
    pub context_budget_bytes: Option<usize>,
}

impl Settings {
    pub fn validate(&mut self) -> Result<(), String> {
        if self
            .context_budget_bytes
            .is_some_and(|budget| budget < 8192 || budget > crate::runtime::MAX_CONTEXT_BYTES)
        {
            return Err("Choose a context budget between 8 KiB and 512 KiB.".into());
        }
        if self.account.len() > 160 || self.account.chars().any(char::is_control) {
            return Err("Invalid native account identifier.".into());
        }
        if self.reasoning.as_deref().is_some_and(|value| {
            !matches!(
                value,
                "none" | "minimal" | "low" | "medium" | "high" | "xhigh"
            )
        }) {
            return Err("Choose an available reasoning effort.".into());
        }
        self.model = self.model.trim().to_string();
        if self.model.len() > 256 {
            return Err("Choose a model name (at most 256 bytes).".into());
        }
        if !self.endpoint.trim().is_empty() {
            let mut url = url::Url::parse(self.endpoint.trim())
                .map_err(|_| "The provider endpoint must be a URL.")?;
            if !url.username().is_empty()
                || url.password().is_some()
                || url.query().is_some()
                || url.fragment().is_some()
            {
                return Err("Keep credentials in the API key field, not in the endpoint.".into());
            }
            let local = url
                .host_str()
                .is_some_and(|host| matches!(host, "localhost" | "127.0.0.1" | "[::1]"));
            if url.scheme() != "https" && !(url.scheme() == "http" && local) {
                return Err("Use HTTPS, or HTTP for a provider on localhost.".into());
            }
            if !url.path().ends_with('/') {
                url.set_path(&format!("{}/", url.path()));
            }
            self.endpoint = url.to_string();
        }
        if !self.directory.as_os_str().is_empty() {
            if !self.directory.is_absolute() || !self.directory.is_dir() {
                return Err("Choose an existing working directory using its full path, or leave it empty to inherit from the Fiote parent.".into());
            }
            self.directory = self.directory.canonicalize().map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Request {
    Connections {
        record: String,
        request: crate::connection::Request,
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
        request: crate::assistant::EntryRequest,
    },
    AssistantContinue {
        conversation: crate::assistant::Conversation,
        request: crate::assistant::EntryRequest,
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
        config: crate::acp::Config,
    },
    AgentOptions {
        record: String,
        config: crate::acp::Config,
    },
    AgentSetOption {
        record: String,
        option: String,
        value: serde_json::Value,
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
        value: serde_json::Value,
    },
    AgentDiscover {
        record: String,
        config: crate::acp::Config,
    },
    AgentCheck {
        record: String,
        config: crate::acp::Config,
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
        answer: crate::acp::Answer,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Status {
    #[serde(default)]
    pub assistant_reply: Option<String>,
    pub assistant_directory: Option<crate::assistant::Directory>,
    pub assistant_request: Option<crate::assistant::EntryReceipt>,
    #[serde(default)]
    pub intake: Option<Intake>,
    #[serde(default)]
    pub handoffs: Vec<crate::assistant::Handoff>,
    #[serde(default)]
    pub working_directory: WorkingDirectory,
    #[serde(default)]
    pub connections: crate::connection::Status,
    #[serde(default)]
    pub activations: Vec<serde_json::Value>,
    #[serde(default)]
    pub questions: Vec<PrivateQuestion>,
    #[serde(default)]
    pub usage: Vec<nucleus::operation::Usage>,
    pub agent_session: Option<AgentSession>,
    pub session: Option<PromptSession>,
    pub behavior: Behavior,
    pub instructions: Vec<PromptSource>,
    pub instruction_error: Option<String>,
    pub fiotes: Vec<FioteChoice>,
    pub tasks: Vec<TaskSession>,
    pub agent: Option<crate::acp::Config>,
    pub agent_info: Option<serde_json::Value>,
    pub agent_activity: Vec<AgentActivity>,
    pub record: String,
    pub settings: Settings,
    pub has_key: bool,
    pub running: Vec<String>,
    pub login_url: Option<String>,
    pub login_pending: bool,
    pub providers: Vec<crate::adapters::Descriptor>,
    pub requires_credential: bool,
    #[serde(default)]
    pub provider_diagnostics: Vec<String>,
    pub tool_connections: Vec<ToolConnection>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkingDirectory {
    pub configured: PathBuf,
    pub effective: Option<PathBuf>,
    pub source: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSession {
    #[serde(default)]
    pub prompt_capabilities: serde_json::Value,
    pub directories: Vec<PathBuf>,
    pub supports_directories: bool,
    pub thread: String,
    pub state: crate::acp::SessionState,
    pub pending: std::collections::BTreeMap<String, serde_json::Value>,
    pub connected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentActivity {
    pub thread: String,
    pub title: String,
    pub permission: Option<crate::acp::Permission>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolConnection {
    pub thread: String,
    pub url: String,
    pub token: Secret,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Behavior {
    pub prompt_parent: Option<String>,
    pub run_assigned: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptSource {
    pub record: String,
    pub title: String,
    pub revision: String,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FioteChoice {
    pub record: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Intake {
    pub record: String,
    pub title: String,
    pub children: usize,
    pub created_at: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSession {
    pub thread: String,
    pub task: String,
    pub title: String,
    pub state: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptSession {
    pub thread: String,
    pub sources: Vec<PromptSource>,
    pub changed: bool,
    #[serde(default)]
    pub context: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivateQuestion {
    pub thread: Option<String>,
    pub request: crate::acp::QuestionRequest,
}
