use std::{
    collections::{BTreeMap, BTreeSet},
    slice::from_ref,
};

use nucleus::{
    RecordKind,
    performance::{Span, add},
};

use super::{
    AssertionRole, AssertionTarget, AuthorityError, GraphSnapshot, Property, RecordState,
    RolePolicy, VisibilityCeiling,
    budget::{Budget, within},
    graph::{Graph, acyclic, reachable, valid_uri},
};
use crate::query::{LinkDirection, Predicate};

pub(super) enum Selector<'a> {
    All(Vec<Selector<'a>>),
    Any(Vec<Selector<'a>>),
    Not(Box<Selector<'a>>),
    Members(BTreeSet<&'a str>),
    Uid(&'a str),
    Kind(RecordKind),
    Content(&'a Predicate),
    TextContains(String),
}

impl<'a> Selector<'a> {
    pub(super) fn matches(
        &self,
        record: &RecordState,
        budget: &mut Budget<'_>,
    ) -> Result<bool, AuthorityError> {
        budget.step()?;
        Ok(match self {
            Self::All(children) | Self::Any(children) => {
                let short_circuit = matches!(self, Self::Any(_));
                for child in children {
                    if child.matches(record, budget)? == short_circuit {
                        return Ok(short_circuit);
                    }
                }
                !short_circuit
            }
            Self::Not(child) => !child.matches(record, budget)?,
            Self::Members(members) => members.contains(record.uid.as_str()),
            Self::Uid(uid) => record.uid == *uid,
            Self::Kind(kind) => *kind == record.kind,
            Self::TextContains(needle) => {
                let content = record
                    .content
                    .as_ref()
                    .ok_or(AuthorityError::IncompleteRecord)?;
                content.head.to_lowercase().contains(needle)
                    || content.body.to_lowercase().contains(needle)
            }
            Self::Content(predicate) => {
                let content = record
                    .content
                    .as_ref()
                    .ok_or(AuthorityError::IncompleteRecord)?;
                match predicate {
                    Predicate::QuantityLt(value) => {
                        content.quantity.exact_numeric_cmp(*value).is_lt()
                    }
                    Predicate::QuantityLte(value) => {
                        !content.quantity.exact_numeric_cmp(*value).is_gt()
                    }
                    Predicate::QuantityGt(value) => {
                        content.quantity.exact_numeric_cmp(*value).is_gt()
                    }
                    Predicate::QuantityGte(value) => {
                        !content.quantity.exact_numeric_cmp(*value).is_lt()
                    }
                    Predicate::QuantityEq(value) => {
                        content.quantity.exact_numeric_cmp(*value).is_eq()
                    }
                    Predicate::SlugEq(value) => content.slug.as_ref() == Some(value),
                    Predicate::UnitEq(value) => content.unit_uid.as_ref() == Some(value),
                    _ => return Err(AuthorityError::UnsupportedPredicate),
                }
            }
        })
    }

    pub(super) fn compile(
        predicate: &'a Predicate,
        graph: &Graph<'a>,
        budget: &mut Budget<'_>,
        depth: usize,
    ) -> Result<Self, AuthorityError> {
        budget.predicate(depth)?;
        match predicate {
            Predicate::QuantityLt(_)
            | Predicate::QuantityLte(_)
            | Predicate::QuantityGt(_)
            | Predicate::QuantityGte(_)
            | Predicate::QuantityEq(_) => Ok(Self::Content(predicate)),
            Predicate::SlugEq(value) => {
                budget.text(value)?;
                Ok(Self::Content(predicate))
            }
            Predicate::TextContains(value) => {
                budget.text(value)?;
                add("authority-text-needles-prepared", 1);
                Ok(Self::TextContains(value.to_lowercase()))
            }
            Predicate::UnitEq(value) => {
                budget.text(value)?;
                graph.concept(value)?;
                Ok(Self::Content(predicate))
            }
            Predicate::All(children) | Predicate::Any(children) => {
                within(children.len(), budget.remaining_predicates())?;
                let children = children
                    .iter()
                    .map(|child| Self::compile(child, graph, budget, depth + 1))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(if matches!(predicate, Predicate::All(_)) {
                    Self::All(children)
                } else {
                    Self::Any(children)
                })
            }
            Predicate::Not(child) => Ok(Self::Not(Box::new(Self::compile(
                child,
                graph,
                budget,
                depth + 1,
            )?))),
            Predicate::UidEq(uid) => {
                budget.text(uid)?;
                let record = graph.record(uid)?;
                Ok(Self::Uid(&record.uid))
            }
            Predicate::KindEq(kind) => {
                budget.text(kind)?;
                RecordKind::parse(kind)
                    .map(Self::Kind)
                    .ok_or(AuthorityError::InvalidPolicy)
            }
            Predicate::ConceptIn(uid) => {
                budget.text(uid)?;
                let family = graph.family(uid, budget)?;
                let mut members = BTreeSet::new();
                for assertion in &graph.source.assertions {
                    budget.step()?;
                    if family.contains(assertion.predicate_uid.as_str()) {
                        members.insert(assertion.subject_uid.as_str());
                    }
                }
                Ok(Self::Members(members))
            }
            Predicate::Relation {
                kind,
                direction,
                other,
            } => {
                budget.text(kind)?;
                let family = graph.family(kind, budget)?;
                if let Some(other) = other {
                    budget.text(other)?;
                    graph.record(other)?;
                }
                let mut members = BTreeSet::new();
                for assertion in &graph.source.assertions {
                    budget.step()?;
                    let Some(object) = graph.object(assertion) else {
                        continue;
                    };
                    if !family.contains(assertion.predicate_uid.as_str()) {
                        continue;
                    }
                    if matches!(direction, LinkDirection::Out | LinkDirection::Both)
                        && other.as_deref().is_none_or(|other| other == object)
                    {
                        members.insert(assertion.subject_uid.as_str());
                    }
                    if matches!(direction, LinkDirection::In | LinkDirection::Both)
                        && other
                            .as_deref()
                            .is_none_or(|other| other == assertion.subject_uid)
                    {
                        members.insert(object);
                    }
                }
                Ok(Self::Members(members))
            }
            Predicate::Under {
                record,
                kind,
                include_self,
            } => {
                budget.text(record)?;
                budget.text(kind)?;
                let root = graph.record(record)?.uid.as_str();
                let family = graph.family(kind, budget)?;
                let mut edges: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
                for assertion in &graph.source.assertions {
                    budget.step()?;
                    if let Some(object) = graph.object(assertion)
                        && family.contains(assertion.predicate_uid.as_str())
                    {
                        edges
                            .entry(object)
                            .or_default()
                            .insert(assertion.subject_uid.as_str());
                    }
                }
                acyclic(graph.records.keys().copied(), &edges, budget)?;
                let mut members = reachable(root, &edges, budget)?;
                if !include_self {
                    members.remove(root);
                }
                Ok(Self::Members(members))
            }
            Predicate::OrganEq(uid) => Self::organs(from_ref(uid), graph, budget),
            Predicate::OrganIn(uids) => Self::organs(uids, graph, budget),
            _ => Err(AuthorityError::UnsupportedPredicate),
        }
    }

    fn organs(
        uids: &[String],
        graph: &Graph<'a>,
        budget: &mut Budget<'_>,
    ) -> Result<Self, AuthorityError> {
        within(uids.len(), budget.limits.records)?;
        let mut organs = BTreeSet::new();
        for uid in uids {
            budget.step()?;
            budget.text(uid)?;
            graph.organ(uid)?;
            organs.insert(uid.as_str());
        }
        let mut members = BTreeSet::new();
        for record in &graph.source.records {
            budget.step()?;
            if record
                .organ_uid
                .as_deref()
                .is_some_and(|uid| organs.contains(uid))
            {
                members.insert(record.uid.as_str());
            }
        }
        Ok(Self::Members(members))
    }
}

pub(super) struct Selectors<'a> {
    pub(super) read: Selector<'a>,
    pub(super) grants: Vec<Selector<'a>>,
}

impl<'a> Selectors<'a> {
    pub(super) fn prepare(
        policy: &'a RolePolicy,
        graph: &Graph<'a>,
        ceiling: &VisibilityCeiling,
        budget: &mut Budget<'_>,
    ) -> Result<Self, AuthorityError> {
        within(ceiling.records.len(), budget.limits.records)?;
        for uid in &ceiling.records {
            budget.step()?;
            graph.record(uid)?;
        }
        within(ceiling.concepts.len(), budget.limits.concepts)?;
        for uid in &ceiling.concepts {
            budget.step()?;
            graph.concept(uid)?;
        }
        within(ceiling.places.len(), budget.limits.places)?;
        for uid in &ceiling.places {
            budget.step()?;
            graph.place(uid)?;
        }
        within(policy.grants.len(), budget.limits.grants)?;
        let read = Selector::compile(&policy.read, graph, budget, 0)?;
        let mut grants = Vec::new();
        for grant in &policy.grants {
            budget.step()?;
            grants.push(Selector::compile(&grant.selector, graph, budget, 0)?);
            for property in &grant.properties {
                budget.step()?;
                if let Property::Extension(property) = property {
                    budget.property(property)?;
                }
            }
            for assertion in grant.assertions_add.iter().chain(&grant.assertions_remove) {
                budget.step()?;
                graph.concept(&assertion.predicate_uid)?;
                if let AssertionTarget::Record(uid) = &assertion.target {
                    graph.record(uid)?;
                }
                if let AssertionTarget::Uri(uri) = &assertion.target {
                    budget.text(uri)?;
                    if !valid_uri(uri)
                        || !uri
                            .split_once(':')
                            .is_some_and(|(scheme, _)| matches!(scheme, "http" | "https" | "file"))
                    {
                        return Err(AuthorityError::InvalidGraph);
                    }
                }
                if assertion.role == AssertionRole::Identity
                    && (!matches!(assertion.target, AssertionTarget::Unary)
                        || !assertion.properties.is_empty())
                {
                    return Err(AuthorityError::InvalidPolicy);
                }
            }
        }
        Ok(Self { read, grants })
    }
}

pub(super) struct Evaluation<'a> {
    pub(super) graph: Graph<'a>,
    pub(super) ceiling: &'a VisibilityCeiling,
    pub(super) read: Selector<'a>,
    pub(super) grants: Vec<Selector<'a>>,
}

impl<'a> Evaluation<'a> {
    pub(super) fn prepare(
        policy: &'a RolePolicy,
        snapshot: &'a GraphSnapshot,
        ceiling: &'a VisibilityCeiling,
        budget: &mut Budget<'_>,
    ) -> Result<Self, AuthorityError> {
        let _preparation = Span::new("authority-evaluation-preparation");
        let graph = Graph::prepare(snapshot, budget)?;
        let Selectors { read, grants } = Selectors::prepare(policy, &graph, ceiling, budget)?;
        Ok(Self {
            graph,
            ceiling,
            read,
            grants,
        })
    }

    pub(super) fn readable(
        &self,
        record: &RecordState,
        budget: &mut Budget<'_>,
    ) -> Result<bool, AuthorityError> {
        Ok(self.ceiling.records.contains(&record.uid) && self.read.matches(record, budget)?)
    }

    pub(super) fn reference_readable(
        &self,
        uid: &str,
        budget: &mut Budget<'_>,
    ) -> Result<bool, AuthorityError> {
        let record = self.graph.record(uid)?;
        Ok(!record.deleted && self.readable(record, budget)?)
    }
}
