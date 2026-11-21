use chrono::{DateTime, Utc};
use nucleus::{
    Fact,
    transfer::{
        OccurrenceClaimRole, TransferBulkClaimReviewItem, TransferOccurrenceClaimEvidence,
        transfer_bulk_claim_review_token,
    },
};
use sqlx::{Row, SqlitePool};

use super::{ensure_phase4_request_unused, insert_claim, seal_claim, validate_request_key};
use crate::StoreError;

/// An occurrence and the claim state the actor saw during review.
#[derive(Debug, Clone)]
pub struct ReviewedBulkOccurrenceClaim {
    pub occurrence_uid: String,
    pub transfer_uid: String,
    pub expected_revision: u64,
    pub role: OccurrenceClaimRole,
    pub expected_delivery_claimed: bool,
    pub expected_receipt_claimed: bool,
}

/// A request to complete the exact set of claims confirmed by the review token.
#[derive(Debug, Clone)]
pub struct BulkOccurrenceClaimInput {
    pub idempotency_key: String,
    pub actor_person_uid: String,
    pub review_token: String,
    pub items: Vec<ReviewedBulkOccurrenceClaim>,
    pub authorization_intent_uid: Option<String>,
}

/// A validation failure for an item in the reviewed selection.
#[derive(Debug, Clone)]
pub struct BulkOccurrenceClaimFailure {
    pub occurrence_uid: String,
    pub code: &'static str,
    pub message: String,
}

/// A committed bulk request and the facts it wrote in ordinal order.
#[derive(Debug, Clone)]
pub struct BulkOccurrenceClaimOutcome {
    pub uid: String,
    pub facts: Vec<Fact>,
}

/// The result of reviewing and applying a bulk occurrence request.
#[derive(Debug, Clone)]
pub enum BulkOccurrenceClaimCommit {
    Committed(BulkOccurrenceClaimOutcome),
    Replayed(BulkOccurrenceClaimOutcome),
    Rejected(Vec<BulkOccurrenceClaimFailure>),
}

async fn bulk_occurrence_claim_outcome(
    pool: &SqlitePool,
    uid: String,
) -> Result<BulkOccurrenceClaimOutcome, StoreError> {
    let fact_uids: Vec<String> = sqlx::query_scalar(
        "SELECT fact_uid FROM transfer_phase6_bulk_item
         WHERE bulk_uid = ? ORDER BY ordinal",
    )
    .bind(&uid)
    .fetch_all(pool)
    .await?;
    let mut facts = Vec::with_capacity(fact_uids.len());
    for fact_uid in fact_uids {
        facts.push(
            crate::facts::get(pool, &fact_uid)
                .await?
                .ok_or(sqlx::Error::RowNotFound)?,
        );
    }
    Ok(BulkOccurrenceClaimOutcome { uid, facts })
}

/// Returns the bulk ID, actor ID, and review token for a request key.
///
/// # Errors
///
/// Returns a storage error if the request cannot be read.
pub async fn phase6_bulk_request_for_request(
    pool: &SqlitePool,
    request_id: &str,
) -> Result<Option<(String, String, String)>, StoreError> {
    sqlx::query_as(
        "SELECT uid, actor_person_uid, review_token
         FROM transfer_phase6_bulk_request WHERE idempotency_key = ?",
    )
    .bind(request_id.trim())
    .fetch_optional(pool)
    .await
}

/// Completes every reviewed claim atomically or returns the validation failures.
///
/// # Errors
///
/// Returns an error for an invalid or conflicting request key, missing
/// authorization, or a storage failure.
pub async fn complete_occurrence_claims_bulk<F>(
    pool: &SqlitePool,
    mut input: BulkOccurrenceClaimInput,
    now: DateTime<Utc>,
    sign: F,
) -> Result<BulkOccurrenceClaimCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    let request_id = validate_request_key(&input.idempotency_key)?;
    input.items.sort_by(|left, right| {
        left.occurrence_uid
            .cmp(&right.occurrence_uid)
            .then_with(|| left.role.as_str().cmp(right.role.as_str()))
    });
    if input.items.is_empty() {
        return Ok(BulkOccurrenceClaimCommit::Rejected(vec![
            BulkOccurrenceClaimFailure {
                occurrence_uid: "*".into(),
                code: "empty_selection",
                message: "bulk completion requires at least one occurrence".into(),
            },
        ]));
    }
    let reviewed = input
        .items
        .iter()
        .map(|item| TransferBulkClaimReviewItem {
            occurrence_uid: item.occurrence_uid.clone(),
            transfer_uid: item.transfer_uid.clone(),
            transfer_revision: item.expected_revision,
            role: item.role,
            delivery_claimed: item.expected_delivery_claimed,
            receipt_claimed: item.expected_receipt_claimed,
        })
        .collect::<Vec<_>>();
    let expected_token = transfer_bulk_claim_review_token(&input.actor_person_uid, &reviewed);
    if input.review_token.trim() != expected_token {
        return Ok(BulkOccurrenceClaimCommit::Rejected(vec![
            BulkOccurrenceClaimFailure {
                occurrence_uid: "*".into(),
                code: "review_token_mismatch",
                message: "bulk review token does not match the actor and exact selection".into(),
            },
        ]));
    }
    if let Some((uid, actor, token)) = phase6_bulk_request_for_request(pool, request_id).await? {
        let outcome = bulk_occurrence_claim_outcome(pool, uid).await?;
        if actor != input.actor_person_uid || token != expected_token {
            return Err(sqlx::Error::Protocol(
                "bulk completion request id was already used with another review".into(),
            ));
        }
        return Ok(BulkOccurrenceClaimCommit::Replayed(outcome));
    }
    ensure_phase4_request_unused(pool, request_id).await?;

    let mut failures = Vec::new();
    let mut previous_role = None;
    let mut tx = crate::write_tx(pool).await?;
    for item in &input.items {
        // Sorting puts identical occurrence and role pairs next to each other.
        let role = (item.occurrence_uid.as_str(), item.role);
        if previous_role.replace(role) == Some(role) {
            failures.push(BulkOccurrenceClaimFailure {
                occurrence_uid: item.occurrence_uid.clone(),
                code: "duplicate_item",
                message: "occurrence role appears more than once in the reviewed selection".into(),
            });
            continue;
        }
        let Some(row) = sqlx::query(
            "SELECT occurrence.transfer_uid, current_transfer.revision,
                    occurrence.giver_person_uid, occurrence.receiver_person_uid,
                    occurrence.delivery_claimed, occurrence.receipt_claimed,
                    occurrence.disputed, occurrence.system_disputed,
                    EXISTS(
                        SELECT 1 FROM transfer sibling
                        JOIN transfer_source_group_result result
                          ON result.source_uid = sibling.source_uid
                         AND result.policy = sibling.satiation
                        WHERE sibling.record_uid = occurrence.transfer_uid
                          AND result.transfer_uid != sibling.record_uid
                    ) AS satiated,
                    COALESCE((SELECT SUM(slice.canonical_quantity)
                              FROM transfer_occurrence_settlement_slice slice
                              WHERE slice.occurrence_uid = occurrence.uid), 0.0)
                        >= occurrence.quantity AS completed
             FROM transfer_occurrence occurrence
             JOIN transfer current_transfer
               ON current_transfer.record_uid = occurrence.transfer_uid
             WHERE occurrence.uid = ?",
        )
        .bind(&item.occurrence_uid)
        .fetch_optional(&mut *tx)
        .await?
        else {
            failures.push(BulkOccurrenceClaimFailure {
                occurrence_uid: item.occurrence_uid.clone(),
                code: "missing",
                message: "occurrence no longer exists".into(),
            });
            continue;
        };
        let current_transfer: &str = row.get("transfer_uid");
        let current_revision = row.get::<i64, _>("revision").cast_unsigned();
        let delivery = row.get::<i64, _>("delivery_claimed") != 0;
        let receipt = row.get::<i64, _>("receipt_claimed") != 0;
        let expected_actor: &str = match item.role {
            OccurrenceClaimRole::Delivery => row.get("giver_person_uid"),
            OccurrenceClaimRole::Receipt => row.get("receiver_person_uid"),
        };
        let blocker = if current_transfer != item.transfer_uid {
            Some(("transfer_changed", "occurrence belongs to another Transfer"))
        } else if current_revision != item.expected_revision {
            Some(("revision_stale", "signed Transfer revision changed"))
        } else if delivery != item.expected_delivery_claimed
            || receipt != item.expected_receipt_claimed
        {
            Some(("claim_state_stale", "occurrence claim state changed"))
        } else if expected_actor != input.actor_person_uid {
            Some(("unauthorized_role", "actor does not own the reviewed role"))
        } else if match item.role {
            OccurrenceClaimRole::Delivery => delivery,
            OccurrenceClaimRole::Receipt => receipt,
        } {
            Some((
                "already_completed",
                "actor's reviewed role is already claimed",
            ))
        } else if row.get::<i64, _>("disputed") != 0 || row.get::<i64, _>("system_disputed") != 0 {
            Some(("disputed", "occurrence is disputed"))
        } else if row.get::<i64, _>("satiated") != 0 {
            Some(("satiated", "Transfer lost its first-completes source group"))
        } else if row.get::<i64, _>("completed") != 0 {
            Some(("already_settled", "occurrence is already fully settled"))
        } else {
            None
        };
        if let Some((code, message)) = blocker {
            failures.push(BulkOccurrenceClaimFailure {
                occurrence_uid: item.occurrence_uid.clone(),
                code,
                message: message.into(),
            });
        }
    }
    if !failures.is_empty() {
        tx.rollback().await?;
        return Ok(BulkOccurrenceClaimCommit::Rejected(failures));
    }

    let bulk_uid = nucleus::new_uid("tbc");
    let at = now.to_rfc3339();
    sqlx::query(
        "INSERT INTO transfer_phase6_bulk_request
            (uid, idempotency_key, actor_person_uid, review_token, item_count, created_at)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&bulk_uid)
    .bind(request_id)
    .bind(&input.actor_person_uid)
    .bind(&expected_token)
    .bind(input.items.len() as i64)
    .bind(&at)
    .execute(&mut *tx)
    .await?;
    let mut facts: Vec<Fact> = Vec::with_capacity(input.items.len());
    let initial_hash = crate::facts::last_hash(&mut tx).await?;
    for (ordinal, item) in input.items.iter().enumerate() {
        let evidence = TransferOccurrenceClaimEvidence {
            action: "complete-transfer-occurrence-claims-bulk".into(),
            idempotency_key: format!("{request_id}/{ordinal}"),
            transfer_uid: item.transfer_uid.clone(),
            occurrence_uid: item.occurrence_uid.clone(),
            role: item.role,
            asserted: true,
            actor_person_uid: input.actor_person_uid.clone(),
        };
        let previous_hash = facts
            .last()
            .map_or(initial_hash.as_str(), |fact| fact.hash.as_str());
        let mut fact = seal_claim(&evidence, previous_hash, now)?;
        fact.signature = sign(&fact.hash);
        if fact.signature.is_none() && input.authorization_intent_uid.is_none() {
            return Err(sqlx::Error::Protocol(
                "bulk occurrence completion requires a signing key or verified Action intent"
                    .into(),
            ));
        }
        let event_uid = insert_claim(
            &mut tx,
            &evidence,
            &fact,
            &at,
            input.authorization_intent_uid.as_deref(),
        )
        .await?;
        sqlx::query(
            "INSERT INTO transfer_phase6_bulk_item
                (bulk_uid, ordinal, occurrence_uid, transfer_uid, transfer_revision,
                 role, expected_delivery_claimed, expected_receipt_claimed,
                 claim_event_uid, fact_uid)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&bulk_uid)
        .bind(ordinal as i64)
        .bind(&item.occurrence_uid)
        .bind(&item.transfer_uid)
        .bind(item.expected_revision.cast_signed())
        .bind(item.role.as_str())
        .bind(item.expected_delivery_claimed)
        .bind(item.expected_receipt_claimed)
        .bind(&event_uid)
        .bind(&fact.uid)
        .execute(&mut *tx)
        .await?;
        crate::records::bump_quantity(&mut tx, &item.transfer_uid, crate::exact::zero(), &at)
            .await?;
        facts.push(fact);
    }
    tx.commit().await?;
    Ok(BulkOccurrenceClaimCommit::Committed(
        BulkOccurrenceClaimOutcome {
            uid: bulk_uid,
            facts,
        },
    ))
}
