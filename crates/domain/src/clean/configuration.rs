use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;

#[derive(Serialize, Deserialize, Clone, FromRow)]
pub struct Configuration {
    pub id: u32,
    pub quantity: i64,
    pub name: String,
    pub language: String,
    pub style: String,
    pub timezone: i64,
    pub show_command_notifications: i64,
    pub command_notification_seconds: f64,
    pub delete_confirmation: i64,
}
