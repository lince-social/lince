use chrono::Utc;
use nucleus::DecimalValue;
use nucleus::graph::Edge;
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

use crate::StoreError;
use crate::sync_ops::OpKind;

fn assertion_op_value(
    subject_uid: &str,
    predicate_uid: &str,
    object_uid: Option<&str>,
    role: &str,
    quantity: Option<DecimalValue>,
    unit_uid: Option<&str>,
    asserted_by: Option<&str>,
    created_at: &str,
) -> String {
    let quantity = quantity.map(crate::exact::decimal_columns);
    serde_json::json!({
        "subject_uid": subject_uid,
        "predicate_uid": predicate_uid,
        "object_uid": object_uid,
        "role": role,
        "quantity_mantissa": quantity.as_ref().map(|pair| pair.0.as_str()),
        "quantity_scale": quantity.as_ref().map(|pair| pair.1),
        "unit_uid": unit_uid,
        "asserted_by": asserted_by,
        "created_at": created_at,
    })
    .to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssertionRole {
    Ordinary,
    Identity,
}

impl AssertionRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ordinary => "ordinary",
            Self::Identity => "identity",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AssertionRow {
    pub uid: String,
    pub subject_uid: String,
    pub predicate_uid: String,
    pub object_uid: Option<String>,
    pub role: String,
    pub quantity: Option<DecimalValue>,
    pub unit_uid: Option<String>,
    pub asserted_by: Option<String>,
    pub created_at: String,
    pub retracted_at: Option<String>,
    pub retracted_by: Option<String>,
}

fn map(row: sqlx::sqlite::SqliteRow) -> Result<AssertionRow, StoreError> {
    let quantity = if row
        .try_get::<Option<String>, _>("quantity_mantissa")?
        .is_some()
    {
        Some(crate::exact::read_decimal(&row, "quantity")?)
    } else {
        None
    };
    Ok(AssertionRow {
        uid: row.try_get("uid")?,
        subject_uid: row.try_get("subject_uid")?,
        predicate_uid: row.try_get("predicate_uid")?,
        object_uid: row.try_get("object_uid")?,
        role: row.try_get("role")?,
        quantity,
        unit_uid: row.try_get("unit_uid")?,
        asserted_by: row.try_get("asserted_by")?,
        created_at: row.try_get("created_at")?,
        retracted_at: row.try_get("retracted_at")?,
        retracted_by: row.try_get("retracted_by")?,
    })
}

pub struct NewAssertion<'a> {
    pub subject_uid: &'a str,
    pub predicate_uid: &'a str,
    pub object_uid: Option<&'a str>,
    pub role: AssertionRole,
    pub quantity: Option<DecimalValue>,
    pub unit_uid: Option<&'a str>,
    pub asserted_by: Option<&'a str>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AssertionQuantity<'a> {
    pub quantity: Option<DecimalValue>,
    pub unit_uid: Option<&'a str>,
}

fn mutation_error(message: &str) -> StoreError {
    StoreError::Protocol(message.into())
}

fn require_uid(uid: &str, prefix: &str) -> Result<(), StoreError> {
    if !nucleus::valid_uid(uid, prefix) {
        return Err(mutation_error("invalid Assertion mutation reference uid"));
    }
    Ok(())
}

async fn record_root_tx(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
) -> Result<Option<String>, StoreError> {
    require_uid(uid, "r")?;
    let row = sqlx::query(
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
    if nucleus::RecordKind::parse(row.try_get::<&str, _>("kind")?).is_none() {
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

async fn record_reference_tx(
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

async fn concept_reference_tx(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
) -> Result<(), StoreError> {
    require_uid(uid, "c")?;
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM concept WHERE uid = ? AND typeof(uid) = 'text')",
    )
    .bind(uid)
    .fetch_one(&mut **tx)
    .await?;
    if !exists {
        return Err(mutation_error("Assertion references a missing Concept"));
    }
    Ok(())
}

async fn validate_new_tx(
    tx: &mut Transaction<'_, Sqlite>,
    new: &NewAssertion<'_>,
) -> Result<(), StoreError> {
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

async fn mutation_row_tx(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
) -> Result<AssertionRow, StoreError> {
    require_uid(uid, "a")?;
    let row = sqlx::query(
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
    if row.quantity.map(crate::exact::decimal_columns) != mantissa.zip(scale) {
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

async fn log_row_tx(
    tx: &mut Transaction<'_, Sqlite>,
    row: &AssertionRow,
) -> Result<(), StoreError> {
    log_mutation_tx(
        tx,
        &row.uid,
        OpKind::Set,
        Some(assertion_op_value(
            &row.subject_uid,
            &row.predicate_uid,
            row.object_uid.as_deref(),
            &row.role,
            row.quantity,
            row.unit_uid.as_deref(),
            row.asserted_by.as_deref(),
            &row.created_at,
        )),
    )
    .await
}

async fn log_mutation_tx(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
    kind: OpKind,
    value: Option<String>,
) -> Result<(), StoreError> {
    let identity = sqlx::query(
        "SELECT c.uid, c.organ_uid FROM record c JOIN record o ON o.uid = c.organ_uid
          WHERE c.slug = ? AND c.kind = 'device' AND c.deleted_at IS NULL
            AND o.kind = 'organ' AND o.deleted_at IS NULL
            AND typeof(c.uid) = 'text' AND length(CAST(c.uid AS BLOB)) = 28
            AND typeof(c.organ_uid) = 'text' AND length(CAST(c.organ_uid AS BLOB)) = 28",
    )
    .bind(crate::cells::LOCAL_CELL_SLUG)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| mutation_error("Assertion mutation requires a local Cell and Organ"))?;
    require_uid(identity.try_get::<&str, _>("uid")?, "r")?;
    require_uid(identity.try_get::<&str, _>("organ_uid")?, "r")?;
    crate::sync_ops::log_local_tx(tx, "record_assertion", uid, "", kind, value).await
}

pub async fn insert_tx(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
    new: NewAssertion<'_>,
) -> Result<AssertionRow, StoreError> {
    require_uid(uid, "a")?;
    validate_new_tx(tx, &new).await?;
    let existing: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM record_assertion WHERE uid = ?
             OR (subject_uid = ? AND predicate_uid = ? AND object_uid IS ?
                 AND retracted_at IS NULL))",
    )
    .bind(uid)
    .bind(new.subject_uid)
    .bind(new.predicate_uid)
    .bind(new.object_uid)
    .fetch_one(&mut **tx)
    .await?;
    if existing {
        return Err(mutation_error(
            "Assertion uid or active tuple already exists",
        ));
    }
    let row = AssertionRow {
        uid: uid.into(),
        subject_uid: new.subject_uid.into(),
        predicate_uid: new.predicate_uid.into(),
        object_uid: new.object_uid.map(str::to_owned),
        role: new.role.as_str().into(),
        quantity: new.quantity,
        unit_uid: new.unit_uid.map(str::to_owned),
        asserted_by: new.asserted_by.map(str::to_owned),
        created_at: Utc::now().to_rfc3339(),
        retracted_at: None,
        retracted_by: None,
    };
    let quantity = new.quantity.map(crate::exact::decimal_columns);
    sqlx::query(
        "INSERT INTO record_assertion
           (uid, subject_uid, predicate_uid, object_uid, role,
            quantity_mantissa, quantity_scale, unit_uid, asserted_by, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(uid)
    .bind(new.subject_uid)
    .bind(new.predicate_uid)
    .bind(new.object_uid)
    .bind(new.role.as_str())
    .bind(quantity.as_ref().map(|pair| pair.0.as_str()))
    .bind(quantity.as_ref().map(|pair| pair.1))
    .bind(new.unit_uid)
    .bind(new.asserted_by)
    .bind(&row.created_at)
    .execute(&mut **tx)
    .await?;
    log_row_tx(tx, &row).await?;
    Ok(row)
}

pub async fn retract_tx(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
    retracted_by: Option<&str>,
) -> Result<bool, StoreError> {
    let row = mutation_row_tx(tx, uid).await?;
    if let Some(actor) = retracted_by {
        record_reference_tx(tx, actor).await?;
    }
    if row.retracted_at.is_some() {
        return Ok(false);
    }
    sqlx::query("UPDATE record_assertion SET retracted_at = ?, retracted_by = ? WHERE uid = ?")
        .bind(Utc::now().to_rfc3339())
        .bind(retracted_by)
        .bind(uid)
        .execute(&mut **tx)
        .await?;
    log_mutation_tx(tx, uid, OpKind::Tombstone, None).await?;
    Ok(true)
}

pub async fn set_quantity_tx(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
    quantity: AssertionQuantity<'_>,
) -> Result<AssertionRow, StoreError> {
    let mut row = mutation_row_tx(tx, uid).await?;
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
    let columns = quantity.quantity.map(crate::exact::decimal_columns);
    sqlx::query(
        "UPDATE record_assertion SET quantity_mantissa = ?, quantity_scale = ?, unit_uid = ?
          WHERE uid = ?",
    )
    .bind(columns.as_ref().map(|pair| pair.0.as_str()))
    .bind(columns.as_ref().map(|pair| pair.1))
    .bind(quantity.unit_uid)
    .bind(uid)
    .execute(&mut **tx)
    .await?;
    row.quantity = quantity.quantity;
    row.unit_uid = quantity.unit_uid.map(str::to_owned);
    log_row_tx(tx, &row).await?;
    Ok(row)
}

pub async fn promote_identity_tx(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
) -> Result<AssertionRow, StoreError> {
    let mut row = mutation_row_tx(tx, uid).await?;
    if row.retracted_at.is_some() || row.object_uid.is_some() {
        return Err(mutation_error(
            "only an active unary Assertion can become identity",
        ));
    }
    if row.role == "identity" {
        return Ok(row);
    }
    sqlx::query(
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
    log_row_tx(tx, &row).await?;
    Ok(row)
}

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

pub async fn import_active(
    pool: &SqlitePool,
    imported: ImportedAssertion<'_>,
) -> Result<(), StoreError> {
    let quantity = imported.quantity.map(crate::exact::decimal_columns);
    sqlx::query(
        "INSERT OR IGNORE INTO record_assertion
           (uid, subject_uid, predicate_uid, object_uid, role,
            quantity_mantissa, quantity_scale, unit_uid, asserted_by, created_at)
         VALUES (?, ?, ?, ?, 'ordinary', ?, ?, ?, ?, ?)",
    )
    .bind(imported.uid)
    .bind(imported.subject_uid)
    .bind(imported.predicate_uid)
    .bind(imported.object_uid)
    .bind(quantity.as_ref().map(|pair| pair.0.as_str()))
    .bind(quantity.as_ref().map(|pair| pair.1))
    .bind(imported.unit_uid)
    .bind(imported.asserted_by)
    .bind(imported.created_at)
    .execute(pool)
    .await?;
    Ok(())
}

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
    let existing = sqlx::query(
        "SELECT uid FROM record_assertion
          WHERE subject_uid = ? AND predicate_uid = ? AND object_uid IS ?
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
    let uid = nucleus::new_uid("a");
    let now = Utc::now().to_rfc3339();
    let quantity = new.quantity.map(crate::exact::decimal_columns);
    sqlx::query(
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
    .bind(quantity.as_ref().map(|pair| pair.0.as_str()))
    .bind(quantity.as_ref().map(|pair| pair.1))
    .bind(new.unit_uid)
    .bind(new.asserted_by)
    .bind(&now)
    .execute(pool)
    .await?;
    crate::sync_ops::log_local(
        pool,
        "record_assertion",
        &uid,
        "",
        OpKind::Set,
        Some(assertion_op_value(
            new.subject_uid,
            new.predicate_uid,
            new.object_uid,
            new.role.as_str(),
            new.quantity,
            new.unit_uid,
            new.asserted_by,
            &now,
        )),
    )
    .await?;
    Ok(uid)
}

pub async fn get(pool: &SqlitePool, uid: &str) -> Result<Option<AssertionRow>, StoreError> {
    sqlx::query("SELECT * FROM record_assertion WHERE uid = ?")
        .bind(uid)
        .fetch_optional(pool)
        .await?
        .map(map)
        .transpose()
}

pub async fn list_active(pool: &SqlitePool) -> Result<Vec<AssertionRow>, StoreError> {
    sqlx::query(
        "SELECT * FROM record_assertion WHERE retracted_at IS NULL ORDER BY created_at, uid",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map)
    .collect()
}

pub async fn retract(
    pool: &SqlitePool,
    uid: &str,
    actor_uid: Option<&str>,
) -> Result<bool, StoreError> {
    let retracted = sqlx::query(
        "UPDATE record_assertion SET retracted_at = ?, retracted_by = ?
          WHERE uid = ? AND retracted_at IS NULL",
    )
    .bind(Utc::now().to_rfc3339())
    .bind(actor_uid)
    .bind(uid)
    .execute(pool)
    .await?
    .rows_affected()
        > 0;
    if retracted {
        crate::sync_ops::log_local(pool, "record_assertion", uid, "", OpKind::Tombstone, None)
            .await?;
    }
    Ok(retracted)
}

pub async fn set_identity(
    pool: &SqlitePool,
    subject_uid: &str,
    predicate_uid: Option<&str>,
    actor_uid: Option<&str>,
) -> Result<Option<String>, StoreError> {
    let mut transaction = crate::write_tx(pool).await?;
    let now = Utc::now().to_rfc3339();
    let displaced: Vec<String> = sqlx::query(
        "SELECT uid FROM record_assertion
          WHERE subject_uid = ? AND role = 'identity' AND retracted_at IS NULL
            AND predicate_uid IS NOT ?",
    )
    .bind(subject_uid)
    .bind(predicate_uid)
    .fetch_all(&mut *transaction)
    .await?
    .into_iter()
    .map(|row| row.get("uid"))
    .collect();
    sqlx::query(
        "UPDATE record_assertion SET retracted_at = ?, retracted_by = ?
          WHERE subject_uid = ? AND role = 'identity' AND retracted_at IS NULL
            AND predicate_uid IS NOT ?",
    )
    .bind(&now)
    .bind(actor_uid)
    .bind(subject_uid)
    .bind(predicate_uid)
    .execute(&mut *transaction)
    .await?;
    for uid in &displaced {
        crate::sync_ops::log_local_tx(
            &mut transaction,
            "record_assertion",
            uid,
            "",
            OpKind::Tombstone,
            None,
        )
        .await?;
    }
    let Some(predicate_uid) = predicate_uid else {
        transaction.commit().await?;
        return Ok(None);
    };
    if let Some(row) = sqlx::query(
        "SELECT uid, role FROM record_assertion
          WHERE subject_uid = ? AND predicate_uid = ? AND object_uid IS NULL
            AND retracted_at IS NULL",
    )
    .bind(subject_uid)
    .bind(predicate_uid)
    .fetch_optional(&mut *transaction)
    .await?
    {
        let uid: String = row.get("uid");
        if row.get::<String, _>("role") != "identity" {
            promote_identity_tx(&mut transaction, &uid).await?;
        }
        transaction.commit().await?;
        return Ok(Some(uid));
    }
    let uid = nucleus::new_uid("a");
    sqlx::query(
        "INSERT INTO record_assertion
           (uid, subject_uid, predicate_uid, role, asserted_by, created_at)
         VALUES (?, ?, ?, 'identity', ?, ?)",
    )
    .bind(&uid)
    .bind(subject_uid)
    .bind(predicate_uid)
    .bind(actor_uid)
    .bind(&now)
    .execute(&mut *transaction)
    .await?;
    crate::sync_ops::log_local_tx(
        &mut transaction,
        "record_assertion",
        &uid,
        "",
        OpKind::Set,
        Some(assertion_op_value(
            subject_uid,
            predicate_uid,
            None,
            "identity",
            None,
            None,
            actor_uid,
            &now,
        )),
    )
    .await?;
    transaction.commit().await?;
    Ok(Some(uid))
}

pub async fn identity_concept(
    pool: &SqlitePool,
    subject_uid: &str,
) -> Result<Option<String>, StoreError> {
    Ok(sqlx::query(
        "SELECT predicate_uid FROM record_assertion
          WHERE subject_uid = ? AND role = 'identity' AND retracted_at IS NULL",
    )
    .bind(subject_uid)
    .fetch_optional(pool)
    .await?
    .map(|row| row.get("predicate_uid")))
}

pub async fn concepts_for_record(
    pool: &SqlitePool,
    subject_uid: &str,
) -> Result<Vec<String>, StoreError> {
    Ok(sqlx::query(
        "SELECT DISTINCT predicate_uid FROM record_assertion
          WHERE subject_uid = ? AND retracted_at IS NULL
          ORDER BY predicate_uid",
    )
    .bind(subject_uid)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| row.get("predicate_uid"))
    .collect())
}

pub async fn active_of_predicates(
    pool: &SqlitePool,
    predicate_uids: &[String],
) -> Result<Vec<AssertionRow>, StoreError> {
    let wanted: std::collections::HashSet<&str> =
        predicate_uids.iter().map(String::as_str).collect();
    Ok(list_active(pool)
        .await?
        .into_iter()
        .filter(|row| wanted.contains(row.predicate_uid.as_str()))
        .collect())
}

#[derive(Debug, Clone, PartialEq)]
pub struct BinaryAssertionRow {
    pub uid: String,
    pub subject_uid: String,
    pub object_uid: String,
    pub predicate_uid: String,
    pub predicate: String,
    pub quantity: Option<DecimalValue>,
    pub unit_uid: Option<String>,
    pub created_at: String,
}

fn map_binary(row: sqlx::sqlite::SqliteRow) -> Result<BinaryAssertionRow, StoreError> {
    let quantity = if row.get::<Option<String>, _>("quantity_mantissa").is_some() {
        Some(crate::exact::read_decimal(&row, "quantity")?)
    } else {
        None
    };
    Ok(BinaryAssertionRow {
        uid: row.get("uid"),
        subject_uid: row.get("subject_uid"),
        object_uid: row.get("object_uid"),
        predicate_uid: row.get("predicate_uid"),
        predicate: row.get("predicate"),
        quantity,
        unit_uid: row.get("unit_uid"),
        created_at: row.get("created_at"),
    })
}

const BINARY_SELECT: &str = "SELECT a.uid, a.subject_uid, a.object_uid, a.predicate_uid,
            c.canonical_name AS predicate, a.quantity_mantissa,
            a.quantity_scale, a.unit_uid, a.created_at
       FROM record_assertion a JOIN concept c ON c.uid = a.predicate_uid
      WHERE a.object_uid IS NOT NULL AND a.retracted_at IS NULL";

pub async fn binary_of_predicate(
    pool: &SqlitePool,
    predicate_uid: &str,
) -> Result<Vec<BinaryAssertionRow>, StoreError> {
    let sql = format!("{BINARY_SELECT} AND a.predicate_uid = ? ORDER BY a.created_at, a.uid");
    sqlx::query(&sql)
        .bind(predicate_uid)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(map_binary)
        .collect()
}

pub async fn binary_of_predicates(
    pool: &SqlitePool,
    predicate_uids: &[String],
) -> Result<Vec<BinaryAssertionRow>, StoreError> {
    let wanted: std::collections::HashSet<&str> =
        predicate_uids.iter().map(String::as_str).collect();
    Ok(all_binary(pool)
        .await?
        .into_iter()
        .filter(|row| wanted.contains(row.predicate_uid.as_str()))
        .collect())
}

pub async fn all_binary(pool: &SqlitePool) -> Result<Vec<BinaryAssertionRow>, StoreError> {
    let sql = format!("{BINARY_SELECT} ORDER BY a.created_at, a.uid");
    sqlx::query(&sql)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(map_binary)
        .collect()
}

pub async fn used_binary_predicates(
    pool: &SqlitePool,
) -> Result<Vec<(String, String)>, StoreError> {
    Ok(sqlx::query(
        "SELECT c.uid, c.canonical_name
           FROM concept c JOIN record_assertion a ON a.predicate_uid = c.uid
          WHERE a.object_uid IS NOT NULL AND a.retracted_at IS NULL
          GROUP BY c.uid, c.canonical_name ORDER BY c.canonical_name, c.uid",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| (row.get("uid"), row.get("canonical_name")))
    .collect())
}

pub async fn retract_tuple(
    pool: &SqlitePool,
    subject_uid: &str,
    predicate_uid: &str,
    object_uid: Option<&str>,
    actor_uid: Option<&str>,
) -> Result<bool, StoreError> {
    let Some(row) = sqlx::query(
        "SELECT uid FROM record_assertion
          WHERE subject_uid = ? AND predicate_uid = ? AND object_uid IS ?
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

pub async fn transition_unary(
    tx: &mut Transaction<'_, Sqlite>,
    subject_uid: &str,
    retract_predicates: &[String],
    assert_predicates: &[String],
    actor_uid: Option<&str>,
) -> Result<(), StoreError> {
    let now = Utc::now().to_rfc3339();
    for predicate_uid in retract_predicates {
        let displaced: Vec<String> = sqlx::query(
            "SELECT uid FROM record_assertion
              WHERE subject_uid = ? AND predicate_uid = ? AND object_uid IS NULL
                AND role = 'ordinary' AND retracted_at IS NULL",
        )
        .bind(subject_uid)
        .bind(predicate_uid)
        .fetch_all(&mut **tx)
        .await?
        .into_iter()
        .map(|row| row.get("uid"))
        .collect();
        sqlx::query(
            "UPDATE record_assertion SET retracted_at = ?, retracted_by = ?
              WHERE subject_uid = ? AND predicate_uid = ? AND object_uid IS NULL
                AND role = 'ordinary' AND retracted_at IS NULL",
        )
        .bind(&now)
        .bind(actor_uid)
        .bind(subject_uid)
        .bind(predicate_uid)
        .execute(&mut **tx)
        .await?;
        for uid in &displaced {
            crate::sync_ops::log_local_tx(tx, "record_assertion", uid, "", OpKind::Tombstone, None)
                .await?;
        }
    }
    for predicate_uid in assert_predicates {
        let existing = sqlx::query(
            "SELECT 1 FROM record_assertion
              WHERE subject_uid = ? AND predicate_uid = ? AND object_uid IS NULL
                AND role = 'ordinary' AND retracted_at IS NULL",
        )
        .bind(subject_uid)
        .bind(predicate_uid)
        .fetch_optional(&mut **tx)
        .await?;
        if existing.is_some() {
            continue;
        }
        let uid = nucleus::new_uid("a");
        sqlx::query(
            "INSERT INTO record_assertion
               (uid, subject_uid, predicate_uid, object_uid, role, asserted_by, created_at)
             VALUES (?, ?, ?, NULL, 'ordinary', ?, ?)",
        )
        .bind(&uid)
        .bind(subject_uid)
        .bind(predicate_uid)
        .bind(actor_uid)
        .bind(&now)
        .execute(&mut **tx)
        .await?;
        crate::sync_ops::log_local_tx(
            tx,
            "record_assertion",
            &uid,
            "",
            OpKind::Set,
            Some(assertion_op_value(
                subject_uid,
                predicate_uid,
                None,
                "ordinary",
                None,
                None,
                actor_uid,
                &now,
            )),
        )
        .await?;
    }
    Ok(())
}

pub async fn refine(
    pool: &SqlitePool,
    subject_uid: &str,
    predicate_uid: &str,
    object_uid: &str,
    actor_uid: Option<&str>,
) -> Result<String, StoreError> {
    let mut transaction = crate::write_tx(pool).await?;
    let now = Utc::now().to_rfc3339();
    if let Some(row) = sqlx::query(
        "SELECT uid FROM record_assertion
          WHERE subject_uid = ? AND predicate_uid = ? AND object_uid IS NULL
            AND retracted_at IS NULL",
    )
    .bind(subject_uid)
    .bind(predicate_uid)
    .fetch_optional(&mut *transaction)
    .await?
    {
        let unary_uid: String = row.get("uid");
        sqlx::query(
            "UPDATE record_assertion SET retracted_at = ?, retracted_by = ?
              WHERE uid = ?",
        )
        .bind(&now)
        .bind(actor_uid)
        .bind(&unary_uid)
        .execute(&mut *transaction)
        .await?;
        crate::sync_ops::log_local_tx(
            &mut transaction,
            "record_assertion",
            &unary_uid,
            "",
            OpKind::Tombstone,
            None,
        )
        .await?;
    }
    let existing = sqlx::query(
        "SELECT uid FROM record_assertion
          WHERE subject_uid = ? AND predicate_uid = ? AND object_uid = ?
            AND retracted_at IS NULL",
    )
    .bind(subject_uid)
    .bind(predicate_uid)
    .bind(object_uid)
    .fetch_optional(&mut *transaction)
    .await?;
    if let Some(existing) = existing {
        transaction.commit().await?;
        return Ok(existing.get("uid"));
    }
    let uid = nucleus::new_uid("a");
    sqlx::query(
        "INSERT INTO record_assertion
           (uid, subject_uid, predicate_uid, object_uid, role, asserted_by, created_at)
         VALUES (?, ?, ?, ?, 'ordinary', ?, ?)",
    )
    .bind(&uid)
    .bind(subject_uid)
    .bind(predicate_uid)
    .bind(object_uid)
    .bind(actor_uid)
    .bind(&now)
    .execute(&mut *transaction)
    .await?;
    crate::sync_ops::log_local_tx(
        &mut transaction,
        "record_assertion",
        &uid,
        "",
        OpKind::Set,
        Some(assertion_op_value(
            subject_uid,
            predicate_uid,
            Some(object_uid),
            "ordinary",
            None,
            None,
            actor_uid,
            &now,
        )),
    )
    .await?;
    transaction.commit().await?;
    Ok(uid)
}

pub async fn retract_predicate_within_set(
    pool: &SqlitePool,
    predicate_uid: &str,
    record_uids: &[String],
    actor_uid: Option<&str>,
) -> Result<u64, StoreError> {
    let wanted: std::collections::HashSet<&str> = record_uids.iter().map(String::as_str).collect();
    let mut affected = 0;
    for row in binary_of_predicate(pool, predicate_uid).await? {
        if wanted.contains(row.subject_uid.as_str()) && wanted.contains(row.object_uid.as_str()) {
            affected += u64::from(retract(pool, &row.uid, actor_uid).await?);
        }
    }
    Ok(affected)
}

fn map_record(row: sqlx::sqlite::SqliteRow) -> Result<crate::records::RecordRow, StoreError> {
    Ok(crate::records::RecordRow {
        uid: row.get("uid"),
        slug: row.get("slug"),
        kind: row.get("kind"),
        head: row.get("head"),
        body: row.get("body"),
        quantity: crate::exact::read_decimal(&row, "quantity")?,
        identity_predicate_uid: row.get("identity_predicate_uid"),
        unit_uid: row.get("unit_uid"),
        place_uid: row.get("place_uid"),
        organ_uid: row.get("organ_uid"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        created_hlc: row.try_get("created_hlc").ok().flatten(),
    })
}

pub async fn subjects_pointing_to(
    pool: &SqlitePool,
    predicate_uid: &str,
    object_uid: &str,
) -> Result<Vec<crate::records::RecordRow>, StoreError> {
    sqlx::query(
        "SELECT r.*,
                (SELECT predicate_uid FROM record_assertion identity
                  WHERE identity.subject_uid = r.uid AND identity.role = 'identity'
                    AND identity.retracted_at IS NULL) AS identity_predicate_uid
           FROM record_assertion a JOIN record r ON r.uid = a.subject_uid
          WHERE a.predicate_uid = ? AND a.object_uid = ?
            AND a.retracted_at IS NULL AND r.deleted_at IS NULL
          ORDER BY a.created_at, a.uid",
    )
    .bind(predicate_uid)
    .bind(object_uid)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map_record)
    .collect()
}

pub async fn objects_from_subject(
    pool: &SqlitePool,
    subject_uid: &str,
    predicate_uid: &str,
) -> Result<Vec<crate::records::RecordRow>, StoreError> {
    sqlx::query(
        "SELECT r.*,
                (SELECT predicate_uid FROM record_assertion identity
                  WHERE identity.subject_uid = r.uid AND identity.role = 'identity'
                    AND identity.retracted_at IS NULL) AS identity_predicate_uid
           FROM record_assertion a JOIN record r ON r.uid = a.object_uid
          WHERE a.subject_uid = ? AND a.predicate_uid = ?
            AND a.retracted_at IS NULL AND r.deleted_at IS NULL
          ORDER BY a.created_at, a.uid",
    )
    .bind(subject_uid)
    .bind(predicate_uid)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map_record)
    .collect()
}

pub async fn edges_of_predicate(
    pool: &SqlitePool,
    predicate_uid: &str,
) -> Result<Vec<Edge>, StoreError> {
    Ok(binary_of_predicate(pool, predicate_uid)
        .await?
        .into_iter()
        .map(|row| Edge {
            from: row.subject_uid,
            to: row.object_uid,
            quantity: row.quantity.map(|value| value.to_f64()),
        })
        .collect())
}

pub async fn edges_of_predicates(
    pool: &SqlitePool,
    predicate_uids: &[String],
) -> Result<Vec<Edge>, StoreError> {
    let mut out = Vec::new();
    for predicate_uid in predicate_uids {
        out.extend(edges_of_predicate(pool, predicate_uid).await?);
    }
    Ok(out)
}

pub async fn object_uids_from_subject(
    pool: &SqlitePool,
    subject_uid: &str,
    predicate_uid: &str,
) -> Result<Vec<String>, StoreError> {
    Ok(sqlx::query_scalar(
        "SELECT object_uid FROM record_assertion
          WHERE subject_uid = ? AND predicate_uid = ?
            AND object_uid IS NOT NULL AND retracted_at IS NULL
          ORDER BY created_at, uid",
    )
    .bind(subject_uid)
    .bind(predicate_uid)
    .fetch_all(pool)
    .await?)
}

pub async fn for_subjects(
    pool: &SqlitePool,
    subject_uids: &[String],
) -> Result<Vec<ProjectedAssertion>, StoreError> {
    if subject_uids.is_empty() {
        return Ok(Vec::new());
    }
    let holes = std::iter::repeat_n("?", subject_uids.len())
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT a.uid, a.subject_uid, a.object_uid, a.predicate_uid,
                c.canonical_name AS predicate, a.quantity_mantissa,
                a.quantity_scale, a.unit_uid
           FROM record_assertion a JOIN concept c ON c.uid = a.predicate_uid
          WHERE a.retracted_at IS NULL AND a.subject_uid IN ({holes})
          ORDER BY a.subject_uid, c.canonical_name, a.object_uid, a.uid"
    );
    let mut query = sqlx::query(&sql);
    for uid in subject_uids {
        query = query.bind(uid);
    }
    query
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| {
            let quantity = if row.get::<Option<String>, _>("quantity_mantissa").is_some() {
                Some(crate::exact::read_decimal(&row, "quantity")?)
            } else {
                None
            };
            Ok(ProjectedAssertion {
                uid: row.get("uid"),
                subject_uid: row.get("subject_uid"),
                predicate_uid: row.get("predicate_uid"),
                predicate: row.get("predicate"),
                object_uid: row.get("object_uid"),
                quantity,
                unit_uid: row.get("unit_uid"),
            })
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProjectedAssertion {
    pub uid: String,
    pub subject_uid: String,
    pub predicate_uid: String,
    pub predicate: String,
    pub object_uid: Option<String>,
    pub quantity: Option<DecimalValue>,
    pub unit_uid: Option<String>,
}

pub async fn record_descendants(
    pool: &SqlitePool,
    root_uid: &str,
    family: &[String],
    include_self: bool,
) -> Result<Vec<String>, StoreError> {
    use std::collections::{HashSet, VecDeque};

    if family.is_empty() {
        return Ok(if include_self {
            vec![root_uid.to_string()]
        } else {
            Vec::new()
        });
    }
    let placeholders = std::iter::repeat_n("?", family.len())
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT subject_uid FROM record_assertion
          WHERE object_uid = ? AND predicate_uid IN ({placeholders})"
    );

    let mut seen: HashSet<String> = HashSet::from([root_uid.to_string()]);
    let mut queue: VecDeque<String> = VecDeque::from([root_uid.to_string()]);
    let mut out = if include_self {
        vec![root_uid.to_string()]
    } else {
        Vec::new()
    };
    while let Some(current) = queue.pop_front() {
        let mut query = sqlx::query(&sql).bind(&current);
        for uid in family {
            query = query.bind(uid);
        }
        for row in query.fetch_all(pool).await? {
            let uid: String = row.get("subject_uid");
            if seen.insert(uid.clone()) {
                out.push(uid.clone());
                queue.push_back(uid);
            }
        }
    }
    Ok(out)
}
