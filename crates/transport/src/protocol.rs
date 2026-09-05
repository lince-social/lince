use engine::actions::Action;
use protein::Protein;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
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
        rows: Vec<Value>,
    },
    Update {
        id: String,
        rows: Vec<Value>,
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
    },
    CollabChange {
        record_uid: String,
        snapshot_base64: String,
    },
    CollabAck {
        id: String,
        record_uid: String,
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
