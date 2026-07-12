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

/// The hard daily interruption budget (blueprint XIII.2).
pub async fn attention_budget(pool: &SqlitePool) -> Result<i64, StoreError> {
    ensure_default(pool).await?;
    Ok(
        sqlx::query("SELECT attention_budget_per_day FROM configuration WHERE id = 1")
            .fetch_one(pool)
            .await?
            .get("attention_budget_per_day"),
    )
}

pub async fn set_attention_budget(pool: &SqlitePool, per_day: i64) -> Result<(), StoreError> {
    ensure_default(pool).await?;
    sqlx::query("UPDATE configuration SET attention_budget_per_day = ? WHERE id = 1")
        .bind(per_day)
        .execute(pool)
        .await?;
    Ok(())
}

/// Set the interface language (installer/staged setup import).
pub async fn set_language(pool: &SqlitePool, language: &str) -> Result<(), StoreError> {
    ensure_default(pool).await?;
    sqlx::query("UPDATE configuration SET language = ? WHERE id = 1")
        .bind(language)
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
            // The schema folds the toggle into the seconds value: zero means
            // "don't show", any positive number enables it (0001_init.sql).
            show_command_notifications: r.get::<f64, _>("command_notification_seconds") > 0.0,
            command_notification_seconds: r.get("command_notification_seconds"),
            delete_confirmation: r.get::<i64, _>("delete_confirmation") != 0,
            error_toast_seconds: r.get("error_toast_seconds"),
            keybinding_mode: r.get("keybinding_mode"),
        }))
}
