use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;

use chrono::{DateTime, FixedOffset, NaiveDate};
use nucleus::{DecimalValue, RecordKind};
use serde_json::Value;

use crate::authority::{AssertionRole, AssertionState, GraphSnapshot, RecordState};
use crate::{
    AggregateOp, DateComparison, GroupBy, LinkDirection, Order, Predicate, Protein, Source,
    WorkDateField,
};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReadableIdentities {
    pub records: BTreeSet<String>,
    pub concepts: BTreeSet<String>,
    pub places: BTreeSet<String>,
    pub assertions: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordScalarRow {
    pub uid: String,
    pub kind: RecordKind,
    pub slug: Option<String>,
    pub head: String,
    pub body: String,
    pub quantity: DecimalValue,
    pub start_date: Option<NaiveDate>,
    pub due_date: Option<NaiveDate>,
    pub created_at: DateTime<FixedOffset>,
    pub updated_at: DateTime<FixedOffset>,
}

#[derive(Debug, Clone)]
pub struct QueryLimits {
    pub records: usize,
    pub concepts: usize,
    pub places: usize,
    pub assertions: usize,
    pub predicate_nodes: usize,
    pub predicate_depth: usize,
    pub graph_edges: usize,
    pub steps: usize,
    pub bytes: usize,
    pub string_bytes: usize,
    pub results: usize,
    pub order_fields: usize,
    pub projection_fields: usize,
}

impl Default for QueryLimits {
    fn default() -> Self {
        Self {
            records: 4096,
            concepts: 4096,
            places: 4096,
            assertions: 32768,
            predicate_nodes: 1024,
            predicate_depth: 12,
            graph_edges: 65536,
            steps: 1_000_000,
            bytes: 16 * 1024 * 1024,
            string_bytes: 1024 * 1024,
            results: 4096,
            order_fields: 8,
            projection_fields: 32,
        }
    }
}

impl QueryLimits {
    fn validate(&self) -> Result<(), QueryError> {
        let maximum = Self::default();
        for (value, hard) in [
            (self.records, maximum.records),
            (self.concepts, maximum.concepts),
            (self.places, maximum.places),
            (self.assertions, maximum.assertions),
            (self.predicate_nodes, maximum.predicate_nodes),
            (self.predicate_depth, maximum.predicate_depth),
            (self.graph_edges, maximum.graph_edges),
            (self.steps, maximum.steps),
            (self.bytes, maximum.bytes),
            (self.string_bytes, maximum.string_bytes),
            (self.results, maximum.results),
            (self.order_fields, maximum.order_fields),
            (self.projection_fields, maximum.projection_fields),
        ] {
            if value == 0 || value > hard {
                return Err(QueryError::InvalidLimits);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryError {
    InvalidLimits,
    LimitExceeded,
    InvalidInput,
    Unavailable,
    Unsupported,
    CyclicGraph,
}

impl fmt::Display for QueryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidLimits => "record query limits are invalid",
            Self::LimitExceeded => "record query limit exceeded",
            Self::InvalidInput => "record query input is invalid",
            Self::Unavailable => "record query dependency is unavailable",
            Self::Unsupported => "record query feature is unsupported",
            Self::CyclicGraph => "record query relation graph is cyclic",
        })
    }
}

impl std::error::Error for QueryError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RecordField {
    Uid,
    Kind,
    Slug,
    Head,
    Body,
    Quantity,
    Concept,
    Unit,
    Place,
    Organ,
    StartDate,
    DueDate,
    CreatedAt,
    UpdatedAt,
    Revision,
    Assertions,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordProjection {
    pub fields: BTreeSet<RecordField>,
    pub extension_namespace: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CountGroup {
    Total,
    Kind(RecordKind),
    Concept(Option<String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CountRow {
    pub group: CountGroup,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryRows {
    Records(Vec<String>),
    Counts(Vec<CountRow>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryResult {
    pub rows: QueryRows,
    pub projection: RecordProjection,
}

struct Budget<'a> {
    limits: &'a QueryLimits,
    steps: usize,
    bytes: usize,
    edges: usize,
    predicates: usize,
}

impl<'a> Budget<'a> {
    fn new(limits: &'a QueryLimits) -> Self {
        Self {
            limits,
            steps: 0,
            bytes: 0,
            edges: 0,
            predicates: 0,
        }
    }

    fn steps(&mut self, count: usize) -> Result<(), QueryError> {
        self.steps = self
            .steps
            .checked_add(count)
            .ok_or(QueryError::LimitExceeded)?;
        within(self.steps, self.limits.steps)
    }

    fn step(&mut self) -> Result<(), QueryError> {
        self.steps(1)
    }

    fn edge(&mut self) -> Result<(), QueryError> {
        self.edges = self.edges.checked_add(1).ok_or(QueryError::LimitExceeded)?;
        within(self.edges, self.limits.graph_edges)
    }

    fn text(&mut self, text: &str) -> Result<(), QueryError> {
        within(text.len(), self.limits.string_bytes)?;
        self.bytes = self
            .bytes
            .checked_add(text.len())
            .ok_or(QueryError::LimitExceeded)?;
        within(self.bytes, self.limits.bytes)
    }

    fn value(&mut self, value: &Value) -> Result<(), QueryError> {
        let mut pending = vec![value];
        while let Some(value) = pending.pop() {
            self.step()?;
            match value {
                Value::Null => self.add_bytes(4)?,
                Value::Bool(true) => self.add_bytes(4)?,
                Value::Bool(false) => self.add_bytes(5)?,
                Value::Number(number) => self.text(&number.to_string())?,
                Value::String(text) => self.text(text)?,
                Value::Array(values) => {
                    within(values.len(), self.limits.steps.saturating_sub(self.steps))?;
                    pending.extend(values);
                }
                Value::Object(values) => {
                    within(values.len(), self.limits.steps.saturating_sub(self.steps))?;
                    for (key, value) in values {
                        self.text(key)?;
                        pending.push(value);
                    }
                }
            }
        }
        Ok(())
    }

    fn add_bytes(&mut self, count: usize) -> Result<(), QueryError> {
        self.bytes = self
            .bytes
            .checked_add(count)
            .ok_or(QueryError::LimitExceeded)?;
        within(self.bytes, self.limits.bytes)
    }

    fn predicate(&mut self, depth: usize) -> Result<(), QueryError> {
        self.step()?;
        within(depth, self.limits.predicate_depth)?;
        self.predicates = self
            .predicates
            .checked_add(1)
            .ok_or(QueryError::LimitExceeded)?;
        within(self.predicates, self.limits.predicate_nodes)
    }
}

fn within(value: usize, limit: usize) -> Result<(), QueryError> {
    if value > limit {
        Err(QueryError::LimitExceeded)
    } else {
        Ok(())
    }
}

struct ValidatedQuery {
    projection: RecordProjection,
    order: Vec<SortKey>,
}

#[derive(Clone, Copy)]
struct SortKey {
    field: SortField,
    descending: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum SortField {
    Head,
    Slug,
    Kind,
    Quantity,
    StartDate,
    DueDate,
    CreatedAt,
    UpdatedAt,
}

fn validate_query(query: &Protein, budget: &mut Budget<'_>) -> Result<ValidatedQuery, QueryError> {
    if query.source != Source::Record {
        return Err(QueryError::Unsupported);
    }
    if query
        .limit
        .is_some_and(|limit| limit > budget.limits.results)
    {
        return Err(QueryError::LimitExceeded);
    }
    within(query.filter.len(), budget.limits.predicate_nodes)?;
    for predicate in &query.filter {
        validate_predicate_shape(predicate, budget, 0)?;
    }
    let projection = validate_projection(query, budget)?;
    within(query.order.len(), budget.limits.order_fields)?;
    let mut order = Vec::with_capacity(query.order.len());
    let mut seen = BTreeSet::new();
    for entry in &query.order {
        budget.step()?;
        let (field, descending) = match entry {
            Order::Asc(field) => (field, false),
            Order::Desc(field) => (field, true),
            Order::Link(_) => return Err(QueryError::Unsupported),
        };
        budget.text(field)?;
        let field = match field.as_str() {
            "head" => SortField::Head,
            "slug" => SortField::Slug,
            "kind" => SortField::Kind,
            "quantity" => SortField::Quantity,
            "start_date" => SortField::StartDate,
            "due_date" => SortField::DueDate,
            "created_at" => SortField::CreatedAt,
            "updated_at" => SortField::UpdatedAt,
            _ => return Err(QueryError::Unsupported),
        };
        if !seen.insert(field) {
            return Err(QueryError::InvalidInput);
        }
        order.push(SortKey { field, descending });
    }
    if let Some(aggregate) = &query.aggregate {
        if aggregate.op != AggregateOp::Count
            || !matches!(
                aggregate.by,
                GroupBy::Total | GroupBy::Kind | GroupBy::Concept
            )
            || query.fields.is_some()
            || query.include.extension.is_some()
            || !query.order.is_empty()
        {
            return Err(QueryError::Unsupported);
        }
    }
    Ok(ValidatedQuery { projection, order })
}

fn validate_projection(
    query: &Protein,
    budget: &mut Budget<'_>,
) -> Result<RecordProjection, QueryError> {
    if query.include.facts.is_some()
        || query.include.promises.is_some()
        || query.include.links.is_some()
        || query.include.threads.is_some()
        || query.include.availability
        || query.include.projection.is_some()
        || query.include.contact
        || query.include.conversations
        || query.include.reference_reads
    {
        return Err(QueryError::Unsupported);
    }
    let mut fields = BTreeSet::from([RecordField::Uid, RecordField::Kind]);
    match &query.fields {
        None => {
            fields.extend([
                RecordField::Slug,
                RecordField::Head,
                RecordField::Quantity,
                RecordField::Concept,
                RecordField::Unit,
                RecordField::Place,
                RecordField::Organ,
                RecordField::StartDate,
                RecordField::DueDate,
                RecordField::CreatedAt,
                RecordField::UpdatedAt,
                RecordField::Revision,
            ]);
        }
        Some(requested) => {
            within(requested.len(), budget.limits.projection_fields)?;
            let mut seen = BTreeSet::new();
            for field in requested {
                budget.step()?;
                budget.text(field)?;
                let field = match field.as_str() {
                    "uid" => RecordField::Uid,
                    "kind" => RecordField::Kind,
                    "slug" => RecordField::Slug,
                    "head" => RecordField::Head,
                    "body" => RecordField::Body,
                    "quantity" => RecordField::Quantity,
                    "concept" => RecordField::Concept,
                    "unit" => RecordField::Unit,
                    "place" => RecordField::Place,
                    "organ" => RecordField::Organ,
                    "start_date" => RecordField::StartDate,
                    "due_date" => RecordField::DueDate,
                    "created_at" => RecordField::CreatedAt,
                    "updated_at" => RecordField::UpdatedAt,
                    "revision" => RecordField::Revision,
                    "assertions" => RecordField::Assertions,
                    _ => return Err(QueryError::Unsupported),
                };
                if !seen.insert(field) {
                    return Err(QueryError::InvalidInput);
                }
                fields.insert(field);
            }
        }
    }
    let extension_namespace = query
        .include
        .extension
        .as_ref()
        .map(|extension| {
            budget.text(&extension.namespace)?;
            if extension.namespace.is_empty() {
                return Err(QueryError::InvalidInput);
            }
            Ok(extension.namespace.clone())
        })
        .transpose()?;
    Ok(RecordProjection {
        fields,
        extension_namespace,
    })
}

fn validate_predicate_shape(
    predicate: &Predicate,
    budget: &mut Budget<'_>,
    depth: usize,
) -> Result<(), QueryError> {
    budget.predicate(depth)?;
    match predicate {
        Predicate::All(children) | Predicate::Any(children) => {
            within(
                children.len(),
                budget
                    .limits
                    .predicate_nodes
                    .saturating_sub(budget.predicates),
            )?;
            for child in children {
                validate_predicate_shape(child, budget, depth + 1)?;
            }
            Ok(())
        }
        Predicate::Not(child) => validate_predicate_shape(child, budget, depth + 1),
        Predicate::QuantityLt(_)
        | Predicate::QuantityLte(_)
        | Predicate::QuantityGt(_)
        | Predicate::QuantityGte(_)
        | Predicate::QuantityEq(_)
        | Predicate::UidEq(_)
        | Predicate::KindEq(_)
        | Predicate::SlugEq(_)
        | Predicate::ConceptIn(_)
        | Predicate::Relation { .. }
        | Predicate::Under { .. }
        | Predicate::TextContains(_)
        | Predicate::WorkDate { .. }
        | Predicate::OrganEq(_)
        | Predicate::OrganIn(_) => Ok(()),
        _ => Err(QueryError::Unsupported),
    }
}

struct Prepared<'a> {
    records: BTreeMap<&'a str, &'a RecordState>,
    concepts: BTreeMap<&'a str, &'a crate::authority::ConceptState>,
    rows: BTreeMap<&'a str, &'a RecordScalarRow>,
    readable: &'a ReadableIdentities,
    assertions: Vec<&'a AssertionState>,
    concept_children: BTreeMap<&'a str, BTreeSet<&'a str>>,
    identity_concepts: BTreeMap<&'a str, &'a str>,
}

impl<'a> Prepared<'a> {
    fn new(
        graph: &'a GraphSnapshot,
        readable: &'a ReadableIdentities,
        scalars: &'a [RecordScalarRow],
        budget: &mut Budget<'_>,
    ) -> Result<Self, QueryError> {
        within(graph.records.len(), budget.limits.records)?;
        within(graph.concepts.len(), budget.limits.concepts)?;
        within(graph.places.len(), budget.limits.places)?;
        within(graph.assertions.len(), budget.limits.assertions)?;
        within(readable.records.len(), budget.limits.records)?;
        within(readable.concepts.len(), budget.limits.concepts)?;
        within(readable.places.len(), budget.limits.places)?;
        within(readable.assertions.len(), budget.limits.assertions)?;
        within(scalars.len(), budget.limits.records)?;

        let mut records = BTreeMap::new();
        for record in &graph.records {
            budget.step()?;
            canonical(&record.uid, "r")?;
            budget.text(&record.uid)?;
            if records.insert(record.uid.as_str(), record).is_some() {
                return Err(QueryError::InvalidInput);
            }
            if let Some(organ) = &record.organ_uid {
                budget.text(organ)?;
            }
            if let Some(content) = &record.content {
                budget.text(&content.head)?;
                budget.text(&content.body)?;
                if let Some(slug) = &content.slug {
                    budget.text(slug)?;
                }
                if let Some(unit) = &content.unit_uid {
                    budget.text(unit)?;
                }
                if let Some(place) = &content.place_uid {
                    budget.text(place)?;
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
            budget.step()?;
            canonical(&concept.uid, "c")?;
            budget.text(&concept.uid)?;
            budget.text(&concept.name)?;
            if concepts.insert(concept.uid.as_str(), concept).is_some() {
                return Err(QueryError::InvalidInput);
            }
        }
        for place in &graph.places {
            budget.step()?;
            canonical(place, "pl")?;
            budget.text(place)?;
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
            canonical(&assertion.uid, "a")?;
            canonical(&assertion.subject_uid, "r")?;
            canonical(&assertion.predicate_uid, "c")?;
            budget.text(&assertion.uid)?;
            budget.text(&assertion.subject_uid)?;
            budget.text(&assertion.predicate_uid)?;
            if indexed_assertions
                .insert(assertion.uid.as_str(), assertion)
                .is_some()
                || !assertion_tuples.insert((
                    assertion.subject_uid.as_str(),
                    assertion.predicate_uid.as_str(),
                    assertion.object_uid.as_deref(),
                ))
            {
                return Err(QueryError::InvalidInput);
            }
            if !records.contains_key(assertion.subject_uid.as_str())
                || !concepts.contains_key(assertion.predicate_uid.as_str())
            {
                return Err(QueryError::InvalidInput);
            }
            if let Some(object) = &assertion.object_uid {
                canonical(object, "r")?;
                budget.text(object)?;
                if !records.contains_key(object.as_str()) {
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
                && (assertion.object_uid.is_some()
                    || assertion.quantity.is_some()
                    || assertion.unit_uid.is_some()
                    || !identity_subjects.insert(assertion.subject_uid.as_str()))
            {
                return Err(QueryError::InvalidInput);
            }
        }

        validate_readable_sets(
            readable,
            &records,
            &concepts,
            &graph.places,
            &indexed_assertions,
            budget,
        )?;

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
            if assertion.role == AssertionRole::Identity
                && identity_concepts
                    .insert(
                        assertion.subject_uid.as_str(),
                        assertion.predicate_uid.as_str(),
                    )
                    .is_some()
            {
                return Err(QueryError::InvalidInput);
            }
            assertions.push(assertion);
        }

        let mut concept_children: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
        for uid in &readable.concepts {
            let concept = concepts
                .get(uid.as_str())
                .copied()
                .ok_or(QueryError::InvalidInput)?;
            concept_children.entry(concept.uid.as_str()).or_default();
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
            budget.step()?;
            canonical(&row.uid, "r")?;
            budget.text(&row.uid)?;
            budget.text(&row.head)?;
            budget.text(&row.body)?;
            if let Some(slug) = &row.slug {
                budget.text(slug)?;
            }
            if rows.insert(row.uid.as_str(), row).is_some() {
                return Err(QueryError::InvalidInput);
            }
            let record = records
                .get(row.uid.as_str())
                .copied()
                .ok_or(QueryError::InvalidInput)?;
            if !readable.records.contains(&row.uid) || record.deleted || row.kind != record.kind {
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
        if rows.len() != readable.records.len()
            || readable
                .records
                .iter()
                .any(|uid| !rows.contains_key(uid.as_str()))
        {
            return Err(QueryError::InvalidInput);
        }

        Ok(Self {
            records,
            concepts,
            rows,
            readable,
            assertions,
            concept_children,
            identity_concepts,
        })
    }

    fn require_record(&self, uid: &str) -> Result<&'a RecordState, QueryError> {
        canonical(uid, "r")?;
        let record = self
            .records
            .get(uid)
            .copied()
            .ok_or(QueryError::Unavailable)?;
        if !self.readable.records.contains(uid) || record.deleted {
            return Err(QueryError::Unavailable);
        }
        Ok(record)
    }

    fn require_concept(&self, uid: &str) -> Result<(), QueryError> {
        canonical(uid, "c")?;
        if self.concepts.contains_key(uid) && self.readable.concepts.contains(uid) {
            Ok(())
        } else {
            Err(QueryError::Unavailable)
        }
    }

    fn require_organ(&self, uid: &str) -> Result<(), QueryError> {
        if self.require_record(uid)?.kind == RecordKind::Organ {
            Ok(())
        } else {
            Err(QueryError::Unavailable)
        }
    }

    fn concept_family(
        &self,
        uid: &str,
        budget: &mut Budget<'_>,
    ) -> Result<BTreeSet<String>, QueryError> {
        self.require_concept(uid)?;
        reachable(uid, &self.concept_children, budget)
    }
}

fn validate_readable_sets(
    readable: &ReadableIdentities,
    records: &BTreeMap<&str, &RecordState>,
    concepts: &BTreeMap<&str, &crate::authority::ConceptState>,
    places: &BTreeSet<String>,
    assertions: &BTreeMap<&str, &AssertionState>,
    budget: &mut Budget<'_>,
) -> Result<(), QueryError> {
    for uid in &readable.records {
        budget.step()?;
        canonical(uid, "r")?;
        budget.text(uid)?;
        let record = records.get(uid.as_str()).ok_or(QueryError::InvalidInput)?;
        if record.deleted {
            return Err(QueryError::InvalidInput);
        }
    }
    for uid in &readable.concepts {
        budget.step()?;
        canonical(uid, "c")?;
        budget.text(uid)?;
        if !concepts.contains_key(uid.as_str()) {
            return Err(QueryError::InvalidInput);
        }
    }
    for uid in &readable.places {
        budget.step()?;
        canonical(uid, "pl")?;
        budget.text(uid)?;
        if !places.contains(uid) {
            return Err(QueryError::InvalidInput);
        }
    }
    for uid in &readable.assertions {
        budget.step()?;
        canonical(uid, "a")?;
        budget.text(uid)?;
        if !assertions.contains_key(uid.as_str()) {
            return Err(QueryError::InvalidInput);
        }
    }
    Ok(())
}

fn canonical(uid: &str, prefix: &str) -> Result<(), QueryError> {
    if nucleus::valid_uid(uid, prefix) {
        Ok(())
    } else {
        Err(QueryError::InvalidInput)
    }
}

fn acyclic<'a>(
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

fn reachable(
    root: &str,
    edges: &BTreeMap<&str, BTreeSet<&str>>,
    budget: &mut Budget<'_>,
) -> Result<BTreeSet<String>, QueryError> {
    let mut seen = BTreeSet::from([root.to_string()]);
    let mut queue = VecDeque::from([root.to_string()]);
    while let Some(uid) = queue.pop_front() {
        budget.step()?;
        if let Some(children) = edges.get(uid.as_str()) {
            for child in children {
                budget.step()?;
                if seen.insert((*child).to_string()) {
                    queue.push_back((*child).to_string());
                }
            }
        }
    }
    Ok(seen)
}

enum Compiled {
    All(Vec<Compiled>),
    Any(Vec<Compiled>),
    Not(Box<Compiled>),
    Quantity(QuantityComparison, DecimalValue),
    Members(BTreeSet<String>),
    Kind(RecordKind),
    Slug(String),
    Text(String),
    WorkDate(WorkDateField, DateComparison, Option<NaiveDate>),
}

#[derive(Clone, Copy)]
enum QuantityComparison {
    Lt,
    Lte,
    Gt,
    Gte,
    Eq,
}

impl Compiled {
    fn matches(&self, row: &RecordScalarRow, budget: &mut Budget<'_>) -> Result<bool, QueryError> {
        budget.step()?;
        match self {
            Self::All(children) => {
                for child in children {
                    if !child.matches(row, budget)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Self::Any(children) => {
                for child in children {
                    if child.matches(row, budget)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            Self::Not(child) => Ok(!child.matches(row, budget)?),
            Self::Quantity(comparison, value) => {
                let order = row.quantity.exact_numeric_cmp(*value);
                Ok(match comparison {
                    QuantityComparison::Lt => order.is_lt(),
                    QuantityComparison::Lte => !order.is_gt(),
                    QuantityComparison::Gt => order.is_gt(),
                    QuantityComparison::Gte => !order.is_lt(),
                    QuantityComparison::Eq => order.is_eq(),
                })
            }
            Self::Members(members) => Ok(members.contains(&row.uid)),
            Self::Kind(kind) => Ok(row.kind == *kind),
            Self::Slug(slug) => Ok(row.slug.as_deref() == Some(slug)),
            Self::Text(query) => {
                let head = row.head.to_lowercase();
                budget.text(&head)?;
                if head.contains(query) {
                    return Ok(true);
                }
                if row.body.trim_start().starts_with(utils::vault::MARKER) {
                    return Ok(false);
                }
                let body = row.body.to_lowercase();
                budget.text(&body)?;
                Ok(body.contains(query))
            }
            Self::WorkDate(field, comparison, expected) => {
                let actual = match field {
                    WorkDateField::Start => row.start_date,
                    WorkDateField::Due => row.due_date,
                };
                Ok(match comparison {
                    DateComparison::Exists => actual.is_some(),
                    DateComparison::Eq => actual.zip(*expected).is_some_and(|(a, b)| a == b),
                    DateComparison::Lt => actual.zip(*expected).is_some_and(|(a, b)| a < b),
                    DateComparison::Lte => actual.zip(*expected).is_some_and(|(a, b)| a <= b),
                    DateComparison::Gt => actual.zip(*expected).is_some_and(|(a, b)| a > b),
                    DateComparison::Gte => actual.zip(*expected).is_some_and(|(a, b)| a >= b),
                })
            }
        }
    }
}

fn compile(
    predicate: &Predicate,
    prepared: &Prepared<'_>,
    budget: &mut Budget<'_>,
) -> Result<Compiled, QueryError> {
    match predicate {
        Predicate::All(children) => Ok(Compiled::All(
            children
                .iter()
                .map(|child| compile(child, prepared, budget))
                .collect::<Result<_, _>>()?,
        )),
        Predicate::Any(children) => Ok(Compiled::Any(
            children
                .iter()
                .map(|child| compile(child, prepared, budget))
                .collect::<Result<_, _>>()?,
        )),
        Predicate::Not(child) => Ok(Compiled::Not(Box::new(compile(child, prepared, budget)?))),
        Predicate::QuantityLt(value) => Ok(Compiled::Quantity(QuantityComparison::Lt, *value)),
        Predicate::QuantityLte(value) => Ok(Compiled::Quantity(QuantityComparison::Lte, *value)),
        Predicate::QuantityGt(value) => Ok(Compiled::Quantity(QuantityComparison::Gt, *value)),
        Predicate::QuantityGte(value) => Ok(Compiled::Quantity(QuantityComparison::Gte, *value)),
        Predicate::QuantityEq(value) => Ok(Compiled::Quantity(QuantityComparison::Eq, *value)),
        Predicate::UidEq(uid) => {
            budget.text(uid)?;
            prepared.require_record(uid)?;
            Ok(Compiled::Members(BTreeSet::from([uid.clone()])))
        }
        Predicate::KindEq(kind) => {
            budget.text(kind)?;
            let kind = RecordKind::parse(kind).ok_or(QueryError::InvalidInput)?;
            Ok(Compiled::Kind(kind))
        }
        Predicate::SlugEq(slug) => {
            budget.text(slug)?;
            Ok(Compiled::Slug(slug.clone()))
        }
        Predicate::ConceptIn(uid) => {
            budget.text(uid)?;
            let family = prepared.concept_family(uid, budget)?;
            let mut members = BTreeSet::new();
            for assertion in &prepared.assertions {
                budget.step()?;
                if assertion.role == AssertionRole::Identity
                    && family.contains(&assertion.predicate_uid)
                {
                    members.insert(assertion.subject_uid.clone());
                }
            }
            Ok(Compiled::Members(members))
        }
        Predicate::Relation {
            kind,
            direction,
            other,
        } => {
            budget.text(kind)?;
            let family = prepared.concept_family(kind, budget)?;
            if let Some(other) = other {
                budget.text(other)?;
                prepared.require_record(other)?;
            }
            let mut members = BTreeSet::new();
            for assertion in &prepared.assertions {
                budget.step()?;
                if assertion.role != AssertionRole::Ordinary
                    || !family.contains(&assertion.predicate_uid)
                {
                    continue;
                }
                let Some(object) = &assertion.object_uid else {
                    continue;
                };
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
            Ok(Compiled::Members(members))
        }
        Predicate::Under {
            record,
            kind,
            include_self,
        } => {
            budget.text(record)?;
            budget.text(kind)?;
            prepared.require_record(record)?;
            let family = prepared.concept_family(kind, budget)?;
            let mut edges: BTreeMap<&str, BTreeSet<&str>> = prepared
                .readable
                .records
                .iter()
                .map(|uid| (uid.as_str(), BTreeSet::new()))
                .collect();
            for assertion in &prepared.assertions {
                budget.step()?;
                if assertion.role != AssertionRole::Ordinary
                    || !family.contains(&assertion.predicate_uid)
                {
                    continue;
                }
                if let Some(object) = assertion.object_uid.as_deref() {
                    budget.edge()?;
                    edges
                        .get_mut(object)
                        .ok_or(QueryError::InvalidInput)?
                        .insert(assertion.subject_uid.as_str());
                }
            }
            acyclic(
                prepared.readable.records.iter().map(String::as_str),
                &edges,
                budget,
            )?;
            let mut members = reachable(record, &edges, budget)?;
            if !include_self {
                members.remove(record);
            }
            Ok(Compiled::Members(members))
        }
        Predicate::TextContains(text) => {
            budget.text(text)?;
            let text = text.to_lowercase();
            budget.text(&text)?;
            Ok(Compiled::Text(text))
        }
        Predicate::WorkDate { field, op, value } => {
            let date = match op {
                DateComparison::Exists => {
                    if value.is_some() {
                        return Err(QueryError::InvalidInput);
                    }
                    None
                }
                _ => {
                    let value = value.as_deref().ok_or(QueryError::InvalidInput)?;
                    budget.text(value)?;
                    Some(parse_date(value)?)
                }
            };
            Ok(Compiled::WorkDate(*field, *op, date))
        }
        Predicate::OrganEq(uid) => compile_organs(std::slice::from_ref(uid), prepared, budget),
        Predicate::OrganIn(uids) => compile_organs(uids, prepared, budget),
        _ => Err(QueryError::Unsupported),
    }
}

fn compile_organs(
    uids: &[String],
    prepared: &Prepared<'_>,
    budget: &mut Budget<'_>,
) -> Result<Compiled, QueryError> {
    within(uids.len(), budget.limits.records)?;
    let mut organs = BTreeSet::new();
    for uid in uids {
        budget.step()?;
        budget.text(uid)?;
        prepared.require_organ(uid)?;
        organs.insert(uid.as_str());
    }
    let mut members = BTreeSet::new();
    for uid in &prepared.readable.records {
        budget.step()?;
        let record = prepared
            .records
            .get(uid.as_str())
            .copied()
            .ok_or(QueryError::InvalidInput)?;
        if record
            .organ_uid
            .as_deref()
            .is_some_and(|organ| organs.contains(organ))
        {
            members.insert(uid.clone());
        }
    }
    Ok(Compiled::Members(members))
}

fn parse_date(value: &str) -> Result<NaiveDate, QueryError> {
    if value.len() != 10
        || value.as_bytes()[4] != b'-'
        || value.as_bytes()[7] != b'-'
        || value
            .bytes()
            .enumerate()
            .any(|(index, byte)| index != 4 && index != 7 && !byte.is_ascii_digit())
    {
        return Err(QueryError::InvalidInput);
    }
    NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| QueryError::InvalidInput)
}

pub fn select_records(
    query: &Protein,
    graph: &GraphSnapshot,
    readable: &ReadableIdentities,
    scalars: &[RecordScalarRow],
    limits: &QueryLimits,
) -> Result<QueryResult, QueryError> {
    limits.validate()?;
    let mut budget = Budget::new(limits);
    let validated = validate_query(query, &mut budget)?;
    let prepared = Prepared::new(graph, readable, scalars, &mut budget)?;
    let compiled = Compiled::All(
        query
            .filter
            .iter()
            .map(|predicate| compile(predicate, &prepared, &mut budget))
            .collect::<Result<_, _>>()?,
    );
    let mut selected = Vec::new();
    for row in prepared.rows.values() {
        budget.step()?;
        if compiled.matches(row, &mut budget)? {
            selected.push(*row);
        }
    }
    let rows = match &query.aggregate {
        Some(aggregate) => QueryRows::Counts(counts(
            &selected,
            aggregate.by,
            &prepared,
            query.limit,
            limits.results,
            &mut budget,
        )?),
        None => {
            order_records(&mut selected, &validated.order, &mut budget)?;
            let requested = query.limit.unwrap_or(limits.results);
            if query.limit.is_none() && selected.len() > limits.results {
                return Err(QueryError::LimitExceeded);
            }
            selected.truncate(requested);
            QueryRows::Records(selected.into_iter().map(|row| row.uid.clone()).collect())
        }
    };
    Ok(QueryResult {
        rows,
        projection: validated.projection,
    })
}

fn counts(
    selected: &[&RecordScalarRow],
    by: GroupBy,
    prepared: &Prepared<'_>,
    limit: Option<usize>,
    result_limit: usize,
    budget: &mut Budget<'_>,
) -> Result<Vec<CountRow>, QueryError> {
    let mut out = match by {
        GroupBy::Total => vec![CountRow {
            group: CountGroup::Total,
            count: selected.len(),
        }],
        GroupBy::Kind => {
            let mut groups = BTreeMap::new();
            for row in selected {
                budget.step()?;
                let count = groups.entry(row.kind.as_str()).or_insert(0usize);
                *count = count.checked_add(1).ok_or(QueryError::LimitExceeded)?;
            }
            groups
                .into_iter()
                .map(|(kind, count)| CountRow {
                    group: CountGroup::Kind(
                        RecordKind::parse(kind).expect("RecordKind::as_str round-trips"),
                    ),
                    count,
                })
                .collect()
        }
        GroupBy::Concept => {
            let mut groups = BTreeMap::new();
            for row in selected {
                budget.step()?;
                let concept = prepared.identity_concepts.get(row.uid.as_str()).copied();
                let count = groups.entry(concept).or_insert(0usize);
                *count = count.checked_add(1).ok_or(QueryError::LimitExceeded)?;
            }
            let mut present = groups
                .iter()
                .filter_map(|(concept, count)| {
                    concept.map(|concept| CountRow {
                        group: CountGroup::Concept(Some(concept.to_string())),
                        count: *count,
                    })
                })
                .collect::<Vec<_>>();
            if let Some(count) = groups.get(&None) {
                present.push(CountRow {
                    group: CountGroup::Concept(None),
                    count: *count,
                });
            }
            present
        }
        _ => return Err(QueryError::Unsupported),
    };
    let requested = limit.unwrap_or(result_limit);
    if limit.is_none() && out.len() > result_limit {
        return Err(QueryError::LimitExceeded);
    }
    out.truncate(requested);
    Ok(out)
}

fn order_records(
    rows: &mut Vec<&RecordScalarRow>,
    order: &[SortKey],
    budget: &mut Budget<'_>,
) -> Result<(), QueryError> {
    let levels = if rows.len() <= 1 {
        1
    } else {
        usize::BITS as usize - (rows.len() - 1).leading_zeros() as usize + 1
    };
    let comparisons = rows
        .len()
        .checked_mul(levels)
        .and_then(|value| value.checked_mul(order.len().max(1)))
        .ok_or(QueryError::LimitExceeded)?;
    budget.steps(comparisons)?;
    rows.sort_by(|left, right| {
        for key in order {
            let compared = compare_field(left, right, *key);
            if compared != Ordering::Equal {
                return compared;
            }
        }
        left.uid.cmp(&right.uid)
    });
    Ok(())
}

fn compare_field(left: &RecordScalarRow, right: &RecordScalarRow, key: SortKey) -> Ordering {
    match key.field {
        SortField::Head => direction(left.head.cmp(&right.head), key.descending),
        SortField::Slug => optional(left.slug.as_ref(), right.slug.as_ref(), key.descending),
        SortField::Kind => direction(left.kind.as_str().cmp(right.kind.as_str()), key.descending),
        SortField::Quantity => direction(
            left.quantity.exact_numeric_cmp(right.quantity),
            key.descending,
        ),
        SortField::StartDate => optional(
            left.start_date.as_ref(),
            right.start_date.as_ref(),
            key.descending,
        ),
        SortField::DueDate => optional(
            left.due_date.as_ref(),
            right.due_date.as_ref(),
            key.descending,
        ),
        SortField::CreatedAt => direction(left.created_at.cmp(&right.created_at), key.descending),
        SortField::UpdatedAt => direction(left.updated_at.cmp(&right.updated_at), key.descending),
    }
}

fn optional<T: Ord>(left: Option<&T>, right: Option<&T>, descending: bool) -> Ordering {
    match (left, right) {
        (Some(left), Some(right)) => direction(left.cmp(right), descending),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

fn direction(order: Ordering, descending: bool) -> Ordering {
    if descending { order.reverse() } else { order }
}
