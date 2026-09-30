use chrono::{DateTime, Utc};
use store::sqlx::Row;

use crate::{Engine, EngineError};

impl Engine {
    pub(crate) async fn application_handoff_exchange(
        &self,
        handoff: &store::transfer_delivery::RemoteApplicationHandoffRow,
        replayed: bool,
    ) -> Result<String, EngineError> {
        if replayed {
            return Ok(String::new());
        }
        if !handoff.reference_uid.is_empty()
            && store::transfer_delivery::remote_reference_by_uid(
                &self.store.pool,
                &handoff.reference_uid,
            )
            .await?
            .is_none_or(|reference| reference.state != "active")
        {
            return Err(inactive_application());
        }
        Ok(store::transfer_accounting::handoff_exchange(&self.store.pool, handoff).await?)
    }

    pub(crate) async fn prepare_counterparty_applications(
        &self,
        transfer_uid: &str,
        now: DateTime<Utc>,
    ) -> Result<(), EngineError> {
        let origin = self.require_transfer_origin_authority(transfer_uid).await?;
        let rows = store::sqlx::query(
            "WITH settled AS (
                SELECT s.uid AS slice_uid, s.occurrence_uid, s.promise_uid,
                       s.owner_person_uid, s.canonical_quantity, s.canonical_unit_uid,
                       s.cumulative_before, s.cumulative_after, s.remaining_after
                FROM transfer_occurrence_settlement_slice s WHERE s.transfer_uid = ?
                UNION ALL
                SELECT h.settlement_slice_uid, h.occurrence_uid, d.source_promise_uid,
                       h.participant_person_uid, d.canonical_quantity, d.canonical_unit_uid,
                       d.canonical_cumulative_before, d.canonical_cumulative_after,
                       d.canonical_remaining_after
                FROM transfer_application_handoff h
                JOIN transfer_application_handoff_detail d ON d.handoff_uid = h.uid
                JOIN promise p ON p.uid = d.source_promise_uid AND p.party_uid = h.participant_person_uid
                WHERE h.transfer_uid = ? AND h.state = 'accepted'
            )
            SELECT s.*, o.revision,
                   CASE WHEN s.owner_person_uid = o.giver_person_uid THEN o.receiver_person_uid ELSE o.giver_person_uid END AS participant,
                   person.organ_uid AS participant_organ,
                   CASE WHEN s.owner_person_uid = o.giver_person_uid THEN 1 ELSE -1 END AS direction
            FROM settled s
            JOIN transfer_occurrence o ON o.uid = s.occurrence_uid
            JOIN transfer_exchange_path path ON path.uid = o.exchange_path_uid
            JOIN record person ON person.uid = CASE WHEN s.owner_person_uid = o.giver_person_uid THEN o.receiver_person_uid ELSE o.giver_person_uid END
            WHERE path.opposite_promise_uid IS NULL AND person.kind = 'person' AND person.deleted_at IS NULL
            ORDER BY s.occurrence_uid, s.cumulative_before, s.slice_uid"
        ).bind(transfer_uid).bind(transfer_uid).fetch_all(&self.store.pool).await?;
        for row in rows {
            let participant: String = row.try_get("participant")?;
            let participant_organ: String = row.try_get("participant_organ")?;
            let occurrence: String = row.try_get("occurrence_uid")?;
            let source: String = row.try_get("promise_uid")?;
            let slice: String = row.try_get("slice_uid")?;
            let revision = row.try_get::<i64, _>("revision")? as u64;
            let quantity: f64 = row.try_get("canonical_quantity")?;
            let unit: Option<String> = row.try_get("canonical_unit_uid")?;
            let before: f64 = row.try_get("cumulative_before")?;
            let after: f64 = row.try_get("cumulative_after")?;
            let remaining: f64 = row.try_get("remaining_after")?;
            let direction = row.try_get::<i64, _>("direction")? as i8;
            let hash = nucleus::fact::sha256_hex(
                &serde_json::to_vec(&serde_json::json!({
                    "action": "counterparty-application", "transfer": transfer_uid,
                    "occurrence": occurrence, "slice": slice, "promise": source,
                    "participant": participant, "quantity": quantity, "unit": unit,
                    "before": before, "after": after, "remaining": remaining,
                    "direction": direction, "revision": revision,
                }))
                .map_err(EngineError::Json)?,
            );
            let handoff = store::transfer_delivery::create_application_handoff(
                &self.store.pool,
                store::transfer_delivery::NewApplicationHandoff {
                    origin_organ_uid: &origin,
                    participant_organ_uid: &participant_organ,
                    participant_person_uid: &participant,
                    transfer_uid,
                    occurrence_uid: &occurrence,
                    settlement_slice_uid: &slice,
                    origin_revision: revision,
                    canonical_slice_hash: &hash,
                    request_id: &format!("counterparty:{slice}:{participant}"),
                    fact_uid: None,
                },
                now,
            )
            .await?;
            store::transfer_delivery::store_application_handoff_detail(
                &self.store.pool,
                &handoff.uid,
                &source,
                quantity,
                unit.as_deref(),
                before,
                after,
                remaining,
                direction,
                None,
                now,
            )
            .await?;
        }
        Ok(())
    }
}

fn inactive_application() -> EngineError {
    EngineError::Conflict {
        code: "transfer_delivery_inactive",
        message: "received Transfer delivery access is no longer active".into(),
    }
}

pub(crate) fn application_error(error: store::StoreError) -> EngineError {
    match error {
        store::StoreError::Protocol(message)
            if message == "Transfer delivery access is no longer active" =>
        {
            inactive_application()
        }
        error => EngineError::Store(error),
    }
}
