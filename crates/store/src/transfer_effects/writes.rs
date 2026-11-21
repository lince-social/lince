//! Records private effect Facts and reverses their quantities.

use std::borrow::Cow;

use chrono::{DateTime, Utc};
use nucleus::{Cause, CauseKind, Fact, NewFact, fact::seal};
use serde_json::json;
use sqlx::{Row, Sqlite, Transaction};

use super::{Group, invalid, mode};
use crate::{StoreError, action_intents::link_pending_fact, exact, facts, records::bump_quantity};

pub(crate) async fn apply_on<F>(
    tx: &mut Transaction<'_, Sqlite>,
    group: Option<&Group>,
    primary: &Fact,
    occurrence: &str,
    intent: Option<&str>,
    now: DateTime<Utc>,
    sign: &F,
) -> Result<(), StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    let Some(group) = group else { return Ok(()) };
    if group.effects.first().is_none_or(|effect| {
        effect.record != primary.record_uid
            || !effect.delta.exact_numeric_cmp(primary.delta).is_eq()
    }) {
        return Err(invalid(
            "the primary application does not match the reviewed group",
        ));
    }
    let person = primary
        .actor_uid
        .as_deref()
        .ok_or_else(|| invalid("the primary application has no Person actor"))?;
    for (index, effect) in group.effects.iter().enumerate() {
        let fact_uid = if index == 0 {
            Cow::Borrowed(primary.uid.as_str())
        } else {
            let payload = json!({
                "action": "apply-transfer-private-effect",
                "primary_fact": primary.uid,
                "occurrence": occurrence,
                "mode": effect.mode,
                "local_unit_uid": effect.unit,
                "review_hash": group.hash,
            });
            let mut fact = seal(
                NewFact {
                    uid: None,
                    record_uid: effect.record.clone(),
                    delta: effect.delta,
                    at: None,
                    actor_uid: Some(person.into()),
                    cause: primary.cause.clone(),
                    payload: Some(payload.to_string()),
                },
                &facts::last_hash(tx).await?,
                now,
            );
            fact.signature = sign(&fact.hash);
            if fact.signature.is_none() && intent.is_none() {
                return Err(invalid("a private effect needs its owner's signature"));
            }
            facts::insert(tx, &fact).await?;
            if let Some(intent) = intent {
                link_pending_fact(tx, intent, &fact).await?;
            }
            Cow::Owned(fact.uid)
        };
        sqlx::query(
            "INSERT INTO transfer_private_effect
             (fact_uid, primary_fact_uid, occurrence_uid, transfer_uid, exchange_uid,
              person_uid, record_uid, mode, unit_uid, formula, review_hash)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(fact_uid.as_ref())
        .bind(&primary.uid)
        .bind(occurrence)
        .bind(&group.transfer)
        .bind(&group.exchange)
        .bind(person)
        .bind(&effect.record)
        .bind(mode(effect.mode))
        .bind(&effect.unit)
        .bind(&effect.formula)
        .bind(&group.hash)
        .execute(&mut **tx)
        .await?;
    }
    let at = now.to_rfc3339();
    for effect in group.effects.iter().skip(1) {
        bump_quantity(tx, &effect.record, effect.delta, &at).await?;
    }
    Ok(())
}

pub(crate) async fn compensate_on<F>(
    tx: &mut Transaction<'_, Sqlite>,
    primary: &str,
    person: &str,
    intent: Option<&str>,
    now: DateTime<Utc>,
    sign: &F,
) -> Result<(), StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    let rows = sqlx::query(
        "SELECT e.fact_uid, e.record_uid, e.person_uid, e.unit_uid, f.delta_mantissa, f.delta_scale,
                r.unit_uid AS current_unit, r.deleted_at, r.organ_uid,
                (SELECT uid FROM record WHERE slug = 'local-organ'
                 AND kind = 'organ' AND deleted_at IS NULL) AS own_organ
         FROM transfer_private_effect e JOIN fact f ON f.uid = e.fact_uid
         LEFT JOIN record r ON r.uid = e.record_uid
         WHERE e.primary_fact_uid = ?1 AND e.fact_uid != ?1 ORDER BY f.rowid",
    )
    .bind(primary)
    .fetch_all(&mut **tx)
    .await?;
    let at = now.to_rfc3339();
    for row in rows {
        let organ: Option<&str> = row.get("organ_uid");
        if row.get::<&str, _>("person_uid") != person
            || row.get::<Option<&str>, _>("deleted_at").is_some()
            || organ.is_none()
            || organ != row.get("own_organ")
            || row.get::<Option<&str>, _>("unit_uid") != row.get::<Option<&str>, _>("current_unit")
        {
            return Err(invalid(
                "restore every original private Record and unit before correcting the group",
            ));
        }
        let original: &str = row.get("fact_uid");
        let record: &str = row.get("record_uid");
        let delta = exact::negate(exact::read_decimal(&row, "delta")?)?;
        let payload = json!({
            "action": "compensate-transfer-private-effect",
            "primary_fact": primary,
            "original_fact": original,
        });
        let mut fact = seal(
            NewFact {
                uid: None,
                record_uid: record.into(),
                delta,
                at: None,
                actor_uid: Some(person.into()),
                cause: Cause {
                    kind: CauseKind::Compensation,
                    uid: Some(original.into()),
                },
                payload: Some(payload.to_string()),
            },
            &facts::last_hash(tx).await?,
            now,
        );
        fact.signature = sign(&fact.hash);
        if fact.signature.is_none() && intent.is_none() {
            return Err(invalid("a private correction needs the owner's signature"));
        }
        facts::insert(tx, &fact).await?;
        if let Some(intent) = intent {
            link_pending_fact(tx, intent, &fact).await?;
        }
        bump_quantity(tx, record, delta, &at).await?;
    }
    Ok(())
}
