//! First-boot seed of the native app tables — the record-model equivalent of the
//! legacy `persistence::seeder`. Only the app tables (configuration + auth) are
//! seeded here; Ledger records are never auto-created. Called once from the cell
//! boot (NOT from `open_memory`, so tests stay empty unless they opt in).
//! Idempotent throughout.

use sqlx::SqlitePool;

use crate::{StoreError, auth, config};

/// Ensure the configuration singleton, the `admin`/`lince` roles, the full
/// permission catalog, and a grant of EVERY permission to `admin`.
///
/// `permissions` is the canonical `(subject, action)` catalog, owned by the
/// caller (`application::auth::ALL_PERMISSIONS`) so `store` stays decoupled from
/// the permission list. Creating the initial admin *user* is a separate,
/// interactive step (first-run prompt) — this only prepares roles/permissions so
/// that user can be assigned the all-powerful admin role.
pub async fn seed(pool: &SqlitePool, permissions: &[(&str, &str)]) -> Result<(), StoreError> {
    config::ensure_default(pool).await?;

    let admin = auth::ensure_role(pool, auth::ADMIN_ROLE).await?;
    auth::ensure_role(pool, auth::LINCE_ROLE).await?;

    for (subject, action) in permissions {
        let permission_id = auth::ensure_permission(pool, subject, action).await?;
        auth::grant(pool, admin, permission_id).await?;
    }

    Ok(())
}
