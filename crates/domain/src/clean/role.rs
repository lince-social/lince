use serde::{Deserialize, Serialize};

#[derive(sqlx::FromRow, Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct Role {
    pub id: i64,
    pub name: String,
}
