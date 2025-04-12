use serde::{Deserialize, Serialize};

#[derive(sqlx::FromRow, Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct QueriedView {
    pub id: u32,
    pub quantity: i32,
    pub name: String,
    pub query: String,
}
