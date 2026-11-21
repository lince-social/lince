//! Local Organ identity, contact configuration, and delivery state.

mod contacts;
mod delivery;
mod local;
pub mod presentation;
mod quarantine;

pub use contacts::{
    Contact, add_contact, contact, contact_by_node_id, contacts, forget_contact,
    pending_introductions, rename_contact, set_contact_accept_scope, set_contact_scope,
    set_contact_share_protein, set_node_id, set_pending_introduction, set_proximity,
    set_sync_policy, set_trust,
};
pub use delivery::{
    Delivery, advance_peer_acked_seq, awaiting_roster_longer_than, backdate_unreachable,
    clear_awaiting_roster, mark_awaiting_roster, mark_mailed, mark_mailed_clear, mark_reachable,
    mark_unreachable, minimum_catchup_interval, set_catchup_interval, set_last_synced_seq,
    set_mode,
};
pub use local::{
    LOCAL_ORGAN_SLUG, OrganRecord, adopt_identity, ensure_local, holds_own_records, local,
    local_head,
};
pub use quarantine::{
    QUARANTINE_PER_CONTACT, clear_quarantine, quarantine, quarantine_by_contact, quarantine_count,
    quarantined_for, quarantined_for_many,
};
