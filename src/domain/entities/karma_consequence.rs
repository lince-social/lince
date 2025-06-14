use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;

#[derive(FromRow, Deserialize, Serialize, Debug, Clone)]
pub struct KarmaConsequence {
    pub id: u32,
    pub quantity: i32,
    pub name: String,
    pub consequence: String,
}
