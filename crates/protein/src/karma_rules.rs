use std::collections::HashSet;

use nucleus::karma::{Condition, rule_field::RuleConsequence};
use serde_json::{Value, json};
use store::Store;

use crate::{Predicate, Protein, ProteinError};

pub(crate) async fn execute(
    store: &Store,
    query: &Protein,
    visible: Option<&HashSet<String>>,
) -> Result<Vec<Value>, ProteinError> {
    if query.aggregate.is_some() || !query.order.is_empty() {
        return Err(store::karma_fields::invalid(
            "Karma rules are listed newest first; aggregation is not supported",
        ));
    }
    let mut rows = Vec::new();
    for rule in store::recurrence::all(&store.pool).await? {
        let Some(condition) = &rule.condition else {
            continue;
        };
        if !readable(store, visible, &rule.record_uid).await? {
            continue;
        }
        let parsed = Condition::parse(&condition.source)
            .map_err(|error| store::karma_fields::invalid(&error.to_string()))?;
        let mut allowed = true;
        for read in parsed.reads() {
            if read.func == nucleus::expr::ASSERTION {
                if let Some(concept) = store::concepts::resolve(&store.pool, &read.slug).await? {
                    for uid in store::ledger::records_with_concept(&store.pool, &concept).await? {
                        allowed &= readable(store, visible, &uid).await?;
                    }
                }
            } else {
                for slug in read.slug.split('|') {
                    allowed &= readable(store, visible, slug).await?;
                }
            }
        }
        if !allowed {
            continue;
        }
        for predicate in &query.filter {
            match predicate {
                Predicate::UidEq(uid) if uid != &rule.uid => allowed = false,
                Predicate::RecordEq(uid)
                    if store::records::resolve(&store.pool, uid)
                        .await?
                        .is_none_or(|record| record.uid != rule.record_uid) =>
                {
                    allowed = false
                }
                Predicate::UidEq(_) | Predicate::RecordEq(_) => {}
                _ => {
                    return Err(store::karma_fields::invalid(
                        "Karma rules support Record and UID filters",
                    ));
                }
            }
        }
        if !allowed {
            continue;
        }
        let mut fields = store::karma_fields::for_rule(&store.pool, &rule.uid).await?;
        for field in &mut fields {
            if field.kind == nucleus::karma::rule_field::RuleFieldKind::Consequence {
                let mut consequence = RuleConsequence::parse(&field.source)
                    .map_err(|error| store::karma_fields::invalid(&error))?;
                if let Some(record) =
                    store::records::resolve(&store.pool, &consequence.target).await?
                {
                    consequence.target = record.slug.unwrap_or(record.uid);
                }
                field.source = consequence.as_text();
            }
        }
        rows.push(json!({"uid": rule.uid, "record": rule.record_uid, "fields": fields, "revision": rule.revision, "state": rule.state}));
        if query
            .limit
            .is_some_and(|limit| rows.len() >= limit as usize)
        {
            break;
        }
    }
    Ok(rows)
}

async fn readable(
    store: &Store,
    visible: Option<&HashSet<String>>,
    token: &str,
) -> Result<bool, ProteinError> {
    Ok(store::records::resolve(&store.pool, token)
        .await?
        .is_some_and(|record| visible.is_none_or(|visible| visible.contains(&record.uid))))
}
