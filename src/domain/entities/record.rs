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
    // pub quantity: f32,
    pub head: String,
    // pub body: String,
    // pub quantity: Option<f32>,
    // pub head: Option<String>,
    // pub body: Option<String>,
}
