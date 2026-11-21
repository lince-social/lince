use std::collections::HashMap;

use nucleus::{
    DecimalValue,
    performance::{Span, add},
};
use serde_json::to_string;
use sqlx::{Executor, QueryBuilder, Row, Sqlite, SqlitePool, sqlite::SqliteRow};

use super::protocol;
use crate::{StoreError, exact::read_decimal};

/// Selects how a Record row represents its body projection.
pub trait RecordBody: Sized + Send + Sync {
    /// Constructs a projection, which may require a loaded body.
    fn from_loaded(body: Option<String>) -> Result<Self, StoreError>;

    /// Borrows the body if the query loaded it.
    fn loaded(&self) -> Option<&str>;

    /// Borrows the loaded body, or returns a protocol error if it was omitted.
    fn required(&self) -> Result<&str, StoreError> {
        self.loaded()
            .ok_or_else(|| protocol("Record body was not loaded for this query"))
    }
}

impl RecordBody for String {
    fn from_loaded(body: Option<String>) -> Result<Self, StoreError> {
        body.ok_or_else(|| protocol("Record body was not loaded for this query"))
    }

    fn loaded(&self) -> Option<&str> {
        Some(self.as_str())
    }
}

impl RecordBody for Option<String> {
    fn from_loaded(body: Option<String>) -> Result<Self, StoreError> {
        Ok(body)
    }

    fn loaded(&self) -> Option<&str> {
        self.as_deref()
    }
}

/// A stored Record with a required or optional body projection.
#[derive(Debug, Clone)]
pub struct RecordRow<Body = String> {
    pub uid: String,
    pub slug: Option<String>,
    pub kind: String,
    pub head: String,
    pub body: Body,
    pub quantity: DecimalValue,
    pub identity_predicate_uid: Option<String>,
    pub unit_uid: Option<String>,
    pub place_uid: Option<String>,
    pub organ_uid: Option<String>,
    pub file_origin: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub created_hlc: Option<i64>,
}

impl<Body> RecordRow<Body> {
    /// Approximates the exact quantity for floating-point consumers.
    pub fn quantity_f64(&self) -> f64 {
        self.quantity.to_f64()
    }

    /// Reports whether the quantity is nonzero.
    pub fn is_active(&self) -> bool {
        !self.quantity.is_zero()
    }
}

pub(crate) fn map_row<Body: RecordBody>(r: SqliteRow) -> Result<RecordRow<Body>, StoreError> {
    let row = RecordRow {
        uid: r.get("uid"),
        slug: r.get("slug"),
        kind: r.get("kind"),
        head: r.get("head"),
        body: Body::from_loaded(r.try_get::<Option<String>, _>("body")?)?,
        quantity: read_decimal(&r, "quantity")?,
        identity_predicate_uid: r.get("identity_predicate_uid"),
        unit_uid: r.get("unit_uid"),
        place_uid: r.get("place_uid"),
        organ_uid: r.get("organ_uid"),
        file_origin: r.try_get("file_origin").ok().flatten(),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
        created_hlc: r.try_get("created_hlc").ok().flatten(),
    };
    add(
        "record-read-body-bytes",
        row.body.loaded().map_or(0, str::len) as u64,
    );
    Ok(row)
}

fn map_rows(rows: Vec<SqliteRow>) -> Result<Vec<RecordRow>, StoreError> {
    rows.into_iter().map(map_row).collect()
}

/// Selects undeleted Records for a projected read.
#[derive(Clone, Copy, Debug)]
pub enum RecordSelection<'a> {
    All,
    Uid(&'a str),
    Token(&'a str),
    Uids(&'a [&'a str]),
    FileOrigin(Option<&'a str>),
}

pub(crate) const METADATA_COLUMNS: &str = "r.uid, r.slug, r.kind, r.head, NULL AS body, r.quantity_mantissa, r.quantity_scale, r.unit_uid, r.place_uid, r.organ_uid, r.created_at, r.updated_at, r.created_hlc, r.file_origin";

/// Reads matching live Records with or without the body.
///
/// # Errors
/// Rejects an oversized UID selection or an incompatible body projection,
/// and propagates database errors.
pub async fn read<'e, Body: RecordBody>(
    executor: impl Executor<'e, Database = Sqlite>,
    selection: RecordSelection<'_>,
    with_body: bool,
) -> Result<Vec<RecordRow<Body>>, StoreError> {
    read_rows(executor, selection, with_body)
        .await?
        .into_iter()
        .map(map_row)
        .collect()
}

pub(crate) async fn read_rows<'e>(
    executor: impl Executor<'e, Database = Sqlite>,
    selection: RecordSelection<'_>,
    with_body: bool,
) -> Result<Vec<SqliteRow>, StoreError> {
    if matches!(selection, RecordSelection::Uids([])) {
        return Ok(Vec::new());
    }
    let mut query = QueryBuilder::<Sqlite>::new("SELECT ");
    query.push(if with_body { "r.*" } else { METADATA_COLUMNS });
    query.push(
        ", (SELECT predicate_uid FROM record_assertion a WHERE a.subject_uid = r.uid AND a.role = 'identity' AND a.retracted_at IS NULL) AS identity_predicate_uid FROM record r WHERE r.deleted_at IS NULL",
    );
    match selection {
        RecordSelection::All => {
            query.push(" ORDER BY r.created_at, r.uid");
        }
        RecordSelection::Uid(uid) => {
            query.push(" AND r.uid = ").push_bind(uid);
        }
        RecordSelection::Token(token) => {
            query
                .push(" AND (r.slug = ")
                .push_bind(token)
                .push(" OR r.uid = ")
                .push_bind(token)
                .push(") LIMIT 1");
        }
        RecordSelection::Uids(uids) => {
            query
                .push(" AND r.uid IN (SELECT value FROM json_each(")
                .push_bind(selected_uids(uids)?)
                .push(")) ORDER BY r.created_at, r.uid");
        }
        RecordSelection::FileOrigin(origin) => {
            query.push(" AND r.file_origin IS ").push_bind(origin);
            query.push(" ORDER BY r.created_at, r.uid");
        }
    }
    query.build().fetch_all(executor).await
}

/// Checks whether a live Record has the given UID.
pub async fn exists(pool: &SqlitePool, uid: &str) -> Result<bool, StoreError> {
    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM record WHERE uid = ? AND deleted_at IS NULL)")
        .bind(uid)
        .fetch_one(pool)
        .await
}

/// Loads a live Record and its body by UID.
pub async fn get(pool: &SqlitePool, uid: &str) -> Result<Option<RecordRow>, StoreError> {
    sqlx::query(
        "SELECT r.*,
                (SELECT predicate_uid FROM record_assertion a
                  WHERE a.subject_uid = r.uid AND a.role = 'identity'
                    AND a.retracted_at IS NULL) AS identity_predicate_uid
           FROM record r WHERE r.uid = ? AND r.deleted_at IS NULL",
    )
    .bind(uid)
    .fetch_optional(pool)
    .await?
    .map(map_row)
    .transpose()
}

/// The title and body of a stored Record.
pub struct RecordText {
    pub head: String,
    pub body: String,
}

/// Independent selections for the unit and identity fields.
#[derive(Default)]
pub struct GroupingFields {
    pub unit: bool,
    pub identity: bool,
}

/// A Record UID and its requested grouping fields.
pub struct RecordGrouping {
    pub uid: String,
    pub unit: Option<String>,
    pub identity: Option<String>,
}

/// Reads the requested grouping fields of live Records in creation order.
pub async fn grouping(
    pool: &SqlitePool,
    fields: GroupingFields,
) -> Result<Vec<RecordGrouping>, StoreError> {
    let _read = Span::new("record-grouping-read-await");
    if !fields.unit && !fields.identity {
        add("record-grouping-read-skipped", 1);
        return Ok(Vec::new());
    }
    let unit = if fields.unit { "r.unit_uid" } else { "NULL" };
    let identity = if fields.identity {
        "(SELECT predicate_uid FROM record_assertion a WHERE a.subject_uid = r.uid AND a.role = 'identity' AND a.retracted_at IS NULL)"
    } else {
        "NULL"
    };
    let query = format!(
        "SELECT r.uid, {unit}, {identity} FROM record r WHERE r.deleted_at IS NULL ORDER BY r.created_at, r.uid"
    );
    let rows =
        sqlx::query_as::<_, (String, Option<String>, Option<String>)>(sqlx::AssertSqlSafe(query))
            .fetch_all(pool)
            .await?;
    add("record-grouping-rows", rows.len() as u64);
    Ok(rows
        .into_iter()
        .map(|(uid, unit, identity)| RecordGrouping {
            uid,
            unit,
            identity,
        })
        .collect())
}

/// Loads the title and body of a live Record.
pub async fn text(pool: &SqlitePool, uid: &str) -> Result<Option<RecordText>, StoreError> {
    let _read = Span::new("record-text-read-await");
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT head, body FROM record WHERE uid = ? AND deleted_at IS NULL")
            .bind(uid)
            .fetch_optional(pool)
            .await?;
    Ok(row.map(|(head, body)| RecordText { head, body }))
}

/// Loads the title of a live Record.
pub async fn head(pool: &SqlitePool, uid: &str) -> Result<Option<String>, StoreError> {
    let _read = Span::new("record-head-read-await");
    sqlx::query_scalar("SELECT head FROM record WHERE uid = ? AND deleted_at IS NULL")
        .bind(uid)
        .fetch_optional(pool)
        .await
}

/// Loads the body of a live Record.
pub async fn body(pool: &SqlitePool, uid: &str) -> Result<Option<String>, StoreError> {
    let _read = Span::new("record-body-read-await");
    sqlx::query_scalar("SELECT body FROM record WHERE uid = ? AND deleted_at IS NULL")
        .bind(uid)
        .fetch_optional(pool)
        .await
}

/// Lists live UIDs and optional slugs in creation order.
pub async fn live_identifiers(
    pool: &SqlitePool,
) -> Result<Vec<(String, Option<String>)>, StoreError> {
    let _read = Span::new("record-live-identifiers-read-await");
    let rows = sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT uid, slug FROM record WHERE deleted_at IS NULL ORDER BY created_at, uid",
    )
    .fetch_all(pool)
    .await?;
    add("record-live-identifiers-read", rows.len() as u64);
    Ok(rows)
}

/// Finds current slugs for the supplied live Record UIDs.
pub async fn live_slugs_for(
    pool: &SqlitePool,
    uids: &[&str],
) -> Result<HashMap<String, String>, StoreError> {
    let _read = Span::new("record-live-slugs-read-await");
    if uids.is_empty() {
        return Ok(HashMap::new());
    }
    let ids = to_string(uids).map_err(|error| protocol(error.to_string()))?;
    Ok(sqlx::query_as::<_, (String, String)>(
        "SELECT uid, slug FROM record WHERE deleted_at IS NULL AND slug IS NOT NULL
         AND uid IN (SELECT value FROM json_each(?))",
    )
    .bind(ids)
    .fetch_all(pool)
    .await?
    .into_iter()
    .collect())
}

/// The Record metadata needed to route an operation.
pub struct RecordRouting {
    pub kind: String,
    pub organ_uid: Option<String>,
    pub file_origin: Option<String>,
}

/// Loads routing metadata for a live Record without loading its body.
pub async fn routing(pool: &SqlitePool, uid: &str) -> Result<Option<RecordRouting>, StoreError> {
    let row: Option<(String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT kind, organ_uid, file_origin FROM record WHERE uid = ? AND deleted_at IS NULL",
    )
    .bind(uid)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(kind, organ_uid, file_origin)| RecordRouting {
        kind,
        organ_uid,
        file_origin,
    }))
}

/// Finds the owning Organ of a live Record, if assigned.
pub async fn organ_uid(pool: &SqlitePool, uid: &str) -> Result<Option<String>, StoreError> {
    Ok(
        sqlx::query_scalar("SELECT organ_uid FROM record WHERE uid = ? AND deleted_at IS NULL")
            .bind(uid)
            .fetch_optional(pool)
            .await?
            .flatten(),
    )
}

/// Finds a live Record UID by its slug or UID.
pub async fn resolve_uid(pool: &SqlitePool, token: &str) -> Result<Option<String>, StoreError> {
    sqlx::query_scalar(
        "SELECT r.uid FROM record r
         WHERE (r.slug = ? OR r.uid = ?) AND r.deleted_at IS NULL LIMIT 1",
    )
    .bind(token)
    .bind(token)
    .fetch_optional(pool)
    .await
}

/// The UID and kind returned by a lightweight Record lookup.
pub struct ResolvedRecord {
    pub uid: String,
    pub kind: String,
}

/// Finds a live Record UID and kind by slug or UID.
pub async fn resolve_kind(
    pool: &SqlitePool,
    token: &str,
) -> Result<Option<ResolvedRecord>, StoreError> {
    let _read = Span::new("record-reference-kind-read-await");
    add("record-reference-kind-reads", 1);
    let row = sqlx::query_as::<_, (String, String)>(
        "SELECT r.uid, r.kind FROM record r
         WHERE (r.slug = ? OR r.uid = ?) AND r.deleted_at IS NULL LIMIT 1",
    )
    .bind(token)
    .bind(token)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(uid, kind)| ResolvedRecord { uid, kind }))
}

/// Loads a live Record and its body by slug or UID.
pub async fn resolve(pool: &SqlitePool, token: &str) -> Result<Option<RecordRow>, StoreError> {
    sqlx::query(
        "SELECT r.*,
                (SELECT predicate_uid FROM record_assertion a
                  WHERE a.subject_uid = r.uid AND a.role = 'identity'
                    AND a.retracted_at IS NULL) AS identity_predicate_uid
           FROM record r
          WHERE (r.slug = ? OR r.uid = ?) AND r.deleted_at IS NULL LIMIT 1",
    )
    .bind(token)
    .bind(token)
    .fetch_optional(pool)
    .await?
    .map(map_row)
    .transpose()
}

/// Reads a live Record's original creation timestamp.
pub async fn created_at(pool: &SqlitePool, uid: &str) -> Result<Option<String>, StoreError> {
    Ok(
        sqlx::query("SELECT created_at FROM record WHERE uid = ? AND deleted_at IS NULL")
            .bind(uid)
            .fetch_optional(pool)
            .await?
            .map(|r| r.get::<String, _>("created_at")),
    )
}

/// Lists live Records and their bodies in creation order, breaking ties by UID.
pub async fn list_all(pool: &SqlitePool) -> Result<Vec<RecordRow>, StoreError> {
    map_rows(
        sqlx::query(
            "SELECT r.*,
                    (SELECT predicate_uid FROM record_assertion a
                      WHERE a.subject_uid = r.uid AND a.role = 'identity'
                        AND a.retracted_at IS NULL) AS identity_predicate_uid
               FROM record r WHERE r.deleted_at IS NULL ORDER BY r.created_at, r.uid",
        )
        .fetch_all(pool)
        .await?,
    )
}

/// Reads up to 10,000 live Records by UID in creation order, breaking ties by UID.
pub async fn get_many(pool: &SqlitePool, uids: &[&str]) -> Result<Vec<RecordRow>, StoreError> {
    let ids = selected_uids(uids)?;
    map_rows(
        sqlx::query(
            "SELECT r.*,
                    (SELECT predicate_uid FROM record_assertion a
                      WHERE a.subject_uid=r.uid AND a.role='identity'
                        AND a.retracted_at IS NULL) AS identity_predicate_uid
               FROM record r WHERE r.deleted_at IS NULL
                AND r.uid IN (SELECT value FROM json_each(?))
              ORDER BY r.created_at,r.uid",
        )
        .bind(ids)
        .fetch_all(pool)
        .await?,
    )
}

/// Lists live plain Records with negative quantities in creation order.
pub async fn active_needs(pool: &SqlitePool) -> Result<Vec<RecordRow>, StoreError> {
    map_rows(
        sqlx::query(
            "SELECT r.*,
                    (SELECT predicate_uid FROM record_assertion a
                      WHERE a.subject_uid = r.uid AND a.role = 'identity'
                        AND a.retracted_at IS NULL) AS identity_predicate_uid
               FROM record r
              WHERE r.quantity_mantissa LIKE '-%' AND r.kind = 'plain'
                AND r.deleted_at IS NULL
              ORDER BY r.created_at, r.uid",
        )
        .fetch_all(pool)
        .await?,
    )
}

/// Loads titles by UID, including titles of deleted Records.
pub async fn heads_for(
    pool: &SqlitePool,
    uids: &[String],
) -> Result<HashMap<String, String>, StoreError> {
    selected_heads(
        pool,
        uids,
        "SELECT uid, head FROM record WHERE uid IN (SELECT value FROM json_each(?))",
    )
    .await
}

/// Loads titles by UID for live Records.
pub async fn live_heads_for(
    pool: &SqlitePool,
    uids: &[&str],
) -> Result<HashMap<String, String>, StoreError> {
    selected_heads(
        pool,
        uids,
        "SELECT uid, head FROM record WHERE deleted_at IS NULL AND uid IN (SELECT value FROM json_each(?))",
    )
    .await
}

async fn selected_heads(
    pool: &SqlitePool,
    uids: &[impl serde::Serialize],
    sql: &'static str,
) -> Result<HashMap<String, String>, StoreError> {
    if uids.is_empty() {
        return Ok(HashMap::new());
    }
    let ids = to_string(uids).map_err(|error| protocol(error.to_string()))?;
    Ok(sqlx::query(sql)
        .bind(ids)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| (row.get("uid"), row.get("head")))
        .collect())
}

fn selected_uids(uids: &[&str]) -> Result<String, StoreError> {
    if uids.len() > 10_000 {
        return Err(protocol("Choose at most 10000 Record identities"));
    }
    to_string(uids).map_err(|error| protocol(error.to_string()))
}
