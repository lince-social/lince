//! Reading the Ledger in aggregate: classification, windowed totals, levels.
//!
//! Two things live here because they are one concern — what a change *was*, and
//! summing changes by that. Splitting them would only make the sums import the
//! classification on every query for a boundary nobody needs.
//!
//! None of it is domain-specific. "What did I spend on food in March", "how much
//! flour did I use last week", and "how many hours went to this project" are the
//! same query over the same primitives: signed deltas, classified by concept,
//! bounded by a half-open window. A sand supplies the vocabulary and the
//! question; this module supplies the arithmetic.
//!
//! Totals are a **query over classified Facts**, never a sum over the quantities
//! of "total Records". `Rent = 1000` is a standing parameter a rule reads; the
//! monthly `-1000` on savings classified `@rent` is what a total sums. Keeping
//! that distinction is what stops an aggregate and a computed `Total` Record
//! from becoming two overlapping numbers on one screen.
//!
//! Nothing here creates accounts, categories, postings, or a second balance
//! truth. The Fact chain is the balance; this module only classifies and reads.

use std::collections::{BTreeMap, HashSet};

use chrono::{DateTime, Utc};
use nucleus::DecimalValue;
use sqlx::{Row, SqlitePool};

use crate::StoreError;
use crate::exact::zero;
use crate::facts::instant;

// ------------------------------------------------------- a Record's concepts

/// Add a concept a Record *counts as*. Its identity concept
/// (`record.concept_uid`) is untouched: a toothbrush stays a toothbrush while
/// also counting as a cost and a health item.
pub async fn add_record_concept(
    pool: &SqlitePool,
    record_uid: &str,
    concept_uid: &str,
    actor_uid: Option<&str>,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO record_concept (record_uid, concept_uid, at, actor_uid)
         VALUES (?, ?, ?, ?)
         ON CONFLICT(record_uid, concept_uid) DO NOTHING",
    )
    .bind(record_uid)
    .bind(concept_uid)
    .bind(instant(Utc::now()))
    .bind(actor_uid)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn remove_record_concept(
    pool: &SqlitePool,
    record_uid: &str,
    concept_uid: &str,
) -> Result<bool, StoreError> {
    let result =
        sqlx::query("DELETE FROM record_concept WHERE record_uid = ? AND concept_uid = ?")
            .bind(record_uid)
            .bind(concept_uid)
            .execute(pool)
            .await?;
    Ok(result.rows_affected() > 0)
}

/// Every concept a Record carries: its identity concept first, then the ones it
/// counts as. Callers that mean "what is this" want the first; callers that
/// mean "what does this answer for" want all of them.
pub async fn record_concepts(
    pool: &SqlitePool,
    record_uid: &str,
) -> Result<Vec<String>, StoreError> {
    let mut out = Vec::new();
    if let Some(record) = crate::records::get(pool, record_uid).await? {
        if let Some(identity) = record.concept_uid {
            out.push(identity);
        }
    }
    let rows = sqlx::query(
        "SELECT concept_uid FROM record_concept WHERE record_uid = ? ORDER BY at, concept_uid",
    )
    .bind(record_uid)
    .fetch_all(pool)
    .await?;
    for row in rows {
        let concept: String = row.get("concept_uid");
        if !out.contains(&concept) {
            out.push(concept);
        }
    }
    Ok(out)
}

/// Every "counts as" edge in one query, for callers that need the whole map.
///
/// The per-Record `record_concepts` is two queries; calling it once per Record
/// while scanning the Ledger makes a read that should be one pass into `2N`
/// round trips. Aggregation loads this once and merges it with the identity
/// concepts it already has.
pub async fn all_record_concepts(
    pool: &SqlitePool,
) -> Result<BTreeMap<String, Vec<String>>, StoreError> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let rows = sqlx::query(
        "SELECT record_uid, concept_uid FROM record_concept ORDER BY record_uid, at, concept_uid",
    )
    .fetch_all(pool)
    .await?;
    for row in rows {
        out.entry(row.get("record_uid"))
            .or_default()
            .push(row.get("concept_uid"));
    }
    Ok(out)
}

/// Records answering for `concept_uid`, following the DAG downward — asking for
/// `@cost` reaches every `@food`, because `concept_parent` is a many-to-many
/// edge table and `@food` may sit under both `@substance` and `@cost`.
///
/// Reads the union of the identity column and the join table, so a Record
/// classified either way appears exactly once.
pub async fn records_with_concept(
    pool: &SqlitePool,
    concept_uid: &str,
) -> Result<Vec<String>, StoreError> {
    let concepts = crate::concepts::descendants_including(pool, concept_uid).await?;
    if concepts.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = placeholders(concepts.len());
    let sql = format!(
        "SELECT uid FROM record
          WHERE deleted_at IS NULL AND concept_uid IN ({placeholders})
         UNION
         SELECT rc.record_uid AS uid FROM record_concept rc
           JOIN record r ON r.uid = rc.record_uid
          WHERE r.deleted_at IS NULL AND rc.concept_uid IN ({placeholders})"
    );
    let mut query = sqlx::query(&sql);
    for _ in 0..2 {
        for concept in &concepts {
            query = query.bind(concept.clone());
        }
    }
    Ok(query
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| row.get::<String, _>("uid"))
        .collect())
}

// -------------------------------------------------- a Fact's classification

/// Assert what a change *was*. Append-only: correcting a mistagged change is
/// a new assertion over the same Fact, never a compensating Fact — the quantity
/// never moved, only our account of what the change meant.
///
/// `concept_uid: None` is an explicit "unclassify", which is different from
/// never having classified the Fact.
pub async fn classify_fact(
    pool: &SqlitePool,
    fact_uid: &str,
    concept_uid: Option<&str>,
    actor_uid: Option<&str>,
    note: Option<&str>,
) -> Result<String, StoreError> {
    let uid = nucleus::new_uid("fc");
    let at = instant(Utc::now());
    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO fact_concept_event (uid, fact_uid, concept_uid, actor_uid, note, at)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&uid)
    .bind(fact_uid)
    .bind(concept_uid)
    .bind(actor_uid)
    .bind(note)
    .bind(&at)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO fact_concept (fact_uid, concept_uid, event_uid, at)
         VALUES (?, ?, ?, ?)
         ON CONFLICT(fact_uid) DO UPDATE
           SET concept_uid = excluded.concept_uid,
               event_uid = excluded.event_uid,
               at = excluded.at",
    )
    .bind(fact_uid)
    .bind(concept_uid)
    .bind(&uid)
    .bind(&at)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(uid)
}

/// The concept currently asserted for a change, if any.
pub async fn fact_concept(
    pool: &SqlitePool,
    fact_uid: &str,
) -> Result<Option<String>, StoreError> {
    Ok(
        sqlx::query("SELECT concept_uid FROM fact_concept WHERE fact_uid = ?")
            .bind(fact_uid)
            .fetch_optional(pool)
            .await?
            .and_then(|row| row.get::<Option<String>, _>("concept_uid")),
    )
}

/// Every current classification in one query, for the same reason as
/// [`all_record_concepts`]: a scan that asks per Fact turns one pass into `N`
/// round trips. Only classified Facts appear; an absent key means unclassified.
pub async fn all_fact_concepts(
    pool: &SqlitePool,
) -> Result<BTreeMap<String, String>, StoreError> {
    let rows = sqlx::query("SELECT fact_uid, concept_uid FROM fact_concept")
        .fetch_all(pool)
        .await?;
    let mut out = BTreeMap::new();
    for row in rows {
        // A NULL concept is an explicit "unclassify" assertion, which reads the
        // same as never having been classified for aggregation purposes.
        if let Some(concept) = row.get::<Option<String>, _>("concept_uid") {
            out.insert(row.get::<String, _>("fact_uid"), concept);
        }
    }
    Ok(out)
}

#[derive(Debug, Clone)]
pub struct ClassificationEvent {
    pub uid: String,
    pub concept_uid: Option<String>,
    pub actor_uid: Option<String>,
    pub note: Option<String>,
    pub at: String,
}

/// Every assertion ever made about this change, oldest first — the audit
/// trail that makes a re-tag safe to do.
pub async fn classification_history(
    pool: &SqlitePool,
    fact_uid: &str,
) -> Result<Vec<ClassificationEvent>, StoreError> {
    Ok(sqlx::query(
        "SELECT uid, concept_uid, actor_uid, note, at FROM fact_concept_event
          WHERE fact_uid = ? ORDER BY rowid",
    )
    .bind(fact_uid)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| ClassificationEvent {
        uid: row.get("uid"),
        concept_uid: row.get("concept_uid"),
        actor_uid: row.get("actor_uid"),
        note: row.get("note"),
        at: row.get("at"),
    })
    .collect())
}

// ------------------------------------------------------------------- totals

/// Inflow, outflow and net over a window, kept apart so both directions are
/// visible without a second pass. Direction is the delta's sign and nothing
/// else, so a refund classified `@cost` correctly *reduces* the cost total.
#[derive(Debug, Clone)]
pub struct DeltaTotals {
    pub net: DecimalValue,
    pub gains: DecimalValue,
    pub losses: DecimalValue,
    pub count: i64,
}

impl Default for DeltaTotals {
    fn default() -> Self {
        Self {
            net: zero(),
            gains: zero(),
            losses: zero(),
            count: 0,
        }
    }
}

impl DeltaTotals {
    fn add(&mut self, delta: DecimalValue) -> Result<(), StoreError> {
        self.net = self.net.aligned_add(delta).ok_or_else(overflow)?;
        if delta.is_positive() {
            self.gains = self.gains.aligned_add(delta).ok_or_else(overflow)?;
        } else if delta.is_negative() {
            self.losses = self.losses.aligned_add(delta).ok_or_else(overflow)?;
        }
        self.count += 1;
        Ok(())
    }
}

/// Totals for one window, split by the concept each change was classified
/// with. Anything unclassified — or classified outside the requested concept —
/// lands in `unclassified` rather than being silently dropped.
#[derive(Debug, Clone, Default)]
pub struct ClassifiedTotals {
    pub by_concept: BTreeMap<String, DeltaTotals>,
    pub unclassified: DeltaTotals,
}

/// A window over changes. Half-open `[from, to)` on purpose: adjacent periods
/// must tile without a Fact landing in both.
///
/// `from`/`to` are arbitrary instants, not a trailing duration — "10:23 on
/// 1 Jan 2020 until 00:00 on 2 Mar 2025" is expressible, which the
/// `sum_window` family cannot do.
#[derive(Debug, Clone)]
pub struct LedgerWindow<'a> {
    pub record_uids: &'a [String],
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    /// Restrict to changes whose classification descends from this concept.
    /// `None` includes every change.
    pub concept_uid: Option<&'a str>,
}

/// Sum the signed deltas of a window over a *set* of Records — a resource
/// lives in checking, cash and savings at once, so a total that can only read
/// one Record cannot answer "how much do I have".
///
/// Refuses to combine Records that do not share a unit: adding litres to
/// kilograms, or one token to another, produces a number that means nothing.
pub async fn totals(
    pool: &SqlitePool,
    window: &LedgerWindow<'_>,
) -> Result<DeltaTotals, StoreError> {
    let rows = window_rows(pool, window).await?;
    let mut totals = DeltaTotals::default();
    for (delta, _) in rows {
        totals.add(delta)?;
    }
    Ok(totals)
}

/// The same window, bucketed by the concept each change was classified with.
/// The breakdown question: "what did this window go to, by kind".
pub async fn totals_by_concept(
    pool: &SqlitePool,
    window: &LedgerWindow<'_>,
) -> Result<ClassifiedTotals, StoreError> {
    let rows = window_rows(pool, window).await?;
    let mut out = ClassifiedTotals::default();
    for (delta, concept) in rows {
        match concept {
            Some(concept) => out
                .by_concept
                .entry(concept)
                .or_default()
                .add(delta)?,
            None => out.unclassified.add(delta)?,
        }
    }
    Ok(out)
}

/// Fetch `(delta, classification)` for every Fact in the window, after checking
/// the Records share a unit and applying the concept filter.
async fn window_rows(
    pool: &SqlitePool,
    window: &LedgerWindow<'_>,
) -> Result<Vec<(DecimalValue, Option<String>)>, StoreError> {
    if window.record_uids.is_empty() {
        return Ok(Vec::new());
    }
    require_shared_unit(pool, window.record_uids).await?;

    // The concept filter expands down the DAG once, here, so the query itself
    // stays a flat membership test.
    let wanted: Option<HashSet<String>> = match window.concept_uid {
        Some(concept) => Some(
            crate::concepts::descendants_including(pool, concept)
                .await?
                .into_iter()
                .collect(),
        ),
        None => None,
    };

    let sql = format!(
        "SELECT f.delta_mantissa, f.delta_scale, fc.concept_uid
           FROM fact f
           LEFT JOIN fact_concept fc ON fc.fact_uid = f.uid
          WHERE f.record_uid IN ({}) AND f.at >= ? AND f.at < ?
          ORDER BY f.at, f.rowid",
        placeholders(window.record_uids.len())
    );
    let mut query = sqlx::query(&sql);
    for uid in window.record_uids {
        query = query.bind(uid.clone());
    }
    let rows = query
        .bind(instant(window.from))
        .bind(instant(window.to))
        .fetch_all(pool)
        .await?;

    let mut out = Vec::new();
    for row in rows {
        let concept: Option<String> = row.get("concept_uid");
        if let Some(wanted) = &wanted {
            match &concept {
                Some(concept) if wanted.contains(concept) => {}
                _ => continue,
            }
        }
        out.push((crate::exact::read_decimal(&row, "delta")?, concept));
    }
    Ok(out)
}

/// Sums stay unit-separated (blueprint E0). Converting between units is
/// deliberately out of scope here: `concept_conversion` has no time dimension,
/// so converting a 2020 change at today's rate would silently rewrite history.
/// A conversion that has to be time-aware is a rule, not a property of a sum.
async fn require_shared_unit(
    pool: &SqlitePool,
    record_uids: &[String],
) -> Result<(), StoreError> {
    let mut units: HashSet<Option<String>> = HashSet::new();
    for uid in record_uids {
        if let Some(record) = crate::records::get(pool, uid).await? {
            units.insert(record.unit_uid);
        }
    }
    if units.len() > 1 {
        return Err(sqlx::Error::Protocol(
            "cannot total Records of different units in one sum".into(),
        ));
    }
    Ok(())
}

// ------------------------------------------------------------------- levels

/// The level of one Record at an instant — "at the end of last month I had 10".
///
/// A distinct question from a sum, and it cannot fold from zero: retention
/// genuinely deletes archived Facts, so a naive full-chain sum under-reports
/// any compacted Record. It is the last checkpoint at or before `at`, plus
/// every delta after it up to `at`. `record.quantity` is the cache for *now*
/// and is never the answer for a past instant.
pub async fn level_at(
    pool: &SqlitePool,
    record_uid: &str,
    at: DateTime<Utc>,
) -> Result<DecimalValue, StoreError> {
    let bound = instant(at);
    let (mut level, after_rowid) = checkpoint_before(pool, record_uid, &bound).await?;
    let rows = match after_rowid {
        Some(rowid) => {
            sqlx::query(
                "SELECT delta_mantissa, delta_scale FROM fact
                  WHERE record_uid = ? AND rowid > ? AND at <= ?",
            )
            .bind(record_uid)
            .bind(rowid)
            .bind(&bound)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query(
                "SELECT delta_mantissa, delta_scale FROM fact
                  WHERE record_uid = ? AND at <= ?",
            )
            .bind(record_uid)
            .bind(&bound)
            .fetch_all(pool)
            .await?
        }
    };
    for row in rows {
        level = level
            .aligned_add(crate::exact::read_decimal(&row, "delta")?)
            .ok_or_else(overflow)?;
    }
    Ok(level)
}

/// One point per change: the running balance across `[from, to)`, so the past
/// line of a graph is a single ordered scan rather than one query per point.
///
/// Computed in **occurred-at** order, which is what makes a backdated Fact
/// correctly reshape the history behind it.
pub async fn level_series(
    pool: &SqlitePool,
    record_uid: &str,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<Vec<(String, DecimalValue)>, StoreError> {
    // The opening balance is everything up to but not including `from`, which
    // is what makes adjacent windows tile.
    let mut level = level_at(pool, record_uid, from).await?;
    let opening = level;
    let rows = sqlx::query(
        "SELECT at, delta_mantissa, delta_scale FROM fact
          WHERE record_uid = ? AND at >= ? AND at < ?
          ORDER BY at, rowid",
    )
    .bind(record_uid)
    .bind(instant(from))
    .bind(instant(to))
    .fetch_all(pool)
    .await?;

    let mut series = Vec::with_capacity(rows.len() + 1);
    series.push((instant(from), opening));
    for row in rows {
        level = level
            .aligned_add(crate::exact::read_decimal(&row, "delta")?)
            .ok_or_else(overflow)?;
        series.push((row.get::<String, _>("at"), level));
    }
    Ok(series)
}

/// The latest checkpoint at or before `bound` that carries a level, with its
/// rowid. Compaction's archive anchors are checkpoints too but carry
/// `{archive, ...}` rather than a level, so they are skipped instead of read
/// as zero.
async fn checkpoint_before(
    pool: &SqlitePool,
    record_uid: &str,
    bound: &str,
) -> Result<(DecimalValue, Option<i64>), StoreError> {
    let rows = sqlx::query(
        "SELECT rowid, payload FROM fact
          WHERE record_uid = ? AND cause_kind = 'checkpoint' AND at <= ?
          ORDER BY rowid DESC LIMIT 32",
    )
    .bind(record_uid)
    .bind(bound)
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
            continue;
        };
        let level = DecimalValue::parse_inferred(text).map_err(|error| {
            StoreError::Decode(format!("checkpoint level is not an exact decimal: {error}").into())
        })?;
        return Ok((level, Some(row.get::<i64, _>("rowid"))));
    }
    Ok((zero(), None))
}

fn overflow() -> StoreError {
    StoreError::Decode("ledger total overflows the exact range".to_string().into())
}

/// `?, ?, ?` for an `IN` list — SQLite has no array binding.
fn placeholders(count: usize) -> String {
    vec!["?"; count].join(", ")
}
