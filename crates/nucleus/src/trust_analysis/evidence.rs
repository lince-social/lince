use super::{Identity, claims};
use crate::{
    karma::CanonicalHash,
    transfer::{
        receipt, receipt_fragment,
        receipt_history::{self, Provenance},
    },
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

pub struct Receipt<'a> {
    pub document: &'a receipt::Document,
    pub provenance: Provenance,
}

#[derive(Debug, Serialize)]
pub struct Pending {
    pub scope: CanonicalHash,
    pub event: CanonicalHash,
    pub state: receipt_fragment::State,
    pub reason: String,
}

#[derive(Debug, Serialize)]
pub struct ReceiptGroup {
    pub origin: String,
    pub occurrence: CanonicalHash,
    pub giver: String,
    pub receiver: String,
    pub history: receipt_history::View,
    pub missing: Vec<CanonicalHash>,
    pub pending: Vec<Pending>,
    pub partial: bool,
}

impl ReceiptGroup {
    pub fn source_evidence(&self) -> super::query::SourceEvidence {
        super::query::SourceEvidence {
            origin: self.origin.clone(),
            occurrence: self.occurrence.clone(),
            giver: self.giver.clone(),
            receiver: self.receiver.clone(),
            copies: self.history.outcomes.iter().flat_map(|outcome| outcome.copies.iter().cloned()).collect(),
            partial: self.partial,
        }
    }
}

pub fn receipts(inputs: &[Receipt<'_>], now: i64) -> Result<Vec<ReceiptGroup>, String> {
    if inputs.len() > super::MAX_DISCLOSURES {
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
        bytes = bytes.saturating_add(
            serde_json::to_vec(input.document)
                .map_err(|e| e.to_string())?
                .len(),
        );
        events = events.saturating_add(bundle.publication.events.len());
        if bytes > super::MAX_BYTES || events > super::MAX_DISCLOSURES {
            return Err("Receipt assembly exceeds its byte or event budget".into());
        }
        let source = &bundle.publication.scope.source;
        sources
            .entry((source.origin.clone(), source.occurrence.clone()))
            .or_default()
            .push(input);
    }
    let mut result = Vec::new();
    for ((origin, occurrence), inputs) in sources {
        let first = &inputs[0].document.public_bundle().unwrap().publication.scope;
        let giver = first.giver.clone();
        let receiver = first.receiver.clone();
        let mut scopes: BTreeMap<_, Vec<_>> = BTreeMap::new();
        for input in inputs {
            let bundle = input.document.public_bundle().unwrap();
            scopes
                .entry(bundle.publication.scope.id()?)
                .or_default()
                .push(input);
        }
        let mut copies = Vec::new();
        let mut missing = BTreeSet::new();
        let mut pending = Vec::new();
        for (scope_id, inputs) in scopes {
            let scope = &inputs[0]
                .document
                .public_bundle()
                .unwrap()
                .publication
                .scope;
            let mut events = BTreeMap::new();
            for input in &inputs {
                for signed in &input.document.public_bundle().unwrap().publication.events {
                    let id = signed.event.id()?;
                    if events
                        .get(&id)
                        .is_some_and(|previous| previous != &signed.event)
                    {
                        return Err("Original event hashes have conflicting bytes".into());
                    }
                    events.insert(id, signed.event.clone());
                }
            }
            let resolution =
                receipt_fragment::resolve(scope, &events, &events.keys().cloned().collect(), now)?;
            let projections = resolution
                .projections
                .iter()
                .map(|projection| (&projection.event, projection))
                .collect::<BTreeMap<_, _>>();
            for input in &inputs {
                for signed in &input.document.public_bundle().unwrap().publication.events {
                    if let Some(projection) = projections.get(&signed.event.id()?) {
                        copies.push(receipt_history::project_copy(
                            scope,
                            &signed.event,
                            projection,
                            input.provenance.clone(),
                        )?);
                    }
                }
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
            origin,
            occurrence,
            giver,
            receiver,
            history: receipt_history::merge(copies),
            partial: !missing.is_empty() || !pending.is_empty(),
            missing: missing.into_iter().collect(),
            pending,
        });
    }
    Ok(result)
}

pub struct Claim<'a> {
    pub document: &'a claims::Document,
    pub provenance: Option<Provenance>,
    pub authority_current: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HeadState {
    CurrentObserved,
    Withdrawn,
    Expired,
    Conflicted,
    Withheld,
}

#[derive(Debug, Serialize)]
pub struct ClaimHead {
    pub issuer: Identity,
    pub claim: String,
    pub revision: Option<u64>,
    pub state: HeadState,
    pub originals: Vec<Provenance>,
}

pub fn claim_heads(inputs: &[Claim<'_>], now: i64) -> Result<Vec<ClaimHead>, String> {
    if inputs.len() > super::MAX_DISCLOSURES {
        return Err("Claim controls exceed their work budget".into());
    }
    let mut bytes = 0usize;
    let mut series: BTreeMap<_, Vec<_>> = BTreeMap::new();
    for input in inputs {
        input.document.validate()?;
        bytes = bytes.saturating_add(
            serde_json::to_vec(input.document)
                .map_err(|e| e.to_string())?
                .len(),
        );
        if bytes > super::MAX_BYTES {
            return Err("Claim controls exceed their byte budget".into());
        }
        let statement = &input.document.statement;
        if statement.issued_at <= now {
            series
                .entry((statement.issuer.clone(), statement.claim.clone()))
                .or_default()
                .push(input);
        }
    }
    let mut result = Vec::new();
    for ((issuer, claim), versions) in series {
        if !versions.iter().any(|input| input.provenance.is_some()) {
            continue;
        }
        let revision = versions
            .iter()
            .map(|input| input.document.statement.revision)
            .max()
            .unwrap();
        let current = versions
            .into_iter()
            .filter(|input| input.document.statement.revision == revision)
            .collect::<Vec<_>>();
        let ids = current
            .iter()
            .map(|input| input.document.statement.id())
            .collect::<Result<BTreeSet<_>, _>>()?;
        let visible = current
            .iter()
            .filter(|input| input.provenance.is_some())
            .collect::<Vec<_>>();
        let originals = visible
            .iter()
            .filter_map(|input| input.provenance.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let statement = &current[0].document.statement;
        let state = if visible.is_empty() || !visible.iter().any(|input| input.authority_current) {
            HeadState::Withheld
        } else if ids.len() != 1 {
            HeadState::Conflicted
        } else if statement.withdrawn {
            HeadState::Withdrawn
        } else if statement.expires_at <= now {
            HeadState::Expired
        } else {
            HeadState::CurrentObserved
        };
        result.push(ClaimHead {
            issuer,
            claim,
            revision: (!visible.is_empty()).then_some(revision),
            state,
            originals,
        });
    }
    Ok(result)
}
