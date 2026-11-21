//! Wire frames shared by local and remote Transport sessions.

use std::{fmt, sync::Arc};

use engine::{
    action_intent::payload_requests_karma_preview, actions::Action, calls, workspace_sync,
};
use fiote::{acp::terminal, config};
use nucleus::{component::Presentation, description_asset, sync};
use protein::Protein;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{command, speech};

pub use engine::presence::Cursor as CollabCursor;

impl ClientMessage {
    /// Checks whether the request names a Karma preview for scheduling purposes.
    /// Signed payload classification precedes signature verification.
    pub fn runs_karma_preview(&self) -> bool {
        match self {
            Self::Act {
                action: Action::PreviewKarmaProposal { .. },
                ..
            } => true,
            Self::SignedAct { action_base64, .. } => payload_requests_karma_preview(action_base64),
            _ => false,
        }
    }
}

/// Requests use a snake-case `type` and carry an `id` when the reply is correlated.
/// Terminal frames accept omitted pixel dimensions as zero. Lane sends without
/// an Organ are scoped to the Cell hosting the lane.
/// Debug output identifies the frame variant and omits its payload.
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    WorkspaceSubscribe {
        id: String,
        workspace: String,
        client: workspace_sync::Client,
    },
    DescriptionAsset {
        id: String,
        request: description_asset::Request,
    },
    Speech {
        id: String,
        request: speech::Request,
    },
    FioteTerminal {
        id: String,
        request: terminal::TerminalRequest,
    },
    Command {
        id: String,
        request: command::Request,
    },
    CallContext {
        id: String,
        thread: String,
    },
    Call {
        id: String,
        thread: String,
        person: Option<String>,
        operation: calls::Operation,
    },
    Fiote {
        id: String,
        request: config::Request,
    },
    SyncInspect {
        id: String,
        before: Option<i64>,
    },
    SyncHistoryPolicy {
        id: String,
        retention: sync::Retention,
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

impl fmt::Debug for ClientMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let variant = match self {
            Self::WorkspaceSubscribe { .. } => "WorkspaceSubscribe",
            Self::DescriptionAsset { .. } => "DescriptionAsset",
            Self::Speech { .. } => "Speech",
            Self::FioteTerminal { .. } => "FioteTerminal",
            Self::Command { .. } => "Command",
            Self::CallContext { .. } => "CallContext",
            Self::Call { .. } => "Call",
            Self::Fiote { .. } => "Fiote",
            Self::SyncInspect { .. } => "SyncInspect",
            Self::SyncHistoryPolicy { .. } => "SyncHistoryPolicy",
            Self::SyncForgetHistory { .. } => "SyncForgetHistory",
            Self::Subscribe { .. } => "Subscribe",
            Self::SubscribeSaved { .. } => "SubscribeSaved",
            Self::Unsubscribe { .. } => "Unsubscribe",
            Self::Act { .. } => "Act",
            Self::SessionAuthenticate { .. } => "SessionAuthenticate",
            Self::SignedAct { .. } => "SignedAct",
            Self::LaneJoin { .. } => "LaneJoin",
            Self::LaneLeave { .. } => "LaneLeave",
            Self::LaneSend { .. } => "LaneSend",
            Self::CollabJoin { .. } => "CollabJoin",
            Self::CollabLeave { .. } => "CollabLeave",
            Self::CollabPresence { .. } => "CollabPresence",
            Self::CollabUpdate { .. } => "CollabUpdate",
            Self::TerminalOpen { .. } => "TerminalOpen",
            Self::TerminalInput { .. } => "TerminalInput",
            Self::TerminalResize { .. } => "TerminalResize",
            Self::TerminalClose { .. } => "TerminalClose",
            Self::LiveLogin { .. } => "LiveLogin",
        };
        f.debug_struct(variant).finish_non_exhaustive()
    }
}

/// Correlated replies and unsolicited updates, tagged by a snake-case `type`.
/// Snapshot and update rows share ownership across consumers.
/// Optional routing metadata and machine-readable errors may be omitted.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    Workspace {
        id: String,
        workspace: Value,
    },
    DescriptionAsset {
        id: String,
        response: description_asset::Response,
    },
    PresentComponent {
        presentation: Presentation,
    },
    Speech {
        id: String,
        status: speech::Status,
    },
    FioteTerminal {
        id: String,
        frame: terminal::TerminalFrame,
    },
    Command {
        id: String,
        response: command::Response,
    },
    CallContext {
        id: String,
        context: calls::Context,
    },
    Call {
        id: String,
        device: String,
        snapshot: calls::Snapshot,
    },
    Fiote {
        id: String,
        status: config::Status,
    },
    SyncStatus {
        id: String,
        overview: sync::Overview,
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
        data: Option<Value>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_debug_omits_credentials_and_signed_payloads() {
        const SECRET: &str = "synthetic-test-credential";
        for (message, variant) in [
            (
                ClientMessage::LiveLogin {
                    username: "user".into(),
                    password: SECRET.into(),
                },
                "LiveLogin",
            ),
            (
                ClientMessage::Act {
                    id: "create-user".into(),
                    action: Action::CreateUser {
                        username: "user".into(),
                        name: "User".into(),
                        password: SECRET.into(),
                        role: "reader".into(),
                    },
                },
                "Act",
            ),
            (
                ClientMessage::SignedAct {
                    id: "sign-action".into(),
                    session_id: "session".into(),
                    session_challenge: "challenge".into(),
                    sequence: 1,
                    action_base64: SECRET.into(),
                    signature: "signature".into(),
                },
                "SignedAct",
            ),
        ] {
            assert_eq!(format!("{message:?}"), format!("{variant} {{ .. }}"));
            assert_eq!(format!("{message:#?}"), format!("{variant} {{ .. }}"));
        }
    }
}
