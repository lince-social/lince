// non-lean-yet
// Scope: struct PromiseRow, struct NewPromise, fn insert_promise, fn expired_promises.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::needless_pass_by_value] Borrow eligible inputs after checking caller and
//   framework contracts. Sites 153.
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 147.
// - [review: §4 API] Check public bool parameters for a named mode or distinct operations. Sites
//   finish_effect:ok (405).
// - [Clippy: clippy::as_conversions] Use From or TryFrom when it preserves the conversion contract.
//   Sites 179, 228, 380, 666.
// - [Clippy: clippy::cast_sign_loss] Prove the sign or preserve behavior with an explicit
//   conversion. Sites 179.
// - [Clippy: clippy::collapsible_if] this `if` statement can be collapsed. Sites 46.
// - [Clippy: clippy::exhaustive_enums] Review whether public API evolution requires non_exhaustive.
//   Sites 326.
// - [Clippy: clippy::missing_errors_doc] State the observable failure cases for exported functions.
//   Sites 42, 86, 104, 117, 130, 190, 198, 220, 247, 264, 281, 292, 321, 332, 343, 396, 402, 433,
//   468, 490, 520, 537, 554, 569, 583, 608, 644, 670.
// - [rustc: missing_debug_implementations] type does not implement `std::fmt::Debug`; consider
//   adding `#[derive(Debug)]` or a manual implementation. Sites 207, 326, 425, 636.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 9, 10,
//   11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 29, 30, 31, 32, 33, 34, 35, 36, 37,
//   38, 39, 42, 86, 104, 117, 130, 190, 198, 207, 208, 209, 210, 211, 220, 247, 264, 281, 292, 314,
//   315, 316, 317, 318, 321, 326, 327, 328, 329, 332, 343, 396, 402, 425, 426, 427, 428, 429, 430,
//   433, 458, 459, 460, 461, 462, 463, 464, 465, 468, 490, 520, 537, 554, 569, 583, 598, 599, 600,
//   601, 602, 603, 604, 605, 608, 636, 637, 638, 639, 640, 641, 644, 670.

use nucleus::{
    PromiseState, RecordKind,
    transfer::{OpenPromiseReusePolicy, TransferLocationSnapshot},
};
use sqlx::{Row, SqlitePool};

use crate::{
    StoreError,
    records::{self, NewRecord},
};

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
    pub state: Option<PromiseState>,
    pub condition: Option<String>,
    pub transfer_uid: Option<String>,
    pub rule_uid: Option<String>,
    pub reserve_from: Option<String>,
}

pub async fn insert_promise(pool: &SqlitePool, p: NewPromise) -> Result<String, StoreError> {
    let uid = nucleus::new_uid("p");
    let now = nucleus::execution::now().to_rfc3339();
    let mut reserve_from = p.reserve_from;
    if reserve_from.is_none()
        && let Some(transfer_uid) = &p.transfer_uid
    {
        reserve_from = sqlx::query("SELECT reserve_default FROM transfer WHERE record_uid = ?")
            .bind(transfer_uid)
            .fetch_optional(pool)
            .await?
            .and_then(|r| r.get::<Option<String>, _>("reserve_default"));
    }
    let reserve_from = match reserve_from {
        Some(value) => value,
        None => crate::config::transfer_reservation_default(pool).await?,
    };
    let mut tx = crate::write_tx(pool).await?;
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
    .execute(&mut *tx)
    .await?;
    crate::transfer_stock::validate_all_on(&mut tx).await?;
    tx.commit().await?;
    Ok(uid)
}

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
    let mut tx = crate::write_tx(pool).await?;
    sqlx::query("UPDATE promise SET delta = ?, updated_at = ? WHERE uid = ?")
        .bind(delta)
        .bind(nucleus::execution::now().to_rfc3339())
        .bind(uid)
        .execute(&mut *tx)
        .await?;
    crate::transfer_stock::validate_all_on(&mut tx).await?;
    tx.commit().await?;
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

pub async fn promises_for_records(
    pool: &SqlitePool,
    records: &[&str],
) -> Result<std::collections::HashMap<String, Vec<PromiseRow>>, StoreError> {
    let mut output = std::collections::HashMap::<String, Vec<PromiseRow>>::new();
    if records.is_empty() {
        return Ok(output);
    }
    let selected =
        serde_json::to_string(records).map_err(|error| StoreError::Protocol(error.to_string()))?;
    let rows =
        sqlx::query("SELECT * FROM promise WHERE record_uid IN (SELECT value FROM json_each(?))")
            .bind(selected)
            .fetch_all(pool)
            .await?;
    for promise in rows.into_iter().filter_map(map_promise) {
        if let Some(record) = &promise.record_uid {
            output.entry(record.clone()).or_default().push(promise);
        }
    }
    Ok(output)
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

pub struct PromiseProjection {
    pub record_uid: Option<String>,
    pub delta: f64,
    pub window_end: Option<String>,
    pub state: PromiseState,
}

fn admitted_promise_state(state: &str, row: &sqlx::sqlite::SqliteRow) -> Option<PromiseState> {
    let state = PromiseState::parse(state)?;
    OpenPromiseReusePolicy::parse(row.get("open_reuse_policy"))?;
    Some(state)
}

pub async fn promise_projections(pool: &SqlitePool) -> Result<Vec<PromiseProjection>, StoreError> {
    let _read = nucleus::performance::Span::new("promise-projection-read-await");
    let rows = sqlx::query(
        "SELECT record_uid, delta, window_end, state, open_reuse_policy
         FROM promise ORDER BY created_at",
    )
    .fetch_all(pool)
    .await?;
    nucleus::performance::add("promise-projection-rows", rows.len() as u64);
    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let state = row.get("state");
            let record_uid = row.get("record_uid");
            let delta = row.get("delta");
            let window_end = row.get("window_end");
            let state = admitted_promise_state(state, &row)?;
            Some(PromiseProjection {
                record_uid,
                delta,
                window_end,
                state,
            })
        })
        .collect())
}

pub async fn promise_party(
    pool: &SqlitePool,
    uid: &str,
) -> Result<Option<Option<String>>, StoreError> {
    Ok(
        sqlx::query("SELECT party_uid, state, open_reuse_policy FROM promise WHERE uid = ?")
            .bind(uid)
            .fetch_optional(pool)
            .await?
            .and_then(|row| {
                let state = row.get("state");
                let party = row.get("party_uid");
                admitted_promise_state(state, &row).map(|_| party)
            }),
    )
}

pub async fn set_promise_state(
    pool: &SqlitePool,
    uid: &str,
    state: PromiseState,
) -> Result<(), StoreError> {
    let mut tx = crate::write_tx(pool).await?;
    sqlx::query("UPDATE promise SET state = ?, updated_at = ? WHERE uid = ?")
        .bind(state.as_str())
        .bind(nucleus::execution::now().to_rfc3339())
        .bind(uid)
        .execute(&mut *tx)
        .await?;
    crate::transfer_stock::validate_all_on(&mut tx).await?;
    tx.commit().await?;
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
    .bind(nucleus::execution::now().to_rfc3339())
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
    due_effects_for_lane(pool, None).await
}

#[derive(Clone, Copy)]
pub enum EffectLane {
    Actions,
    Commands,
    UnlimitedCommands,
}

pub async fn due_effects_for_lane(
    pool: &SqlitePool,
    lane: Option<EffectLane>,
) -> Result<Vec<EffectRow>, StoreError> {
    Ok(due_effect_rows(pool, lane)
        .await?
        .iter()
        .filter_map(effect_row)
        .collect())
}

pub async fn first_due_effect(pool: &SqlitePool) -> Result<Option<EffectRow>, StoreError> {
    Ok(due_effect_rows(pool, None)
        .await?
        .iter()
        .find_map(effect_row))
}

async fn due_effect_rows(
    pool: &SqlitePool,
    lane: Option<EffectLane>,
) -> Result<Vec<sqlx::sqlite::SqliteRow>, StoreError> {
    let _read = nucleus::performance::Span::new("effect-queue-read-await");
    let lane = lane.map(|lane| match lane {
        EffectLane::Actions => 0,
        EffectLane::Commands => 1,
        EffectLane::UnlimitedCommands => 2,
    });
    let rows = sqlx::query(
        "SELECT effect.uid, effect.kind, effect.payload, effect.origin_uid
         FROM effect_queue effect
         WHERE status = 'queued'
           AND (?1 IS NULL OR ?1 = CASE
               WHEN kind = 'saved-command' AND COALESCE(json_extract(payload, '$.unlimited'),
                   json_extract(payload, '$.command_snapshot.unlimited'), 0) = 1 THEN 2
               WHEN kind IN ('command', 'signal', 'saved-command') THEN 1
               ELSE 0 END)
           AND (?1 IS NULL OR NOT EXISTS (
               SELECT 1 FROM effect_queue earlier
               WHERE earlier.rowid < effect.rowid AND earlier.status IN ('queued', 'running')
                 AND json_extract(earlier.payload, '$.rule') = json_extract(effect.payload, '$.rule')
                 AND json_extract(earlier.payload, '$.revision') = json_extract(effect.payload, '$.revision')
                 AND json_extract(earlier.payload, '$.occurrence.event_id') = json_extract(effect.payload, '$.occurrence.event_id')))
         ORDER BY effect.rowid LIMIT 64",
    )
        .bind(lane)
        .fetch_all(pool)
        .await?;
    nucleus::performance::add("effect-queue-rows-read", rows.len() as u64);
    Ok(rows)
}

fn effect_row(row: &sqlx::sqlite::SqliteRow) -> Option<EffectRow> {
    let _decode = nucleus::performance::Span::new("effect-queue-payload-decode");
    nucleus::performance::add("effect-queue-payload-decodes", 1);
    let payload: &str = row.get("payload");
    Some(EffectRow {
        uid: row.get("uid"),
        kind: row.get("kind"),
        payload: serde_json::from_str(payload).ok()?,
        origin_uid: row.get("origin_uid"),
    })
}

pub async fn claim_effect(pool: &SqlitePool, uid: &str) -> Result<bool, StoreError> {
    let claim = sqlx::query("UPDATE effect_queue SET status = 'running' WHERE uid = ? AND status = 'queued' AND NOT EXISTS (SELECT 1 FROM effect_queue earlier WHERE earlier.rowid < effect_queue.rowid AND earlier.status IN ('queued', 'running') AND json_extract(earlier.payload, '$.rule') = json_extract(effect_queue.payload, '$.rule') AND json_extract(earlier.payload, '$.revision') = json_extract(effect_queue.payload, '$.revision') AND json_extract(earlier.payload, '$.occurrence.event_id') = json_extract(effect_queue.payload, '$.occurrence.event_id'))")
        .bind(uid).execute(pool).await?;
    Ok(claim.rows_affected() != 0)
}

pub async fn finish_effect(
    pool: &SqlitePool,
    uid: &str,
    ok: bool,
    result: &str,
) -> Result<(), StoreError> {
    let mut tx = crate::write_tx(pool).await?;
    sqlx::query(
        "UPDATE effect_queue SET status = ?, finished_at = ?, result = ?,
                attempts = attempts + 1 WHERE uid = ?",
    )
    .bind(if ok { "done" } else { "failed" })
    .bind(nucleus::execution::now().to_rfc3339())
    .bind(result)
    .bind(uid)
    .execute(&mut *tx)
    .await?;
    sqlx::query("INSERT INTO karma_effect_outcome(effect_uid, attempt, status, result, at) SELECT uid, attempts, status, result, finished_at FROM effect_queue WHERE uid = ?")
        .bind(uid).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

pub struct NewSignal<'a> {
    pub slug: &'a str,
    pub head: &'a str,
    pub source_kind: &'a str,
    pub source: &'a str,
    pub schedule: &'a str,
}

pub async fn create_signal(pool: &SqlitePool, new: NewSignal<'_>) -> Result<String, StoreError> {
    let rec = records::create(
        pool,
        NewRecord {
            slug: Some(new.slug),
            kind: RecordKind::Signal,
            head: new.head,
            body: "",
            quantity: crate::exact::zero(),
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
    pub actor_uid: Option<String>,
    pub source_kind: String,
    pub source: String,
    pub schedule: String,
    pub last_sampled_at: Option<String>,
    pub current_value: f64,
}

pub async fn list_signals(pool: &SqlitePool) -> Result<Vec<SignalRow>, StoreError> {
    sqlx::query(
        "SELECT s.record_uid, s.actor_uid, s.source_kind, s.source, s.schedule, s.last_sampled_at, r.quantity_mantissa, r.quantity_scale
         FROM signal s JOIN record r ON r.uid = s.record_uid WHERE r.deleted_at IS NULL AND NOT EXISTS (SELECT 1 FROM signal_stream v WHERE v.signal_uid = s.record_uid)",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|r| {
        Ok(SignalRow {
            record_uid: r.get("record_uid"),
            actor_uid: r.get("actor_uid"),
            source_kind: r.get("source_kind"),
            source: r.get("source"),
            schedule: r.get("schedule"),
            last_sampled_at: r.get("last_sampled_at"),
            current_value: crate::exact::read_decimal(&r, "quantity")?.to_f64(),
        })
    })
    .collect()
}

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
            quantity: crate::exact::one(),
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
        let options = decision_options(&r)?;
        Some(DecisionRow {
            record_uid: r.get("record_uid"),
            subject_uid: r.get("subject_uid"),
            kind: r.get("kind"),
            question: r.get("head"),
            options,
            open: r.get::<&str, _>("quantity_mantissa") != "0",
            answer: r.get("answer"),
        })
    })
    .collect())
}

fn decision_options(row: &sqlx::sqlite::SqliteRow) -> Option<serde_json::Value> {
    let raw: &str = row.get("options");
    serde_json::from_str(raw).ok()
}

pub struct OpenDecision {
    pub record_uid: String,
    pub subject_uid: String,
    pub kind: String,
    pub question: String,
    pub options: serde_json::Value,
}

pub async fn open_decision_rows(pool: &SqlitePool) -> Result<Vec<OpenDecision>, StoreError> {
    let _read = nucleus::performance::Span::new("store-open-decision-presentation-await");
    let rows = sqlx::query(
        "SELECT d.record_uid, d.subject_uid, d.kind, d.options, r.head
         FROM decision d JOIN record r ON r.uid = d.record_uid
         WHERE r.quantity_mantissa <> '0' ORDER BY r.created_at",
    )
    .fetch_all(pool)
    .await?;
    let output: Vec<_> = rows
        .into_iter()
        .filter_map(|row| {
            let options = decision_options(&row)?;
            Some(OpenDecision {
                record_uid: row.get("record_uid"),
                subject_uid: row.get("subject_uid"),
                kind: row.get("kind"),
                question: row.get("head"),
                options,
            })
        })
        .collect();
    nucleus::performance::add("open-decision-presentation-rows", output.len() as u64);
    Ok(output)
}

pub async fn answer_decision(
    pool: &SqlitePool,
    record_uid: &str,
    answer: &str,
) -> Result<(), StoreError> {
    sqlx::query("UPDATE decision SET answer = ?, decided_at = ? WHERE record_uid = ?")
        .bind(answer)
        .bind(nucleus::execution::now().to_rfc3339())
        .bind(record_uid)
        .execute(pool)
        .await?;
    Ok(())
}
