use serde::{Deserialize, Serialize};

#[derive(sqlx::FromRow, Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct AppUser {
    pub id: i64,
    pub name: String,
    pub username: String,
    pub password_hash: String,
}
