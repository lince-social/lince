use std::collections::HashSet;

use chrono::{DateTime, Utc};
use nucleus::{DecimalValue, PromiseState};
use serde_json::json;
use store::{
    Store,
    exact::from_f64,
    misc::list_promises,
    recurrence::{self, OccurrenceState},
};

use super::{Scope, Series, Window};
use crate::{ProteinError, sources::promises};

impl<'a> Series<'a> {
    pub(super) async fn read_forecast(
        &mut self,
        store: &Store,
        scope: &'a Scope<'_>,
        protected: &HashSet<String>,
    ) -> Result<(), ProteinError> {
        let window = &scope.window;
        let from = window.now.max(window.from);
        let pool = &store.pool;
        for rule in recurrence::all(pool).await? {
            if !rule
                .consequences
                .capture_concept()
                .is_some_and(|concept| scope.family.contains(concept))
                || !scope.readable(Some(&rule.record_uid))
            {
                continue;
            }
            let unit = scope.unit(Some(&rule.record_uid));
            let occurrences =
                recurrence::occurrences(pool, &rule, from, window.to, window.now).await?;
            self.truncated |= occurrences.truncated;
            for occurrence in occurrences {
                if matches!(
                    occurrence.state,
                    OccurrenceState::Planned | OccurrenceState::Due
                ) && let Some(amount) = occurrence.amount
                {
                    self.add_contributor(
                        window,
                        unit,
                        Contribution {
                            origin: "recurrence",
                            uid: &rule.uid,
                            record: Some(&rule.record_uid),
                            amount,
                            at: occurrence.due_at,
                            state: occurrence.state.as_str(),
                            note: rule.note.as_deref(),
                        },
                    )?;
                }
            }
        }
        for promise in list_promises(pool).await? {
            let record = promise.record_uid.as_deref();
            if !promises::readable(&promise, scope.visible, protected)
                || !promise
                    .concept_uid
                    .as_deref()
                    .is_some_and(|concept| scope.family.contains(concept))
                || !matches!(promise.state, PromiseState::Open | PromiseState::Proposed)
            {
                continue;
            }
            let Some(due) = promise
                .window_end
                .as_deref()
                .and_then(|end| DateTime::parse_from_rfc3339(end).ok())
            else {
                continue;
            };
            let due = due.with_timezone(&Utc);
            if due < from || due >= window.to {
                continue;
            }
            self.add_contributor(
                window,
                scope.unit(record),
                Contribution {
                    origin: "promise",
                    uid: &promise.uid,
                    record,
                    amount: from_f64(promise.delta),
                    at: due,
                    state: promise.state.as_str(),
                    note: promise.condition.as_deref(),
                },
            )?;
        }
        Ok(())
    }

    fn add_contributor(
        &mut self,
        window: &Window,
        unit: &'a str,
        contribution: Contribution<'_>,
    ) -> Result<(), ProteinError> {
        let Contribution {
            origin,
            uid,
            record,
            amount,
            at,
            state,
            note,
        } = contribution;
        let bucket = window.bucket(at);
        self.expected
            .entry(unit)
            .or_default()
            .entry(bucket.clone())
            .or_default()
            .add(amount)?;
        self.contributors.push(json!({
            "kind": "timeline_source", "bucket": bucket, "origin": origin, "uid": uid,
            "record": record, "amount": amount.to_string(), "at": at.to_rfc3339(),
            "state": state, "note": note, "unit": unit,
        }));
        Ok(())
    }
}

struct Contribution<'a> {
    origin: &'static str,
    uid: &'a str,
    record: Option<&'a str>,
    amount: DecimalValue,
    at: DateTime<Utc>,
    state: &'a str,
    note: Option<&'a str>,
}
