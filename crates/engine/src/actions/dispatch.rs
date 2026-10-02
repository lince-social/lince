use super::*;
use std::ops::ControlFlow;

mod part_0;
mod part_1;
mod part_2;
mod part_3;
mod part_4;
mod part_5;
mod part_6;
mod part_7;
mod part_8;

type DispatchOutcome = Result<ControlFlow<ActionOutcome, ActionOutcome>, EngineError>;

impl Engine {
    pub(super) fn dispatch_action_parts(
        &self,
        action: Action,
        actor: Option<String>,
        now: DateTime<Utc>,
        verified_authorship: Option<VerifiedActionAuthorship>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = DispatchOutcome> + Send + '_>> {
        match &action {
            Action::Social { .. }
            | Action::ChangeRecord { .. }
            | Action::CreateRecordWithTags { .. }
            | Action::CreateRecordDraft { .. }
            | Action::CreateCustomComponent { .. }
            | Action::CreateRecord { .. }
            | Action::PreviewAreaTransition { .. }
            | Action::ApplyAreaTransition { .. }
            | Action::SetQuantityExact { .. }
            | Action::PresentComponent { .. }
            | Action::ActivateFiote { .. }
            | Action::InspectFioteActivations { .. }
            | Action::ReportFioteChild { .. }
            | Action::DiscardTransferDraft { .. }
            | Action::SetQuantity { .. }
            | Action::TransitionRecord { .. }
            | Action::CaptureEntry { .. }
            | Action::CreateFrequency { .. }
            | Action::DeleteFrequency { .. }
            | Action::PreviewKarmaReading { .. }
            | Action::PreviewKarmaProposal { .. }
            | Action::InspectKarmaRuleHistory { .. }
            | Action::InspectTransferKarma { .. }
            | Action::PreviewKarmaHabit { .. }
            | Action::ImportKarmaHabit { .. }
            | Action::SaveKarmaSchedule { .. }
            | Action::InspectKarmaSchedules { .. }
            | Action::PreviewKarmaScheduleDates { .. }
            | Action::CancelKarmaSchedule { .. }
            | Action::RetryKarmaSchedule { .. }
            | Action::SaveKarmaRule { .. }
            | Action::ReviseKarmaField { .. }
            | Action::CreateRecurrence { .. }
            | Action::ReviseRecurrence { .. }
            | Action::DeleteRecurrence { .. }
            | Action::SetRecurrencePaused { .. }
            | Action::ApplyRecurrenceOccurrence { .. }
            | Action::SkipRecurrenceOccurrence { .. }
            | Action::UnskipRecurrenceOccurrence { .. }
            | Action::ReviseEntry { .. }
            | Action::VoidEntry { .. }
            | Action::ClassifyFact { .. }
            | Action::AddQuantityGroupExact { .. }
            | Action::AddQuantityExact { .. }
            | Action::AddQuantity { .. }
            | Action::Activate { .. }
            | Action::Deactivate { .. }
            | Action::DeleteRecord { .. }
            | Action::EditRecordText { .. }
            | Action::SetSlug { .. }
            | Action::SetUnit { .. } => {
                self.dispatch_part_0(action, actor, now, verified_authorship)
            }
            Action::SetExtension { .. }
            | Action::RenameOrganContact { .. }
            | Action::SetSyncPolicy { .. }
            | Action::SetContactAcceptScope { .. }
            | Action::DeleteConversation { .. }
            | Action::SendRecordCopy { .. }
            | Action::HideRecordFromContact { .. }
            | Action::SetContactScope { .. }
            | Action::SetContactShare { .. }
            | Action::MoveRecordTo { .. }
            | Action::CancelRecordMove { .. }
            | Action::ForgetOrganContact { .. }
            | Action::AddKnownOrgan { .. }
            | Action::ProposeGroup { .. }
            | Action::SetGroupPerson { .. }
            | Action::RemoveGroupOrgan { .. }
            | Action::StartConversation { .. }
            | Action::OpenThread { .. }
            | Action::GrantOrganLogin { .. }
            | Action::RevokeOrganLogin { .. }
            | Action::AcceptThreadInvite { .. }
            | Action::DeclineThreadInvite { .. }
            | Action::ShareMyKey { .. }
            | Action::RosterCreateOrgan
            | Action::RosterSetKarmaExecution { .. }
            | Action::RosterRenameCell { .. }
            | Action::RosterEnrolToken
            | Action::MailboxSavedStatus
            | Action::MailboxSetCopies { .. }
            | Action::MailboxRetrySaved { .. }
            | Action::MailboxStatus
            | Action::MailboxCarryFor { .. }
            | Action::MailboxStopCarrying { .. }
            | Action::MailboxPickupPoints
            | Action::MailboxAddPickup { .. }
            | Action::MailboxRemovePickup { .. }
            | Action::MailboxCollectNow
            | Action::MailboxRequests => {
                self.dispatch_part_1(action, actor, now, verified_authorship)
            }
            Action::MailboxAnswerRequest { .. }
            | Action::MailboxIssueInvite { .. }
            | Action::MailboxAskCarry { .. }
            | Action::MailboxUseInvite { .. }
            | Action::SetFileSyncEnabled { .. }
            | Action::ConfigureFileSync { .. }
            | Action::FileSyncStatus { .. }
            | Action::MailboxOutbound
            | Action::MailboxMailNow { .. }
            | Action::ReadMessageAttachment { .. }
            | Action::SyncNow
            | Action::RosterStatus
            | Action::SetCellConfig { .. }
            | Action::AuditContact { .. }
            | Action::RosterJoinOrgan { .. }
            | Action::RosterRevokeCell { .. }
            | Action::RootKeyExport { .. }
            | Action::RootKeyDetach { .. }
            | Action::SetContactTrust { .. }
            | Action::SetContactProximity { .. }
            | Action::Compensate { .. }
            | Action::CreateLingua { .. }
            | Action::RenameLingua { .. }
            | Action::DeleteLingua { .. }
            | Action::CreateConcept { .. }
            | Action::RenameConcept { .. }
            | Action::DeleteConcept { .. }
            | Action::AdoptConcept { .. }
            | Action::RemoveConceptFromLingua { .. }
            | Action::AddConceptParent { .. }
            | Action::RemoveConceptParent { .. }
            | Action::AssertRecord { .. }
            | Action::ImportInstinct
            | Action::ConfigureFiote { .. }
            | Action::CreateAgent { .. }
            | Action::RetractAssertion { .. }
            | Action::RefineAssertion { .. }
            | Action::RetractRecord { .. }
            | Action::SetIdentity { .. } => {
                self.dispatch_part_2(action, actor, now, verified_authorship)
            }
            Action::SetAssertionOrder { .. }
            | Action::CreateThread { .. }
            | Action::CreateMessage { .. }
            | Action::ReviseMessage { .. }
            | Action::CreateMessageDraft { .. }
            | Action::ReviseMessageDraft { .. }
            | Action::DeleteMessageDraft { .. }
            | Action::SendMessageDraft { .. }
            | Action::CreateTransferThread { .. }
            | Action::CreateTransferMessage { .. }
            | Action::CreatePromise { .. }
            | Action::PromiseTransition { .. }
            | Action::EditPromiseDelta { .. }
            | Action::CreateTransfer { .. } => {
                self.dispatch_part_3(action, actor, now, verified_authorship)
            }
            Action::CreateTransferDraft { .. }
            | Action::ReviseTransferPromise { .. }
            | Action::ReviseTransferDraft { .. }
            | Action::AdoptTransferDraft { .. }
            | Action::AddressTransferInvitation { .. }
            | Action::AcceptTransferInvitation { .. }
            | Action::RejectTransferInvitation { .. }
            | Action::WithdrawTransferInvitation { .. } => {
                self.dispatch_part_4(action, actor, now, verified_authorship)
            }
            Action::ReopenTransferInvitation { .. }
            | Action::CounterofferTransfer { .. }
            | Action::ClaimOpenTransferPromise { .. }
            | Action::SetTransferAgreementLevel { .. }
            | Action::AssignTransferAgreementLevel { .. }
            | Action::PublishTransfer { .. }
            | Action::ActivateTransferFulfillment { .. }
            | Action::ActivateTransferOccurrence { .. }
            | Action::SetTransferOccurrenceClaim { .. }
            | Action::CompleteTransferOccurrenceClaimsBulk { .. }
            | Action::SetTransferOccurrenceDispute { .. }
            | Action::SetTransferChildRequirement { .. }
            | Action::ProposeTransferLoanExtension { .. }
            | Action::ProposeTransferCancellation { .. }
            | Action::ApplyTransferCancellation { .. }
            | Action::SetRecordStockLimit { .. } => {
                self.dispatch_part_5(action, actor, now, verified_authorship)
            }
            Action::CompensateTransferApplication { .. }
            | Action::SetTransferPrivateApplicationPolicy { .. }
            | Action::SetTransferOccurrenceApplicationFormula { .. }
            | Action::SettleTransferOccurrence { .. }
            | Action::ConfigureTransferDelivery { .. }
            | Action::SetTransferDeliveryMode { .. }
            | Action::RevokeTransferDelivery { .. }
            | Action::EnqueueTransferDelivery { .. }
            | Action::RetryTransferDelivery { .. }
            | Action::RefreshTransferDelivery { .. }
            | Action::BeginTransferSettlement { .. }
            | Action::ApplyTransferApplication { .. }
            | Action::ConfirmTransfer { .. } => {
                self.dispatch_part_6(action, actor, now, verified_authorship)
            }
            Action::AddParty { .. }
            | Action::AddPromiseToTransfer { .. }
            | Action::AgreeTransfer { .. }
            | Action::ActivateTransfer { .. }
            | Action::SettleTransfer { .. }
            | Action::SetPlace { .. }
            | Action::GrantVisibility { .. }
            | Action::SaveProtein { .. }
            | Action::CreateKarmaProgram { .. }
            | Action::ReviseKarmaProgram { .. }
            | Action::ActivateKarmaProgram { .. }
            | Action::PauseKarmaProgram { .. }
            | Action::SetKarmaExecution { .. }
            | Action::DesignateKarmaExecutor { .. }
            | Action::DesignateTransferExecutor { .. }
            | Action::RespondKarmaCandidate { .. }
            | Action::SaveKarmaFrequency { .. }
            | Action::CreateKarmaFrequency { .. }
            | Action::ReviseKarmaFrequency { .. }
            | Action::ActivateKarmaFrequency { .. }
            | Action::SetKarmaFrequencyParameters { .. }
            | Action::ResetKarmaFrequencyParameters { .. }
            | Action::PauseKarmaFrequency { .. }
            | Action::Decide { .. }
            | Action::SaveKarmaCommand { .. }
            | Action::RunKarmaCommand { .. }
            | Action::InspectKarmaCommands
            | Action::CreateSignal { .. }
            | Action::CreateMatchRule { .. }
            | Action::AdoptConcepts { .. } => {
                self.dispatch_part_7(action, actor, now, verified_authorship)
            }
            Action::CreateTransferRemainderDraft { .. }
            | Action::CreateReversingTransferDraft { .. }
            | Action::ReopenTransferPromise { .. }
            | Action::CompensateTransferOccurrenceSettlement { .. }
            | Action::DeclareEquivalence { .. }
            | Action::RenameRole { .. }
            | Action::DeleteRole { .. }
            | Action::CreateRole { .. }
            | Action::CreateUser { .. }
            | Action::UpdateUser { .. }
            | Action::DeleteUser { .. }
            | Action::AssignRole { .. }
            | Action::SetPersonReadFilter { .. }
            | Action::SetRoleReadRules { .. }
            | Action::SetPersonStanding { .. }
            | Action::GrantPermission { .. }
            | Action::RevokePermission { .. } => {
                self.dispatch_part_8(action, actor, now, verified_authorship)
            }
        }
    }
}
