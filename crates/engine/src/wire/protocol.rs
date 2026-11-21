// non-lean-yet

//! JSON messages and data carried by the peer protocol.

use serde::{Deserialize, Serialize};

use crate::{
    roster::SignedRoster,
    sync::{Introduction, OpBatch, WireOp},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailboxBundle {
    pub uid: String,
    pub from_organ: String,
    pub from_cell: String,
    pub body: String,
    pub received_at: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExpiredMail {
    pub uid: String,
    pub to_organ: String,
    pub bytes: i64,
    pub received_at: String,
    pub expired_at: String,
}

/// A peer request, encoded with its operation name in the `op` field.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum WireRequest {
    Location {
        request: nucleus::location::PeerRequest,
    },
    SandPackages {
        query: nucleus::sand_package::Query,
    },
    Presence {
        records: Vec<String>,
        entries: Vec<crate::presence::Entry>,
    },
    MoveOffer {
        uid: String,
        preview: store::record_move::offers::Preview,
    },
    MoveBundle {
        uid: String,
        bundle: store::record_move::offers::Bundle,
    },
    MoveCancel {
        uid: String,
    },
    Introduction,
    OrganAccessStatus,
    Introduce {
        intro: Introduction,
    },
    PushOps {
        batch: OpBatch,
    },
    MailboxDeposit {
        body: String,
    },
    MailboxWaiting {
        organ_uid: String,
        roster: SignedRoster,
    },
    MailboxCollect {
        organ_uid: String,
        roster: SignedRoster,
        limit: i64,
    },
    MailboxCollected {
        organ_uid: String,
        roster: SignedRoster,
        uids: Vec<String>,
    },
    MailboxAskToCarry {
        roster: SignedRoster,
    },
    MailboxRedeemInvite {
        token: String,
        roster: SignedRoster,
    },
    PublishSealingKey {
        cell_uid: String,
        sealing_key: crate::seal::SealingKey,
    },
    MailboxExpiries,
    MailboxExpiriesHeard {
        uids: Vec<String>,
    },
    FetchOpsSince {
        vector: Vec<store::sync_ops::VectorEntry>,
        limit: i64,
    },
    FetchReference {
        root: String,
        record: String,
    },
    OfferGrant {
        root: String,
        title: String,
        intro: Introduction,
    },
    GroupOffer {
        group: crate::groups::SignedMembership,
    },
    GroupAccept {
        root: String,
    },
    Call {
        request: crate::calls::Request,
    },
    AcceptGrant {
        root: String,
    },
    DeclineGrant {
        root: String,
    },
    PushGrantOps {
        root: String,
        batch: OpBatch,
    },
    FetchGrantOpsSince {
        root: String,
        vector: Vec<store::sync_ops::VectorEntry>,
        limit: i64,
    },
    FetchRoster,
    TransferPost {
        verb: TransferVerb,
        body: serde_json::Value,
    },
    FetchSuccessions,
    FetchRevocations,
    Enrol {
        token: String,
        cell_uid: String,
        node_id: String,
        label: String,
        operational_key: String,
        #[serde(default)]
        sealing_key: Option<crate::seal::SealingKey>,
    },
    FetchVector {
        organ_uid: String,
    },
    FetchDoorRequests {
        limit: i64,
    },
    ReleaseDoorRequests {
        uids: Vec<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferVerb {
    Envelope,
    Pull,
    Receipt,
    Command,
    PolicyEvent,
    ApplicationAttestation,
}

#[async_trait::async_trait]
pub trait TransferPeer: Send + Sync {
    async fn handle(
        &self,
        verb: TransferVerb,
        body: serde_json::Value,
    ) -> Result<serde_json::Value, String>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevocationCert {
    pub organ_uid: String,
    pub revoked_key: String,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuccessionCert {
    pub organ_uid: String,
    pub old_key: String,
    pub new_key: String,
    pub created_at: String,
    pub signature: String,
}

/// A peer reply, encoded with its response name in the `ok` field.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "ok", rename_all = "snake_case")]
pub enum WireResponse {
    Location {
        data: serde_json::Value,
    },
    OrganAccessStatus {
        status: store::organ_access::Status,
    },
    SandPackages {
        response: nucleus::sand_package::Response,
    },
    MoveStatus {
        state: String,
        receipt: Option<String>,
    },
    Call {
        snapshot: crate::calls::Snapshot,
    },
    Group {
        group: crate::groups::SignedMembership,
    },
    Presence {
        entries: Vec<crate::presence::Entry>,
    },
    Introduction {
        intro: Introduction,
    },
    Applied {
        applied: usize,
    },
    BatchSaved {
        applied: usize,
        complete: bool,
    },
    Roster {
        roster: Option<crate::roster::SignedRoster>,
    },
    Revocations {
        certs: Vec<RevocationCert>,
    },
    Successions {
        certs: Vec<SuccessionCert>,
    },
    Transfer {
        body: serde_json::Value,
    },
    Ops {
        from_organ: String,
        ops: Vec<WireOp>,
        head: i64,
    },
    MailboxAccepted {
        uid: String,
    },
    MailboxBundles {
        bundles: Vec<MailboxBundle>,
    },
    MailboxWaiting {
        bundles: i64,
        bytes: i64,
        oldest_expires_at: Option<String>,
    },
    MailboxAsked,
    MailboxExpired {
        expired: Vec<ExpiredMail>,
    },
    MailboxCarrying {
        label: String,
        quota_bytes: i64,
    },
    Error {
        message: String,
    },
    Refused {
        code: String,
        message: String,
    },
    DoorRequests {
        requests: Vec<HeldIntroduction>,
    },
    Reference {
        row: serde_json::Value,
    },
    Vector {
        vector: Vec<store::sync_ops::VectorEntry>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditAgreement {
    pub contact_organ: String,
    pub they_lack: usize,
    pub unknown_cells: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaleSibling {
    pub cell_uid: String,
    pub node_id: String,
    pub label: String,
    pub their_epoch: u32,
    pub our_epoch: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeldIntroduction {
    pub uid: String,
    pub node_id: String,
    pub intro: Introduction,
    pub received_at: String,
}
