use crate::config::Intake;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Delegate {
    pub request_id: String,
    pub target: String,
    #[serde(default)]
    pub request: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    pub fiote: String,
    pub author: String,
    pub thread: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candidate {
    pub record: String,
    pub title: String,
    pub description: String,
    pub description_truncated: bool,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Directory {
    pub fiotes: Vec<Candidate>,
    pub next_offset: Option<usize>,
    pub intake: Option<Intake>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Handoff {
    pub source: Source,
    pub request_id: String,
    pub target: String,
    pub fingerprint: String,
    pub chain: Vec<String>,
    pub thread: Option<String>,
    pub message: Option<String>,
    pub reply: Option<String>,
    pub state: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntryRequest {
    pub request_id: String,
    pub target: Option<String>,
    pub text: String,
    #[serde(default)]
    pub content: Vec<nucleus::message::MessagePart>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryReceipt {
    pub request_id: String,
    pub fingerprint: String,
    pub target: String,
    pub thread: Option<String>,
    pub message: Option<String>,
    pub reply: Option<String>,
    pub state: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Conversation {
    pub target: String,
    pub thread: String,
}
