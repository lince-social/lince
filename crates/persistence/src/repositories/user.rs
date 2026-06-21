use async_trait::async_trait;
use domain::clean::app_user::AppUser;
use sqlx::{Pool, Sqlite};
use std::{
    io::{Error, ErrorKind},
    sync::Arc,
};

#[derive(sqlx::FromRow)]
struct AuthUserRow {
    id: i64,
    name: String,
    username: String,
    password_hash: String,
    role_id: i64,
    role: String,
    permissions: Option<String>,
    created_at: String,
    updated_at: String,
}

#[async_trait]
pub trait UserRepository: Send + Sync {
    async fn get_by_username(&self, username: &str) -> Result<Option<AppUser>, Error>;
    async fn list_auth_users(&self) -> Result<Vec<AppUser>, Error>;
}

pub struct UserRepositoryImpl {
    pool: Arc<Pool<Sqlite>>,
}

impl UserRepositoryImpl {
    pub fn new(pool: Arc<Pool<Sqlite>>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl UserRepository for UserRepositoryImpl {
    async fn get_by_username(&self, username: &str) -> Result<Option<AppUser>, Error> {
        sqlx::query_as::<_, AuthUserRow>(
            "
            SELECT
                u.id,
                u.name,
                u.username,
                u.password_hash,
                u.role_id,
                r.name AS role,
                GROUP_CONCAT(p.subject || ':' || p.action, ',') AS permissions,
                u.created_at,
                u.updated_at
            FROM app_user u
            JOIN role r ON r.id = u.role_id
            LEFT JOIN role_permission rp ON rp.role_id = r.id
            LEFT JOIN permission p ON p.id = rp.permission_id
            WHERE u.username = ?
            GROUP BY u.id
            LIMIT 1
            ",
        )
        .bind(username)
        .fetch_optional(&*self.pool)
        .await
        .map(|row| row.map(auth_user_from_row))
        .map_err(|error| Error::new(ErrorKind::InvalidData, error))
    }

    async fn list_auth_users(&self) -> Result<Vec<AppUser>, Error> {
        sqlx::query_as::<_, AuthUserRow>(
            "
            SELECT
                u.id,
                u.name,
                u.username,
                u.password_hash,
                u.role_id,
                r.name AS role,
                GROUP_CONCAT(p.subject || ':' || p.action, ',') AS permissions,
                u.created_at,
                u.updated_at
            FROM app_user u
            JOIN role r ON r.id = u.role_id
            LEFT JOIN role_permission rp ON rp.role_id = r.id
            LEFT JOIN permission p ON p.id = rp.permission_id
            GROUP BY u.id
            ",
        )
        .fetch_all(&*self.pool)
        .await
        .map(|rows| rows.into_iter().map(auth_user_from_row).collect())
        .map_err(|error| Error::new(ErrorKind::InvalidData, error))
    }
}

fn auth_user_from_row(row: AuthUserRow) -> AppUser {
    let mut permissions = row
        .permissions
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    permissions.sort();
    permissions.dedup();

    AppUser {
        id: row.id,
        name: row.name,
        username: row.username,
        password_hash: row.password_hash,
        role_id: row.role_id,
        role: row.role,
        permissions,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}
