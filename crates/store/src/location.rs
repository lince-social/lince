use crate::StoreError;
use nucleus::location::{Choice, Settings};
use sqlx::{SqliteConnection, SqlitePool};

fn invalid(error: impl ToString) -> StoreError {
    StoreError::Protocol(error.to_string())
}

pub async fn settings(pool: &SqlitePool, record: &str) -> Result<Option<Settings>, StoreError> {
    let raw: Option<String> =
        sqlx::query_scalar("SELECT settings_json FROM location_settings WHERE record_uid = ?")
            .bind(record)
            .fetch_optional(pool)
            .await?;
    raw.map(|raw| serde_json::from_str(&raw).map_err(invalid))
        .transpose()
}

pub async fn save_on(
    connection: &mut SqliteConnection,
    settings: &Settings,
) -> Result<(), StoreError> {
    settings.validate().map_err(invalid)?;
    let existing: Option<String> =
        sqlx::query_scalar("SELECT controller_uid FROM location_settings WHERE record_uid = ?")
            .bind(&settings.record_uid)
            .fetch_optional(&mut *connection)
            .await?;
    if existing
        .as_deref()
        .is_some_and(|person| person != settings.controller_uid)
    {
        return Err(invalid(
            "Only the location controller can change these settings",
        ));
    }
    sqlx::query("INSERT INTO location_settings(record_uid,controller_uid,settings_json,updated_at) VALUES(?,?,?,?) ON CONFLICT(record_uid) DO UPDATE SET settings_json=excluded.settings_json,updated_at=excluded.updated_at")
        .bind(&settings.record_uid).bind(&settings.controller_uid)
        .bind(serde_json::to_string(settings).map_err(invalid)?)
        .bind(nucleus::execution::now().to_rfc3339()).execute(connection).await?;
    Ok(())
}

pub async fn people(pool: &SqlitePool) -> Result<Vec<Choice>, StoreError> {
    let rows: Vec<(String, String)> = sqlx::query_as("SELECT uid,head FROM record WHERE kind='person' AND deleted_at IS NULL ORDER BY head,uid LIMIT 256")
        .fetch_all(pool).await?;
    let mut people = Vec::new();
    for (uid, label) in rows {
        if crate::people::is_active(pool, &uid).await? {
            people.push(Choice {
                uid,
                label,
                node_id: None,
            });
        }
    }
    Ok(people)
}

pub async fn retained_for(pool: &SqlitePool, person: &str) -> Result<Vec<Settings>, StoreError> {
    let rows: Vec<String> = sqlx::query_scalar(
        "SELECT settings_json FROM location_settings WHERE controller_uid=? LIMIT 256",
    )
    .bind(person)
    .fetch_all(pool)
    .await?;
    rows.iter()
        .map(|raw| serde_json::from_str(raw).map_err(invalid))
        .collect()
}

pub async fn transfer_ended(pool: &SqlitePool, transfer: &str) -> Result<bool, StoreError> {
    let active: Option<bool> = sqlx::query_scalar("SELECT deleted_at IS NULL AND quantity_mantissa != '0' FROM record WHERE uid=? AND kind='transfer'")
        .bind(transfer).fetch_optional(pool).await?;
    if active != Some(true) {
        return Ok(true);
    }
    let cancelled: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM transfer_cancellation WHERE transfer_uid=? AND applied_fact_uid IS NOT NULL)")
        .bind(transfer).fetch_one(pool).await?;
    if cancelled {
        return Ok(true);
    }
    let terminal: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM promise WHERE transfer_uid=?) AND NOT EXISTS(SELECT 1 FROM promise WHERE transfer_uid=? AND state NOT IN ('kept','broken','withdrawn'))")
        .bind(transfer).bind(transfer).fetch_one(pool).await?;
    if terminal {
        return Ok(true);
    }
    Ok(crate::transfer_agreement::read(pool, transfer)
        .await?
        .0
        .settled)
}
