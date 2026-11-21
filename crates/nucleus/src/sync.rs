//! Types shared by file, peer, plugin, and live interface synchronization.
//!
//! [Instance] describes a link. [Activity] and [Summary] describe one operation,
//! and [Overview] collects the active operations, pending work, and retained history.

use std::path::Path;

use serde::{Deserialize, Serialize};

const SECONDS_PER_DAY: u64 = 86_400;
const MIN_RETENTION_SECONDS: u64 = 60;
const MAX_RETENTION_SECONDS: u64 = 90 * SECONDS_PER_DAY;
const MAX_HISTORY_ENTRIES: u32 = 10_000;
const MAX_FILE_ORIGIN_BYTES: usize = 64;

/// Identifies a synchronized workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceSync {
    pub workspace_uid: String,
}

/// Selects the content flowing through a sync instance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Content {
    Records { root: Option<String> },
    Query { subscription: String },
    Workspace(WorkspaceSync),
}

/// Identifies the other endpoint of a sync instance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Destination {
    Plugin { provider: String, binding: String },
    Directory { organ: String, path: String },
    Organ { uid: String },
    Interface { connection: String },
}

/// Independent capabilities of the sync link.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    /// Supports partial updates on this link.
    pub incremental: bool,
    /// Keeps queued work across restarts.
    pub durable_queue: bool,
    /// Supports review of changes on this link.
    pub review: bool,
}

/// The content, destination and capabilities of one sync link.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Instance {
    pub content: Content,
    pub destination: Destination,
    pub capabilities: Capabilities,
}

impl Capabilities {
    const QUEUED_REVIEW: Self = Self {
        incremental: true,
        durable_queue: true,
        review: true,
    };
}

impl Instance {
    /// Describes a plugin binding.
    pub fn plugin(provider: &str, binding: &str) -> Self {
        Self {
            content: Content::Records { root: None },
            destination: Destination::Plugin {
                provider: provider.into(),
                binding: binding.into(),
            },
            capabilities: Capabilities::QUEUED_REVIEW,
        }
    }

    /// Describes a peer link for all records or a selected root.
    pub fn peer(organ: &str, root: Option<&str>) -> Self {
        Self {
            content: Content::Records {
                root: root.map(str::to_owned),
            },
            destination: Destination::Organ { uid: organ.into() },
            capabilities: Capabilities::QUEUED_REVIEW,
        }
    }

    /// Describes a directory link, storing the path in its display form.
    pub fn files(organ: &str, path: &Path) -> Self {
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

    /// Describes a live query subscription.
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

/// The direction of a sync operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Incoming,
    Outgoing,
    Both,
}

/// The scope of a sync operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Update {
    Full,
    Incremental,
    Reconcile,
}

/// The link, direction and scope of a sync operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Activity {
    pub instance: Instance,
    pub direction: Direction,
    pub update: Update,
}

/// The reported result of a sync operation.
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

/// The result and affected subjects of a sync operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Summary {
    pub outcome: Outcome,
    /// Number of affected items reported by the operation.
    pub count: u64,
    pub subjects: Vec<String>,
    pub message: Option<String>,
}

impl Summary {
    /// Creates a summary before any affected subjects or message are supplied.
    pub fn new(outcome: Outcome, count: usize) -> Self {
        #[expect(
            clippy::as_conversions,
            reason = "supported pointer widths fit in the persisted u64 count"
        )]
        let count = count as u64;
        Self {
            outcome,
            count,
            subjects: Vec::new(),
            message: None,
        }
    }
}

/// A retained sync activity and its result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Change {
    /// Sequence number used to page retained history.
    pub seq: i64,
    /// Unix timestamp in seconds.
    pub at: i64,
    pub activity: Activity,
    pub summary: Summary,
}

/// The age and count limits for retained sync history.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Retention {
    /// Enables recording new history.
    pub enabled: bool,
    /// Maximum age of a retained entry in seconds.
    pub seconds: u64,
    /// Maximum number of retained entries.
    pub max_entries: u32,
}

impl Default for Retention {
    fn default() -> Self {
        Self {
            enabled: false,
            seconds: 7 * SECONDS_PER_DAY,
            max_entries: 1_000,
        }
    }
}

impl Retention {
    /// Checks the supported age and count limits, including for disabled history.
    pub fn valid(self) -> bool {
        (MIN_RETENTION_SECONDS..=MAX_RETENTION_SECONDS).contains(&self.seconds)
            && (1..=MAX_HISTORY_ENTRIES).contains(&self.max_entries)
    }
}

/// An outstanding operation or held conflict.
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

/// A snapshot of current sync activity and history.
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

/// Incoming and outgoing counts for one sync link.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Queue {
    pub instance: Instance,
    pub incoming: u64,
    pub outgoing: u64,
}

/// File origin reserved for the owner's personal directory.
pub const PERSONAL_FILE_ORIGIN: &str = "personal";

/// Checks file origin syntax. The first byte must be a lowercase ASCII letter.
pub fn valid_file_origin(value: &str) -> bool {
    let mut bytes = value.bytes();
    value.len() <= MAX_FILE_ORIGIN_BYTES
        && bytes.next().is_some_and(|first| first.is_ascii_lowercase())
        && bytes.all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

#[cfg(test)]
mod tests {
    use super::{MAX_FILE_ORIGIN_BYTES, PERSONAL_FILE_ORIGIN, valid_file_origin};

    #[test]
    fn file_origins_preserve_ascii_syntax_and_the_byte_limit() {
        assert!(valid_file_origin(PERSONAL_FILE_ORIGIN));
        assert!(valid_file_origin("folder-42"));
        assert!(valid_file_origin(&"a".repeat(MAX_FILE_ORIGIN_BYTES)));
        for origin in ["", "Folder", "2folders", "-folder", "a_b", "équipe"] {
            assert!(!valid_file_origin(origin));
        }
        assert!(!valid_file_origin(&"a".repeat(MAX_FILE_ORIGIN_BYTES + 1)));
    }
}
