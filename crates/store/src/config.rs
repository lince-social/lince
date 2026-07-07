//! The singleton `configuration` row. Configuration is native structured state
//! (not a Ledger record), so it lives in a typed table — the column DEFAULTs are
//! the default policy, an `UPDATE` is the override. Reads always resolve against
//! a materialized row (see `ensure_default`).

use sqlx::{Row, SqlitePool};

use crate::StoreError;

#[derive(Debug, Clone, PartialEq)]
pub struct Configuration {
    pub name: String,
    pub language: String,
    pub timezone: i64,
    pub style: String,
    pub show_command_notifications: bool,
    pub command_notification_seconds: f64,
    pub delete_confirmation: bool,
    pub error_toast_seconds: f64,
    pub keybinding_mode: i64,
}

/// Materialize the singleton row (id = 1) if it is missing. Idempotent — the
/// column DEFAULTs supply every value, so this never overwrites user changes.
pub async fn ensure_default(pool: &SqlitePool) -> Result<(), StoreError> {
    sqlx::query("INSERT OR IGNORE INTO configuration (id) VALUES (1)")
        .execute(pool)
        .await?;
    Ok(())
}

/// Read the singleton configuration (present after `ensure_default`).
pub async fn get(pool: &SqlitePool) -> Result<Option<Configuration>, StoreError> {
    Ok(sqlx::query("SELECT * FROM configuration WHERE id = 1")
        .fetch_optional(pool)
        .await?
        .map(|r| Configuration {
            name: r.get("name"),
            language: r.get("language"),
            timezone: r.get("timezone"),
            style: r.get("style"),
            show_command_notifications: r.get::<i64, _>("show_command_notifications") != 0,
            command_notification_seconds: r.get("command_notification_seconds"),
            delete_confirmation: r.get::<i64, _>("delete_confirmation") != 0,
            error_toast_seconds: r.get("error_toast_seconds"),
            keybinding_mode: r.get("keybinding_mode"),
        }))
}
