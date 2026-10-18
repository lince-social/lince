use super::{
    receipt::{Content, Event, Publication, Scope},
    receipt_history::{self, Projection},
};
use crate::karma::CanonicalHash;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

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

pub fn references(publication: &Publication) -> Result<Vec<Reference>, String> {
    let now = publication
        .events
        .iter()
        .map(|signed| signed.event.issued_at)
        .chain(
            publication
                .approvals
                .iter()
                .map(|approval| approval.issued_at),
        )
        .chain(
            publication
                .person_approvals
                .iter()
                .map(|approval| approval.issued_at),
        )
        .max()
        .unwrap_or(publication.scope.issued_at);
    publication.validate_fragment(now)?;
    let scope = publication.scope.id()?;
    let source = receipt_history::source_key(
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
                need: if let Content::Statement { need, .. } = &event.content {
                    need.clone()
                } else {
                    None
                },
                issuer: publication.scope.organ(event.content.role()).into(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    rows.sort_by(|a, b| a.event.cmp(&b.event));
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

pub fn resolve(
    scope: &Scope,
    events: &BTreeMap<CanonicalHash, Event>,
    selected: &BTreeSet<CanonicalHash>,
    now: i64,
) -> Result<Resolution, String> {
    if events.len() > 5000 || selected.len() > 5000 {
        return Err("Proof assembly exceeds its event budget".into());
    }
    scope.validate()?;
    let scope_id = scope.id()?;
    let mut states = BTreeMap::new();
    let mut reasons = BTreeMap::new();
    let mut missing = BTreeSet::new();
    let mut pending = BTreeMap::new();
    let mut children: BTreeMap<CanonicalHash, Vec<CanonicalHash>> = BTreeMap::new();
    let mut ready = VecDeque::new();
    let mut relevant = BTreeSet::new();
    let mut walk = selected.iter().cloned().collect::<Vec<_>>();
    while let Some(id) = walk.pop() {
        if !relevant.insert(id.clone()) {
            continue;
        }
        if let Some(event) = events.get(&id) {
            walk.extend(event.supersedes.iter().cloned());
            if let Content::Statement { need, .. } = &event.content {
                walk.extend(need.iter().cloned());
            }
        }
    }
    for (id, event) in events.iter().filter(|(id, _)| relevant.contains(*id)) {
        let mut state = State::Complete;
        let mut reason = String::new();
        if event.id()? != *id || event.scope != scope_id || event.validate(scope, now).is_err() {
            state = State::Invalid;
            reason = "Event differs from its approved scope or identifier".into();
        }
        let mut dependencies = BTreeSet::new();
        for (target, is_need) in event.supersedes.iter().map(|parent| (parent, false)).chain(
            match &event.content {
                Content::Statement { need, .. } => need.as_ref(),
                _ => None,
            }
            .into_iter()
            .map(|need| (need, true)),
        ) {
            if let Some(previous) = events.get(target) {
                let same_scope = previous.scope == event.scope;
                let valid = if is_need {
                    matches!(&previous.content,Content::Need {measurement,..} if Some(measurement)==event.content.measurement())
                } else {
                    previous.slot(scope)? == event.slot(scope)?
                };
                if !same_scope || !valid || previous.issued_at > event.issued_at {
                    state = State::Invalid;
                    reason="Predecessor or Need proof changes the scope, role, measurement or publication order".into();
                }
                dependencies.insert(target.clone());
            } else {
                missing.insert(target.clone());
                if state != State::Invalid {
                    state = State::Incomplete;
                    reason = "A predecessor or receiver-authored Need proof is missing".into();
                }
            }
        }
        pending.insert(id.clone(), dependencies.len());
        if dependencies.is_empty() {
            ready.push_back(id.clone());
        }
        for dependency in dependencies {
            children.entry(dependency).or_default().push(id.clone());
        }
        states.insert(id.clone(), state);
        reasons.insert(id.clone(), reason);
    }
    let mut processed = BTreeSet::new();
    while let Some(id) = ready.pop_front() {
        processed.insert(id.clone());
        let state = states[&id];
        for child in children.get(&id).into_iter().flatten() {
            if state == State::Invalid
                || (state == State::Incomplete && states[child] == State::Complete)
            {
                states.insert(child.clone(), state);
                reasons.insert(
                    child.clone(),
                    if state == State::Invalid {
                        "A predecessor or Need proof is invalid"
                    } else {
                        "The original dependency history is incomplete"
                    }
                    .into(),
                );
            }
            let count = pending
                .get_mut(child)
                .ok_or("Proof dependency is unavailable")?;
            *count -= 1;
            if *count == 0 {
                ready.push_back(child.clone());
            }
        }
    }
    let borrowed = events
        .iter()
        .map(|(id, event)| (id.clone(), event))
        .collect();
    let mut projections = Vec::new();
    let mut unresolved = Vec::new();
    for id in selected {
        if !events.contains_key(id) {
            missing.insert(id.clone());
            unresolved.push(Unresolved {
                event: id.clone(),
                state: State::Incomplete,
                reason: "Original event is unavailable".into(),
            });
        } else if !processed.contains(id) {
            unresolved.push(Unresolved {
                event: id.clone(),
                state: State::Invalid,
                reason: "Dependency history contains a cycle".into(),
            });
        } else if states[id] == State::Complete {
            projections.push(receipt_history::project_event(
                scope,
                &events[id],
                &borrowed,
            )?);
        } else {
            unresolved.push(Unresolved {
                event: id.clone(),
                state: states[id],
                reason: reasons[id].clone(),
            });
        }
    }
    Ok(Resolution {
        projections,
        unresolved,
        missing: missing.into_iter().collect(),
    })
}
