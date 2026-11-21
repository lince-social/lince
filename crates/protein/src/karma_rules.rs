//! Karma rule listings and the visibility of their condition inputs.

use crate::{Predicate, Protein, ProteinError, Source, execute_for_with_signer};
use nucleus::{
    expr::ASSERTION,
    karma::Consequence,
    transfer::karma::{Snapshot, is_agreement_reading, is_reading},
};
use serde_json::{Value, from_value, json};
use std::collections::HashSet;
use store::{
    Store, concepts,
    karma_fields::{for_rule, identity, invalid},
    ledger::records_with_concept,
    records, recurrence,
    sqlx::{SqlitePool, query_scalar},
};

pub(crate) async fn execute(
    store: &Store,
    query: &Protein,
    visible: Option<&HashSet<String>>,
    actor: Option<&str>,
    signer: Option<&str>,
) -> Result<Vec<Value>, ProteinError> {
    if query.aggregate.is_some() || !query.order.is_empty() {
        return Err(invalid(
            "Karma rules are listed newest first; aggregation is not supported",
        ));
    }
    let pool = &store.pool;
    let mut rows = Vec::new();
    for rule in recurrence::all(pool).await? {
        let Some(condition) = &rule.condition else {
            continue;
        };
        let target = if let Some(transfer) = rule
            .consequences
            .iter()
            .find_map(Consequence::transfer_target)
        {
            let Some(state) = transfer_snapshot(store, transfer, actor, signer).await? else {
                continue;
            };
            if rule.consequences.iter().any(|effect| {
                effect
                    .transfer_person()
                    .is_none_or(|person| !state.participants.contains_key(person))
            }) {
                continue;
            }
            transfer
        } else {
            if let Some(visible) = visible
                && !readable(pool, visible, &rule.record_uid).await?
            {
                continue;
            }
            &rule.record_uid
        };

        let mut allowed = true;
        let parsed = condition
            .parsed()
            .map_err(|error| invalid(&error.to_string()))?;
        for read in parsed.reads_ref() {
            if is_reading(&read.func) {
                let mut names = read.slug.split('|');
                let transfer = names.next().unwrap_or_default();
                let state = transfer_snapshot(store, transfer, actor, signer).await?;
                allowed &= state.as_ref().is_some_and(|state| {
                    !is_agreement_reading(&read.func)
                        || names
                            .next()
                            .is_some_and(|person| state.participants.contains_key(person))
                });
            } else if let Some(visible) = visible {
                allowed &= visible_input(pool, visible, &read.func, &read.slug).await?;
            }
        }
        if !allowed {
            continue;
        }
        for predicate in &query.filter {
            match predicate {
                Predicate::UidEq(uid) => allowed &= uid == &rule.uid,
                Predicate::RecordEq(uid) => {
                    allowed &= uid == target
                        || records::resolve(pool, uid)
                            .await?
                            .is_some_and(|record| record.uid == target);
                }
                _ => return Err(invalid("Karma rules support Record and UID filters")),
            }
        }
        if !allowed {
            continue;
        }
        let fields = for_rule(pool, &rule.uid).await?;
        let identity = identity(pool, &rule.uid).await?;
        let (name, slug) = identity
            .as_ref()
            .map_or(("", ""), |id| (id.name.as_str(), id.slug.as_str()));
        rows.push(json!({
            "name": name, "slug": slug, "uid": rule.uid, "record": target,
            "bindings": condition.bindings, "fields": fields, "revision": rule.revision, "state": rule.state,
        }));
        if query.limit.is_some_and(|limit| rows.len() >= limit) {
            break;
        }
    }
    Ok(rows)
}

async fn visible_input(
    pool: &SqlitePool,
    visible: &HashSet<String>,
    function: &str,
    token: &str,
) -> Result<bool, ProteinError> {
    let mut allowed = true;
    match function {
        ASSERTION | "demand" => {
            if let Some(concept) = concepts::resolve(pool, token).await? {
                for uid in records_with_concept(pool, &concept).await? {
                    allowed &= readable(pool, visible, &uid).await?;
                }
            }
        }
        "promise_state" | "confidence" => {
            let record: Option<String> =
                query_scalar("SELECT record_uid FROM promise WHERE uid = ?")
                    .bind(token)
                    .fetch_optional(pool)
                    .await?;
            return match record {
                Some(uid) => readable(pool, visible, &uid).await,
                None => Ok(false),
            };
        }
        _ => {
            for slug in token.split('|') {
                allowed &= readable(pool, visible, slug).await?;
            }
        }
    }
    Ok(allowed)
}

async fn transfer_snapshot(
    store: &Store,
    transfer: &str,
    actor: Option<&str>,
    signer: Option<&str>,
) -> Result<Option<Snapshot>, ProteinError> {
    let query = Protein {
        source: Source::Transfer,
        filter: vec![Predicate::UidEq(transfer.into())],
        fields: Some(vec!["uid".into(), "karma_state".into()]),
        include: Default::default(),
        aggregate: None,
        order: Vec::new(),
        limit: Some(1),
    };
    let rows = Box::pin(execute_for_with_signer(store, &query, actor, signer)).await?;
    Ok(rows
        .into_iter()
        .find(|row| row.get("uid").and_then(Value::as_str) == Some(transfer))
        .and_then(|mut row| from_value(row.as_object_mut()?.remove("karma_state")?).ok()))
}

async fn readable(
    pool: &SqlitePool,
    visible: &HashSet<String>,
    token: &str,
) -> Result<bool, ProteinError> {
    Ok(records::resolve(pool, token)
        .await?
        .is_some_and(|record| visible.contains(&record.uid)))
}
