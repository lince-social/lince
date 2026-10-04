use std::collections::HashSet;

use serde_json::{Value, json};
use store::Store;

use crate::{Predicate, Protein, ProteinError};

pub(crate) async fn execute(
    store: &Store,
    query: &Protein,
    visible: Option<&HashSet<String>>,
    actor: Option<&str>,
    installed_signer_actor: Option<&str>,
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
        let target = rule
            .consequences
            .iter()
            .find_map(nucleus::karma::Consequence::transfer_target)
            .unwrap_or(&rule.record_uid);
        let transfer_state = if rule
            .consequences
            .iter()
            .any(|effect| effect.transfer_target().is_some())
        {
            transfer_snapshot(store, target, actor, installed_signer_actor).await?
        } else {
            None
        };
        if rule
            .consequences
            .iter()
            .any(|effect| effect.transfer_target().is_some())
        {
            if transfer_state.as_ref().is_none_or(|state| {
                rule.consequences.iter().any(|effect| {
                    effect
                        .transfer_person()
                        .is_none_or(|person| !state.participants.contains_key(person))
                })
            }) {
                continue;
            }
        } else if !readable(store, visible, &rule.record_uid).await? {
            continue;
        }
        let parsed = condition
            .parsed()
            .map_err(|error| store::karma_fields::invalid(&error.to_string()))?;
        let mut allowed = true;
        for read in parsed.reads() {
            if nucleus::transfer::karma::is_reading(&read.func) {
                let names: Vec<_> = read.slug.split('|').collect();
                let state =
                    transfer_snapshot(store, names[0], actor, installed_signer_actor).await?;
                allowed &= state.as_ref().is_some_and(|state| {
                    !nucleus::transfer::karma::is_agreement_reading(&read.func)
                        || names
                            .get(1)
                            .is_some_and(|person| state.participants.contains_key(*person))
                });
                continue;
            }
            if visible.is_none() {
                continue;
            }
            if read.func == nucleus::expr::ASSERTION || read.func == "demand" {
                if let Some(concept) = store::concepts::resolve(&store.pool, &read.slug).await? {
                    for uid in store::ledger::records_with_concept(&store.pool, &concept).await? {
                        allowed &= readable(store, visible, &uid).await?;
                    }
                }
            } else if read.func == "promise_state" || read.func == "confidence" {
                let record: Option<String> =
                    store::sqlx::query_scalar("SELECT record_uid FROM promise WHERE uid = ?")
                        .bind(&read.slug)
                        .fetch_optional(&store.pool)
                        .await?;
                allowed &= match record {
                    Some(uid) => readable(store, visible, &uid).await?,
                    None => false,
                };
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
                    if uid != target && store::records::resolve(&store.pool, uid)
                        .await?
                        .is_none_or(|record| record.uid != target) =>
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
            .map_or("", |identity| identity.name.as_str());
        let slug = identity
            .as_ref()
            .map_or("", |identity| identity.slug.as_str());
        rows.push(json!({"name": name, "slug": slug, "uid": rule.uid, "record": target, "bindings": condition.bindings, "fields": fields, "revision": rule.revision, "state": rule.state}));
        if query
            .limit
            .is_some_and(|limit| rows.len() >= limit as usize)
        {
            break;
        }
    }
    Ok(rows)
}

async fn transfer_snapshot(
    store: &Store,
    transfer: &str,
    actor: Option<&str>,
    installed_signer_actor: Option<&str>,
) -> Result<Option<nucleus::transfer::karma::Snapshot>, ProteinError> {
    let query = Protein {
        source: crate::Source::Transfer,
        filter: vec![Predicate::UidEq(transfer.into())],
        fields: Some(vec!["uid".into(), "karma_state".into()]),
        include: Default::default(),
        aggregate: None,
        order: Vec::new(),
        limit: Some(1),
    };
    let rows = Box::pin(crate::execute_for_with_signer(
        store,
        &query,
        actor,
        installed_signer_actor,
    ))
    .await?;
    Ok(rows
        .into_iter()
        .find(|row| row["uid"] == transfer)
        .and_then(|row| serde_json::from_value(row["karma_state"].clone()).ok()))
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
