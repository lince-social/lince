use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::StoreError;

pub const NAMESPACE: &str = "lince.person";

pub const STANDING_KEY: &str = "standing";

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
    let Some(fds) = crate::records::get_extension(pool, person_uid, NAMESPACE).await? else {
        return Ok(None);
    };
    match fds.get(STANDING_KEY) {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(value) => Ok(serde_json::from_value(value.clone()).ok()),
    }
}

pub async fn is_active(pool: &SqlitePool, person_uid: &str) -> Result<bool, StoreError> {
    Ok(standing(pool, person_uid)
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
