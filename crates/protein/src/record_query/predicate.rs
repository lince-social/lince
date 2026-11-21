use std::{
    collections::{BTreeMap, BTreeSet},
    slice::from_ref,
};

use chrono::NaiveDate;
use nucleus::{DecimalValue, RecordKind};
use utils::vault::MARKER;

use crate::{DateComparison, LinkDirection, Predicate, WorkDateField, authority::AssertionRole};

use super::{
    QueryError, RecordScalarRow,
    budget::{Budget, within},
    snapshot::{Prepared, acyclic, reachable},
};

pub(super) fn validate_shape(
    predicate: &Predicate,
    budget: &mut Budget<'_>,
    depth: usize,
) -> Result<(), QueryError> {
    budget.predicate(depth)?;
    match predicate {
        Predicate::All(children) | Predicate::Any(children) => {
            within(children.len(), budget.remaining_predicates())?;
            for child in children {
                validate_shape(child, budget, depth + 1)?;
            }
            Ok(())
        }
        Predicate::Not(child) => validate_shape(child, budget, depth + 1),
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

pub(super) enum Compiled<'a> {
    All(Vec<Compiled<'a>>),
    Any(Vec<Compiled<'a>>),
    Not(Box<Compiled<'a>>),
    Quantity(QuantityComparison, DecimalValue),
    Members(BTreeSet<&'a str>),
    Uid(&'a str),
    Kind(RecordKind),
    Slug(&'a str),
    Text(String),
    WorkDate(WorkDateField, DateComparison, Option<NaiveDate>),
}

#[derive(Clone, Copy)]
pub(super) enum QuantityComparison {
    Lt,
    Lte,
    Gt,
    Gte,
    Eq,
}

impl Compiled<'_> {
    pub(super) fn matches(
        &self,
        row: &RecordScalarRow,
        budget: &mut Budget<'_>,
    ) -> Result<bool, QueryError> {
        budget.step()?;
        match self {
            Self::All(children) | Self::Any(children) => {
                let short_circuit = matches!(self, Self::Any(_));
                for child in children {
                    if child.matches(row, budget)? == short_circuit {
                        return Ok(short_circuit);
                    }
                }
                Ok(!short_circuit)
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
            Self::Members(members) => Ok(members.contains(row.uid.as_str())),
            Self::Uid(uid) => Ok(row.uid == *uid),
            Self::Kind(kind) => Ok(row.kind == *kind),
            Self::Slug(slug) => Ok(row.slug.as_deref() == Some(slug)),
            Self::Text(query) => {
                let head = row.head.to_lowercase();
                budget.text(&head)?;
                if head.contains(query) {
                    return Ok(true);
                }
                if row.body.trim_start().starts_with(MARKER) {
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

pub(super) fn compile_all<'a>(
    predicates: &'a [Predicate],
    prepared: &Prepared<'a>,
    budget: &mut Budget<'_>,
) -> Result<Vec<Compiled<'a>>, QueryError> {
    predicates
        .iter()
        .map(|predicate| compile(predicate, prepared, budget))
        .collect()
}

fn compile<'a>(
    predicate: &'a Predicate,
    prepared: &Prepared<'a>,
    budget: &mut Budget<'_>,
) -> Result<Compiled<'a>, QueryError> {
    match predicate {
        Predicate::All(children) => Ok(Compiled::All(compile_all(children, prepared, budget)?)),
        Predicate::Any(children) => Ok(Compiled::Any(compile_all(children, prepared, budget)?)),
        Predicate::Not(child) => Ok(Compiled::Not(Box::new(compile(child, prepared, budget)?))),
        Predicate::QuantityLt(value) => Ok(Compiled::Quantity(QuantityComparison::Lt, *value)),
        Predicate::QuantityLte(value) => Ok(Compiled::Quantity(QuantityComparison::Lte, *value)),
        Predicate::QuantityGt(value) => Ok(Compiled::Quantity(QuantityComparison::Gt, *value)),
        Predicate::QuantityGte(value) => Ok(Compiled::Quantity(QuantityComparison::Gte, *value)),
        Predicate::QuantityEq(value) => Ok(Compiled::Quantity(QuantityComparison::Eq, *value)),
        Predicate::UidEq(uid) => {
            budget.text(uid)?;
            prepared.require_record(uid)?;
            Ok(Compiled::Uid(uid))
        }
        Predicate::KindEq(kind) => {
            budget.text(kind)?;
            let kind = RecordKind::parse(kind).ok_or(QueryError::InvalidInput)?;
            Ok(Compiled::Kind(kind))
        }
        Predicate::SlugEq(slug) => {
            budget.text(slug)?;
            Ok(Compiled::Slug(slug))
        }
        Predicate::ConceptIn(uid) => {
            budget.text(uid)?;
            let family = prepared.concept_family(uid, budget)?;
            let mut members = BTreeSet::new();
            for assertion in &prepared.assertions {
                budget.step()?;
                if assertion.role == AssertionRole::Identity
                    && family.contains(assertion.predicate_uid.as_str())
                {
                    members.insert(assertion.subject_uid.as_str());
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
                    || !family.contains(assertion.predicate_uid.as_str())
                {
                    continue;
                }
                let Some(object) = prepared.readable_object(assertion) else {
                    continue;
                };
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
            let mut edges = BTreeMap::<_, BTreeSet<_>>::new();
            for assertion in &prepared.assertions {
                budget.step()?;
                if assertion.role == AssertionRole::Ordinary
                    && family.contains(assertion.predicate_uid.as_str())
                    && let Some(object) = prepared.readable_object(assertion)
                {
                    budget.edge()?;
                    edges
                        .entry(object)
                        .or_default()
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
                members.remove(record.as_str());
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
        Predicate::OrganEq(uid) => compile_organs(from_ref(uid), prepared, budget),
        Predicate::OrganIn(uids) => compile_organs(uids, prepared, budget),
        _ => Err(QueryError::Unsupported),
    }
}

fn compile_organs<'a>(
    uids: &[String],
    prepared: &Prepared<'a>,
    budget: &mut Budget<'_>,
) -> Result<Compiled<'a>, QueryError> {
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
            members.insert(uid.as_str());
        }
    }
    Ok(Compiled::Members(members))
}

fn parse_date(value: &str) -> Result<NaiveDate, QueryError> {
    if !matches!(value.as_bytes(), [a, b, c, d, b'-', e, f, b'-', g, h]
        if [a, b, c, d, e, f, g, h].into_iter().all(u8::is_ascii_digit))
    {
        return Err(QueryError::InvalidInput);
    }
    NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| QueryError::InvalidInput)
}
