use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;

use nucleus::{DecimalValue, RecordKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{LinkDirection, Predicate};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtensionProperty {
    pub namespace: String,
    pub field: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Property {
    Kind,
    Slug,
    Head,
    Body,
    Quantity,
    Unit,
    Place,
    Organ,
    Extension(ExtensionProperty),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    Create,
    Update,
    Delete,
    Restore,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssertionRole {
    Ordinary,
    Identity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssertionTarget {
    Unary,
    Record(String),
    AnyReadableRecord,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssertionProperty {
    Quantity,
    Unit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssertionGrant {
    pub predicate_uid: String,
    pub target: AssertionTarget,
    pub role: AssertionRole,
    pub properties: BTreeSet<AssertionProperty>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MutationGrant {
    pub operation: Operation,
    pub selector: Predicate,
    pub properties: BTreeSet<Property>,
    pub assertions_add: Vec<AssertionGrant>,
    pub assertions_remove: Vec<AssertionGrant>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RolePolicy {
    pub read: Predicate,
    pub grants: Vec<MutationGrant>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecordState {
    pub uid: String,
    pub kind: RecordKind,
    pub organ_uid: Option<String>,
    pub deleted: bool,
    pub content: Option<RecordContent>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecordContent {
    pub slug: Option<String>,
    pub head: String,
    pub body: String,
    pub quantity: DecimalValue,
    pub unit_uid: Option<String>,
    pub place_uid: Option<String>,
    pub extensions: BTreeMap<ExtensionProperty, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConceptState {
    pub uid: String,
    pub name: String,
    pub parents: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AssertionState {
    pub uid: String,
    pub subject_uid: String,
    pub predicate_uid: String,
    pub object_uid: Option<String>,
    pub role: AssertionRole,
    pub quantity: Option<DecimalValue>,
    pub unit_uid: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct GraphSnapshot {
    pub records: Vec<RecordState>,
    pub concepts: Vec<ConceptState>,
    pub assertions: Vec<AssertionState>,
    pub places: BTreeSet<String>,
}

#[derive(Debug, Clone, Default)]
pub struct VisibilityCeiling {
    pub records: BTreeSet<String>,
    pub concepts: BTreeSet<String>,
    pub places: BTreeSet<String>,
}

#[derive(Debug, Clone)]
pub struct Limits {
    pub records: usize,
    pub concepts: usize,
    pub places: usize,
    pub assertions: usize,
    pub grants: usize,
    pub predicate_nodes: usize,
    pub predicate_depth: usize,
    pub graph_edges: usize,
    pub steps: usize,
    pub bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            records: 4096,
            concepts: 4096,
            places: 4096,
            assertions: 32768,
            grants: 128,
            predicate_nodes: 1024,
            predicate_depth: 12,
            graph_edges: 65536,
            steps: 1_000_000,
            bytes: 16 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorityError {
    MissingPolicy,
    InvalidPolicy,
    UnsupportedPredicate,
    MissingDependency,
    InvalidIdentity,
    InvalidGraph,
    CyclicGraph,
    LimitExceeded,
    InvalidMutation,
    IncompleteRecord,
    Denied,
}

impl fmt::Display for AuthorityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::MissingPolicy => "authority policy is missing",
            Self::InvalidPolicy => "authority policy is invalid",
            Self::UnsupportedPredicate => "authority predicate is unsupported",
            Self::MissingDependency => "authority dependency is unavailable",
            Self::InvalidIdentity => "authority requires durable identities",
            Self::InvalidGraph => "authority graph is invalid",
            Self::CyclicGraph => "authority traversal contains a cycle",
            Self::LimitExceeded => "authority evaluation limit exceeded",
            Self::InvalidMutation => "authority mutation states are invalid",
            Self::IncompleteRecord => "authority mutation needs complete Record content",
            Self::Denied => "operation is not permitted",
        })
    }
}

impl std::error::Error for AuthorityError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordDecision {
    pub operation: Operation,
    pub grant_index: usize,
    pub properties: BTreeSet<Property>,
    pub assertions_added: BTreeSet<String>,
    pub assertions_removed: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MutationTarget {
    pub record_uid: String,
    pub touched_properties: BTreeSet<Property>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AssertionIntent {
    pub before: Option<AssertionState>,
    pub after: Option<AssertionState>,
    pub touched_properties: BTreeSet<AssertionProperty>,
}

struct Budget<'a> {
    limits: &'a Limits,
    steps: usize,
    bytes: usize,
    predicates: usize,
    edges: usize,
}

impl<'a> Budget<'a> {
    fn new(limits: &'a Limits) -> Self {
        Self {
            limits,
            steps: 0,
            bytes: 0,
            predicates: 0,
            edges: 0,
        }
    }

    fn step(&mut self) -> Result<(), AuthorityError> {
        self.steps = self
            .steps
            .checked_add(1)
            .ok_or(AuthorityError::LimitExceeded)?;
        within(self.steps, self.limits.steps)
    }

    fn text(&mut self, text: &str) -> Result<(), AuthorityError> {
        self.bytes = self
            .bytes
            .checked_add(text.len())
            .ok_or(AuthorityError::LimitExceeded)?;
        within(self.bytes, self.limits.bytes)
    }

    fn edge(&mut self) -> Result<(), AuthorityError> {
        self.edges = self
            .edges
            .checked_add(1)
            .ok_or(AuthorityError::LimitExceeded)?;
        within(self.edges, self.limits.graph_edges)
    }

    fn property(&mut self, property: &ExtensionProperty) -> Result<(), AuthorityError> {
        if property.namespace.is_empty() || property.field.is_empty() {
            return Err(AuthorityError::InvalidPolicy);
        }
        self.text(&property.namespace)?;
        self.text(&property.field)
    }

    fn value(&mut self, value: &Value) -> Result<(), AuthorityError> {
        let mut pending = vec![(value, 0)];
        while let Some((value, depth)) = pending.pop() {
            self.step()?;
            within(depth, 32)?;
            match value {
                Value::String(text) => self.text(text)?,
                Value::Array(values) => {
                    within(values.len(), self.limits.steps.saturating_sub(self.steps))?;
                    pending.extend(values.iter().map(|value| (value, depth + 1)));
                }
                Value::Object(values) => {
                    within(values.len(), self.limits.steps.saturating_sub(self.steps))?;
                    for (key, value) in values {
                        self.text(key)?;
                        pending.push((value, depth + 1));
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
}

fn within(value: usize, limit: usize) -> Result<(), AuthorityError> {
    if value > limit {
        Err(AuthorityError::LimitExceeded)
    } else {
        Ok(())
    }
}

fn identity(uid: &str, prefix: &str) -> Result<(), AuthorityError> {
    if nucleus::valid_uid(uid, prefix) {
        Ok(())
    } else {
        Err(AuthorityError::InvalidIdentity)
    }
}

struct Graph<'a> {
    source: &'a GraphSnapshot,
    records: BTreeMap<&'a str, &'a RecordState>,
    concepts: BTreeMap<&'a str, &'a ConceptState>,
    children: BTreeMap<String, BTreeSet<String>>,
    assertions: BTreeMap<&'a str, &'a AssertionState>,
    assertions_by_subject: BTreeMap<&'a str, BTreeMap<&'a str, &'a AssertionState>>,
}

impl<'a> Graph<'a> {
    fn prepare(source: &'a GraphSnapshot, budget: &mut Budget<'_>) -> Result<Self, AuthorityError> {
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
                    .entry(parent.clone())
                    .or_default()
                    .insert(concept.uid.clone());
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
            if !assertion_tuples.insert((
                &assertion.subject_uid,
                &assertion.predicate_uid,
                &assertion.object_uid,
            )) || assertion.role == AssertionRole::Identity
                && !identity_subjects.insert(&assertion.subject_uid)
            {
                return Err(AuthorityError::InvalidGraph);
            }
            graph.record(&assertion.subject_uid)?;
            graph.concept(&assertion.predicate_uid)?;
            if let Some(uid) = &assertion.object_uid {
                graph.record(uid)?;
            }
            if let Some(uid) = &assertion.unit_uid {
                graph.concept(uid)?;
            }
            if assertion.role == AssertionRole::Identity
                && (assertion.object_uid.is_some()
                    || assertion.quantity.is_some()
                    || assertion.unit_uid.is_some())
            {
                return Err(AuthorityError::InvalidGraph);
            }
            graph
                .assertions_by_subject
                .entry(&assertion.subject_uid)
                .or_default()
                .insert(&assertion.uid, assertion);
        }
        Ok(graph)
    }

    fn record(&self, uid: &str) -> Result<&'a RecordState, AuthorityError> {
        identity(uid, "r")?;
        self.records
            .get(uid)
            .copied()
            .ok_or(AuthorityError::MissingDependency)
    }

    fn concept(&self, uid: &str) -> Result<(), AuthorityError> {
        identity(uid, "c")?;
        if self.concepts.contains_key(uid) {
            Ok(())
        } else {
            Err(AuthorityError::MissingDependency)
        }
    }

    fn organ(&self, uid: &str) -> Result<(), AuthorityError> {
        if self.record(uid)?.kind == RecordKind::Organ {
            Ok(())
        } else {
            Err(AuthorityError::InvalidGraph)
        }
    }

    fn place(&self, uid: &str) -> Result<(), AuthorityError> {
        identity(uid, "pl")?;
        if self.source.places.contains(uid) {
            Ok(())
        } else {
            Err(AuthorityError::MissingDependency)
        }
    }

    fn family(
        &self,
        uid: &str,
        budget: &mut Budget<'_>,
    ) -> Result<BTreeSet<String>, AuthorityError> {
        self.concept(uid)?;
        reachable(uid, &self.children, budget)
    }
}

fn acyclic<'a>(
    nodes: impl Iterator<Item = &'a str>,
    edges: &BTreeMap<String, BTreeSet<String>>,
    budget: &mut Budget<'_>,
) -> Result<(), AuthorityError> {
    let mut incoming: BTreeMap<&str, usize> = nodes.map(|uid| (uid, 0)).collect();
    for children in edges.values() {
        for child in children {
            budget.step()?;
            *incoming
                .get_mut(child.as_str())
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
            for child in children {
                budget.step()?;
                let count = incoming
                    .get_mut(child.as_str())
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

fn reachable(
    root: &str,
    edges: &BTreeMap<String, BTreeSet<String>>,
    budget: &mut Budget<'_>,
) -> Result<BTreeSet<String>, AuthorityError> {
    let mut seen = BTreeSet::from([root.to_string()]);
    let mut queue = VecDeque::from([root.to_string()]);
    while let Some(uid) = queue.pop_front() {
        budget.step()?;
        if let Some(children) = edges.get(&uid) {
            for child in children {
                budget.step()?;
                if seen.insert(child.clone()) {
                    queue.push_back(child.clone());
                }
            }
        }
    }
    Ok(seen)
}

enum Selector {
    All(Vec<Selector>),
    Any(Vec<Selector>),
    Not(Box<Selector>),
    Members(BTreeSet<String>),
    Kind(String),
}

impl Selector {
    fn matches(
        &self,
        record: &RecordState,
        budget: &mut Budget<'_>,
    ) -> Result<bool, AuthorityError> {
        budget.step()?;
        Ok(match self {
            Self::All(children) => {
                for child in children {
                    if !child.matches(record, budget)? {
                        return Ok(false);
                    }
                }
                true
            }
            Self::Any(children) => {
                for child in children {
                    if child.matches(record, budget)? {
                        return Ok(true);
                    }
                }
                false
            }
            Self::Not(child) => !child.matches(record, budget)?,
            Self::Members(members) => members.contains(&record.uid),
            Self::Kind(kind) => kind == record.kind.as_str(),
        })
    }

    fn compile(
        predicate: &Predicate,
        graph: &Graph<'_>,
        budget: &mut Budget<'_>,
        depth: usize,
    ) -> Result<Self, AuthorityError> {
        budget.step()?;
        budget.predicates += 1;
        within(depth, budget.limits.predicate_depth)?;
        within(budget.predicates, budget.limits.predicate_nodes)?;
        match predicate {
            Predicate::All(children) | Predicate::Any(children) => {
                within(
                    children.len(),
                    budget
                        .limits
                        .predicate_nodes
                        .saturating_sub(budget.predicates),
                )?;
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
                graph.record(uid)?;
                Ok(Self::Members(BTreeSet::from([uid.clone()])))
            }
            Predicate::KindEq(kind) => {
                budget.text(kind)?;
                if RecordKind::parse(kind).is_none() {
                    return Err(AuthorityError::InvalidPolicy);
                }
                Ok(Self::Kind(kind.clone()))
            }
            Predicate::ConceptIn(uid) => {
                budget.text(uid)?;
                let family = graph.family(uid, budget)?;
                let mut members = BTreeSet::new();
                for assertion in &graph.source.assertions {
                    budget.step()?;
                    if family.contains(&assertion.predicate_uid) {
                        members.insert(assertion.subject_uid.clone());
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
                    let Some(object) = &assertion.object_uid else {
                        continue;
                    };
                    if !family.contains(&assertion.predicate_uid) {
                        continue;
                    }
                    if matches!(direction, LinkDirection::Out | LinkDirection::Both)
                        && other.as_ref().is_none_or(|other| other == object)
                    {
                        members.insert(assertion.subject_uid.clone());
                    }
                    if matches!(direction, LinkDirection::In | LinkDirection::Both)
                        && other
                            .as_ref()
                            .is_none_or(|other| other == &assertion.subject_uid)
                    {
                        members.insert(object.clone());
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
                graph.record(record)?;
                let family = graph.family(kind, budget)?;
                let mut edges: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
                for assertion in &graph.source.assertions {
                    budget.step()?;
                    if let Some(object) = &assertion.object_uid
                        && family.contains(&assertion.predicate_uid)
                    {
                        edges
                            .entry(object.clone())
                            .or_default()
                            .insert(assertion.subject_uid.clone());
                    }
                }
                acyclic(graph.records.keys().copied(), &edges, budget)?;
                let mut members = reachable(record, &edges, budget)?;
                if !include_self {
                    members.remove(record);
                }
                Ok(Self::Members(members))
            }
            Predicate::OrganEq(uid) => Self::organs(std::slice::from_ref(uid), graph, budget),
            Predicate::OrganIn(uids) => Self::organs(uids, graph, budget),
            _ => Err(AuthorityError::UnsupportedPredicate),
        }
    }

    fn organs(
        uids: &[String],
        graph: &Graph<'_>,
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
                members.insert(record.uid.clone());
            }
        }
        Ok(Self::Members(members))
    }
}

struct Evaluation<'a> {
    graph: Graph<'a>,
    ceiling: &'a VisibilityCeiling,
    read: Selector,
    grants: Vec<Selector>,
}

impl<'a> Evaluation<'a> {
    fn prepare(
        policy: &RolePolicy,
        snapshot: &'a GraphSnapshot,
        ceiling: &'a VisibilityCeiling,
        budget: &mut Budget<'_>,
    ) -> Result<Self, AuthorityError> {
        let graph = Graph::prepare(snapshot, budget)?;
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
        let read = Selector::compile(&policy.read, &graph, budget, 0)?;
        let mut grants = Vec::new();
        for grant in &policy.grants {
            budget.step()?;
            grants.push(Selector::compile(&grant.selector, &graph, budget, 0)?);
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
                if assertion.role == AssertionRole::Identity
                    && (!matches!(assertion.target, AssertionTarget::Unary)
                        || !assertion.properties.is_empty())
                {
                    return Err(AuthorityError::InvalidPolicy);
                }
            }
        }
        Ok(Self {
            graph,
            ceiling,
            read,
            grants,
        })
    }

    fn readable(
        &self,
        record: &RecordState,
        budget: &mut Budget<'_>,
    ) -> Result<bool, AuthorityError> {
        Ok(self.ceiling.records.contains(&record.uid) && self.read.matches(record, budget)?)
    }

    fn reference_readable(
        &self,
        uid: &str,
        budget: &mut Budget<'_>,
    ) -> Result<bool, AuthorityError> {
        let record = self.graph.record(uid)?;
        Ok(!record.deleted && self.readable(record, budget)?)
    }
}

pub fn readable_records(
    policy: Option<&RolePolicy>,
    graph: &GraphSnapshot,
    ceiling: &VisibilityCeiling,
    limits: &Limits,
) -> Result<BTreeSet<String>, AuthorityError> {
    let policy = policy.ok_or(AuthorityError::MissingPolicy)?;
    let mut budget = Budget::new(limits);
    let evaluation = Evaluation::prepare(policy, graph, ceiling, &mut budget)?;
    let mut readable = BTreeSet::new();
    for record in evaluation.graph.records.values() {
        budget.step()?;
        if !record.deleted && evaluation.readable(record, &mut budget)? {
            readable.insert(record.uid.clone());
        }
    }
    Ok(readable)
}

pub fn may_read(
    policy: Option<&RolePolicy>,
    graph: &GraphSnapshot,
    ceiling: &VisibilityCeiling,
    record_uid: &str,
    limits: &Limits,
) -> Result<bool, AuthorityError> {
    let policy = policy.ok_or(AuthorityError::MissingPolicy)?;
    let mut budget = Budget::new(limits);
    let evaluation = Evaluation::prepare(policy, graph, ceiling, &mut budget)?;
    let record = evaluation.graph.record(record_uid)?;
    Ok(!record.deleted && evaluation.readable(record, &mut budget)?)
}

pub fn selector_membership_changes(
    selectors: &[Predicate],
    current: &GraphSnapshot,
    proposed: &GraphSnapshot,
    limits: &Limits,
) -> Result<BTreeSet<String>, AuthorityError> {
    within(selectors.len(), limits.predicate_nodes)?;
    let mut budget = Budget::new(limits);
    let current = Graph::prepare(current, &mut budget)?;
    let proposed = Graph::prepare(proposed, &mut budget)?;
    let mut prepared = Vec::new();
    for selector in selectors {
        prepared.push((
            Selector::compile(selector, &current, &mut budget, 0)?,
            Selector::compile(selector, &proposed, &mut budget, 0)?,
        ));
    }
    let mut changed = BTreeSet::new();
    for uid in current.records.keys().chain(
        proposed
            .records
            .keys()
            .filter(|uid| !current.records.contains_key(**uid)),
    ) {
        budget.step()?;
        let before = current.records.get(uid).copied();
        let after = proposed.records.get(uid).copied();
        let mut differs = false;
        for (current_selector, proposed_selector) in &prepared {
            budget.step()?;
            let before = match before {
                Some(record) if !record.deleted => current_selector.matches(record, &mut budget)?,
                _ => false,
            };
            let after = match after {
                Some(record) if !record.deleted => {
                    proposed_selector.matches(record, &mut budget)?
                }
                _ => false,
            };
            differs |= before != after;
        }
        if differs {
            budget.text(uid)?;
            changed.insert((*uid).to_string());
        }
    }
    Ok(changed)
}

pub fn authorize_record_change(
    policy: Option<&RolePolicy>,
    current: &GraphSnapshot,
    proposed: &GraphSnapshot,
    current_ceiling: &VisibilityCeiling,
    proposed_ceiling: &VisibilityCeiling,
    record_uid: &str,
    limits: &Limits,
) -> Result<RecordDecision, AuthorityError> {
    authorize_record_changes(
        policy,
        current,
        proposed,
        current_ceiling,
        proposed_ceiling,
        &[MutationTarget {
            record_uid: record_uid.to_string(),
            touched_properties: BTreeSet::new(),
        }],
        limits,
    )?
    .remove(record_uid)
    .ok_or(AuthorityError::InvalidMutation)
}

pub fn authorize_record_changes(
    policy: Option<&RolePolicy>,
    current: &GraphSnapshot,
    proposed: &GraphSnapshot,
    current_ceiling: &VisibilityCeiling,
    proposed_ceiling: &VisibilityCeiling,
    targets: &[MutationTarget],
    limits: &Limits,
) -> Result<BTreeMap<String, RecordDecision>, AuthorityError> {
    authorize_changes(
        policy,
        current,
        proposed,
        current_ceiling,
        proposed_ceiling,
        targets,
        None,
        limits,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn authorize_record_changes_with_assertion_intents(
    policy: Option<&RolePolicy>,
    current: &GraphSnapshot,
    proposed: &GraphSnapshot,
    current_ceiling: &VisibilityCeiling,
    proposed_ceiling: &VisibilityCeiling,
    targets: &[MutationTarget],
    intents: &[AssertionIntent],
    limits: &Limits,
) -> Result<BTreeMap<String, RecordDecision>, AuthorityError> {
    authorize_changes(
        policy,
        current,
        proposed,
        current_ceiling,
        proposed_ceiling,
        targets,
        Some(intents),
        limits,
    )
}

#[allow(clippy::too_many_arguments)]
fn authorize_changes(
    policy: Option<&RolePolicy>,
    current: &GraphSnapshot,
    proposed: &GraphSnapshot,
    current_ceiling: &VisibilityCeiling,
    proposed_ceiling: &VisibilityCeiling,
    targets: &[MutationTarget],
    intents: Option<&[AssertionIntent]>,
    limits: &Limits,
) -> Result<BTreeMap<String, RecordDecision>, AuthorityError> {
    let policy = policy.ok_or(AuthorityError::MissingPolicy)?;
    let mut budget = Budget::new(limits);
    within(targets.len(), limits.records)?;
    let current = Evaluation::prepare(policy, current, current_ceiling, &mut budget)?;
    let proposed = Evaluation::prepare(policy, proposed, proposed_ceiling, &mut budget)?;
    let mut indexed = BTreeMap::new();
    for target in targets {
        budget.step()?;
        identity(&target.record_uid, "r")?;
        budget.text(&target.record_uid)?;
        if indexed
            .insert(target.record_uid.as_str(), &target.touched_properties)
            .is_some()
        {
            return Err(AuthorityError::InvalidMutation);
        }
        for property in &target.touched_properties {
            budget.step()?;
            if let Property::Extension(property) = property {
                budget.property(property)?;
            }
        }
    }
    validate_batch_changes(&current.graph, &proposed.graph, &indexed, &mut budget)?;
    let intents = intents
        .map(|intents| {
            replay_assertion_intents(intents, &current, &proposed, &indexed, &mut budget)
        })
        .transpose()?
        .unwrap_or_default();
    let mut decisions = BTreeMap::new();
    for (uid, footprint) in indexed {
        let decision = authorize_prepared_record_change(
            policy,
            &current,
            &proposed,
            uid,
            footprint,
            intents.get(uid).map_or(&[], Vec::as_slice),
            &mut budget,
        )?;
        decisions.insert(uid.to_string(), decision);
    }
    Ok(decisions)
}

struct CheckedAssertionIntent<'a> {
    intent: &'a AssertionIntent,
    before_is_current: bool,
}

fn assertion_tuple(assertion: &AssertionState) -> (&str, &str, Option<&str>) {
    (
        &assertion.subject_uid,
        &assertion.predicate_uid,
        assertion.object_uid.as_deref(),
    )
}

fn validate_intent_state(
    assertion: &AssertionState,
    evaluation: &Evaluation<'_>,
    budget: &mut Budget<'_>,
) -> Result<(), AuthorityError> {
    budget.step()?;
    identity(&assertion.uid, "a")?;
    budget.text(&assertion.uid)?;
    for uid in [&assertion.subject_uid, &assertion.predicate_uid] {
        budget.text(uid)?;
        budget.edge()?;
    }
    evaluation.graph.record(&assertion.subject_uid)?;
    evaluation.graph.concept(&assertion.predicate_uid)?;
    if let Some(uid) = &assertion.object_uid {
        budget.text(uid)?;
        budget.edge()?;
        evaluation.graph.record(uid)?;
    }
    if let Some(uid) = &assertion.unit_uid {
        budget.text(uid)?;
        budget.edge()?;
        evaluation.graph.concept(uid)?;
    }
    if assertion.role == AssertionRole::Identity
        && (assertion.object_uid.is_some()
            || assertion.quantity.is_some()
            || assertion.unit_uid.is_some())
    {
        return Err(AuthorityError::InvalidMutation);
    }
    Ok(())
}

fn replay_assertion_intents<'a>(
    intents: &'a [AssertionIntent],
    current: &'a Evaluation<'_>,
    proposed: &Evaluation<'_>,
    targets: &BTreeMap<&str, &BTreeSet<Property>>,
    budget: &mut Budget<'_>,
) -> Result<BTreeMap<&'a str, Vec<CheckedAssertionIntent<'a>>>, AuthorityError> {
    within(intents.len(), budget.limits.assertions)?;
    let mut active = BTreeMap::new();
    let mut seen = BTreeSet::new();
    let mut tuples = BTreeSet::new();
    let mut identities = BTreeSet::new();
    for (uid, assertion) in &current.graph.assertions {
        budget.step()?;
        budget.text(uid)?;
        active.insert(*uid, (*assertion, true));
        seen.insert(*uid);
        tuples.insert(assertion_tuple(assertion));
        if assertion.role == AssertionRole::Identity {
            identities.insert(assertion.subject_uid.as_str());
        }
    }
    let mut checked: BTreeMap<&str, Vec<CheckedAssertionIntent<'_>>> = BTreeMap::new();
    for intent in intents {
        budget.step()?;
        let subject = intent
            .before
            .as_ref()
            .or(intent.after.as_ref())
            .ok_or(AuthorityError::InvalidMutation)?;
        if !targets.contains_key(subject.subject_uid.as_str()) {
            return Err(AuthorityError::InvalidMutation);
        }
        for _ in &intent.touched_properties {
            budget.step()?;
        }
        let mut before_is_current = false;
        if let Some(before) = &intent.before {
            let (actual, is_current) = active
                .get(before.uid.as_str())
                .copied()
                .ok_or(AuthorityError::InvalidMutation)?;
            if actual != before {
                return Err(AuthorityError::InvalidMutation);
            }
            before_is_current = is_current;
            validate_intent_state(before, if is_current { current } else { proposed }, budget)?;
            if let Some(after) = &intent.after {
                if before.uid != after.uid || assertion_tuple(before) != assertion_tuple(after) {
                    return Err(AuthorityError::InvalidMutation);
                }
            }
            active.remove(before.uid.as_str());
            tuples.remove(&assertion_tuple(before));
            if before.role == AssertionRole::Identity {
                identities.remove(before.subject_uid.as_str());
            }
        }
        if let Some(after) = &intent.after {
            validate_intent_state(after, proposed, budget)?;
            if intent.before.is_none() && !seen.insert(after.uid.as_str()) {
                return Err(AuthorityError::InvalidMutation);
            }
            within(seen.len(), budget.limits.assertions)?;
            if !tuples.insert(assertion_tuple(after))
                || after.role == AssertionRole::Identity
                    && !identities.insert(after.subject_uid.as_str())
                || active.insert(after.uid.as_str(), (after, false)).is_some()
            {
                return Err(AuthorityError::InvalidMutation);
            }
            within(active.len(), budget.limits.assertions)?;
        }
        checked
            .entry(&subject.subject_uid)
            .or_default()
            .push(CheckedAssertionIntent {
                intent,
                before_is_current,
            });
    }
    if active.len() != proposed.graph.assertions.len() {
        return Err(AuthorityError::InvalidMutation);
    }
    for (uid, (assertion, _)) in active {
        budget.step()?;
        if proposed.graph.assertions.get(uid).copied() != Some(assertion) {
            return Err(AuthorityError::InvalidMutation);
        }
    }
    Ok(checked)
}

fn validate_batch_changes(
    current: &Graph<'_>,
    proposed: &Graph<'_>,
    targets: &BTreeMap<&str, &BTreeSet<Property>>,
    budget: &mut Budget<'_>,
) -> Result<(), AuthorityError> {
    if current.concepts.len() != proposed.concepts.len()
        || current.source.places.len() != proposed.source.places.len()
    {
        return Err(AuthorityError::InvalidMutation);
    }
    for (uid, concept) in &current.concepts {
        budget.step()?;
        if proposed.concepts.get(uid) != Some(concept) {
            return Err(AuthorityError::InvalidMutation);
        }
    }
    for uid in &current.source.places {
        budget.step()?;
        if !proposed.source.places.contains(uid) {
            return Err(AuthorityError::InvalidMutation);
        }
    }
    for (uid, record) in &current.records {
        budget.step()?;
        if proposed.records.get(uid) != Some(record) && !targets.contains_key(uid) {
            return Err(AuthorityError::InvalidMutation);
        }
    }
    for uid in proposed.records.keys() {
        budget.step()?;
        if !current.records.contains_key(uid) && !targets.contains_key(uid) {
            return Err(AuthorityError::InvalidMutation);
        }
    }
    for (uid, assertion) in &current.assertions {
        budget.step()?;
        if proposed.assertions.get(uid) != Some(assertion)
            && !targets.contains_key(assertion.subject_uid.as_str())
        {
            return Err(AuthorityError::InvalidMutation);
        }
    }
    for (uid, assertion) in &proposed.assertions {
        budget.step()?;
        if current.assertions.get(uid) != Some(assertion)
            && !targets.contains_key(assertion.subject_uid.as_str())
        {
            return Err(AuthorityError::InvalidMutation);
        }
    }
    Ok(())
}

fn authorize_prepared_record_change(
    policy: &RolePolicy,
    current: &Evaluation<'_>,
    proposed: &Evaluation<'_>,
    record_uid: &str,
    footprint: &BTreeSet<Property>,
    intents: &[CheckedAssertionIntent<'_>],
    budget: &mut Budget<'_>,
) -> Result<RecordDecision, AuthorityError> {
    budget.step()?;
    let before = current.graph.records.get(record_uid).copied();
    let after = proposed
        .graph
        .records
        .get(record_uid)
        .copied()
        .ok_or(AuthorityError::InvalidMutation)?;
    if before.is_some_and(|before| before.organ_uid != after.organ_uid) {
        return Err(AuthorityError::InvalidMutation);
    }
    let before_content = before
        .map(|record| {
            record
                .content
                .as_ref()
                .ok_or(AuthorityError::IncompleteRecord)
        })
        .transpose()?;
    let after_content = after
        .content
        .as_ref()
        .ok_or(AuthorityError::IncompleteRecord)?;
    let operation = match before {
        None if !after.deleted => Operation::Create,
        Some(before) if !before.deleted && after.deleted => Operation::Delete,
        Some(before) if before.deleted && !after.deleted => Operation::Restore,
        Some(before) if !before.deleted && !after.deleted => Operation::Update,
        _ => return Err(AuthorityError::InvalidMutation),
    };
    if let Some(before) = before
        && !current.readable(before, budget)?
    {
        return Err(AuthorityError::Denied);
    }
    if operation != Operation::Delete && !proposed.readable(after, budget)? {
        return Err(AuthorityError::Denied);
    }
    let complete = matches!(operation, Operation::Create | Operation::Restore);
    let mut properties = changed_properties(
        if complete { None } else { before },
        if complete { None } else { before_content },
        after,
        after_content,
        budget,
    )?;
    properties.extend(footprint.iter().cloned());
    let old_assertions = current.graph.assertions_by_subject.get(record_uid);
    let new_assertions = proposed.graph.assertions_by_subject.get(record_uid);
    let mut added = Vec::new();
    let mut removed = Vec::new();
    for (uid, assertion) in new_assertions
        .into_iter()
        .flat_map(|assertions| assertions.iter())
    {
        budget.step()?;
        if complete || old_assertions.and_then(|assertions| assertions.get(uid)) != Some(assertion)
        {
            added.push(*assertion);
        }
    }
    for (uid, assertion) in old_assertions
        .into_iter()
        .flat_map(|assertions| assertions.iter())
    {
        budget.step()?;
        if new_assertions.and_then(|assertions| assertions.get(uid)) != Some(assertion) {
            removed.push(*assertion);
        }
    }
    for property in &properties {
        budget.step()?;
        let permitted = match property {
            Property::Unit => after_content
                .unit_uid
                .as_ref()
                .is_none_or(|uid| proposed.ceiling.concepts.contains(uid)),
            Property::Place => after_content
                .place_uid
                .as_ref()
                .is_none_or(|uid| proposed.ceiling.places.contains(uid)),
            Property::Organ => match &after.organ_uid {
                Some(uid) => proposed.reference_readable(uid, budget)?,
                None => true,
            },
            _ => true,
        };
        if !permitted {
            return Err(AuthorityError::Denied);
        }
    }
    for (index, grant) in policy.grants.iter().enumerate() {
        budget.step()?;
        if grant.operation != operation
            || operation != Operation::Delete && !proposed.grants[index].matches(after, budget)?
        {
            continue;
        }
        if let Some(before) = before
            && !current.grants[index].matches(before, budget)?
        {
            continue;
        }
        let mut properties_allowed = true;
        for property in &properties {
            budget.step()?;
            if !grant.properties.contains(property) {
                properties_allowed = false;
                break;
            }
        }
        if properties_allowed
            && assertions_allowed(&added, &grant.assertions_add, proposed, budget)?
            && assertions_allowed(&removed, &grant.assertions_remove, current, budget)?
            && assertion_intents_allowed(intents, grant, current, proposed, budget)?
        {
            return Ok(RecordDecision {
                operation,
                grant_index: index,
                properties,
                assertions_added: added
                    .iter()
                    .map(|assertion| assertion.uid.clone())
                    .collect(),
                assertions_removed: removed
                    .iter()
                    .map(|assertion| assertion.uid.clone())
                    .collect(),
            });
        }
    }
    Err(AuthorityError::Denied)
}

fn assertion_intents_allowed(
    intents: &[CheckedAssertionIntent<'_>],
    grant: &MutationGrant,
    current: &Evaluation<'_>,
    proposed: &Evaluation<'_>,
    budget: &mut Budget<'_>,
) -> Result<bool, AuthorityError> {
    for checked in intents {
        budget.step()?;
        for (state, rules, evaluation) in [
            (
                checked.intent.before.as_ref(),
                &grant.assertions_remove,
                if checked.before_is_current {
                    current
                } else {
                    proposed
                },
            ),
            (
                checked.intent.after.as_ref(),
                &grant.assertions_add,
                proposed,
            ),
        ] {
            if let Some(state) = state {
                let mut permitted = false;
                for rule in rules {
                    budget.step()?;
                    let mut covers = true;
                    if state.role != AssertionRole::Identity {
                        for property in &checked.intent.touched_properties {
                            budget.step()?;
                            if !rule.properties.contains(property) {
                                covers = false;
                            }
                        }
                    }
                    if covers
                        && assertion_allowed(state, std::slice::from_ref(rule), evaluation, budget)?
                    {
                        permitted = true;
                        break;
                    }
                }
                if !permitted {
                    return Ok(false);
                }
            }
        }
    }
    Ok(true)
}

fn assertions_allowed(
    assertions: &[&AssertionState],
    grants: &[AssertionGrant],
    evaluation: &Evaluation<'_>,
    budget: &mut Budget<'_>,
) -> Result<bool, AuthorityError> {
    for assertion in assertions {
        budget.step()?;
        if !assertion_allowed(assertion, grants, evaluation, budget)? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn assertion_allowed(
    assertion: &AssertionState,
    grants: &[AssertionGrant],
    evaluation: &Evaluation<'_>,
    budget: &mut Budget<'_>,
) -> Result<bool, AuthorityError> {
    if !evaluation
        .ceiling
        .concepts
        .contains(&assertion.predicate_uid)
        || assertion
            .unit_uid
            .as_ref()
            .is_some_and(|uid| !evaluation.ceiling.concepts.contains(uid))
    {
        return Ok(false);
    }
    if let Some(uid) = &assertion.object_uid {
        if !evaluation.reference_readable(uid, budget)? {
            return Ok(false);
        }
    }
    for grant in grants {
        budget.step()?;
        if grant.predicate_uid == assertion.predicate_uid
            && grant.role == assertion.role
            && (assertion.quantity.is_none()
                || grant.properties.contains(&AssertionProperty::Quantity))
            && (assertion.unit_uid.is_none() || grant.properties.contains(&AssertionProperty::Unit))
            && match (&grant.target, &assertion.object_uid) {
                (AssertionTarget::Unary, None) => true,
                (AssertionTarget::Record(expected), Some(actual)) => expected == actual,
                (AssertionTarget::AnyReadableRecord, Some(_)) => true,
                _ => false,
            }
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn changed_properties(
    before: Option<&RecordState>,
    before_content: Option<&RecordContent>,
    after: &RecordState,
    after_content: &RecordContent,
    budget: &mut Budget<'_>,
) -> Result<BTreeSet<Property>, AuthorityError> {
    let mut properties = BTreeSet::new();
    macro_rules! changed {
        ($field:ident, $property:ident) => {
            if before_content.is_none_or(|before| before.$field != after_content.$field) {
                properties.insert(Property::$property);
            }
        };
    }
    if before.is_none_or(|before| before.kind != after.kind) {
        properties.insert(Property::Kind);
    }
    changed!(head, Head);
    changed!(body, Body);
    changed!(quantity, Quantity);
    if before_content.is_some() || after_content.slug.is_some() {
        changed!(slug, Slug);
    }
    if before_content.is_some() || after_content.unit_uid.is_some() {
        changed!(unit_uid, Unit);
    }
    if before_content.is_some() || after_content.place_uid.is_some() {
        changed!(place_uid, Place);
    }
    if before.is_none() && after.organ_uid.is_some() {
        properties.insert(Property::Organ);
    }
    for (property, value) in &after_content.extensions {
        budget.step()?;
        if before_content.and_then(|before| before.extensions.get(property)) != Some(value) {
            properties.insert(Property::Extension(property.clone()));
        }
    }
    if let Some(before) = before_content {
        for property in before.extensions.keys() {
            budget.step()?;
            if !after_content.extensions.contains_key(property) {
                properties.insert(Property::Extension(property.clone()));
            }
        }
    }
    Ok(properties)
}
