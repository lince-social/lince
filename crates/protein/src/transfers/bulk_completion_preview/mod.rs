//! Read-only review of proposed claims in one Transfer tree.

mod claims;

use super::{
    transfer_predicate_name, transfer_query_error, viewer::TransferViewer, visibility::can_read,
};
use crate::{Predicate, Protein, ProteinError};
use claims::Review;
use nucleus::transfer::transfer_bulk_claim_review_token;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use store::{
    Store,
    transfers::{self, TransferRow},
    visibility::visible_targets,
};

struct Selection<'a> {
    root: &'a str,
    occurrences: Vec<&'a str>,
}

impl<'a> Selection<'a> {
    fn parse(protein: &'a Protein) -> Result<Self, ProteinError> {
        let invalid =
            |detail| transfer_query_error("protein_transfer_bulk_completion_invalid_query", detail);
        let include = &protein.include;
        if protein.aggregate.is_some()
            || !protein.order.is_empty()
            || protein.limit.is_some()
            || include.facts.is_some()
            || include.promises.is_some()
            || include.links.is_some()
            || include.threads.is_some()
            || include.availability
            || include.extension.is_some()
            || include.numeric_extensions
            || include.projection.is_some()
            || include.contact
        {
            return Err(invalid(
                "include, aggregate, order, and limit are not supported",
            ));
        }
        let mut root = None;
        let mut occurrences = None;
        for predicate in &protein.filter {
            match predicate {
                Predicate::UidEq(uid) if root.is_none() => root = Some(uid.as_str()),
                Predicate::OccurrenceIn(ids) if occurrences.is_none() => occurrences = Some(ids),
                predicate => {
                    return Err(transfer_query_error(
                        "protein_transfer_bulk_completion_unsupported_predicate",
                        transfer_predicate_name(predicate),
                    ));
                }
            }
        }
        let (Some(root), Some(ids)) = (root, occurrences) else {
            return Err(invalid(
                "one direct uid_eq root and one direct occurrence_in selection are required",
            ));
        };
        if ids.is_empty() {
            return Err(invalid("occurrence_in must select at least one occurrence"));
        }
        let mut occurrences: Vec<_> = ids.iter().map(String::as_str).collect();
        occurrences.sort_unstable();
        occurrences.dedup();
        Ok(Self { root, occurrences })
    }
}

fn is_in_tree<'a>(
    mut uid: &'a str,
    root: &str,
    transfers: &HashMap<&str, &'a TransferRow>,
) -> bool {
    let mut seen = HashSet::new();
    while uid != root {
        if !seen.insert(uid) {
            return false;
        }
        let Some(parent) = transfers.get(uid).and_then(|row| row.parent_uid.as_deref()) else {
            return false;
        };
        uid = parent;
    }
    true
}

pub(crate) async fn execute_transfer_bulk_completion_preview(
    store: &Store,
    protein: &Protein,
    subject: Option<&str>,
    signer: Option<&str>,
) -> Result<Vec<Value>, ProteinError> {
    let Selection { root, occurrences } = Selection::parse(protein)?;
    let viewer = TransferViewer::resolve(store, subject).await?;
    let Some(person) =
        signer.filter(|signer| viewer.local || viewer.person.as_deref() == Some(*signer))
    else {
        return Ok(Vec::new());
    };
    if !viewer.update_identity_blockers().is_empty() {
        return Ok(Vec::new());
    }

    let pool = &store.pool;
    let rows = transfers::list_all(pool).await?;
    let transfers: HashMap<_, _> = rows
        .iter()
        .map(|row| (row.transfer.record_uid.as_str(), &row.transfer))
        .collect();
    let Some(root_row) = transfers.get(root) else {
        return Ok(Vec::new());
    };
    let granted = match subject {
        Some(subject) => Some(visible_targets(pool, subject).await?),
        None => None,
    };
    if !can_read(store, root, &viewer, granted.as_ref()).await? {
        return Ok(Vec::new());
    }

    let mut review = Review::default();
    // The selection is sorted by occurrence UID. Each occurrence emits delivery before receipt.
    for uid in &occurrences {
        let Some(occurrence) = transfers::occurrence(pool, uid).await? else {
            review.block(uid, "occurrence_missing");
            continue;
        };
        let transfer = occurrence.transfer_uid.as_str();
        if !can_read(store, transfer, &viewer, granted.as_ref()).await? {
            review.block(uid, "occurrence_not_visible");
            continue;
        }
        let revision = transfers
            .contains_key(transfer)
            .then_some(occurrence.revision);
        review
            .include(
                pool,
                &occurrence,
                person,
                revision,
                is_in_tree(transfer, root, &transfers),
            )
            .await?;
    }
    let eligible_count = review.eligible.len();
    let item_count = review.items.len();
    let blocked_count = item_count - eligible_count;
    let blocker = match (eligible_count, blocked_count) {
        (0, _) => Some("no_eligible_selected_occurrence_claims"),
        (_, 0) => None,
        _ => Some("selection_contains_blocked_items"),
    };
    Ok(vec![json!({
        "kind": "transfer_bulk_completion_preview",
        "root": root,
        "root_revision": root_row.revision,
        "person": person,
        "review_token": transfer_bulk_claim_review_token(person, &review.eligible),
        "selection": occurrences,
        "eligible": blocker.is_none(),
        "item_count": item_count,
        "eligible_count": eligible_count,
        "blocked_count": blocked_count,
        "blockers": blocker.as_slice(),
        "items": review.items,
    })])
}
