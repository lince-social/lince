use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;

#[derive(Debug, Clone, Deserialize, Serialize, FromRow)]
pub struct Frequency {
    pub id: u32,
    pub quantity: f64,
    pub name: String,
    pub day_week: String,
    pub months: f64,
    pub days: f64,
    pub seconds: f64,
    pub next_date: String,
    pub finish_date: Option<String>,
    pub catch_up_sum: u32,
}
