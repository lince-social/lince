//! The wire protocol (blueprint VII.3): JSON messages both ways. Sands use
//! Protein for reads, Actions for writes, lanes for peer ephemera, and named
//! host-capability frames for streams that do not belong in the Ledger.

use engine::actions::Action;
use protein::Protein;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Client -> server. `id` correlates responses to requests/subscriptions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    /// Open a live subscription: an immediate snapshot, then updates whenever a
    /// committed fact may have changed the result.
    Subscribe {
        id: String,
        protein: Protein,
    },
    /// Subscribe to a saved Protein by slug/uid (blueprint VII.1).
    SubscribeSaved {
        id: String,
        name: String,
    },
    Unsubscribe {
        id: String,
    },
    /// A typed write. Protein never mutates; every mutation is an Action.
    Act {
        id: String,
        action: Action,
    },
    /// Bind an authenticated WebSocket to the private key held by its client.
    /// `person_uid` must equal the server-side app_user -> Person mapping
    /// announced in `SessionChallenge`; it is never trusted independently.
    SessionAuthenticate {
        id: String,
        session_id: String,
        session_challenge: String,
        person_uid: String,
        key_id: String,
        public_key_base64: String,
        signature: String,
    },
    /// A remote typed write. The decoded `action_base64` must be exactly one
    /// Action JSON value and its bytes, message id, sequence, session id and
    /// challenge are covered by the registered key's signature. Person and
    /// key identity come from the authenticated session, not this frame.
    SignedAct {
        id: String,
        session_id: String,
        session_challenge: String,
        sequence: u64,
        action_base64: String,
        signature: String,
    },
    /// Join an ephemeral room (presence/cursors/call signaling).
    LaneJoin {
        room: String,
    },
    LaneLeave {
        room: String,
    },
    /// Fan-out to everyone in the room. Never persisted (blueprint VII.3).
    LaneSend {
        room: String,
        payload: Value,
        /// Which Organ the thing this event refers to lives on — a sibling of
        /// `payload`, never inside it, so a sand that has never heard of this
        /// field keeps reading the payload it was built for.
        ///
        /// Absent means "the Cell hosting this lane". Lanes never leave one
        /// Cell (a board's room traffic always rides its own transport, even
        /// when every card on it is bound elsewhere), so that reading is the
        /// same for the sender and for every receiver — there is no uid to
        /// translate at the boundary.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        organ: Option<String>,
    },
    /// Join a record's live collab doc (Ontology §11 "Collab"): the reply is a
    /// `CollabState` snapshot of the record-doc; afterwards every change to the
    /// doc (another client here, or a peer Organ syncing in) is pushed as a
    /// `CollabChange`. Collab frames are a host capability like terminals —
    /// the write is attributed to the ORGAN in the op log, not a Person.
    CollabJoin {
        id: String,
        record_uid: String,
    },
    CollabLeave {
        record_uid: String,
    },
    /// A client-side Loro update (base64 update bytes since the client's last
    /// send). The engine merges it into the record-doc, logs ONE cumulative
    /// crdt op for peer sync, and materializes head/body back to SQLite.
    /// Answered by `CollabAck` carrying this `id` on success, or an `Error`
    /// frame carrying it on failure.
    CollabUpdate {
        id: String,
        record_uid: String,
        update_base64: String,
    },
    /// Open an ephemeral PTY owned by this transport connection. Terminal I/O
    /// is an explicit host capability, never Protein data or a Ledger write.
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
    /// Log into an iroh live session with a username and password.
    ///
    /// The device-INDEPENDENT way in: nothing about the sender's keys is
    /// consulted, so a Lince installed a minute ago works exactly as well as
    /// one the host has known for a year. Valid only as the FIRST frame of a
    /// session that asked for a login; the session driver never accepts it
    /// afterwards, so it can never re-authenticate a session mid-flight.
    LiveLogin {
        username: String,
        password: String,
    },
}

/// Server -> client.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    /// Sent once as the first application frame on every connection. For an
    /// authenticated remote subject, `person` is the authoritative Ledger
    /// identity mapped by the server and signing is mandatory for Actions.
    SessionChallenge {
        session_id: String,
        challenge: String,
        algorithm: String,
        person: Option<String>,
        signing_required: bool,
    },
    /// The connection proved possession of the key now bound to its mapped
    /// Person. This is session state only; no private key reaches the server.
    SessionAuthenticated {
        id: String,
        session_id: String,
        person: String,
        key_id: String,
    },
    /// The full current result of a subscription.
    Snapshot {
        id: String,
        rows: Vec<Value>,
    },
    /// A recomputed result pushed after a relevant commit (v1: full re-send).
    Update {
        id: String,
        rows: Vec<Value>,
    },
    /// Result of an Action: the created uid (if any), committed fact count,
    /// and non-fatal advisories (link cycles, rule Proof loops). Warnings are
    /// advice, never rejections — clients should show them, not error on them.
    ActionOk {
        id: String,
        created: Option<String>,
        facts: usize,
        #[serde(default)]
        warnings: Vec<String>,
        /// Structured result, when one uid is not enough to render what
        /// happened — an enrolment code and its QR, what a front door is
        /// holding, which device is out of date. Omitted when there is none,
        /// so the frame stays the same shape it always was for every Action
        /// that has nothing extra to say.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        data: Option<serde_json::Value>,
    },
    /// An Action or subscription failed.
    Error {
        id: String,
        message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        code: Option<String>,
    },
    /// Reply to `CollabJoin`: the record-doc's full snapshot (base64 Loro
    /// snapshot bytes) — import it into a fresh client doc.
    CollabState {
        id: String,
        record_uid: String,
        snapshot_base64: String,
    },
    /// The record-doc changed (any writer: this client, a sibling session, or
    /// a peer Organ syncing in). Carries the full snapshot; Loro imports are
    /// idempotent by version vector, so over-delivery is harmless.
    CollabChange {
        record_uid: String,
        snapshot_base64: String,
    },
    /// A `CollabUpdate` was merged and durably logged. Carries that update's
    /// `id`.
    ///
    /// This is what lets a client know its work LANDED. A delta is exported
    /// once, relative to the last version the client believes the Cell holds;
    /// if the client advances that version on send rather than on confirmation,
    /// a frame lost to a dropped socket is excluded from every future export
    /// and the edit is gone from the Cell forever while still looking present
    /// on screen. The `CollabChange` echo cannot serve this purpose: it also
    /// fires for a sibling's write, so receiving one proves nothing about
    /// whether YOUR update was applied.
    CollabAck {
        id: String,
        record_uid: String,
    },
    /// A message from another session in a joined room.
    LaneEvent {
        room: String,
        from: String,
        payload: Value,
        /// The sender's identity, present ONLY when the receiver has read
        /// permission on that user (Ontology §11 presence). Absent means an
        /// anonymous cursor: the position still renders, the name does not.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        identity: Option<String>,
        /// Carried through from the `LaneSend` that raised it. See there for
        /// what absent means.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        organ: Option<String>,
    },
    /// First frame of an iroh live session, before anything is served.
    ///
    /// `login_required` is false for a peer whose Organ was granted a device
    /// binding — the handshake already proved who they are. It is true for
    /// everyone else, and then NOTHING is served until `LiveLogin` succeeds.
    /// Announced rather than inferred so the guest never has to guess whether
    /// to send a credential.
    LiveHello {
        login_required: bool,
    },
    /// The credential named a Person and this session now acts as them.
    ///
    /// `organ` is the host's own Organ uid, returned because a guest logging in
    /// from a fresh install has no contact row and therefore no name for the
    /// Cell it just got into. Without it there is nothing stable to bind a sand
    /// to, and the login could not survive a reload.
    LiveLoginOk {
        person: String,
        #[serde(default)]
        organ: String,
    },
    /// The credential did not. The message is identical for every cause, so a
    /// refusal never confirms which half was right.
    LiveLoginError {
        message: String,
    },
    /// This Cell's pending notifications, in full.
    ///
    /// Sent once on connect and again whenever the set changes. The full list
    /// rather than a delta: it is a handful of conversation invites, and a
    /// client that reconnects has to end up with the same list either way —
    /// which a delta stream cannot promise across a dropped socket.
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
