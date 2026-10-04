use crate::StoreError;
use sqlx::{SqliteConnection, SqlitePool};
use std::collections::BTreeSet;

pub const MAX_ROLES: usize = 64;
pub const RECOVERY_PERMISSIONS: &[&str] = &[
    "user:create",
    "user:assign_role",
    "role:update",
    "permission:assign",
];

pub async fn ids_on(
    connection: &mut SqliteConnection,
    person: &str,
) -> Result<Vec<i64>, StoreError> {
    let roles = sqlx::query_scalar::<_, i64>("SELECT role_id FROM person_access WHERE person_uid = ? AND role_id IS NOT NULL UNION SELECT role_id FROM person_role WHERE person_uid = ? ORDER BY role_id LIMIT 65")
        .bind(person).bind(person).fetch_all(&mut *connection).await?;
    if roles.len() > MAX_ROLES || roles.iter().any(|id| *id <= 0) {
        return Err(sqlx::Error::Protocol(
            "Person Role membership exceeds its bounds".into(),
        ));
    }
    Ok(roles)
}

pub async fn ids(pool: &SqlitePool, person: &str) -> Result<Vec<i64>, StoreError> {
    ids_on(&mut *pool.acquire().await?, person).await
}

pub async fn permissions_on(
    connection: &mut SqliteConnection,
    person: &str,
) -> Result<Vec<String>, StoreError> {
    let mut permissions = BTreeSet::new();
    for role in ids_on(connection, person).await? {
        permissions.extend(crate::auth::role_permission_keys_by_id_on(connection, role).await?);
    }
    Ok(permissions.into_iter().collect())
}

pub async fn names(pool: &SqlitePool, person: &str) -> Result<Vec<String>, StoreError> {
    let mut connection = pool.acquire().await?;
    let mut names = Vec::new();
    for id in ids_on(&mut connection, person).await? {
        names.push(
            sqlx::query_scalar("SELECT name FROM role WHERE id = ?")
                .bind(id)
                .fetch_one(&mut *connection)
                .await?,
        );
    }
    Ok(names)
}

pub async fn replace_on(
    connection: &mut SqliteConnection,
    person: &str,
    roles: &[i64],
    expected: i64,
) -> Result<crate::auth::PersonAccess, StoreError> {
    let roles: BTreeSet<_> = roles.iter().copied().collect();
    if roles.len() > MAX_ROLES || roles.iter().any(|id| *id <= 0) {
        return Err(sqlx::Error::Protocol(
            "Assign at most 64 existing Roles".into(),
        ));
    }
    for role in &roles {
        if !sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM role WHERE id = ?)")
            .bind(role)
            .fetch_one(&mut *connection)
            .await?
        {
            return Err(sqlx::Error::Protocol("Choose existing Roles".into()));
        }
    }
    crate::auth::compare_and_set_role_on(connection, person, roles.first().copied(), expected)
        .await?;
    sqlx::query("DELETE FROM person_role WHERE person_uid = ?")
        .bind(person)
        .execute(&mut *connection)
        .await?;
    for role in roles.iter().skip(1) {
        sqlx::query("INSERT INTO person_role (person_uid, role_id) VALUES (?, ?)")
            .bind(person)
            .bind(role)
            .execute(&mut *connection)
            .await?;
    }
    crate::auth::person_access_on(connection, person)
        .await?
        .ok_or(sqlx::Error::RowNotFound)
}

pub fn has_recovery(permissions: &[String]) -> bool {
    RECOVERY_PERMISSIONS
        .iter()
        .all(|required| permissions.iter().any(|key| key == required))
}

pub async fn recovery_people_on(
    connection: &mut SqliteConnection,
) -> Result<Vec<String>, StoreError> {
    let people = sqlx::query_scalar::<_, String>("SELECT a.person_uid FROM person_access a JOIN record p ON p.uid = a.person_uid WHERE p.kind = 'person' AND p.deleted_at IS NULL LIMIT 4097")
        .fetch_all(&mut *connection).await?;
    if people.len() > 4096 {
        return Err(sqlx::Error::Protocol(
            "Recovery catalogue exceeds its bounds".into(),
        ));
    }
    let mut result = Vec::new();
    for person in people {
        if crate::people::is_active_on(connection, &person).await?
            && has_recovery(&permissions_on(connection, &person).await?)
        {
            result.push(person);
        }
    }
    Ok(result)
}

pub async fn require_recovery_on(
    connection: &mut SqliteConnection,
    was_recoverable: bool,
) -> Result<(), StoreError> {
    if was_recoverable && recovery_people_on(connection).await?.is_empty() {
        return Err(sqlx::Error::Protocol(
            "Keep at least one active Actor with access-management capabilities".into(),
        ));
    }
    Ok(())
}
