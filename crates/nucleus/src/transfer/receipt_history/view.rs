//! Merge multiple proof copies while retaining conflicting outcomes and correction heads.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use super::super::receipt::{ApprovalRole, PublicPerson};
use super::Copy;
use crate::karma::CanonicalHash;

#[derive(Debug, Serialize)]
pub struct Outcome {
    pub id: CanonicalHash,
    pub slots: Vec<CanonicalHash>,
    pub conflicted: bool,
    pub copies: Vec<Copy>,
}

#[derive(Debug, Serialize)]
pub struct Slot {
    pub id: CanonicalHash,
    pub heads: Vec<CanonicalHash>,
    pub conflicted: bool,
}

#[derive(Debug, Serialize)]
pub struct View {
    pub person_conflicts: Vec<PublicPerson>,
    pub source_conflict: bool,
    pub bindings: Vec<CanonicalHash>,
    pub outcomes: Vec<Outcome>,
    pub slots: Vec<Slot>,
}

/// Combine proof copies by outcome and retain competing correction heads.
pub fn merge(copies: Vec<Copy>) -> View {
    let mut persons: BTreeMap<ApprovalRole, BTreeSet<String>> = BTreeMap::new();
    let mut grouped: BTreeMap<CanonicalHash, Vec<Copy>> = BTreeMap::new();
    let mut bindings = BTreeSet::new();
    let mut unit = None;
    let mut quantity = None;
    let mut source_conflict = false;
    for copy in copies {
        for target in &copy.persons {
            persons
                .entry(target.role)
                .or_default()
                .insert(target.person.clone());
        }
        source_conflict |= differs(&mut unit, copy.canonical_unit.as_ref());
        source_conflict |= differs(
            &mut quantity,
            copy.agreed_quantity
                .as_ref()
                .map(|value| &value.ieee754_binary64),
        );
        bindings.insert(copy.projection.binding.clone());
        grouped
            .entry(copy.projection.outcome.clone())
            .or_default()
            .push(copy);
    }
    let mut parents = BTreeMap::new();
    let mut outcomes = BTreeMap::new();
    for (id, mut copies) in grouped {
        copies.sort_by(|a, b| {
            (&a.provenance, &a.projection.event).cmp(&(&b.provenance, &b.projection.event))
        });
        copies.dedup_by(|a, b| {
            a.provenance == b.provenance && a.projection.event == b.projection.event
        });
        let Some((first, rest)) = copies.split_first() else {
            continue;
        };
        let mut needs = copies
            .iter()
            .filter_map(|copy| copy.projection.need.as_ref());
        let slots = copies
            .iter()
            .map(|copy| &copy.projection.slot)
            .collect::<BTreeSet<_>>();
        let conflicted = slots.len() > 1
            || needs
                .next()
                .is_some_and(|need| needs.any(|other| other != need))
            || rest.iter().any(|copy| {
                copy.projection.semantic != first.projection.semantic
                    || copy.projection.parent != first.projection.parent
            });
        if !conflicted && let Some(parent) = &first.projection.parent {
            parents.insert(id.clone(), parent.clone());
        }
        let slots = slots.into_iter().cloned().collect();
        outcomes.insert(
            id.clone(),
            Outcome {
                id,
                slots,
                conflicted,
                copies,
            },
        );
    }
    for id in cycle_members(&parents) {
        if let Some(outcome) = outcomes.get_mut(id) {
            outcome.conflicted = true;
        }
    }
    let mut slots: BTreeMap<CanonicalHash, BTreeSet<CanonicalHash>> = BTreeMap::new();
    for outcome in outcomes.values() {
        for slot in &outcome.slots {
            slots
                .entry(slot.clone())
                .or_default()
                .insert(outcome.id.clone());
        }
    }
    for (id, parent) in parents {
        if let (Some(child), Some(previous)) = (outcomes.get(&id), outcomes.get(&parent))
            && !child.conflicted
            && !previous.conflicted
        {
            for slot in child
                .slots
                .iter()
                .filter(|slot| previous.slots.contains(slot))
            {
                if let Some(heads) = slots.get_mut(slot) {
                    heads.remove(&parent);
                }
            }
        }
    }
    let person_conflicts = persons
        .into_iter()
        .filter(|(_, persons)| persons.len() > 1)
        .flat_map(|(role, persons)| {
            persons
                .into_iter()
                .map(move |person| PublicPerson { role, person })
        })
        .collect::<Vec<_>>();
    source_conflict |= bindings.len() > 1 || !person_conflicts.is_empty();
    let slots = slots
        .into_iter()
        .map(|(id, heads)| Slot {
            conflicted: source_conflict
                || heads.len() > 1
                || heads
                    .iter()
                    .any(|head| outcomes.get(head).is_some_and(|outcome| outcome.conflicted)),
            id,
            heads: heads.into_iter().collect(),
        })
        .collect();
    View {
        person_conflicts,
        source_conflict,
        bindings: bindings.into_iter().collect(),
        outcomes: outcomes.into_values().collect(),
        slots,
    }
}

fn differs<T: Clone + Eq>(first: &mut Option<T>, value: Option<&T>) -> bool {
    value.is_some_and(|value| &*first.get_or_insert_with(|| value.clone()) != value)
}

fn cycle_members(parents: &BTreeMap<CanonicalHash, CanonicalHash>) -> Vec<&CanonicalHash> {
    let mut done = BTreeSet::new();
    let mut cycles = Vec::new();
    for start in parents.keys() {
        let mut path = Vec::new();
        let mut positions = BTreeMap::new();
        let mut cursor = start;
        while !done.contains(cursor) {
            if let Some(index) = positions.insert(cursor, path.len()) {
                cycles.extend(path.iter().skip(index).copied());
                break;
            }
            path.push(cursor);
            let Some(parent) = parents.get(cursor) else {
                break;
            };
            cursor = parent;
        }
        done.extend(path);
    }
    cycles
}
