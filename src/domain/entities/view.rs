use serde::{Deserialize, Serialize};

#[derive(sqlx::FromRow, Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct View {
    pub id: u32,
    pub name: String,
    pub query: String,
}
