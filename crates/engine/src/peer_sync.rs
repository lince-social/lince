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
        if vector.len() > 4096 || vector.iter().any(|entry|entry.actor_cell.len() > 200) {
            return Err(EngineError::Consequence("The sync version vector exceeds its bounds".into()));
        }
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
        } else if contact.as_ref().is_some_and(|contact|contact.sync_out) {
            store::sync_ops::ops_missing_from_vector(
                &self.store.pool,
                &local,
                vector,
                limit.clamp(1, 2000),
            )
            .await?
        } else {
            Vec::new()
        };
        let mut head = rows.last().map_or(0, |row| row.seq);
        let scope = contact.and_then(|contact| contact.scope_fields);
        let links = store::sync_ops::resolve_link_scope(&self.store.pool, scope.as_deref()).await?;
        let rows = store::sync_ops::narrow_ops_to_scope(rows, scope.as_deref(), &links);
        let hidden = store::visibility::hidden_from_organ(&self.store.pool, authenticated).await?;
        let mut visible = Vec::new();
        for row in rows {
            if local != authenticated && nucleus::social::private_sync_field(&row.tbl, &row.field) {
                continue;
            }
            if !store::visibility::op_hidden_from(&self.store.pool, &hidden, &row.tbl, &row.uid)
                .await?
            {
                visible.push(row);
            }
        }
        let mut ops = self.hydrate_ops(visible).await?;
        let general_count = ops.len();
        if local == authenticated {
            ops.extend(self.export_own_conversations(&local, vector).await?);
            ops.extend(
                self.export_transfer_transactions(&local, vector, limit.clamp(1, 128) as usize)
                    .await?,
            );
        }
        let mut batch = OpBatch { from_organ: local.clone(), ops };
        if crate::social::history::bound_sync_page(&mut batch, 15 * 1024 * 1024)?
            && batch.ops.len() < general_count
        {
            head = if let Some(last) = batch.ops.last() {
                store::sqlx::query_scalar::<_, Option<i64>>("SELECT MAX(seq) FROM sync_op WHERE organ_uid=? AND actor_cell=? AND hlc=?")
                    .bind(&local).bind(&last.actor_cell).bind(last.hlc).fetch_one(&self.store.pool).await?.unwrap_or(0)
            } else { 0 };
        }
        Ok(Page { batch, head })
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
        let contact=store::organs::contact(&self.store.pool,authenticated).await?;
        if !ours
            && contact.as_ref().is_none_or(|contact| contact.trust != "known")
        {
            return Err(EngineError::Conflict {
                code: "not_known",
                message: "sync requires a known, unblocked Organ".into(),
            });
        }
        if !ours && contact.as_ref().is_some_and(|contact|!contact.sync_in) {
            Err(EngineError::Forbidden("General replication from this contact is disabled".into()))
        } else {
            self.import_op_batch(batch).await
        }
    }
}
