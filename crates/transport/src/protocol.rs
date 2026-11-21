// non-lean-yet
// Scope: enum ClientMessage, enum ServerMessage.
// BeautifulRust (baseline lines):
// - [Clippy probe: clippy::exhaustive_enums] Review whether public API evolution requires
//   non_exhaustive. Sites 21, 144.
// - [Clippy probe: clippy::large_enum_variant] large size difference between variants. Sites 21,
//   144.
// - [rustc probe: missing_docs] Document the externally visible contracts where applicable. Sites
//   10, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 34, 35, 36, 38, 39, 40, 42, 43, 44, 46, 47,
//   49, 50, 51, 53, 54, 55, 57, 58, 60, 61, 62, 64, 65, 66, 67, 68, 69, 70, 71, 73, 74, 75, 76, 77,
//   78, 79, 81, 82, 84, 85, 87, 88, 89, 91, 93, 94, 95, 97, 98, 100, 101, 102, 103, 104, 106, 107,
//   108, 109, 111, 112, 113, 114, 116, 118, 120, 121, 122, 124, 125, 126, 127, 129, 131, 133, 134,
//   136, 137, 138, 144, 145, 146, 147, 148, 149, 150, 151, 152, 153, 154, 155, 157, 158, 159, 161,
//   162, 163, 164, 165, 166, 168, 169, 170, 171, 172, 174, 175, 176, 178, 179, 180, 182, 183, 184,
//   185, 187, 189, 191, 192, 193, 195, 197, 198, 199, 200, 201, 202, 204, 205, 206, 207, 209, 210,
//   211, 213, 214, 215, 217, 218, 219, 220, 222, 224, 226, 227, 229, 230, 232, 234, 235, 237, 238,
//   240, 241, 242, 243, 245, 246, 247, 249, 250, 251.

use engine::actions::Action;
use protein::Protein;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

pub use engine::presence::Cursor as CollabCursor;

impl ClientMessage {
    pub fn runs_karma_preview(&self) -> bool {
        match self {
            Self::Act { action: Action::PreviewKarmaProposal { .. }, .. } => true,
            Self::SignedAct { action_base64, .. } => engine::action_intent::payload_requests_karma_preview(action_base64),
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    WorkspaceSubscribe { id: String, workspace: String, client: engine::workspace_sync::Client },
    DescriptionAsset { id: String, request: nucleus::description_asset::Request },
    Speech { id: String, request: crate::speech::Request },
    FioteTerminal { id: String, request: fiote::acp::terminal::TerminalRequest },
    Command { id: String, request: crate::command::Request },
    CallContext { id: String, thread: String },
    Call {
        id: String,
        thread: String,
        person: Option<String>,
        operation: engine::calls::Operation,
    },
    Fiote {
        id: String,
        request: fiote::config::Request,
    },
    SyncInspect {
        id: String,
        before: Option<i64>,
    },
    SyncHistoryPolicy {
        id: String,
        retention: nucleus::sync::Retention,
    },
    SyncForgetHistory {
        id: String,
    },
    Subscribe {
        id: String,
        protein: Protein,
    },
    SubscribeSaved {
        id: String,
        name: String,
    },
    Unsubscribe {
        id: String,
    },
    Act {
        id: String,
        action: Action,
    },
    SessionAuthenticate {
        id: String,
        session_id: String,
        session_challenge: String,
        person_uid: String,
        key_id: String,
        public_key_base64: String,
        signature: String,
    },
    SignedAct {
        id: String,
        session_id: String,
        session_challenge: String,
        sequence: u64,
        action_base64: String,
        signature: String,
    },
    LaneJoin {
        room: String,
    },
    LaneLeave {
        room: String,
    },
    LaneSend {
        room: String,
        payload: Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        organ: Option<String>,
    },
    CollabJoin {
        id: String,
        record_uid: String,
    },
    CollabLeave {
        record_uid: String,
    },
    CollabPresence {
        record_uid: String,
        property: String,
        anchor: String,
        focus: String,
    },
    CollabUpdate {
        id: String,
        record_uid: String,
        update_base64: String,
    },
    TerminalOpen {
        id: String,
        cols: u16,
        rows: u16,
        #[serde(default)]
        pixel_width: u16,
        #[serde(default)]
        pixel_height: u16,
    },
    TerminalInput {
        id: String,
        data_base64: String,
    },
    TerminalResize {
        id: String,
        cols: u16,
        rows: u16,
        #[serde(default)]
        pixel_width: u16,
        #[serde(default)]
        pixel_height: u16,
    },
    TerminalClose {
        id: String,
    },
    LiveLogin {
        username: String,
        password: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    Workspace { id: String, workspace: Value },
    DescriptionAsset { id: String, response: nucleus::description_asset::Response },
    PresentComponent { presentation: nucleus::component::Presentation },
    Speech { id: String, status: crate::speech::Status },
    FioteTerminal { id: String, frame: fiote::acp::terminal::TerminalFrame },
    Command { id: String, response: crate::command::Response },
    CallContext { id: String, context: engine::calls::Context },
    Call { id: String, device: String, snapshot: engine::calls::Snapshot },
    Fiote {
        id: String,
        status: fiote::config::Status,
    },
    SyncStatus {
        id: String,
        overview: nucleus::sync::Overview,
    },
    SessionChallenge {
        session_id: String,
        challenge: String,
        algorithm: String,
        person: Option<String>,
        signing_required: bool,
    },
    SessionAuthenticated {
        id: String,
        session_id: String,
        person: String,
        key_id: String,
    },
    Snapshot {
        id: String,
        rows: Arc<Vec<Value>>,
    },
    Update {
        id: String,
        rows: Arc<Vec<Value>>,
    },
    ActionOk {
        id: String,
        created: Option<String>,
        facts: usize,
        #[serde(default)]
        warnings: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        data: Option<serde_json::Value>,
    },
    Error {
        id: String,
        message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        code: Option<String>,
    },
    CollabState {
        id: String,
        record_uid: String,
        snapshot_base64: String,
        version: String,
        writable: Vec<String>,
    },
    CollabChange {
        record_uid: String,
        update_base64: String,
        version: String,
    },
    CollabAck {
        id: String,
        record_uid: String,
    },
    CollabCursors {
        record_uid: String,
        cursors: Vec<CollabCursor>,
    },
    LaneEvent {
        room: String,
        from: String,
        payload: Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        identity: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        organ: Option<String>,
    },
    LiveHello {
        login_required: bool,
    },
    LiveLoginOk {
        person: String,
        #[serde(default)]
        organ: String,
    },
    LiveLoginError {
        message: String,
    },
    Notifications {
        items: Vec<Value>,
    },
    TerminalOpened {
        id: String,
        shell: String,
        cwd: String,
    },
    TerminalData {
        id: String,
        data_base64: String,
    },
    TerminalExit {
        id: String,
        exit_code: Option<u32>,
    },
}
