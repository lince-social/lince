//! Public Transfer receipt model and wire protocol.

use serde::Serialize;

use crate::karma::{CanonicalHash, canonical_hash};

mod approval;
mod authority;
mod command;
mod document;
mod event;
mod merge;
mod scope;
mod view;

pub use approval::{Approval, ApprovalRole, PersonApproval, PublicPerson};
pub use authority::{
    Authority, OriginalAuthority, Participation, ParticipationRequest, original_authority_records,
};
pub use command::{Command, EventRequest, EventSource};
pub use document::{Bundle, Document, Publication, Review};
pub use event::{Content, Event, SignedEvent};
pub use scope::{Measurement, PartyAttribution, RecordedFloat, Scope, ScopeRequest, Source, Stage};

pub const MAX_BYTES: usize = 524_288;
pub const MAX_EVENTS: usize = 256;
pub const MAX_MEASURES: usize = 32;
pub const MAX_AUTHORITIES: usize = 64;
pub const MAX_AUTHORITY_SECONDS: i64 = 30 * 24 * 60 * 60;
pub const MAX_BUNDLE_BYTES: usize = MAX_BYTES + 262_144;
pub const NAMESPACE: &str = "lince.transfer-receipt";

fn digest<T: Serialize>(domain: &str, value: &T) -> Result<CanonicalHash, String> {
    canonical_hash(domain, value).map_err(|error| error.to_string())
}

fn signature_shape(key_id: &str, signature: &str) -> Result<(), String> {
    if key_id.is_empty() || key_id.len() > 160 || signature.len() != 88 {
        return Err("Invalid receipt signing key or signature length".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests;
