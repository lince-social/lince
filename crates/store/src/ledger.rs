use std::collections::{BTreeMap, HashSet};

use chrono::{DateTime, Utc};
use nucleus::DecimalValue;
use sqlx::{Row, SqlitePool};

use crate::StoreError;
use crate::exact::zero;
use crate::facts::instant;

pub async fn add_record_concept(
    pool: &SqlitePool,
    record_uid: &str,
    concept_uid: &str,
    actor_uid: Option<&str>,
) -> Result<(), StoreError> {
    crate::assertions::assert(
        pool,
        crate::assertions::NewAssertion {
            subject_uid: record_uid,
            predicate_uid: concept_uid,
            object_uid: None,
            role: crate::assertions::AssertionRole::Ordinary,
            quantity: None,
            unit_uid: None,
            asserted_by: actor_uid,
        },
    )
    .await?;
    Ok(())
}

pub async fn remove_record_concept(
    pool: &SqlitePool,
    record_uid: &str,
    concept_uid: &str,
) -> Result<bool, StoreError> {
    crate::assertions::retract_tuple(pool, record_uid, concept_uid, None, None).await
}

pub async fn record_concepts(
    pool: &SqlitePool,
    record_uid: &str,
) -> Result<Vec<String>, StoreError> {
    let mut out = crate::assertions::concepts_for_record(pool, record_uid).await?;
    if let Some(identity) = crate::assertions::identity_concept(pool, record_uid).await? {
        out.retain(|concept| concept != &identity);
        out.insert(0, identity);
    }
    Ok(out)
}

pub async fn all_record_concepts(
    pool: &SqlitePool,
) -> Result<BTreeMap<String, Vec<String>>, StoreError> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let rows = sqlx::query(
        "SELECT subject_uid, predicate_uid FROM record_assertion
          WHERE retracted_at IS NULL ORDER BY subject_uid, created_at, predicate_uid",
    )
    .fetch_all(pool)
    .await?;
    for row in rows {
        out.entry(row.get("subject_uid"))
            .or_default()
            .push(row.get("predicate_uid"));
    }
    Ok(out)
}

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
        "SELECT DISTINCT a.subject_uid AS uid FROM record_assertion a
           JOIN record r ON r.uid = a.subject_uid
          WHERE r.deleted_at IS NULL AND a.retracted_at IS NULL
            AND a.predicate_uid IN ({placeholders})"
    );
    let mut query = sqlx::query(&sql);
    for concept in &concepts {
        query = query.bind(concept.clone());
    }
    Ok(query
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| row.get::<String, _>("uid"))
        .collect())
}

pub async fn classify_fact(
    pool: &SqlitePool,
    fact_uid: &str,
    concept_uid: Option<&str>,
    actor_uid: Option<&str>,
    note: Option<&str>,
) -> Result<String, StoreError> {
    let uid = nucleus::new_uid("fc");
    let at = instant(Utc::now());
    let mut tx = crate::write_tx(pool).await?;
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

pub async fn fact_concept(pool: &SqlitePool, fact_uid: &str) -> Result<Option<String>, StoreError> {
    Ok(
        sqlx::query("SELECT concept_uid FROM fact_concept WHERE fact_uid = ?")
            .bind(fact_uid)
            .fetch_optional(pool)
            .await?
            .and_then(|row| row.get::<Option<String>, _>("concept_uid")),
    )
}

pub async fn all_fact_concepts(pool: &SqlitePool) -> Result<BTreeMap<String, String>, StoreError> {
    let rows = sqlx::query("SELECT fact_uid, concept_uid FROM fact_concept")
        .fetch_all(pool)
        .await?;
    let mut out = BTreeMap::new();
    for row in rows {
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

#[derive(Debug, Clone, Default)]
pub struct ClassifiedTotals {
    pub by_concept: BTreeMap<String, DeltaTotals>,
    pub unclassified: DeltaTotals,
}

#[derive(Debug, Clone)]
pub struct LedgerWindow<'a> {
    pub record_uids: &'a [String],
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    pub concept_uid: Option<&'a str>,
}

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

pub async fn totals_by_concept(
    pool: &SqlitePool,
    window: &LedgerWindow<'_>,
) -> Result<ClassifiedTotals, StoreError> {
    let rows = window_rows(pool, window).await?;
    let mut out = ClassifiedTotals::default();
    for (delta, concept) in rows {
        match concept {
            Some(concept) => out.by_concept.entry(concept).or_default().add(delta)?,
            None => out.unclassified.add(delta)?,
        }
    }
    Ok(out)
}

async fn window_rows(
    pool: &SqlitePool,
    window: &LedgerWindow<'_>,
) -> Result<Vec<(DecimalValue, Option<String>)>, StoreError> {
    if window.record_uids.is_empty() {
        return Ok(Vec::new());
    }
    require_shared_unit(pool, window.record_uids).await?;

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

async fn require_shared_unit(pool: &SqlitePool, record_uids: &[String]) -> Result<(), StoreError> {
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

pub async fn level_series(
    pool: &SqlitePool,
    record_uid: &str,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<Vec<(String, DecimalValue)>, StoreError> {
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

fn placeholders(count: usize) -> String {
    vec!["?"; count].join(", ")
}
