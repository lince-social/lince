use std::collections::HashSet;

use nucleus::{DecimalValue, execution::now, new_uid};
use serde_json::Value;
use sqlx::{Row, Sqlite, SqlitePool, Transaction, query};

use super::{
    AssertionQuantity, AssertionRole, NewAssertion, binary_of_predicate,
    model::{assertion_op_fields, quantity_columns},
    promote_identity_tx,
};
use crate::{
    StoreError,
    sync_ops::{OpKind, log_local, log_local_tx},
    write_tx,
};

/// Preserved Assertion identity and provenance from an import.
pub struct ImportedAssertion<'a> {
    pub uid: &'a str,
    pub subject_uid: &'a str,
    pub predicate_uid: &'a str,
    pub object_uid: Option<&'a str>,
    pub quantity: Option<DecimalValue>,
    pub unit_uid: Option<&'a str>,
    pub asserted_by: Option<&'a str>,
    pub created_at: &'a str,
}

/// Inserts an imported ordinary Assertion if its UID and tuple are available.
pub async fn import_active(
    pool: &SqlitePool,
    imported: ImportedAssertion<'_>,
) -> Result<(), StoreError> {
    let (mantissa, scale) = quantity_columns(imported.quantity);
    query(
        "INSERT OR IGNORE INTO record_assertion
           (uid, subject_uid, predicate_uid, object_uid, role,
            quantity_mantissa, quantity_scale, unit_uid, asserted_by, created_at)
         VALUES (?, ?, ?, ?, 'ordinary', ?, ?, ?, ?, ?)",
    )
    .bind(imported.uid)
    .bind(imported.subject_uid)
    .bind(imported.predicate_uid)
    .bind(imported.object_uid)
    .bind(mantissa)
    .bind(scale)
    .bind(imported.unit_uid)
    .bind(imported.asserted_by)
    .bind(imported.created_at)
    .execute(pool)
    .await?;
    Ok(())
}

/// Ensures an active tuple exists and logs a new Assertion when needed.
///
/// # Errors
/// Rejects invalid identity shapes or forbidden replica links and propagates
/// database and sync errors.
pub async fn assert(pool: &SqlitePool, new: NewAssertion<'_>) -> Result<String, StoreError> {
    if new.role == AssertionRole::Identity
        && (new.object_uid.is_some() || new.quantity.is_some() || new.unit_uid.is_some())
    {
        return Err(sqlx::Error::Protocol(
            "an identity assertion must be unary and unquantified".into(),
        ));
    }
    if let Some(object_uid) = new.object_uid {
        let subject_root = crate::replica::root_of(pool, new.subject_uid).await?;
        let object_root = crate::replica::root_of(pool, object_uid).await?;
        match (subject_root, object_root) {
            (Some(subject), Some(object)) if subject != object => {
                return Err(sqlx::Error::Protocol(
                    "an assertion cannot cross an individual-replica boundary".into(),
                ));
            }
            (None, Some(_)) => {
                return Err(sqlx::Error::Protocol(
                    "an assertion cannot put a private record on the general feed".into(),
                ));
            }
            _ => {}
        }
    }
    let existing = query(
        "SELECT uid FROM record_assertion
          WHERE subject_uid = ? AND predicate_uid = ? AND object_uid IS ? AND object_uri IS NULL
            AND retracted_at IS NULL",
    )
    .bind(new.subject_uid)
    .bind(new.predicate_uid)
    .bind(new.object_uid)
    .fetch_optional(pool)
    .await?;
    if let Some(existing) = existing {
        return Ok(existing.get("uid"));
    }
    let uid = new_uid("a");
    let now = now().to_rfc3339();
    let (mantissa, scale) = quantity_columns(new.quantity);
    query(
        "INSERT INTO record_assertion
           (uid, subject_uid, predicate_uid, object_uid, role,
            quantity_mantissa, quantity_scale, unit_uid, asserted_by, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&uid)
    .bind(new.subject_uid)
    .bind(new.predicate_uid)
    .bind(new.object_uid)
    .bind(new.role.as_str())
    .bind(mantissa)
    .bind(scale)
    .bind(new.unit_uid)
    .bind(new.asserted_by)
    .bind(&now)
    .execute(pool)
    .await?;
    log_local(
        pool,
        "record_assertion",
        &uid,
        "",
        OpKind::Set,
        Some(
            Value::Object(assertion_op_fields(
                new.subject_uid,
                new.predicate_uid,
                new.object_uid,
                new.role.as_str(),
                AssertionQuantity {
                    quantity: new.quantity,
                    unit_uid: new.unit_uid,
                },
                new.asserted_by,
                &now,
            ))
            .to_string(),
        ),
    )
    .await?;
    Ok(uid)
}

/// Retracts an active Assertion by UID and logs a tombstone when changed.
pub async fn retract(
    pool: &SqlitePool,
    uid: &str,
    actor_uid: Option<&str>,
) -> Result<bool, StoreError> {
    let retracted = query(
        "UPDATE record_assertion SET retracted_at = ?, retracted_by = ?
          WHERE uid = ? AND retracted_at IS NULL",
    )
    .bind(now().to_rfc3339())
    .bind(actor_uid)
    .bind(uid)
    .execute(pool)
    .await?
    .rows_affected()
        > 0;
    if retracted {
        log_local(pool, "record_assertion", uid, "", OpKind::Tombstone, None).await?;
    }
    Ok(retracted)
}

/// Sets or clears a Record's identity predicate and commits the changes.
pub async fn set_identity(
    pool: &SqlitePool,
    subject_uid: &str,
    predicate_uid: Option<&str>,
    actor_uid: Option<&str>,
) -> Result<Option<String>, StoreError> {
    let mut tx = write_tx(pool).await?;
    let result = set_identity_tx(&mut tx, subject_uid, predicate_uid, actor_uid).await?;
    tx.commit().await?;
    Ok(result)
}

/// Sets or clears a Record's identity predicate in the caller's transaction.
pub async fn set_identity_tx(
    tx: &mut Transaction<'_, Sqlite>,
    subject_uid: &str,
    predicate_uid: Option<&str>,
    actor_uid: Option<&str>,
) -> Result<Option<String>, StoreError> {
    let now = now().to_rfc3339();
    let displaced: Vec<String> = query(
        "SELECT uid FROM record_assertion
          WHERE subject_uid = ? AND role = 'identity' AND retracted_at IS NULL
            AND predicate_uid IS NOT ?",
    )
    .bind(subject_uid)
    .bind(predicate_uid)
    .fetch_all(&mut **tx)
    .await?
    .into_iter()
    .map(|row| row.get("uid"))
    .collect();
    query(
        "UPDATE record_assertion SET retracted_at = ?, retracted_by = ?
          WHERE subject_uid = ? AND role = 'identity' AND retracted_at IS NULL
            AND predicate_uid IS NOT ?",
    )
    .bind(&now)
    .bind(actor_uid)
    .bind(subject_uid)
    .bind(predicate_uid)
    .execute(&mut **tx)
    .await?;
    for uid in &displaced {
        log_local_tx(tx, "record_assertion", uid, "", OpKind::Tombstone, None).await?;
    }
    let Some(predicate_uid) = predicate_uid else {
        return Ok(None);
    };
    if let Some(row) = query(
        "SELECT uid, role FROM record_assertion
          WHERE subject_uid = ? AND predicate_uid = ? AND object_uid IS NULL AND object_uri IS NULL
            AND retracted_at IS NULL",
    )
    .bind(subject_uid)
    .bind(predicate_uid)
    .fetch_optional(&mut **tx)
    .await?
    {
        let uid: String = row.get("uid");
        if row.get::<String, _>("role") != "identity" {
            promote_identity_tx(tx, &uid).await?;
        }
        return Ok(Some(uid));
    }
    let uid = new_uid("a");
    query(
        "INSERT INTO record_assertion
           (uid, subject_uid, predicate_uid, role, asserted_by, created_at)
         VALUES (?, ?, ?, 'identity', ?, ?)",
    )
    .bind(&uid)
    .bind(subject_uid)
    .bind(predicate_uid)
    .bind(actor_uid)
    .bind(&now)
    .execute(&mut **tx)
    .await?;
    log_local_tx(
        tx,
        "record_assertion",
        &uid,
        "",
        OpKind::Set,
        Some(
            Value::Object(assertion_op_fields(
                subject_uid,
                predicate_uid,
                None,
                "identity",
                AssertionQuantity::default(),
                actor_uid,
                &now,
            ))
            .to_string(),
        ),
    )
    .await?;
    Ok(Some(uid))
}

/// Retracts the specified active Record tuple, if present.
pub async fn retract_tuple(
    pool: &SqlitePool,
    subject_uid: &str,
    predicate_uid: &str,
    object_uid: Option<&str>,
    actor_uid: Option<&str>,
) -> Result<bool, StoreError> {
    let Some(row) = query(
        "SELECT uid FROM record_assertion
          WHERE subject_uid = ? AND predicate_uid = ? AND object_uid IS ? AND object_uri IS NULL
            AND retracted_at IS NULL",
    )
    .bind(subject_uid)
    .bind(predicate_uid)
    .bind(object_uid)
    .fetch_optional(pool)
    .await?
    else {
        return Ok(false);
    };
    retract(pool, &row.get::<String, _>("uid"), actor_uid).await
}

/// Retracts and ensures ordinary unary predicates in one caller-owned transaction.
pub async fn transition_unary(
    tx: &mut Transaction<'_, Sqlite>,
    subject_uid: &str,
    retract_predicates: &[String],
    assert_predicates: &[String],
    actor_uid: Option<&str>,
) -> Result<(), StoreError> {
    let now = now().to_rfc3339();
    for predicate_uid in retract_predicates {
        let displaced: Vec<String> = query(
            "SELECT uid FROM record_assertion
              WHERE subject_uid = ? AND predicate_uid = ? AND object_uid IS NULL AND object_uri IS NULL
                AND role = 'ordinary' AND retracted_at IS NULL",
        )
        .bind(subject_uid)
        .bind(predicate_uid)
        .fetch_all(&mut **tx)
        .await?
        .into_iter()
        .map(|row| row.get("uid"))
        .collect();
        query(
            "UPDATE record_assertion SET retracted_at = ?, retracted_by = ?
              WHERE subject_uid = ? AND predicate_uid = ? AND object_uid IS NULL AND object_uri IS NULL
                AND role = 'ordinary' AND retracted_at IS NULL",
        )
        .bind(&now)
        .bind(actor_uid)
        .bind(subject_uid)
        .bind(predicate_uid)
        .execute(&mut **tx)
        .await?;
        for uid in &displaced {
            log_local_tx(tx, "record_assertion", uid, "", OpKind::Tombstone, None).await?;
        }
    }
    for predicate_uid in assert_predicates {
        let existing = query(
            "SELECT 1 FROM record_assertion
              WHERE subject_uid = ? AND predicate_uid = ? AND object_uid IS NULL AND object_uri IS NULL
                AND role = 'ordinary' AND retracted_at IS NULL",
        )
        .bind(subject_uid)
        .bind(predicate_uid)
        .fetch_optional(&mut **tx)
        .await?;
        if existing.is_some() {
            continue;
        }
        insert_ordinary_on(tx, subject_uid, predicate_uid, None, actor_uid, &now).await?;
    }
    Ok(())
}

/// Replaces a unary Assertion with a Record link and commits the changes.
pub async fn refine(
    pool: &SqlitePool,
    subject_uid: &str,
    predicate_uid: &str,
    object_uid: &str,
    actor_uid: Option<&str>,
) -> Result<String, StoreError> {
    let mut tx = write_tx(pool).await?;
    let result = refine_tx(&mut tx, subject_uid, predicate_uid, object_uid, actor_uid).await?;
    tx.commit().await?;
    Ok(result)
}

/// Replaces a unary Assertion with a Record link within a caller-owned transaction.
pub async fn refine_tx(
    tx: &mut Transaction<'_, Sqlite>,
    subject_uid: &str,
    predicate_uid: &str,
    object_uid: &str,
    actor_uid: Option<&str>,
) -> Result<String, StoreError> {
    let now = now().to_rfc3339();
    if let Some(row) = query(
        "SELECT uid FROM record_assertion
          WHERE subject_uid = ? AND predicate_uid = ? AND object_uid IS NULL AND object_uri IS NULL
            AND retracted_at IS NULL",
    )
    .bind(subject_uid)
    .bind(predicate_uid)
    .fetch_optional(&mut **tx)
    .await?
    {
        let unary_uid: String = row.get("uid");
        query(
            "UPDATE record_assertion SET retracted_at = ?, retracted_by = ?
              WHERE uid = ?",
        )
        .bind(&now)
        .bind(actor_uid)
        .bind(&unary_uid)
        .execute(&mut **tx)
        .await?;
        log_local_tx(
            tx,
            "record_assertion",
            &unary_uid,
            "",
            OpKind::Tombstone,
            None,
        )
        .await?;
    }
    let existing = query(
        "SELECT uid FROM record_assertion
          WHERE subject_uid = ? AND predicate_uid = ? AND object_uid = ?
            AND retracted_at IS NULL",
    )
    .bind(subject_uid)
    .bind(predicate_uid)
    .bind(object_uid)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some(existing) = existing {
        return Ok(existing.get("uid"));
    }
    insert_ordinary_on(
        tx,
        subject_uid,
        predicate_uid,
        Some(object_uid),
        actor_uid,
        &now,
    )
    .await
}

async fn insert_ordinary_on(
    tx: &mut Transaction<'_, Sqlite>,
    subject_uid: &str,
    predicate_uid: &str,
    object_uid: Option<&str>,
    actor_uid: Option<&str>,
    created_at: &str,
) -> Result<String, StoreError> {
    let uid = new_uid("a");
    query(
        "INSERT INTO record_assertion
           (uid, subject_uid, predicate_uid, object_uid, role, asserted_by, created_at)
         VALUES (?, ?, ?, ?, 'ordinary', ?, ?)",
    )
    .bind(&uid)
    .bind(subject_uid)
    .bind(predicate_uid)
    .bind(object_uid)
    .bind(actor_uid)
    .bind(created_at)
    .execute(&mut **tx)
    .await?;
    log_local_tx(
        tx,
        "record_assertion",
        &uid,
        "",
        OpKind::Set,
        Some(
            Value::Object(assertion_op_fields(
                subject_uid,
                predicate_uid,
                object_uid,
                "ordinary",
                AssertionQuantity::default(),
                actor_uid,
                created_at,
            ))
            .to_string(),
        ),
    )
    .await?;
    Ok(uid)
}

/// Retracts predicate links with both endpoints in the supplied Record set.
pub async fn retract_predicate_within_set(
    pool: &SqlitePool,
    predicate_uid: &str,
    record_uids: &[String],
    actor_uid: Option<&str>,
) -> Result<u64, StoreError> {
    let wanted: HashSet<&str> = record_uids.iter().map(String::as_str).collect();
    let mut affected = 0;
    for row in binary_of_predicate(pool, predicate_uid).await? {
        if wanted.contains(row.subject_uid.as_str()) && wanted.contains(row.object_uid.as_str()) {
            affected += u64::from(retract(pool, &row.uid, actor_uid).await?);
        }
    }
    Ok(affected)
}
