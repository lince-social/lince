use super::receipt::{Content, PartyAttribution, Publication, Scope};
use crate::karma::{CanonicalHash, canonical_hash};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

fn hash<T: Serialize>(domain: &str, value: &T) -> Result<CanonicalHash, String> {
    canonical_hash(domain, value).map_err(|error| error.to_string())
}

pub fn source_key(origin: &str, occurrence: &CanonicalHash) -> Result<CanonicalHash, String> {
    hash("lince.transfer.history.source.v1", &(origin, occurrence))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Projection {
    pub event: CanonicalHash,
    pub source: CanonicalHash,
    pub binding: CanonicalHash,
    pub slot: CanonicalHash,
    pub outcome: CanonicalHash,
    pub semantic: CanonicalHash,
    pub parent: Option<CanonicalHash>,
    pub need: Option<CanonicalHash>,
    pub issuer: String,
}

fn outcome(scope: &Scope, event: &super::receipt::Event) -> Result<CanonicalHash, String> {
    hash(
        "lince.transfer.history.outcome.v1",
        &(
            &scope.source.origin,
            &scope.source.occurrence,
            scope.organ(event.content.role()),
            event.content.role(),
            event.content.stage(),
            &event.source_commitment,
        ),
    )
}

pub fn project(publication: &Publication) -> Result<Vec<Projection>, String> {
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
    publication.validate(now)?;
    let scope = &publication.scope;
    let events = publication
        .events
        .iter()
        .map(|signed| Ok((signed.event.id()?, &signed.event)))
        .collect::<Result<BTreeMap<_, _>, String>>()?;
    events
        .values()
        .map(|event| project_event(scope, event, &events))
        .collect()
}

pub(crate) fn project_event(
    scope: &Scope,
    event: &super::receipt::Event,
    events: &BTreeMap<CanonicalHash, &super::receipt::Event>,
) -> Result<Projection, String> {
    let source = source_key(&scope.source.origin, &scope.source.occurrence)?;
    let binding = hash(
        "lince.transfer.history.source-binding.v1",
        &(&scope.source, &scope.giver, &scope.receiver),
    )?;
    let mut content = event.content.clone();
    let need = if let Content::Statement { need, .. } = &mut content {
        need.take()
    } else {
        None
    };
    let resolve = |id: &CanonicalHash| {
        outcome(
            scope,
            events.get(id).ok_or("History dependency is missing")?,
        )
    };
    Ok(Projection {
        event: event.id()?,
        source: source.clone(),
        binding: binding.clone(),
        slot: event.slot(scope)?,
        outcome: outcome(scope, event)?,
        semantic: hash(
            "lince.transfer.history.semantic.v1",
            &(event.recorded_at, content),
        )?,
        parent: event.supersedes.as_ref().map(resolve).transpose()?,
        need: need.as_ref().map(resolve).transpose()?,
        issuer: scope.organ(event.content.role()).into(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(tag = "provenance", rename_all = "snake_case")]
pub enum Provenance {
    Record { record: String, version: u64 },
    Artifact { hash: CanonicalHash },
}

impl Provenance {
    pub fn record(&self) -> Option<&str> {
        match self {
            Self::Record { record, .. } => Some(record),
            Self::Artifact { .. } => None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Copy {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub persons: Vec<super::receipt::PublicPerson>,
    #[serde(flatten)]
    pub provenance: Provenance,
    pub scope: CanonicalHash,
    pub attribution: PartyAttribution,
    pub projection: Projection,
    pub content: Content,
    pub recorded_at: i64,
    pub canonical_unit: Option<crate::vocabulary::reference::MeaningReference>,
    pub agreed_quantity: Option<super::receipt::RecordedFloat>,
}

pub fn project_copy(
    scope: &Scope,
    event: &super::receipt::Event,
    projection: &Projection,
    provenance: Provenance,
) -> Result<Copy, String> {
    if projection.event != event.id()? || event.scope != scope.id()? {
        return Err("The projected event differs from its original proof".into());
    }
    Ok(Copy {
        persons: scope.public_persons.clone(),
        provenance,
        scope: event.scope.clone(),
        attribution: scope.party_attribution,
        projection: projection.clone(),
        content: event.content.clone(),
        recorded_at: event.recorded_at,
        canonical_unit: scope.canonical_unit.clone(),
        agreed_quantity: scope.agreed_quantity.clone(),
    })
}

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
    pub person_conflicts: Vec<super::receipt::PublicPerson>,
    pub source_conflict: bool,
    pub bindings: Vec<CanonicalHash>,
    pub outcomes: Vec<Outcome>,
    pub slots: Vec<Slot>,
}

pub fn merge(copies: Vec<Copy>) -> View {
    let mut persons: BTreeMap<super::receipt::ApprovalRole, BTreeSet<String>> = BTreeMap::new();
    let mut grouped: BTreeMap<CanonicalHash, Vec<Copy>> = BTreeMap::new();
    let mut bindings = BTreeSet::new();
    let mut units = Vec::new();
    let mut quantities = BTreeSet::new();
    for copy in copies {
        for target in &copy.persons {
            persons
                .entry(target.role)
                .or_default()
                .insert(target.person.clone());
        }
        if let Some(unit) = &copy.canonical_unit
            && !units.contains(unit)
        {
            units.push(unit.clone());
        }
        if let Some(quantity) = &copy.agreed_quantity {
            quantities.insert(quantity.ieee754_binary64.clone());
        }
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
        let semantics = copies
            .iter()
            .map(|copy| &copy.projection.semantic)
            .collect::<BTreeSet<_>>();
        let previous = copies
            .iter()
            .map(|copy| copy.projection.parent.clone())
            .collect::<BTreeSet<_>>();
        let needs = copies
            .iter()
            .filter_map(|copy| copy.projection.need.as_ref())
            .collect::<BTreeSet<_>>();
        let slots = copies
            .iter()
            .map(|copy| copy.projection.slot.clone())
            .collect::<BTreeSet<_>>();
        let conflicted =
            semantics.len() > 1 || previous.len() > 1 || needs.len() > 1 || slots.len() > 1;
        if !conflicted && let Some(Some(parent)) = previous.first() {
            parents.insert(id.clone(), parent.clone());
        }
        outcomes.insert(
            id.clone(),
            Outcome {
                id,
                slots: slots.into_iter().collect(),
                conflicted,
                copies,
            },
        );
    }
    let mut done = BTreeSet::new();
    let mut cycles = BTreeSet::new();
    for start in outcomes.keys() {
        let mut path = Vec::new();
        let mut positions = BTreeMap::new();
        let mut cursor = start;
        while !done.contains(cursor) {
            if let Some(index) = positions.get(cursor) {
                cycles.extend(path[*index..].iter().cloned());
                break;
            }
            positions.insert(cursor.clone(), path.len());
            path.push(cursor.clone());
            let Some(parent) = parents.get(cursor) else {
                break;
            };
            cursor = parent;
        }
        done.extend(path);
    }
    for id in cycles {
        if let Some(outcome) = outcomes.get_mut(&id) {
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
                .map(move |person| super::receipt::PublicPerson { role, person })
        })
        .collect::<Vec<_>>();
    let source_conflict = bindings.len() > 1
        || units.len() > 1
        || quantities.len() > 1
        || !person_conflicts.is_empty();
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

#[cfg(test)]
mod tests;
