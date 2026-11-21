use serde::{Deserialize, Serialize};
use sqlx::FromRow;
/// Operation kinds stored and transmitted by the sync log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpKind {
    Set,
    Tombstone,
    Fact,
    Crdt,
    Snapshot,
}

impl OpKind {
    /// Returns the stable stored and wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Set => "set",
            Self::Tombstone => "tombstone",
            Self::Fact => "fact",
            Self::Crdt => "crdt",
            Self::Snapshot => "snapshot",
        }
    }

    /// Recognizes a stored or wire operation kind.
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "set" => Self::Set,
            "tombstone" => Self::Tombstone,
            "fact" => Self::Fact,
            "crdt" => Self::Crdt,
            "snapshot" => Self::Snapshot,
            _ => return None,
        })
    }
}

/// A stored operation with its local sequence and wire identity.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct OpRow {
    pub seq: i64,
    pub tbl: String,
    pub uid: String,
    pub field: String,
    pub kind: String,
    pub value: Option<String>,
    pub hlc: i64,
    pub actor_cell: String,
    pub organ_uid: String,
    pub replica_root: Option<String>,
}

/// The maximum covered hybrid clock for one sync stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, FromRow)]
pub struct VectorEntry {
    pub actor_cell: String,
    pub max_hlc: i64,
}
