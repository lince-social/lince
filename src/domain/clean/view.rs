use serde::{Deserialize, Serialize};

#[allow(dead_code)]
#[derive(sqlx::FromRow, Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct View {
    pub id: u32,
    pub name: String,
    pub query: String,
}
