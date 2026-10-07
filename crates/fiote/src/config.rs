use serde::{Deserialize, Serialize};
use std::path::PathBuf;
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
                return Err("Choose an existing folder using its full path, or leave it empty for native Lince tools only.".into());
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
    AgentProvider {
        record: String,
        provider: String,
    },
    AgentProviderLogin {
        record: String,
        fields: Secret,
        password: Option<Secret>,
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
    Unlock {
        record: String,
        password: Secret,
    },
    Lock {
        record: String,
    },
    BrowserStart {
        record: String,
        password: Secret,
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
        password: Option<Secret>,
    },
    Stop {
        thread: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Status {
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
    pub vault_exists: bool,
    pub locked: bool,
    pub login_url: Option<String>,
    pub login_pending: bool,
    pub providers: Vec<crate::adapters::Descriptor>,
    pub requires_credential: bool,
    #[serde(default)]
    pub provider_diagnostics: Vec<String>,
    pub tool_connections: Vec<ToolConnection>,
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
