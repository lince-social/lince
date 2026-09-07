use sqlx::SqlitePool;

use crate::{StoreError, auth, config};

pub async fn seed(pool: &SqlitePool, permissions: &[(&str, &str)]) -> Result<(), StoreError> {
    config::ensure_default(pool).await?;

    let mut tx = crate::write_tx(pool).await?;
    let (admin, admin_created) = auth::ensure_role_on(&mut tx, auth::ADMIN_ROLE).await?;
    auth::ensure_role_on(&mut tx, auth::LINCE_ROLE).await?;

    for (subject, action) in permissions {
        let permission_id = auth::ensure_permission_on(&mut tx, subject, action).await?;
        if admin_created {
            auth::grant_on(&mut tx, admin, permission_id).await?;
        }
    }

    tx.commit().await
}
