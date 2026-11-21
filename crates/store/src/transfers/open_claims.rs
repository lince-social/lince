//! Pairing and signed replay for claims of OPEN Promises.

use chrono::{DateTime, Utc};
use nucleus::{
    Fact, new_uid,
    transfer::{
        OpenPromiseReusePolicy, TransferRevisionEvidence, TransferRevisionPromise,
        exchange::ExchangeRoute,
    },
};
use sqlx::{Row, Sqlite, SqlitePool, Transaction, sqlite::SqliteRow};

use super::{
    RevisionChange, TransferLocationSnapshot, advance_transfer_revision,
    ensure_request_not_used_by_invitation_event, insert_revision_fact,
    item_terms::{decode_item, encode_item, validate_location},
    reset_agreements_for_revision, revision_fact, validate_request_key,
};
use crate::{StoreError, action_intents::link_pending_fact, exact::zero, records::bump_quantity};

/// The claimant's refined terms and the revision containing the OPEN source.
#[derive(Debug, Clone)]
pub struct OpenPromiseClaimInput {
    pub transfer_uid: String,
    pub source_promise_uid: String,
    pub expected_revision: u64,
    pub idempotency_key: String,
    pub claimant_person_uid: String,
    pub record_uid: Option<String>,
    pub concept_uid: Option<String>,
    pub unit_uid: Option<String>,
    pub delta: f64,
    pub window_start: Option<String>,
    pub window_end: Option<String>,
    pub location: Option<TransferLocationSnapshot>,
    pub condition: Option<String>,
    pub reserve_from: String,
    pub authorization_intent_uid: Option<String>,
}

/// The new revision, signed fact, and linked Promises of a claim.
#[derive(Debug, Clone)]
pub struct OpenPromiseClaimOutcome {
    pub transfer_uid: String,
    pub revision: u64,
    pub fact: Fact,
    pub party_uid: String,
    pub proposer_promise_uid: String,
    pub promise_uid: String,
    pub source_promise_uid: String,
    pub consumed: bool,
}

/// The stored link between the OPEN source and the negotiated pair.
#[derive(Debug, Clone)]
pub struct OpenPromiseClaimPairRow {
    pub uid: String,
    pub transfer_uid: String,
    pub source_promise_uid: String,
    pub source_record_uid: Option<String>,
    pub proposer_promise_uid: String,
    pub claimant_promise_uid: String,
    pub proposer_person_uid: String,
    pub claimant_person_uid: String,
    pub revision: u64,
    pub reuse_policy: OpenPromiseReusePolicy,
    pub idempotency_key: String,
    pub created_at: String,
}

/// A new claim, the original result of a replay, or a stale revision.
#[derive(Debug, Clone)]
pub enum OpenPromiseClaimCommit {
    Committed(OpenPromiseClaimOutcome),
    Replayed(OpenPromiseClaimOutcome),
    Stale {
        transfer_uid: String,
        current_revision: u64,
    },
}

fn map_open_claim_pair(row: SqliteRow) -> Result<OpenPromiseClaimPairRow, StoreError> {
    let reuse_policy: &str = row.get("reuse_policy");
    Ok(OpenPromiseClaimPairRow {
        uid: row.get("uid"),
        transfer_uid: row.get("transfer_uid"),
        source_promise_uid: row.get("source_promise_uid"),
        source_record_uid: row.get("source_record_uid"),
        proposer_promise_uid: row.get("proposer_promise_uid"),
        claimant_promise_uid: row.get("claimant_promise_uid"),
        proposer_person_uid: row.get("proposer_person_uid"),
        claimant_person_uid: row.get("claimant_person_uid"),
        revision: row.get::<i64, _>("revision").cast_unsigned(),
        reuse_policy: OpenPromiseReusePolicy::parse(reuse_policy).ok_or_else(|| {
            sqlx::Error::Protocol(format!("unknown OPEN reuse policy `{reuse_policy}`"))
        })?,
        idempotency_key: row.get("idempotency_key"),
        created_at: row.get("created_at"),
    })
}

/// Lists the claim pairs for a source in insertion order.
///
/// # Errors
///
/// Returns an error if storage fails or a pair has an unknown reuse policy.
pub async fn open_claim_pairs_for_source(
    pool: &SqlitePool,
    source_promise_uid: &str,
) -> Result<Vec<OpenPromiseClaimPairRow>, StoreError> {
    sqlx::query(
        "SELECT * FROM transfer_open_claim_pair
         WHERE source_promise_uid = ? ORDER BY rowid",
    )
    .bind(source_promise_uid)
    .try_map(map_open_claim_pair)
    .fetch_all(pool)
    .await
}

/// Looks up the pair written for a claim request.
///
/// # Errors
///
/// Returns an error if storage fails or the pair has an unknown reuse policy.
pub async fn open_claim_pair_for_request(
    pool: &SqlitePool,
    request_id: &str,
) -> Result<Option<OpenPromiseClaimPairRow>, StoreError> {
    sqlx::query("SELECT * FROM transfer_open_claim_pair WHERE idempotency_key = ?")
        .bind(request_id.trim())
        .try_map(map_open_claim_pair)
        .fetch_optional(pool)
        .await
}

/// Returns the Transfer, source Promise, claimant Person, and claimant Promise UIDs.
///
/// # Errors
///
/// Returns an error if storage fails or the pair has an unknown reuse policy.
pub async fn open_claim_target_for_request(
    pool: &SqlitePool,
    request_id: &str,
) -> Result<Option<(String, String, String, String)>, StoreError> {
    Ok(open_claim_pair_for_request(pool, request_id)
        .await?
        .map(|pair| {
            (
                pair.transfer_uid,
                pair.source_promise_uid,
                pair.claimant_person_uid,
                pair.claimant_promise_uid,
            )
        }))
}

async fn open_claim_for_request(
    pool: &SqlitePool,
    input: &OpenPromiseClaimInput,
    request_id: &str,
) -> Result<Option<OpenPromiseClaimOutcome>, StoreError> {
    let Some(pair) = open_claim_pair_for_request(pool, request_id).await? else {
        return Ok(None);
    };
    if pair.transfer_uid != input.transfer_uid
        || pair.source_promise_uid != input.source_promise_uid
        || pair.claimant_person_uid != input.claimant_person_uid
        || input.expected_revision.checked_add(1) != Some(pair.revision)
    {
        return Err(sqlx::Error::Protocol(
            "OPEN claim request id was already used with different targets".into(),
        ));
    }
    let fact = revision_fact(pool, &pair.transfer_uid, pair.revision)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    let payload = fact
        .payload
        .as_deref()
        .ok_or_else(|| sqlx::Error::Protocol("OPEN claim revision has no signed payload".into()))?;
    let evidence: TransferRevisionEvidence =
        serde_json::from_str(payload).map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    if evidence.action != "claim-open-transfer-promise" {
        return Err(sqlx::Error::Protocol(
            "OPEN claim request id belongs to another action".into(),
        ));
    }
    let claimant = evidence
        .terms
        .promises
        .iter()
        .find(|promise| promise.uid == pair.claimant_promise_uid)
        .ok_or_else(|| {
            sqlx::Error::Protocol("signed OPEN claim lacks its claimant promise".into())
        })?;
    let proposer = evidence
        .terms
        .promises
        .iter()
        .find(|promise| promise.uid == pair.proposer_promise_uid)
        .ok_or_else(|| {
            sqlx::Error::Protocol("signed OPEN claim lacks its proposer promise".into())
        })?;
    let matches_terms = |promise: &TransferRevisionPromise| {
        promise.concept_uid == input.concept_uid
            && promise.unit_uid == input.unit_uid
            && promise.window_start == input.window_start
            && promise.window_end == input.window_end
            && promise.location == input.location
            && promise.condition == input.condition
            && promise.reserve_from == input.reserve_from
    };
    if claimant.source_promise_uid.as_deref() != Some(pair.source_promise_uid.as_str())
        || claimant.record_uid != input.record_uid
        || claimant.person_uid.as_deref() != Some(input.claimant_person_uid.as_str())
        || claimant.delta != input.delta
        || proposer.record_uid != pair.source_record_uid
        || proposer.person_uid.as_deref() != Some(pair.proposer_person_uid.as_str())
        || proposer.delta != -input.delta
        || ![claimant, proposer].into_iter().all(matches_terms)
    {
        return Err(sqlx::Error::Protocol(
            "OPEN claim request id was replayed with different refined terms".into(),
        ));
    }
    let party_uid: String = sqlx::query_scalar(
        "SELECT uid FROM transfer_party WHERE transfer_uid = ? AND actor_uid = ?",
    )
    .bind(&input.transfer_uid)
    .bind(&input.claimant_person_uid)
    .fetch_optional(pool)
    .await?
    .ok_or(sqlx::Error::RowNotFound)?;
    Ok(Some(OpenPromiseClaimOutcome {
        transfer_uid: pair.transfer_uid,
        revision: pair.revision,
        fact,
        party_uid,
        proposer_promise_uid: pair.proposer_promise_uid,
        promise_uid: pair.claimant_promise_uid,
        source_promise_uid: pair.source_promise_uid,
        consumed: pair.reuse_policy == OpenPromiseReusePolicy::Consume,
    }))
}

/// Claims an OPEN Promise and records the linked pair in a signed revision.
///
/// # Errors
///
/// Returns an error if the request conflicts with a previous claim, the source
/// or claimant is ineligible, the terms or authorization are invalid, or storage fails.
pub async fn claim_open_promise<F>(
    pool: &SqlitePool,
    input: OpenPromiseClaimInput,
    now: DateTime<Utc>,
    sign: F,
) -> Result<OpenPromiseClaimCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    let request_id = validate_request_key(&input.idempotency_key)?;
    if let Some(outcome) = open_claim_for_request(pool, &input, request_id).await? {
        return Ok(OpenPromiseClaimCommit::Replayed(outcome));
    }
    ensure_request_not_used_by_invitation_event(pool, request_id).await?;
    if !input.delta.is_finite() || input.delta == 0.0 {
        return Err(sqlx::Error::Protocol(
            "claimed promise delta must be finite and non-zero".into(),
        ));
    }
    validate_location(input.location.as_ref())?;
    let mut tx = crate::write_tx(pool).await?;
    let is_person: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM record WHERE uid = ? AND kind = 'person')")
            .bind(&input.claimant_person_uid)
            .fetch_one(&mut *tx)
            .await?;
    if !is_person {
        return Err(sqlx::Error::Protocol(
            "OPEN claimant identity is not a Person".into(),
        ));
    }
    let pending_invitation: Option<String> = sqlx::query_scalar(
        "SELECT uid FROM transfer_invitation
         WHERE transfer_uid = ? AND addressed_person_uid = ? AND status = 'pending' LIMIT 1",
    )
    .bind(&input.transfer_uid)
    .bind(&input.claimant_person_uid)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(invitation_uid) = pending_invitation {
        return Err(sqlx::Error::Protocol(format!(
            "invitation `{invitation_uid}` must be accepted before claiming an OPEN promise"
        )));
    }
    let source = sqlx::query(
        "SELECT record_uid, party_uid, delta, open_reuse_policy, item_json FROM promise
         WHERE uid = ? AND transfer_uid = ? AND state = 'open'
           AND party_uid IS NOT NULL",
    )
    .bind(&input.source_promise_uid)
    .bind(&input.transfer_uid)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| sqlx::Error::Protocol("source promise is not OPEN".into()))?;
    let reuse_policy = OpenPromiseReusePolicy::parse(source.get("open_reuse_policy"))
        .ok_or_else(|| sqlx::Error::Protocol("OPEN promise has invalid reuse policy".into()))?;
    let proposer_person_uid: String = source.get("party_uid");
    let proposer_record_uid: Option<String> = source.get("record_uid");
    let source_delta: f64 = source.get("delta");
    let mut pair_item = decode_item(source.get("item_json"))?;
    if pair_item.is_none() && (proposer_record_uid.is_none() || input.record_uid.is_none()) {
        return Err(sqlx::Error::Protocol(
            "an unbound OPEN claim requires a public item".into(),
        ));
    }
    if let Some(item) = &mut pair_item {
        let (giver, receiver) = if source_delta < 0.0 {
            (&proposer_person_uid, &input.claimant_person_uid)
        } else {
            (&input.claimant_person_uid, &proposer_person_uid)
        };
        item.exchange = Some(ExchangeRoute {
            uid: new_uid("exchange"),
            giver: giver.clone(),
            receiver: receiver.clone(),
        });
    }
    if !source_delta.is_finite() || source_delta == 0.0 {
        return Err(sqlx::Error::Protocol(
            "OPEN source direction must be finite and non-zero".into(),
        ));
    }
    if proposer_person_uid == input.claimant_person_uid {
        return Err(sqlx::Error::Protocol(
            "OPEN proposer cannot claim their own proposal".into(),
        ));
    }
    if source_delta.signum() == input.delta.signum() {
        return Err(sqlx::Error::Protocol(
            "OPEN claimant delta must oppose the proposal direction".into(),
        ));
    }
    let proposer_is_party: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM transfer_party WHERE transfer_uid = ? AND actor_uid = ?)",
    )
    .bind(&input.transfer_uid)
    .bind(&proposer_person_uid)
    .fetch_one(&mut *tx)
    .await?;
    if !proposer_is_party {
        return Err(sqlx::Error::Protocol(
            "OPEN proposer is not a current Transfer party".into(),
        ));
    }
    let existing_party: Option<String> = sqlx::query_scalar(
        "SELECT uid FROM transfer_party WHERE transfer_uid = ? AND actor_uid = ?",
    )
    .bind(&input.transfer_uid)
    .bind(&input.claimant_person_uid)
    .fetch_optional(&mut *tx)
    .await?;
    if existing_party.is_none() {
        let visibility: String =
            sqlx::query_scalar("SELECT visibility FROM transfer WHERE record_uid = ?")
                .bind(&input.transfer_uid)
                .fetch_one(&mut *tx)
                .await?;
        if visibility != "public" {
            return Err(sqlx::Error::Protocol(
                "unknown Person may claim an OPEN promise only on a public Transfer".into(),
            ));
        }
    }
    let revision =
        match advance_transfer_revision(&mut tx, &input.transfer_uid, input.expected_revision)
            .await?
        {
            Ok(revision) => revision,
            Err(current_revision) => {
                tx.rollback().await?;
                return Ok(OpenPromiseClaimCommit::Stale {
                    transfer_uid: input.transfer_uid,
                    current_revision,
                });
            }
        };
    let at = now.to_rfc3339();
    let party_uid = if let Some(uid) = existing_party {
        uid
    } else {
        let uid = new_uid("y");
        sqlx::query(
            "INSERT INTO transfer_party (uid, transfer_uid, actor_uid, kind)
             VALUES (?, ?, ?, 'participant')",
        )
        .bind(&uid)
        .bind(&input.transfer_uid)
        .bind(&input.claimant_person_uid)
        .execute(&mut *tx)
        .await?;
        uid
    };
    let location = input.location.as_ref();
    let pair_json = encode_item(pair_item.as_ref())?;
    let proposer_promise_uid = match reuse_policy {
        OpenPromiseReusePolicy::Consume => {
            let updated = sqlx::query(
                "UPDATE promise SET record_uid = ?, concept_uid = ?, unit_uid = ?,
                    party_uid = ?, delta = ?, window_start = ?, window_end = ?,
                    location_lat = ?, location_lon = ?, location_address = ?,
                    condition = ?, reserve_from = ?, state = 'proposed', revision = ?,
                    updated_at = ?
                 WHERE uid = ? AND transfer_uid = ? AND state = 'open'
                   AND party_uid = ?",
            )
            .bind(&proposer_record_uid)
            .bind(&input.concept_uid)
            .bind(&input.unit_uid)
            .bind(&proposer_person_uid)
            .bind(-input.delta)
            .bind(&input.window_start)
            .bind(&input.window_end)
            .bind(location.and_then(|value| value.lat))
            .bind(location.and_then(|value| value.lon))
            .bind(location.and_then(|value| value.address.as_deref()))
            .bind(&input.condition)
            .bind(&input.reserve_from)
            .bind(revision)
            .bind(&at)
            .bind(&input.source_promise_uid)
            .bind(&input.transfer_uid)
            .bind(&proposer_person_uid)
            .execute(&mut *tx)
            .await?;
            if updated.rows_affected() != 1 {
                return Err(sqlx::Error::Protocol(
                    "source promise is no longer OPEN".into(),
                ));
            }
            input.source_promise_uid.clone()
        }
        OpenPromiseReusePolicy::Duplicate => {
            let uid = new_uid("p");
            insert_pair_member(
                &mut tx,
                &input,
                PairMember {
                    uid: &uid,
                    record_uid: proposer_record_uid.as_deref(),
                    person_uid: &proposer_person_uid,
                    delta: -input.delta,
                },
                revision,
                reuse_policy,
                &at,
                pair_json.as_deref(),
            )
            .await?;
            uid
        }
    };
    let claimant_promise_uid = new_uid("p");
    insert_pair_member(
        &mut tx,
        &input,
        PairMember {
            uid: &claimant_promise_uid,
            record_uid: input.record_uid.as_deref(),
            person_uid: &input.claimant_person_uid,
            delta: input.delta,
        },
        revision,
        reuse_policy,
        &at,
        pair_json.as_deref(),
    )
    .await?;
    sqlx::query("UPDATE promise SET item_json = ? WHERE uid IN (?, ?)")
        .bind(pair_json.as_deref())
        .bind(&proposer_promise_uid)
        .bind(&claimant_promise_uid)
        .execute(&mut *tx)
        .await?;
    reset_agreements_for_revision(&mut tx, &input.transfer_uid, revision, now).await?;
    sqlx::query(
        "INSERT INTO transfer_agreement (uid, transfer_uid, party_uid, level, at, revision)
         VALUES (?, ?, ?, 0, ?, ?)
         ON CONFLICT(transfer_uid, party_uid) DO UPDATE SET
             level = 0, at = excluded.at, revision = excluded.revision",
    )
    .bind(new_uid("g"))
    .bind(&input.transfer_uid)
    .bind(&party_uid)
    .bind(&at)
    .bind(revision)
    .execute(&mut *tx)
    .await?;
    let fact = insert_revision_fact(
        &mut tx,
        &input.transfer_uid,
        RevisionChange {
            revision,
            previous_revision: input.expected_revision,
            request_id,
            action: "claim-open-transfer-promise",
        },
        Some(input.claimant_person_uid.clone()),
        now,
        &sign,
    )
    .await?;
    if fact.signature.is_none() && input.authorization_intent_uid.is_none() {
        return Err(sqlx::Error::Protocol(
            "OPEN claim requires a signing key or verified Action intent".into(),
        ));
    }
    if let Some(intent_uid) = input.authorization_intent_uid.as_deref() {
        link_pending_fact(&mut tx, intent_uid, &fact).await?;
    }
    let pair_uid = new_uid("tcp");
    sqlx::query(
        "INSERT INTO transfer_open_claim_pair
            (uid, transfer_uid, source_promise_uid, source_record_uid, proposer_promise_uid,
             claimant_promise_uid, proposer_person_uid, claimant_person_uid,
             revision, reuse_policy, idempotency_key, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&pair_uid)
    .bind(&input.transfer_uid)
    .bind(&input.source_promise_uid)
    .bind(&proposer_record_uid)
    .bind(&proposer_promise_uid)
    .bind(&claimant_promise_uid)
    .bind(&proposer_person_uid)
    .bind(&input.claimant_person_uid)
    .bind(revision)
    .bind(reuse_policy.as_str())
    .bind(request_id)
    .bind(&at)
    .execute(&mut *tx)
    .await?;
    bump_quantity(&mut tx, &input.transfer_uid, zero(), &at).await?;
    tx.commit().await?;
    Ok(OpenPromiseClaimCommit::Committed(OpenPromiseClaimOutcome {
        transfer_uid: input.transfer_uid,
        revision: revision.cast_unsigned(),
        fact,
        party_uid,
        proposer_promise_uid,
        promise_uid: claimant_promise_uid,
        source_promise_uid: input.source_promise_uid,
        consumed: reuse_policy == OpenPromiseReusePolicy::Consume,
    }))
}

struct PairMember<'a> {
    uid: &'a str,
    record_uid: Option<&'a str>,
    person_uid: &'a str,
    delta: f64,
}

async fn insert_pair_member(
    tx: &mut Transaction<'_, Sqlite>,
    input: &OpenPromiseClaimInput,
    member: PairMember<'_>,
    revision: i64,
    reuse_policy: OpenPromiseReusePolicy,
    at: &str,
    item: Option<&str>,
) -> Result<(), StoreError> {
    let location = input.location.as_ref();
    sqlx::query(
        "INSERT INTO promise
            (uid, source_promise_uid, record_uid, concept_uid, unit_uid,
             party_uid, delta, window_start, window_end, location_lat,
             location_lon, location_address, condition, transfer_uid,
             reserve_from, open_reuse_policy, state, revision, created_at, updated_at, item_json)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?,
                 'proposed', ?, ?, ?, ?)",
    )
    .bind(member.uid)
    .bind(&input.source_promise_uid)
    .bind(member.record_uid)
    .bind(&input.concept_uid)
    .bind(&input.unit_uid)
    .bind(member.person_uid)
    .bind(member.delta)
    .bind(&input.window_start)
    .bind(&input.window_end)
    .bind(location.and_then(|value| value.lat))
    .bind(location.and_then(|value| value.lon))
    .bind(location.and_then(|value| value.address.as_deref()))
    .bind(&input.condition)
    .bind(&input.transfer_uid)
    .bind(&input.reserve_from)
    .bind(reuse_policy.as_str())
    .bind(revision)
    .bind(at)
    .bind(at)
    .bind(item)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
