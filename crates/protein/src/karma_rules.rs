use std::collections::HashSet;

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
        let parsed = condition
            .parsed()
            .map_err(|error| store::karma_fields::invalid(&error.to_string()))?;
        let mut allowed = true;
        for read in parsed.reads().into_iter().filter(|_| visible.is_some()) {
            if read.func == nucleus::expr::ASSERTION || read.func == "demand" {
                if let Some(concept) = store::concepts::resolve(&store.pool, &read.slug).await? {
                    for uid in store::ledger::records_with_concept(&store.pool, &concept).await? {
                        allowed &= readable(store, visible, &uid).await?;
                    }
                }
            } else if read.func == "promise_state" || read.func == "confidence" {
                let record: Option<String> = store::sqlx::query_scalar("SELECT record_uid FROM promise WHERE uid = ?").bind(&read.slug).fetch_optional(&store.pool).await?;
                allowed &= match record { Some(uid) => readable(store, visible, &uid).await?, None => false };
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
        let fields = store::karma_fields::for_rule(&store.pool, &rule.uid).await?;
        let identity = store::karma_fields::identity(&store.pool, &rule.uid).await?;
        let name = identity
            .as_ref()
            .map_or("Karma rule", |identity| identity.name.as_str());
        let fallback_slug = rule.uid.to_ascii_lowercase().replace('_', "-");
        let slug = identity
            .as_ref()
            .map_or(fallback_slug.as_str(), |identity| identity.slug.as_str());
        rows.push(json!({"name": name, "slug": slug, "uid": rule.uid, "record": rule.record_uid, "bindings": condition.bindings, "fields": fields, "revision": rule.revision, "state": rule.state}));
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
    if visible.is_none() { return Ok(true); }
    Ok(store::records::resolve(&store.pool, token)
        .await?
        .is_some_and(|record| visible.is_none_or(|visible| visible.contains(&record.uid))))
}
