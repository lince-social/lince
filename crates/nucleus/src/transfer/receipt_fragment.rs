//! Index receipt references and resolve the evidence needed by selected events.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde::{Deserialize, Serialize};

use super::{
    receipt::{Content, Event, Publication, Scope},
    receipt_history::{Projection, Projector, source_key},
};
use crate::karma::CanonicalHash;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reference {
    pub event: CanonicalHash,
    pub scope: CanonicalHash,
    pub source: CanonicalHash,
    pub commitment: CanonicalHash,
    pub slot: CanonicalHash,
    pub parent: Option<CanonicalHash>,
    pub need: Option<CanonicalHash>,
    pub issuer: String,
}

/// Index the events in a valid fragment in event identifier order.
///
/// # Errors
///
/// The fragment violates its scope or publication policy, or cannot be encoded.
pub fn references(publication: &Publication) -> Result<Vec<Reference>, String> {
    publication.validate_fragment(publication.evidence_time())?;
    let scope = publication.scope.id()?;
    let source = source_key(
        &publication.scope.source.origin,
        &publication.scope.source.occurrence,
    )?;
    let mut rows = publication
        .events
        .iter()
        .map(|signed| {
            let event = &signed.event;
            Ok(Reference {
                event: event.id()?,
                scope: scope.clone(),
                source: source.clone(),
                commitment: event.source_commitment.clone(),
                slot: event.slot(&publication.scope)?,
                parent: event.supersedes.clone(),
                need: need(event).cloned(),
                issuer: publication.scope.organ(event.content.role()).into(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    rows.sort_unstable_by(|a, b| a.event.cmp(&b.event));
    Ok(rows)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Complete,
    Incomplete,
    Invalid,
}

#[derive(Debug, Serialize)]
pub struct Unresolved {
    pub event: CanonicalHash,
    pub state: State,
    pub reason: String,
}

#[derive(Debug, Serialize)]
pub struct Resolution {
    pub projections: Vec<Projection>,
    pub unresolved: Vec<Unresolved>,
    pub missing: Vec<CanonicalHash>,
}

fn need(event: &Event) -> Option<&CanonicalHash> {
    match &event.content {
        Content::Statement { need, .. } => need.as_ref(),
        _ => None,
    }
}

#[derive(Clone, Copy)]
enum Verdict {
    Complete,
    Incomplete(&'static str),
    Invalid(&'static str),
}

impl Verdict {
    fn inherit(&mut self, dependency: Self) {
        *self = match dependency {
            Self::Invalid(_) => Self::Invalid("A predecessor or Need proof is invalid"),
            Self::Incomplete(_) if matches!(self, Self::Complete) => {
                Self::Incomplete("The original dependency history is incomplete")
            }
            _ => *self,
        };
    }
}

struct Pending {
    verdict: Verdict,
    remaining: usize,
}

/// Assemble the ancestry and Need proofs of the selected events.
///
/// The results are in identifier order. Missing evidence and invalid branches are
/// reported with the events they affect. Unrelated evidence does not participate.
///
/// # Errors
///
/// The assembly exceeds its budget, the scope is invalid, or evidence cannot be encoded.
pub fn resolve(
    scope: &Scope,
    events: &BTreeMap<CanonicalHash, Event>,
    selected: &BTreeSet<CanonicalHash>,
    now: i64,
) -> Result<Resolution, String> {
    check_budget(events.len(), selected.len())?;
    resolve_selected(
        scope,
        &scope.id()?,
        selected.iter(),
        |id| events.get(id),
        now,
    )
}

/// Resolve a proof index with the scope ID already checked during grouping.
pub(crate) fn resolve_all<'a>(
    scope: &Scope,
    scope_id: &CanonicalHash,
    events: &'a BTreeMap<CanonicalHash, impl AsRef<Event> + 'a>,
    now: i64,
) -> Result<Resolution, String> {
    check_budget(events.len(), events.len())?;
    resolve_selected(
        scope,
        scope_id,
        events.keys(),
        |id| events.get(id).map(AsRef::as_ref),
        now,
    )
}

fn check_budget(events: usize, selected: usize) -> Result<(), String> {
    const MAX_EVENTS: usize = 5000;
    if events > MAX_EVENTS || selected > MAX_EVENTS {
        return Err("Proof assembly exceeds its event budget".into());
    }
    Ok(())
}

fn resolve_selected<'a>(
    scope: &Scope,
    scope_id: &CanonicalHash,
    selected: impl Iterator<Item = &'a CanonicalHash> + Clone,
    get_event: impl Fn(&CanonicalHash) -> Option<&'a Event>,
    now: i64,
) -> Result<Resolution, String> {
    let mut missing = BTreeSet::new();
    let mut pending = BTreeMap::new();
    let mut children = BTreeMap::<_, Vec<_>>::new();
    let mut ready = VecDeque::new();
    let mut relevant = BTreeSet::new();
    let mut walk = selected.clone().collect::<Vec<_>>();
    while let Some(id) = walk.pop() {
        if relevant.insert(id)
            && let Some(event) = get_event(id)
        {
            walk.extend(event.supersedes.as_ref());
            walk.extend(need(event));
        }
    }
    for id in relevant {
        let Some(event) = get_event(id) else {
            continue;
        };
        let mut verdict = Verdict::Complete;
        if event.id()? != *id || event.validate_for_scope(scope, scope_id, now).is_err() {
            verdict = Verdict::Invalid("Event differs from its approved scope or identifier");
        }
        let mut remaining = 0;
        for (target, is_need) in [(event.supersedes.as_ref(), false), (need(event), true)] {
            let Some(target) = target else { continue };
            if let Some(previous) = get_event(target) {
                let valid = if is_need {
                    matches!(&previous.content, Content::Need { measurement, .. }
                        if Some(measurement) == event.content.measurement())
                } else {
                    previous.slot(scope)? == event.slot(scope)?
                };
                if previous.scope != event.scope || !valid || previous.issued_at > event.issued_at {
                    verdict = Verdict::Invalid(
                        "Predecessor or Need proof changes the scope, role, measurement or publication order",
                    );
                }
                if !is_need || event.supersedes.as_ref() != Some(target) {
                    children.entry(target).or_default().push(id);
                    remaining += 1;
                }
            } else {
                missing.insert(target);
                if !matches!(verdict, Verdict::Invalid(_)) {
                    verdict = Verdict::Incomplete(
                        "A predecessor or receiver-authored Need proof is missing",
                    );
                }
            }
        }
        pending.insert(id, Pending { verdict, remaining });
        if remaining == 0 {
            ready.push_back(id);
        }
    }
    while let Some(id) = ready.pop_front() {
        let verdict = pending
            .get(id)
            .ok_or("Proof dependency is unavailable")?
            .verdict;
        for child in children.get(id).into_iter().flatten() {
            let child = *child;
            let node = pending
                .get_mut(child)
                .ok_or("Proof dependency is unavailable")?;
            node.verdict.inherit(verdict);
            node.remaining -= 1;
            if node.remaining == 0 {
                ready.push_back(child);
            }
        }
    }
    let mut projector = None;
    let mut projections = Vec::new();
    let mut unresolved = Vec::new();
    for id in selected {
        let verdict = match pending.get(id) {
            None => {
                missing.insert(id);
                Verdict::Incomplete("Original event is unavailable")
            }
            Some(node) if node.remaining == 0 => node.verdict,
            Some(_) => Verdict::Invalid("Dependency history contains a cycle"),
        };
        let (state, reason) = match verdict {
            Verdict::Complete => {
                let projector = projector
                    .get_or_insert_with(|| Projector::new(scope))
                    .as_ref()
                    .map_err(Clone::clone)?;
                let event = get_event(id).ok_or("Original event is unavailable")?;
                projections.push(projector.event(id, event, &get_event)?);
                continue;
            }
            Verdict::Incomplete(reason) => (State::Incomplete, reason),
            Verdict::Invalid(reason) => (State::Invalid, reason),
        };
        unresolved.push(Unresolved {
            event: id.clone(),
            state,
            reason: reason.into(),
        });
    }
    Ok(Resolution {
        projections,
        unresolved,
        missing: missing.into_iter().cloned().collect(),
    })
}
