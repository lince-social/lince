//! Assemble original receipt proofs by source and publication scope.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use super::super::{MAX_BYTES, MAX_DISCLOSURES, query::SourceEvidence};
use crate::{
    json::encoded_len,
    karma::CanonicalHash,
    transfer::{
        receipt::{Bundle, Document, Event},
        receipt_fragment::{State, resolve_all},
        receipt_history::{self, Copy, Provenance, View},
    },
};

/// A borrowed proof document and its observed provenance.
#[derive(Debug)]
pub struct Receipt<'a> {
    pub document: &'a Document,
    pub provenance: Provenance,
}

#[derive(Debug, Serialize)]
pub struct Pending {
    pub scope: CanonicalHash,
    pub event: CanonicalHash,
    pub state: State,
    pub reason: String,
}

#[derive(Debug, Serialize)]
pub struct ReceiptGroup {
    pub origin: String,
    pub occurrence: CanonicalHash,
    pub giver: String,
    pub receiver: String,
    pub history: View,
    pub missing: Vec<CanonicalHash>,
    pub pending: Vec<Pending>,
    pub partial: bool,
}

impl ReceiptGroup {
    pub fn source_evidence(&self) -> SourceEvidence {
        SourceEvidence {
            origin: self.origin.clone(),
            occurrence: self.occurrence.clone(),
            giver: self.giver.clone(),
            receiver: self.receiver.clone(),
            copies: self
                .history
                .outcomes
                .iter()
                .flat_map(|outcome| outcome.copies.iter().cloned())
                .collect(),
            partial: self.partial,
        }
    }
}

/// Assemble signed receipt evidence in source order, retaining missing and invalid proofs.
///
/// # Errors
///
/// An input is invalid, is not an original public proof, has conflicting bytes,
/// or the assembly exceeds its document, event, or byte budget.
pub fn receipts(inputs: &[Receipt<'_>], now: i64) -> Result<Vec<ReceiptGroup>, String> {
    if inputs.len() > MAX_DISCLOSURES {
        return Err("Receipt assembly exceeds its document budget".into());
    }
    let mut bytes = 0usize;
    let mut events = 0usize;
    let mut sources: BTreeMap<_, Vec<_>> = BTreeMap::new();
    for input in inputs {
        input.document.validate(now)?;
        let bundle = input
            .document
            .public_bundle()
            .ok_or("Only original public receipt proofs can be assembled")?;
        bytes =
            bytes.saturating_add(encoded_len(input.document).map_err(|error| error.to_string())?);
        events = events.saturating_add(bundle.publication.events.len());
        if bytes > MAX_BYTES || events > MAX_DISCLOSURES {
            return Err("Receipt assembly exceeds its byte or event budget".into());
        }
        let source = &bundle.publication.scope.source;
        sources
            .entry((&source.origin, &source.occurrence))
            .or_default()
            .push((&input.provenance, bundle));
    }
    let mut result = Vec::with_capacity(sources.len());
    for ((origin, occurrence), inputs) in sources {
        let Some((_, first)) = inputs.first() else {
            continue;
        };
        let first_scope = &first.publication.scope;
        let mut scopes: BTreeMap<_, Vec<_>> = BTreeMap::new();
        for (provenance, bundle) in inputs {
            scopes
                .entry(bundle.publication.scope.id()?)
                .or_default()
                .push((provenance, bundle));
        }
        let mut copies = Vec::new();
        let mut missing = BTreeSet::new();
        let mut pending = Vec::new();
        for (scope_id, inputs) in scopes {
            let Some((_, first)) = inputs.first() else {
                continue;
            };
            let scope = &first.publication.scope;
            let events = index_events(inputs)?;
            let resolution = resolve_all(scope, &scope_id, &events, now)?;
            for projection in resolution.projections {
                let proof = events
                    .get(&projection.event)
                    .ok_or("Original event is unavailable")?;
                copies.extend(proof.additional.iter().map(|&provenance| {
                    Copy::from_resolved(scope, proof.event, projection.clone(), provenance.clone())
                }));
                copies.push(Copy::from_resolved(
                    scope,
                    proof.event,
                    projection,
                    proof.first.clone(),
                ));
            }
            missing.extend(resolution.missing);
            pending.extend(resolution.unresolved.into_iter().map(|issue| Pending {
                scope: scope_id.clone(),
                event: issue.event,
                state: issue.state,
                reason: issue.reason,
            }));
        }
        result.push(ReceiptGroup {
            origin: origin.clone(),
            occurrence: occurrence.clone(),
            giver: first_scope.giver.clone(),
            receiver: first_scope.receiver.clone(),
            history: receipt_history::merge(copies),
            partial: !missing.is_empty() || !pending.is_empty(),
            missing: missing.into_iter().collect(),
            pending,
        });
    }
    Ok(result)
}

struct Proofs<'a> {
    event: &'a Event,
    first: &'a Provenance,
    additional: Vec<&'a Provenance>,
}

impl AsRef<Event> for Proofs<'_> {
    fn as_ref(&self) -> &Event {
        self.event
    }
}

fn index_events<'a>(
    inputs: Vec<(&'a Provenance, &'a Bundle)>,
) -> Result<BTreeMap<CanonicalHash, Proofs<'a>>, String> {
    use std::collections::btree_map::Entry;

    let mut events = BTreeMap::new();
    for (provenance, bundle) in inputs {
        for signed in &bundle.publication.events {
            let event = &signed.event;
            match events.entry(event.id()?) {
                Entry::Vacant(slot) => {
                    slot.insert(Proofs {
                        event,
                        first: provenance,
                        additional: Vec::new(),
                    });
                }
                Entry::Occupied(mut slot) => {
                    let proof = slot.get_mut();
                    if proof.event != event {
                        return Err("Original event hashes have conflicting bytes".into());
                    }
                    proof.additional.push(provenance);
                }
            }
        }
    }
    Ok(events)
}
