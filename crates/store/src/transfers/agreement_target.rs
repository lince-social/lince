use chrono::{DateTime, Utc};
use nucleus::{Fact, transfer::AgreementGuard};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqliteConnection, SqlitePool};

use super::{AgreementTransitionCommit, AgreementTransitionEventRow, AgreementTransitionInput};
use crate::StoreError;

#[derive(Clone, Debug)]
pub struct AgreementTargetInput {
    pub transition: AgreementTransitionInput,
    pub expected: Option<AgreementGuard>,
    pub expected_state: Option<nucleus::transfer::karma::Guard>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgreementTargetOutcome {
    pub before: u8,
    pub level: u8,
    pub changes: Vec<AgreementTransitionEventRow>,
    pub facts: Vec<Fact>,
    pub replayed: bool,
}

fn invalid(message: impl ToString) -> StoreError {
    StoreError::Protocol(message.to_string())
}

fn fingerprint(input: &AgreementTargetInput) -> Result<String, StoreError> {
    let value = &input.transition;
    nucleus::karma::canonical_hash(
        "lince.transfer.agreement-target.v1",
        &(
            &value.transfer_uid,
            value.expected_revision,
            &value.person_uid,
            value.to_level,
            &input.expected,
            &input.expected_state,
        ),
    )
    .map(|hash| hash.as_str().into())
    .map_err(invalid)
}

async fn receipt(
    connection: &mut SqliteConnection,
    request: &str,
    fingerprint: &str,
) -> Result<Option<AgreementTargetOutcome>, StoreError> {
    let Some(row) = sqlx::query("SELECT fingerprint, result FROM transfer_agreement_target_request WHERE idempotency_key = ?")
        .bind(request).fetch_optional(connection).await? else { return Ok(None) };
    if row.get::<String, _>("fingerprint") != fingerprint {
        return Err(invalid("transfer_agreement_target_request_conflict"));
    }
    let mut outcome: AgreementTargetOutcome =
        serde_json::from_str(&row.get::<String, _>("result")).map_err(invalid)?;
    outcome.replayed = true;
    Ok(Some(outcome))
}

async fn source(
    connection: &mut SqliteConnection,
    input: &AgreementTransitionInput,
) -> Result<AgreementGuard, StoreError> {
    let revision = i64::try_from(input.expected_revision).map_err(invalid)?;
    let current: Option<i64> =
        sqlx::query_scalar("SELECT revision FROM transfer WHERE record_uid = ?")
            .bind(&input.transfer_uid)
            .fetch_optional(&mut *connection)
            .await?;
    if current != Some(revision) {
        return Err(invalid("transfer_revision_stale"));
    }
    let signed: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM transfer_revision tr JOIN fact f ON f.uid = tr.fact_uid WHERE tr.transfer_uid = ? AND tr.revision = ? AND (f.signature IS NOT NULL OR EXISTS(SELECT 1 FROM fact_action_intent fai JOIN signed_action_intent sai ON sai.uid = fai.intent_uid WHERE fai.fact_uid = f.uid AND sai.status = 'committed')))")
        .bind(&input.transfer_uid).bind(revision).fetch_one(&mut *connection).await?;
    if !signed || revision == 0 {
        return Err(invalid(
            "agreement requires an existing signed Transfer revision",
        ));
    }
    let party: Option<String> = sqlx::query_scalar("SELECT tp.uid FROM transfer_party tp JOIN record person ON person.uid = tp.actor_uid AND person.kind = 'person' AND person.deleted_at IS NULL WHERE tp.transfer_uid = ? AND tp.actor_uid = ?")
        .bind(&input.transfer_uid).bind(&input.person_uid).fetch_optional(&mut *connection).await?;
    let party = party.ok_or_else(|| {
        invalid("only an accepted Person party may change their own Transfer agreement")
    })?;
    let row = sqlx::query("SELECT level, last_event_uid FROM transfer_agreement WHERE transfer_uid = ? AND party_uid = ? AND revision = ?")
        .bind(&input.transfer_uid).bind(&party).bind(revision).fetch_optional(&mut *connection).await?;
    Ok(AgreementGuard {
        level: row
            .as_ref()
            .map_or(0, |row| row.get::<i64, _>("level") as u8),
        change_uid: row.and_then(|row| row.get("last_event_uid")),
    })
}

pub async fn assign_agreement<F>(
    pool: &SqlitePool,
    input: AgreementTargetInput,
    now: DateTime<Utc>,
    sign: F,
) -> Result<AgreementTargetOutcome, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    let transition = &input.transition;
    let request = super::validate_request_key(&transition.idempotency_key)?;
    if transition.to_level > 2
        || input.expected.as_ref().is_some_and(|guard| {
            guard.level > 2
                || guard.change_uid.as_ref().is_some_and(|uid| {
                    uid.is_empty() || uid.len() > 200 || uid.chars().any(char::is_control)
                })
        })
    {
        return Err(invalid(
            "agreement target and expected level must be 0, 1, or 2 with a valid change identity",
        ));
    }
    let fingerprint = fingerprint(&input)?;
    {
        let mut connection = pool.acquire().await?;
        if let Some(outcome) = receipt(&mut connection, request, &fingerprint).await? {
            return Ok(outcome);
        }
    }
    let mut tx = crate::write_tx(pool).await?;
    if let Some(outcome) = receipt(&mut tx, request, &fingerprint).await? {
        tx.rollback().await?;
        return Ok(outcome);
    }
    let initial = source(&mut tx, transition).await?;
    super::karma_snapshot::check_on(
        &mut tx,
        &transition.transfer_uid,
        input.expected_state.as_ref(),
    )
    .await?;
    if input
        .expected
        .as_ref()
        .is_some_and(|expected| expected != &initial)
    {
        return Err(invalid("transfer_agreement_source_changed"));
    }
    let mut outcome = AgreementTargetOutcome {
        before: initial.level,
        level: initial.level,
        changes: Vec::new(),
        facts: Vec::new(),
        replayed: false,
    };
    while outcome.level != transition.to_level {
        let next = if outcome.level < transition.to_level {
            outcome.level + 1
        } else {
            outcome.level - 1
        };
        let step = nucleus::karma::canonical_hash(
            "lince.transfer.agreement-target-step.v1",
            &(request, outcome.level, next),
        )
        .map_err(invalid)?;
        let mut adjacent = transition.clone();
        adjacent.to_level = next;
        adjacent.idempotency_key = format!("agreement-target-step:{}", step.as_str());
        match super::transition_agreement_on(&mut tx, adjacent, now, &sign).await? {
            AgreementTransitionCommit::Committed(committed) => {
                outcome.level = next;
                outcome.changes.push(committed.event);
                outcome.facts.push(committed.fact);
            }
            _ => {
                return Err(invalid(
                    "agreement target did not commit its expected adjacent step",
                ));
            }
        }
    }
    sqlx::query("INSERT INTO transfer_agreement_target_request(idempotency_key, transfer_uid, person_uid, revision, target_level, fingerprint, result, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)")
        .bind(request).bind(&transition.transfer_uid).bind(&transition.person_uid).bind(i64::try_from(transition.expected_revision).map_err(invalid)?).bind(i64::from(transition.to_level)).bind(&fingerprint).bind(serde_json::to_string(&outcome).map_err(invalid)?).bind(now.to_rfc3339()).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(outcome)
}
