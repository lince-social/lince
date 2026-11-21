// non-lean-yet

use super::*;
use std::{future::Future, ops::ControlFlow, pin::Pin};

mod access;
mod contacts;
mod conversations;
mod file_sync;
mod fiote;
mod karma;
mod mailboxes;
mod messaging;
mod organ_management;
mod promises;
mod record_extensions;
mod record_moves;
mod record_mutations;
mod records;
mod saved_proteins;
mod signals;
mod social;
mod transfers;
mod vocabulary;
mod workspace_components;

type DispatchOutcome = Result<ControlFlow<ActionOutcome, ActionOutcome>, EngineError>;
type DispatchFuture<'engine> = Pin<Box<dyn Future<Output = DispatchOutcome> + Send + 'engine>>;

fn invalid_dispatch() -> EngineError {
    EngineError::Consequence("Invalid internal action dispatch".into())
}

impl Engine {
    pub(super) fn dispatch_action(
        &self,
        action: Action,
        actor: Option<String>,
        now: DateTime<Utc>,
        verified_authorship: Option<VerifiedActionAuthorship>,
    ) -> DispatchFuture<'_> {
        match action {
            Action::RecordJourneyProgress {
                transfer,
                expected_revision,
                person,
                request_id,
                step,
            } => Box::pin(async move {
                self.record_journey_progress(
                    transfer,
                    expected_revision,
                    person,
                    request_id,
                    step,
                    actor.as_deref(),
                    now,
                    verified_authorship.as_ref(),
                )
                .await
                .map(ControlFlow::Continue)
            }),
            Action::Audience { request } => Box::pin(async move {
                self.audience_command(request, actor.as_deref(), now)
                    .await
                    .map(ControlFlow::Continue)
            }),
            Action::Identity { request } => Box::pin(async move {
                self.identity_command_with_authorship(
                    request,
                    actor.as_deref(),
                    now,
                    verified_authorship.as_ref(),
                )
                .await
                .map(ControlFlow::Continue)
            }),
            Action::TrustAnalysis {
                person,
                request_id,
                request,
            } => Box::pin(async move {
                self.trust_analysis_command(
                    person.as_deref(),
                    request_id,
                    request,
                    actor.as_deref(),
                    now,
                    verified_authorship.as_ref(),
                )
                .await
                .map(ControlFlow::Continue)
            }),
            Action::TransferReceipt {
                person,
                request_id,
                request,
            } => Box::pin(async move {
                self.transfer_receipt_command(
                    person.as_deref(),
                    request_id,
                    request,
                    actor.as_deref(),
                    now,
                    verified_authorship.as_ref(),
                )
                .await
                .map(ControlFlow::Continue)
            }),
            Action::RecordTransferOutcome { .. } => Box::pin(async move {
                self.record_transfer_outcome(
                    action,
                    actor.as_deref(),
                    now,
                    verified_authorship.as_ref(),
                )
                .await
                .map(ControlFlow::Continue)
            }),
            Action::Vocabulary { request } => Box::pin(async move {
                self.vocabulary_command(request, actor.as_deref())
                    .await
                    .map(ControlFlow::Continue)
            }),
            Action::Knowledge { request } => Box::pin(async move {
                self.knowledge_command(request, actor.as_deref())
                    .await
                    .map(ControlFlow::Continue)
            }),
            Action::Demeter { request } => Box::pin(async move {
                self.demeter_command(request, actor.as_deref(), now)
                    .await
                    .map(ControlFlow::Continue)
            }),
            Action::Optimize { request } => Box::pin(async move {
                self.optimization_command(request, actor.as_deref(), now)
                    .await
                    .map(ControlFlow::Continue)
            }),
            Action::Sources { request } => Box::pin(async move {
                self.source_command(request, actor.as_deref())
                    .await
                    .map(ControlFlow::Continue)
            }),
            Action::SavedLocation { request } => Box::pin(async move {
                self.saved_location_command(request, actor.as_deref(), now)
                    .await
                    .map(ControlFlow::Continue)
            }),
            Action::Location { request } => Box::pin(async move {
                self.location_request(request, actor.as_deref())
                    .await
                    .map(ControlFlow::Continue)
            }),
            Action::Plugins { request } => Box::pin(async move {
                self.plugin_command(request, actor.as_deref())
                    .await
                    .map(ControlFlow::Continue)
            }),
            Action::Visibility { request } => Box::pin(async move {
                self.visibility_request(request, actor.as_deref())
                    .await
                    .map(ControlFlow::Continue)
            }),
            Action::InspectRecordAuthority { person, record_uid } => Box::pin(async move {
                self.inspect_record_authority(actor.as_deref(), &person, &record_uid)
                    .await
                    .map(ControlFlow::Continue)
            }),
            Action::Workspace { request } => Box::pin(async move {
                self.workspace_request(request, actor.as_deref())
                    .await
                    .map(ControlFlow::Continue)
            }),
            Action::SetRolePolicy {
                role,
                policy,
                expected_revision,
            } => Box::pin(async move {
                self.set_role_policy(actor.as_deref(), &role, policy, expected_revision)
                    .await
                    .map(ControlFlow::Continue)
            }),
            Action::AssignRoles {
                person,
                roles,
                expected_revision,
            } => Box::pin(async move {
                self.assign_roles(actor.as_deref(), &person, &roles, expected_revision)
                    .await
                    .map(ControlFlow::Continue)
            }),
            Action::RecordExtensions { target, request } => Box::pin(async move {
                self.record_extensions(target, request, actor.as_deref(), now)
                    .await
                    .map(ControlFlow::Continue)
            }),
            Action::Canvas { canvas, request } => Box::pin(async move {
                self.canvas_action(canvas, request, actor.as_deref())
                    .await
                    .map(ControlFlow::Continue)
            }),
            Action::InspectCanvases => Box::pin(async move {
                if actor.is_some() {
                    return Err(EngineError::Forbidden(
                        "Canvas inspection currently requires the local interface session.".into(),
                    ));
                }
                Ok(ControlFlow::Continue(ActionOutcome {
                    data: Some(serde_json::json!({"canvases":self.connected_canvases()})),
                    ..Default::default()
                }))
            }),
            Action::ChangeRecord { request } => Box::pin(async move {
                let outcome = self.change_record(request, actor.as_deref()).await?;
                Ok(ControlFlow::Continue(outcome))
            }),
            Action::CreateRecordWithTags { .. }
            | Action::CreateRecordDraft { .. }
            | Action::CreateRecord { .. } => {
                Box::pin(self.dispatch_records_creation(action, actor, now))
            }
            Action::TransitionRecord { .. }
            | Action::Activate { .. }
            | Action::Deactivate { .. }
            | Action::DeleteRecord { .. }
            | Action::EditRecordText { .. }
            | Action::SetSlug { .. }
            | Action::SetUnit { .. }
            | Action::SetAssertionOrder { .. }
            | Action::SetPlace { .. }
            | Action::GrantVisibility { .. } => {
                Box::pin(self.dispatch_records_editing(action, actor, now))
            }
            Action::SetQuantityExact { .. }
            | Action::SetQuantity { .. }
            | Action::AddQuantityGroupExact { .. }
            | Action::AddQuantityExact { .. }
            | Action::AddQuantity { .. } => {
                Box::pin(self.dispatch_records_quantities(action, actor, now))
            }
            Action::SandPackage { .. }
            | Action::CreateCustomComponent { .. }
            | Action::PresentComponent { .. }
            | Action::PreviewAreaTransition { .. }
            | Action::ApplyAreaTransition { .. } => {
                Box::pin(self.dispatch_workspace_components(action, actor, now))
            }
            Action::Social { .. } => Box::pin(self.dispatch_social(action, actor, now)),
            Action::ConfigureFiote { .. }
            | Action::CreateAgent { .. }
            | Action::ActivateFiote { .. }
            | Action::InspectFioteActivations { .. }
            | Action::ReportFioteChild { .. } => Box::pin(self.dispatch_fiote(action, actor, now)),
            Action::CaptureEntry { .. }
            | Action::CreateFrequency { .. }
            | Action::DeleteFrequency { .. }
            | Action::ReviseEntry { .. }
            | Action::VoidEntry { .. }
            | Action::ClassifyFact { .. } => {
                Box::pin(self.dispatch_karma_entries(action, actor, now))
            }
            Action::PreviewKarmaReading { .. }
            | Action::PreviewKarmaProposal { .. }
            | Action::InspectKarmaRuleHistory { .. }
            | Action::SaveKarmaRule { .. }
            | Action::ReviseKarmaField { .. } => {
                Box::pin(self.dispatch_karma_rules(action, actor, now))
            }
            Action::PreviewKarmaHabit { .. } | Action::ImportKarmaHabit { .. } => {
                Box::pin(self.dispatch_karma_habits(action, actor, now))
            }
            Action::SaveKarmaSchedule { .. }
            | Action::InspectKarmaSchedules { .. }
            | Action::PreviewKarmaScheduleDates { .. }
            | Action::CancelKarmaSchedule { .. }
            | Action::RetryKarmaSchedule { .. } => {
                Box::pin(self.dispatch_karma_schedules(action, actor, now))
            }
            Action::CreateRecurrence { .. }
            | Action::ReviseRecurrence { .. }
            | Action::DeleteRecurrence { .. }
            | Action::SetRecurrencePaused { .. }
            | Action::ApplyRecurrenceOccurrence { .. }
            | Action::SkipRecurrenceOccurrence { .. }
            | Action::UnskipRecurrenceOccurrence { .. } => {
                Box::pin(self.dispatch_karma_recurrences(action, actor, now))
            }
            Action::CreateKarmaProgram { .. }
            | Action::ReviseKarmaProgram { .. }
            | Action::ActivateKarmaProgram { .. }
            | Action::PauseKarmaProgram { .. }
            | Action::SetKarmaExecution { .. }
            | Action::DesignateKarmaExecutor { .. }
            | Action::RespondKarmaCandidate { .. } => {
                Box::pin(self.dispatch_karma_programs(action, actor, now))
            }
            Action::SaveKarmaFrequency { .. }
            | Action::CreateKarmaFrequency { .. }
            | Action::ReviseKarmaFrequency { .. }
            | Action::ActivateKarmaFrequency { .. }
            | Action::SetKarmaFrequencyParameters { .. }
            | Action::ResetKarmaFrequencyParameters { .. }
            | Action::PauseKarmaFrequency { .. } => {
                Box::pin(self.dispatch_karma_frequencies(action, actor, now))
            }
            Action::Decide { .. }
            | Action::SaveKarmaCommand { .. }
            | Action::RunKarmaCommand { .. }
            | Action::InspectKarmaCommands => {
                Box::pin(self.dispatch_karma_commands(action, actor, now))
            }
            Action::DiscardTransferDraft { .. }
            | Action::CreateTransferDraft { .. }
            | Action::ReviseTransferPromise { .. }
            | Action::ReviseTransferDraft { .. }
            | Action::AdoptTransferDraft { .. } => {
                Box::pin(self.dispatch_transfers_drafts(action, actor, now, verified_authorship))
            }
            Action::CreateTransferRemainderDraft { .. }
            | Action::CreateReversingTransferDraft { .. } => Box::pin(
                self.dispatch_transfers_correction_drafts(action, actor, now, verified_authorship),
            ),
            Action::AddressTransferInvitation { .. }
            | Action::AcceptTransferInvitation { .. }
            | Action::RejectTransferInvitation { .. }
            | Action::WithdrawTransferInvitation { .. }
            | Action::ReopenTransferInvitation { .. }
            | Action::CounterofferTransfer { .. } => Box::pin(self.dispatch_transfers_invitations(
                action,
                actor,
                now,
                verified_authorship,
            )),
            Action::CreateTransfer { .. }
            | Action::AddParty { .. }
            | Action::AddPromiseToTransfer { .. }
            | Action::AgreeTransfer { .. }
            | Action::SetTransferAgreementLevel { .. }
            | Action::AssignTransferAgreementLevel { .. }
            | Action::PublishTransfer { .. }
            | Action::ConfirmTransfer { .. }
            | Action::SetRecordStockLimit { .. } => Box::pin(self.dispatch_transfers_agreements(
                action,
                actor,
                now,
                verified_authorship,
            )),
            Action::ActivateTransfer { .. }
            | Action::ActivateTransferFulfillment { .. }
            | Action::ActivateTransferOccurrence { .. }
            | Action::SetTransferChildRequirement { .. }
            | Action::DesignateTransferExecutor { .. }
            | Action::InspectTransferKarma { .. } => Box::pin(self.dispatch_transfers_fulfillment(
                action,
                actor,
                now,
                verified_authorship,
            )),
            Action::ClaimOpenTransferPromise { .. }
            | Action::SetTransferOccurrenceClaim { .. }
            | Action::CompleteTransferOccurrenceClaimsBulk { .. }
            | Action::SetTransferOccurrenceDispute { .. } => {
                Box::pin(self.dispatch_transfers_claims(action, actor, now, verified_authorship))
            }
            Action::SettleTransfer { .. }
            | Action::SettleTransferOccurrence { .. }
            | Action::BeginTransferSettlement { .. }
            | Action::CompensateTransferOccurrenceSettlement { .. } => Box::pin(
                self.dispatch_transfers_settlement(action, actor, now, verified_authorship),
            ),
            Action::CompensateTransferApplication { .. }
            | Action::SetTransferPrivateApplicationPolicy { .. }
            | Action::SetTransferOccurrenceApplicationFormula { .. }
            | Action::ApplyTransferApplication { .. } => Box::pin(
                self.dispatch_transfers_applications(action, actor, now, verified_authorship),
            ),
            Action::ConfigureTransferDelivery { .. }
            | Action::SetTransferDeliveryMode { .. }
            | Action::RevokeTransferDelivery { .. }
            | Action::EnqueueTransferDelivery { .. }
            | Action::RetryTransferDelivery { .. }
            | Action::RefreshTransferDelivery { .. } => {
                Box::pin(self.dispatch_transfers_delivery(action, actor, now, verified_authorship))
            }
            Action::ProposeTransferLoanExtension { .. }
            | Action::ProposeTransferCancellation { .. }
            | Action::ApplyTransferCancellation { .. }
            | Action::ReopenTransferPromise { .. } => Box::pin(self.dispatch_transfers_amendments(
                action,
                actor,
                now,
                verified_authorship,
            )),
            Action::CreateThread { .. } | Action::CreateTransferThread { .. } => {
                Box::pin(self.dispatch_messaging_threads(action, actor, now, verified_authorship))
            }
            Action::CreateMessage { .. }
            | Action::ReviseMessage { .. }
            | Action::CreateTransferMessage { .. } => {
                Box::pin(self.dispatch_messaging_messages(action, actor, now, verified_authorship))
            }
            Action::CreateMessageDraft { .. }
            | Action::ReviseMessageDraft { .. }
            | Action::DeleteMessageDraft { .. }
            | Action::SendMessageDraft { .. } => {
                Box::pin(self.dispatch_messaging_drafts(action, actor, now, verified_authorship))
            }
            Action::CreatePromise { .. }
            | Action::PromiseTransition { .. }
            | Action::EditPromiseDelta { .. } => {
                Box::pin(self.dispatch_promises(action, actor, now))
            }
            Action::RenameRole { .. }
            | Action::DeleteRole { .. }
            | Action::CreateRole { .. }
            | Action::AssignRole { .. }
            | Action::GrantPermission { .. }
            | Action::RevokePermission { .. } => {
                Box::pin(self.dispatch_access_roles(action, actor))
            }
            Action::CreateUser { .. }
            | Action::UpdateUser { .. }
            | Action::DeleteUser { .. }
            | Action::SetPersonStanding { .. } => {
                Box::pin(self.dispatch_access_accounts(action, actor, now))
            }
            Action::SetPersonReadFilter { .. } | Action::SetRoleReadRules { .. } => {
                Box::pin(self.dispatch_access_read_rules(action, actor))
            }
            Action::SignalEndpoints { .. } | Action::CreateSignal { .. } => {
                Box::pin(self.dispatch_signals(action, actor, now))
            }
            Action::CreateLingua { .. }
            | Action::RenameLingua { .. }
            | Action::DeleteLingua { .. }
            | Action::CreateConcept { .. }
            | Action::RenameConcept { .. }
            | Action::DeleteConcept { .. }
            | Action::AdoptConcept { .. }
            | Action::RemoveConceptFromLingua { .. }
            | Action::AddConceptParent { .. }
            | Action::RemoveConceptParent { .. }
            | Action::PreviewInstinct
            | Action::ImportInstinct { .. }
            | Action::CreateMatchRule { .. }
            | Action::AdoptConcepts { .. }
            | Action::DeclareEquivalence { .. } => {
                Box::pin(self.dispatch_vocabulary(action, actor, now))
            }
            Action::SaveProtein { .. } => Box::pin(self.dispatch_saved_proteins(action, actor)),
            Action::SetContactDelivery { .. }
            | Action::ReconnectContact { .. }
            | Action::RenameOrganContact { .. }
            | Action::SetSyncPolicy { .. }
            | Action::SetContactAcceptScope { .. }
            | Action::HideRecordFromContact { .. }
            | Action::SetContactScope { .. }
            | Action::SetContactShare { .. }
            | Action::ForgetOrganContact { .. }
            | Action::AddKnownOrgan { .. }
            | Action::AuditContact { .. }
            | Action::SetContactTrust { .. }
            | Action::SetContactProximity { .. } => self.dispatch_contacts(action, actor, now),
            Action::SetExtension { .. } => self.dispatch_record_extensions(action, actor, now),
            Action::DeleteConversation { .. }
            | Action::SendRecordCopy { .. }
            | Action::StartConversation { .. }
            | Action::OpenThread { .. }
            | Action::AcceptThreadInvite { .. }
            | Action::DeclineThreadInvite { .. }
            | Action::ShareMyKey { .. }
            | Action::ReadMessageAttachment { .. } => {
                self.dispatch_conversations(action, actor, now)
            }
            Action::PreviewRecordMove { .. }
            | Action::CancelReplicaOffer { .. }
            | Action::PendingOffers
            | Action::AnswerRecordMove { .. }
            | Action::MoveRecordTo { .. }
            | Action::CancelRecordMove { .. } => self.dispatch_record_moves(action, actor),
            Action::ProposeGroup { .. }
            | Action::SetGroupPerson { .. }
            | Action::RemoveGroupOrgan { .. }
            | Action::GrantOrganLogin { .. }
            | Action::RevokeOrganLogin { .. }
            | Action::RosterCreateOrgan
            | Action::RosterSetKarmaExecution { .. }
            | Action::RosterRenameCell { .. }
            | Action::RosterEnrolToken
            | Action::SyncNow
            | Action::RosterStatus
            | Action::SetCellConfig { .. }
            | Action::RosterJoinOrgan { .. }
            | Action::RosterRevokeCell { .. }
            | Action::RootKeyExport { .. }
            | Action::RootKeyDetach { .. } => self.dispatch_organ_management(action, actor),
            Action::MailboxSavedStatus
            | Action::MailboxSetCopies { .. }
            | Action::MailboxRetrySaved { .. }
            | Action::MailboxStatus
            | Action::MailboxCarryFor { .. }
            | Action::MailboxStopCarrying { .. }
            | Action::MailboxPickupPoints
            | Action::MailboxAddPickup { .. }
            | Action::MailboxRemovePickup { .. }
            | Action::MailboxCollectNow
            | Action::MailboxRequests
            | Action::MailboxAnswerRequest { .. }
            | Action::MailboxIssueInvite { .. }
            | Action::MailboxAskCarry { .. }
            | Action::MailboxUseInvite { .. }
            | Action::MailboxOutbound
            | Action::MailboxMailNow { .. } => self.dispatch_mailboxes(action),
            Action::SetFileSyncEnabled { .. }
            | Action::ConfigureFileSync { .. }
            | Action::DirectorySync { .. }
            | Action::FileSyncStatus { .. } => self.dispatch_file_sync(action, actor, now),
            Action::Compensate { .. }
            | Action::AssertRecord { .. }
            | Action::RetractAssertion { .. }
            | Action::RefineAssertion { .. }
            | Action::RetractRecord { .. }
            | Action::SetIdentity { .. } => self.dispatch_record_mutations(action, actor, now),
        }
    }
}
