//! Fact repository (blueprint Part II).

use chrono::{DateTime, SecondsFormat, TimeDelta, Utc};
use nucleus::{Cause, CauseKind, DecimalValue, Fact};
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

use crate::StoreError;
use crate::exact::{decimal_columns, read_decimal, zero};

/// Render an instant for the `fact.at` column and for every comparison against
/// it (blueprint E0).
///
/// `at` is TEXT compared lexically, which equals chronological order only when
/// every value shares one format. Two things break that: a `+00:00` suffix
/// sorting against a `Z` one, and variable-length fractional seconds. This is
/// fixed-width UTC with `Z`, so the two orders coincide — and a wrong answer
/// here would look exactly like a correct one, which is why it is centralised
/// rather than spelled out at each call site.
///
/// Nanosecond precision is kept deliberately. The Fact's hash preimage is built
/// from the parsed `DateTime`, so truncating here would change the instant a
/// re-read Fact reconstructs and silently break `verify_chain_step`.
pub fn instant(at: DateTime<Utc>) -> String {
    at.to_rfc3339_opts(SecondsFormat::Nanos, true)
}

/// Re-render a caller-supplied timestamp into the stored format. A caller that
/// passes `+00:00` would otherwise compare wrong against `Z`-suffixed rows.
fn instant_str(value: &str) -> String {
    DateTime::parse_from_rfc3339(value)
        .map(|parsed| instant(parsed.with_timezone(&Utc)))
        .unwrap_or_else(|_| value.to_string())
}

pub async fn exists(tx: &mut Transaction<'_, Sqlite>, uid: &str) -> Result<bool, StoreError> {
    Ok(sqlx::query("SELECT 1 FROM fact WHERE uid = ?")
        .bind(uid)
        .fetch_optional(&mut **tx)
        .await?
        .is_some())
}

/// Head of the Cell's hash chain; "genesis" for an empty ledger.
pub async fn last_hash(tx: &mut Transaction<'_, Sqlite>) -> Result<String, StoreError> {
    Ok(
        sqlx::query("SELECT hash FROM fact ORDER BY rowid DESC LIMIT 1")
            .fetch_optional(&mut **tx)
            .await?
            .map(|r| r.get::<String, _>("hash"))
            .unwrap_or_else(|| "genesis".to_string()),
    )
}

pub async fn insert(tx: &mut Transaction<'_, Sqlite>, f: &Fact) -> Result<(), StoreError> {
    let (mantissa, scale) = decimal_columns(f.delta);
    sqlx::query(
        "INSERT INTO fact (uid, record_uid, delta_mantissa, delta_scale, at, actor_uid,
                           cause_kind, cause_uid, payload, prev_hash, hash, signature)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&f.uid)
    .bind(&f.record_uid)
    .bind(mantissa)
    .bind(scale)
    .bind(instant(f.at))
    .bind(&f.actor_uid)
    .bind(f.cause.kind.as_str())
    .bind(&f.cause.uid)
    .bind(&f.payload)
    .bind(&f.prev_hash)
    .bind(&f.hash)
    .bind(&f.signature)
    .execute(&mut **tx)
    .await?;
    // Facts join the op log (Ontology §11) — except imported ones, whose
    // original op the sync import path appends under its origin identity.
    if f.cause.kind != CauseKind::Sync {
        crate::sync_ops::log_local_tx(tx, "fact", &f.uid, "", crate::sync_ops::OpKind::Fact, None)
            .await?;
    }
    Ok(())
}

fn map_fact(r: sqlx::sqlite::SqliteRow) -> Result<Fact, StoreError> {
    let at: String = r.get("at");
    let cause_kind: String = r.get("cause_kind");
    Ok(Fact {
        uid: r.get("uid"),
        record_uid: r.get("record_uid"),
        delta: read_decimal(&r, "delta")?,
        at: DateTime::parse_from_rfc3339(&at)
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        actor_uid: r.get("actor_uid"),
        cause: Cause {
            kind: CauseKind::parse(&cause_kind).unwrap_or(CauseKind::UserEdit),
            uid: r.get("cause_uid"),
        },
        payload: r.get("payload"),
        prev_hash: r.get("prev_hash"),
        hash: r.get("hash"),
        signature: r.get("signature"),
    })
}

fn map_facts(rows: Vec<sqlx::sqlite::SqliteRow>) -> Result<Vec<Fact>, StoreError> {
    rows.into_iter().map(map_fact).collect()
}

/// Fetch a single sealed fact by uid (the target of a compensation/undo).
pub async fn get(pool: &SqlitePool, uid: &str) -> Result<Option<Fact>, StoreError> {
    sqlx::query("SELECT * FROM fact WHERE uid = ?")
        .bind(uid)
        .fetch_optional(pool)
        .await?
        .map(map_fact)
        .transpose()
}

pub async fn get_in_transaction(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
) -> Result<Option<Fact>, StoreError> {
    sqlx::query("SELECT * FROM fact WHERE uid = ?")
        .bind(uid)
        .fetch_optional(&mut **tx)
        .await?
        .map(map_fact)
        .transpose()
}

/// The actor_uid of a record's EARLIEST fact (rowid order — creation, not the
/// latest edit). Used for "who created this" (record ownership checks,
/// message/thread sender resolution) — a record's own row carries no creator
/// column, only its facts do.
pub async fn creator_uid(
    pool: &SqlitePool,
    record_uid: &str,
) -> Result<Option<String>, StoreError> {
    Ok(
        sqlx::query("SELECT actor_uid FROM fact WHERE record_uid = ? ORDER BY rowid ASC LIMIT 1")
            .bind(record_uid)
            .fetch_optional(pool)
            .await?
            .and_then(|r| r.get::<Option<String>, _>("actor_uid")),
    )
}

pub async fn for_record(
    pool: &SqlitePool,
    record_uid: &str,
    limit: i64,
) -> Result<Vec<Fact>, StoreError> {
    map_facts(
        sqlx::query("SELECT * FROM fact WHERE record_uid = ? ORDER BY rowid DESC LIMIT ?")
            .bind(record_uid)
            .bind(limit)
            .fetch_all(pool)
            .await?,
    )
}

/// Which deltas of a window a sum should keep. Sign is a Rust-side test now: a
/// canonical mantissa is TEXT, so `delta > 0` is no longer a SQL predicate.
/// The `(record_uid, at)` index still bounds the scan to the window itself.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SignFilter {
    All,
    Positive,
    Negative,
}

impl SignFilter {
    fn keeps(self, value: DecimalValue) -> bool {
        match self {
            Self::All => true,
            Self::Positive => value.is_positive(),
            Self::Negative => value.is_negative(),
        }
    }
}

/// Fold the deltas of `[start, end)` exactly, in Rust, at the finest scale any
/// of them declares.
async fn fold_window(
    pool: &SqlitePool,
    record_uid: &str,
    start: &str,
    end: Option<&str>,
    filter: SignFilter,
) -> Result<DecimalValue, StoreError> {
    let rows = match end {
        Some(end) => {
            sqlx::query(
                "SELECT delta_mantissa, delta_scale FROM fact
                  WHERE record_uid = ? AND at >= ? AND at < ?",
            )
            .bind(record_uid)
            .bind(start)
            .bind(end)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query(
                "SELECT delta_mantissa, delta_scale FROM fact
                  WHERE record_uid = ? AND at >= ?",
            )
            .bind(record_uid)
            .bind(start)
            .fetch_all(pool)
            .await?
        }
    };
    let mut total = zero();
    for row in rows {
        let delta = read_decimal(&row, "delta")?;
        if filter.keeps(delta) {
            total = total.aligned_add(delta).ok_or_else(|| {
                StoreError::Decode("quantity sum overflows i128".to_string().into())
            })?;
        }
    }
    Ok(total)
}

/// `sum(@x, <window>)` (blueprint II.1): net delta over the trailing window.
pub async fn sum_window(
    pool: &SqlitePool,
    record_uid: &str,
    window_secs: i64,
    now: DateTime<Utc>,
) -> Result<DecimalValue, StoreError> {
    let cutoff = instant(now - TimeDelta::seconds(window_secs));
    fold_window(pool, record_uid, &cutoff, None, SignFilter::All).await
}

/// `sum_pos(@x, <window>)`: only the inflows (positive deltas) of the window.
pub async fn sum_pos_window(
    pool: &SqlitePool,
    record_uid: &str,
    window_secs: i64,
    now: DateTime<Utc>,
) -> Result<DecimalValue, StoreError> {
    let cutoff = instant(now - TimeDelta::seconds(window_secs));
    fold_window(pool, record_uid, &cutoff, None, SignFilter::Positive).await
}

/// `sum_neg(@x, <window>)`: only the outflows (negative deltas); the sum is
/// returned as-is (a negative number, or 0 when there were none).
pub async fn sum_neg_window(
    pool: &SqlitePool,
    record_uid: &str,
    window_secs: i64,
    now: DateTime<Utc>,
) -> Result<DecimalValue, StoreError> {
    let cutoff = instant(now - TimeDelta::seconds(window_secs));
    fold_window(pool, record_uid, &cutoff, None, SignFilter::Negative).await
}

/// End-lagged window: net delta over `[now - end_lag - window, now - end_lag)`.
/// `end_lag = 0` degenerates to `sum_window` (with an exclusive upper bound).
pub async fn sum_window_lagged(
    pool: &SqlitePool,
    record_uid: &str,
    window_secs: i64,
    end_lag_secs: i64,
    now: DateTime<Utc>,
) -> Result<DecimalValue, StoreError> {
    let end = now - TimeDelta::seconds(end_lag_secs);
    let start = instant(end - TimeDelta::seconds(window_secs));
    let end = instant(end);
    fold_window(pool, record_uid, &start, Some(&end), SignFilter::All).await
}

/// A Record's exact level, folded from its Fact chain (blueprint E0.1) rather
/// than read from the `record.quantity` cache. This is what a Program reads:
/// the cache is a cache, and a rule that decides something should decide it
/// from the truth.
///
/// Anchored on the last checkpoint that carries a level. Retention genuinely
/// deletes archived Facts, so folding whatever rows remain would silently
/// under-report a compacted Record — the checkpoint already accounts for
/// everything before it, and only the Facts after it still need adding.
pub async fn level(pool: &SqlitePool, record_uid: &str) -> Result<DecimalValue, StoreError> {
    let (base, after_rowid) = level_anchor(pool, record_uid).await?;
    let rows = match after_rowid {
        Some(rowid) => {
            sqlx::query(
                "SELECT delta_mantissa, delta_scale FROM fact
                  WHERE record_uid = ? AND rowid > ?",
            )
            .bind(record_uid)
            .bind(rowid)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query("SELECT delta_mantissa, delta_scale FROM fact WHERE record_uid = ?")
                .bind(record_uid)
                .fetch_all(pool)
                .await?
        }
    };
    let mut total = base;
    for row in rows {
        let delta = read_decimal(&row, "delta")?;
        total = total.aligned_add(delta).ok_or_else(|| {
            StoreError::Decode(format!("level of {record_uid} overflows i128").into())
        })?;
    }
    Ok(total)
}

/// The most recent checkpoint that actually carries a level, and its rowid.
/// Compaction's archive anchors are checkpoints too but carry `{archive, ...}`
/// instead of a level, so they are skipped rather than read as zero.
async fn level_anchor(
    pool: &SqlitePool,
    record_uid: &str,
) -> Result<(DecimalValue, Option<i64>), StoreError> {
    let rows = sqlx::query(
        "SELECT rowid, payload FROM fact
          WHERE record_uid = ? AND cause_kind = 'checkpoint'
          ORDER BY rowid DESC LIMIT 32",
    )
    .bind(record_uid)
    .fetch_all(pool)
    .await?;
    for row in rows {
        let Some(payload) = row.get::<Option<String>, _>("payload") else {
            continue;
        };
        let Ok(json) = serde_json::from_str::<serde_json::Value>(&payload) else {
            continue;
        };
        let Some(text) = json.get("level").and_then(serde_json::Value::as_str) else {
            continue; // an archive anchor, not a level checkpoint
        };
        let level = DecimalValue::parse_inferred(text).map_err(|error| {
            StoreError::Decode(format!("checkpoint level is not an exact decimal: {error}").into())
        })?;
        return Ok((level, Some(row.get::<i64, _>("rowid"))));
    }
    Ok((zero(), None))
}

/// Set (upsert) the retention horizon for a record kind (blueprint II.2).
pub async fn set_retention(
    pool: &SqlitePool,
    kind: &str,
    horizon_seconds: i64,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO retention_policy (kind, horizon_seconds) VALUES (?, ?)
         ON CONFLICT(kind) DO UPDATE SET horizon_seconds = excluded.horizon_seconds",
    )
    .bind(kind)
    .bind(horizon_seconds)
    .execute(pool)
    .await?;
    Ok(())
}

/// All retention policies as `(kind, horizon_seconds)`.
pub async fn retention_policies(pool: &SqlitePool) -> Result<Vec<(String, i64)>, StoreError> {
    Ok(
        sqlx::query("SELECT kind, horizon_seconds FROM retention_policy")
            .fetch_all(pool)
            .await?
            .into_iter()
            .map(|r| (r.get("kind"), r.get("horizon_seconds")))
            .collect(),
    )
}

/// The record's most recent checkpoint fact and its rowid (compaction bound).
pub async fn last_checkpoint(
    pool: &SqlitePool,
    record_uid: &str,
) -> Result<Option<(i64, Fact)>, StoreError> {
    sqlx::query(
        "SELECT rowid, * FROM fact
          WHERE record_uid = ? AND cause_kind = 'checkpoint'
          ORDER BY rowid DESC LIMIT 1",
    )
    .bind(record_uid)
    .fetch_optional(pool)
    .await?
    .map(|r| Ok((r.get::<i64, _>("rowid"), map_fact(r)?)))
    .transpose()
}

/// Facts of a record eligible for compaction: strictly before the checkpoint
/// row AND older than the cutoff time. Ordered by rowid (chain order).
pub async fn archivable_before(
    pool: &SqlitePool,
    record_uid: &str,
    checkpoint_rowid: i64,
    cutoff: DateTime<Utc>,
) -> Result<Vec<Fact>, StoreError> {
    map_facts(
        sqlx::query(
            "SELECT * FROM fact
              WHERE record_uid = ? AND rowid < ? AND at < ?
              ORDER BY rowid",
        )
        .bind(record_uid)
        .bind(checkpoint_rowid)
        .bind(instant(cutoff))
        .fetch_all(pool)
        .await?,
    )
}

/// Delete archived facts by uid, in one transaction. The quantity cache is
/// untouched on purpose: the deltas live on, folded into the checkpoint level.
pub async fn delete_by_uids(pool: &SqlitePool, uids: &[String]) -> Result<u64, StoreError> {
    let mut tx = crate::write_tx(pool).await?;
    let mut deleted = 0;
    for uid in uids {
        deleted += sqlx::query("DELETE FROM fact WHERE uid = ?")
            .bind(uid)
            .execute(&mut *tx)
            .await?
            .rows_affected();
    }
    tx.commit().await?;
    Ok(deleted)
}

/// Facts across all records, optionally since a cutoff, in chain order —
/// the `source: fact` Protein's raw feed.
pub async fn list_since(
    pool: &SqlitePool,
    since_rfc3339: Option<&str>,
    limit: i64,
) -> Result<Vec<Fact>, StoreError> {
    let rows = match since_rfc3339 {
        Some(since) => {
            sqlx::query("SELECT * FROM fact WHERE at >= ? ORDER BY rowid LIMIT ?")
                .bind(instant_str(since))
                .bind(limit)
                .fetch_all(pool)
                .await?
        }
        None => {
            sqlx::query("SELECT * FROM fact ORDER BY rowid LIMIT ?")
                .bind(limit)
                .fetch_all(pool)
                .await?
        }
    };
    map_facts(rows)
}

/// Hours since the most recent fact on a record; None when it has none.
pub async fn hours_since_last(
    pool: &SqlitePool,
    record_uid: &str,
    now: DateTime<Utc>,
) -> Result<Option<f64>, StoreError> {
    let last: Option<String> =
        sqlx::query("SELECT at FROM fact WHERE record_uid = ? ORDER BY rowid DESC LIMIT 1")
            .bind(record_uid)
            .fetch_optional(pool)
            .await?
            .map(|r| r.get("at"));
    Ok(last.and_then(|at| {
        DateTime::parse_from_rfc3339(&at)
            .ok()
            .map(|d| (now - d.with_timezone(&Utc)).num_seconds() as f64 / 3600.0)
    }))
}
