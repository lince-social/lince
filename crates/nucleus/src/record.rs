use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageState {
    Writing,
    #[default]
    Finished,
    Interrupted,
}

impl MessageState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Writing => "writing",
            Self::Finished => "finished",
            Self::Interrupted => "interrupted",
        }
    }

    pub fn is_finished(&self) -> bool {
        *self == Self::Finished
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageDraftTiming {
    #[default]
    Now,
    NextSafePoint,
    AfterTurn,
}

impl MessageDraftTiming {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Now => "now",
            Self::NextSafePoint => "next_safe_point",
            Self::AfterTurn => "after_turn",
        }
    }
}

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
    Conversation,
    Thread,
    Message,
    MessageDraft,
    ThreadInvite,
    Program,
    Frequency,
    Grant,
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
            Self::MessageDraft => "message_draft",
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
            "message_draft" => Self::MessageDraft,
            "thread_invite" => Self::ThreadInvite,
            "program" => Self::Program,
            "frequency" => Self::Frequency,
            "grant" => Self::Grant,
            "call_session" => Self::CallSession,
            _ => return None,
        })
    }
}
