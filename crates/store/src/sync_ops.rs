//! The durable sync operation log and outgoing delivery queue.

mod model;
mod outbox;
mod read;
mod retention;
mod scope;
mod vector;
mod write;

pub use model::{OpKind, OpRow, VectorEntry};
pub use outbox::{
    OutboxRow, enqueue_record_for_contact, enqueue_widened_for_contact, outbox_bump_attempts,
    outbox_clear_contact, outbox_delete, outbox_due,
};
pub use read::{
    after, after_in_root, all_by_hlc, for_field, get_by_seq, latest_author_for_field,
    latest_hlc_for_field, latest_set_hlc_for_row, max_hlc, max_seq, record_field_tips,
};
pub use retention::{PruneReport, prune, retention_floor};
pub use scope::{
    LINK_ALL, LINK_PREFIX, LinkScope, link_names, names_links, narrow_ops_to_scope, op_in_scope,
    op_in_scope_with_links, resolve_link_scope,
};
pub use vector::{
    ops_missing_from_vector, ops_missing_from_vector_in_root, own_conversation_ops_after,
    seq_covered_by_vector, sibling_ops_missing_from_vector, version_vector_for_organ,
    version_vector_for_root,
};
pub use write::{append, log_local, log_local_tx};
