use sqlx::SqlitePool;

use crate::{StoreError, auth, config};

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
