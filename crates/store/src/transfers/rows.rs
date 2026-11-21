//! Transfer catalog rows and their related party and Promise lookups.

use nucleus::{RecordKind, new_uid, performance::Span};
use sqlx::{Row, SqlitePool, sqlite::SqliteRow};

use super::{TransferLocationSnapshot, item_terms::location_snapshot};
use crate::{
    StoreError,
    exact::zero,
    misc::{PromiseRow, map_promise_pub},
    records::{self, NewRecord},
};

/// Current terms and status of a stored Transfer.
#[derive(Debug, Clone)]
pub struct TransferRow {
    pub record_uid: String,
    pub revision: i64,
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
    pub require_confirmation: bool,
    pub default_place: Option<TransferLocationSnapshot>,
}

/// Terms for creating a legacy Transfer without a signed revision.
#[derive(Debug)]
pub struct NewTransfer<'a> {
    pub slug: Option<&'a str>,
    pub head: &'a str,
    pub agreement_type: &'a str,
    pub agreement_pct: Option<i64>,
    pub satiation: Option<&'a str>,
    pub source_uid: Option<&'a str>,
    pub reserve_default: Option<&'a str>,
    pub require_confirmation: bool,
}

/// Creates a legacy Transfer and returns its Record UID.
///
/// # Errors
///
/// Returns an error if Record creation or the Transfer insert fails.
pub async fn create(pool: &SqlitePool, new: NewTransfer<'_>) -> Result<String, StoreError> {
    let uid = records::create(
        pool,
        NewRecord {
            slug: new.slug,
            kind: RecordKind::Transfer,
            head: new.head,
            body: "",
            quantity: zero(),
        },
    )
    .await?
    .uid;
    sqlx::query(
        "INSERT INTO transfer (record_uid, agreement_type, agreement_pct, satiation, source_uid,
                               reserve_default, require_confirmation)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&uid)
    .bind(new.agreement_type)
    .bind(new.agreement_pct)
    .bind(new.satiation)
    .bind(new.source_uid)
    .bind(new.reserve_default)
    .bind(new.require_confirmation)
    .execute(pool)
    .await?;
    Ok(uid)
}

/// A Transfer together with the title and slug of its Record.
#[derive(Debug, Clone)]
pub struct TransferListRow {
    pub transfer: TransferRow,
    pub slug: Option<String>,
    pub head: String,
}

/// Lists Transfers in Record creation order.
///
/// # Errors
///
/// Returns an error if the catalog query fails.
pub async fn list_all(pool: &SqlitePool) -> Result<Vec<TransferListRow>, StoreError> {
    sqlx::query(
        "SELECT t.*, r.slug, r.head, r.quantity_mantissa FROM transfer t
           JOIN record r ON r.uid = t.record_uid
          ORDER BY r.created_at",
    )
    .map(|row| TransferListRow {
        transfer: transfer_row(&row),
        slug: row.get("slug"),
        head: row.get("head"),
    })
    .fetch_all(pool)
    .await
}

/// Checks whether a Record UID belongs to a stored Transfer.
///
/// # Errors
///
/// Returns an error if the existence query fails.
pub async fn exists(pool: &SqlitePool, uid: &str) -> Result<bool, StoreError> {
    let _read = Span::new("transfer-exists-await");
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM transfer t JOIN record r ON r.uid = t.record_uid
         WHERE t.record_uid = ?)",
    )
    .bind(uid)
    .fetch_one(pool)
    .await
}

/// Looks up the current Transfer terms by Record UID.
///
/// # Errors
///
/// Returns an error if the lookup fails.
pub async fn get(pool: &SqlitePool, uid: &str) -> Result<Option<TransferRow>, StoreError> {
    sqlx::query(
        "SELECT t.*, r.quantity_mantissa FROM transfer t JOIN record r ON r.uid = t.record_uid
         WHERE t.record_uid = ?",
    )
    .bind(uid)
    .map(|row| transfer_row(&row))
    .fetch_optional(pool)
    .await
}

/// Adds a participant and returns its party UID.
///
/// # Errors
///
/// Returns an error if the Transfer is missing, the actor is already a party, or the insert fails.
pub async fn add_party(
    pool: &SqlitePool,
    transfer_uid: &str,
    actor_uid: &str,
) -> Result<String, StoreError> {
    let uid = new_uid("y");
    sqlx::query("INSERT INTO transfer_party (uid, transfer_uid, actor_uid) VALUES (?, ?, ?)")
        .bind(&uid)
        .bind(transfer_uid)
        .bind(actor_uid)
        .execute(pool)
        .await?;
    Ok(uid)
}

/// Finds the actor recorded as the Transfer's creator.
///
/// # Errors
///
/// Returns an error if the lookup fails.
pub async fn creator_party_actor(
    pool: &SqlitePool,
    transfer_uid: &str,
) -> Result<Option<String>, StoreError> {
    sqlx::query_scalar(
        "SELECT actor_uid FROM transfer_party
         WHERE transfer_uid = ? AND kind = 'creator' LIMIT 1",
    )
    .bind(transfer_uid)
    .fetch_optional(pool)
    .await
}

/// Lists party UID, actor UID, and agreement level for the current revision.
///
/// A party without an agreement at this revision has level zero.
///
/// # Errors
///
/// Returns an error if the query fails.
pub async fn party_levels(
    pool: &SqlitePool,
    transfer_uid: &str,
) -> Result<Vec<(String, String, i64)>, StoreError> {
    sqlx::query_as(
        "SELECT p.uid, p.actor_uid, COALESCE(a.level, 0) AS level
         FROM transfer_party p
         JOIN transfer t ON t.record_uid = p.transfer_uid
         LEFT JOIN transfer_agreement a
           ON a.party_uid = p.uid
          AND a.transfer_uid = p.transfer_uid
          AND a.revision = t.revision
         WHERE p.transfer_uid = ?",
    )
    .bind(transfer_uid)
    .fetch_all(pool)
    .await
}

/// Looks up the actor of a party on the given Transfer.
///
/// # Errors
///
/// Returns an error if the lookup fails.
pub async fn party_actor(
    pool: &SqlitePool,
    transfer_uid: &str,
    party_uid: &str,
) -> Result<Option<String>, StoreError> {
    sqlx::query_scalar("SELECT actor_uid FROM transfer_party WHERE transfer_uid = ? AND uid = ?")
        .bind(transfer_uid)
        .bind(party_uid)
        .fetch_optional(pool)
        .await
}

/// Looks up the party UID of an actor on the given Transfer.
///
/// # Errors
///
/// Returns an error if the lookup fails.
pub async fn party_for_actor(
    pool: &SqlitePool,
    transfer_uid: &str,
    actor_uid: &str,
) -> Result<Option<String>, StoreError> {
    sqlx::query_scalar("SELECT uid FROM transfer_party WHERE transfer_uid = ? AND actor_uid = ?")
        .bind(transfer_uid)
        .bind(actor_uid)
        .fetch_optional(pool)
        .await
}

/// Lists the recognized Promises of a Transfer.
///
/// # Errors
///
/// Returns an error if the lookup fails.
pub async fn promises_of(
    pool: &SqlitePool,
    transfer_uid: &str,
) -> Result<Vec<PromiseRow>, StoreError> {
    Ok(sqlx::query("SELECT * FROM promise WHERE transfer_uid = ?")
        .bind(transfer_uid)
        .fetch_all(pool)
        .await?
        .into_iter()
        .filter_map(map_promise_pub)
        .collect())
}

/// Lists Transfer UIDs with the same source, excluding the given Transfer.
///
/// # Errors
///
/// Returns an error if the lookup fails.
pub async fn siblings_of_source(
    pool: &SqlitePool,
    source_uid: &str,
    except: &str,
) -> Result<Vec<String>, StoreError> {
    sqlx::query_scalar("SELECT record_uid FROM transfer WHERE source_uid = ? AND record_uid != ?")
        .bind(source_uid)
        .bind(except)
        .fetch_all(pool)
        .await
}

/// Lists proposed and agreed Promises with a condition.
///
/// # Errors
///
/// Returns an error if the query fails.
pub async fn conditional_pending(pool: &SqlitePool) -> Result<Vec<PromiseRow>, StoreError> {
    Ok(sqlx::query(
        "SELECT * FROM promise WHERE condition IS NOT NULL AND state IN ('proposed', 'agreed')",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .filter_map(map_promise_pub)
    .collect())
}

fn transfer_row(r: &SqliteRow) -> TransferRow {
    TransferRow {
        record_uid: r.get("record_uid"),
        revision: r.get("revision"),
        agreement_type: r.get("agreement_type"),
        agreement_pct: r.get("agreement_pct"),
        settlement: r.get("settlement"),
        visibility: r.get("visibility"),
        max_proximity: r.get("max_proximity"),
        satiation: r.get("satiation"),
        parent_uid: r.get("parent_uid"),
        source_uid: r.get("source_uid"),
        reserve_default: r.get("reserve_default"),
        active: r.get::<&str, _>("quantity_mantissa") != "0",
        require_confirmation: r.get("require_confirmation"),
        default_place: location_snapshot(
            r.get("default_location_lat"),
            r.get("default_location_lon"),
            r.get("default_location_address"),
        ),
    }
}
