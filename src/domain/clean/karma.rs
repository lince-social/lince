use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;

#[derive(Deserialize, Serialize, Debug, Clone, FromRow)]
pub struct Karma {
    pub id: u32,
    pub quantity: i32,
    pub name: String,
    pub condition: String,
    pub operator: String,
    pub consequence: String,
}
