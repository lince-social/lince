//! Record kinds (blueprint Part I). Everything is a Record; `kind` selects the
//! sidecar table. `quantity` is the universal activation knob on non-plain kinds.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordKind {
    Plain,
    Rule,
    Signal,
    Transfer,
    Decision,
    Device,
    Organ,
    Person,
    Protein,
    Sand,
    /// One relationship, one grant (Ontology §11 "Threads"): the
    /// individually-replicated root that Threads and Messages live inside.
    /// Sharing one is granting exactly one contact sync access to it.
    Conversation,
    Thread,
    Message,
    /// A request to open a conversation, before anyone has agreed to one.
    ///
    /// Deliberately NOT a Conversation: an invite is what an Organ you may not
    /// know can put in front of you, so it must be a thing you can decline
    /// without ever having held a copy of anything. One pending per Organ, so
    /// a declined conversation cannot become a spam channel.
    ThreadInvite,
    Program,
    Frequency,
    Grant,
    /// One occupancy of a conversation's audio/video room (Communication
    /// sand). A child Record linked `call-session-of` → conversation; carries
    /// the `communication.session.v1` sidecar. See `notes/institute/Communication.md`.
    CallSession,
}

impl RecordKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Plain => "plain",
            Self::Rule => "rule",
            Self::Signal => "signal",
            Self::Transfer => "transfer",
            Self::Decision => "decision",
            Self::Device => "device",
            Self::Organ => "organ",
            Self::Person => "person",
            Self::Protein => "protein",
            Self::Sand => "sand",
            Self::Conversation => "conversation",
            Self::Thread => "thread",
            Self::Message => "message",
            Self::ThreadInvite => "thread_invite",
            Self::Program => "program",
            Self::Frequency => "frequency",
            Self::Grant => "grant",
            Self::CallSession => "call_session",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "plain" => Self::Plain,
            "rule" => Self::Rule,
            "signal" => Self::Signal,
            "transfer" => Self::Transfer,
            "decision" => Self::Decision,
            "device" => Self::Device,
            "organ" => Self::Organ,
            "person" => Self::Person,
            "protein" => Self::Protein,
            "sand" => Self::Sand,
            "conversation" => Self::Conversation,
            "thread" => Self::Thread,
            "message" => Self::Message,
            "thread_invite" => Self::ThreadInvite,
            "program" => Self::Program,
            "frequency" => Self::Frequency,
            "grant" => Self::Grant,
            "call_session" => Self::CallSession,
            _ => return None,
        })
    }
}
