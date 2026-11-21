use std::collections::{BTreeMap, BTreeSet, VecDeque};

use nucleus::{
    RecordKind,
    performance::{Span, add},
    vocabulary::parse_meaning_set,
};

use super::{
    AssertionRole, AssertionState, AuthorityError, ConceptState, GraphSnapshot, RecordState,
    budget::{Budget, identity, within},
};

pub(super) struct Graph<'a> {
    pub(super) source: &'a GraphSnapshot,
    pub(super) records: BTreeMap<&'a str, &'a RecordState>,
    pub(super) concepts: BTreeMap<&'a str, &'a ConceptState>,
    children: BTreeMap<&'a str, BTreeSet<&'a str>>,
    pub(super) assertions: BTreeMap<&'a str, &'a AssertionState>,
    pub(super) assertions_by_subject: BTreeMap<&'a str, BTreeMap<&'a str, &'a AssertionState>>,
}

impl<'a> Graph<'a> {
    pub(super) fn object(&self, assertion: &'a AssertionState) -> Option<&'a str> {
        assertion.object_uid.as_deref().or_else(|| {
            self.source
                .resolved_links
                .get(&assertion.uid)
                .map(String::as_str)
        })
    }

    pub(super) fn prepare(
        source: &'a GraphSnapshot,
        budget: &mut Budget<'_>,
    ) -> Result<Self, AuthorityError> {
        let _preparation = Span::new("authority-graph-preparation");
        let mut indexed_edges = 0;
        within(source.records.len(), budget.limits.records)?;
        within(source.concepts.len(), budget.limits.concepts)?;
        within(source.assertions.len(), budget.limits.assertions)?;
        within(source.places.len(), budget.limits.places)?;
        let mut graph = Self {
            source,
            records: BTreeMap::new(),
            concepts: BTreeMap::new(),
            children: BTreeMap::new(),
            assertions: BTreeMap::new(),
            assertions_by_subject: BTreeMap::new(),
        };
        for record in &source.records {
            budget.step()?;
            identity(&record.uid, "r")?;
            if graph.records.insert(&record.uid, record).is_some() {
                return Err(AuthorityError::InvalidGraph);
            }
            if let Some(content) = &record.content {
                budget.text(&content.head)?;
                budget.text(&content.body)?;
                if let Some(slug) = &content.slug {
                    budget.text(slug)?;
                }
                for (property, value) in &content.extensions {
                    budget.step()?;
                    budget.property(property)?;
                    budget.value(value)?;
                }
            }
        }
        for concept in &source.concepts {
            budget.step()?;
            identity(&concept.uid, "c")?;
            budget.text(&concept.name)?;
            if graph.concepts.insert(&concept.uid, concept).is_some() {
                return Err(AuthorityError::InvalidGraph);
            }
        }
        for record in &source.records {
            if let Some(content) = &record.content {
                if let Some(uid) = &content.unit_uid {
                    budget.edge()?;
                    graph.concept(uid)?;
                }
                if let Some(uid) = &content.place_uid {
                    budget.edge()?;
                    graph.place(uid)?;
                }
            }
            if let Some(uid) = &record.organ_uid {
                budget.edge()?;
                graph.organ(uid)?;
            }
        }
        for uid in &source.places {
            budget.step()?;
            identity(uid, "pl")?;
        }
        for concept in &source.concepts {
            for parent in &concept.parents {
                budget.edge()?;
                graph.concept(parent)?;
                graph
                    .children
                    .entry(parent.as_str())
                    .or_default()
                    .insert(concept.uid.as_str());
                indexed_edges += 1;
            }
        }
        acyclic(graph.concepts.keys().copied(), &graph.children, budget)?;
        let mut assertion_tuples = BTreeSet::new();
        let mut identity_subjects = BTreeSet::new();
        for assertion in &source.assertions {
            budget.step()?;
            budget.edge()?;
            identity(&assertion.uid, "a")?;
            if graph.assertions.insert(&assertion.uid, assertion).is_some() {
                return Err(AuthorityError::InvalidGraph);
            }
            if !assertion_tuples.insert(assertion.relation_key())
                || assertion.role == AssertionRole::Identity
                    && !identity_subjects.insert(&assertion.subject_uid)
            {
                return Err(AuthorityError::InvalidGraph);
            }
            graph.record(&assertion.subject_uid)?;
            graph.concept(&assertion.predicate_uid)?;
            if let Some(uid) = &assertion.object_uid {
                graph.record(uid)?;
            }
            if let Some(uri) = &assertion.object_uri {
                budget.text(uri)?;
                if assertion.object_uid.is_some() || !valid_uri(uri) {
                    return Err(AuthorityError::InvalidGraph);
                }
            }
            if let Some(uid) = &assertion.unit_uid {
                graph.concept(uid)?;
            }
            if assertion.invalid_identity_shape() {
                return Err(AuthorityError::InvalidGraph);
            }
            graph
                .assertions_by_subject
                .entry(&assertion.subject_uid)
                .or_default()
                .insert(&assertion.uid, assertion);
        }
        within(source.resolved_links.len(), budget.limits.assertions)?;
        for (uid, target) in &source.resolved_links {
            budget.step()?;
            budget.text(uid)?;
            budget.text(target)?;
            let assertion = graph
                .assertions
                .get(uid.as_str())
                .ok_or(AuthorityError::InvalidGraph)?;
            let object = graph.record(target)?;
            let subject = graph.record(&assertion.subject_uid)?;
            if assertion.object_uri.is_none()
                || assertion.object_uid.is_some()
                || object.organ_uid != subject.organ_uid
            {
                return Err(AuthorityError::InvalidGraph);
            }
        }
        add("authority-concept-edges-indexed", indexed_edges);
        Ok(graph)
    }

    pub(super) fn record(&self, uid: &str) -> Result<&'a RecordState, AuthorityError> {
        identity(uid, "r")?;
        self.records
            .get(uid)
            .copied()
            .ok_or(AuthorityError::MissingDependency)
    }

    pub(super) fn concept(&self, uid: &str) -> Result<&'a ConceptState, AuthorityError> {
        identity(uid, "c")?;
        self.concepts
            .get(uid)
            .copied()
            .ok_or(AuthorityError::MissingDependency)
    }

    pub(super) fn organ(&self, uid: &str) -> Result<(), AuthorityError> {
        if self.record(uid)?.kind == RecordKind::Organ {
            Ok(())
        } else {
            Err(AuthorityError::InvalidGraph)
        }
    }

    pub(super) fn place(&self, uid: &str) -> Result<(), AuthorityError> {
        identity(uid, "pl")?;
        if self.source.places.contains(uid) {
            Ok(())
        } else {
            Err(AuthorityError::MissingDependency)
        }
    }

    pub(super) fn family(
        &self,
        uid: &str,
        budget: &mut Budget<'_>,
    ) -> Result<BTreeSet<&'a str>, AuthorityError> {
        if let Some(concepts) = parse_meaning_set(uid).map_err(|_| AuthorityError::InvalidGraph)? {
            return concepts
                .iter()
                .map(|concept| {
                    budget.step()?;
                    Ok(self.concept(concept)?.uid.as_str())
                })
                .collect();
        }
        let root = self.concept(uid)?.uid.as_str();
        reachable(root, &self.children, budget)
    }
}

pub(super) fn valid_uri(uri: &str) -> bool {
    !uri.is_empty() && uri.len() <= 8192 && !uri.chars().any(char::is_control)
}

pub(super) fn acyclic<'a>(
    nodes: impl Iterator<Item = &'a str>,
    edges: &BTreeMap<&'a str, BTreeSet<&'a str>>,
    budget: &mut Budget<'_>,
) -> Result<(), AuthorityError> {
    let mut incoming: BTreeMap<&str, usize> = nodes.map(|uid| (uid, 0)).collect();
    for children in edges.values() {
        for &child in children {
            budget.step()?;
            *incoming
                .get_mut(child)
                .ok_or(AuthorityError::MissingDependency)? += 1;
        }
    }
    let mut queue: VecDeque<&str> = incoming
        .iter()
        .filter_map(|(uid, count)| (*count == 0).then_some(*uid))
        .collect();
    let mut visited = 0;
    while let Some(uid) = queue.pop_front() {
        budget.step()?;
        visited += 1;
        if let Some(children) = edges.get(uid) {
            for &child in children {
                budget.step()?;
                let count = incoming
                    .get_mut(child)
                    .ok_or(AuthorityError::MissingDependency)?;
                *count -= 1;
                if *count == 0 {
                    queue.push_back(child);
                }
            }
        }
    }
    if visited == incoming.len() {
        Ok(())
    } else {
        Err(AuthorityError::CyclicGraph)
    }
}

pub(super) fn reachable<'a>(
    root: &'a str,
    edges: &BTreeMap<&'a str, BTreeSet<&'a str>>,
    budget: &mut Budget<'_>,
) -> Result<BTreeSet<&'a str>, AuthorityError> {
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
