//! Transfer repository (blueprint VIII.1): a transfer is a record
//! (kind='transfer', quantity = active) whose items ARE promises.

use chrono::{DateTime, Utc};
use nucleus::{Cause, Fact, NewFact, RecordKind};
use sqlx::{Row, SqlitePool};

use crate::StoreError;
use crate::records::{self, NewRecord};

#[derive(Debug, Clone)]
pub struct TransferRow {
    pub record_uid: String,
    pub agreement_type: String,
    pub agreement_pct: Option<i64>,
    pub settlement: String,
    pub visibility: String,
    pub max_proximity: Option<i64>,
    pub satiation: Option<String>,
    pub parent_uid: Option<String>,
    pub source_uid: Option<String>,
    pub reserve_default: Option<String>,
    pub active: bool,
    /// Settlement demands delivery+receipt confirmation facts (VIII.3).
    pub require_confirmation: bool,
}

pub struct NewTransfer<'a> {
    pub slug: Option<&'a str>,
    pub head: &'a str,
    pub agreement_type: &'a str,
    pub agreement_pct: Option<i64>,
    pub satiation: Option<&'a str>,
    pub source_uid: Option<&'a str>,
    /// Default `reserve_from` for promises bundled into this transfer (V.3);
    /// None = the global default ('active').
    pub reserve_default: Option<&'a str>,
    /// Settlement demands delivery+receipt confirmation facts (VIII.3).
    pub require_confirmation: bool,
}

#[derive(Debug, Clone)]
pub struct DraftPromise {
    pub record_uid: String,
    pub person_uid: String,
    pub delta: f64,
    pub window_end: Option<String>,
    pub condition: Option<String>,
    pub reserve_from: String,
}

#[derive(Debug, Clone)]
pub struct NewTransferDraft {
    pub slug: Option<String>,
    pub head: String,
    pub agreement_type: String,
    pub agreement_pct: Option<i64>,
    pub satiation: Option<String>,
    pub parent_uid: Option<String>,
    pub source_uid: Option<String>,
    pub visibility: String,
    pub max_proximity: Option<i64>,
    pub reserve_default: String,
    pub require_confirmation: bool,
    pub people: Vec<String>,
    pub promises: Vec<DraftPromise>,
    pub organ_uid: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CreatedTransferDraft {
    pub transfer_uid: String,
    pub party_uids: Vec<String>,
    pub promise_uids: Vec<String>,
    pub fact: Fact,
}

/// Commit a fully validated draft, its creator visibility, and its creation
/// Fact as one database unit. `sign` keeps key material in the engine while
/// preserving the same signed hash-chain contract as the normal Fact path.
pub async fn create_draft<F>(
    pool: &SqlitePool,
    draft: NewTransferDraft,
    now: DateTime<Utc>,
    fact_actor: Option<String>,
    visibility_actor: Option<String>,
    sign: F,
) -> Result<CreatedTransferDraft, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    if let Some(slug) = draft.slug.as_deref() {
        if !nucleus::valid_slug(slug) {
            return Err(sqlx::Error::Protocol(format!("invalid slug `{slug}`")));
        }
    }

    let now_string = now.to_rfc3339();
    let mut tx = pool.begin().await?;
    let transfer_uid = nucleus::new_uid("r");
    sqlx::query(
        "INSERT INTO record
            (uid, slug, kind, head, body, quantity, organ_uid, created_at, updated_at)
         VALUES (?, ?, ?, ?, '', 0, ?, ?, ?)",
    )
    .bind(&transfer_uid)
    .bind(&draft.slug)
    .bind(RecordKind::Transfer.as_str())
    .bind(&draft.head)
    .bind(&draft.organ_uid)
    .bind(&now_string)
    .bind(&now_string)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO transfer
            (record_uid, agreement_type, agreement_pct, visibility, max_proximity,
             satiation, parent_uid, source_uid, reserve_default, require_confirmation)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&transfer_uid)
    .bind(&draft.agreement_type)
    .bind(draft.agreement_pct)
    .bind(&draft.visibility)
    .bind(draft.max_proximity)
    .bind(&draft.satiation)
    .bind(&draft.parent_uid)
    .bind(&draft.source_uid)
    .bind(&draft.reserve_default)
    .bind(draft.require_confirmation as i64)
    .execute(&mut *tx)
    .await?;

    let mut party_uids = Vec::with_capacity(draft.people.len());
    for person_uid in &draft.people {
        let party_uid = nucleus::new_uid("y");
        sqlx::query(
            "INSERT INTO transfer_party (uid, transfer_uid, actor_uid) VALUES (?, ?, ?)",
        )
        .bind(&party_uid)
        .bind(&transfer_uid)
        .bind(person_uid)
        .execute(&mut *tx)
        .await?;
        party_uids.push(party_uid);
    }

    let mut promise_uids = Vec::with_capacity(draft.promises.len());
    for promise in &draft.promises {
        let promise_uid = nucleus::new_uid("p");
        sqlx::query(
            "INSERT INTO promise
                (uid, record_uid, delta, window_end, party_uid, state, condition,
                 transfer_uid, reserve_from, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, 'proposed', ?, ?, ?, ?, ?)",
        )
        .bind(&promise_uid)
        .bind(&promise.record_uid)
        .bind(promise.delta)
        .bind(&promise.window_end)
        .bind(&promise.person_uid)
        .bind(&promise.condition)
        .bind(&transfer_uid)
        .bind(&promise.reserve_from)
        .bind(&now_string)
        .bind(&now_string)
        .execute(&mut *tx)
        .await?;
        promise_uids.push(promise_uid);
    }

    if let Some(subject) = visibility_actor.as_deref() {
        sqlx::query(
            "INSERT INTO visibility_rule
                (uid, subject_kind, subject_uid, target_uid, grant_level)
             VALUES (?, 'actor', ?, ?, 'visible')",
        )
        .bind(nucleus::new_uid("v"))
        .bind(subject)
        .bind(&transfer_uid)
        .execute(&mut *tx)
        .await?;
    }
    if draft.visibility == "public" {
        sqlx::query(
            "INSERT INTO visibility_rule
                (uid, subject_kind, subject_uid, target_uid, grant_level)
             VALUES (?, 'public', NULL, ?, 'visible')",
        )
        .bind(nucleus::new_uid("v"))
        .bind(&transfer_uid)
        .execute(&mut *tx)
        .await?;
    }

    let previous_hash = crate::facts::last_hash(&mut tx).await?;
    let mut fact = nucleus::fact::seal(
        NewFact {
            uid: None,
            record_uid: transfer_uid.clone(),
            delta: 1.0,
            at: None,
            actor_uid: fact_actor,
            cause: Cause::user_edit(),
            payload: Some(
                serde_json::json!({
                    "action": "create-transfer-draft",
                    "parties": party_uids.clone(),
                    "promises": promise_uids.clone(),
                })
                .to_string(),
            ),
        },
        &previous_hash,
        now,
    );
    fact.signature = sign(&fact.hash);
    crate::facts::insert(&mut tx, &fact).await?;
    crate::records::bump_quantity(&mut tx, &transfer_uid, 1.0, &now_string).await?;

    tx.commit().await?;
    Ok(CreatedTransferDraft {
        transfer_uid,
        party_uids,
        promise_uids,
        fact,
    })
}

pub async fn create(pool: &SqlitePool, new: NewTransfer<'_>) -> Result<String, StoreError> {
    let rec = records::create(
        pool,
        NewRecord {
            slug: new.slug,
            kind: RecordKind::Transfer,
            head: new.head,
            body: "",
            // The engine appends the activation/creation fact after the
            // sidecar exists. Quantity remains a fact-derived cache.
            quantity: 0.0,
        },
    )
    .await?;
    sqlx::query(
        "INSERT INTO transfer (record_uid, agreement_type, agreement_pct, satiation, source_uid,
                               reserve_default, require_confirmation)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&rec.uid)
    .bind(new.agreement_type)
    .bind(new.agreement_pct)
    .bind(new.satiation)
    .bind(new.source_uid)
    .bind(new.reserve_default)
    .bind(new.require_confirmation as i64)
    .execute(pool)
    .await?;
    Ok(rec.uid)
}

/// A transfer with its record's identity — the `source: transfer` Protein feed.
#[derive(Debug, Clone)]
pub struct TransferListRow {
    pub transfer: TransferRow,
    pub slug: Option<String>,
    pub head: String,
}

/// Every transfer joined to its record, oldest first.
pub async fn list_all(pool: &SqlitePool) -> Result<Vec<TransferListRow>, StoreError> {
    Ok(sqlx::query(
        "SELECT t.*, r.slug, r.head, r.quantity FROM transfer t
           JOIN record r ON r.uid = t.record_uid
          ORDER BY r.created_at",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|r| TransferListRow {
        transfer: TransferRow {
            record_uid: r.get("record_uid"),
            agreement_type: r.get("agreement_type"),
            agreement_pct: r.get("agreement_pct"),
            settlement: r.get("settlement"),
            visibility: r.get("visibility"),
            max_proximity: r.get("max_proximity"),
            satiation: r.get("satiation"),
            parent_uid: r.get("parent_uid"),
            source_uid: r.get("source_uid"),
            reserve_default: r.get("reserve_default"),
            active: r.get::<f64, _>("quantity") != 0.0,
            require_confirmation: r.get::<i64, _>("require_confirmation") != 0,
        },
        slug: r.get("slug"),
        head: r.get("head"),
    })
    .collect())
}

pub async fn get(pool: &SqlitePool, uid: &str) -> Result<Option<TransferRow>, StoreError> {
    Ok(sqlx::query(
        "SELECT t.*, r.quantity FROM transfer t JOIN record r ON r.uid = t.record_uid
         WHERE t.record_uid = ?",
    )
    .bind(uid)
    .fetch_optional(pool)
    .await?
    .map(|r| TransferRow {
        record_uid: r.get("record_uid"),
        agreement_type: r.get("agreement_type"),
        agreement_pct: r.get("agreement_pct"),
        settlement: r.get("settlement"),
        visibility: r.get("visibility"),
        max_proximity: r.get("max_proximity"),
        satiation: r.get("satiation"),
        parent_uid: r.get("parent_uid"),
        source_uid: r.get("source_uid"),
        reserve_default: r.get("reserve_default"),
        active: r.get::<f64, _>("quantity") != 0.0,
        require_confirmation: r.get::<i64, _>("require_confirmation") != 0,
    }))
}

pub async fn add_party(
    pool: &SqlitePool,
    transfer_uid: &str,
    actor_uid: &str,
) -> Result<String, StoreError> {
    let uid = nucleus::new_uid("y");
    sqlx::query("INSERT INTO transfer_party (uid, transfer_uid, actor_uid) VALUES (?, ?, ?)")
        .bind(&uid)
        .bind(transfer_uid)
        .bind(actor_uid)
        .execute(pool)
        .await?;
    Ok(uid)
}

/// (party_uid, actor_uid, agreement level) for every party; level 0 when the
/// party never agreed or was invalidated.
pub async fn party_levels(
    pool: &SqlitePool,
    transfer_uid: &str,
) -> Result<Vec<(String, String, i64)>, StoreError> {
    Ok(sqlx::query(
        "SELECT p.uid, p.actor_uid, COALESCE(a.level, 0) AS level
         FROM transfer_party p
         LEFT JOIN transfer_agreement a ON a.party_uid = p.uid AND a.transfer_uid = p.transfer_uid
         WHERE p.transfer_uid = ?",
    )
    .bind(transfer_uid)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|r| (r.get("uid"), r.get("actor_uid"), r.get("level")))
    .collect())
}

/// Resolve one transfer-party row to its Person record uid, only when that
/// party belongs to the requested transfer.
pub async fn party_actor(
    pool: &SqlitePool,
    transfer_uid: &str,
    party_uid: &str,
) -> Result<Option<String>, StoreError> {
    sqlx::query_scalar(
        "SELECT actor_uid FROM transfer_party WHERE transfer_uid = ? AND uid = ?",
    )
    .bind(transfer_uid)
    .bind(party_uid)
    .fetch_optional(pool)
    .await
}

/// The transfer-party row representing a particular Person, if present.
pub async fn party_for_actor(
    pool: &SqlitePool,
    transfer_uid: &str,
    actor_uid: &str,
) -> Result<Option<String>, StoreError> {
    sqlx::query_scalar(
        "SELECT uid FROM transfer_party WHERE transfer_uid = ? AND actor_uid = ?",
    )
    .bind(transfer_uid)
    .bind(actor_uid)
    .fetch_optional(pool)
    .await
}

pub async fn set_agreement(
    pool: &SqlitePool,
    transfer_uid: &str,
    party_uid: &str,
    level: i64,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO transfer_agreement (uid, transfer_uid, party_uid, level, at)
         VALUES (?, ?, ?, ?, ?)
         ON CONFLICT(transfer_uid, party_uid) DO UPDATE SET level = excluded.level, at = excluded.at",
    )
    .bind(nucleus::new_uid("g"))
    .bind(transfer_uid)
    .bind(party_uid)
    .bind(level)
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

/// Counteroffers are edits: editing a bundled promise drops every party's
/// agreement back to 0 (blueprint VIII.1).
pub async fn invalidate_agreements(
    pool: &SqlitePool,
    transfer_uid: &str,
) -> Result<(), StoreError> {
    sqlx::query("UPDATE transfer_agreement SET level = 0 WHERE transfer_uid = ?")
        .bind(transfer_uid)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn promises_of(
    pool: &SqlitePool,
    transfer_uid: &str,
) -> Result<Vec<crate::misc::PromiseRow>, StoreError> {
    Ok(sqlx::query("SELECT * FROM promise WHERE transfer_uid = ?")
        .bind(transfer_uid)
        .fetch_all(pool)
        .await?
        .into_iter()
        .filter_map(crate::misc::map_promise_pub)
        .collect())
}

/// Sibling transfers duplicated from the same source (satiation, VIII.3).
pub async fn siblings_of_source(
    pool: &SqlitePool,
    source_uid: &str,
    except: &str,
) -> Result<Vec<String>, StoreError> {
    Ok(
        sqlx::query("SELECT record_uid FROM transfer WHERE source_uid = ? AND record_uid != ?")
            .bind(source_uid)
            .bind(except)
            .fetch_all(pool)
            .await?
            .into_iter()
            .map(|r| r.get("record_uid"))
            .collect(),
    )
}

/// Promises with a condition that are waiting to activate (chains/spectators).
pub async fn conditional_pending(
    pool: &SqlitePool,
) -> Result<Vec<crate::misc::PromiseRow>, StoreError> {
    Ok(sqlx::query(
        "SELECT * FROM promise WHERE condition IS NOT NULL AND state IN ('proposed', 'agreed')",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .filter_map(crate::misc::map_promise_pub)
    .collect())
}
