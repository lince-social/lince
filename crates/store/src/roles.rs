use crate::{StoreError, auth};
use sqlx::{SqliteConnection, SqlitePool};

pub const MAX_NAME_BYTES: usize = 100;

#[derive(Debug, Clone)]
pub struct Role {
    pub id: i64,
    pub name: String,
    pub revision: i64,
    pub permissions: Vec<String>,
}

pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.trim() == name
        && name.len() <= MAX_NAME_BYTES
        && !name.chars().any(char::is_control)
}

pub async fn get_on(
    connection: &mut SqliteConnection,
    id: i64,
) -> Result<Option<Role>, StoreError> {
    let row = sqlx::query_as::<_, (i64, String, i64)>(
        "SELECT r.id, r.name, v.revision FROM role r JOIN role_permission_revision v ON v.role_id = r.id WHERE r.id = ?",
    ).bind(id).fetch_optional(&mut *connection).await?;
    let Some((id, name, revision)) = row else {
        return Ok(None);
    };
    let permissions = auth::role_permission_keys_by_id_on(connection, id).await?;
    Ok(Some(Role {
        id,
        name,
        revision,
        permissions,
    }))
}

pub async fn catalog(
    pool: &SqlitePool,
    include_permissions: bool,
) -> Result<Vec<Role>, StoreError> {
    let mut tx = pool.begin().await?;
    let rows = sqlx::query_as::<_, (i64, String, i64)>(
        "SELECT r.id, r.name, v.revision FROM role r JOIN role_permission_revision v ON v.role_id = r.id ORDER BY r.name",
    ).fetch_all(&mut *tx).await?;
    let mut roles = Vec::with_capacity(rows.len());
    for (id, name, revision) in rows {
        let permissions = if include_permissions {
            auth::role_permission_keys_by_id_on(&mut tx, id).await?
        } else {
            Vec::new()
        };
        roles.push(Role {
            id,
            name,
            revision,
            permissions,
        });
    }
    tx.commit().await?;
    Ok(roles)
}

pub async fn name_exists_on(
    connection: &mut SqliteConnection,
    name: &str,
    except: i64,
) -> Result<bool, StoreError> {
    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM role WHERE name = ? AND id <> ?)")
        .bind(name)
        .bind(except)
        .fetch_one(connection)
        .await
}

pub async fn assigned_on(connection: &mut SqliteConnection, id: i64) -> Result<bool, StoreError> {
    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM person_access WHERE role_id = ?)")
        .bind(id)
        .fetch_one(connection)
        .await
}

pub async fn rename_on(
    connection: &mut SqliteConnection,
    id: i64,
    name: &str,
) -> Result<(), StoreError> {
    if !valid_name(name) {
        return Err(sqlx::Error::Protocol("Invalid Role name".into()));
    }
    sqlx::query("UPDATE role SET name = ? WHERE id = ?")
        .bind(name)
        .bind(id)
        .execute(connection)
        .await?;
    Ok(())
}

pub async fn delete_on(connection: &mut SqliteConnection, id: i64) -> Result<(), StoreError> {
    sqlx::query("DELETE FROM role WHERE id = ?")
        .bind(id)
        .execute(connection)
        .await?;
    Ok(())
}
