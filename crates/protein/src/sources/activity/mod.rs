//! Fact and timeline aggregation.

mod facts;
mod timeline;

pub(crate) use facts::execute_facts;
pub(crate) use timeline::execute_timeline;

use std::collections::HashSet;

use chrono::{DateTime, TimeDelta, Utc};
use nucleus::{DecimalValue, parse_duration};
use serde_json::{Map, Value};
use store::{
    Store,
    concepts::{descendants_including, resolve},
    exact::zero,
    sqlx::Error::Protocol,
};

use crate::{AggregateOp, ProteinError};

pub(in crate::sources) fn resolve_since(value: &str) -> Option<String> {
    if value.trim_end().ends_with(['s', 'm', 'h', 'd'])
        && let Some(secs) = parse_duration(value)
    {
        return Utc::now()
            .checked_sub_signed(TimeDelta::try_seconds(secs)?)
            .map(|at| at.to_rfc3339());
    }
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|_| value.to_owned())
}

pub(super) const UNCLASSIFIED: &str = "(unclassified)";
pub(super) const TOTAL: &str = "(total)";

struct DeltaBucket {
    net: DecimalValue,
    gains: DecimalValue,
    losses: DecimalValue,
    count: i64,
}

impl Default for DeltaBucket {
    fn default() -> Self {
        Self {
            net: zero(),
            gains: zero(),
            losses: zero(),
            count: 0,
        }
    }
}

impl DeltaBucket {
    fn add(&mut self, delta: DecimalValue) -> Result<(), ProteinError> {
        let overflow =
            || Protocol("protein_fact_total_overflow:total exceeds the exact range".into());
        self.net = self.net.aligned_add(delta).ok_or_else(overflow)?;
        if delta.is_positive() {
            self.gains = self.gains.aligned_add(delta).ok_or_else(overflow)?;
        } else if delta.is_negative() {
            self.losses = self.losses.aligned_add(delta).ok_or_else(overflow)?;
        }
        self.count += 1;
        Ok(())
    }

    fn row(&self, group: &str, unit: &str, op: AggregateOp) -> Value {
        let mut row = Map::from_iter([
            ("group".into(), group.into()),
            ("unit_uid".into(), (!unit.is_empty()).then_some(unit).into()),
        ]);
        if matches!(op, AggregateOp::Sum) {
            row.insert("net".into(), self.net.to_string().into());
            row.insert("gains".into(), self.gains.to_string().into());
            row.insert("losses".into(), self.losses.to_string().into());
        }
        row.insert("count".into(), self.count.into());
        Value::Object(row)
    }
}

pub(in crate::sources) async fn concept_descendants(
    store: &Store,
    name: &str,
) -> Result<HashSet<String>, ProteinError> {
    Ok(match resolve(&store.pool, name).await? {
        Some(uid) => descendants_including(&store.pool, &uid)
            .await?
            .into_iter()
            .collect(),
        None => HashSet::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::resolve_since;

    #[test]
    fn date_bounds_reject_invalid_suffixes_and_unrepresentable_durations() {
        assert_eq!(resolve_since("10年"), None);
        assert_eq!(resolve_since("1e30d"), None);
        assert!(resolve_since(" 2d ").is_some());
        let instant = "2026-01-01T10:30:00-03:00";
        assert_eq!(resolve_since(instant).as_deref(), Some(instant));
    }
}
