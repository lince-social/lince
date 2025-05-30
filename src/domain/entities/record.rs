use serde::{Deserialize, Serialize};

#[derive(sqlx::FromRow, Debug, PartialEq, Deserialize, Serialize)]
pub struct Record {
    pub id: u32,
    pub quantity: f64,
    pub head: String,
    pub body: String,
}

#[derive(sqlx::FromRow, Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct RecordSchemaCreate {
    pub head: String,
}
