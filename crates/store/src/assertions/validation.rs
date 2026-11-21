use nucleus::{RecordKind, valid_uid};
use sqlx::{Row, Sqlite, Transaction, query, query_scalar};

use super::{AssertionRole, AssertionRow, NewAssertion, model::map, mutation_error};
use crate::{StoreError, exact::decimal_columns};

pub(super) fn require_uid(uid: &str, prefix: &str) -> Result<(), StoreError> {
    if !valid_uid(uid, prefix) {
        return Err(mutation_error("invalid Assertion mutation reference uid"));
    }
    Ok(())
}

async fn record_root_tx(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
) -> Result<Option<String>, StoreError> {
    require_uid(uid, "r")?;
    let row = query(
        "SELECT kind, replica_root FROM record
          WHERE uid = ? AND typeof(uid) = 'text' AND deleted_at IS NULL
            AND typeof(kind) = 'text' AND length(CAST(kind AS BLOB)) <= 32
            AND (replica_root IS NULL OR
                 (typeof(replica_root) = 'text' AND length(CAST(replica_root AS BLOB)) = 28))",
    )
    .bind(uid)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| mutation_error("Assertion references a missing, deleted or corrupt Record"))?;
    if RecordKind::parse(row.try_get::<&str, _>("kind")?).is_none() {
        return Err(mutation_error(
            "Assertion references an unknown Record kind",
        ));
    }
    let root: Option<String> = row.try_get("replica_root")?;
    if let Some(root) = &root {
        require_uid(root, "r")?;
    }
    Ok(root)
}

pub(super) async fn record_reference_tx(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
) -> Result<Option<String>, StoreError> {
    let root = record_root_tx(tx, uid).await?;
    if let Some(root_uid) = &root {
        let parent = record_root_tx(tx, root_uid).await?;
        if parent.as_ref().is_some_and(|parent| parent != root_uid) {
            return Err(mutation_error(
                "Assertion references a nested or cyclic replica root",
            ));
        }
    }
    Ok(root)
}

pub(super) async fn concept_reference_tx(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
) -> Result<(), StoreError> {
    require_uid(uid, "c")?;
    let exists: bool =
        query_scalar("SELECT EXISTS(SELECT 1 FROM concept WHERE uid = ? AND typeof(uid) = 'text')")
            .bind(uid)
            .fetch_one(&mut **tx)
            .await?;
    if !exists {
        return Err(mutation_error("Assertion references a missing Concept"));
    }
    Ok(())
}

pub(super) async fn validate_new_tx(
    tx: &mut Transaction<'_, Sqlite>,
    new: &NewAssertion<'_>,
) -> Result<(), StoreError> {
    crate::vocabulary::validate_feature_assertion_on(tx, new).await?;
    crate::places::validate_location_assertion_on(
        tx,
        new.predicate_uid,
        new.subject_uid,
        new.object_uid,
        new.quantity.is_some(),
        new.unit_uid.is_some(),
    )
    .await?;
    if new.role == AssertionRole::Identity
        && (new.object_uid.is_some() || new.quantity.is_some() || new.unit_uid.is_some())
    {
        return Err(mutation_error(
            "an identity assertion must be unary and unquantified",
        ));
    }
    let subject_root = record_reference_tx(tx, new.subject_uid).await?;
    concept_reference_tx(tx, new.predicate_uid).await?;
    if let Some(unit) = new.unit_uid {
        concept_reference_tx(tx, unit).await?;
    }
    if let Some(actor) = new.asserted_by {
        record_reference_tx(tx, actor).await?;
    }
    if let Some(object) = new.object_uid {
        let object_root = record_reference_tx(tx, object).await?;
        if object_root.is_some() && object_root != subject_root {
            return Err(mutation_error(
                "an assertion cannot cross an individual-replica boundary",
            ));
        }
    }
    Ok(())
}

pub(super) async fn mutation_row_tx(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
) -> Result<AssertionRow, StoreError> {
    require_uid(uid, "a")?;
    let row = query(
        "SELECT * FROM record_assertion WHERE uid = ? AND typeof(uid) = 'text'
            AND typeof(subject_uid) = 'text' AND length(CAST(subject_uid AS BLOB)) = 28
            AND typeof(predicate_uid) = 'text' AND length(CAST(predicate_uid AS BLOB)) = 28
            AND (object_uid IS NULL OR (typeof(object_uid) = 'text' AND length(CAST(object_uid AS BLOB)) = 28))
            AND typeof(role) = 'text' AND length(CAST(role AS BLOB)) <= 8
            AND (quantity_mantissa IS NULL OR (typeof(quantity_mantissa) = 'text' AND length(CAST(quantity_mantissa AS BLOB)) <= 40))
            AND (quantity_scale IS NULL OR typeof(quantity_scale) = 'integer')
            AND (unit_uid IS NULL OR (typeof(unit_uid) = 'text' AND length(CAST(unit_uid AS BLOB)) = 28))
            AND (asserted_by IS NULL OR (typeof(asserted_by) = 'text' AND length(CAST(asserted_by AS BLOB)) = 28))
            AND typeof(created_at) = 'text' AND length(CAST(created_at AS BLOB)) BETWEEN 1 AND 64
            AND (retracted_at IS NULL OR (typeof(retracted_at) = 'text' AND length(CAST(retracted_at AS BLOB)) BETWEEN 1 AND 64))
            AND (retracted_by IS NULL OR (typeof(retracted_by) = 'text' AND length(CAST(retracted_by AS BLOB)) = 28))",
    )
    .bind(uid)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| mutation_error("Assertion mutation target is missing or corrupt"))?;
    let mantissa: Option<String> = row.try_get("quantity_mantissa")?;
    let scale: Option<i64> = row.try_get("quantity_scale")?;
    if mantissa.is_none() != scale.is_none() {
        return Err(mutation_error("Assertion has incomplete exact quantity"));
    }
    let row = map(row)?;
    if row.quantity.map(decimal_columns) != mantissa.zip(scale) {
        return Err(mutation_error("Assertion has noncanonical exact quantity"));
    }
    for actor in [&row.asserted_by, &row.retracted_by].into_iter().flatten() {
        require_uid(actor, "r")?;
    }
    let role = match row.role.as_str() {
        "ordinary" => AssertionRole::Ordinary,
        "identity" => AssertionRole::Identity,
        _ => return Err(mutation_error("Assertion has an unknown role")),
    };
    validate_new_tx(
        tx,
        &NewAssertion {
            subject_uid: &row.subject_uid,
            predicate_uid: &row.predicate_uid,
            object_uid: row.object_uid.as_deref(),
            role,
            quantity: row.quantity,
            unit_uid: row.unit_uid.as_deref(),
            asserted_by: None,
        },
    )
    .await?;
    Ok(row)
}
