//! Cell and Interface preferences stored in the singleton configuration row.

use nucleus::execution::now;
use sqlx::{Row, SqlitePool, query, query_as, query_scalar, sqlite::SqliteRow};

use crate::StoreError;

/// The persisted preferences exposed to callers.
#[derive(Debug, Clone, PartialEq)]
pub struct Configuration {
    /// Cell name.
    pub name: String,
    /// Preferred language tag.
    pub language: String,
    /// Stored timezone selection.
    pub timezone: i64,
    /// Interface style name.
    pub style: String,
    /// Whether command notifications have a positive duration.
    pub show_command_notifications: bool,
    /// Command notification duration in seconds.
    pub command_notification_seconds: f64,
    /// Whether deleting a Record requires confirmation.
    pub delete_confirmation: bool,
    /// Whether deleting a Sand requires confirmation.
    pub sand_delete_confirmation: bool,
    /// Error toast duration in seconds.
    pub error_toast_seconds: f64,
    /// Stored keybinding mode.
    pub keybinding_mode: i64,
    /// Default Transfer phase at which stock is reserved.
    pub transfer_reservation_default: String,
    /// Default Transfer application formula.
    pub transfer_application_formula: String,
    /// Default policy for remainders after applying a Transfer.
    pub transfer_remainder_policy: String,
    /// Whether closing the Interface suspends it.
    pub interface_close_suspends: bool,
    /// Interface snapshot and history preferences.
    pub interface_storage: InterfaceStorage,
}

/// Intervals and retention count for Interface snapshots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InterfaceStorage {
    /// Interval between snapshots in seconds.
    pub snapshot_seconds: u64,
    /// Interval between archived snapshots in seconds.
    pub history_seconds: u64,
    /// Number of archived snapshots to retain.
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
    /// Checks the supported intervals and retention count.
    /// # Errors
    /// Returns an error for an unsupported interval, history shorter than the snapshot interval,
    /// or a retention count outside the supported range.
    pub fn validate(self) -> Result<Self, StoreError> {
        if !(1..=86400).contains(&self.snapshot_seconds)
            || !(self.snapshot_seconds..=604800).contains(&self.history_seconds)
            || !(1..=100).contains(&self.history_count)
        {
            return Err(invalid_storage());
        }
        Ok(self)
    }

    fn read(row: &SqliteRow) -> Result<Self, StoreError> {
        Ok(Self {
            snapshot_seconds: storage_limit(row.try_get::<i64, _>("interface_snapshot_seconds")?)?,
            history_seconds: storage_limit(row.try_get::<i64, _>("interface_history_seconds")?)?,
            history_count: storage_limit(row.try_get::<i64, _>("interface_history_count")?)?,
        })
    }
}

fn storage_limit<T>(value: impl TryInto<T>) -> Result<T, StoreError> {
    value.try_into().map_err(|_| invalid_storage())
}

fn invalid_storage() -> StoreError {
    StoreError::Protocol("invalid interface snapshot or history settings".into())
}

/// Reads and validates the Interface storage settings, creating defaults if needed.
/// # Errors
/// Returns an error if the settings cannot be initialized, read, or validated.
pub async fn interface_storage(pool: &SqlitePool) -> Result<InterfaceStorage, StoreError> {
    ensure_default(pool).await?;
    let row = query("SELECT interface_snapshot_seconds, interface_history_seconds, interface_history_count FROM configuration WHERE id = 1")
        .fetch_one(pool).await?;
    InterfaceStorage::read(&row)?.validate()
}

/// Stores validated Interface storage settings together.
/// # Errors
/// Returns an error if the settings are outside supported ranges or cannot be saved.
pub async fn set_interface_storage(
    pool: &SqlitePool,
    settings: InterfaceStorage,
) -> Result<(), StoreError> {
    settings.validate()?;
    ensure_default(pool).await?;
    query("UPDATE configuration SET interface_snapshot_seconds = ?, interface_history_seconds = ?, interface_history_count = ?, updated_at = ? WHERE id = 1")
        .bind(storage_limit::<i64>(settings.snapshot_seconds)?)
        .bind(storage_limit::<i64>(settings.history_seconds)?)
        .bind(storage_limit::<i64>(settings.history_count)?)
        .bind(now().to_rfc3339())
        .execute(pool).await?;
    Ok(())
}

/// Creates the default configuration row if it is absent.
/// # Errors
/// Returns an error if the configuration cannot be initialized.
pub async fn ensure_default(pool: &SqlitePool) -> Result<(), StoreError> {
    query("INSERT OR IGNORE INTO configuration (id, created_at, updated_at) VALUES (1, ?, ?)")
        .bind(now().to_rfc3339())
        .bind(now().to_rfc3339())
        .execute(pool)
        .await?;
    Ok(())
}

/// Reads the daily attention budget, creating defaults if needed.
/// # Errors
/// Returns an error if the budget cannot be initialized or read.
pub async fn attention_budget(pool: &SqlitePool) -> Result<i64, StoreError> {
    ensure_default(pool).await?;
    query_scalar("SELECT attention_budget_per_day FROM configuration WHERE id = 1")
        .fetch_one(pool)
        .await
}

/// Updates the daily attention budget.
/// # Errors
/// Returns an error if the budget cannot be saved.
pub async fn set_attention_budget(pool: &SqlitePool, per_day: i64) -> Result<(), StoreError> {
    ensure_default(pool).await?;
    query("UPDATE configuration SET attention_budget_per_day = ? WHERE id = 1")
        .bind(per_day)
        .execute(pool)
        .await?;
    Ok(())
}

/// Updates the preferred language.
/// # Errors
/// Returns an error if the language cannot be saved.
pub async fn set_language(pool: &SqlitePool, language: &str) -> Result<(), StoreError> {
    ensure_default(pool).await?;
    query("UPDATE configuration SET language = ? WHERE id = 1")
        .bind(language)
        .execute(pool)
        .await?;
    Ok(())
}

/// Reads the stored preferences when the configuration row exists.
/// # Errors
/// Returns an error if a stored preference cannot be read or decoded.
pub async fn get(pool: &SqlitePool) -> Result<Option<Configuration>, StoreError> {
    query("SELECT * FROM configuration WHERE id = 1")
        .try_map(|row: SqliteRow| {
            let notification_seconds = row.try_get("command_notification_seconds")?;
            Ok(Configuration {
                name: row.try_get("name")?,
                language: row.try_get("language")?,
                timezone: row.try_get("timezone")?,
                style: row.try_get("style")?,
                show_command_notifications: notification_seconds > 0.0,
                command_notification_seconds: notification_seconds,
                delete_confirmation: row.try_get("delete_confirmation")?,
                sand_delete_confirmation: row.try_get("sand_delete_confirmation")?,
                error_toast_seconds: row.try_get("error_toast_seconds")?,
                keybinding_mode: row.try_get("keybinding_mode")?,
                transfer_reservation_default: row.try_get("transfer_reservation_default")?,
                transfer_application_formula: row.try_get("transfer_application_formula")?,
                transfer_remainder_policy: row.try_get("transfer_remainder_policy")?,
                interface_close_suspends: row.try_get("interface_close_suspends")?,
                interface_storage: InterfaceStorage::read(&row)?,
            })
        })
        .fetch_optional(pool)
        .await
}

/// Reads the language preference if the configuration row exists.
/// # Errors
/// Returns an error if the language cannot be read.
pub async fn language(pool: &SqlitePool) -> Result<Option<String>, StoreError> {
    query_scalar("SELECT language FROM configuration WHERE id = 1")
        .fetch_optional(pool)
        .await
}

/// Reads whether closing the Interface suspends it.
/// # Errors
/// Returns an error if the configuration cannot be initialized or read.
pub async fn interface_close_suspends(pool: &SqlitePool) -> Result<bool, StoreError> {
    ensure_default(pool).await?;
    query_scalar("SELECT interface_close_suspends FROM configuration WHERE id = 1")
        .fetch_one(pool)
        .await
}

/// Sets whether closing the Interface suspends it.
/// # Errors
/// Returns an error if the preference cannot be saved.
pub async fn set_interface_close_suspends(
    pool: &SqlitePool,
    enabled: bool,
) -> Result<(), StoreError> {
    ensure_default(pool).await?;
    query("UPDATE configuration SET interface_close_suspends = ?, updated_at = ? WHERE id = 1")
        .bind(i64::from(enabled))
        .bind(now().to_rfc3339())
        .execute(pool)
        .await?;
    Ok(())
}

/// Reads the default reservation phase, creating defaults if needed.
/// # Errors
/// Returns an error if the default cannot be initialized or read.
pub async fn transfer_reservation_default(pool: &SqlitePool) -> Result<String, StoreError> {
    ensure_default(pool).await?;
    query_scalar("SELECT transfer_reservation_default FROM configuration WHERE id = 1")
        .fetch_one(pool)
        .await
}

/// Sets the default Transfer reservation phase.
/// # Errors
/// Returns an error if the phase is unknown or cannot be saved.
pub async fn set_transfer_reservation_default(
    pool: &SqlitePool,
    reserve_from: &str,
) -> Result<(), StoreError> {
    if !matches!(reserve_from, "none" | "proposed" | "agreed" | "active") {
        return Err(StoreError::Protocol(format!(
            "unknown reservation point `{reserve_from}`"
        )));
    }
    ensure_default(pool).await?;
    query(
        "UPDATE configuration
         SET transfer_reservation_default = ?, updated_at = ? WHERE id = 1",
    )
    .bind(reserve_from)
    .bind(now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

/// Reads the default Transfer application formula, falling back to `incoming()`.
/// # Errors
/// Returns an error if the formula cannot be read.
pub async fn transfer_application_formula(pool: &SqlitePool) -> Result<String, StoreError> {
    Ok(
        query_scalar("SELECT transfer_application_formula FROM configuration WHERE id = 1")
            .fetch_optional(pool)
            .await?
            .unwrap_or_else(|| "incoming()".into()),
    )
}

/// Stores a trimmed default Transfer application formula.
/// # Errors
/// Returns an error if the trimmed formula is empty, too long, or cannot be saved.
pub async fn set_transfer_application_formula(
    pool: &SqlitePool,
    formula: &str,
) -> Result<(), StoreError> {
    let formula = formula.trim();
    if formula.is_empty() || formula.chars().count() > 2_000 {
        return Err(StoreError::Protocol(
            "application formula must contain 1 to 2000 characters".into(),
        ));
    }
    ensure_default(pool).await?;
    query(
        "UPDATE configuration
         SET transfer_application_formula = ?, updated_at = ? WHERE id = 1",
    )
    .bind(formula)
    .bind(now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

/// Reads the default Transfer remainder policy, falling back to `visible`.
/// # Errors
/// Returns an error if the policy cannot be read.
pub async fn transfer_remainder_policy(pool: &SqlitePool) -> Result<String, StoreError> {
    Ok(
        query_scalar("SELECT transfer_remainder_policy FROM configuration WHERE id = 1")
            .fetch_optional(pool)
            .await?
            .unwrap_or_else(|| "visible".into()),
    )
}

/// Sets the default Transfer remainder policy.
/// # Errors
/// Returns an error if the policy is unknown or cannot be saved.
pub async fn set_transfer_remainder_policy(
    pool: &SqlitePool,
    policy: &str,
) -> Result<(), StoreError> {
    if !matches!(policy, "visible" | "local_draft") {
        return Err(StoreError::Protocol(format!(
            "unknown Transfer remainder policy `{policy}`"
        )));
    }
    ensure_default(pool).await?;
    query(
        "UPDATE configuration
         SET transfer_remainder_policy = ?, updated_at = ? WHERE id = 1",
    )
    .bind(policy)
    .bind(now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

/// Returns the confirmation preferences for Sands and Records in that order.
/// # Errors
/// Returns an error if the preferences cannot be initialized or read.
pub async fn deletion_confirmations(pool: &SqlitePool) -> Result<(bool, bool), StoreError> {
    ensure_default(pool).await?;
    query_as("SELECT sand_delete_confirmation, delete_confirmation FROM configuration WHERE id = 1")
        .fetch_one(pool)
        .await
}

/// Updates Record confirmation when `records` is true, or Sand confirmation otherwise.
/// # Errors
/// Returns an error if the preference cannot be saved or the resulting pair cannot be read.
pub async fn set_deletion_confirmation(
    pool: &SqlitePool,
    records: bool,
    enabled: bool,
) -> Result<(bool, bool), StoreError> {
    ensure_default(pool).await?;
    let statement = if records {
        "UPDATE configuration SET delete_confirmation = ?, updated_at = ? WHERE id = 1"
    } else {
        "UPDATE configuration SET sand_delete_confirmation = ?, updated_at = ? WHERE id = 1"
    };
    query(statement)
        .bind(enabled)
        .bind(now().to_rfc3339())
        .execute(pool)
        .await?;
    deletion_confirmations(pool).await
}
