use nucleus::{execution::now, visibility::Data, vocabulary::SAVED_LOCATION_ROLE_UID};
use serde_json::{Value, json, to_string};
use sqlx::{Row, Sqlite, Transaction, query, query_scalar};

use super::{
    AssertionQuantity, AssertionRole, AssertionRow, NewAssertion,
    model::{assertion_op_fields, quantity_columns},
    mutation_error,
    validation::{
        concept_reference_tx, mutation_row_tx, record_reference_tx, require_uid, validate_new_tx,
    },
};
use crate::{
    StoreError,
    cells::LOCAL_CELL_SLUG,
    sync_ops::{OpKind, log_local_tx},
};

async fn log_row_tx(
    tx: &mut Transaction<'_, Sqlite>,
    row: &AssertionRow,
) -> Result<(), StoreError> {
    let mut fields = assertion_op_fields(
        &row.subject_uid,
        &row.predicate_uid,
        row.object_uid.as_deref(),
        &row.role,
        AssertionQuantity {
            quantity: row.quantity,
            unit_uid: row.unit_uid.as_deref(),
        },
        row.asserted_by.as_deref(),
        &row.created_at,
    );
    fields.insert("object_uri".into(), json!(row.object_uri));
    log_mutation_tx(
        tx,
        &row.uid,
        OpKind::Set,
        Some(Value::Object(fields).to_string()),
    )
    .await
}

async fn log_mutation_tx(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
    kind: OpKind,
    value: Option<String>,
) -> Result<(), StoreError> {
    let identity = query(
        "SELECT c.uid, c.organ_uid FROM record c JOIN record o ON o.uid = c.organ_uid
          WHERE c.slug = ? AND c.kind = 'device' AND c.deleted_at IS NULL
            AND o.kind = 'organ' AND o.deleted_at IS NULL
            AND typeof(c.uid) = 'text' AND length(CAST(c.uid AS BLOB)) = 28
            AND typeof(c.organ_uid) = 'text' AND length(CAST(c.organ_uid AS BLOB)) = 28",
    )
    .bind(LOCAL_CELL_SLUG)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| mutation_error("Assertion mutation requires a local Cell and Organ"))?;
    require_uid(identity.try_get::<&str, _>("uid")?, "r")?;
    require_uid(identity.try_get::<&str, _>("organ_uid")?, "r")?;
    log_local_tx(tx, "record_assertion", uid, "", kind, value).await
}

/// Strictly inserts a caller-identified Assertion and logs it in the same transaction.
///
/// # Errors
/// Rejects an occupied UID or tuple, invalid references, and disallowed links,
/// and propagates database or sync errors.
pub async fn insert_tx(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
    new: NewAssertion<'_>,
) -> Result<AssertionRow, StoreError> {
    insert_target_tx(tx, uid, new, None).await
}

/// Strictly inserts an ordinary Assertion with a file or HTTP(S) endpoint.
///
/// # Errors
/// Rejects invalid endpoints and references, occupied UIDs or tuples,
/// and failed database or sync writes.
pub async fn insert_uri_tx(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
    new: NewAssertion<'_>,
    uri: &str,
) -> Result<AssertionRow, StoreError> {
    if new.object_uid.is_some()
        || new.role != AssertionRole::Ordinary
        || uri.is_empty()
        || uri.len() > 8192
        || uri.chars().any(char::is_control)
        || !uri
            .split_once(':')
            .is_some_and(|(scheme, _)| matches!(scheme, "file" | "http" | "https"))
    {
        return Err(mutation_error(
            "Choose an absolute file or HTTP(S) assertion endpoint",
        ));
    }
    insert_target_tx(tx, uid, new, Some(uri)).await
}

async fn insert_target_tx(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
    new: NewAssertion<'_>,
    object_uri: Option<&str>,
) -> Result<AssertionRow, StoreError> {
    require_uid(uid, "a")?;
    validate_new_tx(tx, &new).await?;
    let location_count: i64 = query_scalar("SELECT COUNT(*) FROM record_assertion a JOIN vocabulary_role_concept b ON b.concept_uid=a.predicate_uid AND b.role_uid=? WHERE a.subject_uid=? AND a.retracted_at IS NULL AND EXISTS(SELECT 1 FROM vocabulary_role_concept WHERE role_uid=b.role_uid AND concept_uid=?)")
        .bind(SAVED_LOCATION_ROLE_UID).bind(new.subject_uid).bind(new.predicate_uid).fetch_one(&mut **tx).await?;
    if location_count >= 64 {
        return Err(mutation_error(
            "Choose at most 64 saved address links per Record",
        ));
    }
    let location: bool = query_scalar(
        "SELECT EXISTS(SELECT 1 FROM vocabulary_role_concept WHERE role_uid=? AND concept_uid=?)",
    )
    .bind(SAVED_LOCATION_ROLE_UID)
    .bind(new.predicate_uid)
    .fetch_one(&mut **tx)
    .await?;
    if location
        && let Some(policy) = crate::data_visibility::ensure_private_for_person_on(
            tx,
            new.subject_uid,
            Data::Place,
            new.asserted_by,
        )
        .await?
    {
        log_local_tx(
            tx,
            "data_visibility",
            new.subject_uid,
            "place",
            OpKind::Set,
            Some(to_string(&policy).map_err(|error| StoreError::Protocol(error.to_string()))?),
        )
        .await?;
    }
    let existing: bool = query_scalar(
        "SELECT EXISTS(SELECT 1 FROM record_assertion WHERE uid = ?
             OR (subject_uid = ? AND predicate_uid = ? AND object_uid IS ? AND object_uri IS ?
                 AND retracted_at IS NULL))",
    )
    .bind(uid)
    .bind(new.subject_uid)
    .bind(new.predicate_uid)
    .bind(new.object_uid)
    .bind(object_uri)
    .fetch_one(&mut **tx)
    .await?;
    if existing {
        return Err(mutation_error(
            "Assertion uid or active tuple already exists",
        ));
    }
    let created_at = now().to_rfc3339();
    let (mantissa, scale) = quantity_columns(new.quantity);
    query(
        "INSERT INTO record_assertion
           (uid, subject_uid, predicate_uid, object_uid, object_uri, role,
            quantity_mantissa, quantity_scale, unit_uid, asserted_by, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(uid)
    .bind(new.subject_uid)
    .bind(new.predicate_uid)
    .bind(new.object_uid)
    .bind(object_uri)
    .bind(new.role.as_str())
    .bind(mantissa)
    .bind(scale)
    .bind(new.unit_uid)
    .bind(new.asserted_by)
    .bind(created_at)
    .execute(&mut **tx)
    .await?;
    let row = mutation_row_tx(tx, uid).await?;
    log_row_tx(tx, &row).await?;
    Ok(row)
}

/// Retracts an Assertion or observed ordinary tuple and logs tombstones.
///
/// # Errors
/// Rejects invalid mutation targets and actors and propagates write errors.
pub async fn retract_tx(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
    retracted_by: Option<&str>,
) -> Result<bool, StoreError> {
    let row = mutation_row_tx(tx, uid).await?;
    if let Some(actor) = retracted_by {
        record_reference_tx(tx, actor).await?;
    }
    if row.role == "ordinary" {
        let removed: bool = query_scalar("SELECT EXISTS(SELECT 1 FROM sync_op WHERE tbl = 'record_assertion' AND uid = ? AND kind = 'tombstone')").bind(uid).fetch_one(&mut **tx).await?;
        if removed {
            return Ok(false);
        }
        let observed: Vec<String> = query_scalar("SELECT a.uid FROM record_assertion a WHERE a.subject_uid = ? AND a.predicate_uid = ? AND a.object_uid IS ? AND a.object_uri IS ? AND a.role = 'ordinary' AND NOT EXISTS (SELECT 1 FROM sync_op o WHERE o.tbl = 'record_assertion' AND o.uid = a.uid AND o.kind = 'tombstone')")
            .bind(&row.subject_uid).bind(&row.predicate_uid).bind(&row.object_uid).bind(&row.object_uri).fetch_all(&mut **tx).await?;
        for member in &observed {
            record_retraction(tx, member, retracted_by).await?;
        }
        return Ok(!observed.is_empty());
    }
    if row.retracted_at.is_some() {
        return Ok(false);
    }
    record_retraction(tx, uid, retracted_by).await?;
    Ok(true)
}

async fn record_retraction(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
    retracted_by: Option<&str>,
) -> Result<(), StoreError> {
    query("UPDATE record_assertion SET retracted_at = ?, retracted_by = ? WHERE uid = ?")
        .bind(now().to_rfc3339())
        .bind(retracted_by)
        .bind(uid)
        .execute(&mut **tx)
        .await?;
    log_mutation_tx(tx, uid, OpKind::Tombstone, None).await
}

/// Updates the exact quantity and unit of an active Assertion.
///
/// # Errors
/// Rejects retracted Assertions, illegal identity quantities, invalid units,
/// and failed database or sync writes.
pub async fn set_quantity_tx(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
    quantity: AssertionQuantity<'_>,
) -> Result<AssertionRow, StoreError> {
    let row = mutation_row_tx(tx, uid).await?;
    if row.retracted_at.is_some() {
        return Err(mutation_error("cannot change a retracted Assertion"));
    }
    if row.role == "identity" && (quantity.quantity.is_some() || quantity.unit_uid.is_some()) {
        return Err(mutation_error("an identity assertion must be unquantified"));
    }
    if let Some(unit) = quantity.unit_uid {
        concept_reference_tx(tx, unit).await?;
    }
    if row.quantity == quantity.quantity && row.unit_uid.as_deref() == quantity.unit_uid {
        return Ok(row);
    }
    let (mantissa, scale) = quantity_columns(quantity.quantity);
    query(
        "UPDATE record_assertion SET quantity_mantissa = ?, quantity_scale = ?, unit_revision = CASE WHEN unit_uid IS ? THEN unit_revision ELSE NULL END, unit_uid = ?
          WHERE uid = ?",
    )
    .bind(mantissa)
    .bind(scale)
    .bind(quantity.unit_uid)
    .bind(quantity.unit_uid)
    .bind(uid)
    .execute(&mut **tx)
    .await?;
    let row = mutation_row_tx(tx, uid).await?;
    log_row_tx(tx, &row).await?;
    Ok(row)
}

/// Promotes an active unary Assertion and clears its quantity and unit.
///
/// # Errors
/// Rejects inactive or non-unary targets and propagates write errors.
pub async fn promote_identity_tx(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
) -> Result<AssertionRow, StoreError> {
    let mut row = mutation_row_tx(tx, uid).await?;
    if row.retracted_at.is_some() || row.object_uid.is_some() || row.object_uri.is_some() {
        return Err(mutation_error(
            "only an active unary Assertion can become identity",
        ));
    }
    if row.role == "identity" {
        return Ok(row);
    }
    query(
        "UPDATE record_assertion
            SET role = 'identity', quantity_mantissa = NULL, quantity_scale = NULL, unit_uid = NULL
          WHERE uid = ?",
    )
    .bind(uid)
    .execute(&mut **tx)
    .await?;
    row.role = "identity".into();
    row.quantity = None;
    row.unit_uid = None;
    row.unit_revision = None;
    log_row_tx(tx, &row).await?;
    Ok(row)
}
