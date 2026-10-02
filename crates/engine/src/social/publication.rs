use super::*;

impl Engine {
    pub(super) async fn social_publication_is_current(
        &self,
        job: &store::social::PublicationJob,
        request: &PublicRequest,
    ) -> Result<bool, EngineError> {
        let mut tx = self.store.pool.begin().await?;
        self.social_require_local_write_on(&mut tx).await?;
        let current = Self::social_publication_is_current_on(&mut tx, job, request).await?;
        tx.commit().await?;
        Ok(current)
    }

    async fn social_publication_is_current_on(
        tx: &mut store::sqlx::Transaction<'_, store::sqlx::Sqlite>,
        job: &store::social::PublicationJob,
        request: &PublicRequest,
    ) -> Result<bool, EngineError> {
        let pending: bool = store::sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM social_publication_job WHERE hash=? AND destination=? AND state='pending' AND body=? AND expires_at>?)",
        )
        .bind(&job.hash)
        .bind(&job.destination)
        .bind(&job.body)
        .bind(nucleus::execution::now().timestamp())
        .fetch_one(&mut **tx)
        .await?;
        if !pending {
            return Ok(false);
        }
        let (profile, ending) = match request {
            PublicRequest::PublishProfile { document } => (Some(&document.authority), false),
            PublicRequest::PublishAuthority { document } => (Some(&document.authority), false),
            PublicRequest::PublishProfileImage { document } => {
                (Some(&document.profile.authority), false)
            }
            PublicRequest::PublishSnippet { document } => (
                document.profile.as_ref(),
                document.state == PostState::Withdrawn,
            ),
            _ => (None, false),
        };
        if let Some(authority) = profile {
            let revoked: bool = store::sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM identity_revocation WHERE organ_uid=? AND revoked_key=?)",
            )
            .bind(&authority.organ)
            .bind(&authority.root_key)
            .fetch_one(&mut **tx)
            .await?;
            let floor: Option<(i64, String, String)> = store::sqlx::query_as(
                "SELECT generation,root_key,editor_key FROM social_profile_authority WHERE organ=?",
            )
            .bind(&authority.organ)
            .fetch_optional(&mut **tx)
            .await?;
            let generation = authority.generation.parse::<i64>().unwrap_or(0);
            if revoked
                || generation <= 0
                || !ending
                    && floor.is_some_and(|(floor, root, editor)| {
                        generation < floor
                            || generation == floor
                                && (root != authority.root_key || editor != authority.editor_key)
                    })
            {
                return Ok(false);
            }
        }
        let anonymous = match request {
            PublicRequest::PublishSnippet { document } => document.anonymous.as_ref(),
            PublicRequest::PublishPostingAuthority { document } => Some(&document.authority),
            _ => None,
        };
        if let Some(authority) = anonymous {
            let floor: Option<(i64, String)> = store::sqlx::query_as(
                "SELECT generation,editor FROM social_posting_authority WHERE owner=?",
            )
            .bind(&authority.owner_key)
            .fetch_optional(&mut **tx)
            .await?;
            let generation = authority.generation.parse::<i64>().unwrap_or(0);
            if generation <= 0
                || floor.is_some_and(|(floor, editor)| {
                    generation < floor || generation == floor && editor != authority.editor_key
                })
            {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub(super) async fn social_finish_publication(
        &self,
        job: &store::social::PublicationJob,
        request: &PublicRequest,
        expected_hash: &str,
        success: bool,
        now: i64,
    ) -> Result<bool, EngineError> {
        let mut tx = self.social_write_tx().await?;
        if success {
            store::social::publication_receipt_on(&mut tx, job, expected_hash).await?;
        }
        let current = Self::social_publication_is_current_on(&mut tx, job, request).await?;
        let writable = match self.social_require_local_write_on(&mut tx).await {
            Ok(()) => true,
            Err(EngineError::Forbidden(_)) => false,
            Err(error) => return Err(error),
        };
        if current {
            store::social::publication_result_on(
                &mut tx,
                job,
                if success {
                    None
                } else {
                    Some("Publication host unavailable or refused this revision")
                },
                now,
            )
            .await?;
        } else {
            store::social::fail_publication_on(
                &mut tx,
                job,
                "This publication was superseded while waiting for its host",
            )
            .await?;
        }
        tx.commit().await?;
        Ok(success && current && writable)
    }
}
