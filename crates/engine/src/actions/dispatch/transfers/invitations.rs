// non-lean-yet

use super::super::{
    Action, ActionOutcome, ControlFlow, DateTime, DispatchOutcome, Engine, EngineError, RecordKind,
    Utc, VerifiedActionAuthorship, invalid_dispatch, normalize_transfer_invitation_expiry,
    reject_existing_transfer_revision_request, transfer_invitation_replay,
    transfer_request_id_conflict,
};

impl Engine {
    pub(in super::super) async fn dispatch_transfers_invitations(
        &self,
        action: Action,
        actor: Option<String>,
        now: DateTime<Utc>,
        verified_authorship: Option<VerifiedActionAuthorship>,
    ) -> DispatchOutcome {
        let mut outcome = ActionOutcome::default();
        match action {
            Action::AddressTransferInvitation {
                transfer,
                expected_revision,
                request_id,
                person,
                expires_at,
            } => {
                let transfer = self.resolve(&transfer).await?;
                self.require_permission(actor.as_deref(), "transfer:update")
                    .await?;
                let creator = self.transfer_creator_person(&transfer).await?;
                let acting = self
                    .transfer_action_person(actor.as_deref(), None, Some(&creator))
                    .await?;
                if acting != creator {
                    return Err(EngineError::Forbidden(
                        "only the transfer creator may address an invitation".into(),
                    ));
                }
                let addressed = self.resolve(person.trim()).await?;
                if let Some(event) = store::transfers::invitation_event_for_request(
                    &self.store.pool,
                    request_id.trim(),
                )
                .await?
                {
                    let replayed =
                        store::transfers::invitation(&self.store.pool, &event.invitation_uid)
                            .await?
                            .ok_or_else(|| {
                                EngineError::Consequence("unknown transfer invitation".into())
                            })?;
                    if event.kind != "addressed"
                        || event.transfer_uid != transfer
                        || replayed.addressed_person_uid != addressed
                    {
                        return Err(transfer_request_id_conflict());
                    }
                    outcome.created = Some(event.invitation_uid);
                    return Ok(ControlFlow::Break(outcome));
                }
                reject_existing_transfer_revision_request(&self.store.pool, request_id.trim())
                    .await?;
                let addressed_record = store::records::get(&self.store.pool, &addressed)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(addressed.clone()))?;
                if addressed_record.kind != RecordKind::Person.as_str() {
                    return Err(EngineError::Consequence(
                        "transfer invitation addressee must be a Person record".into(),
                    ));
                }
                let signer = self
                    .transfer_person_signer(&acting, verified_authorship.as_ref())
                    .await?;
                let commit = store::transfers::address_transfer_invitation(
                    &self.store.pool,
                    store::transfers::AddressTransferInvitationInput {
                        transfer_uid: transfer.clone(),
                        expected_revision,
                        idempotency_key: request_id.trim().to_string(),
                        addressed_person_uid: addressed,
                        invited_by_person_uid: acting,
                        expires_at: normalize_transfer_invitation_expiry(expires_at, now)?,
                    },
                    now,
                    |hash| signer.as_ref().map(|value| value.sign_hash(hash)),
                )
                .await?;
                self.apply_transfer_invitation_commit(commit, expected_revision, &mut outcome)?;
            }
            Action::AcceptTransferInvitation {
                invitation,
                expected_revision,
                request_id,
                transfer,
                person,
            } => {
                let invitation_row = store::transfers::invitation(&self.store.pool, &invitation)
                    .await?
                    .ok_or_else(|| {
                        EngineError::Consequence("unknown transfer invitation".into())
                    })?;
                self.require_permission(actor.as_deref(), "transfer:update")
                    .await?;
                if let Some(transfer) = transfer.as_deref() {
                    let transfer = self.resolve(transfer).await?;
                    if transfer != invitation_row.transfer_uid {
                        return Err(transfer_request_id_conflict());
                    }
                }
                let acting = self
                    .transfer_action_person(
                        actor.as_deref(),
                        person.as_deref(),
                        Some(&invitation_row.addressed_person_uid),
                    )
                    .await?;
                if acting != invitation_row.addressed_person_uid {
                    return Err(EngineError::Forbidden(
                        "only the addressed Person may accept an invitation".into(),
                    ));
                }
                if let Some(created) = transfer_invitation_replay(
                    &self.store.pool,
                    request_id.trim(),
                    &invitation,
                    "accepted",
                )
                .await?
                {
                    outcome.created = Some(created);
                    return Ok(ControlFlow::Break(outcome));
                }
                let signer = self
                    .transfer_person_signer(&acting, verified_authorship.as_ref())
                    .await?;
                let commit = store::transfers::accept_transfer_invitation(
                    &self.store.pool,
                    store::transfers::InvitationTransitionInput {
                        invitation_uid: invitation,
                        expected_revision,
                        idempotency_key: request_id.trim().to_string(),
                        actor_person_uid: Some(acting),
                        expires_at: None,
                    },
                    now,
                    |hash| signer.as_ref().map(|value| value.sign_hash(hash)),
                )
                .await?;
                self.apply_transfer_invitation_commit(commit, expected_revision, &mut outcome)?;
            }
            Action::RejectTransferInvitation {
                invitation,
                request_id,
                transfer,
                person,
            } => {
                let invitation_row = store::transfers::invitation(&self.store.pool, &invitation)
                    .await?
                    .ok_or_else(|| {
                        EngineError::Consequence("unknown transfer invitation".into())
                    })?;
                self.require_permission(actor.as_deref(), "transfer:update")
                    .await?;
                if let Some(transfer) = transfer.as_deref() {
                    let transfer = self.resolve(transfer).await?;
                    if transfer != invitation_row.transfer_uid {
                        return Err(transfer_request_id_conflict());
                    }
                }
                let acting = self
                    .transfer_action_person(
                        actor.as_deref(),
                        person.as_deref(),
                        Some(&invitation_row.addressed_person_uid),
                    )
                    .await?;
                if acting != invitation_row.addressed_person_uid {
                    return Err(EngineError::Forbidden(
                        "only the addressed Person may reject an invitation".into(),
                    ));
                }
                store::offers::refuse(
                    &self.store.pool,
                    store::offers::OfferKind::Transfer,
                    &invitation_row.transfer_uid,
                    &invitation_row.addressed_person_uid,
                )
                .await?;
                if let Some(created) = transfer_invitation_replay(
                    &self.store.pool,
                    request_id.trim(),
                    &invitation,
                    "rejected",
                )
                .await?
                {
                    outcome.created = Some(created);
                    return Ok(ControlFlow::Break(outcome));
                }
                let signer = self
                    .transfer_person_signer(&acting, verified_authorship.as_ref())
                    .await?;
                let commit = store::transfers::reject_transfer_invitation(
                    &self.store.pool,
                    store::transfers::InvitationTransitionInput {
                        invitation_uid: invitation,
                        expected_revision: 0,
                        idempotency_key: request_id.trim().to_string(),
                        actor_person_uid: Some(acting),
                        expires_at: None,
                    },
                    now,
                    |hash| signer.as_ref().map(|value| value.sign_hash(hash)),
                )
                .await?;
                self.apply_transfer_invitation_commit(commit, 0, &mut outcome)?;
            }
            Action::WithdrawTransferInvitation {
                invitation,
                expected_revision,
                request_id,
            } => {
                let invitation_row = store::transfers::invitation(&self.store.pool, &invitation)
                    .await?
                    .ok_or_else(|| {
                        EngineError::Consequence("unknown transfer invitation".into())
                    })?;
                self.require_permission(actor.as_deref(), "transfer:update")
                    .await?;
                let creator = self
                    .transfer_creator_person(&invitation_row.transfer_uid)
                    .await?;
                let acting = self
                    .transfer_action_person(actor.as_deref(), None, Some(&creator))
                    .await?;
                if acting != creator {
                    return Err(EngineError::Forbidden(
                        "only the transfer creator may change an invitation".into(),
                    ));
                }
                if let Some(created) = transfer_invitation_replay(
                    &self.store.pool,
                    request_id.trim(),
                    &invitation,
                    "withdrawn",
                )
                .await?
                {
                    outcome.created = Some(created);
                    return Ok(ControlFlow::Break(outcome));
                }
                let signer = self
                    .transfer_person_signer(&acting, verified_authorship.as_ref())
                    .await?;
                let input = store::transfers::InvitationTransitionInput {
                    invitation_uid: invitation,
                    expected_revision,
                    idempotency_key: request_id.trim().to_string(),
                    actor_person_uid: Some(acting),
                    expires_at: None,
                };
                let commit = store::transfers::withdraw_transfer_invitation(
                    &self.store.pool,
                    input,
                    now,
                    |hash| signer.as_ref().map(|value| value.sign_hash(hash)),
                )
                .await?;
                self.apply_transfer_invitation_commit(commit, expected_revision, &mut outcome)?;
            }
            Action::ReopenTransferInvitation {
                invitation,
                expected_revision,
                request_id,
                expires_at,
            } => {
                let invitation_row = store::transfers::invitation(&self.store.pool, &invitation)
                    .await?
                    .ok_or_else(|| {
                        EngineError::Consequence("unknown transfer invitation".into())
                    })?;
                self.require_permission(actor.as_deref(), "transfer:update")
                    .await?;
                let creator = self
                    .transfer_creator_person(&invitation_row.transfer_uid)
                    .await?;
                let acting = self
                    .transfer_action_person(actor.as_deref(), None, Some(&creator))
                    .await?;
                if acting != creator {
                    return Err(EngineError::Forbidden(
                        "only the transfer creator may reopen an invitation".into(),
                    ));
                }
                if let Some(created) = transfer_invitation_replay(
                    &self.store.pool,
                    request_id.trim(),
                    &invitation,
                    "reopened",
                )
                .await?
                {
                    outcome.created = Some(created);
                    return Ok(ControlFlow::Break(outcome));
                }
                let signer = self
                    .transfer_person_signer(&acting, verified_authorship.as_ref())
                    .await?;
                let commit = store::transfers::reopen_transfer_invitation(
                    &self.store.pool,
                    store::transfers::InvitationTransitionInput {
                        invitation_uid: invitation,
                        expected_revision,
                        idempotency_key: request_id.trim().to_string(),
                        actor_person_uid: Some(acting),
                        expires_at: normalize_transfer_invitation_expiry(expires_at, now)?,
                    },
                    now,
                    |hash| signer.as_ref().map(|value| value.sign_hash(hash)),
                )
                .await?;
                self.apply_transfer_invitation_commit(commit, expected_revision, &mut outcome)?;
            }
            Action::CounterofferTransfer {
                transfer,
                expected_revision,
                request_id,
                person,
                draft,
            } => {
                let transfer = self.resolve(&transfer).await?;
                let acting = self
                    .transfer_action_person(actor.as_deref(), person.as_deref(), None)
                    .await?;
                if store::transfers::phase4_request_for_request(&self.store.pool, &request_id)
                    .await?
                    .is_some()
                {
                    return Err(transfer_request_id_conflict());
                }
                self.require_permission(actor.as_deref(), "transfer:update")
                    .await?;
                let request_id = request_id.trim().to_string();
                if request_id.is_empty() || request_id.chars().count() > 200 {
                    return Err(EngineError::Consequence(
                        "transfer request_id must contain 1 to 200 characters".into(),
                    ));
                }
                if store::transfers::party_for_actor(&self.store.pool, &transfer, &acting)
                    .await?
                    .is_none()
                {
                    return Err(EngineError::Forbidden(
                        "a counteroffer requires an accepted transfer participant".into(),
                    ));
                }
                if let Some((replayed_transfer, _, replayed_action)) =
                    store::transfers::revision_for_request(&self.store.pool, request_id.trim())
                        .await?
                {
                    if replayed_transfer != transfer
                        || replayed_action != "counteroffer-transfer-draft"
                    {
                        return Err(transfer_request_id_conflict());
                    }
                    outcome.created = Some(transfer);
                    return Ok(ControlFlow::Break(outcome));
                }
                if store::transfers::invitation_event_for_request(
                    &self.store.pool,
                    request_id.trim(),
                )
                .await?
                .is_some()
                {
                    return Err(transfer_request_id_conflict());
                }
                let creator = self.transfer_creator_person(&transfer).await?;
                let submitted_creator = if draft.creator.trim().is_empty() {
                    creator.clone()
                } else {
                    self.resolve(draft.creator.trim()).await?
                };
                if submitted_creator != creator {
                    return Err(EngineError::Conflict {
                        code: "transfer_creator_immutable",
                        message: "transfer creator Person cannot change in a counteroffer".into(),
                    });
                }
                let mut input = self
                    .resolve_whole_transfer_draft(
                        transfer.clone(),
                        expected_revision,
                        request_id,
                        draft,
                        creator,
                        acting.clone(),
                        now,
                        true,
                        actor.as_deref().or_else(|| {
                            verified_authorship
                                .as_ref()
                                .map(|authorship| authorship.person_uid.as_str())
                        }),
                    )
                    .await?;
                input.authorization_intent_uid = verified_authorship
                    .as_ref()
                    .map(|value| value.intent_uid.clone());
                let signer = self
                    .transfer_person_signer(&acting, verified_authorship.as_ref())
                    .await?;
                let commit = store::transfers::counteroffer_whole_draft(
                    &self.store.pool,
                    input,
                    now,
                    Some(acting),
                    |hash| signer.as_ref().map(|value| value.sign_hash(hash)),
                )
                .await?;
                self.apply_transfer_revision_commit(
                    commit,
                    expected_revision,
                    &transfer,
                    &mut outcome,
                )
                .await?;
            }
            _ => return Err(invalid_dispatch()),
        }
        Ok(ControlFlow::Continue(outcome))
    }
}
