use async_trait::async_trait;
use domain::clean::app_user::AppUser;
use sqlx::{Pool, Sqlite};
use std::{
    io::{Error, ErrorKind},
    sync::Arc,
};

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
        sqlx::query_as::<_, AppUser>(
            "
            SELECT
                u.id,
                u.name,
                u.username,
                u.password_hash,
                u.role_id,
                r.name AS role,
                u.created_at,
                u.updated_at
            FROM app_user u
            JOIN role r ON r.id = u.role_id
            WHERE u.username = ?
            LIMIT 1
            ",
        )
        .bind(username)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|error| Error::new(ErrorKind::InvalidData, error))
    }

    async fn list_auth_users(&self) -> Result<Vec<AppUser>, Error> {
        sqlx::query_as::<_, AppUser>(
            "
            SELECT
                u.id,
                u.name,
                u.username,
                u.password_hash,
                u.role_id,
                r.name AS role,
                u.created_at,
                u.updated_at
            FROM app_user u
            JOIN role r ON r.id = u.role_id
            ",
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|error| Error::new(ErrorKind::InvalidData, error))
    }
}
