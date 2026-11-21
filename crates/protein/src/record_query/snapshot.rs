use std::collections::{BTreeMap, BTreeSet, VecDeque};

use nucleus::{RecordKind, valid_uid, vocabulary::parse_meaning_set};

use crate::authority::{AssertionRole, AssertionState, GraphSnapshot, RecordState};

use super::{
    QueryError, ReadableIdentities, RecordScalarRow,
    budget::{Budget, within},
};

pub(super) struct Prepared<'a> {
    pub(super) records: BTreeMap<&'a str, &'a RecordState>,
    pub(super) rows: BTreeMap<&'a str, &'a RecordScalarRow>,
    pub(super) readable: &'a ReadableIdentities,
    pub(super) assertions: Vec<&'a AssertionState>,
    resolved_links: &'a BTreeMap<String, String>,
    concept_children: BTreeMap<&'a str, BTreeSet<&'a str>>,
    pub(super) identity_concepts: BTreeMap<&'a str, &'a str>,
}

impl<'a> Prepared<'a> {
    pub(super) fn new(
        graph: &'a GraphSnapshot,
        readable: &'a ReadableIdentities,
        scalars: &'a [RecordScalarRow],
        budget: &mut Budget<'_>,
    ) -> Result<Self, QueryError> {
        let limits = budget.limits;
        within(graph.records.len(), limits.records)?;
        within(graph.concepts.len(), limits.concepts)?;
        within(graph.places.len(), limits.places)?;
        within(graph.assertions.len(), limits.assertions)?;
        within(readable.records.len(), limits.records)?;
        within(readable.concepts.len(), limits.concepts)?;
        within(readable.places.len(), limits.places)?;
        within(readable.assertions.len(), limits.assertions)?;
        within(scalars.len(), limits.records)?;

        let mut records = BTreeMap::new();
        for record in &graph.records {
            let uid = charge_uid(&record.uid, "r", budget)?;
            if records.insert(uid, record).is_some() {
                return Err(QueryError::InvalidInput);
            }
            if let Some(organ) = &record.organ_uid {
                budget.text(organ)?;
            }
            if let Some(content) = &record.content {
                budget.text(&content.head)?;
                budget.text(&content.body)?;
                for text in [&content.slug, &content.unit_uid, &content.place_uid]
                    .into_iter()
                    .flatten()
                {
                    budget.text(text)?;
                }
                for (property, value) in &content.extensions {
                    budget.step()?;
                    budget.text(&property.namespace)?;
                    budget.text(&property.field)?;
                    if property.namespace.is_empty() || property.field.is_empty() {
                        return Err(QueryError::InvalidInput);
                    }
                    budget.value(value)?;
                }
            }
        }

        let mut concepts = BTreeMap::new();
        for concept in &graph.concepts {
            let uid = charge_uid(&concept.uid, "c", budget)?;
            budget.text(&concept.name)?;
            if concepts.insert(uid, concept).is_some() {
                return Err(QueryError::InvalidInput);
            }
        }
        for place in &graph.places {
            charge_uid(place, "pl", budget)?;
        }

        for record in records.values() {
            if let Some(organ) = &record.organ_uid {
                budget.edge()?;
                let target = records
                    .get(organ.as_str())
                    .ok_or(QueryError::InvalidInput)?;
                if target.kind != RecordKind::Organ {
                    return Err(QueryError::InvalidInput);
                }
            }
            if let Some(content) = &record.content {
                if let Some(unit) = &content.unit_uid {
                    budget.edge()?;
                    canonical(unit, "c")?;
                    if !concepts.contains_key(unit.as_str()) {
                        return Err(QueryError::InvalidInput);
                    }
                }
                if let Some(place) = &content.place_uid {
                    budget.edge()?;
                    canonical(place, "pl")?;
                    if !graph.places.contains(place) {
                        return Err(QueryError::InvalidInput);
                    }
                }
            }
        }
        for concept in concepts.values() {
            for parent in &concept.parents {
                budget.edge()?;
                canonical(parent, "c")?;
                if !concepts.contains_key(parent.as_str()) {
                    return Err(QueryError::InvalidInput);
                }
            }
        }

        let mut indexed_assertions = BTreeMap::new();
        let mut assertion_tuples = BTreeSet::new();
        let mut identity_subjects = BTreeSet::new();
        for assertion in &graph.assertions {
            budget.step()?;
            budget.edge()?;
            let uid = canonical(&assertion.uid, "a")?;
            let subject = canonical(&assertion.subject_uid, "r")?;
            let predicate = canonical(&assertion.predicate_uid, "c")?;
            for text in [uid, subject, predicate] {
                budget.text(text)?;
            }
            let object = assertion.object_uid.as_deref();
            let uri = assertion.object_uri.as_deref();
            if indexed_assertions.insert(uid, assertion).is_some()
                || !assertion_tuples.insert((subject, predicate, object, uri))
                || !records.contains_key(subject)
                || !concepts.contains_key(predicate)
            {
                return Err(QueryError::InvalidInput);
            }
            if let Some(object) = object {
                canonical(object, "r")?;
                budget.text(object)?;
                if !records.contains_key(object) {
                    return Err(QueryError::InvalidInput);
                }
            }
            if let Some(unit) = &assertion.unit_uid {
                canonical(unit, "c")?;
                budget.text(unit)?;
                if !concepts.contains_key(unit.as_str()) {
                    return Err(QueryError::InvalidInput);
                }
            }
            if assertion.role == AssertionRole::Identity
                && (object.is_some()
                    || uri.is_some()
                    || assertion.quantity.is_some()
                    || assertion.unit_uid.is_some()
                    || !identity_subjects.insert(subject))
            {
                return Err(QueryError::InvalidInput);
            }
        }

        validate_readable(&readable.records, "r", budget, |uid| {
            records.get(uid).is_some_and(|record| !record.deleted)
        })?;
        validate_readable(&readable.concepts, "c", budget, |uid| {
            concepts.contains_key(uid)
        })?;
        validate_readable(&readable.places, "pl", budget, |uid| {
            graph.places.contains(uid)
        })?;
        validate_readable(&readable.assertions, "a", budget, |uid| {
            indexed_assertions.contains_key(uid)
        })?;

        within(graph.resolved_links.len(), budget.limits.assertions)?;
        for (uid, target) in &graph.resolved_links {
            budget.step()?;
            budget.text(uid)?;
            budget.text(target)?;
            let assertion = indexed_assertions
                .get(uid.as_str())
                .ok_or(QueryError::InvalidInput)?;
            let object = records
                .get(target.as_str())
                .ok_or(QueryError::InvalidInput)?;
            let subject = records
                .get(assertion.subject_uid.as_str())
                .ok_or(QueryError::InvalidInput)?;
            if assertion.object_uri.is_none()
                || assertion.object_uid.is_some()
                || object.organ_uid != subject.organ_uid
            {
                return Err(QueryError::InvalidInput);
            }
        }
        let mut assertions = Vec::with_capacity(readable.assertions.len());
        let mut identity_concepts = BTreeMap::new();
        for uid in &readable.assertions {
            let assertion = indexed_assertions
                .get(uid.as_str())
                .copied()
                .ok_or(QueryError::InvalidInput)?;
            if !readable.records.contains(&assertion.subject_uid)
                || !readable.concepts.contains(&assertion.predicate_uid)
                || assertion
                    .object_uid
                    .as_ref()
                    .is_some_and(|object| !readable.records.contains(object))
                || assertion
                    .unit_uid
                    .as_ref()
                    .is_some_and(|unit| !readable.concepts.contains(unit))
            {
                return Err(QueryError::InvalidInput);
            }
            if assertion.role == AssertionRole::Identity {
                // The full snapshot rejects more than one identity per subject.
                identity_concepts.insert(
                    assertion.subject_uid.as_str(),
                    assertion.predicate_uid.as_str(),
                );
            }
            assertions.push(assertion);
        }

        let mut concept_children: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
        for uid in &readable.concepts {
            let concept = concepts
                .get(uid.as_str())
                .copied()
                .ok_or(QueryError::InvalidInput)?;
            for parent in &concept.parents {
                if readable.concepts.contains(parent) {
                    concept_children
                        .entry(parent.as_str())
                        .or_default()
                        .insert(concept.uid.as_str());
                }
            }
        }
        acyclic(
            readable.concepts.iter().map(String::as_str),
            &concept_children,
            budget,
        )?;

        let mut rows = BTreeMap::new();
        for row in scalars {
            let uid = charge_uid(&row.uid, "r", budget)?;
            budget.text(&row.head)?;
            budget.text(&row.body)?;
            if let Some(slug) = &row.slug {
                budget.text(slug)?;
            }
            if rows.insert(uid, row).is_some() {
                return Err(QueryError::InvalidInput);
            }
            let record = records.get(uid).ok_or(QueryError::InvalidInput)?;
            if !readable.records.contains(uid) || row.kind != record.kind {
                return Err(QueryError::InvalidInput);
            }
            if let Some(content) = &record.content
                && (row.slug != content.slug
                    || row.head != content.head
                    || row.body != content.body
                    || row.quantity != content.quantity)
            {
                return Err(QueryError::InvalidInput);
            }
        }
        // Every row above was shown to have a distinct, readable Record UID.
        if rows.len() != readable.records.len() {
            return Err(QueryError::InvalidInput);
        }

        Ok(Self {
            records,
            rows,
            readable,
            assertions,
            resolved_links: &graph.resolved_links,
            concept_children,
            identity_concepts,
        })
    }

    pub(super) fn require_record(&self, uid: &str) -> Result<&'a RecordState, QueryError> {
        let uid = canonical(uid, "r")?;
        if !self.readable.records.contains(uid) {
            return Err(QueryError::Unavailable);
        }
        self.records
            .get(uid)
            .copied()
            .ok_or(QueryError::Unavailable)
    }

    fn require_concept(&self, uid: &str) -> Result<&'a str, QueryError> {
        canonical(uid, "c")?;
        self.readable
            .concepts
            .get(uid)
            .map(String::as_str)
            .ok_or(QueryError::Unavailable)
    }

    pub(super) fn require_organ(&self, uid: &str) -> Result<(), QueryError> {
        if self.require_record(uid)?.kind == RecordKind::Organ {
            Ok(())
        } else {
            Err(QueryError::Unavailable)
        }
    }

    pub(super) fn readable_object(&self, assertion: &'a AssertionState) -> Option<&'a str> {
        let object = assertion
            .object_uid
            .as_deref()
            .or_else(|| self.resolved_links.get(&assertion.uid).map(String::as_str))?;
        self.readable.records.contains(object).then_some(object)
    }

    pub(super) fn concept_family(
        &self,
        uid: &str,
        budget: &mut Budget<'_>,
    ) -> Result<BTreeSet<&'a str>, QueryError> {
        if let Some(concepts) = parse_meaning_set(uid).map_err(|_| QueryError::InvalidInput)? {
            return concepts
                .iter()
                .map(|concept| {
                    budget.step()?;
                    self.require_concept(concept)
                })
                .collect();
        }
        reachable(self.require_concept(uid)?, &self.concept_children, budget)
    }
}

fn validate_readable(
    ids: &BTreeSet<String>,
    prefix: &str,
    budget: &mut Budget<'_>,
    available: impl Fn(&str) -> bool,
) -> Result<(), QueryError> {
    for uid in ids {
        let uid = charge_uid(uid, prefix, budget)?;
        if !available(uid) {
            return Err(QueryError::InvalidInput);
        }
    }
    Ok(())
}

fn charge_uid<'a>(
    uid: &'a str,
    prefix: &str,
    budget: &mut Budget<'_>,
) -> Result<&'a str, QueryError> {
    budget.step()?;
    canonical(uid, prefix)?;
    budget.text(uid)?;
    Ok(uid)
}

fn canonical<'a>(uid: &'a str, prefix: &str) -> Result<&'a str, QueryError> {
    valid_uid(uid, prefix)
        .then_some(uid)
        .ok_or(QueryError::InvalidInput)
}

pub(super) fn acyclic<'a>(
    nodes: impl Iterator<Item = &'a str>,
    edges: &BTreeMap<&'a str, BTreeSet<&'a str>>,
    budget: &mut Budget<'_>,
) -> Result<(), QueryError> {
    let mut incoming = nodes.map(|uid| (uid, 0usize)).collect::<BTreeMap<_, _>>();
    for children in edges.values() {
        for child in children {
            budget.step()?;
            let count = incoming.get_mut(*child).ok_or(QueryError::InvalidInput)?;
            *count = count.checked_add(1).ok_or(QueryError::LimitExceeded)?;
        }
    }
    let mut queue = incoming
        .iter()
        .filter_map(|(uid, count)| (*count == 0).then_some(*uid))
        .collect::<VecDeque<_>>();
    let mut visited = 0usize;
    while let Some(uid) = queue.pop_front() {
        budget.step()?;
        visited = visited.checked_add(1).ok_or(QueryError::LimitExceeded)?;
        if let Some(children) = edges.get(uid) {
            for child in children {
                budget.step()?;
                let count = incoming.get_mut(*child).ok_or(QueryError::InvalidInput)?;
                *count = count.checked_sub(1).ok_or(QueryError::InvalidInput)?;
                if *count == 0 {
                    queue.push_back(*child);
                }
            }
        }
    }
    if visited == incoming.len() {
        Ok(())
    } else {
        Err(QueryError::CyclicGraph)
    }
}

pub(super) fn reachable<'a>(
    root: &'a str,
    edges: &BTreeMap<&str, BTreeSet<&'a str>>,
    budget: &mut Budget<'_>,
) -> Result<BTreeSet<&'a str>, QueryError> {
    let mut seen = BTreeSet::from([root]);
    let mut queue = VecDeque::from([root]);
    while let Some(uid) = queue.pop_front() {
        budget.step()?;
        if let Some(children) = edges.get(uid) {
            for &child in children {
                budget.step()?;
                if seen.insert(child) {
                    queue.push_back(child);
                }
            }
        }
    }
    Ok(seen)
}
