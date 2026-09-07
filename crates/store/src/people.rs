use std::io::{self, Write};

use chrono::{DateTime, SecondsFormat, Utc};
use nucleus::RecordKind;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sqlx::{Sqlite, SqliteConnection, SqlitePool, Transaction};

use crate::StoreError;

pub const NAMESPACE: &str = "lince.person";

pub const STANDING_KEY: &str = "standing";

pub const MAX_PERSON_EXTENSION_BYTES: usize = 64 * 1024;
pub const MAX_STANDING_NOTE_BYTES: usize = 16 * 1024;

pub fn is_standing_field(field: &str) -> bool {
    field == format!("{NAMESPACE}.{STANDING_KEY}")
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Standing {
    pub active: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

pub enum StandingChange {
    Deactivate {
        at: DateTime<Utc>,
        note: Option<String>,
    },
    Reactivate,
}

struct ExtensionSize {
    bytes: usize,
}

impl Write for ExtensionSize {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let total = self
            .bytes
            .checked_add(bytes.len())
            .filter(|total| *total <= MAX_PERSON_EXTENSION_BYTES)
            .ok_or_else(|| io::Error::other("Person extension exceeds its byte limit"))?;
        self.bytes = total;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub async fn compare_and_set_standing_on(
    tx: &mut Transaction<'_, Sqlite>,
    person_uid: &str,
    expected_revision: i64,
    change: StandingChange,
) -> Result<crate::record_revisions::RecordRevision, StoreError> {
    let proposed_standing = match change {
        StandingChange::Deactivate { at, note } => {
            if note
                .as_ref()
                .is_some_and(|note| note.len() > MAX_STANDING_NOTE_BYTES)
            {
                return Err(sqlx::Error::Protocol(
                    "Person standing note exceeds its byte limit".into(),
                ));
            }
            Some(
                serde_json::to_value(Standing {
                    active: false,
                    at: Some(at.to_rfc3339_opts(SecondsFormat::Nanos, true)),
                    note,
                })
                .map_err(|_| sqlx::Error::Protocol("Person standing is not serialisable".into()))?,
            )
        }
        StandingChange::Reactivate => None,
    };
    crate::record_revisions::check_expected_on(tx, person_uid, expected_revision).await?;
    let valid = sqlx::query_scalar::<_, i64>(
        "SELECT CASE WHEN typeof(uid) = 'text' AND typeof(kind) = 'text'
                          AND kind = 'person' AND deleted_at IS NULL
                     THEN 1 ELSE 0 END FROM record WHERE uid = ?",
    )
    .bind(person_uid)
    .fetch_optional(&mut **tx)
    .await?;
    if valid != Some(1) {
        return Err(sqlx::Error::Protocol(
            "Person standing requires an existing undeleted Person".into(),
        ));
    }
    let mut fields = extension_on(tx, person_uid)
        .await?
        .map(|fds| parse_extension(&fds))
        .transpose()?
        .unwrap_or_default();
    standing_from_fields(&fields)?;
    match proposed_standing {
        Some(standing) => {
            fields.insert(STANDING_KEY.to_owned(), standing);
        }
        None => {
            fields.remove(STANDING_KEY);
        }
    }
    let proposed = Value::Object(fields);
    serde_json::to_writer(&mut ExtensionSize { bytes: 0 }, &proposed).map_err(|_| {
        sqlx::Error::Protocol("Proposed Person extension exceeds its byte limit".into())
    })?;
    crate::records::set_extension_on(tx, person_uid, NAMESPACE, &proposed).await?;
    crate::record_revisions::get_on(tx, person_uid).await
}

pub async fn deactivate(
    pool: &SqlitePool,
    person_uid: &str,
    at: &str,
    note: Option<&str>,
) -> Result<(), StoreError> {
    write(
        pool,
        person_uid,
        Some(Standing {
            active: false,
            at: Some(at.to_string()),
            note: note.map(str::to_string),
        }),
    )
    .await
}

pub async fn reactivate(pool: &SqlitePool, person_uid: &str) -> Result<(), StoreError> {
    write(pool, person_uid, None).await
}

async fn write(
    pool: &SqlitePool,
    person_uid: &str,
    standing: Option<Standing>,
) -> Result<(), StoreError> {
    let value = match standing {
        Some(standing) => serde_json::to_value(standing).map_err(|error| {
            sqlx::Error::Protocol(format!("Person standing is not serialisable: {error}"))
        })?,
        None => serde_json::Value::Null,
    };
    crate::records::set_extension(
        pool,
        person_uid,
        NAMESPACE,
        &serde_json::json!({ STANDING_KEY: value }),
    )
    .await
}

pub async fn standing(pool: &SqlitePool, person_uid: &str) -> Result<Option<Standing>, StoreError> {
    let mut connection = pool.acquire().await?;
    standing_on(&mut connection, person_uid).await
}

pub async fn standing_on(
    connection: &mut SqliteConnection,
    person_uid: &str,
) -> Result<Option<Standing>, StoreError> {
    let fds = extension_on(connection, person_uid).await?;
    parse_standing(fds.as_deref())
}

async fn extension_on(
    connection: &mut SqliteConnection,
    person_uid: &str,
) -> Result<Option<String>, StoreError> {
    if !nucleus::valid_uid(person_uid, "r") {
        return Err(sqlx::Error::Protocol("invalid Person identity".into()));
    }
    let row = sqlx::query_as::<_, (i64, i64, Option<String>)>(
        "WITH bounded AS (
             SELECT COUNT(*) AS matches,
                    COALESCE(SUM(CASE WHEN typeof(record_uid) = 'text'
                         AND typeof(namespace) = 'text' AND typeof(fds) = 'text'
                         AND length(CAST(fds AS BLOB)) <= ? THEN 0 ELSE 1 END), 0) AS invalid
               FROM record_extension
              WHERE CAST(record_uid AS TEXT) = ? AND CAST(namespace AS TEXT) = ?
         )
         SELECT b.matches, b.invalid, e.fds FROM bounded b
           LEFT JOIN record_extension e ON b.matches = 1 AND b.invalid = 0
                AND e.record_uid = ? AND e.namespace = ? LIMIT 1",
    )
    .bind(MAX_PERSON_EXTENSION_BYTES as i64)
    .bind(person_uid)
    .bind(NAMESPACE)
    .bind(person_uid)
    .bind(NAMESPACE)
    .fetch_one(&mut *connection)
    .await?;
    match row {
        (0, 0, None) => Ok(None),
        (1, 0, Some(fds)) => Ok(Some(fds)),
        _ => Err(sqlx::Error::Protocol(
            "Person extension has invalid storage or exceeds its byte limit".into(),
        )),
    }
}

fn parse_standing(fds: Option<&str>) -> Result<Option<Standing>, StoreError> {
    let Some(fds) = fds else {
        return Ok(None);
    };
    standing_from_fields(&parse_extension(fds)?)
}

fn parse_extension(fds: &str) -> Result<Map<String, Value>, StoreError> {
    let extension: Value = serde_json::from_str(fds).map_err(|error| {
        sqlx::Error::Protocol(format!("Person extension is not valid JSON: {error}"))
    })?;
    match extension {
        Value::Object(fields) => Ok(fields),
        _ => Err(sqlx::Error::Protocol(
            "Person extension is not a JSON object".into(),
        )),
    }
}

fn standing_from_fields(fields: &Map<String, Value>) -> Result<Option<Standing>, StoreError> {
    match fields.get(STANDING_KEY) {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(value) => serde_json::from_value(value.clone())
            .map(Some)
            .map_err(|error| {
                sqlx::Error::Protocol(format!("Person standing is not readable: {error}"))
            }),
    }
}

pub async fn is_active(pool: &SqlitePool, person_uid: &str) -> Result<bool, StoreError> {
    let mut connection = pool.acquire().await?;
    is_active_on(&mut connection, person_uid).await
}

pub async fn is_active_on(
    connection: &mut SqliteConnection,
    person_uid: &str,
) -> Result<bool, StoreError> {
    if !nucleus::valid_uid(person_uid, "r") {
        return Err(sqlx::Error::Protocol("invalid Person identity".into()));
    }
    let row = sqlx::query_as::<_, (Option<String>, i64)>(
        "SELECT CASE WHEN typeof(kind) = 'text'
                     AND length(CAST(kind AS BLOB)) <= 32 THEN kind END,
                CASE WHEN deleted_at IS NULL THEN 1
                     WHEN typeof(deleted_at) = 'text' THEN 0 ELSE -1 END
           FROM record WHERE uid = ?",
    )
    .bind(person_uid)
    .fetch_optional(&mut *connection)
    .await?;
    let Some((kind, present)) = row else {
        return Ok(false);
    };
    let kind = kind
        .as_deref()
        .and_then(RecordKind::parse)
        .ok_or_else(|| sqlx::Error::Protocol("Person Record has an invalid stored kind".into()))?;
    if present < 0 {
        return Err(sqlx::Error::Protocol(
            "Person Record has invalid deletion storage".into(),
        ));
    }
    if kind != RecordKind::Person || present == 0 {
        return Ok(false);
    }
    Ok(standing_on(connection, person_uid)
        .await?
        .is_none_or(|standing| standing.active))
}

pub async fn deactivated(pool: &SqlitePool) -> Result<Vec<(String, Standing)>, StoreError> {
    let rows = sqlx::query_as::<_, (String, String)>(
        "SELECT record_uid, fds FROM record_extension WHERE namespace = ?",
    )
    .bind(NAMESPACE)
    .fetch_all(pool)
    .await?;
    let mut out: Vec<(String, Standing)> = rows
        .into_iter()
        .filter_map(|(uid, fds)| {
            let value: serde_json::Value = serde_json::from_str(&fds).ok()?;
            let standing: Standing =
                serde_json::from_value(value.get(STANDING_KEY)?.clone()).ok()?;
            (!standing.active).then_some((uid, standing))
        })
        .collect();
    out.sort_by(|a, b| b.1.at.cmp(&a.1.at));
    Ok(out)
}
