//! Promises, effects, and decisions (blueprint V, VI.3, XIII.1).

use chrono::Utc;
use nucleus::transfer::{OpenPromiseReusePolicy, TransferLocationSnapshot};
use nucleus::{PromiseState, RecordKind};
use sqlx::{Row, SqlitePool};

use crate::StoreError;
use crate::records::{self, NewRecord};

// ------------------------------------------------------------------ promises

#[derive(Debug, Clone)]
pub struct PromiseRow {
    pub uid: String,
    pub record_uid: Option<String>,
    pub concept_uid: Option<String>,
    pub unit_uid: Option<String>,
    pub delta: f64,
    pub window_start: Option<String>,
    pub window_end: Option<String>,
    pub location: Option<TransferLocationSnapshot>,
    pub party_uid: Option<String>,
    pub state: PromiseState,
    pub condition: Option<String>,
    pub transfer_uid: Option<String>,
    pub rule_uid: Option<String>,
    pub reserve_from: String,
    pub revision: u64,
    pub open_reuse_policy: OpenPromiseReusePolicy,
}

#[derive(Debug, Clone, Default)]
pub struct NewPromise {
    pub record_uid: Option<String>,
    pub concept_uid: Option<String>,
    pub delta: f64,
    pub window_end: Option<String>,
    pub party_uid: Option<String>,
    pub state: Option<PromiseState>, // None = proposed
    pub condition: Option<String>,
    pub transfer_uid: Option<String>,
    pub rule_uid: Option<String>,
    /// None = inherit Transfer, then Cell default (`none` by default).
    pub reserve_from: Option<String>,
}

pub async fn insert_promise(pool: &SqlitePool, p: NewPromise) -> Result<String, StoreError> {
    let uid = nucleus::new_uid("p");
    let now = Utc::now().to_rfc3339();
    // Reservation precedence: promise override, Transfer default, Cell
    // default, then the schema/code default `none`.
    let mut reserve_from = p.reserve_from;
    if reserve_from.is_none() {
        if let Some(transfer_uid) = &p.transfer_uid {
            reserve_from = sqlx::query("SELECT reserve_default FROM transfer WHERE record_uid = ?")
                .bind(transfer_uid)
                .fetch_optional(pool)
                .await?
                .and_then(|r| r.get::<Option<String>, _>("reserve_default"));
        }
    }
    let reserve_from = match reserve_from {
        Some(value) => value,
        None => crate::config::transfer_reservation_default(pool).await?,
    };
    sqlx::query(
        "INSERT INTO promise (uid, record_uid, concept_uid, delta, window_end, party_uid,
                              state, condition, transfer_uid, rule_uid, reserve_from,
                              created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&uid)
    .bind(&p.record_uid)
    .bind(&p.concept_uid)
    .bind(p.delta)
    .bind(&p.window_end)
    .bind(&p.party_uid)
    .bind(p.state.unwrap_or(PromiseState::Proposed).as_str())
    .bind(&p.condition)
    .bind(&p.transfer_uid)
    .bind(&p.rule_uid)
    .bind(&reserve_from)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;
    Ok(uid)
}

/// Promises whose window has passed while still undone (blueprint V.2): the
/// expiry sweep moves agreed/active → broken, open/proposed → withdrawn.
pub async fn expired_promises(
    pool: &SqlitePool,
    now_rfc3339: &str,
) -> Result<Vec<PromiseRow>, StoreError> {
    Ok(sqlx::query(
        "SELECT * FROM promise
          WHERE window_end IS NOT NULL AND window_end < ?
            AND state IN ('open', 'proposed', 'agreed', 'active')
          ORDER BY created_at",
    )
    .bind(now_rfc3339)
    .fetch_all(pool)
    .await?
    .into_iter()
    .filter_map(map_promise)
    .collect())
}

pub async fn set_promise_delta(pool: &SqlitePool, uid: &str, delta: f64) -> Result<(), StoreError> {
    sqlx::query("UPDATE promise SET delta = ?, updated_at = ? WHERE uid = ?")
        .bind(delta)
        .bind(Utc::now().to_rfc3339())
        .bind(uid)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn promises_for_record(
    pool: &SqlitePool,
    record_uid: &str,
) -> Result<Vec<PromiseRow>, StoreError> {
    Ok(sqlx::query("SELECT * FROM promise WHERE record_uid = ?")
        .bind(record_uid)
        .fetch_all(pool)
        .await?
        .into_iter()
        .filter_map(map_promise)
        .collect())
}

fn map_promise(r: sqlx::sqlite::SqliteRow) -> Option<PromiseRow> {
    let state: String = r.get("state");
    Some(PromiseRow {
        uid: r.get("uid"),
        record_uid: r.get("record_uid"),
        concept_uid: r.get("concept_uid"),
        unit_uid: r.get("unit_uid"),
        delta: r.get("delta"),
        window_start: r.get("window_start"),
        window_end: r.get("window_end"),
        location: {
            let lat: Option<f64> = r.get("location_lat");
            let lon: Option<f64> = r.get("location_lon");
            let address: Option<String> = r.get("location_address");
            if lat.is_none() && lon.is_none() && address.is_none() {
                None
            } else {
                Some(TransferLocationSnapshot { lat, lon, address })
            }
        },
        party_uid: r.get("party_uid"),
        state: PromiseState::parse(&state)?,
        condition: r.get("condition"),
        transfer_uid: r.get("transfer_uid"),
        rule_uid: r.get("rule_uid"),
        reserve_from: r.get("reserve_from"),
        revision: r.get::<i64, _>("revision") as u64,
        open_reuse_policy: OpenPromiseReusePolicy::parse(
            r.get::<String, _>("open_reuse_policy").as_str(),
        )?,
    })
}

/// Row mapper shared with the transfers repository.
pub(crate) fn map_promise_pub(r: sqlx::sqlite::SqliteRow) -> Option<PromiseRow> {
    map_promise(r)
}

pub async fn get_promise(pool: &SqlitePool, uid: &str) -> Result<Option<PromiseRow>, StoreError> {
    Ok(sqlx::query("SELECT * FROM promise WHERE uid = ?")
        .bind(uid)
        .fetch_optional(pool)
        .await?
        .and_then(map_promise))
}

pub async fn list_promises(pool: &SqlitePool) -> Result<Vec<PromiseRow>, StoreError> {
    Ok(sqlx::query("SELECT * FROM promise ORDER BY created_at")
        .fetch_all(pool)
        .await?
        .into_iter()
        .filter_map(map_promise)
        .collect())
}

pub async fn set_promise_state(
    pool: &SqlitePool,
    uid: &str,
    state: PromiseState,
) -> Result<(), StoreError> {
    sqlx::query("UPDATE promise SET state = ?, updated_at = ? WHERE uid = ?")
        .bind(state.as_str())
        .bind(Utc::now().to_rfc3339())
        .bind(uid)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn promise_state(
    pool: &SqlitePool,
    uid: &str,
) -> Result<Option<PromiseState>, StoreError> {
    Ok(sqlx::query("SELECT state FROM promise WHERE uid = ?")
        .bind(uid)
        .fetch_optional(pool)
        .await?
        .and_then(|r| PromiseState::parse(&r.get::<String, _>("state"))))
}

// ------------------------------------------------------------------- effects

pub async fn queue_effect(
    pool: &SqlitePool,
    kind: &str,
    payload: &serde_json::Value,
    origin_uid: Option<&str>,
) -> Result<String, StoreError> {
    let uid = nucleus::new_uid("e");
    sqlx::query(
        "INSERT INTO effect_queue (uid, kind, payload, origin_uid, created_at)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&uid)
    .bind(kind)
    .bind(payload.to_string())
    .bind(origin_uid)
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(uid)
}

#[derive(Debug, Clone)]
pub struct EffectRow {
    pub uid: String,
    pub kind: String,
    pub payload: serde_json::Value,
    pub origin_uid: Option<String>,
}

pub async fn due_effects(pool: &SqlitePool) -> Result<Vec<EffectRow>, StoreError> {
    Ok(
        sqlx::query("SELECT * FROM effect_queue WHERE status = 'queued' ORDER BY rowid")
            .fetch_all(pool)
            .await?
            .into_iter()
            .filter_map(|r| {
                let payload: String = r.get("payload");
                Some(EffectRow {
                    uid: r.get("uid"),
                    kind: r.get("kind"),
                    payload: serde_json::from_str(&payload).ok()?,
                    origin_uid: r.get("origin_uid"),
                })
            })
            .collect(),
    )
}

pub async fn finish_effect(
    pool: &SqlitePool,
    uid: &str,
    ok: bool,
    result: &str,
) -> Result<(), StoreError> {
    sqlx::query(
        "UPDATE effect_queue SET status = ?, finished_at = ?, result = ?,
                attempts = attempts + 1 WHERE uid = ?",
    )
    .bind(if ok { "done" } else { "failed" })
    .bind(Utc::now().to_rfc3339())
    .bind(result)
    .bind(uid)
    .execute(pool)
    .await?;
    Ok(())
}

// ------------------------------------------------------------------- signals

/// Signals sample the world on their own schedule and land as facts
/// (blueprint VI.1). A signal is a record (kind='signal') with this sidecar.
pub struct NewSignal<'a> {
    pub slug: &'a str,
    pub head: &'a str,
    pub source_kind: &'a str, // command | http | sensor | query
    pub source: &'a str,
    pub schedule: &'a str, // duration literal: '90s', '5m', '1h', '1d'
}

pub async fn create_signal(pool: &SqlitePool, new: NewSignal<'_>) -> Result<String, StoreError> {
    let rec = records::create(
        pool,
        NewRecord {
            slug: Some(new.slug),
            kind: RecordKind::Signal,
            head: new.head,
            body: "",
            quantity: crate::exact::zero(), // quantity holds the last sampled value
        },
    )
    .await?;
    sqlx::query(
        "INSERT INTO signal (record_uid, source_kind, source, schedule) VALUES (?, ?, ?, ?)",
    )
    .bind(&rec.uid)
    .bind(new.source_kind)
    .bind(new.source)
    .bind(new.schedule)
    .execute(pool)
    .await?;
    Ok(rec.uid)
}

#[derive(Debug, Clone)]
pub struct SignalRow {
    pub record_uid: String,
    pub source_kind: String,
    pub source: String,
    pub schedule: String,
    pub last_sampled_at: Option<String>,
    pub current_value: f64,
}

pub async fn list_signals(pool: &SqlitePool) -> Result<Vec<SignalRow>, StoreError> {
    sqlx::query(
        "SELECT s.record_uid, s.source_kind, s.source, s.schedule, s.last_sampled_at, r.quantity_mantissa, r.quantity_scale
         FROM signal s JOIN record r ON r.uid = s.record_uid",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|r| {
        Ok(SignalRow {
            record_uid: r.get("record_uid"),
            source_kind: r.get("source_kind"),
            source: r.get("source"),
            schedule: r.get("schedule"),
            last_sampled_at: r.get("last_sampled_at"),
            // A sampled sensor reading is a float at its origin; this is a
            // display value, not a Ledger write.
            current_value: crate::exact::read_decimal(&r, "quantity")?.to_f64(),
        })
    })
    .collect()
}

pub async fn set_signal_sampled(
    pool: &SqlitePool,
    record_uid: &str,
    at_rfc3339: &str,
) -> Result<(), StoreError> {
    sqlx::query("UPDATE signal SET last_sampled_at = ? WHERE record_uid = ?")
        .bind(at_rfc3339)
        .bind(record_uid)
        .execute(pool)
        .await?;
    Ok(())
}

// ----------------------------------------------------------------- decisions

/// A decision is a record (kind='decision') plus its sidecar (blueprint XIII.1).
pub async fn create_decision(
    pool: &SqlitePool,
    subject_uid: &str,
    kind: &str,
    question: &str,
    options: &serde_json::Value,
) -> Result<String, StoreError> {
    let rec = records::create(
        pool,
        NewRecord {
            slug: None,
            kind: RecordKind::Decision,
            head: question,
            body: "",
            quantity: crate::exact::one(), // 1 = open; deciding sets it to 0 via a fact
        },
    )
    .await?;
    sqlx::query(
        "INSERT INTO decision (record_uid, subject_uid, kind, options) VALUES (?, ?, ?, ?)",
    )
    .bind(&rec.uid)
    .bind(subject_uid)
    .bind(kind)
    .bind(options.to_string())
    .execute(pool)
    .await?;
    Ok(rec.uid)
}

/// Create a decision with a deadline (blueprint XIII.1 `expires_at`); the
/// heartbeat closes it as 'expired' past that instant.
pub async fn create_decision_expiring(
    pool: &SqlitePool,
    subject_uid: &str,
    kind: &str,
    question: &str,
    options: &serde_json::Value,
    expires_at: &str,
) -> Result<String, StoreError> {
    let uid = create_decision(pool, subject_uid, kind, question, options).await?;
    sqlx::query("UPDATE decision SET expires_at = ? WHERE record_uid = ?")
        .bind(expires_at)
        .bind(&uid)
        .execute(pool)
        .await?;
    Ok(uid)
}

/// Open decisions whose deadline has passed.
pub async fn expired_open_decisions(
    pool: &SqlitePool,
    now_rfc3339: &str,
) -> Result<Vec<String>, StoreError> {
    Ok(sqlx::query(
        "SELECT d.record_uid FROM decision d
         JOIN record r ON r.uid = d.record_uid
         WHERE r.quantity_mantissa != '0' AND d.expires_at IS NOT NULL AND d.expires_at < ?",
    )
    .bind(now_rfc3339)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|r| r.get("record_uid"))
    .collect())
}

/// Notify effects delivered (not parked) since an instant — the budget meter.
pub async fn notifies_delivered_since(
    pool: &SqlitePool,
    since_rfc3339: &str,
) -> Result<i64, StoreError> {
    Ok(sqlx::query(
        "SELECT COUNT(1) AS n FROM effect_queue
         WHERE kind = 'notify' AND status = 'done'
           AND result NOT LIKE 'parked%' AND finished_at >= ?",
    )
    .bind(since_rfc3339)
    .fetch_one(pool)
    .await?
    .get("n"))
}

/// `(subject_uid, kind)` of every OPEN decision — the dedup key for sweeps
/// (senses drafts, crossings, expiry) so one situation asks only once.
pub async fn open_decision_subjects(
    pool: &SqlitePool,
) -> Result<std::collections::HashSet<(String, String)>, StoreError> {
    Ok(sqlx::query(
        "SELECT d.subject_uid, d.kind FROM decision d
         JOIN record r ON r.uid = d.record_uid WHERE r.quantity_mantissa != '0'",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|r| (r.get("subject_uid"), r.get("kind")))
    .collect())
}

pub async fn open_decisions(
    pool: &SqlitePool,
) -> Result<Vec<(String, String, String)>, StoreError> {
    Ok(sqlx::query(
        "SELECT d.record_uid, d.kind, r.head FROM decision d
         JOIN record r ON r.uid = d.record_uid WHERE r.quantity_mantissa != '0'",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|r| (r.get("record_uid"), r.get("kind"), r.get("head")))
    .collect())
}

#[derive(Debug, Clone)]
pub struct DecisionRow {
    pub record_uid: String,
    pub subject_uid: String,
    pub kind: String,
    pub question: String,
    pub options: serde_json::Value,
    pub open: bool,
    pub answer: Option<String>,
}

pub async fn list_decisions(pool: &SqlitePool) -> Result<Vec<DecisionRow>, StoreError> {
    Ok(sqlx::query(
        "SELECT d.record_uid, d.subject_uid, d.kind, d.options, d.answer, r.head, r.quantity_mantissa
         FROM decision d JOIN record r ON r.uid = d.record_uid ORDER BY r.created_at",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .filter_map(|r| {
        let options: String = r.get("options");
        Some(DecisionRow {
            record_uid: r.get("record_uid"),
            subject_uid: r.get("subject_uid"),
            kind: r.get("kind"),
            question: r.get("head"),
            options: serde_json::from_str(&options).ok()?,
            open: r.get::<String, _>("quantity_mantissa") != "0",
            answer: r.get("answer"),
        })
    })
    .collect())
}

pub async fn answer_decision(
    pool: &SqlitePool,
    record_uid: &str,
    answer: &str,
) -> Result<(), StoreError> {
    sqlx::query("UPDATE decision SET answer = ?, decided_at = ? WHERE record_uid = ?")
        .bind(answer)
        .bind(Utc::now().to_rfc3339())
        .bind(record_uid)
        .execute(pool)
        .await?;
    Ok(())
}
