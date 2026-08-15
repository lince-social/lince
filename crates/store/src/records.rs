//! Record repository. NOTE: `record.quantity` is written ONLY by the engine's
//! fact appender (`bump_quantity` below is called inside that transaction and
//! nowhere else — blueprint 0.3).

use chrono::Utc;
use nucleus::{DecimalValue, RecordKind};
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

use crate::StoreError;
use crate::exact::{decimal_columns, read_decimal};
use crate::sync_ops::OpKind;

/// Log one local `set` op on the `record` table (Ontology §11 op log).
async fn log_set(
    pool: &SqlitePool,
    uid: &str,
    field: &str,
    value: serde_json::Value,
) -> Result<(), StoreError> {
    crate::sync_ops::log_local(
        pool,
        "record",
        uid,
        field,
        OpKind::Set,
        Some(value.to_string()),
    )
    .await
}

#[derive(Debug, Clone)]
pub struct RecordRow {
    pub uid: String,
    pub slug: Option<String>,
    pub kind: String,
    pub head: String,
    pub body: String,
    /// The cache of this record's fact fold — exact, so it can never disagree
    /// with its chain by a rounding step (blueprint E0.0).
    pub quantity: DecimalValue,
    pub identity_predicate_uid: Option<String>,
    pub unit_uid: Option<String>,
    pub place_uid: Option<String>,
    /// The organ (a `kind='organ'` record) this record originated from —
    /// `None` means "no known origin" (e.g. created before this column, or
    /// never stamped). Lets Protein filter records by organ (`organ_eq` /
    /// `organ_in`) and lets Sync/File Sync select WHAT travels where by
    /// pointing at a Protein instead of a hardcoded rule.
    pub organ_uid: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    /// Creation order that does not depend on any machine's clock. `None` for
    /// rows written before this column existed — sort those LAST rather than
    /// treating the absence as a time.
    pub created_hlc: Option<i64>,
}

impl RecordRow {
    /// Lossy view of the quantity for display, charts and legacy float math.
    /// Never write this back to the Ledger.
    pub fn quantity_f64(&self) -> f64 {
        self.quantity.to_f64()
    }

    /// `quantity != 0` — the universal activation knob on non-plain kinds.
    pub fn is_active(&self) -> bool {
        !self.quantity.is_zero()
    }
}

fn map_row(r: sqlx::sqlite::SqliteRow) -> Result<RecordRow, StoreError> {
    Ok(RecordRow {
        uid: r.get("uid"),
        slug: r.get("slug"),
        kind: r.get("kind"),
        head: r.get("head"),
        body: r.get("body"),
        quantity: read_decimal(&r, "quantity")?,
        identity_predicate_uid: r.get("identity_predicate_uid"),
        unit_uid: r.get("unit_uid"),
        place_uid: r.get("place_uid"),
        organ_uid: r.get("organ_uid"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
        created_hlc: r.try_get("created_hlc").ok().flatten(),
    })
}

fn map_rows(rows: Vec<sqlx::sqlite::SqliteRow>) -> Result<Vec<RecordRow>, StoreError> {
    rows.into_iter().map(map_row).collect()
}

pub struct NewRecord<'a> {
    pub slug: Option<&'a str>,
    pub kind: RecordKind,
    pub head: &'a str,
    pub body: &'a str,
    pub quantity: DecimalValue,
}

pub async fn create(pool: &SqlitePool, new: NewRecord<'_>) -> Result<RecordRow, StoreError> {
    create_in_root(pool, new, None).await
}

/// Create a Record inside an individual-replica root (Ontology §11 "Threads").
///
/// `root` is written in the SAME INSERT as the row, deliberately: every
/// `log_set` below resolves the op's `replica_root` from this column, so a
/// Record that got its root a moment later would have already logged ops onto
/// the GENERAL feed — and those ops are served to any `sync_in` contact on
/// their next catch-up. Stamping at creation is what closes that window, and
/// it is why there is no "make this existing Record private" call here.
///
/// Deferred, and not solvable by adding one: adopting an ARBITRARY existing
/// Record into a root. It needs a backfill decision for ops already logged and
/// a UI that says plainly that already-sent ops cannot be un-sent. Until then
/// the only way into a root is to be born in one.
pub async fn create_in_root(
    pool: &SqlitePool,
    new: NewRecord<'_>,
    root: Option<&str>,
) -> Result<RecordRow, StoreError> {
    if let Some(slug) = new.slug {
        if !nucleus::valid_slug(slug) {
            return Err(sqlx::Error::Protocol(format!("invalid slug `{slug}`")));
        }
    }
    let uid = nucleus::new_uid("r");
    let now = Utc::now().to_rfc3339();
    // One stamp for the record, taken here rather than derived from its ops:
    // `log_local` mints an HLC per FIELD, so "the record's HLC" would otherwise
    // be several different values. This is creation order, and like
    // `replica_root` it is written once and never changes.
    let created_hlc = nucleus::hlc::next();
    let (mantissa, scale) = decimal_columns(new.quantity);
    // The origin Organ, IN the insert. It used to be stamped by a separate
    // UPDATE below, which left every new Record briefly unattributable and —
    // when no local Organ existed — permanently so. `Store::open` mints the
    // identity, so the `None` arm is unreachable; it is an error rather than a
    // silent skip because a Record with no origin can no longer be stored.
    let origin = crate::organs::local(pool)
        .await?
        .map(|organ| organ.uid)
        .ok_or_else(|| {
            sqlx::Error::Protocol("cannot create a Record before this Cell has an Organ".into())
        })?;
    sqlx::query(
        "INSERT INTO record (uid, slug, kind, head, body, quantity_mantissa, quantity_scale,
                             organ_uid, created_at, updated_at, replica_root, created_hlc)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&uid)
    .bind(new.slug)
    .bind(new.kind.as_str())
    .bind(new.head)
    .bind(new.body)
    .bind(&mantissa)
    .bind(scale)
    .bind(&origin)
    .bind(&now)
    .bind(&now)
    .bind(root)
    .bind(&created_hlc)
    .execute(pool)
    .await?;
    log_set(pool, &uid, "kind", serde_json::json!(new.kind.as_str())).await?;
    log_set(pool, &uid, "head", serde_json::json!(new.head)).await?;
    log_set(pool, &uid, "body", serde_json::json!(new.body)).await?;
    log_set(
        pool,
        &uid,
        "quantity",
        serde_json::json!({ "mantissa": mantissa, "scale": scale }),
    )
    .await?;
    if let Some(slug) = new.slug {
        log_set(pool, &uid, "slug", serde_json::json!(slug)).await?;
    }
    // The column is written by the INSERT now; the op is still logged so
    // contacts learn the origin from the feed like every other field.
    log_set(pool, &uid, "organ_uid", serde_json::json!(&origin)).await?;
    get(pool, &uid).await.map(|r| r.expect("just inserted"))
}

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

/// Resolve `@token`: slug first, uid fallback.
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

pub async fn quantity(pool: &SqlitePool, uid: &str) -> Result<Option<DecimalValue>, StoreError> {
    sqlx::query(
        "SELECT quantity_mantissa, quantity_scale FROM record
          WHERE uid = ? AND deleted_at IS NULL",
    )
    .bind(uid)
    .fetch_optional(pool)
    .await?
    .map(|r| read_decimal(&r, "quantity"))
    .transpose()
}

/// Read a live Record's exact quantity while participating in a larger write
/// transaction (for example, a Kanban state transition that also changes
/// assertions). Keeping this beside `quantity` prevents callers from opening a
/// second connection and observing a different level mid-transition.
pub async fn quantity_in_transaction(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
) -> Result<Option<DecimalValue>, StoreError> {
    sqlx::query(
        "SELECT quantity_mantissa, quantity_scale FROM record
          WHERE uid = ? AND deleted_at IS NULL",
    )
    .bind(uid)
    .fetch_optional(&mut **tx)
    .await?
    .map(|row| read_decimal(&row, "quantity"))
    .transpose()
}

/// ISO timestamp a record was created — threads/messages surface this so a
/// Record-style UI can show "when" without RecordRow carrying it
/// everywhere (most callers never need it).
pub async fn created_at(pool: &SqlitePool, uid: &str) -> Result<Option<String>, StoreError> {
    Ok(
        sqlx::query("SELECT created_at FROM record WHERE uid = ? AND deleted_at IS NULL")
            .bind(uid)
            .fetch_optional(pool)
            .await?
            .map(|r| r.get::<String, _>("created_at")),
    )
}

/// HARD delete = tombstone (2026-07-17), DISTINCT from `deactivate` (quantity
/// -> 0). The row stays (uids/provenance stay resolvable in the Ledger's
/// history) but no read path returns it again; the UNIQUE slug is freed for
/// reuse. Facts are never touched — the hash chain stays verifiable.
pub async fn mark_deleted(pool: &SqlitePool, uid: &str) -> Result<bool, StoreError> {
    let now = Utc::now().to_rfc3339();
    let res = sqlx::query(
        "UPDATE record SET deleted_at = ?, slug = NULL, updated_at = ?
         WHERE uid = ? AND deleted_at IS NULL",
    )
    .bind(&now)
    .bind(&now)
    .bind(uid)
    .execute(pool)
    .await?;
    let deleted = res.rows_affected() > 0;
    if deleted {
        crate::sync_ops::log_local(pool, "record", uid, "", OpKind::Tombstone, None).await?;
    }
    Ok(deleted)
}

/// Every record, oldest first — the Protein `source: record` base set.
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

/// Active Needs in stable tie-break order (oldest first) — the focus-queue
/// candidate set (blueprint Window 1b). Window-based urgency joins in later
/// with Promises; created_at is the final tie-break already.
pub async fn active_needs(pool: &SqlitePool) -> Result<Vec<RecordRow>, StoreError> {
    // A canonical mantissa carries its own sign, so "is negative" is an exact
    // text test — there is no numeric column left to compare against 0.
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

/// (uid, quantity) of every record — checkpoint sweep input (blueprint II.2).
pub async fn all_levels(pool: &SqlitePool) -> Result<Vec<(String, DecimalValue)>, StoreError> {
    sqlx::query(
        "SELECT uid, quantity_mantissa, quantity_scale FROM record WHERE deleted_at IS NULL",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|r| Ok((r.get("uid"), read_decimal(&r, "quantity")?)))
    .collect()
}

/// Namespaced fds sidecar (blueprint I.2) — also where saved Proteins live
/// (`namespace = "lince.protein"`).
/// Write an extension WITHOUT logging an op, for a projection that every
/// Cell derives for itself rather than replicating.
///
/// The roster mirror is the case that forced this: it is display state
/// derived from a signed blob both sides already hold, and as a logged write
/// it locked a relay Cell out of publishing its own roster — the relay has no
/// write capability, so the mirror was refused and the whole publish failed.
/// A projection nobody needs to receive should not be an op in the first
/// place.
pub async fn set_extension_raw(
    pool: &SqlitePool,
    record_uid: &str,
    namespace: &str,
    fds: &serde_json::Value,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO record_extension (record_uid, namespace, fds) VALUES (?, ?, ?)
         ON CONFLICT(record_uid, namespace)
         DO UPDATE SET fds = excluded.fds, version = version + 1",
    )
    .bind(record_uid)
    .bind(namespace)
    .bind(fds.to_string())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn set_extension(
    pool: &SqlitePool,
    record_uid: &str,
    namespace: &str,
    fds: &serde_json::Value,
) -> Result<(), StoreError> {
    let old = get_extension(pool, record_uid, namespace).await?;
    sqlx::query(
        "INSERT INTO record_extension (record_uid, namespace, fds) VALUES (?, ?, ?)
         ON CONFLICT(record_uid, namespace)
         DO UPDATE SET fds = excluded.fds, version = version + 1",
    )
    .bind(record_uid)
    .bind(namespace)
    .bind(fds.to_string())
    .execute(pool)
    .await?;
    // Ops are per KEY inside the namespace, so two Cells editing different
    // keys of one namespace never clobber each other's whole blob.
    let empty = serde_json::Map::new();
    match (old.as_ref().and_then(|v| v.as_object()), fds.as_object()) {
        (old_map, Some(new_map)) => {
            let old_map = old_map.unwrap_or(&empty);
            for (key, value) in new_map {
                if old_map.get(key) != Some(value) {
                    crate::sync_ops::log_local(
                        pool,
                        "record_extension",
                        record_uid,
                        &format!("{namespace}.{key}"),
                        OpKind::Set,
                        Some(value.to_string()),
                    )
                    .await?;
                }
            }
            for key in old_map.keys() {
                if !new_map.contains_key(key) {
                    crate::sync_ops::log_local(
                        pool,
                        "record_extension",
                        record_uid,
                        &format!("{namespace}.{key}"),
                        OpKind::Tombstone,
                        None,
                    )
                    .await?;
                }
            }
        }
        // A non-object namespace value replicates as one opaque field.
        (_, None) => {
            crate::sync_ops::log_local(
                pool,
                "record_extension",
                record_uid,
                namespace,
                OpKind::Set,
                Some(fds.to_string()),
            )
            .await?;
        }
    }
    Ok(())
}

/// Drops one namespaced extension row without touching the Record itself —
/// e.g. unpublishing a DNA sand package ("no longer offered") is not the
/// same act as deleting its Record.
pub async fn delete_extension(
    pool: &SqlitePool,
    record_uid: &str,
    namespace: &str,
) -> Result<(), StoreError> {
    sqlx::query("DELETE FROM record_extension WHERE record_uid = ? AND namespace = ?")
        .bind(record_uid)
        .bind(namespace)
        .execute(pool)
        .await?;
    crate::sync_ops::log_local(
        pool,
        "record_extension",
        record_uid,
        namespace,
        OpKind::Tombstone,
        None,
    )
    .await?;
    Ok(())
}

pub async fn get_extension(
    pool: &SqlitePool,
    record_uid: &str,
    namespace: &str,
) -> Result<Option<serde_json::Value>, StoreError> {
    Ok(
        sqlx::query("SELECT fds FROM record_extension WHERE record_uid = ? AND namespace = ?")
            .bind(record_uid)
            .bind(namespace)
            .fetch_optional(pool)
            .await?
            .and_then(|r| serde_json::from_str(&r.get::<String, _>("fds")).ok()),
    )
}

/// Load one extension namespace for every live Record in one query.
/// Protein uses this when a predicate refers to structured Record metadata;
/// keeping it batched avoids one database read per candidate Record.
pub async fn all_extensions(
    pool: &SqlitePool,
    namespace: &str,
) -> Result<std::collections::HashMap<String, serde_json::Value>, StoreError> {
    let rows = sqlx::query(
        "SELECT e.record_uid, e.fds
           FROM record_extension e
           JOIN record r ON r.uid = e.record_uid
          WHERE e.namespace = ? AND r.deleted_at IS NULL",
    )
    .bind(namespace)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let uid = row.get::<String, _>("record_uid");
            serde_json::from_str(&row.get::<String, _>("fds"))
                .ok()
                .map(|value| (uid, value))
        })
        .collect())
}

/// Edit a record's text (head/title and/or body). Not the quantity cache, so a
/// plain `UPDATE` is allowed; provenance/live-refresh is the engine's job via an
/// annotation fact. `None` leaves a field untouched.
///
/// Logs NO ops: text edits sync as `crdt` ops through `engine::collab`
/// (Ontology §11 "Merge") and this fn is their raw materializer. The create
/// path still logs head/body `set` ops so a never-collab-edited record's text
/// travels; import gives crdt history precedence over those.
pub async fn set_text(
    pool: &SqlitePool,
    uid: &str,
    head: Option<&str>,
    body: Option<&str>,
) -> Result<(), StoreError> {
    let now = Utc::now().to_rfc3339();
    let res = sqlx::query(
        "UPDATE record
           SET head = COALESCE(?, head),
               body = COALESCE(?, body),
               updated_at = ?
         WHERE uid = ?",
    )
    .bind(head)
    .bind(body)
    .bind(&now)
    .bind(uid)
    .execute(pool)
    .await?;
    if res.rows_affected() == 0 {
        return Err(sqlx::Error::RowNotFound);
    }
    Ok(())
}

/// Rename a record's slug (uniqueness is enforced by the `record.slug` UNIQUE
/// index; an empty slug clears it).
pub async fn set_slug(pool: &SqlitePool, uid: &str, slug: Option<&str>) -> Result<(), StoreError> {
    if let Some(slug) = slug {
        if !nucleus::valid_slug(slug) {
            return Err(sqlx::Error::Protocol(format!("invalid slug `{slug}`")));
        }
    }
    let now = Utc::now().to_rfc3339();
    let res = sqlx::query("UPDATE record SET slug = ?, updated_at = ? WHERE uid = ?")
        .bind(slug)
        .bind(&now)
        .bind(uid)
        .execute(pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(sqlx::Error::RowNotFound);
    }
    log_set(pool, uid, "slug", serde_json::json!(slug)).await?;
    Ok(())
}

/// Re-stamp the record's origin Organ — carried through Sync, so lineage
/// survives a relaying intermediate.
///
/// No longer clearable. Origin is written by the INSERT now, and a Record with
/// none is a state the schema refuses: `record_origin_required_update` aborts
/// a `None` here rather than letting the front door re-create what the
/// migration deleted.
pub async fn set_organ_origin(
    pool: &SqlitePool,
    uid: &str,
    organ_uid: Option<&str>,
) -> Result<(), StoreError> {
    let now = Utc::now().to_rfc3339();
    let res = sqlx::query("UPDATE record SET organ_uid = ?, updated_at = ? WHERE uid = ?")
        .bind(organ_uid)
        .bind(&now)
        .bind(uid)
        .execute(pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(sqlx::Error::RowNotFound);
    }
    Ok(())
}

/// Set (or clear, with `None`) the record's unit-of-measure concept.
pub async fn set_unit(
    pool: &SqlitePool,
    uid: &str,
    unit_uid: Option<&str>,
) -> Result<(), StoreError> {
    let now = Utc::now().to_rfc3339();
    let res = sqlx::query("UPDATE record SET unit_uid = ?, updated_at = ? WHERE uid = ?")
        .bind(unit_uid)
        .bind(&now)
        .bind(uid)
        .execute(pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(sqlx::Error::RowNotFound);
    }
    log_set(pool, uid, "unit_uid", serde_json::json!(unit_uid)).await?;
    Ok(())
}

/// The single writer of the quantity cache — called only from engine::append
/// inside the fact transaction.
///
/// Exact addition cannot be expressed in SQL over a `(mantissa, scale)` pair,
/// so this reads, adds in Rust as `i128`, and writes back. That is safe
/// precisely because it runs inside the append transaction that already
/// serializes writes to this record. The cache takes the finer of the two
/// scales, which is always `<= 18` — no rounding step can enter here.
pub async fn bump_quantity(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
    delta: DecimalValue,
    now_rfc3339: &str,
) -> Result<(), StoreError> {
    let row = sqlx::query("SELECT quantity_mantissa, quantity_scale FROM record WHERE uid = ?")
        .bind(uid)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    let updated = read_decimal(&row, "quantity")?
        .aligned_add(delta)
        .ok_or_else(|| {
            StoreError::Decode(format!("quantity of {uid} overflows i128 exact range").into())
        })?;
    let (mantissa, scale) = decimal_columns(updated);
    let res = sqlx::query(
        "UPDATE record SET quantity_mantissa = ?, quantity_scale = ?, updated_at = ?
          WHERE uid = ?",
    )
    .bind(mantissa)
    .bind(scale)
    .bind(now_rfc3339)
    .bind(uid)
    .execute(&mut **tx)
    .await?;
    if res.rows_affected() == 0 {
        return Err(sqlx::Error::RowNotFound);
    }
    Ok(())
}
