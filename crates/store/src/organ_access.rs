use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::StoreError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Status {
    pub organ_uid: String,
    pub generation: i64,
    pub granted: bool,
}

pub async fn for_peer(pool: &SqlitePool, peer: &str) -> Result<Status, StoreError> {
    let mut tx = pool.begin().await?;
    let organ_uid: String = sqlx::query_scalar(
        "SELECT uid FROM record WHERE slug = 'local-organ' AND kind = 'organ' AND deleted_at IS NULL",
    )
    .fetch_one(&mut *tx)
    .await?;
    let (generation, person): (i64, Option<String>) = sqlx::query_as(
        "SELECT COALESCE((SELECT generation FROM organ_login_generation WHERE organ_uid = ?), 0), (SELECT person_uid FROM organ_login WHERE organ_uid = ?)",
    )
    .bind(peer)
    .bind(peer)
    .fetch_one(&mut *tx)
    .await?;
    let granted = match person {
        Some(person) => crate::people::is_active_on(&mut tx, &person).await?,
        None => false,
    };
    tx.commit().await?;
    Ok(Status {
        organ_uid,
        generation,
        granted,
    })
}

pub async fn observe(
    pool: &SqlitePool,
    authenticated: &str,
    status: &Status,
) -> Result<(), StoreError> {
    if authenticated != status.organ_uid || status.generation < 0 {
        return Err(sqlx::Error::Protocol(
            "Organ access status does not match the authenticated host".into(),
        ));
    }
    sqlx::query("INSERT INTO organ_access_catalog (organ_uid, generation, granted, observed_at) SELECT ?, ?, ?, ? WHERE EXISTS(SELECT 1 FROM organ_contact WHERE record_uid = ? AND trust = 'known') ON CONFLICT(organ_uid) DO UPDATE SET generation = excluded.generation, granted = excluded.granted, observed_at = excluded.observed_at WHERE excluded.generation >= organ_access_catalog.generation")
        .bind(authenticated).bind(status.generation).bind(status.granted)
        .bind(nucleus::execution::now().to_rfc3339()).bind(authenticated)
        .execute(pool).await?;
    Ok(())
}

pub async fn get(pool: &SqlitePool, organ: &str) -> Result<Option<Status>, StoreError> {
    Ok(sqlx::query_as::<_, (String, i64, bool)>("SELECT a.organ_uid, a.generation, a.granted FROM organ_access_catalog a JOIN organ_contact c ON c.record_uid = a.organ_uid WHERE a.organ_uid = ? AND c.trust = 'known'")
        .bind(organ).fetch_optional(pool).await?
        .map(|(organ_uid, generation, granted)| Status { organ_uid, generation, granted }))
}
