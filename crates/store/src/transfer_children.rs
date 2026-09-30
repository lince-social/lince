use chrono::{DateTime, Utc};
use nucleus::{Fact, transfer::TransferChildTerms};
use serde::{Deserialize, Serialize};
use sqlx::{Sqlite, SqliteConnection, SqlitePool, Transaction};

use crate::{StoreError, exact, transfers};

fn invalid(message: impl ToString) -> StoreError {
    StoreError::Protocol(message.to_string())
}

pub async fn terms_on(
    connection: &mut SqliteConnection,
    transfer: &str,
) -> Result<Vec<TransferChildTerms>, StoreError> {
    Ok(sqlx::query_as::<_, (String, bool)>(
        "SELECT t.record_uid, coalesce(r.required, 1) FROM transfer t
         LEFT JOIN transfer_child_requirement r ON r.parent_uid = t.parent_uid AND r.child_uid = t.record_uid
         WHERE t.parent_uid = ? ORDER BY t.record_uid",
    )
    .bind(transfer)
    .fetch_all(connection)
    .await?
    .into_iter()
    .map(|(uid, required)| TransferChildTerms { uid, required })
    .collect())
}

pub(crate) async fn require_participant(
    connection: &mut SqliteConnection,
    parent: &str,
    person: &str,
) -> Result<u64, StoreError> {
    let revision: Option<i64> = sqlx::query_scalar(
        "SELECT t.revision FROM transfer t JOIN record r ON r.uid = t.record_uid
         JOIN record o ON o.slug = 'local-organ' AND o.uid = r.organ_uid
         JOIN transfer_party p ON p.transfer_uid = t.record_uid
         WHERE t.record_uid = ? AND p.actor_uid = ? AND r.deleted_at IS NULL",
    )
    .bind(parent)
    .bind(person)
    .fetch_optional(connection)
    .await?;
    revision
        .filter(|revision| *revision > 0)
        .map(|revision| revision as u64)
        .ok_or_else(|| {
            invalid("changing required children needs an accepted parent participant at its origin")
        })
}

pub(crate) async fn amend<F>(
    tx: &mut Transaction<'_, Sqlite>,
    parent: &str,
    person: &str,
    request: &str,
    intent: Option<&str>,
    now: DateTime<Utc>,
    sign: &F,
) -> Result<Fact, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    let previous = require_participant(tx, parent, person).await?;
    let revision = transfers::advance_transfer_revision(tx, parent, previous)
        .await?
        .map_err(|_| invalid("parent revision changed"))?;
    transfers::reset_agreements_for_revision(tx, parent, revision, now).await?;
    let fact = transfers::insert_revision_fact(
        tx,
        parent,
        revision,
        previous,
        request,
        "change-transfer-children",
        Some(person.into()),
        now,
        sign,
    )
    .await?;
    if let Some(intent) = intent {
        crate::action_intents::link_pending_fact(tx, intent, &fact).await?;
    } else if fact.signature.is_none() {
        return Err(invalid(
            "changing required children needs a participant signature",
        ));
    }
    crate::records::bump_quantity(tx, parent, exact::zero(), &now.to_rfc3339()).await?;
    transfers::cancel_stale_item_delivery(tx, parent, revision).await?;
    Ok(fact)
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Input {
    pub transfer: String,
    pub child: String,
    pub required: bool,
    pub expected_revision: u64,
    pub person: String,
    pub request_id: String,
    #[serde(skip)]
    pub authorization_intent_uid: Option<String>,
}

pub async fn set<F>(
    pool: &SqlitePool,
    input: Input,
    now: DateTime<Utc>,
    sign: F,
) -> Result<Option<Fact>, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    if input.request_id.trim().is_empty() || input.request_id.len() > 200 {
        return Err(invalid("required-child change needs a request identifier"));
    }
    let payload = serde_json::to_string(&input).map_err(invalid)?;
    transfers::ensure_request_not_used_by_invitation_event(pool, &input.request_id).await?;
    let mut tx = crate::write_tx(pool).await?;
    let prior: Option<String> =
        sqlx::query_scalar("SELECT payload FROM transfer_child_request WHERE request_id = ?")
            .bind(&input.request_id)
            .fetch_optional(&mut *tx)
            .await?;
    if let Some(prior) = prior {
        if prior != payload {
            return Err(invalid(
                "required-child request reused with different terms",
            ));
        }
        tx.rollback().await?;
        return Ok(None);
    }
    let revision = require_participant(&mut tx, &input.transfer, &input.person).await?;
    if revision != input.expected_revision {
        return Err(invalid("parent revision changed"));
    }
    let child = terms_on(&mut tx, &input.transfer)
        .await?
        .into_iter()
        .find(|child| child.uid == input.child)
        .ok_or_else(|| invalid("child is not linked to this parent"))?;
    if child.required == input.required {
        return Err(invalid("child requirement is unchanged"));
    }
    sqlx::query("INSERT INTO transfer_child_requirement (parent_uid,child_uid,required) VALUES (?,?,?) ON CONFLICT(parent_uid,child_uid) DO UPDATE SET required = excluded.required")
        .bind(&input.transfer).bind(&input.child).bind(input.required).execute(&mut *tx).await?;
    transfers::validate_dependency_dag(&mut tx, "", &[]).await?;
    let fact = amend(
        &mut tx,
        &input.transfer,
        &input.person,
        &input.request_id,
        input.authorization_intent_uid.as_deref(),
        now,
        &sign,
    )
    .await?;
    sqlx::query("INSERT INTO transfer_child_request (request_id,payload,fact_uid) VALUES (?,?,?)")
        .bind(&input.request_id)
        .bind(payload)
        .bind(&fact.uid)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Some(fact))
}

pub async fn history_children(
    pool: &SqlitePool,
    transfer: &str,
) -> Result<Vec<String>, StoreError> {
    sqlx::query_scalar("SELECT DISTINCT json_extract(child.value, '$.uid') FROM transfer_revision r JOIN fact f ON f.uid = r.fact_uid, json_each(f.payload, '$.terms.children') child WHERE r.transfer_uid = ? ORDER BY 1")
        .bind(transfer).fetch_all(pool).await
}

pub async fn history_parents(pool: &SqlitePool, transfer: &str) -> Result<Vec<String>, StoreError> {
    sqlx::query_scalar("SELECT DISTINCT json_extract(f.payload, '$.terms.transfer.parent_uid') FROM transfer_revision r JOIN fact f ON f.uid = r.fact_uid WHERE r.transfer_uid = ? AND json_extract(f.payload, '$.terms.transfer.parent_uid') IS NOT NULL ORDER BY 1")
        .bind(transfer).fetch_all(pool).await
}
