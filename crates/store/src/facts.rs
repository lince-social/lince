//! Exact facts, ledger views, and fact retention.

mod ledger;
mod quantities;
mod retention;
mod rows;

#[cfg(test)]
mod tests;

use chrono::{DateTime, SecondsFormat, Utc};

pub use ledger::{LedgerRow, hours_since_last, ledger_since, list_since};
pub use quantities::{
    level, quantity_offset, sum_neg_window, sum_pos_window, sum_window, sum_window_lagged,
};
pub use retention::{
    archivable_before, delete_by_uids, last_checkpoint, retention_policies, set_retention,
};
pub use rows::{
    Summary, after_position, after_position_with_commits, creator_uid, exists, for_record, get,
    get_in_transaction, get_on, insert, last_hash, retain_origin, summaries_for_records,
};

pub(crate) use quantities::checkpoint_base;
pub(crate) use rows::map_fact_with_origin;

/// Formats a fact timestamp at nanosecond precision for SQLite ordering.
pub fn instant(at: DateTime<Utc>) -> String {
    at.to_rfc3339_opts(SecondsFormat::Nanos, true)
}
