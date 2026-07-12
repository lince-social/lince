//! Senses repository (blueprint X): match rules as records (`sense_rule`
//! sidecar) and the discovery cache of remote open promises.

use chrono::Utc;
use nucleus::RecordKind;
use sqlx::{Row, SqlitePool};

use crate::StoreError;
use crate::records::{self, NewRecord};

#[derive(Debug, Clone)]
pub struct SenseRuleRow {
    pub record_uid: String,
    pub watch_concept: Option<String>,
    pub max_proximity: u32,
    pub min_confidence: f64,
    pub auto: String,
}

pub struct NewSenseRule<'a> {
    pub slug: &'a str,
    pub head: &'a str,
    pub watch_concept: Option<&'a str>,
    pub max_proximity: u32,
    pub min_confidence: f64,
    pub auto: &'a str, // draft_only | ask | auto_propose
}

pub async fn create_sense_rule(
    pool: &SqlitePool,
    new: NewSenseRule<'_>,
) -> Result<String, StoreError> {
    let rec = records::create(
        pool,
        NewRecord {
            slug: Some(new.slug),
            kind: RecordKind::Rule,
            head: new.head,
            body: "",
            quantity: 1.0, // active by default
        },
    )
    .await?;
    sqlx::query(
        "INSERT INTO sense_rule (record_uid, watch_concept, max_proximity, min_confidence, auto)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&rec.uid)
    .bind(new.watch_concept)
    .bind(new.max_proximity as i64)
    .bind(new.min_confidence)
    .bind(new.auto)
    .execute(pool)
    .await?;
    Ok(rec.uid)
}

/// Every ACTIVE match rule (record quantity != 0).
pub async fn active_sense_rules(pool: &SqlitePool) -> Result<Vec<SenseRuleRow>, StoreError> {
    Ok(sqlx::query(
        "SELECT s.*, r.quantity FROM sense_rule s
           JOIN record r ON r.uid = s.record_uid
          WHERE r.quantity != 0",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|r| SenseRuleRow {
        record_uid: r.get("record_uid"),
        watch_concept: r.get("watch_concept"),
        max_proximity: r.get::<i64, _>("max_proximity") as u32,
        min_confidence: r.get("min_confidence"),
        auto: r.get("auto"),
    })
    .collect())
}

#[derive(Debug, Clone)]
pub struct RemoteOpenRow {
    pub promise_uid: String,
    pub organ: String,
    pub proximity: u32,
    pub concept: Option<String>,
    pub unit: Option<String>,
    pub delta: f64,
    pub window_start: Option<String>,
    pub window_end: Option<String>,
    pub confidence: f64,
}

/// Upsert one remote open promise into the discovery cache (Part XV feeds
/// this; tests and the matcher read it).
pub async fn upsert_remote_open(
    pool: &SqlitePool,
    row: &RemoteOpenRow,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO discovery_cache
            (promise_uid, organ, proximity, concept, unit, delta,
             window_start, window_end, confidence, fetched_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(promise_uid) DO UPDATE SET
            organ = excluded.organ, proximity = excluded.proximity,
            concept = excluded.concept, unit = excluded.unit,
            delta = excluded.delta, window_start = excluded.window_start,
            window_end = excluded.window_end, confidence = excluded.confidence,
            fetched_at = excluded.fetched_at",
    )
    .bind(&row.promise_uid)
    .bind(&row.organ)
    .bind(row.proximity as i64)
    .bind(&row.concept)
    .bind(&row.unit)
    .bind(row.delta)
    .bind(&row.window_start)
    .bind(&row.window_end)
    .bind(row.confidence)
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_remote_open(pool: &SqlitePool) -> Result<Vec<RemoteOpenRow>, StoreError> {
    Ok(sqlx::query("SELECT * FROM discovery_cache ORDER BY promise_uid")
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|r| RemoteOpenRow {
            promise_uid: r.get("promise_uid"),
            organ: r.get("organ"),
            proximity: r.get::<i64, _>("proximity") as u32,
            concept: r.get("concept"),
            unit: r.get("unit"),
            delta: r.get("delta"),
            window_start: r.get("window_start"),
            window_end: r.get("window_end"),
            confidence: r.get("confidence"),
        })
        .collect())
}
