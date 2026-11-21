// non-lean-yet
// Scope: struct WorkspaceSync, enum Content, enum Destination, struct Capabilities.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::indexing_slicing] Verify bounds or use checked access on fallible inputs.
//   Sites 220.
// - [Clippy: clippy::as_conversions] Use From or TryFrom when it preserves the conversion contract.
//   Sites 146.
// - [Clippy: clippy::exhaustive_enums] Review whether public API evolution requires non_exhaustive.
//   Sites 10, 18, 102, 110, 125.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 4, 5,
//   10, 11, 12, 13, 18, 19, 20, 21, 22, 26, 27, 28, 29, 33, 34, 35, 36, 40, 54, 68, 83, 102, 103,
//   104, 105, 110, 111, 112, 113, 117, 118, 119, 120, 125, 126, 127, 128, 129, 130, 131, 135, 136,
//   137, 138, 139, 143, 154, 155, 156, 157, 158, 162, 163, 164, 165, 179, 185, 186, 187, 188, 189,
//   190, 191, 192, 193, 197, 198, 199, 200, 201, 202, 203, 204, 205, 206, 210, 211, 212, 213, 215,
//   217.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceSync {
    pub workspace_uid: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Content {
    Records { root: Option<String> },
    Query { subscription: String },
    Workspace(WorkspaceSync),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Destination {
    Plugin { provider: String, binding: String },
    Directory { organ: String, path: String },
    Organ { uid: String },
    Interface { connection: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    pub incremental: bool,
    pub durable_queue: bool,
    pub review: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Instance {
    pub content: Content,
    pub destination: Destination,
    pub capabilities: Capabilities,
}

impl Instance {
    pub fn plugin(provider: &str, binding: &str) -> Self {
        Self {
            content: Content::Records { root: None },
            destination: Destination::Plugin {
                provider: provider.into(),
                binding: binding.into(),
            },
            capabilities: Capabilities {
                incremental: true,
                durable_queue: true,
                review: true,
            },
        }
    }
    pub fn peer(organ: &str, root: Option<&str>) -> Self {
        Self {
            content: Content::Records {
                root: root.map(str::to_owned),
            },
            destination: Destination::Organ { uid: organ.into() },
            capabilities: Capabilities {
                incremental: true,
                durable_queue: true,
                review: true,
            },
        }
    }

    pub fn files(organ: &str, path: &std::path::Path) -> Self {
        Self {
            content: Content::Records { root: None },
            destination: Destination::Directory {
                organ: organ.into(),
                path: path.display().to_string(),
            },
            capabilities: Capabilities {
                incremental: false,
                durable_queue: false,
                review: true,
            },
        }
    }

    pub fn interface(connection: &str, subscription: &str) -> Self {
        Self {
            content: Content::Query {
                subscription: subscription.into(),
            },
            destination: Destination::Interface {
                connection: connection.into(),
            },
            capabilities: Capabilities {
                incremental: false,
                durable_queue: false,
                review: false,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Incoming,
    Outgoing,
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Update {
    Full,
    Incremental,
    Reconcile,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Activity {
    pub instance: Instance,
    pub direction: Direction,
    pub update: Update,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Applied,
    Delivered,
    Refreshed,
    Pending,
    Conflict,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Summary {
    pub outcome: Outcome,
    pub count: u64,
    pub subjects: Vec<String>,
    pub message: Option<String>,
}

impl Summary {
    pub fn new(outcome: Outcome, count: usize) -> Self {
        Self {
            outcome,
            count: count as u64,
            subjects: Vec::new(),
            message: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Change {
    pub seq: i64,
    pub at: i64,
    pub activity: Activity,
    pub summary: Summary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Retention {
    pub enabled: bool,
    pub seconds: u64,
    pub max_entries: u32,
}

impl Default for Retention {
    fn default() -> Self {
        Self {
            enabled: false,
            seconds: 7 * 24 * 60 * 60,
            max_entries: 1000,
        }
    }
}

impl Retention {
    pub fn valid(self) -> bool {
        (60..=90 * 24 * 60 * 60).contains(&self.seconds) && (1..=10_000).contains(&self.max_entries)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pending {
    pub id: String,
    pub instance: Instance,
    pub direction: Direction,
    pub outcome: Outcome,
    pub subject: Option<String>,
    pub field: Option<String>,
    pub attempts: u64,
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Overview {
    pub active: Vec<Activity>,
    pub queues: Vec<Queue>,
    pub history: Vec<Change>,
    pub pending: Vec<Pending>,
    pub outgoing: u64,
    pub incoming: u64,
    pub held: u64,
    pub retention: Retention,
    pub history_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Queue {
    pub instance: Instance,
    pub incoming: u64,
    pub outgoing: u64,
}
pub const PERSONAL_FILE_ORIGIN: &str = "personal";

pub fn valid_file_origin(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.as_bytes()[0].is_ascii_lowercase()
        && value
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
}
