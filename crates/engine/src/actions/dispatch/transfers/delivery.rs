// non-lean-yet

use super::super::{
    Action, ActionOutcome, ControlFlow, DateTime, DispatchOutcome, Engine, EngineError, Utc,
    VerifiedActionAuthorship, invalid_dispatch,
};

impl Engine {
    pub(in super::super) async fn dispatch_transfers_delivery(
        &self,
        action: Action,
        actor: Option<String>,
        now: DateTime<Utc>,
        verified_authorship: Option<VerifiedActionAuthorship>,
    ) -> DispatchOutcome {
        let mut outcome = ActionOutcome::default();
        match action {
            Action::ConfigureTransferDelivery {
                transfer,
                recipient_person,
                recipient_organ,
                person,
                request_id,
                mode,
            } => {
                let transfer = self.resolve(&transfer).await?;
                self.require_transfer_editor(&transfer, actor.as_deref())
                    .await?;
                let recipient_person = self.resolve(&recipient_person).await?;
                let recipient_organ = self.resolve(&recipient_organ).await?;
                let derived = self.transfer_creator_person(&transfer).await?;
                let acting = self
                    .transfer_action_person(actor.as_deref(), person.as_deref(), Some(&derived))
                    .await?;
                let (fact_uid, facts) = self
                    .append_transfer_delivery_evidence(
                        &transfer,
                        &acting,
                        &request_id,
                        "configure",
                        serde_json::json!({
                            "action": "configure-transfer-delivery",
                            "request_id": request_id,
                            "recipient_person": recipient_person,
                            "recipient_organ": recipient_organ,
                            "mode": mode.as_str(),
                        }),
                        now,
                        verified_authorship.as_ref(),
                    )
                    .await?;
                let policy = self
                    .create_transfer_delivery_policy(
                        &transfer,
                        &recipient_person,
                        &recipient_organ,
                        &acting,
                        &fact_uid,
                        &request_id,
                        mode,
                        now,
                    )
                    .await?;
                outcome.created = Some(policy.uid);
                outcome.facts = facts;
            }
            Action::SetTransferDeliveryMode {
                transfer,
                delivery,
                expected_revision,
                person,
                request_id,
                mode,
            } => {
                let transfer = self.resolve(&transfer).await?;
                self.require_transfer_editor(&transfer, actor.as_deref())
                    .await?;
                let derived = self.transfer_creator_person(&transfer).await?;
                let acting = self
                    .transfer_action_person(actor.as_deref(), person.as_deref(), Some(&derived))
                    .await?;
                let (fact_uid, facts) = self
                    .append_transfer_delivery_evidence(
                        &transfer,
                        &acting,
                        &request_id,
                        "set-mode",
                        serde_json::json!({
                            "action": "set-transfer-delivery-mode",
                            "request_id": request_id,
                            "delivery": delivery,
                            "expected_revision": expected_revision,
                            "mode": mode.as_str(),
                        }),
                        now,
                        verified_authorship.as_ref(),
                    )
                    .await?;
                self.change_transfer_delivery_mode(
                    &transfer,
                    &delivery,
                    expected_revision,
                    &acting,
                    &fact_uid,
                    &request_id,
                    mode,
                    now,
                )
                .await?;
                outcome.created = Some(delivery);
                outcome.facts = facts;
            }
            Action::RevokeTransferDelivery {
                transfer,
                delivery,
                expected_revision,
                person,
                request_id,
            } => {
                let transfer = self.resolve(&transfer).await?;
                self.require_transfer_editor(&transfer, actor.as_deref())
                    .await?;
                let derived = self.transfer_creator_person(&transfer).await?;
                let acting = self
                    .transfer_action_person(actor.as_deref(), person.as_deref(), Some(&derived))
                    .await?;
                let (fact_uid, facts) = self
                    .append_transfer_delivery_evidence(
                        &transfer,
                        &acting,
                        &request_id,
                        "revoke",
                        serde_json::json!({
                            "action": "revoke-transfer-delivery",
                            "request_id": request_id,
                            "delivery": delivery,
                            "expected_revision": expected_revision,
                            "retains_received_evidence": true,
                        }),
                        now,
                        verified_authorship.as_ref(),
                    )
                    .await?;
                self.revoke_transfer_delivery_policy(
                    &transfer,
                    &delivery,
                    expected_revision,
                    &acting,
                    &fact_uid,
                    &request_id,
                    now,
                )
                .await?;
                outcome.created = Some(delivery);
                outcome.facts = facts;
            }
            Action::EnqueueTransferDelivery {
                transfer,
                delivery,
                person,
                request_id,
            } => {
                let operation = "enqueue";
                let transfer = self.resolve(&transfer).await?;
                self.require_transfer_editor(&transfer, actor.as_deref())
                    .await?;
                let derived = self.transfer_creator_person(&transfer).await?;
                let acting = self
                    .transfer_action_person(actor.as_deref(), person.as_deref(), Some(&derived))
                    .await?;
                let (_, facts) = self
                    .append_transfer_delivery_evidence(
                        &transfer,
                        &acting,
                        &request_id,
                        operation,
                        serde_json::json!({
                            "action": format!("{operation}-transfer-delivery"),
                            "request_id": request_id,
                            "delivery": delivery,
                        }),
                        now,
                        verified_authorship.as_ref(),
                    )
                    .await?;
                outcome.created = Some(
                    self.enqueue_transfer_delivery(&transfer, &delivery, &request_id, now)
                        .await?,
                );
                outcome.facts = facts;
            }
            Action::RetryTransferDelivery {
                transfer,
                delivery,
                person,
                request_id,
            } => {
                let transfer = self.resolve(&transfer).await?;
                self.require_transfer_editor(&transfer, actor.as_deref())
                    .await?;
                let derived = self.transfer_creator_person(&transfer).await?;
                let acting = self
                    .transfer_action_person(actor.as_deref(), person.as_deref(), Some(&derived))
                    .await?;
                let (_, facts) = self
                    .append_transfer_delivery_evidence(
                        &transfer,
                        &acting,
                        &request_id,
                        "retry",
                        serde_json::json!({
                            "action": "retry-transfer-delivery",
                            "request_id": request_id,
                            "delivery": delivery,
                        }),
                        now,
                        verified_authorship.as_ref(),
                    )
                    .await?;
                outcome.created = Some(
                    self.retry_transfer_delivery(&transfer, &delivery, &request_id, now)
                        .await?,
                );
                outcome.facts = facts;
            }
            Action::RefreshTransferDelivery {
                transfer,
                delivery,
                person,
                request_id,
            } => {
                let local = store::organs::local(&self.store.pool)
                    .await?
                    .ok_or_else(|| EngineError::Consequence("no local Organ".into()))?;
                let reference = store::sqlx::query_as::<_, (String, String, String, String)>(
                    "SELECT transfer_uid, recipient_organ_uid, recipient_person_uid, state FROM transfer_remote_reference WHERE uid = ?",
                )
                .bind(&delivery)
                .fetch_optional(&self.store.pool)
                .await?
                .ok_or_else(|| EngineError::UnknownRecord(delivery.clone()))?;
                let acting = self
                    .transfer_action_person(actor.as_deref(), person.as_deref(), Some(&reference.2))
                    .await?;
                if reference.0 != transfer
                    || reference.1 != local.uid
                    || reference.2 != acting
                    || reference.3 != "active"
                {
                    return Err(EngineError::Conflict {
                        code: "transfer_delivery_refresh_forbidden",
                        message: "only an active local hosted/replica reference may be refreshed"
                            .into(),
                    });
                }
                outcome.created = Some(
                    store::transfer_delivery::enqueue_pull(
                        &self.store.pool,
                        &delivery,
                        &request_id,
                        now,
                    )
                    .await?
                    .uid,
                );
            }
            _ => return Err(invalid_dispatch()),
        }
        Ok(ControlFlow::Continue(outcome))
    }
}
