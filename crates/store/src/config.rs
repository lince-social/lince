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
    pub interface_close_suspends: bool,
    pub interface_storage: InterfaceStorage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InterfaceStorage {
    pub snapshot_seconds: u64,
    pub history_seconds: u64,
    pub history_count: usize,
}

impl Default for InterfaceStorage {
    fn default() -> Self {
        Self {
            snapshot_seconds: 30,
            history_seconds: 300,
            history_count: 10,
        }
    }
}

impl InterfaceStorage {
    pub fn validate(self) -> Result<Self, StoreError> {
        if !(1..=86400).contains(&self.snapshot_seconds)
            || !(self.snapshot_seconds..=604800).contains(&self.history_seconds)
            || !(1..=100).contains(&self.history_count)
        {
            return Err(sqlx::Error::Protocol(
                "invalid interface snapshot or history settings".into(),
            ));
        }
        Ok(self)
    }
}

pub async fn interface_storage(pool: &SqlitePool) -> Result<InterfaceStorage, StoreError> {
    ensure_default(pool).await?;
    let row = sqlx::query("SELECT interface_snapshot_seconds, interface_history_seconds, interface_history_count FROM configuration WHERE id = 1")
        .fetch_one(pool).await?;
    InterfaceStorage {
        snapshot_seconds: row.get::<i64, _>("interface_snapshot_seconds") as u64,
        history_seconds: row.get::<i64, _>("interface_history_seconds") as u64,
        history_count: row.get::<i64, _>("interface_history_count") as usize,
    }
    .validate()
}

pub async fn set_interface_storage(
    pool: &SqlitePool,
    settings: InterfaceStorage,
) -> Result<(), StoreError> {
    settings.validate()?;
    ensure_default(pool).await?;
    sqlx::query("UPDATE configuration SET interface_snapshot_seconds = ?, interface_history_seconds = ?, interface_history_count = ?, updated_at = CURRENT_TIMESTAMP WHERE id = 1")
        .bind(settings.snapshot_seconds as i64)
        .bind(settings.history_seconds as i64)
        .bind(settings.history_count as i64)
        .execute(pool).await?;
    Ok(())
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
            interface_close_suspends: r.get::<i64, _>("interface_close_suspends") != 0,
            interface_storage: InterfaceStorage {
                snapshot_seconds: r.get::<i64, _>("interface_snapshot_seconds") as u64,
                history_seconds: r.get::<i64, _>("interface_history_seconds") as u64,
                history_count: r.get::<i64, _>("interface_history_count") as usize,
            },
        }))
}

pub async fn interface_close_suspends(pool: &SqlitePool) -> Result<bool, StoreError> {
    ensure_default(pool).await?;
    Ok(
        sqlx::query_scalar("SELECT interface_close_suspends FROM configuration WHERE id = 1")
            .fetch_one(pool)
            .await?,
    )
}

pub async fn set_interface_close_suspends(
    pool: &SqlitePool,
    enabled: bool,
) -> Result<(), StoreError> {
    ensure_default(pool).await?;
    sqlx::query(
        "UPDATE configuration SET interface_close_suspends = ?, updated_at = CURRENT_TIMESTAMP WHERE id = 1",
    )
    .bind(i64::from(enabled))
    .execute(pool)
    .await?;
    Ok(())
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
