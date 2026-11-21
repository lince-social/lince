//! Read implementations grouped by the data they query.

mod activity;
mod auth;
mod decisions;
mod entries;
mod frequency;
mod karma;
mod nearby;
mod promises;
mod records;
mod recurrences;
mod vocabulary;

pub use records::{matching_among, matching_records, select_records};

pub(crate) use activity::{execute_facts, execute_timeline};
pub(crate) use auth::execute_auth;
pub(crate) use decisions::execute_decisions;
pub(crate) use entries::execute_entries;
pub(crate) use frequency::execute_frequency;
pub(crate) use karma::execute_karma;
pub(crate) use nearby::execute_nearby;
pub(crate) use promises::execute_promises;
pub(crate) use records::{execute_records, matching_records_for, record_availability, resolve_at};
pub(crate) use recurrences::execute_recurrence;
pub(crate) use vocabulary::{execute_assertions, execute_concepts, execute_linguas};
