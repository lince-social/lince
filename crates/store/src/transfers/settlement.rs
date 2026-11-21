use std::fmt;

use chrono::{DateTime, Utc};
use nucleus::{
    Cause, Fact, NewFact, PromiseState,
    fact::seal,
    transfer::{
        TransferOccurrenceSettlementApplicationEvidence, TransferOccurrenceSettlementEvidence,
        TransferRemainderPolicy,
    },
};
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

use super::{ensure_phase4_request_unused, phase5_request_for_request, validate_request_key};
use crate::{
    StoreError, action_intents, config, exact, facts, organs, records,
    transfer_accounting as accounting, transfer_cancellations as cancellations,
    transfer_effects as effects, write_tx,
};

pub(super) mod compensation;
pub(super) mod reads;
pub(super) mod source_groups;

use reads::{OccurrenceSettlementOutcome, occurrence_settlement_for_request};
use source_groups::record_first_completes_result;

/// The reviewed public and private quantities for one settlement.
#[derive(Clone)]
pub struct OccurrenceSettlementInput {
    pub expected_effects_hash: Option<String>,
    pub occurrence_uid: String,
    pub idempotency_key: String,
    pub actor_person_uid: String,
    pub canonical_quantity: f64,
    pub local_record_uid: String,
    pub local_delta: f64,
    pub local_cumulative_after: f64,
    pub application_formula_hash: String,
    pub application_formula: String,
    pub application_formula_version: u64,
    pub remainder_policy: TransferRemainderPolicy,
    pub authorization_intent_uid: Option<String>,
}

impl fmt::Debug for OccurrenceSettlementInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OccurrenceSettlementInput")
            .field("occurrence_uid", &self.occurrence_uid)
            .field("idempotency_key", &self.idempotency_key)
            .field("canonical_quantity", &self.canonical_quantity)
            .finish_non_exhaustive()
    }
}

/// Whether the request wrote a settlement or replayed an existing one.
#[derive(Debug, Clone)]
pub enum OccurrenceSettlementCommit {
    Committed(OccurrenceSettlementOutcome),
    Replayed(OccurrenceSettlementOutcome),
}

struct Signing<'a, F> {
    now: DateTime<Utc>,
    intent: Option<&'a str>,
    sign: &'a F,
}

impl<F: Fn(&str) -> Option<String> + Send + Sync> Signing<'_, F> {
    async fn append(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        new: NewFact,
        previous: &str,
        missing_signature: &'static str,
    ) -> Result<Fact, StoreError> {
        let mut fact = seal(new, previous, self.now);
        fact.signature = (self.sign)(&fact.hash);
        if fact.signature.is_none() && self.intent.is_none() {
            return Err(StoreError::Protocol(missing_signature.into()));
        }
        facts::insert(tx, &fact).await?;
        if let Some(intent) = self.intent {
            action_intents::link_pending_fact(tx, intent, &fact).await?;
        }
        Ok(fact)
    }
}

/// Reads the owner's occurrence policy or the configured default.
///
/// # Errors
///
/// Returns an error if the policy is invalid or cannot be read.
pub async fn effective_occurrence_remainder_policy(
    pool: &SqlitePool,
    occurrence_uid: &str,
    owner_person_uid: &str,
) -> Result<TransferRemainderPolicy, StoreError> {
    let policy = match sqlx::query_scalar::<_, String>(
        "SELECT policy FROM transfer_occurrence_remainder_policy
         WHERE occurrence_uid = ? AND owner_person_uid = ?",
    )
    .bind(occurrence_uid)
    .bind(owner_person_uid)
    .fetch_optional(pool)
    .await?
    {
        Some(policy) => policy,
        None => config::transfer_remainder_policy(pool).await?,
    };
    TransferRemainderPolicy::parse(&policy).ok_or_else(|| {
        sqlx::Error::Protocol(format!("unknown Transfer remainder policy `{policy}`"))
    })
}

/// Sets the remainder policy for the source-promise owner.
///
/// # Errors
///
/// Returns an error if the caller is not the owner or storage fails.
pub async fn set_occurrence_remainder_policy(
    pool: &SqlitePool,
    occurrence_uid: &str,
    owner_person_uid: &str,
    policy: TransferRemainderPolicy,
    now: DateTime<Utc>,
) -> Result<(), StoreError> {
    let source_owner: Option<String> = sqlx::query_scalar(
        "SELECT source.party_uid
         FROM transfer_occurrence occurrence
         JOIN promise source ON source.uid = occurrence.promise_uid
         WHERE occurrence.uid = ?",
    )
    .bind(occurrence_uid)
    .fetch_optional(pool)
    .await?;
    if source_owner.as_deref() != Some(owner_person_uid) {
        return Err(sqlx::Error::Protocol(
            "only the source-promise owner may set the occurrence remainder policy".into(),
        ));
    }
    sqlx::query(
        "INSERT INTO transfer_occurrence_remainder_policy
            (occurrence_uid, owner_person_uid, policy, updated_at)
         VALUES (?, ?, ?, ?)
         ON CONFLICT(occurrence_uid) DO UPDATE SET
            owner_person_uid = excluded.owner_person_uid,
            policy = excluded.policy,
            updated_at = excluded.updated_at",
    )
    .bind(occurrence_uid)
    .bind(owner_person_uid)
    .bind(policy.as_str())
    .bind(now.to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

/// Records the reviewed settlement, its private application, and any source-group facts atomically.
///
/// # Errors
///
/// Returns an error for invalid, stale, or unauthorized input, insufficient
/// remaining quantity, missing authorization, or a storage failure.
pub async fn settle_occurrence<F>(
    pool: &SqlitePool,
    input: OccurrenceSettlementInput,
    now: DateTime<Utc>,
    sign: F,
) -> Result<OccurrenceSettlementCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    let request_id = validate_request_key(&input.idempotency_key)?;
    let formula = input.application_formula.trim();
    if formula.is_empty() || formula.chars().count() > 2_000 {
        return Err(sqlx::Error::Protocol(
            "application formula must contain 1 to 2000 characters".into(),
        ));
    }
    if !input.canonical_quantity.is_finite() || input.canonical_quantity <= 0.0 {
        return Err(sqlx::Error::Protocol(
            "settlement canonical quantity must be finite and positive".into(),
        ));
    }
    if !input.local_delta.is_finite() || !input.local_cumulative_after.is_finite() {
        return Err(sqlx::Error::Protocol(
            "settlement local quantities must be finite".into(),
        ));
    }
    let formula_version = i64::try_from(input.application_formula_version).map_err(|_| {
        sqlx::Error::Protocol("application formula version exceeds SQLite range".into())
    })?;
    let formula_hash = input.application_formula_hash.as_str();
    if let Some(outcome) = occurrence_settlement_for_request(pool, request_id).await? {
        let slice = &outcome.slice;
        if slice.occurrence_uid != input.occurrence_uid
            || slice.owner_person_uid != input.actor_person_uid
            || slice.canonical_quantity != input.canonical_quantity
            || slice.local_record_uid != input.local_record_uid
            || slice.local_delta != input.local_delta
            || slice.local_cumulative_after != input.local_cumulative_after
            || slice.application_formula_hash != formula_hash
            || slice.application_formula != formula
            || slice.application_formula_version != input.application_formula_version
            || slice.remainder_policy != input.remainder_policy
            || effects::reviewed_hash(pool, &slice.application_fact_uid).await?
                != input.expected_effects_hash
        {
            return Err(sqlx::Error::Protocol(
                "settlement request id was already used with different preview values".into(),
            ));
        }
        return Ok(OccurrenceSettlementCommit::Replayed(outcome));
    }
    if phase5_request_for_request(pool, request_id)
        .await?
        .is_some()
    {
        return Err(sqlx::Error::Protocol(
            "settlement request id exists without its immutable slice".into(),
        ));
    }
    ensure_phase4_request_unused(pool, request_id).await?;

    let local_organ_uid = organs::local(pool).await?.map(|organ| organ.uid);
    let mut tx = write_tx(pool).await?;
    let occurrence = sqlx::query(
        "SELECT occurrence.transfer_uid, occurrence.promise_uid,
                occurrence.quantity, occurrence.unit_uid,
                occurrence.delivery_claimed, occurrence.receipt_claimed,
                occurrence.disputed, source.party_uid, source.state, source.delta, path.public_exchange_uid AS exchange_uid,
                local_record.organ_uid, local_record.deleted_at
         FROM transfer_occurrence occurrence
         JOIN promise source ON source.uid = occurrence.promise_uid
         LEFT JOIN transfer_exchange_path path ON path.uid = occurrence.exchange_path_uid
         LEFT JOIN record local_record ON local_record.uid = ?
         WHERE occurrence.uid = ?",
    )
    .bind(&input.local_record_uid)
    .bind(&input.occurrence_uid)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(sqlx::Error::RowNotFound)?;
    let transfer_uid: &str = occurrence.get("transfer_uid");
    let promise_uid: &str = occurrence.get("promise_uid");
    let canonical_total: f64 = occurrence.get("quantity");
    let canonical_unit_uid: Option<&str> = occurrence.get("unit_uid");
    let source_owner: &str = occurrence.get("party_uid");
    let promise_state: &str = occurrence.get("state");
    let record_organ_uid: Option<&str> = occurrence.get("organ_uid");
    let record_deleted_at: Option<&str> = occurrence.get("deleted_at");

    if source_owner != input.actor_person_uid {
        return Err(sqlx::Error::Protocol(
            "only the source-promise owner may settle this occurrence".into(),
        ));
    }
    if promise_state != PromiseState::Active.as_str() {
        return Err(sqlx::Error::Protocol(
            "only an active source promise may be settled".into(),
        ));
    }
    if occurrence.get::<i64, _>("delivery_claimed") == 0
        || occurrence.get::<i64, _>("receipt_claimed") == 0
    {
        return Err(sqlx::Error::Protocol(
            "settlement requires current delivery and receipt confirmation".into(),
        ));
    }
    if occurrence.get::<i64, _>("disputed") != 0 {
        return Err(sqlx::Error::Protocol(
            "a disputed occurrence cannot be settled".into(),
        ));
    }
    if record_deleted_at.is_some()
        || record_organ_uid.is_none()
        || record_organ_uid != local_organ_uid.as_deref()
    {
        return Err(sqlx::Error::Protocol(
            "settlement may mutate only a live Record originating in this Cell".into(),
        ));
    }

    let exchange: Option<&str> = occurrence.get("exchange_uid");
    let binding = accounting::Binding {
        transfer: transfer_uid,
        exchange: exchange.unwrap_or(promise_uid),
        occurrence: Some(&input.occurrence_uid),
        person: &input.actor_person_uid,
        record: Some(&input.local_record_uid),
        unit: canonical_unit_uid,
        outgoing: occurrence.get::<f64, _>("delta") < 0.0,
    };
    let application = accounting::effective_on(&mut tx, &binding).await?;
    if application.formula != formula
        || application.formula_hash != formula_hash
        || application.version != input.application_formula_version
    {
        return Err(sqlx::Error::Protocol(
            "settlement private policy or units changed after review".into(),
        ));
    }
    accounting::require_same_record(
        &mut tx,
        &input.occurrence_uid,
        &input.actor_person_uid,
        &input.local_record_uid,
    )
    .await?;
    let applied =
        accounting::applied_on(&mut tx, &input.occurrence_uid, &input.actor_person_uid).await?;
    let amount = nucleus::transfer::application::amount(input.canonical_quantity)
        .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    let total = nucleus::transfer::application::amount(canonical_total)
        .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    let total = cancellations::effective_total_on(&mut tx, &input.occurrence_uid, total).await?;
    let exact_after = exact::sum_exact([applied.canonical, amount])?;
    let exact_remaining = exact::difference(total, exact_after)?;
    if exact_remaining.mantissa() < 0 {
        return Err(sqlx::Error::Protocol(
            "settlement slice exceeds the occurrence's remaining quantity".into(),
        ));
    }
    let group = effects::quote_on(&mut tx, &binding, exact_after).await?;
    effects::require_review(group.as_ref(), input.expected_effects_hash.as_deref())?;
    let (exact_delta, exact_local_after) = if let Some(group) = &group {
        let primary = group.effects.first().ok_or_else(|| {
            StoreError::Protocol("settlement effects have no primary Record".into())
        })?;
        (primary.delta, primary.cumulative_after)
    } else {
        accounting::calculate(formula, exact_after, applied.local)?
    };
    if exact_delta.to_f64() != input.local_delta
        || exact_local_after.to_f64() != input.local_cumulative_after
    {
        return Err(sqlx::Error::Protocol(
            "settlement local delta differs from the reviewed cumulative result".into(),
        ));
    }
    let cumulative_before = applied.canonical.to_f64();
    let cumulative_after = exact_after.to_f64();
    let local_cumulative_before = applied.local.to_f64();
    let remaining_after = exact_remaining.to_f64();

    let settlement_uid = nucleus::new_uid("tss");
    let public_evidence = TransferOccurrenceSettlementEvidence {
        action: "settle-transfer-occurrence".into(),
        idempotency_key: request_id.into(),
        settlement_uid: settlement_uid.clone(),
        transfer_uid: transfer_uid.into(),
        occurrence_uid: input.occurrence_uid.clone(),
        promise_uid: promise_uid.into(),
        owner_person_uid: input.actor_person_uid.clone(),
        canonical_quantity: input.canonical_quantity,
        canonical_unit_uid: canonical_unit_uid.map(str::to_owned),
        cumulative_before,
        cumulative_after,
        remaining_after,
        application_formula_hash: formula_hash.into(),
        application_formula_version: input.application_formula_version,
        remainder_policy: input.remainder_policy,
    };
    let public_payload = serde_json::to_string(&public_evidence)
        .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    let previous_hash = facts::last_hash(&mut tx).await?;
    let signing = Signing {
        now,
        intent: input.authorization_intent_uid.as_deref(),
        sign: &sign,
    };
    let evidence_fact = signing
        .append(
            &mut tx,
            NewFact {
                uid: None,
                record_uid: transfer_uid.into(),
                delta: exact::zero(),
                at: None,
                actor_uid: Some(input.actor_person_uid.clone()),
                cause: Cause::settlement(settlement_uid.clone()),
                payload: Some(public_payload),
            },
            &previous_hash,
            "occurrence settlement requires a signing key or verified Action intent",
        )
        .await?;

    let application_evidence = TransferOccurrenceSettlementApplicationEvidence {
        action: "apply-transfer-occurrence-settlement".into(),
        settlement_uid: settlement_uid.clone(),
        transfer_uid: transfer_uid.into(),
        occurrence_uid: input.occurrence_uid.clone(),
        promise_uid: promise_uid.into(),
        owner_person_uid: input.actor_person_uid.clone(),
        evidence_fact_uid: evidence_fact.uid.clone(),
        application_formula_hash: formula_hash.into(),
        application_formula_version: input.application_formula_version,
        local_cumulative_before,
        local_cumulative_after: input.local_cumulative_after,
    };
    let mut application_payload = serde_json::to_value(&application_evidence)
        .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    application_payload["local_unit_uid"] = serde_json::json!(application.unit);
    application_payload["private_effects_hash"] = serde_json::json!(input.expected_effects_hash);
    let application_payload = application_payload.to_string();
    let application_fact = signing
        .append(
            &mut tx,
            NewFact {
                uid: None,
                record_uid: input.local_record_uid.clone(),
                delta: exact_delta,
                at: None,
                actor_uid: Some(input.actor_person_uid.clone()),
                cause: Cause::settlement(settlement_uid.clone()),
                payload: Some(application_payload),
            },
            &evidence_fact.hash,
            "settlement application requires a signing key or verified Action intent",
        )
        .await?;

    let at = now.to_rfc3339();
    sqlx::query(
        "INSERT INTO transfer_phase5_request
            (idempotency_key, kind, target_uid, fact_uid, created_at)
         VALUES (?, 'settlement', ?, ?, ?)",
    )
    .bind(request_id)
    .bind(&input.occurrence_uid)
    .bind(&evidence_fact.uid)
    .bind(&at)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO transfer_occurrence_settlement_slice
            (uid, occurrence_uid, transfer_uid, promise_uid, owner_person_uid,
             canonical_quantity, canonical_unit_uid, cumulative_before,
             cumulative_after, remaining_after, evidence_fact_uid,
             application_fact_uid, local_record_uid, local_delta,
             local_cumulative_before, local_cumulative_after,
             application_formula, application_formula_hash,
             application_formula_version, remainder_policy, idempotency_key,
             created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&settlement_uid)
    .bind(&input.occurrence_uid)
    .bind(transfer_uid)
    .bind(promise_uid)
    .bind(&input.actor_person_uid)
    .bind(input.canonical_quantity)
    .bind(canonical_unit_uid)
    .bind(cumulative_before)
    .bind(cumulative_after)
    .bind(remaining_after)
    .bind(&evidence_fact.uid)
    .bind(&application_fact.uid)
    .bind(&input.local_record_uid)
    .bind(input.local_delta)
    .bind(local_cumulative_before)
    .bind(input.local_cumulative_after)
    .bind(formula)
    .bind(formula_hash)
    .bind(formula_version)
    .bind(input.remainder_policy.as_str())
    .bind(request_id)
    .bind(&at)
    .execute(&mut *tx)
    .await?;
    record_first_completes_result(
        &mut tx,
        transfer_uid,
        &settlement_uid,
        &input.actor_person_uid,
        &signing,
    )
    .await?;
    if remaining_after == 0.0 {
        sqlx::query("UPDATE promise SET state = 'kept', updated_at = ? WHERE uid = ?")
            .bind(&at)
            .bind(promise_uid)
            .execute(&mut *tx)
            .await?;
    }
    records::bump_quantity(&mut tx, &input.local_record_uid, exact_delta, &at).await?;
    records::bump_quantity(&mut tx, transfer_uid, exact::zero(), &at).await?;
    effects::apply_on(
        &mut tx,
        group.as_ref(),
        &application_fact,
        &input.occurrence_uid,
        input.authorization_intent_uid.as_deref(),
        now,
        &sign,
    )
    .await?;
    tx.commit().await?;

    let outcome = occurrence_settlement_for_request(pool, request_id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    Ok(OccurrenceSettlementCommit::Committed(outcome))
}
