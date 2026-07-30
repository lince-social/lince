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
    Thread,
    Message,
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
            Self::Thread => "thread",
            Self::Message => "message",
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
            "thread" => Self::Thread,
            "message" => Self::Message,
            "program" => Self::Program,
            "frequency" => Self::Frequency,
            "grant" => Self::Grant,
            "call_session" => Self::CallSession,
            _ => return None,
        })
    }
}
