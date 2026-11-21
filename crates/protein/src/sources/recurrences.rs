//! Recurrence rules and their occurrences within a query window.

use std::collections::HashSet;

use chrono::{DateTime, Duration, Utc};
use serde_json::{Value, json};
use store::{
    Store,
    records::resolve_uid,
    recurrence::{all, occurrences},
};

use super::activity::resolve_since;
use crate::{Predicate, Protein, ProteinError};

pub(crate) async fn execute_recurrence(
    store: &Store,
    protein: &Protein,
    visible: Option<&HashSet<String>>,
) -> Result<Vec<Value>, ProteinError> {
    let now = Utc::now();
    let pool = &store.pool;
    let mut since = None;
    let mut before = None;
    let mut record = None;
    for predicate in &protein.filter {
        match predicate {
            Predicate::AtSince(value) => since = resolve_since(value),
            Predicate::AtBefore(value) => before = resolve_since(value),
            Predicate::RecordEq(token) => {
                record = resolve_uid(pool, token).await?;
                if record.is_none() {
                    return Ok(Vec::new());
                }
            }
            _ => {}
        }
    }
    let parse = |value: Option<&str>| {
        value
            .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
            .map(|at| at.with_timezone(&Utc))
    };
    let from = parse(since.as_deref()).unwrap_or_else(|| now - Duration::days(60));
    let to = parse(before.as_deref()).unwrap_or_else(|| now + Duration::days(90));

    let mut out = Vec::new();
    for rule in all(pool).await? {
        if visible.is_some_and(|visible| !visible.contains(&rule.record_uid))
            || record.as_ref().is_some_and(|uid| uid != &rule.record_uid)
        {
            continue;
        }
        let derived = occurrences(pool, &rule, from, to, now).await?;
        let concept = rule.consequences.capture_concept();
        out.push(json!({
            "kind": "recurrence",
            "uid": rule.uid,
            "record": rule.record_uid,
            "consequences": rule.consequences,
            "amount": rule.consequences.declared_delta().map(|a| a.to_string()),
            "truncated": derived.truncated,
            "concept": concept,
            "condition": rule.condition.as_ref().map(|condition| condition.source.as_str()),
            "gate": rule.condition.as_ref().map(|c| c.gate.as_text()),
            "carry": rule.condition.as_ref().map(|c| c.carry.as_text()),
            "note": rule.note,
            "cadence": rule.cadence,
            "anchor_at": rule.anchor_at,
            "state": rule.state,
            "paused": rule.is_paused(),
            "revision": rule.revision,
        }));
        for occurrence in derived {
            out.push(json!({
                "kind": "occurrence",
                "recurrence": rule.uid,
                "record": rule.record_uid,
                "due_at": occurrence.due_at.to_rfc3339(),
                "state": occurrence.state.as_str(),
                "amount": occurrence.amount.map(|a| a.to_string()),
                "entry": occurrence.entry_uid,
                "concept": concept,
                "note": rule.note,
            }));
        }
    }
    Ok(out)
}
