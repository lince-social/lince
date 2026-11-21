//! Pending offers, refusal windows, and locally recorded outcomes.

mod history;
mod listing;

pub use history::{
    REFUSAL_WINDOW_DAYS, Refusal, forget_refusal, local_outcomes, refusals, refuse,
    refused_by_party, remember_outcome, standing_refusal,
};
pub use listing::pending;

/// The stored category of an offer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfferKind {
    /// Incoming file copy.
    BlobSync,
    /// Invitation to a conversation thread.
    ThreadInvite,
    /// Offer to share a replica root.
    ReplicaGrant,
    /// Offer to move a Record.
    RecordMove,
    /// Invitation to participate in a Transfer.
    Transfer,
}

impl OfferKind {
    /// Returns the stable stored label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::BlobSync => "blob-sync",
            Self::ThreadInvite => "thread-invite",
            Self::ReplicaGrant => "replica-grant",
            Self::RecordMove => "record-move",
            Self::Transfer => "transfer",
        }
    }
}

/// Direction of an offer relative to the local Organ.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Received by the local Organ.
    Incoming,
    /// Sent by the local Organ.
    Outgoing,
}

impl Direction {
    /// Returns the stable stored label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Incoming => "incoming",
            Self::Outgoing => "outgoing",
        }
    }
}

/// The local view of an offer awaiting a decision.
#[derive(Debug, Clone)]
pub struct Offer {
    /// Offer category.
    pub kind: OfferKind,
    /// Direction of the offer.
    pub direction: Direction,
    /// Identity of the pending item.
    pub subject_uid: String,
    /// Display title, which may be empty.
    pub title: String,
    /// Peer or addressee carried by the offer.
    pub other_party: String,
    /// Stored offer creation timestamp.
    pub created_at: String,
    /// Whether this offer tuple has a standing refusal.
    pub previously_refused: bool,
}
