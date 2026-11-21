//! Fact reads and exact quantity change summaries.

use std::{
    borrow::Cow,
    collections::{BTreeMap, HashMap, HashSet},
};

use chrono::{DateTime, FixedOffset};
use nucleus::performance::Span;
use serde_json::{Value, json};
use store::{
    Store,
    facts::{LedgerRow, ledger_since},
    ledger::{all_fact_concepts, all_record_concepts},
    records::{GroupingFields, RecordGrouping, grouping, resolve_uid},
    sqlx::Error::Protocol,
};

use super::{DeltaBucket, TOTAL, UNCLASSIFIED, concept_descendants, resolve_since};
use crate::{
    Aggregate, GroupBy, Predicate, Protein, ProteinError, transfers::protected_transfer_history,
};

const SCAN_CAP: i64 = 100_000;

pub(crate) async fn execute_facts(
    store: &Store,
    protein: &Protein,
    visible: Option<&HashSet<String>>,
    subject: Option<&str>,
) -> Result<Vec<Value>, ProteinError> {
    let protected = protected_transfer_history(store, subject).await?;
    let pool = &store.pool;
    let aggregate = protein.aggregate.as_ref();
    let needs_concepts = protein
        .filter
        .iter()
        .any(|predicate| matches!(predicate, Predicate::ConceptIn(_)))
        || aggregate.is_some_and(|aggregate| aggregate.by == GroupBy::Concept);
    let counts_as = if needs_concepts {
        all_record_concepts(pool).await?
    } else {
        BTreeMap::new()
    };
    let records = grouping(
        pool,
        GroupingFields {
            unit: aggregate.is_some(),
            identity: needs_concepts,
        },
    )
    .await?;
    let dimensions = {
        let _prepare = Span::new("protein-fact-dimensions");
        records
            .iter()
            .map(|record| (record.uid.as_str(), record))
            .collect::<HashMap<_, _>>()
    };
    let Some(filter) = Filter::load(store, &protein.filter).await? else {
        return Ok(Vec::new());
    };
    let classification = if filter.classifications.is_some()
        || aggregate.is_some_and(|aggregate| aggregate.by == GroupBy::Classification)
    {
        all_fact_concepts(pool).await?
    } else {
        BTreeMap::new()
    };
    let mut facts =
        ledger_since(pool, filter.since.as_deref(), SCAN_CAP, aggregate.is_none()).await?;
    facts.retain(|fact| {
        let uid = fact.record_uid.as_str();
        if visible.is_some_and(|visible| !visible.contains(uid)) || !filter.matches(fact) {
            return false;
        }
        if let Some(family) = &filter.concepts
            && !dimensions.get(uid).is_some_and(|record| {
                record
                    .identity
                    .as_ref()
                    .into_iter()
                    .chain(counts_as.get(uid).into_iter().flatten())
                    .any(|concept| family.contains(concept))
            })
        {
            return false;
        }
        filter.classifications.as_ref().is_none_or(|family| {
            classification
                .get(&fact.uid)
                .is_some_and(|concept| family.contains(concept))
        })
    });
    if let Some(aggregate) = aggregate {
        return summarize(&facts, aggregate, &dimensions, &counts_as, &classification);
    }
    let start = protein
        .limit
        .map_or(0, |limit| facts.len().saturating_sub(limit));
    Ok(facts
        .into_iter()
        .skip(start)
        .map(|fact| {
            let disclose = !protected.contains(&fact.record_uid);
            json!({
                "uid": fact.uid,
                "record": fact.record_uid,
                "delta": fact.delta.to_f64(),
                "at": fact.at.to_rfc3339(),
                "cause_kind": fact.cause.kind.as_str(),
                "cause": fact.cause.uid.filter(|_| disclose),
                "actor": fact.actor_uid.filter(|_| disclose),
                "payload": fact.payload.filter(|_| disclose),
            })
        })
        .collect())
}

#[derive(Default)]
struct Filter<'a> {
    since: Option<String>,
    before: Option<DateTime<FixedOffset>>,
    cause: Option<&'a str>,
    record: Option<String>,
    concepts: Option<HashSet<String>>,
    classifications: Option<HashSet<String>>,
}

impl<'a> Filter<'a> {
    async fn load(
        store: &Store,
        predicates: &'a [Predicate],
    ) -> Result<Option<Self>, ProteinError> {
        let mut filter = Self::default();
        let mut before = None;
        for predicate in predicates {
            match predicate {
                Predicate::AtSince(value) => filter.since = resolve_since(value),
                Predicate::AtBefore(value) => before = resolve_since(value),
                Predicate::CauseKindEq(kind) => filter.cause = Some(kind),
                Predicate::RecordEq(token) => {
                    filter.record = resolve_uid(&store.pool, token).await?;
                    if filter.record.is_none() {
                        return Ok(None);
                    }
                }
                Predicate::ConceptIn(name) => {
                    filter.concepts = Some(concept_descendants(store, name).await?)
                }
                Predicate::ClassifiedIn(name) => {
                    filter.classifications = Some(concept_descendants(store, name).await?)
                }
                _ => {}
            }
        }
        filter.before = before
            .as_deref()
            .map(|value| {
                DateTime::parse_from_rfc3339(value).map_err(|_| {
                    Protocol(format!(
                        "protein_fact_instant_invalid:`{value}` is not an instant"
                    ))
                })
            })
            .transpose()?;
        Ok(Some(filter))
    }

    fn matches(&self, fact: &LedgerRow) -> bool {
        self.cause
            .is_none_or(|cause| fact.cause.kind.as_str() == cause)
            && self
                .record
                .as_ref()
                .is_none_or(|record| fact.record_uid == *record)
            && self.before.is_none_or(|before| fact.at < before)
    }
}

fn summarize(
    facts: &[LedgerRow],
    aggregate: &Aggregate,
    dimensions: &HashMap<&str, &RecordGrouping>,
    counts_as: &BTreeMap<String, Vec<String>>,
    classification: &BTreeMap<String, String>,
) -> Result<Vec<Value>, ProteinError> {
    let _aggregate = Span::new("protein-fact-aggregate");
    let mut buckets = BTreeMap::<_, DeltaBucket>::new();
    for fact in facts {
        let uid = fact.record_uid.as_str();
        let record = dimensions.get(uid);
        let group = match aggregate.by {
            GroupBy::Total => Cow::Borrowed(TOTAL),
            GroupBy::CauseKind | GroupBy::Kind => Cow::Borrowed(fact.cause.kind.as_str()),
            GroupBy::Day => Cow::Owned(fact.at.format("%Y-%m-%d").to_string()),
            GroupBy::Month => Cow::Owned(fact.at.format("%Y-%m").to_string()),
            GroupBy::Concept => Cow::Borrowed(
                record
                    .and_then(|record| {
                        record
                            .identity
                            .as_deref()
                            .or_else(|| counts_as.get(uid)?.first().map(String::as_str))
                    })
                    .unwrap_or(UNCLASSIFIED),
            ),
            GroupBy::Classification => Cow::Borrowed(
                classification
                    .get(&fact.uid)
                    .map_or(UNCLASSIFIED, String::as_str),
            ),
        };
        let unit = record
            .and_then(|record| record.unit.as_deref())
            .unwrap_or_default();
        buckets.entry((group, unit)).or_default().add(fact.delta)?;
    }
    Ok(buckets
        .into_iter()
        .map(|((group, unit), bucket)| bucket.row(&group, unit, aggregate.op))
        .collect())
}
