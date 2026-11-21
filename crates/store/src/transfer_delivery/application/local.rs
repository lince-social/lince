//! Applies a reviewed handoff to the participant's local Record.

use chrono::{DateTime, Utc};
use nucleus::{Cause, Fact, NewFact, fact::seal, new_uid};
use serde_json::json;
use sqlx::{
    Row, SqlitePool,
    sqlite::{SqliteExecutor, SqliteRow},
};

use crate::{
    StoreError,
    exact::sum_exact,
    facts,
    records::bump_quantity,
    transfer_accounting::{self, Binding},
    transfer_delivery::validation::protocol,
    transfer_effects,
};

mod handoff;

/// The immutable application, its exact reviewed formula, and its evidence UID.
#[derive(Debug, Clone)]
pub struct LocalTransferApplicationRow {
    pub uid: String,
    pub handoff_uid: String,
    pub participant_person_uid: String,
    pub local_record_uid: String,
    pub application_fact_uid: String,
    pub local_delta: f64,
    pub local_cumulative_before: f64,
    pub local_cumulative_after: f64,
    pub application_formula: String,
    pub application_formula_hash: String,
    pub application_formula_version: u64,
    pub authorization_intent_uid: Option<String>,
    pub request_id: String,
    pub created_at: String,
}

fn map_local_application(row: SqliteRow) -> LocalTransferApplicationRow {
    LocalTransferApplicationRow {
        uid: row.get("uid"),
        handoff_uid: row.get("handoff_uid"),
        participant_person_uid: row.get("participant_person_uid"),
        local_record_uid: row.get("local_record_uid"),
        application_fact_uid: row.get("application_fact_uid"),
        local_delta: row.get("local_delta"),
        local_cumulative_before: row.get("local_cumulative_before"),
        local_cumulative_after: row.get("local_cumulative_after"),
        application_formula: row.get("application_formula"),
        application_formula_hash: row.get("application_formula_hash"),
        application_formula_version: row
            .get::<i64, _>("application_formula_version")
            .cast_unsigned(),
        authorization_intent_uid: row.get("authorization_intent_uid"),
        request_id: row.get("request_id"),
        created_at: row.get("created_at"),
    }
}

/// Looks up a committed local application by request ID.
///
/// # Errors
///
/// Returns an error if the lookup fails.
pub async fn local_application_for_request(
    pool: &SqlitePool,
    request_id: &str,
) -> Result<Option<LocalTransferApplicationRow>, StoreError> {
    application_for_request(pool, request_id).await
}

async fn application_for_request<'e>(
    executor: impl SqliteExecutor<'e>,
    request_id: &str,
) -> Result<Option<LocalTransferApplicationRow>, StoreError> {
    sqlx::query("SELECT * FROM transfer_local_application WHERE request_id = ?")
        .bind(request_id)
        .map(map_local_application)
        .fetch_optional(executor)
        .await
}

/// The participant's reviewed private Record changes for a handoff.
pub struct NewLocalTransferApplication {
    pub expected_effects_hash: Option<String>,
    pub exchange_uid: String,
    pub handoff_uid: String,
    pub participant_person_uid: String,
    pub local_record_uid: String,
    pub local_delta: f64,
    pub local_cumulative_before: f64,
    pub local_cumulative_after: f64,
    pub application_formula: String,
    pub application_formula_hash: String,
    pub application_formula_version: u64,
    pub authorization_intent_uid: Option<String>,
    pub request_id: String,
}

/// The saved application, signed Fact, and replay status.
pub struct LocalTransferApplicationCommit {
    pub application: LocalTransferApplicationRow,
    pub fact: Fact,
    pub replayed: bool,
}

/// Applies the reviewed quantities once, or returns the saved Fact on replay.
///
/// # Errors
///
/// Returns an error for conflicting replay, changed review, missing authority
/// or signing evidence, or a failed write.
pub async fn apply_transfer_locally<F>(
    pool: &SqlitePool,
    input: NewLocalTransferApplication,
    now: DateTime<Utc>,
    sign: F,
) -> Result<LocalTransferApplicationCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    if !input.local_delta.is_finite()
        || !input.local_cumulative_before.is_finite()
        || !input.local_cumulative_after.is_finite()
    {
        return Err(protocol(
            "local Transfer application quantities are invalid",
        ));
    }
    let mut tx = crate::write_tx(pool).await?;
    if let Some(application) = application_for_request(&mut *tx, &input.request_id).await? {
        let exact = application.handoff_uid == input.handoff_uid
            && application.participant_person_uid == input.participant_person_uid
            && application.local_record_uid == input.local_record_uid
            && application.local_delta == input.local_delta
            && application.local_cumulative_before == input.local_cumulative_before
            && application.local_cumulative_after == input.local_cumulative_after
            && application.application_formula == input.application_formula
            && application.application_formula_hash == input.application_formula_hash
            && application.application_formula_version == input.application_formula_version
            && transfer_effects::reviewed_hash_on(&mut *tx, &application.application_fact_uid)
                .await?
                == input.expected_effects_hash;
        if !exact {
            return Err(protocol(
                "local Transfer application request was replayed with changed contents",
            ));
        }
        let fact = facts::get_on(&mut tx, &application.application_fact_uid)
            .await?
            .ok_or(sqlx::Error::RowNotFound)?;
        tx.rollback().await?;
        return Ok(LocalTransferApplicationCommit {
            application,
            fact,
            replayed: true,
        });
    }

    let handoff = sqlx::query("SELECT * FROM transfer_application_effect_handoff WHERE uid = ?")
        .bind(&input.handoff_uid)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    if handoff.get::<&str, _>("state") != "pending"
        || handoff.get::<&str, _>("participant_person_uid") != input.participant_person_uid
    {
        return Err(protocol(
            "remote Transfer application handoff is not pending for this Person",
        ));
    }
    crate::transfer_replication::require_private_writer(
        &mut tx,
        handoff.get::<&str, _>("transfer_uid"),
        &input.participant_person_uid,
    )
    .await?;
    let reference_uid: &str = handoff.get("reference_uid");
    let local = reference_uid.is_empty();
    let active: bool = local || sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM transfer_remote_reference WHERE uid = ? AND state = 'active')",
    ).bind(reference_uid).fetch_one(&mut *tx).await?;
    if !active {
        return Err(protocol("Transfer delivery access is no longer active"));
    }
    let record = sqlx::query(
        "SELECT record.organ_uid, record.deleted_at, local_organ.uid AS local_organ_uid
         FROM record LEFT JOIN record local_organ
           ON local_organ.slug = 'local-organ' AND local_organ.kind = 'organ' AND local_organ.deleted_at IS NULL
         WHERE record.uid = ?",
    )
    .bind(&input.local_record_uid)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(sqlx::Error::RowNotFound)?;
    let record_organ_uid = record.get::<Option<&str>, _>("organ_uid");
    let local_organ_uid = record.get::<Option<&str>, _>("local_organ_uid");
    if record.get::<Option<&str>, _>("deleted_at").is_some()
        || record_organ_uid.is_none()
        || record_organ_uid != local_organ_uid
        || local_organ_uid != Some(handoff.get("participant_organ_uid"))
    {
        return Err(protocol(
            "remote Transfer application may mutate only a live Record originating in this Cell",
        ));
    }
    let occurrence: &str = handoff.get("occurrence_uid");
    let binding = Binding {
        transfer: handoff.get("transfer_uid"),
        exchange: &input.exchange_uid,
        occurrence: Some(occurrence),
        person: &input.participant_person_uid,
        record: Some(&input.local_record_uid),
        unit: handoff.get("canonical_unit_uid"),
        outgoing: handoff.get::<i64, _>("application_direction") < 0,
    };
    let application = transfer_accounting::effective_on(&mut tx, &binding).await?;
    if application.formula != input.application_formula
        || application.formula_hash != input.application_formula_hash
        || application.version != input.application_formula_version
    {
        return Err(protocol(
            "private application policy or units changed after review",
        ));
    }
    let applied =
        transfer_accounting::applied_on(&mut tx, occurrence, &input.participant_person_uid).await?;
    let amount = transfer_accounting::amount(handoff.get("canonical_quantity"))?;
    let canonical_after = sum_exact([applied.canonical, amount])?;
    let group =
        transfer_effects::quote_effective_on(&mut tx, &binding, canonical_after, &application)
            .await?;
    transfer_effects::require_review(group.as_ref(), input.expected_effects_hash.as_deref())?;
    let (exact_delta, exact_after) = if let Some(group) = &group {
        let primary = group
            .effects
            .first()
            .ok_or_else(|| protocol("reviewed effect group is empty"))?;
        (primary.delta, primary.cumulative_after)
    } else {
        transfer_accounting::calculate(&application.formula, canonical_after, applied.local)?
    };
    if applied.local.to_f64() != input.local_cumulative_before
        || applied.canonical.to_f64() != handoff.get::<f64, _>("canonical_cumulative_before")
        || exact_delta.to_f64() != input.local_delta
        || exact_after.to_f64() != input.local_cumulative_after
    {
        return Err(protocol(
            "Transfer application changed after review or an earlier slice is not applied",
        ));
    }
    let application_uid = new_uid("tla");
    let payload = json!({
        "action": "apply-remote-transfer-settlement",
        "application_uid": application_uid,
        "handoff_uid": input.handoff_uid,
        "origin_organ_uid": handoff.get::<&str, _>("origin_organ_uid"),
        "transfer_uid": binding.transfer,
        "occurrence_uid": occurrence,
        "settlement_slice_uid": handoff.get::<&str, _>("settlement_slice_uid"),
        "canonical_slice_hash": handoff.get::<&str, _>("canonical_slice_hash"),
        "participant_person_uid": input.participant_person_uid,
        "application_formula_hash": input.application_formula_hash,
        "application_formula_version": input.application_formula_version,
        "local_unit_uid": application.unit,
        "private_effects_hash": input.expected_effects_hash,
        "local_cumulative_before": input.local_cumulative_before,
        "local_cumulative_after": input.local_cumulative_after,
    });
    let previous_hash = facts::last_hash(&mut tx).await?;
    let mut fact = seal(
        NewFact {
            uid: None,
            record_uid: input.local_record_uid.clone(),
            delta: exact_delta,
            at: None,
            actor_uid: Some(input.participant_person_uid.clone()),
            cause: Cause::settlement(handoff.get::<String, _>("settlement_slice_uid")),
            payload: Some(payload.to_string()),
        },
        &previous_hash,
        now,
    );
    fact.signature = sign(&fact.hash);
    if fact.signature.is_none() && input.authorization_intent_uid.is_none() {
        return Err(protocol(
            "local Transfer application requires a Person signer or verified Action intent",
        ));
    }
    facts::insert(&mut tx, &fact).await?;
    if let Some(intent_uid) = input.authorization_intent_uid.as_deref() {
        crate::action_intents::link_pending_fact(&mut tx, intent_uid, &fact).await?;
    }
    let application = sqlx::query(
        "INSERT INTO transfer_local_application
         (uid, handoff_uid, participant_person_uid, local_record_uid,
          application_fact_uid, local_delta, local_cumulative_before,
          local_cumulative_after, application_formula, application_formula_hash,
          application_formula_version, authorization_intent_uid, request_id, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING *",
    )
    .bind(&application_uid)
    .bind(&input.handoff_uid)
    .bind(&input.participant_person_uid)
    .bind(&input.local_record_uid)
    .bind(&fact.uid)
    .bind(input.local_delta)
    .bind(input.local_cumulative_before)
    .bind(input.local_cumulative_after)
    .bind(&input.application_formula)
    .bind(&input.application_formula_hash)
    .bind(input.application_formula_version.cast_signed())
    .bind(&input.authorization_intent_uid)
    .bind(&input.request_id)
    .bind(now.to_rfc3339())
    .map(map_local_application)
    .fetch_one(&mut *tx)
    .await?;
    handoff::accept(&mut tx, &handoff, &application, local).await?;
    bump_quantity(
        &mut tx,
        &input.local_record_uid,
        exact_delta,
        &application.created_at,
    )
    .await?;
    transfer_effects::apply_on(
        &mut tx,
        group.as_ref(),
        &fact,
        occurrence,
        input.authorization_intent_uid.as_deref(),
        now,
        &sign,
    )
    .await?;
    tx.commit().await?;
    Ok(LocalTransferApplicationCommit {
        application,
        fact,
        replayed: false,
    })
}
