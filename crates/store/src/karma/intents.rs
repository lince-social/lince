//! K5.2 — durable authorized intents.
//!
//! An intent is created inside the very transaction that accepts a candidate,
//! so a person can never end up with an accepted proposal whose authority was
//! never checked, nor with authority reserved for work nobody accepted.
//!
//! Budget consumption is a query over the intents themselves rather than a
//! counter: the rows that still hold a reservation *are* the ledger, so the
//! accounting cannot drift from the evidence, and cancelling an intent gives
//! its budget back automatically.

use chrono::{DateTime, Utc};
use nucleus::karma::{
    BudgetUsage, CanonicalHash, Capability, DecimalValue, DelegationGrantRevision,
    GrantAuthorityRequest, IntentAmount, IntentAuthorization, IntentStatus, IntentTransition,
    IntentTransitionSchema, KarmaCandidateProposal, KarmaIntent, KarmaIntentSchema, ReferenceKind,
    TimestampMs, TypedUid, authorize_intent, canonical_json_bytes, proposal_amount,
    proposal_target,
};
use nucleus::{Cause, CauseKind, Fact, NewFact};
use serde::{Deserialize, Serialize};
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

use crate::StoreError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KarmaIntentRow {
    pub intent_hash: CanonicalHash,
    pub candidate_hash: CanonicalHash,
    pub grant_uid: String,
    pub grant_revision_hash: CanonicalHash,
    pub intent: KarmaIntent,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KarmaIntentStateRow {
    pub intent_hash: CanonicalHash,
    pub state_revision: u64,
    pub status: IntentStatus,
    pub current_event_hash: CanonicalHash,
    pub cancelled_reason: Option<String>,
    pub actor_person_uid: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KarmaIntentEventRow {
    pub event_hash: CanonicalHash,
    pub transition: IntentTransition,
    pub created_at: String,
}

pub async fn get(
    pool: &SqlitePool,
    intent_hash: &CanonicalHash,
) -> Result<Option<KarmaIntentRow>, StoreError> {
    let row = sqlx::query("SELECT * FROM karma_intent WHERE intent_hash = ?")
        .bind(intent_hash.as_str())
        .fetch_optional(pool)
        .await?;
    row.map(map_intent).transpose()
}

pub async fn list(pool: &SqlitePool) -> Result<Vec<KarmaIntentRow>, StoreError> {
    sqlx::query("SELECT * FROM karma_intent ORDER BY created_at, intent_hash")
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(map_intent)
        .collect()
}

pub async fn get_state(
    pool: &SqlitePool,
    intent_hash: &CanonicalHash,
) -> Result<Option<KarmaIntentStateRow>, StoreError> {
    let row = sqlx::query("SELECT * FROM karma_intent_state WHERE intent_hash = ?")
        .bind(intent_hash.as_str())
        .fetch_optional(pool)
        .await?;
    row.map(map_state).transpose()
}

pub async fn list_states(pool: &SqlitePool) -> Result<Vec<KarmaIntentStateRow>, StoreError> {
    sqlx::query("SELECT * FROM karma_intent_state ORDER BY updated_at, intent_hash")
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(map_state)
        .collect()
}

/// One intent's whole lifecycle, oldest first.
pub async fn history(
    pool: &SqlitePool,
    intent_hash: &CanonicalHash,
) -> Result<Vec<KarmaIntentEventRow>, StoreError> {
    sqlx::query(
        "SELECT * FROM karma_intent_event WHERE intent_hash = ? ORDER BY state_revision",
    )
    .bind(intent_hash.as_str())
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map_event)
    .collect()
}

pub async fn get_for_candidate(
    pool: &SqlitePool,
    candidate_hash: &CanonicalHash,
) -> Result<Option<KarmaIntentRow>, StoreError> {
    let row = sqlx::query("SELECT * FROM karma_intent WHERE candidate_hash = ?")
        .bind(candidate_hash.as_str())
        .fetch_optional(pool)
        .await?;
    row.map(map_intent).transpose()
}

/// Authorize one accepted `act` candidate against one named grant and reserve
/// its budget, inside the caller's transaction.
///
/// The caller supplies no amount, target, or deadline: every one of those is
/// read from the stored proposal and the stored grant. A client that could name
/// the amount could understate it and spend a budget it was never given.
pub(crate) async fn authorize_accepted_candidate_tx(
    tx: &mut Transaction<'_, Sqlite>,
    candidate_hash: &CanonicalHash,
    proposal: &KarmaCandidateProposal,
    grant_uid: &str,
    request_id: &str,
    actor_person_uid: &str,
    now: DateTime<Utc>,
    at: &str,
) -> Result<KarmaIntent, StoreError> {
    if proposal.route != nucleus::karma::CandidateRoute::Act {
        return Err(protocol(
            "only an act candidate can be authorized into a Karma intent",
        ));
    }
    let principal = TypedUid::new(ReferenceKind::Person, actor_person_uid.to_string())
        .map_err(|error| protocol(format!("invalid Karma intent actor: {error}")))?;

    let handle = crate::karma::grants::get_handle_in_tx(tx, grant_uid)
        .await?
        .ok_or_else(|| protocol("authorizing Karma grant does not exist"))?;
    let revision_hash = handle
        .active_revision_hash
        .clone()
        .ok_or_else(|| protocol("authorizing Karma grant is not active"))?;
    let revision = crate::karma::grants::get_revision_in_tx(tx, grant_uid, &revision_hash)
        .await?
        .ok_or_else(|| protocol("authorizing Karma grant revision is missing"))?
        .revision;

    let capability = template_capability(proposal)?;
    let amount = proposal_amount(&proposal.fields).map_err(boundary)?;
    let target = proposal_target(&proposal.fields).map_err(boundary)?;
    let logical_at = TimestampMs::from_millis(now.timestamp_millis()).map_err(boundary)?;
    let request = GrantAuthorityRequest {
        principal_person_uid: principal,
        program_uid: TypedUid::new(ReferenceKind::Program, proposal.program_uid.clone())
            .map_err(|error| protocol(format!("invalid Karma intent Program: {error}")))?,
        program_revision_hash: proposal.program_revision_hash.clone(),
        candidate_template: proposal.template.clone(),
        capability,
        target,
        logical_at,
    };

    let usage = budget_usage_tx(tx, grant_uid, &revision, logical_at, amount.as_ref()).await?;
    let outcome =
        authorize_intent(&revision, &request, &usage, amount.as_ref()).map_err(boundary)?;
    if !outcome.allowed {
        // Fail closed and loudly: the whole accept is refused rather than
        // quietly becoming an accepted candidate with no authority behind it.
        return Err(protocol(format!(
            "Karma authority refused this intent: authority={} budget={}",
            denial_list(&outcome.decision.denials),
            denial_list(&outcome.budget_denials),
        )));
    }
    let snapshot = outcome
        .budget
        .ok_or_else(|| protocol("an allowed authorization must carry its reservation"))?;

    let intent = KarmaIntent {
        schema: KarmaIntentSchema::V1,
        candidate_hash: candidate_hash.clone(),
        program_uid: proposal.program_uid.clone(),
        program_revision_hash: proposal.program_revision_hash.clone(),
        template: proposal.template.clone(),
        capability,
        target: request.target.clone(),
        fields: proposal.fields.clone(),
        quantity: amount.clone(),
        // One accepted candidate is one intent, forever: a retry of the same
        // acceptance can never mint a second authorized request.
        idempotency_key: format!("karma-intent:{}", candidate_hash.as_str()),
        // Authority cannot outlive the consent that granted it.
        deadline: revision.spec.expires_at,
        authorization: IntentAuthorization {
            schema: KarmaIntentSchema::V1,
            grant_uid: grant_uid.to_string(),
            grant_handle_revision: handle.handle_revision,
            grant_revision_hash: revision_hash.clone(),
            request,
            decision: outcome.decision,
            budget: snapshot.clone(),
        },
    };
    let intent_hash = intent.intent_hash().map_err(boundary)?;

    sqlx::query(
        "INSERT INTO karma_intent
            (intent_hash, candidate_hash, grant_uid, grant_revision_hash, grant_handle_revision,
             program_uid, program_revision_hash, template, capability, idempotency_key,
             window_index, quantity_unit_uid, quantity_scale, quantity_mantissa, deadline,
             intent_json, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(intent_hash.as_str())
    .bind(candidate_hash.as_str())
    .bind(grant_uid)
    .bind(revision_hash.as_str())
    .bind(sql_i64(handle.handle_revision, "grant handle revision")?)
    .bind(&proposal.program_uid)
    .bind(proposal.program_revision_hash.as_str())
    .bind(proposal.template.as_str())
    .bind(capability_name(capability)?)
    .bind(&intent.idempotency_key)
    .bind(snapshot.window_index.map(|index| index as i64))
    .bind(amount.as_ref().map(|value| value.unit_uid.as_str()))
    .bind(amount.as_ref().map(|value| i64::from(value.amount.scale())))
    .bind(
        amount
            .as_ref()
            .map(|value| value.amount.mantissa().to_string()),
    )
    .bind(intent.deadline.to_string())
    .bind(canonical_string(&intent)?)
    .bind(at)
    .execute(&mut **tx)
    .await?;
    let event_hash = append_transition_tx(
        tx,
        IntentTransition {
            schema: IntentTransitionSchema::V1,
            intent_hash: intent_hash.clone(),
            state_revision: 1,
            previous_event_hash: None,
            cause_request_id: request_id.to_string(),
            actor_person_uid: actor_person_uid.to_string(),
            status: IntentStatus::Authorized,
            reason: None,
        },
        at,
    )
    .await?;
    sqlx::query(
        "INSERT INTO karma_intent_state
            (intent_hash, state_revision, status, current_event_hash, cancelled_reason,
             actor_person_uid, updated_at)
         VALUES (?, 1, 'authorized', ?, NULL, ?, ?)",
    )
    .bind(intent_hash.as_str())
    .bind(event_hash.as_str())
    .bind(actor_person_uid)
    .bind(at)
    .execute(&mut **tx)
    .await?;
    Ok(intent)
}

/// Write one transition into the immutable history and return its hash. Every
/// state change goes through here, so a projection can never point at an event
/// that was never recorded.
async fn append_transition_tx(
    tx: &mut Transaction<'_, Sqlite>,
    transition: IntentTransition,
    at: &str,
) -> Result<CanonicalHash, StoreError> {
    let event_hash = transition.event_hash().map_err(boundary)?;
    sqlx::query(
        "INSERT INTO karma_intent_event
            (event_hash, intent_hash, state_revision, previous_event_hash, status, reason,
             cause_request_id, actor_person_uid, event_json, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(event_hash.as_str())
    .bind(transition.intent_hash.as_str())
    .bind(sql_i64(transition.state_revision, "intent state revision")?)
    .bind(
        transition
            .previous_event_hash
            .as_ref()
            .map(CanonicalHash::as_str),
    )
    .bind(transition.status.as_str())
    .bind(transition.reason.as_deref())
    .bind(&transition.cause_request_id)
    .bind(&transition.actor_person_uid)
    .bind(canonical_string(&transition)?)
    .bind(at)
    .execute(&mut **tx)
    .await?;
    Ok(event_hash)
}

/// Revoking consent cancels the work it authorized. Cancellation releases the
/// reservation, so a revoked-and-replaced grant starts from its own budget.
pub(crate) async fn cancel_for_grant_tx(
    tx: &mut Transaction<'_, Sqlite>,
    grant_uid: &str,
    request_id: &str,
    actor_person_uid: &str,
    reason: &str,
    at: &str,
) -> Result<Vec<CanonicalHash>, StoreError> {
    // A lifecycle rule, not the accounting rule: `authorized` is the only state
    // K5.2 can cancel from. When E0.3 adds leased and dispatching intents this
    // selection must widen to every reservation-holding state, or a revoked
    // grant will leave claimed work alive — exactly the revocation race the K5
    // exit gate names.
    let rows = sqlx::query(
        "SELECT intent.intent_hash, state.state_revision, state.current_event_hash
         FROM karma_intent intent
         JOIN karma_intent_state state ON state.intent_hash = intent.intent_hash
         WHERE intent.grant_uid = ? AND state.status = 'authorized'
         ORDER BY intent.intent_hash",
    )
    .bind(grant_uid)
    .fetch_all(&mut **tx)
    .await?;
    let mut cancelled = Vec::new();
    for row in rows {
        let intent_hash = parse_hash(row.get("intent_hash"))?;
        let previous_event_hash = parse_hash(row.get("current_event_hash"))?;
        let state_revision: i64 = row.get("state_revision");
        let next_revision = u64::try_from(state_revision + 1)
            .map_err(|_| protocol("stored Karma intent revision is invalid"))?;
        // Every cancelled intent names the same revocation request as its cause;
        // the Fact for that revocation is reachable through it.
        let event_hash = append_transition_tx(
            tx,
            IntentTransition {
                schema: IntentTransitionSchema::V1,
                intent_hash: intent_hash.clone(),
                state_revision: next_revision,
                previous_event_hash: Some(previous_event_hash),
                cause_request_id: request_id.to_string(),
                actor_person_uid: actor_person_uid.to_string(),
                status: IntentStatus::Cancelled,
                reason: Some(reason.to_string()),
            },
            at,
        )
        .await?;
        let updated = sqlx::query(
            "UPDATE karma_intent_state
             SET state_revision = ?, status = 'cancelled', current_event_hash = ?,
                 cancelled_reason = ?, actor_person_uid = ?, updated_at = ?
             WHERE intent_hash = ? AND state_revision = ? AND status = 'authorized'",
        )
        .bind(state_revision + 1)
        .bind(event_hash.as_str())
        .bind(reason)
        .bind(actor_person_uid)
        .bind(at)
        .bind(intent_hash.as_str())
        .bind(state_revision)
        .execute(&mut **tx)
        .await?;
        if updated.rows_affected() != 1 {
            return Err(protocol("Karma intent cancellation lost its state CAS"));
        }
        cancelled.push(intent_hash);
    }
    Ok(cancelled)
}

/// What this grant handle has already spent. Counted across the handle, never
/// per revision: if narrowing reset consumption, narrowing would be a way to
/// refill a spent budget.
async fn budget_usage_tx(
    tx: &mut Transaction<'_, Sqlite>,
    grant_uid: &str,
    revision: &DelegationGrantRevision,
    logical_at: TimestampMs,
    amount: Option<&IntentAmount>,
) -> Result<BudgetUsage, StoreError> {
    // Every query below asks the database which states hold a reservation
    // instead of naming them. When execution states arrive, they start counting
    // against the budget automatically rather than being silently free.
    let intents: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM karma_intent intent
         JOIN karma_intent_state state ON state.intent_hash = intent.intent_hash
         JOIN karma_intent_status kind ON kind.status = state.status
         WHERE intent.grant_uid = ? AND kind.holds_reservation = 1",
    )
    .bind(grant_uid)
    .fetch_one(&mut **tx)
    .await?;

    let window_index = revision
        .spec
        .budget
        .window_index(revision.spec.valid_from, logical_at);
    let window_intents: i64 = match window_index {
        Some(index) => {
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM karma_intent intent
                 JOIN karma_intent_state state ON state.intent_hash = intent.intent_hash
                 JOIN karma_intent_status kind ON kind.status = state.status
                 WHERE intent.grant_uid = ? AND kind.holds_reservation = 1
                   AND intent.window_index = ?",
            )
            .bind(grant_uid)
            .bind(index as i64)
            .fetch_one(&mut **tx)
            .await?
        }
        None => 0,
    };

    // Summed in Rust as i128: exact decimals do not survive SQLite's numeric
    // affinity, and a budget must never be approximate.
    let quantity = match (&revision.spec.budget.quantity_limit, amount) {
        (Some(limit), Some(_)) => {
            let mantissas: Vec<String> = sqlx::query_scalar(
                "SELECT intent.quantity_mantissa FROM karma_intent intent
                 JOIN karma_intent_state state ON state.intent_hash = intent.intent_hash
                 JOIN karma_intent_status kind ON kind.status = state.status
                 WHERE intent.grant_uid = ? AND kind.holds_reservation = 1
                   AND intent.quantity_unit_uid = ? AND intent.quantity_scale = ?
                   AND intent.quantity_mantissa IS NOT NULL",
            )
            .bind(grant_uid)
            .bind(limit.unit_uid.as_str())
            .bind(i64::from(limit.limit.scale()))
            .fetch_all(&mut **tx)
            .await?;
            let mut total: i128 = 0;
            for mantissa in mantissas {
                let parsed: i128 = mantissa
                    .parse()
                    .map_err(|_| protocol("stored Karma intent quantity is invalid"))?;
                total = total
                    .checked_add(parsed)
                    .ok_or_else(|| protocol("Karma intent quantity total overflowed"))?;
            }
            Some(DecimalValue::from_mantissa(limit.limit.scale(), total).map_err(boundary)?)
        }
        _ => None,
    };

    Ok(BudgetUsage {
        intents: u64::try_from(intents).unwrap_or(u64::MAX),
        window_index,
        window_intents: u64::try_from(window_intents).unwrap_or(u64::MAX),
        quantity,
    })
}

/// The Fact that records an intent's creation or cancellation. Intents are
/// durable evidence, so their lifecycle joins the Ledger like everything else.
pub(crate) async fn append_intent_fact<F>(
    tx: &mut Transaction<'_, Sqlite>,
    program_uid: &str,
    request_id: &str,
    actor_person_uid: &str,
    payload: &impl Serialize,
    now: DateTime<Utc>,
    sign: &F,
) -> Result<Fact, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    let previous = crate::facts::last_hash(tx).await?;
    let mut fact = nucleus::fact::seal(
        NewFact {
            uid: None,
            record_uid: program_uid.to_string(),
            delta: crate::exact::zero(),
            at: Some(now),
            actor_uid: Some(actor_person_uid.to_string()),
            cause: Cause {
                kind: CauseKind::Action,
                uid: Some(request_id.to_string()),
            },
            payload: Some(canonical_string(payload)?),
        },
        &previous,
        now,
    );
    fact.signature = sign(&fact.hash);
    crate::facts::insert(tx, &fact).await?;
    Ok(fact)
}

/// Which capability an `act` template asks for. K5.2 ships the reversible local
/// data capabilities only; anything else has no mapping yet and is refused
/// rather than silently treated as a lesser permission.
fn template_capability(proposal: &KarmaCandidateProposal) -> Result<Capability, StoreError> {
    let capability = match proposal.template.as_str() {
        // No domain aliases here. `record.add-quantity` already names what the
        // capability grants; an `economy.add` spelling beside it would make one
        // domain a first-class citizen of the kernel's permission table and
        // invite every other domain to add its own synonym.
        "record.add-quantity" => Capability::RecordAddQuantity,
        "record.set-quantity" => Capability::RecordSetQuantity,
        "link.create" => Capability::LinkCreate,
        "metadata.write" => Capability::MetadataWrite,
        "task.create" => Capability::TaskCreate,
        other => {
            return Err(protocol(format!(
                "Karma template {other} has no authorized capability in K5.2"
            )));
        }
    };
    // Belt and braces: this slice may only ever authorize reversible local data
    // work, so a future template mapping cannot quietly reach further.
    if capability.family() != nucleus::karma::CapabilityFamily::LocalReversibleData {
        return Err(protocol(
            "K5.2 authorizes only reversible local data capabilities",
        ));
    }
    Ok(capability)
}

/// The capability's wire name, taken from its own serialization so the column
/// and the frozen intent can never disagree.
fn capability_name(capability: Capability) -> Result<String, StoreError> {
    serde_json::to_value(capability)
        .map_err(json_protocol)?
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| protocol("Karma capability has no wire name"))
}

fn denial_list<T: std::fmt::Debug>(denials: &[T]) -> String {
    if denials.is_empty() {
        "none".to_string()
    } else {
        format!("{denials:?}")
    }
}

fn map_intent(row: sqlx::sqlite::SqliteRow) -> Result<KarmaIntentRow, StoreError> {
    let intent_hash = parse_hash(row.get("intent_hash"))?;
    let intent_json: String = row.get("intent_json");
    let intent: KarmaIntent = serde_json::from_str(&intent_json).map_err(json_protocol)?;
    if canonical_string(&intent)? != intent_json
        || intent.intent_hash().map_err(boundary)? != intent_hash
    {
        return Err(protocol("stored Karma intent is invalid"));
    }
    Ok(KarmaIntentRow {
        intent_hash,
        candidate_hash: parse_hash(row.get("candidate_hash"))?,
        grant_uid: row.get("grant_uid"),
        grant_revision_hash: parse_hash(row.get("grant_revision_hash"))?,
        intent,
        created_at: row.get("created_at"),
    })
}

fn map_state(row: sqlx::sqlite::SqliteRow) -> Result<KarmaIntentStateRow, StoreError> {
    let status = IntentStatus::parse(&row.get::<String, _>("status"))
        .ok_or_else(|| protocol("stored Karma intent status is invalid"))?;
    let state_revision: i64 = row.get("state_revision");
    Ok(KarmaIntentStateRow {
        intent_hash: parse_hash(row.get("intent_hash"))?,
        state_revision: u64::try_from(state_revision)
            .map_err(|_| protocol("stored Karma intent revision is invalid"))?,
        status,
        current_event_hash: parse_hash(row.get("current_event_hash"))?,
        cancelled_reason: row.get("cancelled_reason"),
        actor_person_uid: row.get("actor_person_uid"),
        updated_at: row.get("updated_at"),
    })
}

fn map_event(row: sqlx::sqlite::SqliteRow) -> Result<KarmaIntentEventRow, StoreError> {
    let event_hash = parse_hash(row.get("event_hash"))?;
    let event_json: String = row.get("event_json");
    let transition: IntentTransition = serde_json::from_str(&event_json).map_err(json_protocol)?;
    if canonical_string(&transition)? != event_json
        || transition.event_hash().map_err(boundary)? != event_hash
    {
        return Err(protocol("stored Karma intent event is invalid"));
    }
    Ok(KarmaIntentEventRow {
        event_hash,
        transition,
        created_at: row.get("created_at"),
    })
}

fn canonical_string<T: Serialize>(value: &T) -> Result<String, StoreError> {
    String::from_utf8(canonical_json_bytes(value).map_err(boundary)?)
        .map_err(|error| protocol(error.to_string()))
}

fn parse_hash(value: String) -> Result<CanonicalHash, StoreError> {
    CanonicalHash::parse(value).map_err(boundary)
}

fn sql_i64(value: u64, label: &str) -> Result<i64, StoreError> {
    i64::try_from(value).map_err(|_| protocol(format!("{label} exceeds SQLite range")))
}

fn boundary(error: nucleus::karma::KarmaBoundaryError) -> StoreError {
    protocol(error.to_string())
}

fn json_protocol(error: serde_json::Error) -> StoreError {
    protocol(error.to_string())
}

fn protocol(message: impl Into<String>) -> StoreError {
    sqlx::Error::Protocol(message.into())
}
