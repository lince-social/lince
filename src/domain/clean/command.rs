use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;

#[derive(Serialize, Deserialize, Debug, Clone, FromRow)]
pub struct Command {
    pub id: u32,
    pub quantity: f64,
    pub name: String,
    pub command: String,
}
