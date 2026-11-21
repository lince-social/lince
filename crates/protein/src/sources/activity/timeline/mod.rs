//! Timeline windows and their actual and expected quantity changes.

use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{DateTime, Duration, Utc};
use nucleus::DecimalValue;
use serde_json::Value;
use store::{
    Store,
    exact::zero,
    facts::ledger_since,
    ledger::all_fact_concepts,
    records::{GroupingFields, grouping},
    sqlx::Error::Protocol,
};

use super::{DeltaBucket, concept_descendants, resolve_since};
use crate::{GroupBy, Predicate, Protein, ProteinError, transfers::protected_transfer_history};

mod forecast;
mod publication;

const SCAN_CAP: i64 = 100_000;
type Totals<'a> = BTreeMap<&'a str, DecimalValue>;
type Buckets<'a> = BTreeMap<&'a str, BTreeMap<String, DeltaBucket>>;

pub(crate) async fn execute_timeline(
    store: &Store,
    protein: &Protein,
    visible: Option<&HashSet<String>>,
    subject: Option<&str>,
) -> Result<Vec<Value>, ProteinError> {
    let protected = protected_transfer_history(store, subject).await?;
    let now = Utc::now();
    let mut concept = None;
    let mut since = None;
    let mut before = None;
    for predicate in &protein.filter {
        match predicate {
            Predicate::ClassifiedIn(name) => concept = Some(name.as_str()),
            Predicate::AtSince(value) => since = resolve_since(value),
            Predicate::AtBefore(value) => before = resolve_since(value),
            _ => {}
        }
    }
    let concept = concept.ok_or_else(|| {
        Protocol("protein_timeline_concept_required:a timeline needs `classified_in`".into())
    })?;
    let family = concept_descendants(store, concept).await?;
    if family.is_empty() {
        return Ok(Vec::new());
    }
    let parse = |value: &str| {
        DateTime::parse_from_rfc3339(value)
            .map(|at| at.with_timezone(&Utc))
            .map_err(|_| {
                Protocol(format!(
                    "protein_timeline_instant_invalid:`{value}` is not an instant"
                ))
            })
    };
    let window = Window {
        from: since
            .as_deref()
            .map(parse)
            .transpose()?
            .unwrap_or_else(|| now - Duration::days(180)),
        to: before
            .as_deref()
            .map(parse)
            .transpose()?
            .unwrap_or_else(|| now + Duration::days(180)),
        now,
        daily: protein
            .aggregate
            .as_ref()
            .is_some_and(|aggregate| aggregate.by == GroupBy::Day),
    };
    let classifications = all_fact_concepts(&store.pool).await?;
    let units = grouping(
        &store.pool,
        GroupingFields {
            unit: true,
            identity: false,
        },
    )
    .await?
    .into_iter()
    .map(|record| (record.uid, record.unit))
    .collect();
    let scope = Scope {
        window,
        family,
        visible,
        units,
    };
    let mut series = Series::default();
    series.read_history(store, &scope, &classifications).await?;
    series.read_forecast(store, &scope, &protected).await?;
    series.rows(concept, &scope.window)
}

struct Window {
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    now: DateTime<Utc>,
    daily: bool,
}

impl Window {
    fn bucket(&self, at: DateTime<Utc>) -> String {
        at.format(if self.daily { "%Y-%m-%d" } else { "%Y-%m" })
            .to_string()
    }
}

struct Scope<'a> {
    window: Window,
    family: HashSet<String>,
    visible: Option<&'a HashSet<String>>,
    units: HashMap<String, Option<String>>,
}

impl Scope<'_> {
    fn readable(&self, record: Option<&str>) -> bool {
        self.visible
            .is_none_or(|visible| record.is_some_and(|uid| visible.contains(uid)))
    }

    fn unit(&self, record: Option<&str>) -> &str {
        record
            .and_then(|uid| self.units.get(uid))
            .and_then(Option::as_deref)
            .unwrap_or_default()
    }
}

#[derive(Default)]
struct Series<'a> {
    opening: Totals<'a>,
    settled: Totals<'a>,
    actual: Buckets<'a>,
    expected: Buckets<'a>,
    contributors: Vec<Value>,
    truncated: bool,
}

impl<'a> Series<'a> {
    async fn read_history(
        &mut self,
        store: &Store,
        scope: &'a Scope<'_>,
        classifications: &BTreeMap<String, String>,
    ) -> Result<(), ProteinError> {
        let window = &scope.window;
        for fact in ledger_since(&store.pool, None, SCAN_CAP, false).await? {
            let record = Some(fact.record_uid.as_str());
            if !scope.readable(record)
                || !classifications
                    .get(&fact.uid)
                    .is_some_and(|concept| scope.family.contains(concept))
            {
                continue;
            }
            let unit = scope.unit(record);
            if fact.at < window.from {
                accumulate(&mut self.opening, unit, fact.delta)?;
                accumulate(&mut self.settled, unit, fact.delta)?;
                continue;
            }
            if fact.at >= window.to {
                continue;
            }
            if fact.at <= window.now {
                accumulate(&mut self.settled, unit, fact.delta)?;
            }
            self.actual
                .entry(unit)
                .or_default()
                .entry(window.bucket(fact.at))
                .or_default()
                .add(fact.delta)?;
        }
        Ok(())
    }
}

fn accumulate<'a>(
    totals: &mut Totals<'a>,
    unit: &'a str,
    delta: DecimalValue,
) -> Result<(), ProteinError> {
    let slot = totals.entry(unit).or_insert_with(zero);
    *slot = slot.aligned_add(delta).ok_or_else(overflow)?;
    Ok(())
}

fn overflow() -> ProteinError {
    Protocol("protein_timeline_overflow:a total grew past its bounds".into())
}
