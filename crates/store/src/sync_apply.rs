//! Raw appliers for REMOTE ops (Ontology §11 "Merge"). These write the read
//! model directly and NEVER log local ops — the import path appends the
//! original op (origin identity) itself. LWW decisions are the engine's; by
//! the time a function here runs, the op has already won its HLC compare.

use chrono::Utc;
use sqlx::{Row, SqlitePool};

use crate::StoreError;

/// Make sure a record row exists for an incoming op (uid is the origin's —
/// cross-organ joins line up by uid; a slug is a local suggestion, dropped on
/// collision).
/// `created_hlc` comes from the op that brought this record into being, NOT
/// from a fresh stamp. A fresh one would order every imported record by when
/// it happened to arrive here, which puts a conversation back on local
/// wall-clock time by another route — the exact thing the column exists to
/// avoid.
pub async fn ensure_record_stub(
    pool: &SqlitePool,
    uid: &str,
    kind: &str,
    organ_uid: &str,
    replica_root: Option<&str>,
    created_hlc: Option<i64>,
) -> Result<(), StoreError> {
    let exists = sqlx::query("SELECT 1 FROM record WHERE uid = ?")
        .bind(uid)
        .fetch_optional(pool)
        .await?
        .is_some();
    if exists {
        return Ok(());
    }
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO record (uid, slug, kind, head, body, quantity_mantissa, quantity_scale,
                             organ_uid, created_at, updated_at, replica_root, created_hlc)
         VALUES (?, NULL, ?, '', '', '0', 0, ?, ?, ?, ?, ?)",
    )
    .bind(uid)
    .bind(if kind.is_empty() { "plain" } else { kind })
    .bind(organ_uid)
    .bind(&now)
    .bind(&now)
    .bind(replica_root)
    .bind(created_hlc)
    .execute(pool)
    .await?;
    Ok(())
}

/// Apply one record `set` op. Unknown fields are ignored (a newer peer may
/// speak columns this Cell does not have yet). `undelete` clears the
/// tombstone — "undelete is a newer write".
pub async fn set_record_field(
    pool: &SqlitePool,
    uid: &str,
    field: &str,
    value: &serde_json::Value,
    undelete: bool,
) -> Result<bool, StoreError> {
    let now = Utc::now().to_rfc3339();
    let text = value.as_str().map(str::to_string);
    let applied = match field {
        "head" | "body" | "kind" => {
            let sql = format!("UPDATE record SET {field} = ?, updated_at = ? WHERE uid = ?");
            sqlx::query(&sql)
                .bind(text.unwrap_or_default())
                .bind(&now)
                .bind(uid)
                .execute(pool)
                .await?
                .rows_affected()
                > 0
        }
        "slug" => {
            // A slug is a local suggestion, never identity — drop on collision.
            let taken = match text.as_deref() {
                Some(slug) => sqlx::query("SELECT 1 FROM record WHERE slug = ? AND uid != ?")
                    .bind(slug)
                    .bind(uid)
                    .fetch_optional(pool)
                    .await?
                    .is_some(),
                None => false,
            };
            sqlx::query("UPDATE record SET slug = ?, updated_at = ? WHERE uid = ?")
                .bind(if taken { None } else { text })
                .bind(&now)
                .bind(uid)
                .execute(pool)
                .await?
                .rows_affected()
                > 0
        }
        "unit_uid" | "organ_uid" => {
            let sql = format!("UPDATE record SET {field} = ?, updated_at = ? WHERE uid = ?");
            sqlx::query(&sql)
                .bind(text)
                .bind(&now)
                .bind(uid)
                .execute(pool)
                .await?
                .rows_affected()
                > 0
        }
        _ => false,
    };
    if applied && undelete {
        sqlx::query("UPDATE record SET deleted_at = NULL WHERE uid = ?")
            .bind(uid)
            .execute(pool)
            .await?;
    }
    Ok(applied)
}

/// Apply a record tombstone: mark deleted, free the slug.
/// Undelete without touching any field — "undelete is a newer write" when the
/// winning write is text owned by the record-doc rather than a scalar set.
pub async fn undelete_record(pool: &SqlitePool, uid: &str) -> Result<(), StoreError> {
    sqlx::query("UPDATE record SET deleted_at = NULL, updated_at = ? WHERE uid = ?")
        .bind(Utc::now().to_rfc3339())
        .bind(uid)
        .execute(pool)
        .await?;
    Ok(())
}

/// Whether a record row is tombstoned: `None` = no row at all, `Some(true)` =
/// deleted, `Some(false)` = alive. Sees THROUGH the deleted_at filter the
/// normal getters apply.
pub async fn record_deleted(pool: &SqlitePool, uid: &str) -> Result<Option<bool>, StoreError> {
    Ok(
        sqlx::query("SELECT deleted_at IS NOT NULL AS gone FROM record WHERE uid = ?")
            .bind(uid)
            .fetch_optional(pool)
            .await?
            .map(|row| row.get::<bool, _>("gone")),
    )
}

/// Materialize a record-doc's text (engine::collab's raw writer): both fields
/// at once, no ops logged, no existence error — the caller ensured the row.
pub async fn set_record_text_raw(
    pool: &SqlitePool,
    uid: &str,
    head: &str,
    body: &str,
) -> Result<(), StoreError> {
    sqlx::query("UPDATE record SET head = ?, body = ?, updated_at = ? WHERE uid = ?")
        .bind(head)
        .bind(body)
        .bind(Utc::now().to_rfc3339())
        .bind(uid)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn tombstone_record(pool: &SqlitePool, uid: &str) -> Result<(), StoreError> {
    let now = Utc::now().to_rfc3339();
    sqlx::query("UPDATE record SET deleted_at = ?, slug = NULL, updated_at = ? WHERE uid = ?")
        .bind(&now)
        .bind(&now)
        .bind(uid)
        .execute(pool)
        .await?;
    Ok(())
}

async fn extension_fds(
    pool: &SqlitePool,
    record_uid: &str,
    namespace: &str,
) -> Result<serde_json::Value, StoreError> {
    Ok(
        sqlx::query("SELECT fds FROM record_extension WHERE record_uid = ? AND namespace = ?")
            .bind(record_uid)
            .bind(namespace)
            .fetch_optional(pool)
            .await?
            .and_then(|r| serde_json::from_str(&r.get::<String, _>("fds")).ok())
            .unwrap_or_else(|| serde_json::json!({})),
    )
}

async fn write_extension_fds(
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

/// Set one key inside a namespace (read-modify-write of the fds object).
pub async fn set_extension_key(
    pool: &SqlitePool,
    record_uid: &str,
    namespace: &str,
    key: &str,
    value: serde_json::Value,
) -> Result<(), StoreError> {
    let mut fds = extension_fds(pool, record_uid, namespace).await?;
    if !fds.is_object() {
        fds = serde_json::json!({});
    }
    fds.as_object_mut()
        .expect("just ensured object")
        .insert(key.to_string(), value);
    write_extension_fds(pool, record_uid, namespace, &fds).await
}

/// Remove one key from a namespace.
pub async fn tombstone_extension_key(
    pool: &SqlitePool,
    record_uid: &str,
    namespace: &str,
    key: &str,
) -> Result<(), StoreError> {
    let mut fds = extension_fds(pool, record_uid, namespace).await?;
    if let Some(map) = fds.as_object_mut() {
        map.remove(key);
    }
    write_extension_fds(pool, record_uid, namespace, &fds).await
}

/// Replace a whole non-object namespace value.
pub async fn set_extension_whole(
    pool: &SqlitePool,
    record_uid: &str,
    namespace: &str,
    value: &serde_json::Value,
) -> Result<(), StoreError> {
    write_extension_fds(pool, record_uid, namespace, value).await
}

/// Apply an assertion `set` op: insert under the origin uid, or un-retract a
/// row that a newer set revives (later HLC wins per uid).
pub async fn upsert_assertion(
    pool: &SqlitePool,
    uid: &str,
    value: &serde_json::Value,
) -> Result<(), StoreError> {
    let s = |k: &str| value.get(k).and_then(|v| v.as_str()).map(str::to_string);
    // An Assertion's predicate is a Concept, and a Concept lives on the
    // GENERAL feed — so an Assertion arriving through an individual-replica
    // grant channel can reference a predicate the receiver has never seen.
    // Without a stub the insert fails the foreign key and the whole
    // conversation refuses to land.
    //
    // The stub carries the uid and no name: naming is the general feed's job
    // and the real Concept overwrites this the moment it arrives. Depending on
    // the general feed to carry it would be wrong in the case that matters —
    // a contact granted ONE conversation and no feed sync at all.
    if let Some(predicate_uid) = s("predicate_uid") {
        sqlx::query(
            "INSERT OR IGNORE INTO concept (uid, canonical_name, created_at)
             VALUES (?, ?, ?)",
        )
        .bind(&predicate_uid)
        .bind(format!("concept:{predicate_uid}"))
        .bind(Utc::now().to_rfc3339())
        .execute(pool)
        .await?;
    }
    let res = sqlx::query(
        "INSERT OR IGNORE INTO record_assertion
           (uid, subject_uid, predicate_uid, object_uid, role,
            quantity_mantissa, quantity_scale, unit_uid, asserted_by, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(uid)
    .bind(s("subject_uid").unwrap_or_default())
    .bind(s("predicate_uid").unwrap_or_default())
    .bind(s("object_uid"))
    .bind(s("role").unwrap_or_else(|| "ordinary".into()))
    .bind(s("quantity_mantissa"))
    .bind(value.get("quantity_scale").and_then(|v| v.as_i64()))
    .bind(s("unit_uid"))
    .bind(s("asserted_by"))
    .bind(s("created_at").unwrap_or_else(|| Utc::now().to_rfc3339()))
    .execute(pool)
    .await?;
    if res.rows_affected() == 0 {
        // Already present: a newer set re-activates it.
        sqlx::query(
            "UPDATE record_assertion SET retracted_at = NULL, retracted_by = NULL WHERE uid = ?",
        )
        .bind(uid)
        .execute(pool)
        .await?;
    }
    Ok(())
}

/// Apply an assertion tombstone.
pub async fn retract_assertion(pool: &SqlitePool, uid: &str) -> Result<(), StoreError> {
    sqlx::query(
        "UPDATE record_assertion SET retracted_at = ? WHERE uid = ? AND retracted_at IS NULL",
    )
    .bind(Utc::now().to_rfc3339())
    .bind(uid)
    .execute(pool)
    .await?;
    Ok(())
}

/// Apply a concept `set`: adopt under the origin uid, or rename. A canonical
/// name held by a DIFFERENT local concept stays local (names are unique;
/// keep ours, skip theirs).
pub async fn upsert_concept(
    pool: &SqlitePool,
    uid: &str,
    canonical_name: &str,
    origin_organ: &str,
) -> Result<(), StoreError> {
    let name_holder: Option<String> =
        sqlx::query("SELECT uid FROM concept WHERE canonical_name = ?")
            .bind(canonical_name)
            .fetch_optional(pool)
            .await?
            .map(|r| r.get("uid"));
    if let Some(holder) = name_holder {
        if holder != uid {
            return Ok(());
        }
    }
    let res = sqlx::query(
        "INSERT OR IGNORE INTO concept (uid, canonical_name, origin_organ, created_at)
         VALUES (?, ?, ?, ?)",
    )
    .bind(uid)
    .bind(canonical_name)
    .bind(origin_organ)
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    if res.rows_affected() == 0 {
        sqlx::query("UPDATE concept SET canonical_name = ? WHERE uid = ?")
            .bind(canonical_name)
            .bind(uid)
            .execute(pool)
            .await?;
    }
    Ok(())
}

/// Apply a concept tombstone: the same cascade as `concepts::delete`, without
/// local op logging.
pub async fn delete_concept(pool: &SqlitePool, uid: &str) -> Result<(), StoreError> {
    let mut tx = pool.begin().await?;
    for sql in [
        "DELETE FROM concept_name WHERE concept_uid = ?",
        "DELETE FROM lingua_concept WHERE concept_uid = ?",
    ] {
        sqlx::query(sql).bind(uid).execute(&mut *tx).await?;
    }
    sqlx::query("DELETE FROM concept_parent WHERE concept_uid = ? OR parent_uid = ?")
        .bind(uid)
        .bind(uid)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM concept_equivalence WHERE a_uid = ? OR b_uid = ?")
        .bind(uid)
        .bind(uid)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM concept WHERE uid = ?")
        .bind(uid)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}
