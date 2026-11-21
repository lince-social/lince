// non-lean-yet

use super::{
    Action, ActionOutcome, ControlFlow, DispatchFuture, Engine, EngineError, invalid_dispatch,
};

impl Engine {
    pub(super) fn dispatch_record_moves(
        &self,
        action: Action,
        actor: Option<String>,
    ) -> DispatchFuture<'_> {
        Box::pin(async move {
            let mut outcome = ActionOutcome::default();
            match action {
                Action::PreviewRecordMove { record, target } => {
                    if actor.is_some() {
                        return Err(EngineError::Forbidden(
                            "Only the local owner can preview Organ moves".into(),
                        ));
                    }
                    let (preview, _) = self.preview_record_move(&record, &target).await?;
                    outcome.data = Some(serde_json::to_value(preview)?);
                }
                Action::CancelReplicaOffer { root, target } => {
                    if actor.is_some() {
                        return Err(EngineError::Forbidden(
                            "Only the local owner can cancel replica offers".into(),
                        ));
                    }
                    if store::replica::state(&self.store.pool, &root, &target)
                        .await?
                        .as_deref()
                        != Some(store::replica::OFFERED)
                    {
                        return Err(EngineError::Consequence(
                            "This replica offer is no longer pending".into(),
                        ));
                    }
                    store::replica::revoke(&self.store.pool, &root, &target).await?;
                    store::offers::remember_outcome(
                        &self.store.pool,
                        store::offers::OfferKind::ReplicaGrant,
                        &root,
                        &target,
                        "cancelled",
                    )
                    .await?;
                    self.notify_query_changed();
                }
                Action::PendingOffers => {
                    if actor.is_some() {
                        return Err(EngineError::Forbidden(
                            "Only the local owner can inspect pending Organ offers".into(),
                        ));
                    }
                    outcome.data = Some(self.pending_offer_status().await?);
                }
                Action::AnswerRecordMove { offer, accept } => {
                    if actor.is_some() {
                        return Err(EngineError::Forbidden(
                            "Only the local owner can answer Organ moves".into(),
                        ));
                    }
                    self.answer_record_move(&offer, accept).await?;
                }
                Action::MoveRecordTo {
                    record,
                    target,
                    expected_preview,
                } => {
                    if actor.is_some() {
                        return Err(EngineError::Forbidden(
                            "Only the local owner can move Organ data".into(),
                        ));
                    }
                    let uid = self
                        .offer_record_move(&record, &target, expected_preview.as_deref())
                        .await?;
                    outcome.data = Some(
                        serde_json::json!({"offer":uid,"status":"Awaiting explicit acceptance. Source retained locally."}),
                    );
                }
                Action::CancelRecordMove { record } => {
                    if actor.is_some() {
                        return Err(EngineError::Forbidden(
                            "Only the local owner can cancel Organ moves".into(),
                        ));
                    }
                    self.cancel_record_move(&record).await?;
                }
                _ => return Err(invalid_dispatch()),
            }
            Ok(ControlFlow::Continue(outcome))
        })
    }
}
