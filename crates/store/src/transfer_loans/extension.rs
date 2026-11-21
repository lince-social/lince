//! Signed proposals to extend previously agreed lending intervals.

use chrono::{DateTime, Utc};
use nucleus::{Fact, transfer::disclosure::TransferItem};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use super::{accepted_on, invalid};
use crate::StoreError;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// A request to propose a later loan end.
pub struct Extension {
    /// Transfer holding the accepted loan.
    pub transfer: String,
    /// Exchange whose end is being extended.
    pub exchange: String,
    /// Proposed RFC 3339 end date.
    pub until: String,
    /// Transfer revision the caller reviewed.
    pub expected_revision: u64,
    /// Participant proposing the extension.
    pub person: String,
    /// Idempotency key for the signed request.
    pub request_id: String,
    #[serde(skip)]
    /// Pending authorization to link to the resulting Fact.
    pub authorization_intent_uid: Option<String>,
}

/// Saved proposal or replay of an earlier request.
#[derive(Debug)]
pub struct Outcome {
    /// Identifier of the saved revision Fact.
    pub uid: String,
    /// New Fact when this request created a proposal.
    pub fact: Option<Fact>,
}

/// Create a signed extension proposal or replay the previous request.
///
/// # Errors
///
/// Returns an error when the request is invalid, stale, unauthorized, or storage fails.
pub async fn extend<F>(
    pool: &SqlitePool,
    input: Extension,
    now: DateTime<Utc>,
    sign: F,
) -> Result<Outcome, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    let reviewed = i64::try_from(input.expected_revision)
        .ok()
        .filter(|&revision| revision < i64::MAX);
    let Some(reviewed) =
        reviewed.filter(|_| !input.request_id.trim().is_empty() && input.request_id.len() <= 200)
    else {
        return Err(invalid(
            "an extension needs a current revision and request identifier",
        ));
    };
    let payload = serde_json::to_string(&input).map_err(invalid)?;
    if let Some((previous, fact)) = sqlx::query_as::<_, (String, String)>(
        "SELECT payload,fact_uid FROM transfer_loan_extension WHERE request_id = ?",
    )
    .bind(&input.request_id)
    .fetch_optional(pool)
    .await?
    {
        if previous != payload {
            return Err(invalid("extension request reused with different terms"));
        }
        return Ok(Outcome {
            uid: fact,
            fact: None,
        });
    }
    crate::transfers::ensure_request_not_used_by_transfer_revision(pool, &input.request_id).await?;
    crate::transfers::ensure_request_not_used_by_invitation_event(pool, &input.request_id).await?;
    let mut tx = crate::write_tx(pool).await?;
    let authority: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM transfer t JOIN record r ON r.uid = t.record_uid JOIN record own ON own.uid = r.organ_uid AND own.slug = 'local-organ' JOIN transfer_party p ON p.transfer_uid = t.record_uid WHERE t.record_uid = ? AND t.revision = ? AND p.actor_uid = ?)")
        .bind(&input.transfer).bind(reviewed).bind(&input.person).fetch_one(&mut *tx).await?;
    if !authority {
        return Err(invalid(
            "extend the current loan as a participant on its origin",
        ));
    }
    let previous = accepted_on(&mut tx, &input.transfer)
        .await?
        .into_iter()
        .find(|loan| loan.exchange == input.exchange)
        .ok_or_else(|| invalid("agree to the original loan before proposing an extension"))?;
    let rows: Vec<(String,String)> = sqlx::query_as("SELECT uid,item_json FROM promise WHERE transfer_uid = ? AND json_extract(item_json,'$.exchange.uid') = ? ORDER BY uid")
        .bind(&input.transfer).bind(&input.exchange).fetch_all(&mut *tx).await?;
    if rows.is_empty() {
        return Err(invalid("the original loan exchange is unavailable"));
    }
    let mut items = Vec::with_capacity(rows.len());
    for (promise, raw) in rows {
        let mut item: TransferItem = serde_json::from_str(&raw).map_err(invalid)?;
        let loan = item
            .loan
            .as_mut()
            .ok_or_else(|| invalid("this exchange is not a loan"))?;
        loan.until = input.until.clone();
        let (from, until) = loan.bounds().map_err(invalid)?;
        if from != previous.from_ms || until <= previous.until_ms {
            return Err(invalid(
                "an extension must keep the start and move the accepted end later",
            ));
        }
        items.push((promise, serde_json::to_string(&item).map_err(invalid)?));
    }
    let revision = crate::transfers::advance_transfer_revision(
        &mut tx,
        &input.transfer,
        input.expected_revision,
    )
    .await?
    .map_err(|_| invalid("the loan changed after review"))?;
    let updated_at = now.to_rfc3339();
    for (promise, item) in items {
        sqlx::query("UPDATE promise SET item_json = ?,revision = ?,updated_at = ? WHERE uid = ?")
            .bind(item)
            .bind(revision)
            .bind(&updated_at)
            .bind(promise)
            .execute(&mut *tx)
            .await?;
    }
    crate::transfers::reset_agreements_for_revision(&mut tx, &input.transfer, revision, now)
        .await?;
    let fact = crate::transfers::insert_revision_fact(
        &mut tx,
        &input.transfer,
        crate::transfers::RevisionChange {
            revision,
            previous_revision: input.expected_revision,
            request_id: &input.request_id,
            action: "propose-transfer-loan-extension",
        },
        Some(input.person),
        now,
        &sign,
    )
    .await?;
    if let Some(intent) = input.authorization_intent_uid {
        crate::action_intents::link_pending_fact(&mut tx, &intent, &fact).await?;
    } else if fact.signature.is_none() {
        return Err(invalid("an extension needs the participant's signature"));
    }
    sqlx::query("INSERT INTO transfer_loan_extension (request_id,payload,revision,fact_uid) VALUES (?,?,?,?)")
        .bind(input.request_id).bind(payload).bind(revision).bind(&fact.uid).execute(&mut *tx).await?;
    crate::records::bump_quantity(&mut tx, &input.transfer, crate::exact::zero(), &updated_at)
        .await?;
    crate::transfers::cancel_stale_item_delivery(&mut tx, &input.transfer, revision).await?;
    tx.commit().await?;
    Ok(Outcome {
        uid: fact.uid.clone(),
        fact: Some(fact),
    })
}
