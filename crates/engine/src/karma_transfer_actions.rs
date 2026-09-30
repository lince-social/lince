use chrono::{DateTime, Utc};
use nucleus::transfer::AgreementGuard;

use crate::{
    Engine, EngineError,
    actions::{Action, ActionOutcome, VerifiedActionAuthorship},
};

tokio::task_local! {
    pub(crate) static ACTIVATION_GUARD: (Option<AgreementGuard>, Option<nucleus::transfer::karma::Guard>);
}

fn invalid(message: impl ToString) -> EngineError {
    EngineError::Conflict {
        code: "karma_transfer_state_changed",
        message: message.to_string(),
    }
}

pub(crate) fn guard_error(error: store::StoreError) -> EngineError {
    match error {
        store::StoreError::Protocol(message) if message == "transfer_state_source_changed" => {
            EngineError::Conflict {
                code: "transfer_state_source_changed",
                message:
                    "Transfer state changed after evaluation; refresh before assigning its state"
                        .into(),
            }
        }
        store::StoreError::Protocol(message) if message == "transfer_agreement_source_changed" => {
            EngineError::Conflict {
                code: "transfer_agreement_source_changed",
                message: "Agreement changed after evaluation; the old state assignment was refused"
                    .into(),
            }
        }
        other => EngineError::Store(other),
    }
}

pub(crate) fn fulfillment_request_id(
    transfer: &str,
    person: &str,
    promise: &str,
    fulfillment: &str,
) -> Result<String, EngineError> {
    if fulfillment.trim().is_empty()
        || fulfillment.len() > 200
        || fulfillment.chars().any(char::is_control)
    {
        return Err(invalid(
            "Use a fulfillment key of 1–200 bytes without control characters",
        ));
    }
    let hash = nucleus::karma::canonical_hash("lince.transfer-fulfillment.v1", &serde_json::json!({"transfer":transfer,"person":person,"promise":promise,"fulfillment":fulfillment})).map_err(invalid)?;
    Ok(format!("fulfillment:{}", hash.as_str()))
}

impl Engine {
    pub(crate) async fn check_transfer_action_guard(
        &self,
        transfer: &str,
        person: &str,
        expected: Option<&AgreementGuard>,
        actor: Option<&str>,
    ) -> Result<(), EngineError> {
        let state = self.transfer_karma_snapshot(transfer, actor).await?;
        let current = state
            .participants
            .get(person)
            .ok_or_else(|| invalid("The acting Person is not an accepted visible participant"))?;
        if expected.is_some_and(|expected| expected != &current.guard) {
            return Err(invalid(
                "Agreement changed after the state assignment was evaluated",
            ));
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn publish_transfer_action(
        &self,
        transfer: String,
        revision: u64,
        request: String,
        person: Option<String>,
        expected: Option<AgreementGuard>,
        expected_state: Option<nucleus::transfer::karma::Guard>,
        actor: Option<&str>,
        now: DateTime<Utc>,
        authorship: Option<&VerifiedActionAuthorship>,
    ) -> Result<ActionOutcome, EngineError> {
        if request.trim().is_empty() || request.len() > 200 {
            return Err(invalid("Use a request identity of 1–200 bytes"));
        }
        let transfer = self.resolve(&transfer).await?;
        self.require_permission(actor, "transfer:update").await?;
        let acting = self
            .transfer_action_person(actor, person.as_deref(), None)
            .await?;
        let signer = self.transfer_person_signer(&acting, authorship).await?;
        let mut outcome = ActionOutcome::default();
        if let Some((owner, result_revision, action)) =
            store::transfers::revision_for_request(&self.store.pool, &request).await?
        {
            let fact = store::transfers::revision_fact(&self.store.pool, &owner, result_revision)
                .await?
                .ok_or_else(|| invalid("Publication receipt has no signed revision"))?;
            let evidence: nucleus::transfer::TransferRevisionEvidence =
                serde_json::from_str(fact.payload.as_deref().unwrap_or(""))
                    .map_err(EngineError::Json)?;
            if owner != transfer
                || action != "publish-transfer"
                || fact.actor_uid.as_deref() != Some(&acting)
                || evidence.previous_revision != Some(revision)
            {
                return Err(invalid(
                    "Publication request identity was used for different input",
                ));
            }
            outcome.data = Some(
                serde_json::json!({"transfer": transfer, "revision": result_revision, "published": true, "replayed": true}),
            );
            return Ok(outcome);
        }
        self.require_verified_transfer_revision(&transfer, revision)
            .await?;
        self.check_transfer_action_guard(&transfer, &acting, expected.as_ref(), actor)
            .await?;
        let state = self.transfer_karma_snapshot(&transfer, actor).await?;
        if state.published {
            outcome.data = Some(
                serde_json::json!({"transfer": transfer, "revision": revision, "published": true, "unchanged": true}),
            );
            return Ok(outcome);
        }
        let mut input = store::transfers::publication_input(
            &self.store.pool,
            &transfer,
            revision,
            &acting,
            &request,
        )
        .await?;
        input.authorization_intent_uid = authorship.map(|value| value.intent_uid.clone());
        let commit = store::transfers::publish_whole_draft_guarded(
            &self.store.pool,
            input,
            expected.as_ref(),
            expected_state.as_ref(),
            now,
            Some(acting),
            |hash| signer.as_ref().map(|signer| signer.sign_hash(hash)),
        )
        .await
        .map_err(guard_error)?;
        self.apply_transfer_revision_commit(commit, revision, &transfer, &mut outcome)
            .await?;
        outcome.data = Some(
            serde_json::json!({"transfer": transfer, "revision": revision + 1, "published": true}),
        );
        Ok(outcome)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn activate_transfer_fulfillment_action(
        &self,
        transfer: String,
        promise: String,
        fulfillment: String,
        revision: u64,
        person: Option<String>,
        expected: Option<AgreementGuard>,
        expected_state: Option<nucleus::transfer::karma::Guard>,
        actor: Option<&str>,
        now: DateTime<Utc>,
        authorship: Option<&VerifiedActionAuthorship>,
    ) -> Result<ActionOutcome, EngineError> {
        let transfer = self.resolve(&transfer).await?;
        self.require_permission(actor, "transfer:update").await?;
        let acting = self
            .transfer_action_person(actor, person.as_deref(), None)
            .await?;
        self.transfer_person_signer(&acting, authorship).await?;
        let request_id = fulfillment_request_id(&transfer, &acting, &promise, &fulfillment)?;
        let original =
            store::transfers::occurrences_for_activation_request(&self.store.pool, &request_id)
                .await?;
        let original_revision = if let Some(original) = original {
            self.require_permission(actor, "transfer:read").await?;
            original.revision
        } else {
            self.require_verified_transfer_revision(&transfer, revision)
                .await?;
            self.check_transfer_action_guard(&transfer, &acting, expected.as_ref(), actor)
                .await?;
            revision
        };
        ACTIVATION_GUARD
            .scope(
                (expected, expected_state),
                Box::pin(self.act_at_with_authorship(
                    Action::ActivateTransferOccurrence {
                        transfer,
                        promise,
                        expected_revision: original_revision,
                        request_id,
                        person: Some(acting),
                    },
                    actor.map(str::to_owned),
                    now,
                    authorship.cloned(),
                )),
            )
            .await
    }
}
