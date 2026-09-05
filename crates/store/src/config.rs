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
    pub transfer_reservation_default: String,
    pub transfer_application_formula: String,
    pub transfer_remainder_policy: String,
}

pub async fn ensure_default(pool: &SqlitePool) -> Result<(), StoreError> {
    sqlx::query("INSERT OR IGNORE INTO configuration (id) VALUES (1)")
        .execute(pool)
        .await?;
    Ok(())
}

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

pub async fn set_language(pool: &SqlitePool, language: &str) -> Result<(), StoreError> {
    ensure_default(pool).await?;
    sqlx::query("UPDATE configuration SET language = ? WHERE id = 1")
        .bind(language)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn get(pool: &SqlitePool) -> Result<Option<Configuration>, StoreError> {
    Ok(sqlx::query("SELECT * FROM configuration WHERE id = 1")
        .fetch_optional(pool)
        .await?
        .map(|r| Configuration {
            name: r.get("name"),
            language: r.get("language"),
            timezone: r.get("timezone"),
            style: r.get("style"),
            show_command_notifications: r.get::<f64, _>("command_notification_seconds") > 0.0,
            command_notification_seconds: r.get("command_notification_seconds"),
            delete_confirmation: r.get::<i64, _>("delete_confirmation") != 0,
            error_toast_seconds: r.get("error_toast_seconds"),
            keybinding_mode: r.get("keybinding_mode"),
            transfer_reservation_default: r.get("transfer_reservation_default"),
            transfer_application_formula: r.get("transfer_application_formula"),
            transfer_remainder_policy: r.get("transfer_remainder_policy"),
        }))
}

pub async fn transfer_reservation_default(pool: &SqlitePool) -> Result<String, StoreError> {
    ensure_default(pool).await?;
    Ok(
        sqlx::query_scalar("SELECT transfer_reservation_default FROM configuration WHERE id = 1")
            .fetch_one(pool)
            .await?,
    )
}

pub async fn set_transfer_reservation_default(
    pool: &SqlitePool,
    reserve_from: &str,
) -> Result<(), StoreError> {
    if !matches!(reserve_from, "none" | "proposed" | "agreed" | "active") {
        return Err(sqlx::Error::Protocol(format!(
            "unknown reservation point `{reserve_from}`"
        )));
    }
    ensure_default(pool).await?;
    sqlx::query(
        "UPDATE configuration
         SET transfer_reservation_default = ?, updated_at = CURRENT_TIMESTAMP WHERE id = 1",
    )
    .bind(reserve_from)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn transfer_application_formula(pool: &SqlitePool) -> Result<String, StoreError> {
    ensure_default(pool).await?;
    Ok(
        sqlx::query_scalar("SELECT transfer_application_formula FROM configuration WHERE id = 1")
            .fetch_one(pool)
            .await?,
    )
}

pub async fn set_transfer_application_formula(
    pool: &SqlitePool,
    formula: &str,
) -> Result<(), StoreError> {
    let formula = formula.trim();
    if formula.is_empty() || formula.chars().count() > 2_000 {
        return Err(sqlx::Error::Protocol(
            "application formula must contain 1 to 2000 characters".into(),
        ));
    }
    ensure_default(pool).await?;
    sqlx::query(
        "UPDATE configuration
         SET transfer_application_formula = ?, updated_at = CURRENT_TIMESTAMP WHERE id = 1",
    )
    .bind(formula)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn transfer_remainder_policy(pool: &SqlitePool) -> Result<String, StoreError> {
    ensure_default(pool).await?;
    Ok(
        sqlx::query_scalar("SELECT transfer_remainder_policy FROM configuration WHERE id = 1")
            .fetch_one(pool)
            .await?,
    )
}

pub async fn set_transfer_remainder_policy(
    pool: &SqlitePool,
    policy: &str,
) -> Result<(), StoreError> {
    if !matches!(policy, "visible" | "local_draft") {
        return Err(sqlx::Error::Protocol(format!(
            "unknown Transfer remainder policy `{policy}`"
        )));
    }
    ensure_default(pool).await?;
    sqlx::query(
        "UPDATE configuration
         SET transfer_remainder_policy = ?, updated_at = CURRENT_TIMESTAMP WHERE id = 1",
    )
    .bind(policy)
    .execute(pool)
    .await?;
    Ok(())
}
