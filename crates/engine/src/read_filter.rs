use std::collections::HashSet;

use crate::Engine;
use crate::error::EngineError;

pub struct PersonFilter {
    pub predicate: protein::Predicate,
}

impl Engine {
    pub async fn read_filter_of(
        &self,
        person_uid: &str,
    ) -> Result<Option<PersonFilter>, EngineError> {
        let Some(predicate) =
            protein::read_rules::effective_predicate(&self.store, person_uid).await?
        else {
            return Ok(None);
        };
        Ok(Some(PersonFilter { predicate }))
    }

    pub async fn set_read_filter(
        &self,
        person_uid: &str,
        predicate: Option<&protein::Predicate>,
    ) -> Result<(), EngineError> {
        let raw = match predicate {
            Some(predicate) => Some(
                serde_json::to_string(predicate)
                    .map_err(|error| EngineError::Consequence(error.to_string()))?,
            ),
            None => None,
        };
        store::read_filter::set(&self.store.pool, person_uid, raw.as_deref()).await?;
        Ok(())
    }

    pub async fn readable_by(
        &self,
        person_uid: &str,
    ) -> Result<Option<HashSet<String>>, EngineError> {
        let Some(filter) = self.read_filter_of(person_uid).await? else {
            return Ok(None);
        };
        if let Some(access) = store::auth::person_access(&self.store.pool, person_uid).await? {
            if let Some(role) = access.role_id {
                if store::role_policies::get(&self.store.pool, role)
                    .await?
                    .is_some_and(|row| row.policy.is_some())
                {
                    let query = protein::Protein {
                        source: protein::Source::Record,
                        filter: vec![],
                        fields: Some(vec!["uid".into()]),
                        include: Default::default(),
                        aggregate: None,
                        order: vec![],
                        limit: None,
                    };
                    let rows = protein::execute_for(&self.store, &query, Some(person_uid)).await?;
                    return Ok(Some(
                        rows.into_iter()
                            .filter_map(|row| row["uid"].as_str().map(str::to_string))
                            .collect(),
                    ));
                }
            }
        }
        let protein = protein::Protein {
            source: protein::Source::Record,
            filter: vec![filter.predicate],
            fields: None,
            include: Default::default(),
            aggregate: None,
            order: vec![],
            limit: None,
        };
        let matched = protein::matching_records(&self.store, &protein, None).await?;
        Ok(Some(matched.into_iter().map(|row| row.uid).collect()))
    }

    pub(crate) async fn record_targets_of(
        &self,
        action: &crate::actions::Action,
    ) -> Result<Vec<String>, EngineError> {
        use crate::actions::Action;
        let named: Vec<&String> = match action {
            Action::AddQuantityGroupExact { changes } => changes.keys().collect(),
            Action::SetTransferPrivateApplicationPolicy { effects, .. } => effects.iter().map(|effect| &effect.record).collect(),
            Action::SetTransferChildRequirement { transfer, child, .. } => vec![transfer, child],
            Action::ProposeTransferCancellation { transfer, .. }
            | Action::ProposeTransferLoanExtension { transfer, .. }
            | Action::ApplyTransferCancellation { transfer, .. } => vec![transfer],
            Action::ApplyTransferApplication { local_record, .. } => vec![local_record],
            Action::ChangeRecord { request } => vec![&request.record_uid],
            Action::CreateMessage { thread, .. } => vec![thread],
            Action::ProposeGroup { thread, .. } => vec![thread],
            Action::SetGroupPerson { root, .. } | Action::RemoveGroupOrgan { root, .. } => {
                vec![root]
            }
            Action::CreateMessageDraft {
                conversation,
                thread,
                ..
            } => vec![conversation, thread],
            Action::SetRecordStockLimit { record: target, .. }
            | Action::SetQuantity { target, .. }
            | Action::AddQuantityExact { target, .. }
            | Action::AddQuantity { target, .. }
            | Action::Activate { target }
            | Action::Deactivate { target }
            | Action::CaptureEntry { target, .. }
            | Action::SetSlug { target, .. }
            | Action::SetUnit { target, .. }
            | Action::SetExtension { target, .. }
            | Action::SetPlace { target, .. }
            | Action::GrantVisibility { target, .. }
            | Action::CreateThread { target, .. }
            | Action::SetQuantityExact { target, .. }
            | Action::EditRecordText { target, .. }
            | Action::ReviseMessage {
                message: target, ..
            }
            | Action::ReviseMessageDraft { draft: target, .. }
            | Action::DeleteMessageDraft { draft: target }
            | Action::SendMessageDraft { draft: target }
            | Action::DeleteRecord { target }
            | Action::MoveRecordTo { record: target, .. }
            | Action::CancelRecordMove { record: target } => vec![target],
            Action::PreviewAreaTransition {
                target,
                changes,
                constraints,
            } => {
                let mut targets = vec![target];
                targets.extend(
                    changes
                        .assign
                        .iter()
                        .chain(&changes.unassign)
                        .chain(&constraints.assign)
                        .chain(&constraints.unassign),
                );
                targets
            }
            Action::ApplyAreaTransition { preview, .. } => {
                let mut targets = vec![&preview.target];
                targets.extend(
                    preview
                        .changes
                        .assign
                        .iter()
                        .chain(&preview.changes.unassign),
                );
                targets
            }
            Action::TransitionRecord { subject, .. }
            | Action::SetIdentity { subject, .. }
            | Action::RetractRecord { subject, .. } => vec![subject],
            Action::AssertRecord {
                subject, object, ..
            } => {
                let mut targets = vec![subject];
                targets.extend(object.iter());
                targets
            }
            Action::RefineAssertion {
                subject, object, ..
            } => vec![subject, object],
            Action::SetAssertionOrder { ordered, .. } => ordered.iter().collect(),
            _ => Vec::new(),
        };
        let mut out = Vec::new();
        for name in named {
            if let Ok(uid) = self.resolve(name).await {
                out.push(uid);
            }
        }
        match action {
            Action::RetractAssertion { assertion } => {
                if let Some(assertion) = store::assertions::get(&self.store.pool, assertion).await?
                {
                    out.push(assertion.subject_uid);
                }
            }
            Action::ReviseEntry { entry, .. } | Action::VoidEntry { entry, .. } => {
                if let Some(entry) = store::entries::get(&self.store.pool, entry).await? {
                    out.push(entry.record_uid);
                }
            }
            Action::CompensateTransferApplication { application, .. } => {
                if let Some(fact) = store::sqlx::query_scalar::<_, String>("SELECT application_fact_uid FROM transfer_local_application WHERE uid = ?")
                    .bind(application).fetch_optional(&self.store.pool).await? {
                    out.extend(store::transfer_effects::records(&self.store.pool, &fact).await?);
                }
            }
            Action::ApplyTransferApplication { handoff, request_id, .. } => {
                if let Some(previous) = store::transfer_delivery::local_application_for_request(&self.store.pool, request_id).await? {
                    out.extend(store::transfer_effects::records(&self.store.pool, &previous.application_fact_uid).await?);
                } else if let Some(handoff) = store::transfer_delivery::application_effect_handoff(&self.store.pool, handoff).await? {
                    let exchange = self.application_handoff_exchange(&handoff, false).await?;
                    if let Some(policy) = store::transfer_accounting::policy(&self.store.pool, &handoff.transfer_uid, &exchange, &handoff.participant_person_uid).await? {
                        out.extend(policy.effects.into_iter().map(|effect| effect.record));
                    }
                }
            }
            Action::SettleTransferOccurrence { occurrence, .. } => {
                if let Some(occurrence) = store::transfers::occurrence(&self.store.pool, occurrence).await? {
                    let owner = store::misc::get_promise(&self.store.pool, &occurrence.promise_uid).await?
                        .and_then(|promise| promise.party_uid);
                    if let Some(owner) = owner {
                        if let Some(policy) = store::transfer_accounting::policy(&self.store.pool, &occurrence.transfer_uid,
                            occurrence.exchange_uid.as_deref().unwrap_or(&occurrence.promise_uid), &owner).await? {
                            out.extend(policy.effects.into_iter().map(|effect| effect.record));
                        }
                        if let Some(record) = store::transfer_accounting::bound_record(&self.store.pool, &occurrence.transfer_uid,
                            occurrence.exchange_uid.as_deref().unwrap_or(&occurrence.promise_uid), &owner,
                            occurrence.record_uid.as_deref()).await? { out.push(record); }
                    }
                }
            }
            Action::CompensateTransferOccurrenceSettlement { settlement, .. } => {
                if let Some(slice) = store::transfers::occurrence_settlement_slice(&self.store.pool, settlement).await? {
                    out.extend(store::transfer_effects::records(&self.store.pool, &slice.application_fact_uid).await?);
                }
            }
            Action::ClassifyFact { fact, .. } => {
                if let Some(fact) = store::facts::get(&self.store.pool, fact).await? {
                    out.push(fact.record_uid);
                }
            }
            _ => {}
        }
        Ok(out)
    }

    pub async fn refuse_unreadable(
        &self,
        actor: Option<&str>,
        targets: &[String],
    ) -> Result<(), EngineError> {
        if targets.is_empty() {
            return Ok(());
        }
        let Some(actor) = actor else {
            return Ok(());
        };
        for target in targets {
            if !self.may_read_record(Some(actor), target).await? {
                return Err(EngineError::Forbidden(
                    "a Record is outside what this login may see, so it may not be changed either".into()
                ));
            }
        }
        Ok(())
    }
}
