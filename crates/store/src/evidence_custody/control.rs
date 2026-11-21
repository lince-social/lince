use chrono::DateTime;
use nucleus::{Cause, NewFact, RecordKind, fact::seal, new_uid, visibility::Data};
use serde_json::{Value, to_string};
use sqlx::{Sqlite, Transaction, query_scalar};

use super::invalid;
use crate::{
    StoreError,
    action_intents::link_pending_fact,
    data_visibility::ensure_private_for_person_on,
    exact::zero,
    facts::{insert, last_hash},
    organs::LOCAL_ORGAN_SLUG,
    people::is_active_on,
    records::{NewRecord, create_with_uid_on},
    sync_ops::{OpKind, log_local_tx},
};

pub(crate) struct Control<'a> {
    pub person: &'a str,
    pub actor: &'a str,
    pub head: &'a str,
    pub payload: &'a Value,
    pub now: i64,
    pub intent: Option<&'a str>,
}

pub(crate) async fn control_on(
    tx: &mut Transaction<'_, Sqlite>,
    input: Control<'_>,
    sign: impl Fn(&str) -> Option<String>,
) -> Result<String, StoreError> {
    let Control {
        person,
        actor,
        head,
        payload,
        now,
        intent,
    } = input;
    if !is_active_on(tx, person).await? {
        return Err(invalid("Custody requires an active local operator"));
    }
    let local: String =
        query_scalar("SELECT uid FROM record WHERE slug=? AND kind='organ' AND deleted_at IS NULL")
            .bind(LOCAL_ORGAN_SLUG)
            .fetch_one(&mut **tx)
            .await?;
    let record = new_uid("r");
    create_with_uid_on(
        tx,
        &record,
        NewRecord {
            slug: None,
            kind: RecordKind::Plain,
            head,
            body: "Private custody control. This is not a contribution, payment or physical-delivery confirmation.",
            quantity: zero(),
        },
        &local,
        None,
    )
    .await?;
    if let Some(policy) =
        ensure_private_for_person_on(tx, &record, Data::Record, Some(person)).await?
    {
        log_local_tx(
            tx,
            "data_visibility",
            &record,
            "record",
            OpKind::Set,
            Some(to_string(&policy).map_err(invalid)?),
        )
        .await?;
    }
    let at = DateTime::from_timestamp(now, 0).ok_or_else(|| invalid("Invalid custody time"))?;
    let mut fact = seal(
        NewFact {
            uid: None,
            record_uid: record,
            delta: zero(),
            at: None,
            actor_uid: Some(actor.into()),
            cause: Cause::user_edit(),
            payload: Some(to_string(payload).map_err(invalid)?),
        },
        &last_hash(tx).await?,
        at,
    );
    fact.signature = sign(&fact.hash);
    if fact.signature.is_none() && intent.is_none() {
        return Err(invalid("Custody control requires signed authorization"));
    }
    insert(tx, &fact).await?;
    if let Some(intent) = intent {
        link_pending_fact(tx, intent, &fact).await?;
    }
    Ok(fact.record_uid)
}
