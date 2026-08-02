//! One canonical Record assertion store. Unary rows are tags/classifications;
//! binary rows are relationships. Identity is a constrained unary role.

use chrono::Utc;
use nucleus::DecimalValue;
use nucleus::graph::Edge;
use sqlx::{Row, SqlitePool};

use crate::StoreError;

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
    let quantity = if row.get::<Option<String>, _>("quantity_mantissa").is_some() {
        Some(crate::exact::read_decimal(&row, "quantity")?)
    } else {
        None
    };
    Ok(AssertionRow {
        uid: row.get("uid"),
        subject_uid: row.get("subject_uid"),
        predicate_uid: row.get("predicate_uid"),
        object_uid: row.get("object_uid"),
        role: row.get("role"),
        quantity,
        unit_uid: row.get("unit_uid"),
        asserted_by: row.get("asserted_by"),
        created_at: row.get("created_at"),
        retracted_at: row.get("retracted_at"),
        retracted_by: row.get("retracted_by"),
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

/// Import one active ordinary assertion without changing its global identity.
/// Identity assertions travel in the Record seed and are restored through the
/// constrained `set_identity` path.
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
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
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
    Ok(sqlx::query(
        "UPDATE record_assertion SET retracted_at = ?, retracted_by = ?
          WHERE uid = ? AND retracted_at IS NULL",
    )
    .bind(Utc::now().to_rfc3339())
    .bind(actor_uid)
    .bind(uid)
    .execute(pool)
    .await?
    .rows_affected()
        > 0)
}

pub async fn set_identity(
    pool: &SqlitePool,
    subject_uid: &str,
    predicate_uid: Option<&str>,
    actor_uid: Option<&str>,
) -> Result<Option<String>, StoreError> {
    let mut transaction = pool.begin().await?;
    let now = Utc::now().to_rfc3339();
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
            sqlx::query("UPDATE record_assertion SET role = 'identity' WHERE uid = ?")
                .bind(&uid)
                .execute(&mut *transaction)
                .await?;
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
