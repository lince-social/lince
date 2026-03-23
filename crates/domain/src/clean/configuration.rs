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
    pub error_toast_seconds: f64,
    pub keybinding_mode: i64,
    pub bucket_enabled: i64,
    pub bucket_username: Option<String>,
    pub bucket_password: Option<String>,
    pub bucket_uri: Option<String>,
    pub bucket_name: Option<String>,
    pub bucket_region: Option<String>,
}
