use std::borrow::Cow;

use nucleus::execution::now;
use serde_json::Value;
use sqlx::{Sqlite, SqlitePool, Transaction, query, query_as, query_scalar};

use super::{Stamp, stamped};
use crate::{StoreError, write_tx};

/// Applies a replicated Assertion and updates the ordinary tuple projection.
///
/// # Errors
/// Refuses an invalid or changed definition revision and propagates SQLite failures.
pub async fn upsert_assertion(
    pool: &SqlitePool,
    uid: &str,
    value: &Value,
    stamp: Stamp<'_>,
) -> Result<(), StoreError> {
    let mut tx = write_tx(pool).await?;
    if stamp.superseded(&mut tx).await? {
        return Ok(());
    }
    let text = |key: &str| value.get(key).and_then(Value::as_str);
    let number = |key: &str| value.get(key).and_then(Value::as_i64);
    let predicate_revision = number("predicate_revision").unwrap_or(0);
    let unit_revision = number("unit_revision");
    if !(0..=256).contains(&predicate_revision)
        || unit_revision.is_some_and(|revision| !(1..=256).contains(&revision))
    {
        return Err(StoreError::Protocol(
            "Invalid Assertion definition version".into(),
        ));
    }
    let unit = text("unit_uid");
    let existing: Option<(i64, Option<String>, Option<i64>)> = query_as(
        "SELECT predicate_revision,unit_uid,unit_revision FROM record_assertion WHERE uid=?",
    )
    .bind(uid)
    .fetch_optional(&mut *tx)
    .await?;
    if existing.is_some_and(|(revision, saved_unit, saved_revision)| {
        predicate_revision > 0 && predicate_revision != revision
            || saved_unit.as_deref() == unit
                && unit_revision.is_some()
                && unit_revision != saved_revision
    }) {
        return Err(StoreError::Protocol(
            "An Assertion cannot replace its saved definition version".into(),
        ));
    }
    let predicate = text("predicate_uid");
    if let Some(predicate) = predicate {
        ensure_predicate(&mut tx, predicate, text("predicate_name")).await?;
    }
    let subject = text("subject_uid").unwrap_or_default();
    let predicate = predicate.unwrap_or_default();
    let object = text("object_uid");
    let role = text("role").unwrap_or("ordinary");
    let mantissa = text("quantity_mantissa");
    let scale = number("quantity_scale");
    let author = text("asserted_by");
    let created = text("created_at")
        .map(Cow::Borrowed)
        .unwrap_or_else(|| Cow::Owned(now().to_rfc3339()));
    if role == "ordinary" {
        let uri = text("object_uri");
        query(
            "INSERT INTO record_assertion
                (uid, subject_uid, predicate_uid, object_uid, object_uri, role,
                 quantity_mantissa, quantity_scale, unit_uid, asserted_by, created_at,
                 retracted_at, predicate_revision, unit_revision)
             VALUES (?, ?, ?, ?, ?, 'ordinary', ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(uid) DO UPDATE SET
                 quantity_mantissa = excluded.quantity_mantissa,
                 quantity_scale = excluded.quantity_scale,
                 unit_revision = CASE WHEN record_assertion.unit_uid IS excluded.unit_uid
                                      THEN record_assertion.unit_revision ELSE excluded.unit_revision END,
                 unit_uid = excluded.unit_uid",
        )
        .bind(uid)
        .bind(subject)
        .bind(predicate)
        .bind(object)
        .bind(uri)
        .bind(mantissa)
        .bind(scale)
        .bind(unit)
        .bind(author)
        .bind(created.as_ref())
        .bind(now().to_rfc3339())
        .bind(predicate_revision)
        .bind(unit_revision)
        .execute(&mut *tx)
        .await?;
        project_ordinary_assertion(&mut tx, subject, predicate, object, uri).await?;
    } else {
        let inserted = query(
            "INSERT OR IGNORE INTO record_assertion
               (uid, subject_uid, predicate_uid, object_uid, role,
                quantity_mantissa, quantity_scale, unit_uid, asserted_by, created_at,
                predicate_revision, unit_revision)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(uid)
        .bind(subject)
        .bind(predicate)
        .bind(object)
        .bind(role)
        .bind(mantissa)
        .bind(scale)
        .bind(unit)
        .bind(author)
        .bind(created.as_ref())
        .bind(predicate_revision)
        .bind(unit_revision)
        .execute(&mut *tx)
        .await?
        .rows_affected()
            > 0;
        if !inserted {
            clear_retraction(&mut tx, uid).await?;
        }
    }
    tx.commit().await
}

async fn ensure_predicate(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
    name: Option<&str>,
) -> Result<(), StoreError> {
    let placeholder = format!("concept:{uid}");
    let name = name
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(&placeholder);
    query(
        "INSERT OR IGNORE INTO concept (uid, canonical_name, created_at)
         VALUES (?, ?, ?)",
    )
    .bind(uid)
    .bind(name)
    .bind(now().to_rfc3339())
    .execute(&mut **tx)
    .await?;
    if name != placeholder {
        query(
            "UPDATE concept SET canonical_name = ?
              WHERE uid = ? AND canonical_name = ?",
        )
        .bind(name)
        .bind(uid)
        .bind(placeholder)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

/// Applies a replica retraction and refreshes the winner of its ordinary tuple.
pub async fn retract_assertion(
    pool: &SqlitePool,
    uid: &str,
    stamp: Stamp<'_>,
) -> Result<(), StoreError> {
    stamp
        .bind(
            query(stamped!(
                "UPDATE record_assertion SET retracted_at = ?
              WHERE uid = ? AND retracted_at IS NULL"
            ))
            .bind(now().to_rfc3339())
            .bind(uid),
        )
        .execute(pool)
        .await?;
    let tuple: Option<(String, String, Option<String>, Option<String>)> = query_as(
        "SELECT subject_uid, predicate_uid, object_uid, object_uri
           FROM record_assertion WHERE uid = ? AND role = 'ordinary'",
    )
    .bind(uid)
    .fetch_optional(pool)
    .await?;
    if let Some((subject, predicate, object, uri)) = tuple {
        let mut tx = write_tx(pool).await?;
        project_ordinary_assertion(
            &mut tx,
            &subject,
            &predicate,
            object.as_deref(),
            uri.as_deref(),
        )
        .await?;
        tx.commit().await?;
    }
    Ok(())
}

async fn project_ordinary_assertion(
    tx: &mut Transaction<'_, Sqlite>,
    subject: &str,
    predicate: &str,
    object: Option<&str>,
    uri: Option<&str>,
) -> Result<(), StoreError> {
    let winner: Option<String> = query_scalar(
        "SELECT a.uid FROM record_assertion a
          WHERE a.subject_uid = ? AND a.predicate_uid = ? AND a.object_uid IS ? AND a.object_uri IS ?
            AND a.role = 'ordinary' AND NOT EXISTS (
                SELECT 1 FROM sync_op o
                 WHERE o.tbl = 'record_assertion' AND o.uid = a.uid AND o.kind = 'tombstone')
          ORDER BY a.uid DESC LIMIT 1",
    )
    .bind(subject)
    .bind(predicate)
    .bind(object)
    .bind(uri)
    .fetch_optional(&mut **tx)
    .await?;
    query(
        "UPDATE record_assertion SET retracted_at = COALESCE(retracted_at, ?)
          WHERE subject_uid = ? AND predicate_uid = ? AND object_uid IS ? AND object_uri IS ?
            AND role = 'ordinary'",
    )
    .bind(now().to_rfc3339())
    .bind(subject)
    .bind(predicate)
    .bind(object)
    .bind(uri)
    .execute(&mut **tx)
    .await?;
    if let Some(winner) = winner {
        let identity: bool = query_scalar(
            "SELECT EXISTS(SELECT 1 FROM record_assertion
              WHERE subject_uid = ? AND predicate_uid = ? AND object_uid IS ? AND object_uri IS ?
                AND role = 'identity' AND retracted_at IS NULL)",
        )
        .bind(subject)
        .bind(predicate)
        .bind(object)
        .bind(uri)
        .fetch_one(&mut **tx)
        .await?;
        if !identity {
            clear_retraction(tx, &winner).await?;
        }
    }
    Ok(())
}

async fn clear_retraction(tx: &mut Transaction<'_, Sqlite>, uid: &str) -> Result<(), StoreError> {
    query("UPDATE record_assertion SET retracted_at = NULL, retracted_by = NULL WHERE uid = ?")
        .bind(uid)
        .execute(&mut **tx)
        .await?;
    Ok(())
}
