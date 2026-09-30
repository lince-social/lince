use crate::sync::OpBatch;
use crate::{Engine, EngineError};

pub struct Page {
    pub batch: OpBatch,
    pub head: i64,
}

impl Engine {
    pub async fn export_sync_page(
        &self,
        authenticated: &str,
        vector: &[store::sync_ops::VectorEntry],
        limit: i64,
    ) -> Result<Page, EngineError> {
        let local = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| EngineError::Consequence("no local Organ".into()))?
            .uid;
        let contact = store::organs::contact(&self.store.pool, authenticated).await?;
        if local != authenticated
            && contact
                .as_ref()
                .is_none_or(|contact| contact.trust != "known")
        {
            return Err(EngineError::Conflict {
                code: "not_known",
                message: "sync requires a known, unblocked Organ".into(),
            });
        }
        if vector.is_empty() {
            if let Some(message) = store::contact_rate::backing_off(
                &self.store.pool,
                authenticated,
                store::contact_rate::RateKind::FullLogServe,
            )
            .await?
            {
                return Err(EngineError::Conflict {
                    code: "rate_limited",
                    message,
                });
            }
            store::contact_rate::spend(
                &self.store.pool,
                authenticated,
                store::contact_rate::RateKind::FullLogServe,
            )
            .await?;
        }
        let covered =
            store::sync_ops::seq_covered_by_vector(&self.store.pool, &local, vector).await?;
        store::organs::advance_peer_acked_seq(&self.store.pool, authenticated, covered).await?;
        let rows = if local == authenticated {
            store::sync_ops::sibling_ops_missing_from_vector(
                &self.store.pool,
                &local,
                vector,
                limit.clamp(1, 2000),
            )
            .await?
        } else {
            store::sync_ops::ops_missing_from_vector(
                &self.store.pool,
                &local,
                vector,
                limit.clamp(1, 2000),
            )
            .await?
        };
        let head = rows.last().map_or(0, |row| row.seq);
        let scope = contact.and_then(|contact| contact.scope_fields);
        let links = store::sync_ops::resolve_link_scope(&self.store.pool, scope.as_deref()).await?;
        let rows = store::sync_ops::narrow_ops_to_scope(rows, scope.as_deref(), &links);
        let hidden = store::visibility::hidden_from_organ(&self.store.pool, authenticated).await?;
        let mut visible = Vec::new();
        for row in rows {
            if !store::visibility::op_hidden_from(&self.store.pool, &hidden, &row.tbl, &row.uid)
                .await?
            {
                visible.push(row);
            }
        }
        let mut ops = self.hydrate_ops(visible).await?;
        if local == authenticated {
            ops.extend(
                self.export_transfer_transactions(&local, vector, limit.clamp(1, 128) as usize)
                    .await?,
            );
        }
        Ok(Page {
            batch: OpBatch {
                from_organ: local,
                ops,
            },
            head,
        })
    }

    pub async fn receive_sync_batch(
        &self,
        authenticated: &str,
        batch: &OpBatch,
    ) -> Result<usize, EngineError> {
        if authenticated != batch.from_organ {
            return Err(EngineError::Conflict {
                code: "batch_peer_mismatch",
                message: "the batch does not belong to the authenticated Organ".into(),
            });
        }
        let ours = store::organs::local(&self.store.pool)
            .await?
            .is_some_and(|organ| organ.uid == authenticated);
        if !ours
            && store::organs::contact(&self.store.pool, authenticated)
                .await?
                .is_none_or(|contact| contact.trust != "known")
        {
            return Err(EngineError::Conflict {
                code: "not_known",
                message: "sync requires a known, unblocked Organ".into(),
            });
        }
        self.import_op_batch(batch).await
    }
}
