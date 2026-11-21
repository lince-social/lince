//! Transfer queries, previews, and delivery projections.

mod application;
mod bulk_completion_preview;
mod delivery;
mod filter;
mod hierarchy;
mod history;
mod invitations;
mod karma_state;
mod occurrences;
mod order;
mod origin_delivery;
mod projection;
mod promises;
mod publication;
mod query_context;
mod remote_delivery;
mod roles;
mod rows;
mod settlement_preview;
mod status;
mod viewer;
mod visibility;

pub(super) use bulk_completion_preview::execute_transfer_bulk_completion_preview;
pub use delivery::transfer_delivery_projection;
pub(super) use settlement_preview::execute_transfer_settlement_preview;
pub use status::{
    transfer_agreement_ready, transfer_occurrence_roles, transfer_ready_promises_for_person,
};
use viewer::TransferViewer;

use crate::{Predicate, Protein, ProteinError};
use serde_json::Value;
use std::{collections::HashSet, fmt::Display};
use store::{
    Store,
    sqlx::{Error::Protocol, query_scalar},
};

pub(super) async fn execute(
    store: &Store,
    protein: &Protein,
    visible: Option<&HashSet<String>>,
    subject: Option<&str>,
    signer: Option<&str>,
) -> Result<Vec<Value>, ProteinError> {
    rows::execute_transfers(store, protein, visible, subject, signer, None).await
}

pub(super) async fn protected_transfer_history(
    store: &Store,
    subject: Option<&str>,
) -> Result<HashSet<String>, ProteinError> {
    let viewer = TransferViewer::resolve(store, subject).await?;
    if viewer.local {
        return Ok(HashSet::new());
    }
    let transfers: Vec<String> = query_scalar(
        "SELECT DISTINCT p.transfer_uid FROM promise p
         WHERE p.item_json IS NOT NULL AND p.transfer_uid IS NOT NULL
         AND NOT EXISTS (SELECT 1 FROM transfer_party tp
             WHERE tp.transfer_uid = p.transfer_uid AND tp.kind = 'creator' AND tp.actor_uid = ?)
         UNION SELECT r.transfer_uid FROM transfer_revision r JOIN fact f ON f.uid = r.fact_uid
         WHERE json_array_length(f.payload, '$.terms.children') > 0 OR json_extract(f.payload, '$.terms.transfer.parent_uid') IS NOT NULL OR json_array_length(f.payload, '$.terms.dependencies') > 0",
    )
    .bind(viewer.person.as_deref())
    .fetch_all(&store.pool)
    .await?;
    Ok(transfers.into_iter().collect())
}

fn transfer_query_error(code: &str, detail: impl Display) -> ProteinError {
    Protocol(format!("{code}: {detail}"))
}

fn transfer_predicate_name(predicate: &Predicate) -> &'static str {
    match predicate {
        Predicate::ProjectionWindow(_) => "projection_window",
        Predicate::RollingWindow(_) => "rolling_window",
        Predicate::All(_) => "all",
        Predicate::Any(_) => "any",
        Predicate::Not(_) => "not",
        Predicate::UidEq(_) => "uid_eq",
        Predicate::OccurrenceIn(_) => "occurrence_in",
        Predicate::SlugEq(_) => "slug_eq",
        Predicate::RevisionEq(_) => "revision_eq",
        Predicate::RevisionLt(_) => "revision_lt",
        Predicate::RevisionLte(_) => "revision_lte",
        Predicate::RevisionGt(_) => "revision_gt",
        Predicate::RevisionGte(_) => "revision_gte",
        Predicate::StatusIn(_) => "status_in",
        Predicate::ViewerRoleIn(_) => "viewer_role_in",
        Predicate::InvitationStateIn(_) => "invitation_state_in",
        Predicate::PersonEq(_) => "person_eq",
        Predicate::RecordEq(_) => "record_eq",
        Predicate::ConceptIn(_) => "concept_in",
        Predicate::UnitEq(_) => "unit_eq",
        Predicate::WindowEndBefore(_) => "window_end_before",
        Predicate::WindowEndAfter(_) => "window_end_after",
        Predicate::QuantityLt(_) => "quantity_lt",
        Predicate::QuantityLte(_) => "quantity_lte",
        Predicate::QuantityGt(_) => "quantity_gt",
        Predicate::QuantityGte(_) => "quantity_gte",
        Predicate::QuantityEq(_) => "quantity_eq",
        Predicate::KindEq(_) => "kind_eq",
        Predicate::Relation { .. } => "relation",
        Predicate::Under { .. } => "under",
        Predicate::TextContains(_) => "text_contains",
        Predicate::WorkDate { .. } => "work_date",
        Predicate::StateIn(_) => "state_in",
        Predicate::AtSince(_) => "at_since",
        Predicate::AtBefore(_) => "at_before",
        Predicate::ClassifiedIn(_) => "classified_in",
        Predicate::CauseKindEq(_) => "cause_kind_eq",
        Predicate::Near { .. } => "near",
        Predicate::OrganEq(_) => "organ_eq",
        Predicate::OrganIn(_) => "organ_in",
        Predicate::FileOriginEq(_) => "file_origin_eq",
    }
}
