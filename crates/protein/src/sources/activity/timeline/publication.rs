use std::{cmp::Ordering, collections::BTreeSet};

use nucleus::performance::{Span, add};
use serde_json::{Value, json};
use store::exact::zero;

use super::{Series, Totals, Window, overflow};
use crate::ProteinError;

impl Series<'_> {
    pub(super) fn rows(
        mut self,
        concept: &str,
        window: &Window,
    ) -> Result<Vec<Value>, ProteinError> {
        let _publish = Span::new("protein-timeline-publication");
        let units: BTreeSet<_> = self
            .actual
            .keys()
            .chain(self.expected.keys())
            .copied()
            .collect();
        let per_unit = |totals: &Totals<'_>| {
            Value::Object(
                units
                    .iter()
                    .map(|&unit| (unit.to_owned(), total(totals, unit)))
                    .collect(),
            )
        };
        let scalar = |totals: &Totals<'_>| {
            if units.len() > 1 {
                Value::Null
            } else {
                total(totals, units.first().copied().unwrap_or_default())
            }
        };
        let mut rows = vec![json!({
            "kind": "timeline_context", "concept": concept,
            "from": window.from.to_rfc3339(), "to": window.to.to_rfc3339(), "now": window.now.to_rfc3339(),
            "bucket": if window.daily { "day" } else { "month" },
            "current": scalar(&self.settled), "opening": scalar(&self.opening),
            "current_by_unit": per_unit(&self.settled), "opening_by_unit": per_unit(&self.opening),
            "units": units, "projection_truncated": self.truncated,
        })];

        let present = window.bucket(window.now);
        for unit in units {
            let actual = self.actual.get(unit);
            let expected = self.expected.get(unit);
            let buckets: BTreeSet<_> = actual
                .into_iter()
                .chain(expected)
                .flat_map(|buckets| buckets.keys().map(String::as_str))
                .collect();
            add(
                "protein-timeline-published-buckets",
                u64::try_from(buckets.len()).unwrap_or(u64::MAX),
            );
            let mut running = self.opening.get(unit).copied().unwrap_or_else(zero);
            for bucket in buckets {
                let settled = actual.and_then(|buckets| buckets.get(bucket));
                let declared = expected.and_then(|buckets| buckets.get(bucket));
                for delta in settled.into_iter().chain(declared) {
                    running = running.aligned_add(delta.net).ok_or_else(overflow)?;
                }
                let phase = match bucket.cmp(present.as_str()) {
                    Ordering::Less => "actual",
                    Ordering::Equal => "present",
                    Ordering::Greater => "expected",
                };
                rows.push(json!({
                    "kind": "timeline_point", "bucket": bucket, "unit": unit, "phase": phase,
                    "actual_net": settled.map(|b| b.net.to_string()),
                    "actual_gains": settled.map(|b| b.gains.to_string()),
                    "actual_losses": settled.map(|b| b.losses.to_string()),
                    "actual_count": settled.map_or(0, |b| b.count),
                    "expected_net": declared.map(|b| b.net.to_string()),
                    "expected_gains": declared.map(|b| b.gains.to_string()),
                    "expected_losses": declared.map(|b| b.losses.to_string()),
                    "expected_count": declared.map_or(0, |b| b.count),
                    "cumulative": running.to_string(),
                }));
            }
        }
        self.contributors.sort_by(|a, b| {
            a["at"]
                .as_str()
                .unwrap_or_default()
                .cmp(b["at"].as_str().unwrap_or_default())
        });
        rows.extend(self.contributors);
        Ok(rows)
    }
}

fn total(totals: &Totals<'_>, unit: &str) -> Value {
    Value::String(totals.get(unit).copied().unwrap_or_else(zero).to_string())
}
