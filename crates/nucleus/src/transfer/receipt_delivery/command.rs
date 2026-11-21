//! Delivery commands, permissions, and referenced records.

use std::iter::once;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::karma::CanonicalHash;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Source {
    Document {
        record: String,
        expected_version: u64,
    },
    Participation {
        transfer: String,
        expected_revision: u64,
        party: Option<String>,
    },
}

impl Source {
    pub fn records(&self) -> Vec<&String> {
        self.referenced_records().collect()
    }

    fn referenced_records(&self) -> impl Iterator<Item = &String> {
        let (record, party) = match self {
            Self::Document { record, .. } => (record, None),
            Self::Participation {
                transfer, party, ..
            } => (transfer, party.as_ref()),
        };
        once(record).chain(party)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub source: Source,
    pub thread: String,
    pub recipients: Vec<String>,
    pub expires_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Inbox {
        #[serde(default)]
        before: Option<String>,
    },
    Options {
        source: Source,
    },
    Preview {
        request: Request,
    },
    Send {
        request: Request,
        reviewed_at: i64,
        expected_projection: CanonicalHash,
    },
    Inspect {
        message: String,
    },
    Import {
        message: String,
        expected_projection: CanonicalHash,
    },
    Cancel {
        message: String,
    },
    Retry {
        message: String,
    },
}

impl Command {
    pub fn permission(&self) -> &'static str {
        match self {
            Self::Inbox { .. }
            | Self::Options { .. }
            | Self::Preview { .. }
            | Self::Inspect { .. } => "record:read",
            Self::Send { .. } | Self::Import { .. } => "record:create",
            Self::Cancel { .. } | Self::Retry { .. } => "record:update",
        }
    }
    pub fn is_mutation(&self) -> bool {
        self.permission() != "record:read"
    }
    pub fn records(&self) -> Vec<&String> {
        match self {
            Self::Inbox { .. } => vec![],
            Self::Options { source } => source.records(),
            Self::Preview { request } | Self::Send { request, .. } => once(&request.thread)
                .chain(request.source.referenced_records())
                .collect(),
            Self::Inspect { message }
            | Self::Import { message, .. }
            | Self::Cancel { message }
            | Self::Retry { message } => vec![message],
        }
    }
}
