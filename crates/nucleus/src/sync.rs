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
    pub seconds: u64,
    pub max_entries: u32,
}

impl Default for Retention {
    fn default() -> Self {
        Self {
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
