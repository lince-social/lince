//! Project the current revision's bounded Journey event history.

use std::collections::BTreeMap;

use chrono::DateTime;
use nucleus::{
    execution::now,
    transfer::transport::{Event, JOURNEY_DOMAIN, Journey, PICKUP_GRACE_MILLIS, Progress, Step},
};
use sqlx::{Row, SqliteConnection, SqlitePool};

use super::invalid;
use crate::StoreError;

/// Read the current Journey for a Transport Transfer, if present.
///
/// # Errors
///
/// Returns an error if the pickup window or event history is invalid or unavailable.
pub async fn journey(pool: &SqlitePool, transfer: &str) -> Result<Option<Journey>, StoreError> {
    journey_on(&mut *pool.acquire().await?, transfer).await
}

/// Read the current Journey on the caller's connection.
///
/// # Errors
///
/// Returns an error if the pickup window or event history is invalid or unavailable.
pub async fn journey_on(
    connection: &mut SqliteConnection,
    transfer: &str,
) -> Result<Option<Journey>, StoreError> {
    let header: Option<(i64, Option<String>)> = sqlx::query_as(
        "SELECT t.revision, MIN(p.window_end) FROM transfer t
         JOIN record r ON r.uid = t.record_uid AND r.deleted_at IS NULL
         JOIN promise p ON p.transfer_uid = t.record_uid AND p.state != 'withdrawn'
         WHERE t.record_uid = ? AND json_extract(p.item_json, '$.transport.activity') IS NOT NULL
         GROUP BY t.record_uid",
    )
    .bind(transfer)
    .fetch_optional(&mut *connection)
    .await?;
    let Some((revision, until)) = header else {
        return Ok(None);
    };
    let until = until.ok_or_else(|| invalid("Transport pickup window unavailable"))?;
    let until = DateTime::parse_from_rfc3339(&until)
        .map_err(invalid)?
        .timestamp_millis()
        .checked_add(PICKUP_GRACE_MILLIS)
        .ok_or_else(|| invalid("Pickup grace interval overflows"))?;
    let people: Vec<(String, i64)> = sqlx::query_as(
        "SELECT p.actor_uid, COALESCE(a.level, 0) FROM transfer_party p
         LEFT JOIN transfer_agreement a ON a.party_uid = p.uid AND a.transfer_uid = p.transfer_uid AND a.revision = ?
         WHERE p.transfer_uid = ? ORDER BY p.actor_uid",
    )
    .bind(revision)
    .bind(transfer)
    .fetch_all(&mut *connection)
    .await?;
    let agreed = !people.is_empty() && people.iter().all(|(_, level)| *level == 2);
    let mut participants: BTreeMap<_, Option<(Step, String, String)>> = people
        .into_iter()
        .map(|(person, _)| (person, None))
        .collect();

    let rows = sqlx::query(
        "SELECT uid, actor_uid, at, CASE WHEN length(CAST(payload AS BLOB)) <= 4096 THEN payload END AS payload
         FROM fact WHERE record_uid = ? AND delta_mantissa = '0' AND cause_kind = 'user_edit'
           AND CASE WHEN json_valid(payload) THEN json_extract(payload, '$.journey.domain') = ?
               AND json_extract(payload, '$.journey.revision') = ? END
         ORDER BY rowid LIMIT 1025",
    )
    .bind(transfer)
    .bind(JOURNEY_DOMAIN)
    .bind(revision)
    .fetch_all(connection)
    .await?;
    if rows.len() > 1024 {
        return Err(invalid("Journey history exceeds its bounded event count"));
    }

    let revision = revision.cast_unsigned();
    let mut started = false;
    for row in rows {
        let event = row
            .get::<Option<&str>, _>("payload")
            .and_then(Event::parse)
            .ok_or_else(|| invalid("Journey event is unavailable or invalid"))?;
        if event.transfer != transfer
            || event.revision != revision
            || row.get::<Option<&str>, _>("actor_uid") != Some(event.person.as_str())
        {
            return Err(invalid("Journey attribution differs from its signed event"));
        }
        if let Some(progress) = participants.get_mut(&event.person) {
            started |= matches!(event.step, Step::Departed | Step::Destination);
            if progress
                .as_ref()
                .is_none_or(|(step, ..)| event.step > *step)
            {
                *progress = Some((event.step, row.get("uid"), row.get("at")));
            }
        }
    }
    let expired = !started && now().timestamp_millis() >= until;
    let finished = participants
        .values()
        .filter(|entry| entry.as_ref().is_some_and(|(step, ..)| step.terminal()))
        .count();
    let all_finished = !participants.is_empty() && participants.len() == finished;
    Ok(Some(Journey {
        transfer: transfer.into(),
        revision,
        agreed,
        participants: participants
            .into_iter()
            .map(|(person, progress)| {
                let (step, fact, at) = progress.map_or((None, None, None), |(step, fact, at)| {
                    (Some(step), Some(fact), Some(at))
                });
                Progress {
                    person,
                    step,
                    fact,
                    at,
                }
            })
            .collect(),
        started,
        expired,
        capacity_released: expired || finished > 0,
        all_finished,
        pickup_grace_until: DateTime::from_timestamp_millis(until)
            .ok_or_else(|| invalid("Pickup grace timestamp unavailable"))?
            .to_rfc3339(),
    }))
}
