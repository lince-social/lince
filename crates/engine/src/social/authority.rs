use super::*;
use store::sqlx::{Sqlite, Transaction};

struct Authorization {
    engine: usize,
    actor: Option<String>,
    permission: &'static str,
}

tokio::task_local! {
    static AUTHORIZATION: Authorization;
}

impl Engine {
    pub(super) async fn social_authorized<T>(
        &self,
        actor: Option<&str>,
        permission: &'static str,
        operation: impl std::future::Future<Output = Result<T, EngineError>>,
    ) -> Result<T, EngineError> {
        AUTHORIZATION
            .scope(
                Authorization {
                    engine: self as *const Self as usize,
                    actor: actor.map(str::to_owned),
                    permission,
                },
                operation,
            )
            .await
    }

    pub(super) async fn social_require_actor_on(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        actor: Option<&str>,
        permission: &str,
    ) -> Result<(), EngineError> {
        self.require_login_on(tx).await?;
        if let Some(actor) = actor {
            let principal = store::auth::principal_on(tx, actor).await?;
            if principal.is_none_or(|principal| !principal.permits(permission)) {
                return Err(EngineError::Forbidden(format!(
                    "Current {permission} permission is required; refresh before retrying"
                )));
            }
        }
        Ok(())
    }

    pub(super) async fn social_write_tx(&self) -> Result<Transaction<'_, Sqlite>, EngineError> {
        #[cfg(test)]
        tests::pause().await;
        let mut tx = store::write_tx(&self.store.pool).await?;
        self.require_login_on(&mut tx).await?;
        if let Some((actor, permission)) = AUTHORIZATION
            .try_with(|held| {
                (held.engine == self as *const Self as usize)
                    .then(|| (held.actor.clone(), held.permission))
            })
            .ok()
            .flatten()
        {
            self.social_require_actor_on(&mut tx, actor.as_deref(), permission)
                .await?;
            if permission != "view:stream" {
                self.social_require_local_write_on(&mut tx).await?;
            }
        }
        Ok(tx)
    }

    pub(super) async fn social_require_local_write_on(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
    ) -> Result<(), EngineError> {
        let organ: String = store::sqlx::query_scalar(
            "SELECT uid FROM record WHERE slug=? AND kind=? AND deleted_at IS NULL LIMIT 1",
        )
        .bind(store::organs::LOCAL_ORGAN_SLUG)
        .bind(nucleus::RecordKind::Organ.as_str())
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| invalid("No local Organ"))?;
        let held: Option<(String, String)> =
            store::sqlx::query_as("SELECT payload,signature FROM organ_roster WHERE organ_uid=?")
                .bind(&organ)
                .fetch_optional(&mut **tx)
                .await?;
        let Some((payload, signature)) = held else {
            return Ok(());
        };
        let signed = crate::roster::SignedRoster {
            roster: serde_json::from_str(&payload)?,
            signature,
        };
        let cell: String = store::sqlx::query_scalar("SELECT uid FROM record WHERE slug=? AND kind=? AND organ_uid=? AND deleted_at IS NULL LIMIT 1")
            .bind(store::cells::LOCAL_CELL_SLUG).bind(nucleus::RecordKind::Device.as_str()).bind(&organ).fetch_optional(&mut **tx).await?
            .ok_or_else(||invalid("No local Cell"))?;
        let key = match self.local_organ_public_key().await? {
            Some(key) => Some(key),
            None => {
                store::sqlx::query_scalar::<_, String>(
                    "SELECT public_key FROM identity_key WHERE actor_uid=? AND key_id=?",
                )
                .bind(&organ)
                .bind(crate::roster::cell_key_id(&cell))
                .fetch_optional(&mut **tx)
                .await?
            }
        };
        let revoked: bool = store::sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM identity_revocation WHERE organ_uid=? AND revoked_key=?)",
        )
        .bind(&organ)
        .bind(&signed.roster.root_key)
        .fetch_one(&mut **tx)
        .await?;
        if signed.roster.organ_uid != organ
            || revoked
            || !crate::roster::roster_signature_is_valid(&signed)
            || DateTime::parse_from_rfc3339(&signed.roster.not_after)
                .ok()
                .is_none_or(|expiry| expiry <= nucleus::execution::now())
            || !signed.roster.cells.iter().any(|entry| {
                entry.cell_uid == cell
                    && entry.may(crate::roster::CAP_WRITE)
                    && key.as_ref() == Some(&entry.operational_key)
            })
        {
            return Err(EngineError::Forbidden("This device is read-only, removed or has obsolete keys or expired authority. Refresh its valid write membership before editing or publishing social data".into()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
