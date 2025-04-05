use serde::{Deserialize, Serialize};

#[derive(sqlx::FromRow, Debug, PartialEq)]
pub struct Record {
    pub id: u32,
    pub quantity: f32, //default 1
    pub head: String,  // default None
    pub body: String,  // default None
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
