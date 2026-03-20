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
            "SELECT id, name, username, password_hash FROM app_user WHERE username = ? LIMIT 1",
        )
        .bind(username)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|error| Error::new(ErrorKind::InvalidData, error))
    }
}
