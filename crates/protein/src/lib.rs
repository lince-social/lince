#![recursion_limit = "512"]

//! Executes declarative Protein queries against Lince's readable data.

mod assertion_order;
pub mod calendar;
mod conjunction;
mod execution;
mod invalidation;
mod karma_rules;
mod meanings;
mod projection;
pub mod read_rules;
mod record_includes;
mod record_links;
mod record_order;
mod record_predicates;
pub mod record_query;
pub mod record_schema;
mod record_threads;
pub mod relation_context;
pub mod role_authority;
pub mod schedule;
pub mod shader_inputs;
mod sources;
mod transfers;
mod validation;

pub use execution::{
    Context, execute, execute_for, execute_for_with_context, execute_for_with_signer,
    execute_saved, execute_saved_with_signer,
};
pub use invalidation::{FactChanges, affects};
pub use meanings::bind_meanings;
pub use protein_authority::authority;
pub use protein_authority::query::{
    Aggregate, AggregateOp, DateComparison, ExtensionInclude, FactsInclude, GroupBy, Include,
    LinkDirection, LinkEndpoint, LinkOrder, LinksInclude, MessageCursor, Order, Predicate,
    ProjectionInclude, PromisesInclude, Protein, Source, ThreadsInclude, WorkDateField,
};
pub use sources::{matching_among, matching_records, select_records};
pub use transfers::{
    transfer_agreement_ready, transfer_delivery_projection, transfer_occurrence_roles,
    transfer_ready_promises_for_person,
};
pub use validation::validate;

use sources::{execute_records, matching_records_for, record_availability, resolve_at};
use store::{exact::zero, sqlx::Error::Protocol};

/// Error returned by query validation or a source read.
pub type ProteinError = store::StoreError;

/// Extracts a Protein protocol code for callers that handle particular failures.
pub fn error_code(error: &ProteinError) -> Option<String> {
    let Protocol(message) = error else {
        return None;
    };
    let code = message
        .split_once(':')
        .map_or(message.as_str(), |(code, _)| code);
    if code.starts_with("protein_")
        && code.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
        })
    {
        Some(code.to_string())
    } else {
        None
    }
}

/// Reports whether the query depends on live peers supplied by the caller.
pub fn is_ephemeral(protein: &Protein) -> bool {
    protein.source == Source::Nearby
}

/// Reports whether the query projects future entries into a Calendar or Schedule.
pub fn depends_on_forecast(protein: &Protein) -> bool {
    matches!(protein.source, Source::Calendar | Source::Schedule)
}

/// Reports whether the source can use an installed signer to compute its rows.
pub fn uses_signer(protein: &Protein) -> bool {
    match protein.source {
        Source::KarmaRule
        | Source::Transfer
        | Source::TransferSettlementPreview
        | Source::TransferBulkCompletionPreview => true,
        Source::Record
        | Source::Promise
        | Source::Decision
        | Source::Fact
        | Source::Concept
        | Source::Lingua
        | Source::Assertion
        | Source::Auth
        | Source::Karma
        | Source::Timeline
        | Source::Calendar
        | Source::Schedule
        | Source::Entry
        | Source::Frequency
        | Source::Recurrence
        | Source::Nearby => false,
    }
}

/// Selects negative-quantity plain Records in link order for the focus queue.
pub fn focus_queue(order_kind: &str) -> Protein {
    Protein {
        source: Source::Record,
        filter: vec![
            Predicate::QuantityLt(zero()),
            Predicate::KindEq("plain".into()),
        ],
        fields: None,
        include: Include::default(),
        aggregate: None,
        order: vec![
            Order::Link(LinkOrder {
                kind: order_kind.into(),
                higher: LinkEndpoint::From,
            }),
            Order::Asc("created_at".into()),
        ],
        limit: None,
    }
}

/// Selects the Decisions that currently need attention.
pub fn decision_queue() -> Protein {
    Protein {
        source: Source::Decision,
        filter: vec![],
        fields: None,
        include: Include::default(),
        aggregate: None,
        order: vec![],
        limit: None,
    }
}
