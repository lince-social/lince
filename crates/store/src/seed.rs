use sqlx::SqlitePool;

use crate::{StoreError, auth, config};

pub async fn seed(pool: &SqlitePool, permissions: &[(&str, &str)]) -> Result<(), StoreError> {
    config::ensure_default(pool).await?;

    let mut tx = crate::write_tx(pool).await?;
    let seeded: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM authority_seed_state)").fetch_one(&mut *tx).await?;
    let (admin, admin_created) = if seeded { (0, false) } else { auth::ensure_role_on(&mut tx, auth::ADMIN_ROLE).await? };
    if admin_created {
        auth::ensure_role_on(&mut tx, auth::LINCE_ROLE).await?;
    }

    for (subject, action) in permissions {
        let permission_id = auth::ensure_permission_on(&mut tx, subject, action).await?;
        if admin_created {
            auth::grant_on(&mut tx, admin, permission_id).await?;
        }
    }

    sqlx::query("INSERT OR IGNORE INTO authority_seed_state (id) VALUES (1)").execute(&mut *tx).await?;
    tx.commit().await
}
