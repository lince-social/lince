use crate::{
    Engine, EngineError,
    actions::{Action, ActionOutcome, VerifiedActionAuthorship},
};
use chrono::{DateTime, Utc};
use nucleus::transfer::outcome::{Content, Request};

impl Engine {
    pub(crate) async fn record_transfer_outcome(
        &self,
        action: Action,
        actor: Option<&str>,
        now: DateTime<Utc>,
        authorship: Option<&VerifiedActionAuthorship>,
    ) -> Result<ActionOutcome, EngineError> {
        let Action::RecordTransferOutcome {
            occurrence,
            expected_revision,
            request_id,
            person,
            request,
        } = action
        else {
            unreachable!()
        };
        let person = self
            .transfer_action_person(actor, person.as_deref(), None)
            .await?;
        self.require_permission(actor, "transfer:update").await?;
        let saved_occurrence = store::transfers::occurrence(&self.store.pool, &occurrence)
            .await?
            .ok_or_else(|| EngineError::Consequence("Choose a resource occurrence".into()))?;
        self.require_transfer_origin_authority(&saved_occurrence.transfer_uid)
            .await?;
        self.refuse_unreadable(actor, std::slice::from_ref(&saved_occurrence.transfer_uid))
            .await?;
        let signer = self.transfer_person_signer(&person, authorship).await?;
        let replay = store::transfer_evidence::for_request(&self.store.pool, &request_id).await?;
        if let Some(uid) = &replay {
            self.refuse_unreadable(actor, std::slice::from_ref(uid))
                .await?;
        }
        if replay.is_none() {
            self.require_verified_transfer_revision(
                &saved_occurrence.transfer_uid,
                saved_occurrence.revision,
            )
            .await?;
            let activation =
                store::facts::get(&self.store.pool, &saved_occurrence.activation_fact_uid)
                    .await?
                    .ok_or_else(|| {
                        EngineError::Consequence("Outcome activation evidence is missing".into())
                    })?;
            if !nucleus::fact::verify_chain_step(&activation)
                || !crate::trust::verify_fact(&self.store, &activation).await?
            {
                return Err(EngineError::Consequence(
                    "Outcome activation evidence has no verified author".into(),
                ));
            }
        }
        if replay.is_none()
            && let Request::BindNeed { selection, .. } = &request
        {
            let terms = store::transfer_evidence::freeze_need_on(
                &mut *self.store.pool.acquire().await?,
                &person,
                selection,
            )
            .await?;
            if let Some(object) = &terms.measure.object {
                self.refuse_unreadable(actor, std::slice::from_ref(object))
                    .await?;
            }
        }
        if replay.is_none()
            && let Request::State {
                need_binding: Some(binding),
                ..
            } = &request
        {
            let binding = store::transfer_evidence::get(&self.store.pool, binding).await?;
            if let Content::NeedBinding { terms, .. } = binding.evidence.content {
                self.refuse_unreadable(actor, std::slice::from_ref(&terms.selection.need))
                    .await?;
            }
        }
        let committed = store::transfer_evidence::record(
            &self.store.pool,
            store::transfer_evidence::OutcomeInput {
                occurrence,
                expected_revision,
                person,
                request_id,
                request,
                authorization_intent: authorship.map(|proof| proof.intent_uid.clone()),
            },
            now,
            |hash| signer.as_ref().map(|signer| signer.sign_hash(hash)),
        )
        .await?;
        let saved = committed.saved;
        Ok(ActionOutcome {
            created: Some(saved.evidence.record.clone()),
            data: Some(serde_json::to_value(saved.evidence).map_err(EngineError::Json)?),
            facts: if committed.created {
                self.publish_committed_fact(saved.fact)
            } else {
                Vec::new()
            },
            ..Default::default()
        })
    }
}
