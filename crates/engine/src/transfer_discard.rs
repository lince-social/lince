use crate::{
    Engine, EngineError,
    actions::{ActionOutcome, VerifiedActionAuthorship},
};
use nucleus::{Cause, NewFact};

impl Engine {
    pub(crate) async fn discard_transfer_draft(
        &self,
        transfer: String,
        person: Option<String>,
        expected_revision: u64,
        request_id: String,
        actor: Option<&str>,
        authorship: Option<&VerifiedActionAuthorship>,
    ) -> Result<ActionOutcome, EngineError> {
        self.require_permission(actor, "transfer:update").await?;
        if request_id.trim().is_empty() || request_id.len() > 256 {
            return Err(EngineError::Consequence(
                "Choose a draft-discard request identity of 1–256 bytes".into(),
            ));
        }
        let replay: Option<(String, String, i64, String)> = store::sqlx::query_as("SELECT transfer_uid, person_uid, expected_revision, fact_uid FROM transfer_draft_discard WHERE request_id = ?").bind(&request_id).fetch_optional(&self.store.pool).await?;
        if let Some((uid, original_person, revision, fact)) = replay {
            let acting = self
                .transfer_action_person(actor, person.as_deref(), Some(&original_person))
                .await?;
            if uid != transfer
                || original_person != acting
                || u64::try_from(revision).ok() != Some(expected_revision)
            {
                return Err(EngineError::Consequence(
                    "Discard identity belongs to another operation".into(),
                ));
            }
            return Ok(ActionOutcome {
                data: Some(serde_json::json!({"discarded":uid,"fact":fact,"duplicate":true})),
                ..Default::default()
            });
        }
        let transfer = self.resolve(&transfer).await?;
        self.require_transfer_origin_authority(&transfer).await?;
        self.refuse_unreadable(actor, std::slice::from_ref(&transfer))
            .await?;
        let creator = store::transfers::creator_party_actor(&self.store.pool, &transfer)
            .await?
            .ok_or_else(|| EngineError::Consequence("This Transfer has no creator".into()))?;
        let acting = self
            .transfer_action_person(actor, person.as_deref(), Some(&creator))
            .await?;
        if creator != acting {
            return Err(EngineError::Forbidden(
                "Only the creator may discard an unused draft".into(),
            ));
        }
        let signer = self.transfer_person_signer(&acting, authorship).await?;
        let revision = i64::try_from(expected_revision)
            .map_err(|_| EngineError::Consequence("Invalid Transfer revision".into()))?;
        let mut tx = store::write_tx(&self.store.pool).await?;
        let eligible: i64 = store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM transfer t JOIN record r ON r.uid = t.record_uid WHERE t.record_uid = ? AND t.revision = ? AND r.deleted_at IS NULL AND t.visibility = 'hidden' AND t.parent_uid IS NULL AND t.source_uid IS NULL AND NOT EXISTS(SELECT 1 FROM transfer_party p WHERE p.transfer_uid = t.record_uid AND (p.kind != 'creator' OR p.actor_uid != ?)) AND NOT EXISTS(SELECT 1 FROM transfer_invitation i WHERE i.transfer_uid = t.record_uid) AND NOT EXISTS(SELECT 1 FROM transfer_agreement_event a WHERE a.transfer_uid = t.record_uid) AND NOT EXISTS(SELECT 1 FROM transfer_occurrence o WHERE o.transfer_uid = t.record_uid) AND NOT EXISTS(SELECT 1 FROM transfer_delivery_policy d WHERE d.transfer_uid = t.record_uid) AND NOT EXISTS(SELECT 1 FROM promise p WHERE p.transfer_uid = t.record_uid AND p.state NOT IN ('proposed','open')) AND NOT EXISTS(SELECT 1 FROM transfer child WHERE child.parent_uid = t.record_uid OR child.source_uid = t.record_uid) AND NOT EXISTS(SELECT 1 FROM transfer_dependency d WHERE d.transfer_uid != t.record_uid AND (d.upstream_uid = t.record_uid OR EXISTS(SELECT 1 FROM promise p WHERE p.transfer_uid = t.record_uid AND p.uid = d.upstream_uid))))")
            .bind(&transfer).bind(revision).bind(&acting).fetch_one(&mut *tx).await?;
        if eligible == 0 {
            return Err(EngineError::Conflict { code: "transfer_draft_not_discardable", message: "The revision changed, or this draft was published, addressed, agreed, delivered, activated or referenced. Use its cancellation/correction workflow.".into() });
        }
        let now = nucleus::execution::now();
        let fact = crate::append::append_one_in_transaction(&mut tx, NewFact {
            actor_uid: Some(acting.clone()), payload: Some(serde_json::json!({"action":"discard-transfer-draft","request_id":request_id,"expected_revision":revision}).to_string()), ..NewFact::quantity(transfer.clone(), store::exact::zero(), Cause::user_edit())
        }, now, signer.as_ref()).await?.ok_or_else(|| EngineError::Consequence("Missing draft-discard evidence".into()))?;
        if let Some(authorship) = authorship {
            store::action_intents::link_pending_fact(&mut tx, &authorship.intent_uid, &fact)
                .await?;
        }
        store::records::mark_deleted_on(&mut tx, &transfer).await?;
        store::sqlx::query("INSERT INTO transfer_draft_discard (request_id, transfer_uid, person_uid, expected_revision, fact_uid) VALUES (?, ?, ?, ?, ?)").bind(&request_id).bind(&transfer).bind(&acting).bind(revision).bind(&fact.uid).execute(&mut *tx).await?;
        tx.commit().await?;
        let _ = self.bus.send(fact.clone());
        Ok(ActionOutcome {
            facts: vec![fact],
            data: Some(serde_json::json!({"discarded":transfer})),
            ..Default::default()
        })
    }
}
