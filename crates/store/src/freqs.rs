//! Frequency repository: rows the timer wheel drives (blueprint VI.1).

use chrono::{DateTime, Utc};
use nucleus::{FrequencySpec, RecordKind};
use sqlx::{Row, SqlitePool};

use crate::records::{self, NewRecord};
use crate::StoreError;

#[derive(Debug, Clone)]
pub struct FreqRow {
    pub record_uid: String,
    pub slug: Option<String>,
    pub spec: FrequencySpec,
}

pub struct NewFrequency<'a> {
    pub slug: &'a str,
    pub head: &'a str,
    pub seconds: i64,
    pub days: i64,
    pub months: i64,
    pub day_of_week: Option<u8>,
    pub next_at: DateTime<Utc>,
    pub catch_up: bool,
}

pub async fn create(pool: &SqlitePool, new: NewFrequency<'_>) -> Result<String, StoreError> {
    let rec = records::create(
        pool,
        NewRecord {
            slug: Some(new.slug),
            kind: RecordKind::Signal, // frequencies are timer-signals
            head: new.head,
            body: "",
            quantity: 1.0,
        },
    )
    .await?;
    sqlx::query(
        "INSERT INTO frequency (record_uid, seconds, days, months, day_of_week, next_at, catch_up)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&rec.uid)
    .bind(new.seconds)
    .bind(new.days)
    .bind(new.months)
    .bind(new.day_of_week.map(|d| d as i64))
    .bind(new.next_at.to_rfc3339())
    .bind(new.catch_up as i64)
    .execute(pool)
    .await?;
    Ok(rec.uid)
}

pub async fn due(pool: &SqlitePool, now: DateTime<Utc>) -> Result<Vec<FreqRow>, StoreError> {
    let rows = sqlx::query(
        "SELECT r.uid, r.slug, f.seconds, f.days, f.months, f.day_of_week, f.next_at,
                f.finish_at, f.catch_up
         FROM frequency f JOIN record r ON r.uid = f.record_uid
         WHERE f.next_at <= ? AND r.quantity != 0",
    )
    .bind(now.to_rfc3339())
    .fetch_all(pool)
    .await?;
    map_rows(rows)
}

/// Every enabled frequency — Imagination's virtual timer wheel input.
pub async fn all_enabled(pool: &SqlitePool) -> Result<Vec<FreqRow>, StoreError> {
    let rows = sqlx::query(
        "SELECT r.uid, r.slug, f.seconds, f.days, f.months, f.day_of_week, f.next_at,
                f.finish_at, f.catch_up
         FROM frequency f JOIN record r ON r.uid = f.record_uid WHERE r.quantity != 0",
    )
    .fetch_all(pool)
    .await?;
    map_rows(rows)
}

fn map_rows(rows: Vec<sqlx::sqlite::SqliteRow>) -> Result<Vec<FreqRow>, StoreError> {
    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let next_at: String = row.get("next_at");
            let finish_at: Option<String> = row.get("finish_at");
            Some(FreqRow {
                record_uid: row.get("uid"),
                slug: row.get("slug"),
                spec: FrequencySpec {
                    seconds: row.get::<i64, _>("seconds"),
                    days: row.get::<i64, _>("days"),
                    months: row.get::<i64, _>("months"),
                    day_of_week: row.get::<Option<i64>, _>("day_of_week").map(|d| d as u8),
                    next_at: DateTime::parse_from_rfc3339(&next_at).ok()?.with_timezone(&Utc),
                    finish_at: finish_at
                        .and_then(|f| DateTime::parse_from_rfc3339(&f).ok())
                        .map(|f| f.with_timezone(&Utc)),
                    catch_up: row.get::<i64, _>("catch_up") != 0,
                },
            })
        })
        .collect())
}

pub async fn set_next_at(
    pool: &SqlitePool,
    record_uid: &str,
    next_at: DateTime<Utc>,
) -> Result<(), StoreError> {
    sqlx::query("UPDATE frequency SET next_at = ? WHERE record_uid = ?")
        .bind(next_at.to_rfc3339())
        .bind(record_uid)
        .execute(pool)
        .await?;
    Ok(())
}
