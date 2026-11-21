//! Reverse a local application and its secondary effects in one transaction.

use chrono::{DateTime, Utc};
use nucleus::{
    Cause, CauseKind, Fact,
    fact::{NewFact, seal},
    new_uid,
};
use serde_json::json;
use sqlx::{Row, SqlitePool};

use super::invalid;
use crate::{
    StoreError,
    exact::{negate, read_decimal},
    facts,
    records::bump_quantity,
    transfer_effects, write_tx,
};

/// Reverse a signed local application and return its correction Fact.
///
/// The second value is true for an exact replay of a saved request.
///
/// # Errors
///
/// Returns an error for an unauthorized owner or Cell, a conflicting request,
/// a changed Record unit, a duplicate correction, or any failed write.
pub async fn compensate<F>(
    pool: &SqlitePool,
    application: &str,
    person: &str,
    request: &str,
    now: DateTime<Utc>,
    sign: F,
) -> Result<(Fact, bool), StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    if request.trim().is_empty() || request.len() > 200 {
        return Err(invalid("a correction needs a request identifier"));
    }
    let mut tx = write_tx(pool).await?;
    let row = sqlx::query(
        "SELECT a.local_record_uid, a.application_fact_uid, f.delta_mantissa, f.delta_scale,
                json_extract(f.payload, '$.local_unit_uid') AS original_unit,
                r.unit_uid AS current_unit
         FROM transfer_local_application a JOIN fact f ON f.uid = a.application_fact_uid
         JOIN record r ON r.uid = a.local_record_uid
         JOIN record o ON o.slug = 'local-organ' AND o.kind = 'organ'
         WHERE a.uid = ? AND a.participant_person_uid = ? AND r.organ_uid = o.uid
           AND r.deleted_at IS NULL",
    )
    .bind(application)
    .bind(person)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| invalid("only the owner can correct a local application to a live Record"))?;
    let writer: Option<String> = sqlx::query_scalar(
        "SELECT cell_uid FROM transfer_sync_owner
         WHERE table_name = 'transfer_local_application' AND row_key = json_array(?)
           AND cell_uid != (SELECT uid FROM record WHERE slug = 'local-cell')",
    )
    .bind(application)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(writer) = writer {
        return Err(invalid(format!(
            "Submit this private correction to its writing Cell {writer}"
        )));
    }
    let previous: Option<(String, String, String)> = sqlx::query_as(
        "SELECT application_uid, person_uid, fact_uid
         FROM transfer_private_application_correction WHERE request_id = ?",
    )
    .bind(request)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some((previous, owner, fact)) = previous {
        if previous != application || owner != person {
            return Err(invalid(
                "correction request was reused with different targets",
            ));
        }
        tx.rollback().await?;
        return Ok((
            facts::get(pool, &fact)
                .await?
                .ok_or(sqlx::Error::RowNotFound)?,
            true,
        ));
    }
    if row.get::<Option<&str>, _>("original_unit") != row.get::<Option<&str>, _>("current_unit") {
        return Err(invalid(
            "restore the original Record unit before correcting this application",
        ));
    }
    let corrected: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM transfer_private_application_correction WHERE application_uid = ?)",
    )
    .bind(application)
    .fetch_one(&mut *tx)
    .await?;
    if corrected {
        return Err(invalid("this private application was already corrected"));
    }
    let record: &str = row.get("local_record_uid");
    let original: &str = row.get("application_fact_uid");
    let delta = negate(read_decimal(&row, "delta")?)?;
    let hash = facts::last_hash(&mut tx).await?;
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
            payload: Some(
                json!({
                    "action": "compensate-transfer-application",
                    "application": application,
                    "original_fact": original,
                    "request_id": request,
                })
                .to_string(),
            ),
        },
        &hash,
        now,
    );
    fact.signature = Some(
        sign(&fact.hash)
            .ok_or_else(|| invalid("a private correction needs the owner's signature"))?,
    );
    facts::insert(&mut tx, &fact).await?;
    let at = now.to_rfc3339();
    bump_quantity(&mut tx, record, delta, &at).await?;
    sqlx::query(
        "INSERT INTO transfer_private_application_correction
            (uid, application_uid, person_uid, fact_uid, request_id, created_at)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(new_uid("tpc"))
    .bind(application)
    .bind(person)
    .bind(&fact.uid)
    .bind(request)
    .bind(at)
    .execute(&mut *tx)
    .await?;
    transfer_effects::compensate_on(&mut tx, original, person, None, now, &sign).await?;
    tx.commit().await?;
    Ok((fact, false))
}
