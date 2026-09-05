use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DomainRecord {
    pub uid: String,
    pub title: String,
    pub description: String,
    pub quantity: f64,
    pub kind: String,
    pub slug: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DomainMessageState {
    Writing,
    Finished,
    Interrupted,
}

impl DomainMessageState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Writing => "writing",
            Self::Finished => "finished",
            Self::Interrupted => "interrupted",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DomainMessage {
    pub uid: String,
    pub author: String,
    pub operator: String,
    pub content: String,
    pub state: DomainMessageState,
    pub created_at: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DomainThread {
    pub uid: String,
    pub title: String,
    pub messages: Vec<DomainMessage>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DomainConversation {
    pub uid: String,
    pub title: String,
    pub threads: Vec<DomainThread>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DomainDraftTiming {
    Now,
    NextSafePoint,
    AfterTurn,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DomainMessageDraft {
    pub uid: String,
    pub conversation: String,
    pub thread: String,
    pub author: String,
    pub operator: String,
    pub content: String,
    pub pinned: bool,
    pub timing: DomainDraftTiming,
    pub position: u32,
    pub created_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DomainActionReceipt {
    pub id: String,
    pub succeeded: bool,
    pub created: Option<String>,
    pub error: Option<String>,
}
