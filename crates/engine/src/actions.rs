mod dispatch;

use chrono::{DateTime, Utc};
use nucleus::karma::{CanonicalHash, FrequencyAst, FrequencyParameterValue, LocalId, ProgramAst};
use nucleus::{
    Cause, CauseKind, Fact, MessageDraftTiming, MessageState, NewFact, PromiseState, RecordKind,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

use crate::Engine;
use crate::error::EngineError;

fn default_lingua_visibility() -> String {
    "private".into()
}

fn validate_saved_protein_shape(ast: &serde_json::Value) -> Result<(), EngineError> {
    let Some(predicates) = ast.get("where") else {
        return Ok(());
    };
    let Some(predicates) = predicates.as_array() else {
        return Err(EngineError::Consequence(
            "invalid Protein: `where` must be an array".into(),
        ));
    };
    if predicates.is_empty() {
        return Ok(());
    }
    let root = predicates
        .first()
        .and_then(serde_json::Value::as_object)
        .filter(|root| predicates.len() == 1 && root.len() == 1);
    if !root.is_some_and(|root| root.contains_key("all") || root.contains_key("any")) {
        return Err(EngineError::Consequence(
            "invalid Protein: saved filters must have one root `all` or `any` group".into(),
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "action", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Action {
    Social {
        request: nucleus::social::Command,
    },
    ChangeRecord {
        request: crate::record_change::Request,
    },
    CreateRecord {
        slug: Option<String>,
        kind: RecordKind,
        head: String,
        #[serde(default)]
        body: String,
        #[serde(default)]
        quantity: f64,
    },
    CreateRecordDraft {
        draft: crate::record_creation::Draft,
    },
    CreateCustomComponent {
        head: String,
        body: String,
    },
    SetQuantity {
        target: String,
        value: f64,
    },
    SetQuantityExact {
        target: String,
        amount: String,
    },
    PreviewAreaTransition {
        target: String,
        changes: crate::area_transition::RecordChanges,
        #[serde(default)]
        constraints: crate::area_transition::RecordChanges,
    },
    ApplyAreaTransition {
        request_id: String,
        preview: crate::area_transition::TransitionPreview,
    },
    TransitionRecord {
        subject: String,
        #[serde(default)]
        retract: Vec<String>,
        #[serde(default)]
        assert: Vec<String>,
        #[serde(default)]
        quantity: Option<f64>,
    },
    AddQuantityExact {
        target: String,
        delta: nucleus::DecimalValue,
    },
    AddQuantityGroupExact {
        changes: std::collections::BTreeMap<String, nucleus::DecimalValue>,
    },
    AddQuantity {
        target: String,
        delta: f64,
    },
    CaptureEntry {
        target: String,
        amount: String,
        concept: Option<String>,
        #[serde(default)]
        note: Option<String>,
        #[serde(default)]
        at: Option<String>,
        #[serde(default)]
        request_id: Option<String>,
    },
    ReviseEntry {
        entry: String,
        expected_revision: i64,
        request_id: String,
        amount: String,
        #[serde(default)]
        note: Option<String>,
        #[serde(default)]
        at: Option<String>,
    },
    VoidEntry {
        entry: String,
        expected_revision: i64,
        request_id: String,
    },
    ClassifyFact {
        fact: String,
        concept: Option<String>,
        #[serde(default)]
        note: Option<String>,
    },
    CreateFrequency {
        slug: String,
        #[serde(default)]
        head: Option<String>,
        every: nucleus::karma::CadenceStep,
        #[serde(default)]
        anchor_at: Option<String>,
        #[serde(default)]
        request_id: Option<String>,
    },
    DeleteFrequency {
        frequency: String,
    },
    PreviewKarmaReading {
        source: String,
    },
    PreviewKarmaProposal {
        request: crate::karma_preview::Request,
    },
    InspectKarmaRuleHistory {
        rule: String,
        limit: u32,
    },
    InspectTransferKarma {
        transfer: String,
        #[serde(default)]
        person: Option<String>,
    },
    PreviewKarmaHabit {
        input: crate::karma_habits::Input,
    },
    ImportKarmaHabit {
        input: crate::karma_habits::Input,
        expected_preview: String,
        request_id: String,
    },
    SaveKarmaSchedule {
        schedule: Option<String>,
        expected_revision: Option<i64>,
        name: String,
        boundaries: Vec<nucleus::karma::scheduled_change::BoundaryInput>,
        request_id: String,
    },
    InspectKarmaSchedules {
        schedule: Option<String>,
    },
    PreviewKarmaScheduleDates {
        date: nucleus::karma::CivilDateTime,
        timezone: nucleus::karma::TimeZoneId,
        gap: nucleus::karma::GapPolicy,
        fold: nucleus::karma::FoldPolicy,
    },
    CancelKarmaSchedule {
        schedule: String,
        expected_revision: i64,
        request_id: String,
    },
    RetryKarmaSchedule {
        schedule: String,
        expected_revision: i64,
        boundary: String,
        request_id: String,
    },
    SaveKarmaRule {
        #[serde(default)]
        identity: Option<nucleus::karma::rule_field::RuleIdentity>,
        rule: Option<String>,
        expected_revision: Option<i64>,
        fields: [nucleus::karma::rule_field::RuleFieldInput; 3],
        request_id: String,
    },
    PresentComponent {
        target: String,
        component: nucleus::component::ComponentState,
    },
    ActivateFiote {
        target: String,
        value: String,
        request_id: String,
    },
    InspectFioteActivations { target: String },
    ReportFioteChild { parent: String, thread: String, task: String, state: String },
    DiscardTransferDraft {
        transfer: String,
        person: Option<String>,
        expected_revision: u64,
        request_id: String,
    },
    ReviseKarmaField {
        field: String,
        expected_revision: i64,
        source: String,
        request_id: String,
    },
    CreateRecurrence {
        target: String,
        consequences: Vec<nucleus::karma::Consequence>,
        #[serde(default)]
        condition: Option<String>,
        #[serde(default)]
        gate: Option<String>,
        #[serde(default)]
        carry: Option<String>,
        #[serde(default)]
        note: Option<String>,
        cadence: nucleus::karma::Cadence,
        #[serde(default)]
        anchor_at: Option<String>,
        #[serde(default)]
        request_id: Option<String>,
    },
    ReviseRecurrence {
        recurrence: String,
        expected_revision: i64,
        request_id: String,
        consequences: Vec<nucleus::karma::Consequence>,
        #[serde(default)]
        condition: Option<String>,
        #[serde(default)]
        gate: Option<String>,
        #[serde(default)]
        carry: Option<String>,
        #[serde(default)]
        note: Option<String>,
        cadence: nucleus::karma::Cadence,
        #[serde(default)]
        anchor_at: Option<String>,
    },
    SetRecurrencePaused {
        recurrence: String,
        expected_revision: i64,
        request_id: String,
        paused: bool,
    },
    DeleteRecurrence {
        recurrence: String,
    },
    ApplyRecurrenceOccurrence {
        recurrence: String,
        due_at: String,
        #[serde(default)]
        amount: Option<String>,
        #[serde(default)]
        note: Option<String>,
    },
    SkipRecurrenceOccurrence {
        recurrence: String,
        due_at: String,
        #[serde(default)]
        note: Option<String>,
    },
    UnskipRecurrenceOccurrence {
        recurrence: String,
        due_at: String,
    },
    Activate {
        target: String,
    },
    Deactivate {
        target: String,
    },
    DeleteRecord {
        target: String,
    },
    EditRecordText {
        target: String,
        #[serde(default)]
        head: Option<String>,
        #[serde(default)]
        body: Option<String>,
    },
    SetSlug {
        target: String,
        slug: Option<String>,
    },
    SetUnit {
        target: String,
        unit: Option<String>,
    },
    SetExtension {
        target: String,
        namespace: String,
        fds: serde_json::Value,
    },
    AddKnownOrgan {
        invite: String,
        name: String,
    },
    RenameOrganContact {
        target: String,
        name: String,
    },
    SetSyncPolicy {
        target: String,
        sync_out: bool,
        sync_in: bool,
    },
    SetContactScope {
        target: String,
        fields: Option<Vec<String>>,
    },
    SetContactShare {
        target: String,
        protein: Option<serde_json::Value>,
    },
    MoveRecordTo {
        record: String,
        target: String,
    },
    CancelRecordMove {
        record: String,
    },
    SetContactAcceptScope {
        target: String,
        fields: Option<Vec<String>>,
    },
    DeleteConversation {
        conversation: String,
    },
    SendRecordCopy {
        thread: String,
        record: String,
    },
    HideRecordFromContact {
        target: String,
        record: String,
        hidden: bool,
    },
    ForgetOrganContact {
        target: String,
    },
    ShareMyKey {
        thread: String,
    },
    StartConversation {
        contact: String,
        title: String,
    },
    ProposeGroup {
        thread: String,
        title: String,
        organs: Vec<String>,
    },
    SetGroupPerson {
        root: String,
        person: String,
        allowed: bool,
    },
    RemoveGroupOrgan {
        root: String,
        organ: String,
    },
    OpenThread {
        conversation: String,
        title: String,
    },
    GrantOrganLogin {
        organ: String,
        person_name: String,
    },
    RevokeOrganLogin {
        organ: String,
    },
    AcceptThreadInvite {
        invite: String,
    },
    DeclineThreadInvite {
        invite: String,
    },
    RosterEnrolToken,
    RosterCreateOrgan,
    RosterSetKarmaExecution { cell_uid: String, enabled: bool, #[serde(default)] additional: bool, expected_roster_version: i64 },
    RosterRenameCell {
        cell_uid: String,
        label: String,
    },
    RosterRevokeCell {
        cell_uid: String,
    },
    RosterStatus,
    SyncNow,
    MailboxStatus,
    MailboxSavedStatus,
    MailboxRetrySaved { uid: String },
    MailboxSetCopies { copies: u8 },
    MailboxCarryFor {
        organ_uid: String,
        #[serde(default)]
        label: String,
        #[serde(default)]
        quota_bytes: i64,
    },
    MailboxStopCarrying {
        organ_uid: String,
    },
    MailboxPickupPoints,
    MailboxAddPickup {
        organ_uid: String,
        #[serde(default)]
        node_id: String,
        #[serde(default)]
        label: String,
    },
    MailboxRemovePickup {
        organ_uid: String,
    },
    MailboxCollectNow,
    MailboxOutbound,
    SetFileSyncEnabled {
        enabled: bool,
    },
    ConfigureFileSync {
        protein: String,
        path: String,
        format: crate::file_sync::FileFormat,
        enabled: bool,
    },
    FileSyncStatus {
        organ: String,
    },
    MailboxRequests,
    MailboxAnswerRequest {
        organ_uid: String,
        accept: bool,
        #[serde(default)]
        quota_bytes: i64,
    },
    MailboxIssueInvite {
        #[serde(default)]
        label: String,
        #[serde(default)]
        quota_bytes: i64,
    },
    MailboxAskCarry {
        organ_uid: String,
    },
    MailboxUseInvite {
        code: String,
    },
    MailboxMailNow {
        organ_uid: String,
    },
    SetCellConfig {
        namespace: String,
        fds: serde_json::Value,
    },
    AuditContact {
        contact: String,
    },
    RosterJoinOrgan {
        code: String,
    },
    RootKeyExport {
        destination: String,
    },
    RootKeyDetach {
        copy_at: String,
    },
    SetContactTrust {
        target: String,
        trust: String,
    },
    SetContactProximity {
        target: String,
        proximity: u32,
    },
    Compensate {
        fact: String,
    },
    SetTransferChildRequirement {
        transfer: String,
        child: String,
        required: bool,
        expected_revision: u64,
        request_id: String,
        person: Option<String>,
    },
    ProposeTransferCancellation {
        transfer: String,
        occurrence: String,
        expected_revision: u64,
        expected_remaining_quantity: nucleus::DecimalValue,
        request_id: String,
        #[serde(default)]
        person: Option<String>,
    },
    ProposeTransferLoanExtension {
        transfer: String,
        exchange: String,
        until: String,
        expected_revision: u64,
        request_id: String,
        person: String,
    },
    ApplyTransferCancellation {
        transfer: String,
        cancellation: String,
        expected_revision: u64,
        request_id: String,
        #[serde(default)]
        person: Option<String>,
    },
    SetRecordStockLimit {
        record: String,
        person: String,
        minimum: Option<nucleus::DecimalValue>,
        expected_version: u64,
        request_id: String,
    },
    CompensateTransferApplication {
        application: String,
        request_id: String,
        person: String,
    },
    CompensateTransferOccurrenceSettlement {
        settlement: String,
        request_id: String,
        #[serde(default)]
        person: Option<String>,
    },
    CreateTransferRemainderDraft {
        occurrence: String,
        expected_revision: u64,
        expected_remaining_quantity: f64,
        request_id: String,
        #[serde(default)]
        person: Option<String>,
    },
    CreateReversingTransferDraft {
        occurrence: String,
        expected_revision: u64,
        canonical_quantity: f64,
        request_id: String,
        #[serde(default)]
        person: Option<String>,
    },
    ReopenTransferPromise {
        transfer: String,
        promise: String,
        expected_revision: u64,
        request_id: String,
        #[serde(default)]
        person: Option<String>,
        window_end: Option<String>,
        #[serde(default)]
        open: bool,
    },
    CreateLingua {
        name: String,
        #[serde(default = "default_lingua_visibility")]
        visibility: String,
    },
    RenameLingua {
        lingua: String,
        name: String,
    },
    DeleteLingua {
        lingua: String,
    },
    CreateConcept {
        lingua: String,
        name: String,
        #[serde(default)]
        parents: Vec<String>,
    },
    RenameConcept {
        concept: String,
        name: String,
    },
    DeleteConcept {
        concept: String,
    },
    AdoptConcept {
        lingua: String,
        concept: String,
    },
    RemoveConceptFromLingua {
        lingua: String,
        concept: String,
    },
    AddConceptParent {
        concept: String,
        parent: String,
    },
    RemoveConceptParent {
        concept: String,
        parent: String,
    },
    AssertRecord {
        subject: String,
        predicate: String,
        #[serde(default)]
        object: Option<String>,
        #[serde(default)]
        quantity: Option<String>,
        #[serde(default)]
        unit: Option<String>,
    },
    RetractAssertion {
        assertion: String,
    },
    ImportInstinct,
    ConfigureFiote {
        target: String,
        prompt_parent: Option<String>,
        run_assigned: bool,
    },
    CreateAgent {
        head: String,
        #[serde(default)]
        operated_by: Option<String>,
    },
    RefineAssertion {
        subject: String,
        predicate: String,
        object: String,
    },
    RetractRecord {
        subject: String,
        predicate: String,
        #[serde(default)]
        object: Option<String>,
    },
    SetIdentity {
        subject: String,
        #[serde(default)]
        predicate: Option<String>,
    },
    SetAssertionOrder {
        predicate: String,
        ordered: Vec<String>,
        #[serde(default)]
        reverse: bool,
    },
    CreateThread {
        target: String,
        head: String,
    },
    CreateMessage {
        thread: String,
        body: String,
        #[serde(default)]
        content: Vec<nucleus::message::MessagePart>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        author: Option<String>,
        #[serde(default, skip_serializing_if = "MessageState::is_finished")]
        state: MessageState,
        #[serde(default)]
        parent: Option<String>,
        #[serde(default)]
        references: Vec<String>,
    },
    ReviseMessage {
        message: String,
        body: String,
        state: MessageState,
    },
    ReadMessageAttachment {
        message: String,
        index: usize,
    },
    CreateMessageDraft {
        conversation: String,
        thread: String,
        #[serde(default)]
        body: String,
        #[serde(default)]
        pinned: bool,
        #[serde(default)]
        timing: MessageDraftTiming,
        #[serde(default)]
        position: u32,
    },
    ReviseMessageDraft {
        draft: String,
        body: String,
        pinned: bool,
        timing: MessageDraftTiming,
        position: u32,
    },
    DeleteMessageDraft {
        draft: String,
    },
    SendMessageDraft {
        draft: String,
    },
    CreateTransferThread {
        transfer: String,
        head: String,
        request_id: String,
        person: String,
    },
    CreateTransferMessage {
        transfer: String,
        thread: String,
        body: String,
        #[serde(default)]
        parent: Option<String>,
        #[serde(default)]
        references: Vec<String>,
        request_id: String,
        person: String,
    },
    CreatePromise {
        record: String,
        delta: f64,
        window_end: Option<String>,
        party: Option<String>,
        #[serde(default)]
        open: bool,
    },
    PromiseTransition {
        promise: String,
        to: PromiseState,
    },
    EditPromiseDelta {
        promise: String,
        delta: f64,
    },
    Decide {
        decision: String,
        answer: String,
    },
    CreateTransfer {
        slug: Option<String>,
        head: String,
        #[serde(default = "default_agreement")]
        agreement: String,
        agreement_pct: Option<i64>,
        satiation: Option<String>,
        source: Option<String>,
        #[serde(default)]
        reserve_default: Option<String>,
        #[serde(default)]
        require_confirmation: bool,
    },
    CreateTransferDraft {
        request_id: String,
        #[serde(default)]
        creator: Option<String>,
        slug: Option<String>,
        head: String,
        #[serde(default = "default_typed_agreement")]
        agreement: nucleus::transfer::AgreementType,
        agreement_pct: Option<u8>,
        #[serde(default)]
        satiation: TransferSatiation,
        parent: Option<String>,
        source: Option<String>,
        #[serde(default)]
        visibility: TransferVisibility,
        max_proximity: Option<u32>,
        #[serde(default)]
        reserve_default: TransferReservePoint,
        #[serde(default)]
        require_confirmation: bool,
        #[serde(default)]
        default_place: Option<TransferPlaceInput>,
        #[serde(default)]
        invitees: Vec<String>,
        #[serde(default)]
        promises: Vec<TransferPromiseInput>,
        #[serde(default)]
        dependencies: Vec<TransferDependencyInput>,
    },
    ReviseTransferPromise {
        transfer: String,
        promise: String,
        expected_revision: u64,
        request_id: String,
        terms: TransferPromiseInput,
    },
    ReviseTransferDraft {
        transfer: String,
        expected_revision: u64,
        request_id: String,
        draft: TransferDraftRevisionInput,
    },
    AdoptTransferDraft {
        transfer: String,
        request_id: String,
        draft: TransferDraftRevisionInput,
    },
    AddressTransferInvitation {
        transfer: String,
        expected_revision: u64,
        request_id: String,
        person: String,
        #[serde(default)]
        expires_at: Option<String>,
    },
    AcceptTransferInvitation {
        invitation: String,
        expected_revision: u64,
        request_id: String,
        #[serde(default)]
        transfer: Option<String>,
        #[serde(default)]
        person: Option<String>,
    },
    RejectTransferInvitation {
        invitation: String,
        request_id: String,
        #[serde(default)]
        transfer: Option<String>,
        #[serde(default)]
        person: Option<String>,
    },
    WithdrawTransferInvitation {
        invitation: String,
        expected_revision: u64,
        request_id: String,
    },
    ReopenTransferInvitation {
        invitation: String,
        expected_revision: u64,
        request_id: String,
        #[serde(default)]
        expires_at: Option<String>,
    },
    CounterofferTransfer {
        transfer: String,
        expected_revision: u64,
        request_id: String,
        #[serde(default)]
        person: Option<String>,
        draft: TransferDraftRevisionInput,
    },
    ClaimOpenTransferPromise {
        transfer: String,
        promise: String,
        expected_revision: u64,
        request_id: String,
        #[serde(default)]
        person: Option<String>,
        terms: TransferPromiseInput,
    },
    SetTransferAgreementLevel {
        transfer: String,
        expected_revision: u64,
        request_id: String,
        #[serde(default)]
        person: Option<String>,
        level: u8,
    },
    AssignTransferAgreementLevel {
        transfer: String,
        expected_revision: u64,
        request_id: String,
        #[serde(default)]
        person: Option<String>,
        level: u8,
        #[serde(default)]
        expected: Option<nucleus::transfer::AgreementGuard>,
        #[serde(default)]
        expected_state: Option<nucleus::transfer::karma::Guard>,
    },
    PublishTransfer {
        transfer: String,
        expected_revision: u64,
        request_id: String,
        #[serde(default)]
        person: Option<String>,
        #[serde(default)]
        expected: Option<nucleus::transfer::AgreementGuard>,
        #[serde(default)]
        expected_state: Option<nucleus::transfer::karma::Guard>,
    },
    ActivateTransferFulfillment {
        transfer: String,
        promise: String,
        fulfillment: String,
        expected_revision: u64,
        request_id: String,
        #[serde(default)]
        person: Option<String>,
        #[serde(default)]
        expected: Option<nucleus::transfer::AgreementGuard>,
        #[serde(default)]
        expected_state: Option<nucleus::transfer::karma::Guard>,
    },
    ActivateTransferOccurrence {
        transfer: String,
        promise: String,
        expected_revision: u64,
        request_id: String,
        #[serde(default)]
        person: Option<String>,
    },
    SetTransferOccurrenceClaim {
        occurrence: String,
        request_id: String,
        #[serde(default)]
        person: Option<String>,
        role: TransferOccurrenceClaimRole,
        claimed: bool,
    },
    CompleteTransferOccurrenceClaimsBulk {
        request_id: String,
        #[serde(default)]
        person: Option<String>,
        review_token: String,
        items: Vec<TransferOccurrenceBulkClaimInput>,
    },
    SetTransferOccurrenceDispute {
        occurrence: String,
        request_id: String,
        #[serde(default)]
        person: Option<String>,
        disputed: bool,
    },
    SetTransferPrivateApplicationPolicy {
        transfer: String,
        exchange: String,
        effects: Vec<nucleus::transfer::application::PrivateEffect>,
        expected_version: u64,
        request_id: String,
        person: String,
    },
    SetTransferOccurrenceApplicationFormula {
        occurrence: String,
        request_id: String,
        #[serde(default)]
        person: Option<String>,
        formula: String,
    },
    SettleTransferOccurrence {
        #[serde(default)]
        expected_effects_hash: Option<String>,
        occurrence: String,
        request_id: String,
        #[serde(default)]
        person: Option<String>,
        canonical_quantity: f64,
        expected_remaining_quantity: f64,
        expected_local_delta: f64,
        expected_application_formula_hash: String,
        expected_application_formula_version: u64,
        expected_remainder_policy: nucleus::transfer::TransferRemainderPolicy,
    },
    ConfigureTransferDelivery {
        transfer: String,
        recipient_person: String,
        recipient_organ: String,
        #[serde(default)]
        person: Option<String>,
        request_id: String,
        #[serde(default = "default_transfer_delivery_mode")]
        mode: nucleus::transfer_delivery::TransferDeliveryMode,
    },
    SetTransferDeliveryMode {
        transfer: String,
        delivery: String,
        expected_revision: u64,
        #[serde(default)]
        person: Option<String>,
        request_id: String,
        mode: nucleus::transfer_delivery::TransferDeliveryMode,
    },
    EnqueueTransferDelivery {
        transfer: String,
        delivery: String,
        #[serde(default)]
        person: Option<String>,
        request_id: String,
    },
    RetryTransferDelivery {
        transfer: String,
        delivery: String,
        #[serde(default)]
        person: Option<String>,
        request_id: String,
    },
    RevokeTransferDelivery {
        transfer: String,
        delivery: String,
        expected_revision: u64,
        #[serde(default)]
        person: Option<String>,
        request_id: String,
    },
    RefreshTransferDelivery {
        transfer: String,
        delivery: String,
        #[serde(default)]
        person: Option<String>,
        request_id: String,
    },
    BeginTransferSettlement {
        transfer: String,
        occurrence: String,
        expected_revision: u64,
        expected_remaining_quantity: f64,
        canonical_quantity: f64,
        request_id: String,
        person: String,
    },
    ApplyTransferApplication {
        #[serde(default)]
        expected_effects_hash: Option<String>,
        transfer: String,
        handoff: String,
        local_record: String,
        expected_formula_hash: String,
        expected_formula_version: u64,
        expected_local_delta: f64,
        expected_local_cumulative_before: f64,
        request_id: String,
        person: String,
    },
    ConfirmTransfer {
        transfer: String,
        confirmation: String,
    },
    AddParty {
        transfer: String,
        actor: String,
    },
    AddPromiseToTransfer {
        transfer: String,
        record: String,
        delta: f64,
        party: String,
        window_end: Option<String>,
        condition: Option<String>,
    },
    AgreeTransfer {
        transfer: String,
        party: String,
        level: i64,
    },
    ActivateTransfer {
        transfer: String,
    },
    SettleTransfer {
        transfer: String,
        actor: String,
    },
    SetPlace {
        target: String,
        lat: f64,
        lon: f64,
        address: Option<String>,
    },
    GrantVisibility {
        subject_kind: String,
        subject: Option<String>,
        target: String,
    },
    SaveProtein {
        slug: String,
        head: String,
        ast: serde_json::Value,
    },
    CreateKarmaProgram {
        request_id: String,
        program: ProgramAst,
        #[serde(default)]
        owner_person_uid: Option<String>,
    },
    ReviseKarmaProgram {
        request_id: String,
        program_uid: String,
        expected_handle_revision: u64,
        program: ProgramAst,
    },
    ActivateKarmaProgram {
        request_id: String,
        program_uid: String,
        expected_handle_revision: u64,
        revision_hash: CanonicalHash,
    },
    PauseKarmaProgram {
        request_id: String,
        program_uid: String,
        expected_handle_revision: u64,
    },
    SetKarmaExecution {
        program_uid: String,
        executes: bool,
        #[serde(default)]
        note: Option<String>,
    },
    DesignateKarmaExecutor {
        target: String,
        #[serde(default)]
        cell_uid: Option<String>,
    },
    DesignateTransferExecutor {
        transfer_uid: String,
        #[serde(default)]
        cell_uid: Option<String>,
    },
    RespondKarmaCandidate {
        request_id: String,
        candidate_hash: CanonicalHash,
        expected_state_revision: u64,
        response: nucleus::karma::CandidateReviewAction,
    },
    CreateKarmaFrequency {
        request_id: String,
        frequency: FrequencyAst,
        #[serde(default)]
        owner_person_uid: Option<String>,
    },
    SaveKarmaFrequency {
        request_id: String,
        frequency_uid: Option<String>,
        expected_handle_revision: Option<u64>,
        frequency: FrequencyAst,
        #[serde(default)]
        restart: bool,
    },
    ReviseKarmaFrequency {
        request_id: String,
        frequency_uid: String,
        expected_handle_revision: u64,
        frequency: FrequencyAst,
    },
    ActivateKarmaFrequency {
        request_id: String,
        frequency_uid: String,
        expected_handle_revision: u64,
        revision_hash: CanonicalHash,
        #[serde(default)]
        parameter_overrides: BTreeMap<LocalId, FrequencyParameterValue>,
    },
    SetKarmaFrequencyParameters {
        request_id: String,
        frequency_uid: String,
        expected_handle_revision: u64,
        expected_active_revision_hash: CanonicalHash,
        parameter_overrides: BTreeMap<LocalId, FrequencyParameterValue>,
    },
    ResetKarmaFrequencyParameters {
        request_id: String,
        frequency_uid: String,
        expected_handle_revision: u64,
        expected_active_revision_hash: CanonicalHash,
    },
    PauseKarmaFrequency {
        request_id: String,
        frequency_uid: String,
        expected_handle_revision: u64,
    },
    SaveKarmaCommand {
        target: Option<String>,
        expected_revision: Option<i64>,
        slug: String,
        head: String,
        configuration: nucleus::command::Command,
        host: Option<String>,
    },
    RunKarmaCommand { command: String, request_id: String, #[serde(default)] numeric: bool },
    InspectKarmaCommands,
    CreateSignal {
        slug: String,
        head: String,
        source_kind: String,
        source: String,
        schedule: String,
    },
    CreateMatchRule {
        slug: String,
        head: String,
        #[serde(default)]
        watch_concept: Option<String>,
        #[serde(default = "default_proximity")]
        max_proximity: u32,
        #[serde(default)]
        min_confidence: f64,
        #[serde(default = "default_auto")]
        auto: String,
    },
    AdoptConcepts {
        concepts: Vec<ConceptSeed>,
    },
    DeclareEquivalence {
        a: String,
        b: String,
    },
    CreateRole {
        name: String,
    },
    RenameRole {
        role: i64,
        expected_revision: i64,
        name: String,
    },
    DeleteRole {
        role: i64,
        expected_revision: i64,
    },
    CreateUser {
        username: String,
        name: String,
        password: String,
        role: String,
    },
    UpdateUser {
        user: String,
        username: String,
        name: String,
        #[serde(default)]
        password: String,
    },
    DeleteUser {
        user: String,
    },
    AssignRole {
        user: String,
        role: String,
    },
    SetPersonStanding {
        person: String,
        active: bool,
        #[serde(default)]
        note: Option<String>,
    },
    SetPersonReadFilter {
        person: String,
        #[serde(default)]
        filter: Option<protein::Predicate>,
    },
    SetRoleReadRules {
        role: String,
        rules: protein::read_rules::ReadRules,
        expected_revision: i64,
    },
    CreateRecordWithTags {
        head: String,
        body: String,
        quantity: f64,
        tags: Vec<String>,
    },
    GrantPermission {
        role: String,
        permission: String,
    },
    RevokePermission {
        role: String,
        permission: String,
    },
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TransferSatiation {
    #[default]
    None,
    FirstCompletes,
}

impl TransferSatiation {
    fn as_option(self) -> Option<String> {
        match self {
            Self::None => None,
            Self::FirstCompletes => Some("first_completes".into()),
        }
    }
}

fn default_transfer_delivery_mode() -> nucleus::transfer_delivery::TransferDeliveryMode {
    nucleus::transfer_delivery::TransferDeliveryMode::Hosted
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TransferVisibility {
    #[default]
    Hidden,
    Public,
    Proximity,
}

impl TransferVisibility {
    fn as_str(self) -> &'static str {
        match self {
            Self::Hidden => "hidden",
            Self::Public => "public",
            Self::Proximity => "proximity",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TransferReservePoint {
    #[default]
    Inherit,
    None,
    Proposed,
    Agreed,
    Active,
}

impl TransferReservePoint {
    fn as_str(self) -> &'static str {
        match self {
            Self::Inherit => "inherit",
            Self::None => "none",
            Self::Proposed => "proposed",
            Self::Agreed => "agreed",
            Self::Active => "active",
        }
    }

    fn resolve(self, cell_default: Self) -> Self {
        if self == Self::Inherit {
            cell_default
        } else {
            self
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TransferPromiseInput {
    #[serde(default)]
    pub uid: Option<String>,
    #[serde(default)]
    pub item: Option<nucleus::transfer::disclosure::TransferItem>,
    #[serde(default)]
    pub record: String,
    #[serde(default)]
    pub party: Option<String>,
    #[serde(default)]
    pub open: bool,
    pub delta: f64,
    #[serde(default)]
    pub unit: Option<String>,
    #[serde(default)]
    pub window_start: Option<String>,
    #[serde(default)]
    pub window_end: Option<String>,
    #[serde(default)]
    pub place: Option<TransferPlaceInput>,
    #[serde(default)]
    pub condition: Option<String>,
    #[serde(default)]
    pub reserve_from: Option<TransferReservePoint>,
    #[serde(default)]
    pub reuse_policy: nucleus::transfer::OpenPromiseReusePolicy,
    #[serde(default)]
    pub withdrawn: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TransferDraftRevisionInput {
    pub creator: String,
    pub slug: Option<String>,
    pub head: String,
    #[serde(default = "default_typed_agreement")]
    pub agreement: nucleus::transfer::AgreementType,
    pub agreement_pct: Option<u8>,
    #[serde(default)]
    pub satiation: TransferSatiation,
    pub parent: Option<String>,
    pub source: Option<String>,
    #[serde(default)]
    pub visibility: TransferVisibility,
    pub max_proximity: Option<u32>,
    #[serde(default)]
    pub reserve_default: TransferReservePoint,
    #[serde(default)]
    pub require_confirmation: bool,
    #[serde(default)]
    pub default_place: Option<TransferPlaceInput>,
    #[serde(default)]
    pub invitees: Vec<String>,
    #[serde(default)]
    pub promises: Vec<TransferPromiseInput>,
    #[serde(default)]
    pub dependencies: Vec<TransferDependencyInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TransferDependencyInput {
    #[serde(default)]
    pub uid: Option<String>,
    pub scope: TransferDependencyScopeInput,
    #[serde(default)]
    pub promise: Option<String>,
    pub upstream_kind: TransferDependencyUpstreamKindInput,
    pub upstream: String,
    #[serde(default = "default_dependency_required_state")]
    pub required_state: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TransferDependencyScopeInput {
    Transfer,
    Promise,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TransferDependencyUpstreamKindInput {
    Transfer,
    Promise,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TransferOccurrenceClaimRole {
    Delivery,
    Receipt,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TransferOccurrenceBulkClaimInput {
    pub occurrence: String,
    pub transfer: String,
    pub expected_revision: u64,
    pub role: TransferOccurrenceClaimRole,
    pub expected_delivery_claimed: bool,
    pub expected_receipt_claimed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TransferPlaceInput {
    #[serde(default)]
    pub lat: Option<f64>,
    #[serde(default)]
    pub lon: Option<f64>,
    #[serde(default)]
    pub address: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ConceptSeed {
    pub uid: String,
    pub name: String,
    #[serde(default)]
    pub origin: Option<String>,
    #[serde(default)]
    pub parents: Vec<String>,
}

fn default_proximity() -> u32 {
    1
}

fn default_auto() -> String {
    "draft_only".into()
}

fn default_agreement() -> String {
    "individual".into()
}

fn default_typed_agreement() -> nucleus::transfer::AgreementType {
    nucleus::transfer::AgreementType::Individual
}

fn default_dependency_required_state() -> String {
    "kept".into()
}

fn transfer_phase_locked() -> bool {
    true
}

fn validate_transfer_application_formula(formula: &str) -> Result<String, EngineError> {
    nucleus::transfer::application::validate(formula)
        .map_err(|error| EngineError::Consequence(format!("invalid application formula: {error}")))
}

fn evaluate_transfer_application_formula(formula: &str, incoming: f64) -> Result<f64, EngineError> {
    nucleus::transfer::application::evaluate(formula, incoming)
        .map_err(|error| EngineError::Consequence(format!("invalid application formula: {error}")))
}

fn normalize_transfer_place(
    place: Option<TransferPlaceInput>,
) -> Result<Option<nucleus::transfer::TransferLocationSnapshot>, EngineError> {
    let Some(place) = place else {
        return Ok(None);
    };
    let address = place
        .address
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    match (place.lat, place.lon) {
        (Some(lat), Some(lon)) => {
            if !lat.is_finite()
                || !lon.is_finite()
                || !(-90.0..=90.0).contains(&lat)
                || !(-180.0..=180.0).contains(&lon)
            {
                return Err(EngineError::Consequence(
                    "transfer location coordinates are outside valid latitude/longitude ranges"
                        .into(),
                ));
            }
            Ok(Some(nucleus::transfer::TransferLocationSnapshot {
                lat: Some(lat),
                lon: Some(lon),
                address,
            }))
        }
        (None, None) if address.is_some() => {
            Ok(Some(nucleus::transfer::TransferLocationSnapshot {
                lat: None,
                lon: None,
                address,
            }))
        }
        (None, None) => Err(EngineError::Consequence(
            "transfer location requires an address or coordinates".into(),
        )),
        _ => Err(EngineError::Consequence(
            "transfer location latitude and longitude must be provided together".into(),
        )),
    }
}

fn normalize_transfer_window(
    window_start: Option<String>,
    window_end: Option<String>,
    now: DateTime<Utc>,
    preserved_window_end: Option<&str>,
) -> Result<(Option<String>, Option<String>), EngineError> {
    let normalize = |value: Option<String>| {
        value
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
    };
    let window_start = normalize(window_start);
    let window_end = normalize(window_end);
    let start = window_start
        .as_deref()
        .map(DateTime::parse_from_rfc3339)
        .transpose()
        .map_err(|_| EngineError::Consequence("promise window_start must be RFC3339".into()))?;
    let end = window_end
        .as_deref()
        .map(DateTime::parse_from_rfc3339)
        .transpose()
        .map_err(|_| EngineError::Consequence("promise window_end must be RFC3339".into()))?;
    let preserves_existing_end = end.is_some_and(|submitted| {
        preserved_window_end
            .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
            .is_some_and(|existing| existing == submitted)
    });
    if end.is_some_and(|value| value.with_timezone(&Utc) <= now) && !preserves_existing_end {
        return Err(EngineError::Consequence(
            "promise window must end in the future".into(),
        ));
    }
    if start.zip(end).is_some_and(|(start, end)| start >= end) {
        return Err(EngineError::Consequence(
            "promise window_start must be before window_end".into(),
        ));
    }
    Ok((window_start, window_end))
}

fn normalize_transfer_invitation_expiry(
    expires_at: Option<String>,
    now: DateTime<Utc>,
) -> Result<Option<DateTime<Utc>>, EngineError> {
    let Some(value) = expires_at
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    let parsed = DateTime::parse_from_rfc3339(&value)
        .map_err(|_| {
            EngineError::Consequence(
                "transfer invitation expires_at must be an RFC3339 date and time".into(),
            )
        })?
        .with_timezone(&Utc);
    if parsed <= now {
        return Err(EngineError::Consequence(
            "transfer invitation expiry must be in the future".into(),
        ));
    }
    Ok(Some(parsed))
}

pub(crate) fn parse_instant_field(text: &str) -> Result<DateTime<Utc>, EngineError> {
    chrono::DateTime::parse_from_rfc3339(text)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| EngineError::Conflict {
            code: "entry_at_invalid",
            message: format!("`{text}` is not an RFC3339 instant"),
        })
}

struct GatheredReadings {
    demanded: Option<nucleus::expr::TokenKey>,
    values: std::collections::HashMap<String, nucleus::DecimalValue>,
}

impl nucleus::karma::ExactResolver for GatheredReadings {
    fn lookup(
        &mut self,
        func: &str,
        slug: &str,
        window_secs: Option<i64>,
    ) -> Result<nucleus::DecimalValue, nucleus::karma::ConditionError> {
        if let Some(value) = self.values.get(&reading_key(func, slug, window_secs)) {
            return Ok(*value);
        }
        self.demanded = Some(nucleus::expr::TokenKey { func: func.into(), slug: slug.into(), dur_secs: window_secs });
        Err(nucleus::karma::ConditionError::UnknownReference(slug.to_string()))
    }
}

const VALUE_DEPTH_CAP: usize = 4;

fn reading_key(func: &str, slug: &str, window_secs: Option<i64>) -> String {
    match window_secs {
        Some(secs) => format!("{func}:{slug}:{secs}"),
        None => format!("{func}:{slug}"),
    }
}

pub(crate) fn concept_tokens_in(source: &str) -> Vec<(usize, usize, String)> {
    let bytes = source.as_bytes();
    let mut found = Vec::new();
    let mut at = 0usize;
    while at < bytes.len() {
        if bytes[at] == b'"' {
            at += 1;
            let mut escaped = false;
            while at < bytes.len() {
                let character = bytes[at];
                at += 1;
                if character == b'"' && !escaped {
                    break;
                }
                escaped = character == b'\\' && !escaped;
            }
            continue;
        }
        if bytes[at] != b'#' {
            at += 1;
            continue;
        }
        let start = at;
        let mut end = at + 1;
        while end < bytes.len()
            && (bytes[end].is_ascii_alphanumeric()
                || bytes[end] == b'.'
                || bytes[end] == b'-'
                || bytes[end] == b'_'
                || bytes[end] == b'/')
        {
            end += 1;
        }
        if end > start + 1 {
            found.push((start, end, source[start + 1..end].to_string()));
        }
        at = end.max(start + 1);
    }
    found
}

fn parse_rule_condition(
    condition: Option<String>,
    gate: Option<String>,
    carry: Option<String>,
) -> Result<Option<store::recurrence::RuleCondition>, EngineError> {
    let Some(source) = condition
        .map(|c| c.trim().to_string())
        .filter(|c| !c.is_empty())
    else {
        if gate.is_some() || carry.is_some() {
            return Err(EngineError::Consequence(
                "a gate or carry needs a condition to act on".into(),
            ));
        }
        return Ok(None);
    };
    nucleus::karma::Condition::parse(&source)
        .map_err(|e| EngineError::Consequence(format!("that condition cannot be read: {e}")))?;
    let gate = nucleus::karma::Gate::parse(gate.as_deref().unwrap_or("!=0"))
        .map_err(|e| EngineError::Consequence(format!("that gate cannot be read: {e}")))?;
    let carry = nucleus::karma::Carry::parse(carry.as_deref().unwrap_or("value"))
        .map_err(|e| EngineError::Consequence(format!("that carry cannot be read: {e}")))?;
    Ok(Some(store::recurrence::RuleCondition {
        source,
        bindings: Vec::new(),
        gate,
        carry,
    }))
}

fn parse_optional_instant(text: Option<&str>) -> Result<Option<DateTime<Utc>>, EngineError> {
    text.map(parse_instant_field).transpose()
}

fn transfer_request_id_conflict() -> EngineError {
    EngineError::Conflict {
        code: "transfer_request_id_conflict",
        message: "transfer request id belongs to another action or target".into(),
    }
}

fn transfer_settlement_conflict(code: &'static str, message: impl Into<String>) -> EngineError {
    EngineError::Conflict {
        code,
        message: message.into(),
    }
}

fn transfer_settlement_values_match(left: f64, right: f64) -> bool {
    left.is_finite() && right.is_finite() && left == right
}

async fn reject_existing_transfer_revision_request(
    pool: &store::sqlx::SqlitePool,
    request_id: &str,
) -> Result<(), EngineError> {
    if store::transfers::revision_for_request(pool, request_id)
        .await?
        .is_some()
        || store::transfers::phase6_bulk_request_for_request(pool, request_id)
            .await?
            .is_some()
    {
        return Err(transfer_request_id_conflict());
    }
    Ok(())
}

async fn transfer_invitation_replay(
    pool: &store::sqlx::SqlitePool,
    request_id: &str,
    invitation_uid: &str,
    expected_kind: &str,
) -> Result<Option<String>, EngineError> {
    if let Some(event) = store::transfers::invitation_event_for_request(pool, request_id).await? {
        if event.invitation_uid != invitation_uid || event.kind != expected_kind {
            return Err(transfer_request_id_conflict());
        }
        return Ok(Some(event.invitation_uid));
    }
    reject_existing_transfer_revision_request(pool, request_id).await?;
    Ok(None)
}

#[derive(Debug, Default)]
pub struct ActionOutcome {
    pub facts: Vec<Fact>,
    pub created: Option<String>,
    pub warnings: Vec<String>,
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum KarmaKind {
    Program,
    Frequency,
}

fn apply_program_mutation(
    commit: store::karma::programs::ProgramMutationCommit,
    outcome: &mut ActionOutcome,
) -> Result<(), EngineError> {
    match commit {
        store::karma::programs::ProgramMutationCommit::Committed { handle, fact } => {
            outcome.created = Some(handle.record_uid);
            outcome.facts.push(fact);
            Ok(())
        }
        store::karma::programs::ProgramMutationCommit::Replayed { handle, .. } => {
            outcome.created = Some(handle.record_uid);
            Ok(())
        }
        store::karma::programs::ProgramMutationCommit::Stale {
            current_handle_revision,
        } => Err(stale_karma_handle(current_handle_revision)),
    }
}

fn apply_frequency_mutation(
    commit: store::karma::frequencies::FrequencyMutationCommit,
    outcome: &mut ActionOutcome,
) -> Result<(), EngineError> {
    match commit {
        store::karma::frequencies::FrequencyMutationCommit::Committed { handle, fact } => {
            outcome.created = Some(handle.record_uid);
            outcome.facts.push(fact);
            Ok(())
        }
        store::karma::frequencies::FrequencyMutationCommit::Replayed { handle, .. } => {
            outcome.created = Some(handle.record_uid);
            Ok(())
        }
        store::karma::frequencies::FrequencyMutationCommit::Stale {
            current_handle_revision,
        } => Err(stale_karma_handle(current_handle_revision)),
    }
}

fn apply_candidate_review(
    commit: store::karma::candidates::CandidateReviewCommit,
    outcome: &mut ActionOutcome,
) -> Result<(), EngineError> {
    match commit {
        store::karma::candidates::CandidateReviewCommit::Committed { state, fact } => {
            outcome.created = Some(state.candidate_hash.as_str().to_string());
            outcome.facts.push(fact);
            Ok(())
        }
        store::karma::candidates::CandidateReviewCommit::Replayed { state, .. } => {
            outcome.created = Some(state.candidate_hash.as_str().to_string());
            Ok(())
        }
        store::karma::candidates::CandidateReviewCommit::Stale {
            current_state_revision,
        } => Err(EngineError::Conflict {
            code: "karma_stale_candidate_revision",
            message: format!(
                "Karma candidate changed; current state revision is {current_state_revision}"
            ),
        }),
    }
}

fn stale_karma_handle(current_handle_revision: u64) -> EngineError {
    EngineError::Conflict {
        code: "karma_stale_handle_revision",
        message: format!(
            "Karma object changed; current handle revision is {current_handle_revision}"
        ),
    }
}

#[derive(Clone)]
pub(crate) struct VerifiedActionAuthorship {
    pub person_uid: String,
    pub intent_uid: String,
}

impl Engine {
    pub fn act(
        &self,
        action: Action,
        actor: Option<String>,
    ) -> impl std::future::Future<Output = Result<ActionOutcome, EngineError>> + Send + '_ {
        async move { self.act_at(action, actor, nucleus::execution::now()).await }
    }

    pub async fn expire_due_transfer_invitations(
        &self,
        now: DateTime<Utc>,
    ) -> Result<Vec<Fact>, EngineError> {
        let due = store::transfers::due_transfer_invitations(&self.store.pool, now).await?;
        let signer = self.signer.lock().await.clone();
        let mut facts = Vec::new();
        for invitation in due {
            let commit = store::transfers::expire_transfer_invitation(
                &self.store.pool,
                store::transfers::InvitationTransitionInput {
                    invitation_uid: invitation.uid.clone(),
                    expected_revision: 0,
                    idempotency_key: format!(
                        "automatic-transfer-invitation-expiry:{}:{}",
                        invitation.uid, invitation.attempt
                    ),
                    actor_person_uid: None,
                    expires_at: None,
                },
                now,
                |hash| signer.as_ref().map(|value| value.sign_hash(hash)),
            )
            .await?;
            let mut outcome = ActionOutcome::default();
            self.apply_transfer_invitation_commit(commit, 0, &mut outcome)?;
            facts.extend(outcome.facts);
        }
        Ok(facts)
    }

    pub async fn act_at(
        &self,
        action: Action,
        actor: Option<String>,
        now: DateTime<Utc>,
    ) -> Result<ActionOutcome, EngineError> {
        self.act_at_with_authorship(action, actor, now, None).await
    }

    pub(crate) fn act_at_with_authorship(
        &self,
        action: Action,
        actor: Option<String>,
        now: DateTime<Utc>,
        verified_authorship: Option<VerifiedActionAuthorship>,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<ActionOutcome, EngineError>> + Send + '_>,
    > {
        Box::pin(async move {
            if let Action::PreviewKarmaProposal { request } = action {
                let report = self.preview_karma_proposal(request, actor, now).await?;
                return Ok(ActionOutcome {
                    data: Some(serde_json::to_value(report).map_err(EngineError::Json)?),
                    ..Default::default()
                });
            }
            self.access_scope(
                true,
                Box::pin(self.act_authorized(action, actor, now, verified_authorship)),
            )
            .await
        })
    }

    async fn act_authorized(
        &self,
        action: Action,
        actor: Option<String>,
        now: DateTime<Utc>,
        verified_authorship: Option<VerifiedActionAuthorship>,
    ) -> Result<ActionOutcome, EngineError> {
        crate::rule_runtime::execution_checkpoint(false).await?;
        let changes_rules = matches!(&action,
            Action::SaveKarmaRule { .. } | Action::ReviseKarmaField { .. } |
            Action::CreateFrequency { .. } | Action::DeleteFrequency { .. }
            | Action::CreateRecurrence { .. } | Action::ReviseRecurrence { .. }
            | Action::DeleteRecurrence { .. } | Action::SetRecurrencePaused { .. }
            | Action::CreateSignal { .. } | Action::DeleteRecord { .. }
            | Action::SetSlug { .. } | Action::CreateRecord { .. }
            | Action::SetKarmaExecution { .. } | Action::DesignateKarmaExecutor { .. } | Action::ImportKarmaHabit { .. }
        );
        let preview = matches!(&action, Action::PreviewKarmaReading { .. } | Action::InspectTransferKarma { .. } | Action::InspectKarmaSchedules { .. } | Action::PreviewKarmaScheduleDates { .. } | Action::PreviewKarmaHabit { .. });
        let _rule_guard = if matches!(&action,
            Action::SaveKarmaRule { .. } | Action::ReviseKarmaField { .. } |
            Action::CreateRecurrence { .. } | Action::ReviseRecurrence { .. }
            | Action::DeleteRecurrence { .. } | Action::SetRecurrencePaused { .. }
            | Action::SaveKarmaSchedule { .. } | Action::CancelKarmaSchedule { .. } | Action::RetryKarmaSchedule { .. }
        ) { Some(self.rule_execution.lock().await) } else { None };
        let outcome = if let Action::Social { request } = &action {
            self.authorize_action(&action, actor.as_deref()).await?;
            self.social_command(request.clone(), actor.as_deref(), now).await?
        } else if let Action::SetCellConfig { namespace, fds } = &action {
            self.authorize_action(&action, actor.as_deref()).await?;
            self.set_cell_config_action(namespace, fds.clone()).await?
        } else if let Some(outcome) = self.social_message_action(&action, actor.as_deref()).await? {
            outcome
        } else if matches!(action, Action::ApplyRecurrenceOccurrence { .. })
            && !crate::already_firing()
        {
            Box::pin(crate::as_one_firing(self.act_at_inner(
                action,
                actor,
                now,
                verified_authorship,
            )))
            .await?
        } else {
            Box::pin(self.act_at_inner(action, actor, now, verified_authorship)).await?
        };
        if changes_rules {
            self.notify_karma_deadline_change();
        }
        if outcome.facts.is_empty() && !preview {
            self.query_changed
                .send_modify(|revision| *revision = revision.wrapping_add(1));
        }
        Ok(outcome)
    }

    async fn social_message_action(
        &self,
        action: &Action,
        actor: Option<&str>,
    ) -> Result<Option<ActionOutcome>, EngineError> {
        let Action::CreateMessage { thread, body, state, content, references, parent, .. } = action else {
            return Ok(None);
        };
        let thread_uid = self.resolve(thread).await?;
        let Some(root) = store::replica::root_of(&self.store.pool, &thread_uid).await? else {
            return Ok(None);
        };
        if store::records::get_extension(&self.store.pool, &root, nucleus::social::requests::PARTICIPANTS_NAMESPACE).await?.is_none() {
            return Ok(None);
        }
        for transfer in self.canonical_transfer_action_targets(action).await? {
            self.require_transfer_origin_authority(&transfer).await?;
        }
        self.authorize_action(action, actor).await?;
        self.social_require_local_write().await?;
        let thread_row = store::records::get(&self.store.pool, &thread_uid).await?
            .ok_or_else(|| EngineError::UnknownRecord(thread.clone()))?;
        if thread_row.kind != RecordKind::Thread.as_str() {
            return Err(EngineError::Consequence("Choose a Thread for this private message".into()));
        }
        if *state != MessageState::Finished || !references.is_empty() || parent.is_some() {
            return Err(EngineError::Consequence("Private conversations support finished messages with text and attached files. Shared Record references and replies to Messages require separate consent".into()));
        }
        let data = self.social_send_message(&root, body.clone(), content.clone(), actor).await?;
        Ok(Some(ActionOutcome {
            created: data["message"].as_str().map(str::to_owned),
            data: Some(data),
            ..Default::default()
        }))
    }

    async fn set_cell_config_action(
        &self,
        namespace: &str,
        mut fds: serde_json::Value,
    ) -> Result<ActionOutcome, EngineError> {
        if namespace == "lince.discovery" {
            let relays = crate::wire::configured_relays(Some(&fds))?;
            if fds.get("relays").is_some() {
                fds["relays"] = serde_json::json!(relays.iter().map(ToString::to_string).collect::<Vec<_>>());
            }
        }
        if namespace == "lince.network" {
            crate::wire::configured_peer_port(Some(&fds))?;
        }
        if namespace == "lince.karma-runtime" && (fds["running"].as_bool().is_none() || fds.as_object().is_none_or(|fields| fields.len() != 1)) {
            return Err(EngineError::Consequence("Karma runtime configuration needs one running boolean".into()));
        }
        store::cells::set_config(&self.store.pool, namespace, &fds).await?;
        self.notify_config_changed();
        let mut outcome = ActionOutcome::default();
        if namespace == "lince.karma-runtime" {
            self.notify_karma_deadline_change();
            self.effects_changed.send_modify(|revision| *revision = revision.wrapping_add(1));
            outcome.data = Some(serde_json::json!({"karma":self.karma_device_execution().await?}));
        }
        Ok(outcome)
    }

    fn act_at_inner(
        &self,
        action: Action,
        actor: Option<String>,
        now: DateTime<Utc>,
        verified_authorship: Option<VerifiedActionAuthorship>,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<ActionOutcome, EngineError>> + Send + '_>,
    > {
        Box::pin(async move {
            for transfer_uid in self.canonical_transfer_action_targets(&action).await? {
                self.require_transfer_origin_authority(&transfer_uid).await?;
            }
            self.authorize_action(&action, actor.as_deref()).await?;
            let reacts_to_transfer = matches!(
                &action,
                Action::AssignTransferAgreementLevel { .. }
                    | Action::SetTransferAgreementLevel { .. }
                    | Action::CounterofferTransfer { .. }
                    | Action::PublishTransfer { .. }
                    | Action::ActivateTransferOccurrence { .. }
            );
            let mut outcome = match self
                .dispatch_action_parts(action, actor, now, verified_authorship)
                .await?
            {
                std::ops::ControlFlow::Break(outcome) => return Ok(outcome),
                std::ops::ControlFlow::Continue(outcome) => outcome,
            };
            if reacts_to_transfer {
                let mut roots = std::collections::BTreeMap::new();
                for fact in &outcome.facts {
                    self.observe_fact_state(fact, now).await?;
                    roots.insert(fact.record_uid.clone(), fact.uid.clone());
                }
                for (record, fact) in roots {
                    outcome.facts.extend(
                        Box::pin(self.react_to_event(vec![record], fact, now)).await?,
                    );
                }
            }
            Ok(outcome)
        })
    }

    pub(crate) async fn resolve(&self, token: &str) -> Result<String, EngineError> {
        let token = token.trim_start_matches('@');
        store::records::resolve(&self.store.pool, token)
            .await?
            .map(|r| r.uid)
            .ok_or_else(|| EngineError::UnknownRecord(token.to_string()))
    }

    pub fn authorize_action<'a>(
        &'a self,
        action: &'a Action,
        actor: Option<&'a str>,
    ) -> impl std::future::Future<Output = Result<(), EngineError>> + 'a {
        Box::pin(self.authorize_action_inner(action, actor))
    }

    async fn authorize_action_inner(
        &self,
        action: &Action,
        actor: Option<&str>,
    ) -> Result<(), EngineError> {
        if let Some(actor) = actor {
            self.actor_user(actor).await?;
        }
        if let Some(permission) = Self::generic_write_permission(action) {
            self.require_permission(actor, permission).await?;
        }
        let touched = self.record_targets_of(action).await?;
        self.refuse_unreadable(actor, &touched).await?;
        let edited = match action {
            Action::AssertRecord { subject, .. } | Action::RefineAssertion { subject, .. } => {
                vec![self.resolve(subject).await?]
            }
            Action::CreateThread { .. }
            | Action::CreateMessage { .. }
            | Action::CreateMessageDraft { .. }
            | Action::PreviewAreaTransition { .. } => vec![],
            _ => touched,
        };
        if let Some(actor) = actor {
            for uid in &edited {
                if store::records::get(&self.store.pool, uid)
                    .await?
                    .is_some_and(|record| record.kind == "person")
                    && store::auth::person_access(&self.store.pool, uid)
                        .await?
                        .is_some()
                {
                    self.require_permission(
                        Some(actor),
                        if uid == actor {
                            "user:update_self"
                        } else {
                            "user:update"
                        },
                    )
                    .await?;
                    self.require_manageable_person(Some(actor), uid).await?;
                }
            }
        }
        match action {
            Action::CreateUser {
                username,
                name,
                password,
                role,
            } => {
                if username.trim().is_empty()
                    || username.len() > 256
                    || name.len() > 500
                    || password.is_empty()
                    || password.len() > 1024
                {
                    return Err(EngineError::Consequence("Invalid account details".into()));
                }
                self.require_permission(actor, "user:create").await?;
                self.require_permission(actor, "user:assign_role").await?;
                self.require_assignable_role(actor, role).await?;
            }
            Action::CreateRole { name } => {
                self.require_permission(actor, "role:create").await?;
                if !store::roles::valid_name(name) {
                    return Err(EngineError::Consequence("Invalid role name".into()));
                }
            }
            Action::RenameRole { .. } => self.require_permission(actor, "role:update").await?,
            Action::DeleteRole { .. } => self.require_permission(actor, "role:delete").await?,
            Action::UpdateUser { user, .. } | Action::DeleteUser { user } => {
                self.require_permission(
                    actor,
                    if matches!(action, Action::DeleteUser { .. }) {
                        "user:delete"
                    } else {
                        "user:update"
                    },
                )
                .await?;
                self.require_manageable_person(actor, &self.resolve(user).await?)
                    .await?;
            }
            Action::AssignRole { user, role } => {
                self.require_permission(actor, "user:assign_role").await?;
                self.require_assignable_role(actor, role).await?;
                let uid = self.resolve(user).await?;
                self.require_manageable_person(actor, &uid).await?;
                if let Some(target) = store::auth::principal(&self.store.pool, &uid).await?
                    && target.role == "admin"
                    && role != "admin"
                {
                    self.require_other_admin(&uid).await?;
                }
            }
            Action::GrantPermission { role, permission }
            | Action::RevokePermission { role, permission } => {
                self.require_permission(actor, "permission:assign").await?;
                self.require_permission(actor, permission).await?;
                self.require_assignable_role(actor, role).await?;
                if role == "admin" || !utils::auth::all_permission_keys().contains(permission) {
                    return Err(EngineError::Forbidden(
                        "Admin permissions are fixed; choose an existing permission".into(),
                    ));
                }
            }
            Action::SetPersonReadFilter { person, .. }
            | Action::SetPersonStanding { person, .. } => {
                self.require_permission(actor, "user:update").await?;
                self.require_manageable_person(actor, &self.resolve(person).await?)
                    .await?;
            }
            Action::SetRoleReadRules { role, .. } => {
                self.require_permission(actor, "role:update").await?;
                self.require_permission(actor, "permission:assign").await?;
                self.require_assignable_role(actor, role).await?;
                if role == "admin"
                    || (actor.is_some() && self.actor_user(actor.unwrap()).await?.role != "admin")
                {
                    return Err(EngineError::Forbidden(
                        "Only admins may change non-admin role rules".into(),
                    ));
                }
            }
            Action::DeleteRecord { target } | Action::DeleteMessageDraft { draft: target } => {
                let uid = self.resolve(target).await?;
                self.check_delete_permission(&uid, actor).await?;
                if store::auth::person_access(&self.store.pool, &uid)
                    .await?
                    .is_some()
                {
                    self.require_permission(actor, "user:delete").await?;
                    self.require_manageable_person(actor, &uid).await?;
                    if store::auth::principal(&self.store.pool, &uid)
                        .await?
                        .is_some_and(|user| user.role == "admin")
                    {
                        self.require_other_admin(&uid).await?;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    async fn require_assignable_role(
        &self,
        actor: Option<&str>,
        role: &str,
    ) -> Result<(), EngineError> {
        let Some(actor) = actor else {
            return Ok(());
        };
        let viewer = self.actor_user(actor).await?;
        let permissions = store::auth::role_permission_keys(&self.store.pool, role).await?;
        if (role == "admin" && viewer.role != "admin")
            || permissions.iter().any(|key| !viewer.permits(key))
        {
            return Err(EngineError::Forbidden(
                "You cannot manage permissions beyond your own access".into(),
            ));
        }
        Ok(())
    }

    pub(crate) async fn require_manageable_person(
        &self,
        actor: Option<&str>,
        person: &str,
    ) -> Result<(), EngineError> {
        let Some(actor) = actor else {
            return Ok(());
        };
        let viewer = self.actor_user(actor).await?;
        let target = {
            let mut connection = self.store.pool.acquire().await?;
            store::auth::assigned_role_on(&mut connection, person).await?
        };
        if let Some(target) = target
            && ((target.role == "admin" && viewer.role != "admin")
                || target.permissions.iter().any(|key| !viewer.permits(key)))
        {
            return Err(EngineError::Forbidden(
                "You cannot manage a person with greater access".into(),
            ));
        }
        Ok(())
    }

    pub(crate) async fn require_other_admin(&self, person: &str) -> Result<(), EngineError> {
        for uid in store::auth::admins(&self.store.pool).await? {
            if uid != person && store::people::is_active(&self.store.pool, &uid).await? {
                return Ok(());
            }
        }
        Err(EngineError::Forbidden(
            "Keep at least one active administrator".into(),
        ))
    }

    async fn actor_user(&self, actor: &str) -> Result<store::auth::Principal, EngineError> {
        store::auth::principal(&self.store.pool, actor)
            .await?
            .ok_or_else(|| EngineError::Forbidden("unrecognized actor".into()))
    }

    pub async fn require_permission(
        &self,
        actor: Option<&str>,
        permission: &str,
    ) -> Result<(), EngineError> {
        let Some(actor) = actor else {
            return Ok(());
        };
        let user = self.actor_user(actor).await?;
        if user.permissions.iter().any(|p| p == permission) {
            return Ok(());
        }
        Err(EngineError::Forbidden(format!(
            "missing {permission} permission"
        )))
    }

    pub(crate) async fn actor_person(
        &self,
        actor: Option<&str>,
    ) -> Result<Option<String>, EngineError> {
        Ok(actor.map(str::to_string))
    }

    pub(crate) async fn canonical_transfer_action_targets(
        &self,
        action: &Action,
    ) -> Result<Vec<String>, EngineError> {
        let mut targets = Vec::new();
        let direct = match action {
            Action::ReopenTransferPromise { transfer, .. }
            | Action::ReviseTransferPromise { transfer, .. }
            | Action::ReviseTransferDraft { transfer, .. }
            | Action::AdoptTransferDraft { transfer, .. }
            | Action::AddressTransferInvitation { transfer, .. }
            | Action::CounterofferTransfer { transfer, .. }
            | Action::ClaimOpenTransferPromise { transfer, .. }
            | Action::SetTransferAgreementLevel { transfer, .. }
            | Action::AssignTransferAgreementLevel { transfer, .. }
            | Action::PublishTransfer { transfer, .. }
            | Action::ActivateTransferFulfillment { transfer, .. }
            | Action::SetTransferChildRequirement { transfer, .. }
            | Action::ProposeTransferCancellation { transfer, .. }
            | Action::ProposeTransferLoanExtension { transfer, .. }
            | Action::ApplyTransferCancellation { transfer, .. }
            | Action::ActivateTransferOccurrence { transfer, .. }
            | Action::ConfigureTransferDelivery { transfer, .. }
            | Action::SetTransferDeliveryMode { transfer, .. }
            | Action::EnqueueTransferDelivery { transfer, .. }
            | Action::RetryTransferDelivery { transfer, .. }
            | Action::RevokeTransferDelivery { transfer, .. }
            | Action::CreateTransferThread { transfer, .. }
            | Action::CreateTransferMessage { transfer, .. }
            | Action::BeginTransferSettlement { transfer, .. }
            | Action::ConfirmTransfer { transfer, .. }
            | Action::AddParty { transfer, .. }
            | Action::AddPromiseToTransfer { transfer, .. }
            | Action::AgreeTransfer { transfer, .. }
            | Action::ActivateTransfer { transfer }
            | Action::SettleTransfer { transfer, .. } => Some(transfer.as_str()),
            Action::DesignateTransferExecutor { transfer_uid, .. } => Some(transfer_uid.as_str()),
            _ => None,
        };
        if let Some(transfer) = direct {
            targets.push(self.resolve(transfer).await?);
        }
        let invitation = match action {
            Action::AcceptTransferInvitation { invitation, .. }
            | Action::RejectTransferInvitation { invitation, .. }
            | Action::WithdrawTransferInvitation { invitation, .. }
            | Action::ReopenTransferInvitation { invitation, .. } => Some(invitation.as_str()),
            _ => None,
        };
        if let Some(invitation) = invitation {
            let row = store::transfers::invitation(&self.store.pool, invitation)
                .await?
                .ok_or_else(|| EngineError::UnknownRecord(invitation.into()))?;
            targets.push(row.transfer_uid);
        }
        let occurrence = match action {
            Action::CreateTransferRemainderDraft { occurrence, .. }
            | Action::CreateReversingTransferDraft { occurrence, .. }
            | Action::SetTransferOccurrenceClaim { occurrence, .. }
            | Action::SetTransferOccurrenceDispute { occurrence, .. }
            | Action::SetTransferOccurrenceApplicationFormula { occurrence, .. }
            | Action::SettleTransferOccurrence { occurrence, .. } => Some(occurrence.as_str()),
            _ => None,
        };
        if let Some(occurrence) = occurrence {
            let row = store::transfers::occurrence(&self.store.pool, occurrence)
                .await?
                .ok_or_else(|| EngineError::UnknownRecord(occurrence.into()))?;
            targets.push(row.transfer_uid);
        }
        if let Action::CompleteTransferOccurrenceClaimsBulk { items, .. } = action {
            for item in items {
                let row = store::transfers::occurrence(&self.store.pool, &item.occurrence)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(item.occurrence.clone()))?;
                targets.push(row.transfer_uid);
            }
        }
        if let Action::CompensateTransferOccurrenceSettlement { settlement, .. } = action {
            let row = store::transfers::occurrence_settlement_slice(&self.store.pool, settlement)
                .await?
                .ok_or_else(|| EngineError::UnknownRecord(settlement.clone()))?;
            targets.push(row.transfer_uid);
        }
        if let Action::CreateThread { target, .. } = action {
            let target = self.resolve(target).await?;
            if store::transfers::get(&self.store.pool, &target)
                .await?
                .is_some()
            {
                targets.push(target);
            }
        }
        if let Action::CreateMessage { thread, .. } = action {
            let thread = self.resolve(thread).await?;
            if let Some(transfer) = self.transfer_for_thread(&thread).await? {
                targets.push(transfer);
            }
        }
        targets.sort();
        targets.dedup();
        Ok(targets)
    }

    async fn require_transfer_creator(
        &self,
        actor: Option<&str>,
    ) -> Result<Option<String>, EngineError> {
        self.require_permission(actor, "transfer:create").await?;
        self.actor_person(actor).await
    }

    async fn require_transfer_draft_creator(
        &self,
        transfer_uid: &str,
        actor: Option<&str>,
    ) -> Result<(), EngineError> {
        self.require_transfer_origin_authority(transfer_uid).await?;
        let Some(actor) = actor else {
            return Ok(());
        };
        self.require_permission(Some(actor), "transfer:update")
            .await?;
        if store::facts::creator_uid(&self.store.pool, transfer_uid)
            .await?
            .as_deref()
            == Some(actor)
        {
            return Ok(());
        }
        Err(EngineError::Forbidden(
            "Phase 1 draft revision requires the transfer creator".into(),
        ))
    }

    async fn transfer_creator_person(&self, transfer_uid: &str) -> Result<String, EngineError> {
        store::transfers::creator_party_actor(&self.store.pool, transfer_uid)
            .await?
            .ok_or_else(|| EngineError::Conflict {
                code: "transfer_creator_evidence_missing",
                message: "revisioned transfer has no signed creator Person marker".into(),
            })
    }

    pub(crate) async fn transfer_action_person(
        &self,
        actor: Option<&str>,
        explicit: Option<&str>,
        derived_local: Option<&str>,
    ) -> Result<String, EngineError> {
        if let Some(mapped) = self.actor_person(actor).await? {
            if let Some(token) = explicit.map(str::trim).filter(|value| !value.is_empty()) {
                let submitted = self.resolve(token).await?;
                if submitted != mapped {
                    return Err(EngineError::Forbidden(
                        "authenticated transfer identity is derived from the session".into(),
                    ));
                }
            }
            return Ok(mapped);
        }
        let token = explicit
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .or(derived_local)
            .ok_or_else(|| {
                EngineError::Forbidden(
                    "trusted local transfer action requires an acting Person".into(),
                )
            })?;
        let person_uid = self.resolve(token).await?;
        let record = store::records::get(&self.store.pool, &person_uid)
            .await?
            .ok_or_else(|| EngineError::UnknownRecord(person_uid.clone()))?;
        if record.kind != RecordKind::Person.as_str() {
            return Err(EngineError::Consequence(
                "transfer actor must be a Person record".into(),
            ));
        }
        Ok(person_uid)
    }

    async fn transfer_for_thread(&self, thread_uid: &str) -> Result<Option<String>, EngineError> {
        let Some(thread_of) = store::concepts::resolve(&self.store.pool, "thread-of").await? else {
            return Ok(None);
        };
        let mut transfers = Vec::new();
        for target in
            store::assertions::objects_from_subject(&self.store.pool, thread_uid, &thread_of)
                .await?
        {
            if store::transfers::get(&self.store.pool, &target.uid)
                .await?
                .is_some()
            {
                transfers.push(target.uid);
            }
        }
        match transfers.as_slice() {
            [] => Ok(None),
            [transfer] => Ok(Some(transfer.clone())),
            _ => Err(EngineError::Conflict {
                code: "transfer_thread_target_ambiguous",
                message: "a negotiation thread must belong to exactly one Transfer".into(),
            }),
        }
    }

    async fn thread_for_message(&self, message_uid: &str) -> Result<String, EngineError> {
        let message_in = store::concepts::resolve(&self.store.pool, "message-in")
            .await?
            .ok_or_else(|| EngineError::Conflict {
                code: "message_thread_missing",
                message: "the message-in relationship is not defined".into(),
            })?;
        let threads =
            store::assertions::objects_from_subject(&self.store.pool, message_uid, &message_in)
                .await?;
        match threads.as_slice() {
            [thread] if thread.kind == RecordKind::Thread.as_str() => Ok(thread.uid.clone()),
            [] => Err(EngineError::Conflict {
                code: "message_thread_missing",
                message: "the message does not belong to a thread".into(),
            }),
            _ => Err(EngineError::Conflict {
                code: "message_thread_ambiguous",
                message: "the message must belong to exactly one thread".into(),
            }),
        }
    }

    async fn message_authorship(
        &self,
        author: Option<&str>,
        actor: Option<&str>,
    ) -> Result<(String, String), EngineError> {
        let operator = match actor {
            Some(actor) => actor.to_string(),
            None => store::organs::local(&self.store.pool)
                .await?
                .map(|organ| organ.uid)
                .unwrap_or_else(|| "local".into()),
        };
        let Some(author) = author.map(str::trim).filter(|value| !value.is_empty()) else {
            return Ok((operator.clone(), operator));
        };
        let author_uid = self.resolve(author).await?;
        let author_record = store::records::get(&self.store.pool, &author_uid)
            .await?
            .ok_or_else(|| EngineError::UnknownRecord(author_uid.clone()))?;
        if author_record.kind != RecordKind::Person.as_str() {
            return Err(EngineError::Consequence(
                "a message author must be a Person or Agent record".into(),
            ));
        }
        if let Some(actor) = actor
            && author_uid != actor
        {
            let operated_by = store::concepts::resolve(&self.store.pool, "operated-by").await?;
            let delegated = match operated_by {
                Some(predicate) => store::assertions::objects_from_subject(
                    &self.store.pool,
                    &author_uid,
                    &predicate,
                )
                .await?
                .iter()
                .any(|record| record.uid == actor),
                None => false,
            };
            if !delegated {
                return Err(EngineError::Forbidden(
                    "a delegated message author must be operated by the acting Person".into(),
                ));
            }
        }
        Ok((author_uid, operator))
    }

    async fn validate_message_draft_target(
        &self,
        conversation: &str,
        thread: &str,
    ) -> Result<(String, String), EngineError> {
        let conversation_uid = self.resolve(conversation).await?;
        let conversation_row = store::records::get(&self.store.pool, &conversation_uid)
            .await?
            .ok_or_else(|| EngineError::UnknownRecord(conversation.to_string()))?;
        if conversation_row.kind != RecordKind::Conversation.as_str() {
            return Err(EngineError::Consequence(
                "a message draft must target a Conversation".into(),
            ));
        }
        let thread_uid = self.resolve(thread).await?;
        let thread_row = store::records::get(&self.store.pool, &thread_uid)
            .await?
            .ok_or_else(|| EngineError::UnknownRecord(thread.to_string()))?;
        if thread_row.kind != RecordKind::Thread.as_str() {
            return Err(EngineError::Consequence(
                "a message draft must target a Thread".into(),
            ));
        }
        let thread_of = store::concepts::resolve(&self.store.pool, "thread-of")
            .await?
            .ok_or_else(|| EngineError::Conflict {
                code: "message_draft_thread_missing",
                message: "the thread-of relationship is not defined".into(),
            })?;
        let targets =
            store::assertions::objects_from_subject(&self.store.pool, &thread_uid, &thread_of)
                .await?;
        if !targets.iter().any(|target| target.uid == conversation_uid) {
            return Err(EngineError::Conflict {
                code: "message_draft_thread_mismatch",
                message: "the draft thread belongs to another Record".into(),
            });
        }
        Ok((conversation_uid, thread_uid))
    }

    async fn message_draft_metadata(
        &self,
        draft: &str,
        actor: Option<&str>,
    ) -> Result<(String, serde_json::Map<String, serde_json::Value>), EngineError> {
        let draft_uid = self.resolve(draft).await?;
        let row = store::records::get(&self.store.pool, &draft_uid)
            .await?
            .ok_or_else(|| EngineError::UnknownRecord(draft.to_string()))?;
        if row.kind != RecordKind::MessageDraft.as_str() {
            return Err(EngineError::Consequence(
                "the target is not a message draft".into(),
            ));
        }
        let metadata =
            store::records::get_extension(&self.store.pool, &draft_uid, "lince.message-draft")
                .await?
                .and_then(|value| value.as_object().cloned())
                .ok_or_else(|| EngineError::Conflict {
                    code: "message_draft_invalid",
                    message: "the message draft has no persisted metadata".into(),
                })?;
        if let Some(actor) = actor
            && metadata.get("operator").and_then(serde_json::Value::as_str) != Some(actor)
        {
            return Err(EngineError::Forbidden(
                "only the message draft author may use it".into(),
            ));
        }
        Ok((draft_uid, metadata))
    }

    async fn resolve_message_references(
        &self,
        references: Vec<String>,
    ) -> Result<Vec<String>, EngineError> {
        const MAX_MESSAGE_REFERENCES: usize = 32;
        if references.len() > MAX_MESSAGE_REFERENCES {
            return Err(EngineError::Consequence(format!(
                "a message may reference at most {MAX_MESSAGE_REFERENCES} Records"
            )));
        }
        let mut resolved = Vec::with_capacity(references.len());
        let mut seen = HashSet::with_capacity(references.len());
        for reference in references {
            let token = reference.trim();
            if token.is_empty() {
                return Err(EngineError::Consequence(
                    "message Record references cannot be empty".into(),
                ));
            }
            let uid = self.resolve(token).await?;
            if !seen.insert(uid.clone()) {
                return Err(EngineError::Consequence(
                    "a message cannot reference the same Record twice".into(),
                ));
            }
            let record = store::records::get(&self.store.pool, &uid)
                .await?
                .ok_or_else(|| EngineError::UnknownRecord(uid.clone()))?;
            if matches!(
                RecordKind::parse(&record.kind),
                Some(
                    RecordKind::Transfer
                        | RecordKind::Thread
                        | RecordKind::Message
                        | RecordKind::CallSession
                )
            ) {
                return Err(EngineError::Consequence(
                    "messages may reference content Records, not Transfer communication structure"
                        .into(),
                ));
            }
            resolved.push(uid);
        }
        Ok(resolved)
    }

    pub(crate) async fn require_verified_transfer_revision(
        &self,
        transfer_uid: &str,
        revision: u64,
    ) -> Result<(), EngineError> {
        self.require_transfer_origin_authority(transfer_uid).await?;
        let fact = store::transfers::revision_fact(&self.store.pool, transfer_uid, revision)
            .await?
            .ok_or_else(|| EngineError::Conflict {
                code: "transfer_revision_evidence_missing",
                message: "agreement requires an existing signed Transfer revision".into(),
            })?;
        if !crate::trust::verify_fact(&self.store, &fact).await? {
            return Err(EngineError::Conflict {
                code: "transfer_revision_signature_invalid",
                message:
                    "agreement requires a Transfer revision signed by its published author key"
                        .into(),
            });
        }
        Ok(())
    }

    pub(crate) async fn transfer_person_signer(
        &self,
        person_uid: &str,
        verified_authorship: Option<&VerifiedActionAuthorship>,
    ) -> Result<Option<crate::trust::Signer>, EngineError> {
        let signer = self.signer.lock().await.clone();
        if let Some(signer) = signer.as_ref() {
            if signer.actor_uid == person_uid {
                return Ok(Some(signer.clone()));
            }
            if verified_authorship.map(|authorship| authorship.person_uid.as_str())
                != Some(person_uid)
            {
                return Err(EngineError::Conflict {
                    code: "signer_identity_mismatch",
                    message: "the installed signing key does not belong to the acting Person"
                        .into(),
                });
            }
        }
        if verified_authorship.is_some_and(|authorship| authorship.person_uid == person_uid) {
            return Ok(None);
        }
        if signer.is_some() {
            return Err(EngineError::Conflict {
                code: "signer_identity_mismatch",
                message: "the installed signing key does not belong to the acting Person".into(),
            });
        }
        Err(EngineError::Conflict {
            code: "missing_person_signer",
            message: "this Transfer action requires the acting Person's signing key or verified signed Action intent"
                .into(),
        })
    }

    async fn transfer_reservation_cell_default(&self) -> Result<TransferReservePoint, EngineError> {
        let value = store::sqlx::query_scalar::<_, String>(
            "SELECT transfer_reservation_default FROM configuration WHERE id = 1",
        )
        .fetch_optional(&self.store.pool)
        .await?
        .unwrap_or_else(|| "none".into());
        match value.as_str() {
            "none" => Ok(TransferReservePoint::None),
            "proposed" => Ok(TransferReservePoint::Proposed),
            "agreed" => Ok(TransferReservePoint::Agreed),
            "active" => Ok(TransferReservePoint::Active),
            _ => Err(EngineError::Conflict {
                code: "transfer_reservation_default_invalid",
                message: "Cell transfer reservation default is invalid".into(),
            }),
        }
    }

    async fn require_transfer_thread_writer(
        &self,
        transfer_uid: &str,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<(), EngineError> {
        self.require_transfer_origin_authority(transfer_uid).await?;
        let Some(person) = self.actor_person(actor).await? else {
            return Ok(());
        };
        let creator = self.transfer_creator_person(transfer_uid).await?;
        let participant =
            store::transfers::party_for_actor(&self.store.pool, transfer_uid, &person)
                .await?
                .is_some();
        let pending_addressee =
            store::transfers::invitations_for_transfer(&self.store.pool, transfer_uid)
                .await?
                .into_iter()
                .any(|invitation| {
                    invitation.addressed_person_uid == person
                        && invitation.status == store::transfers::TransferInvitationStatus::Pending
                        && !invitation.expires_at.as_deref().is_some_and(|value| {
                            DateTime::parse_from_rfc3339(value)
                                .is_ok_and(|expiry| expiry.with_timezone(&Utc) <= now)
                        })
                });
        if person == creator || participant || pending_addressee {
            return Ok(());
        }
        Err(EngineError::Forbidden(
            "Transfer negotiation writes require the creator, an accepted participant, or a pending addressee"
                .into(),
        ))
    }

    async fn resolve_transfer_item(
        &self,
        item: Option<nucleus::transfer::disclosure::TransferItem>,
        person: &str,
    ) -> Result<Option<nucleus::transfer::disclosure::TransferItem>, EngineError> {
        let Some(mut item) = item else {
            return Ok(None);
        };
        item.title = item.title.trim().to_string();
        item.validate()
            .map_err(|error| EngineError::Consequence(error.into()))?;
        if let Some(exchange) = &mut item.exchange {
            for person in [&mut exchange.giver, &mut exchange.receiver] {
                let uid = self.resolve(person.trim()).await?;
                let record = store::records::get(&self.store.pool, &uid).await?
                    .ok_or_else(|| EngineError::UnknownRecord(uid.clone()))?;
                if record.kind != RecordKind::Person.as_str() {
                    return Err(EngineError::Consequence("exchange endpoints must be People".into()));
                }
                *person = uid;
            }
            exchange.validate().map_err(|message| EngineError::Consequence(message.into()))?;
        }
        for field in item.disclosure.fields_mut() {
            for person in &mut field.people {
                let uid = self.resolve(person.trim()).await?;
                let record = store::records::get(&self.store.pool, &uid)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(uid.clone()))?;
                if record.kind != RecordKind::Person.as_str() {
                    return Err(EngineError::Consequence(
                        "a disclosure recipient must be a Person".into(),
                    ));
                }
                *person = uid;
            }
            field.people.sort();
            field.people.dedup();
        }
        if let Some(link) = item.return_of.as_ref().or(item.future_need_for.as_ref()) {
            if store::records::get(&self.store.pool, &link.transfer).await?.is_some()
                && self.readable_by(person).await?.is_some_and(|visible| !visible.contains(&link.transfer)) {
                return Err(EngineError::Consequence("the linked loan is unavailable to this Person".into()));
            }
            store::transfer_loans::validate_link(&self.store.pool, &item, person).await?;
        }
        Ok(Some(item))
    }

    async fn resolve_transfer_item_source(
        &self,
        token: &str,
        has_item: bool,
        actor: Option<&str>,
    ) -> Result<(Option<String>, Option<String>), EngineError> {
        if token.trim().is_empty() && has_item {
            return Ok((None, None));
        }
        let uid = self.resolve_transfer_visible_record(token, actor).await?;
        let record = store::records::get(&self.store.pool, &uid)
            .await?
            .ok_or_else(|| EngineError::UnknownRecord(uid.clone()))?;
        Ok((Some(uid), record.identity_predicate_uid))
    }

    async fn resolve_transfer_visible_record(
        &self,
        token: &str,
        actor: Option<&str>,
    ) -> Result<String, EngineError> {
        let uid = self.resolve(token.trim()).await?;
        if !self.may_read_record(actor, &uid).await? {
            return Err(EngineError::Forbidden(
                "Transfer source is unavailable".into(),
            ));
        }
        Ok(uid)
    }

    pub(crate) async fn readable_remote_transfer(&self, target: &str, actor: Option<&str>) -> Result<Option<String>, EngineError> {
        Ok(store::sqlx::query_scalar(
            "SELECT r.transfer_uid FROM transfer_remote_reference r
             WHERE (r.uid = ? OR r.transfer_uid = ?) AND r.state = 'active' AND r.projection IS NOT NULL
               AND (? IS NULL OR r.recipient_person_uid = ?)
               AND r.recipient_organ_uid = (SELECT uid FROM record WHERE slug = 'local-organ' AND kind = 'organ' AND deleted_at IS NULL)
             ORDER BY r.last_transfer_revision DESC LIMIT 1",
        ).bind(target).bind(target).bind(actor).bind(actor).fetch_optional(&self.store.pool).await?)
    }

    async fn resolve_transfer_dependencies(
        &self,
        transfer_uid: Option<&str>,
        inputs: Vec<TransferDependencyInput>,
        promise_uids: &HashSet<String>,
        actor: Option<&str>,
    ) -> Result<Vec<store::transfers::TransferDependencyInput>, EngineError> {
        let mut dependency_uids = HashSet::new();
        let mut resolved = Vec::with_capacity(inputs.len());
        for input in inputs {
            if let Some(uid) = input.uid.as_deref()
                && !dependency_uids.insert(uid.to_string())
            {
                return Err(EngineError::Consequence(
                    "a Transfer draft cannot repeat a dependency uid".into(),
                ));
            }
            let promise_uid = match input.scope {
                TransferDependencyScopeInput::Transfer => {
                    if input.promise.is_some() {
                        return Err(EngineError::Consequence(
                            "a Transfer-scoped dependency cannot name a promise".into(),
                        ));
                    }
                    None
                }
                TransferDependencyScopeInput::Promise => {
                    let promise = input
                        .promise
                        .map(|value| value.trim().to_string())
                        .filter(|value| !value.is_empty())
                        .ok_or_else(|| {
                            EngineError::Consequence(
                                "a promise-scoped dependency must name its local promise".into(),
                            )
                        })?;
                    if !promise_uids.contains(&promise) {
                        return Err(EngineError::Consequence(
                            "a promise-scoped dependency must target a retained promise uid; new dependent promises need an explicit stable uid"
                                .into(),
                        ));
                    }
                    Some(promise)
                }
            };
            let upstream_token = input.upstream.trim();
            if upstream_token.is_empty() {
                return Err(EngineError::Consequence(
                    "a Transfer dependency must name an upstream target".into(),
                ));
            }
            let (upstream_kind, upstream_uid, upstream_transfer) = match input.upstream_kind {
                TransferDependencyUpstreamKindInput::Transfer => {
                    let uid = if let Some(record) = store::records::resolve(&self.store.pool, upstream_token).await? {
                        if store::transfers::get(&self.store.pool, &record.uid).await?.is_none() {
                            return Err(EngineError::Consequence("a Transfer dependency upstream must be a Transfer record".into()));
                        }
                        record.uid
                    } else {
                        self.readable_remote_transfer(upstream_token, actor).await?
                            .ok_or_else(|| EngineError::Forbidden("The upstream outcome is unavailable".into()))?
                    };
                    (
                        nucleus::transfer::TransferDependencyUpstreamKind::Transfer,
                        uid.clone(),
                        Some(uid),
                    )
                }
                TransferDependencyUpstreamKindInput::Promise => {
                    if promise_uids.contains(upstream_token) {
                        (
                            nucleus::transfer::TransferDependencyUpstreamKind::Promise,
                            upstream_token.to_string(),
                            transfer_uid.map(str::to_string),
                        )
                    } else {
                        let promise = store::misc::get_promise(&self.store.pool, upstream_token)
                            .await?
                            .ok_or_else(|| {
                                EngineError::UnknownRecord(upstream_token.to_string())
                            })?;
                        (
                            nucleus::transfer::TransferDependencyUpstreamKind::Promise,
                            promise.uid,
                            promise.transfer_uid,
                        )
                    }
                }
            };
            if let Some(actor) = actor {
                let target = if let Some(transfer) = &upstream_transfer {
                    Some(transfer.clone())
                } else {
                    store::misc::get_promise(&self.store.pool, &upstream_uid).await?.and_then(|promise| promise.record_uid)
                };
                if let Some(target) = target {
                    if !self.may_read_record(Some(actor), &target).await?
                        && self.readable_remote_transfer(&target, Some(actor)).await?.is_none() {
                        return Err(EngineError::Forbidden("The upstream outcome is unavailable".into()));
                    }
                } else {
                    return Err(EngineError::Forbidden("The upstream outcome is unavailable".into()));
                }
            }
            if promise_uid.as_deref() == Some(upstream_uid.as_str()) {
                return Err(EngineError::Consequence(
                    "a promise cannot depend on itself".into(),
                ));
            }
            if transfer_uid.is_some_and(|transfer| upstream_uid == transfer) {
                return Err(EngineError::Consequence(
                    "a Transfer cannot depend on itself".into(),
                ));
            }
            if let (Some(transfer_uid), Some(upstream_transfer)) =
                (transfer_uid, upstream_transfer.as_deref())
                && upstream_transfer != transfer_uid
                && self
                    .transfer_dependency_reaches(upstream_transfer, transfer_uid)
                    .await?
            {
                return Err(EngineError::Conflict {
                    code: "transfer_dependency_cycle",
                    message: "the dependency would create a Transfer cycle".into(),
                });
            }
            let required_state = input.required_state.trim().to_ascii_lowercase();
            let required_state = if required_state == "settled" { "kept".to_string() } else { required_state };
            if upstream_kind == nucleus::transfer::TransferDependencyUpstreamKind::Transfer && !matches!(required_state.as_str(), "agreed" | "kept") {
                return Err(EngineError::Consequence("Transfer dependencies require an agreed or settled outcome".into()));
            }
            if !matches!(
                required_state.as_str(),
                "open" | "proposed" | "agreed" | "active" | "kept" | "broken" | "withdrawn"
            ) {
                return Err(EngineError::Consequence(
                    "dependency required_state must be a promise state".into(),
                ));
            }
            resolved.push(store::transfers::TransferDependencyInput {
                uid: input.uid,
                scope: match input.scope {
                    TransferDependencyScopeInput::Transfer => {
                        nucleus::transfer::TransferDependencyScope::Transfer
                    }
                    TransferDependencyScopeInput::Promise => {
                        nucleus::transfer::TransferDependencyScope::Promise
                    }
                },
                promise_uid,
                upstream_kind,
                upstream_uid,
                required_state,
            });
        }
        let mut local_edges: std::collections::HashMap<&str, Vec<&str>> = Default::default();
        for dependency in &resolved {
            if dependency.upstream_kind
                != nucleus::transfer::TransferDependencyUpstreamKind::Promise
                || !promise_uids.contains(&dependency.upstream_uid)
            {
                continue;
            }
            let Some(target) = dependency.promise_uid.as_deref() else {
                return Err(EngineError::Conflict {
                    code: "transfer_dependency_cycle",
                    message: "a Transfer-wide dependency cannot point at one of its own promises"
                        .into(),
                });
            };
            local_edges
                .entry(target)
                .or_default()
                .push(&dependency.upstream_uid);
        }
        for start in local_edges.keys() {
            let mut pending = local_edges.get(start).cloned().unwrap_or_default();
            let mut seen = HashSet::new();
            while let Some(next) = pending.pop() {
                if next == *start {
                    return Err(EngineError::Conflict {
                        code: "transfer_dependency_cycle",
                        message: "the dependency would create a promise cycle".into(),
                    });
                }
                if seen.insert(next)
                    && let Some(further) = local_edges.get(next)
                {
                    pending.extend(further.iter().copied());
                }
            }
        }
        Ok(resolved)
    }

    async fn transfer_dependency_reaches(
        &self,
        start_transfer_uid: &str,
        target_transfer_uid: &str,
    ) -> Result<bool, EngineError> {
        let mut pending = vec![start_transfer_uid.to_string()];
        let mut seen = HashSet::new();
        while let Some(transfer_uid) = pending.pop() {
            if transfer_uid == target_transfer_uid {
                return Ok(true);
            }
            if !seen.insert(transfer_uid.clone()) {
                continue;
            }
            let Some(transfer) = store::transfers::get(&self.store.pool, &transfer_uid).await?
            else {
                continue;
            };
            if transfer.revision <= 0 {
                continue;
            }
            let Some(fact) = store::transfers::revision_fact(
                &self.store.pool,
                &transfer_uid,
                transfer.revision as u64,
            )
            .await?
            else {
                continue;
            };
            let Some(payload) = fact.payload.as_deref() else {
                continue;
            };
            let Ok(evidence) =
                serde_json::from_str::<nucleus::transfer::TransferRevisionEvidence>(payload)
            else {
                continue;
            };
            for dependency in evidence.terms.dependencies {
                match dependency.upstream_kind {
                    nucleus::transfer::TransferDependencyUpstreamKind::Transfer => {
                        pending.push(dependency.upstream_uid);
                    }
                    nucleus::transfer::TransferDependencyUpstreamKind::Promise => {
                        if let Some(promise) =
                            store::misc::get_promise(&self.store.pool, &dependency.upstream_uid)
                                .await?
                            && let Some(owner) = promise.transfer_uid
                        {
                            pending.push(owner);
                        }
                    }
                }
            }
        }
        Ok(false)
    }

    async fn resolve_whole_transfer_draft(
        &self,
        transfer_uid: String,
        expected_revision: u64,
        request_id: String,
        mut draft: TransferDraftRevisionInput,
        creator_person_uid: String,
        proposal_author_person_uid: String,
        now: DateTime<Utc>,
        preserve_invitation_lifecycle: bool,
        actor: Option<&str>,
    ) -> Result<store::transfers::WholeDraftRevisionInput, EngineError> {
        let transfer = store::transfers::get(&self.store.pool, &transfer_uid)
            .await?
            .ok_or_else(|| EngineError::UnknownRecord(transfer_uid.clone()))?;
        let existing_promises =
            store::transfers::promises_of(&self.store.pool, &transfer_uid).await?;
        let mut preserve_private_terms = false;
        if proposal_author_person_uid != creator_person_uid {
            for promise in &existing_promises {
                if let Some(item) =
                    store::transfers::item_of(&self.store.pool, &promise.uid).await?
                {
                    let access = nucleus::transfer::disclosure::ItemAccess::for_item(
                        Some(&item),
                        Some(&proposal_author_person_uid),
                        promise.party_uid.as_deref(),
                        true,
                        false,
                    );
                    preserve_private_terms = true;
                    if !access.location {
                        draft.default_place =
                            transfer
                                .default_place
                                .as_ref()
                                .map(|place| TransferPlaceInput {
                                    lat: place.lat,
                                    lon: place.lon,
                                    address: place.address.clone(),
                                });
                    }
                }
            }
            if preserve_private_terms {
                draft.source = transfer.source_uid.clone();
                draft.parent = transfer.parent_uid.clone();
            }
        }
        let dependency_inputs = draft.dependencies;
        let request_id = request_id.trim().to_string();
        if request_id.is_empty() || request_id.chars().count() > 200 {
            return Err(EngineError::Consequence(
                "transfer request_id must contain 1 to 200 characters".into(),
            ));
        }
        let head = draft.head.trim().to_string();
        if head.is_empty() || head.chars().count() > 200 {
            return Err(EngineError::Consequence(
                "transfer title must contain 1 to 200 characters".into(),
            ));
        }
        let slug = draft
            .slug
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        if let Some(value) = slug.as_deref()
            && !nucleus::valid_slug(value)
        {
            return Err(EngineError::Consequence(format!(
                "invalid transfer slug `{value}`"
            )));
        }
        match draft.agreement {
            nucleus::transfer::AgreementType::Percentage => {
                if !draft
                    .agreement_pct
                    .is_some_and(|percentage| (1..=100).contains(&percentage))
                {
                    return Err(EngineError::Consequence(
                        "percentage agreement requires a threshold from 1 to 100".into(),
                    ));
                }
            }
            _ if draft.agreement_pct.is_some() => {
                return Err(EngineError::Consequence(
                    "agreement_pct is valid only for percentage agreement".into(),
                ));
            }
            _ => {}
        }
        match draft.visibility {
            TransferVisibility::Proximity => {
                if !draft.max_proximity.is_some_and(|value| value > 0) {
                    return Err(EngineError::Consequence(
                        "proximity visibility requires max_proximity greater than zero".into(),
                    ));
                }
            }
            _ if draft.max_proximity.is_some() => {
                return Err(EngineError::Consequence(
                    "max_proximity is valid only for proximity visibility".into(),
                ));
            }
            _ => {}
        }
        let parent_uid = match draft
            .parent
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            Some(token) => {
                let uid = if preserve_private_terms {
                    token.to_string()
                } else {
                    self.resolve_transfer_visible_record(token, actor).await?
                };
                if uid == transfer_uid {
                    return Err(EngineError::Consequence(
                        "a transfer cannot be its own parent".into(),
                    ));
                }
                let record = store::records::get(&self.store.pool, &uid)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(uid.clone()))?;
                if record.kind != RecordKind::Transfer.as_str() {
                    return Err(EngineError::Consequence(
                        "a transfer parent must be another transfer".into(),
                    ));
                }
                Some(uid)
            }
            None => None,
        };
        let source_uid = match draft
            .source
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            Some(token) => Some(if preserve_private_terms {
                token.to_string()
            } else {
                self.resolve_transfer_visible_record(token, actor).await?
            }),
            None => None,
        };
        if matches!(draft.satiation, TransferSatiation::FirstCompletes) && source_uid.is_none() {
            return Err(EngineError::Consequence(
                "first_completes requires a shared source record".into(),
            ));
        }

        let invitations =
            store::transfers::invitations_for_transfer(&self.store.pool, &transfer_uid).await?;
        if preserve_private_terms && draft.invitees.is_empty() {
            draft.invitees = invitations
                .iter()
                .filter(|invitation| {
                    invitation.status == store::transfers::TransferInvitationStatus::Pending
                })
                .map(|invitation| invitation.uid.clone())
                .collect();
        }
        let mut retained_invitation_uids = Vec::with_capacity(draft.invitees.len());
        let mut retained_people = std::collections::HashSet::new();
        for token in draft.invitees {
            let token = token.trim();
            let invitation = invitations.iter().find(|invitation| {
                invitation.status == store::transfers::TransferInvitationStatus::Pending
                    && (invitation.uid == token || invitation.addressed_person_uid == token)
            });
            let Some(invitation) = invitation else {
                return Err(EngineError::Conflict {
                    code: "transfer_phase_2_invitation_required",
                    message: "Phase 1 may retain or withdraw existing invitations but cannot address a new Person"
                        .into(),
                });
            };
            if !retained_people.insert(invitation.addressed_person_uid.clone()) {
                return Err(EngineError::Consequence(
                    "the same pending invitee cannot appear twice".into(),
                ));
            }
            retained_invitation_uids.push(invitation.uid.clone());
        }
        if preserve_invitation_lifecycle {
            let pending_uids = invitations
                .iter()
                .filter(|invitation| {
                    invitation.status == store::transfers::TransferInvitationStatus::Pending
                })
                .map(|invitation| invitation.uid.as_str())
                .collect::<std::collections::HashSet<_>>();
            let submitted_uids = retained_invitation_uids
                .iter()
                .map(String::as_str)
                .collect::<std::collections::HashSet<_>>();
            if submitted_uids != pending_uids {
                return Err(EngineError::Conflict {
                    code: "transfer_counteroffer_invitation_lifecycle_immutable",
                    message: "a counteroffer cannot address, withdraw, or reopen invitations"
                        .into(),
                });
            }
        }

        let effective_reserve_default = draft
            .reserve_default
            .resolve(self.transfer_reservation_cell_default().await?);
        let mut promises = Vec::with_capacity(draft.promises.len());
        for mut input in draft.promises {
            if input.withdrawn {
                if input.uid.is_none() {
                    return Err(EngineError::Consequence(
                        "a new promise cannot already be withdrawn".into(),
                    ));
                }
                continue;
            }
            let existing_promise = input
                .uid
                .as_deref()
                .and_then(|uid| existing_promises.iter().find(|promise| promise.uid == uid));
            let private_source = if proposal_author_person_uid != creator_person_uid
                && let Some(existing) = existing_promise
                && let Some(previous) =
                    store::transfers::item_of(&self.store.pool, &existing.uid).await?
            {
                let access = nucleus::transfer::disclosure::ItemAccess::for_item(
                    Some(&previous),
                    Some(&proposal_author_person_uid),
                    existing.party_uid.as_deref(),
                    true,
                    false,
                );
                let mut revised = input.item.unwrap_or_else(|| previous.clone());
                revised.disclosure = previous.disclosure.clone();
                if !access.title {
                    revised.title = previous.title;
                }
                if !access.description {
                    revised.description = previous.description;
                }
                input.item = Some(revised);
                if !access.quantity {
                    input.delta = existing.delta;
                    input.unit = existing.unit_uid.clone();
                    input.party = input.item.as_ref().and_then(|item| item.exchange.as_ref())
                        .map(|exchange| exchange.owner(existing.delta).to_string())
                        .or_else(|| existing.party_uid.clone());
                }
                if !access.parties {
                    input.item.as_mut().unwrap().exchange = previous.exchange;
                    input.party = existing.party_uid.clone();
                    input.open = existing.state == PromiseState::Open;
                }
                if !access.location {
                    input.place = existing.location.as_ref().map(|place| TransferPlaceInput {
                        lat: place.lat,
                        lon: place.lon,
                        address: place.address.clone(),
                    });
                }
                input.condition = existing.condition.clone();
                Some((existing.record_uid.clone(), existing.concept_uid.clone()))
            } else {
                None
            };
            if !input.delta.is_finite() || input.delta == 0.0 {
                return Err(EngineError::Consequence(
                    "every promise delta must be finite and non-zero".into(),
                ));
            }
            let item = self.resolve_transfer_item(input.item, &proposal_author_person_uid).await?;
            let (record_uid, concept_uid) = match private_source {
                Some(source) => source,
                None => {
                    self.resolve_transfer_item_source(&input.record, item.is_some(), actor)
                        .await?
                }
            };
            if existing_promise.is_some_and(|promise| promise.state == PromiseState::Open)
                && !input.open
            {
                return Err(EngineError::Conflict {
                    code: "transfer_open_claim_action_required",
                    message:
                        "an OPEN proposal can become concrete only through a signed OPEN claim"
                            .into(),
                });
            }
            let person_uid = if input.open {
                let proposer = existing_promise
                    .filter(|promise| promise.state == PromiseState::Open)
                    .and_then(|promise| promise.party_uid.clone())
                    .unwrap_or_else(|| proposal_author_person_uid.clone());
                if let Some(token) = input.party.as_deref() {
                    let submitted = self.resolve(token.trim()).await?;
                    if submitted != proposer {
                        return Err(EngineError::Conflict {
                            code: "transfer_open_proposer_immutable",
                            message: "a retained OPEN proposal keeps its original proposer Person"
                                .into(),
                        });
                    }
                }
                Some(proposer)
            } else {
                let token = input.party.as_deref().ok_or_else(|| {
                    EngineError::Consequence("a non-OPEN promise requires a reviewed Person".into())
                })?;
                let person_uid = self.resolve(token.trim()).await?;
                let record = store::records::get(&self.store.pool, &person_uid)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(person_uid.clone()))?;
                if record.kind != RecordKind::Person.as_str() {
                    return Err(EngineError::Consequence(
                        "promise Person must be a Person record".into(),
                    ));
                }
                let is_participant =
                    store::transfers::party_for_actor(&self.store.pool, &transfer_uid, &person_uid)
                        .await?
                        .is_some();
                let is_pending = invitations.iter().any(|invitation| {
                    invitation.status == store::transfers::TransferInvitationStatus::Pending
                        && retained_invitation_uids.contains(&invitation.uid)
                        && invitation.addressed_person_uid == person_uid
                });
                if !is_participant && !is_pending {
                    return Err(EngineError::Consequence(
                        "promise Person must be a participant or retained pending invitee".into(),
                    ));
                }
                Some(person_uid)
            };
            if !input.open
                && input.reuse_policy == nucleus::transfer::OpenPromiseReusePolicy::Consume
            {
                return Err(EngineError::Consequence(
                    "reuse_policy applies only to an OPEN promise".into(),
                ));
            }
            let unit_uid = self.resolve_concept_opt(input.unit).await?;
            let preserved_window_end = input.uid.as_deref().and_then(|uid| {
                existing_promises
                    .iter()
                    .find(|promise| promise.uid == uid)
                    .and_then(|promise| promise.window_end.as_deref())
            });
            let (window_start, window_end) = normalize_transfer_window(
                input.window_start,
                input.window_end,
                now,
                preserved_window_end,
            )?;
            let condition = input
                .condition
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty());
            if let Some(value) = condition.as_deref() {
                nucleus::expr::Expr::parse(value).map_err(|error| {
                    EngineError::Consequence(format!("invalid promise condition: {error}"))
                })?;
            }
            promises.push(store::transfers::DraftPromiseRevisionInput {
                uid: Some(input.uid.unwrap_or_else(|| nucleus::new_uid("p"))),
                item,
                source_promise_uid: None,
                record_uid,
                concept_uid,
                unit_uid,
                person_uid,
                open: input.open,
                delta: input.delta,
                window_start,
                window_end,
                location: normalize_transfer_place(input.place)?,
                condition,
                reserve_from: input
                    .reserve_from
                    .unwrap_or(effective_reserve_default)
                    .resolve(effective_reserve_default)
                    .as_str()
                    .into(),
                open_reuse_policy: input.reuse_policy,
            });
        }
        if promises.is_empty() && !retained_invitation_uids.is_empty() {
            return Err(EngineError::Consequence(
                "complete draft withdrawal requires withdrawing every pending invitation".into(),
            ));
        }
        let promise_uids = promises
            .iter()
            .filter_map(|promise| promise.uid.clone())
            .collect::<HashSet<_>>();
        let dependencies = if preserve_private_terms && dependency_inputs.is_empty() {
            store::transfers::dependencies_of(&self.store.pool, &transfer_uid)
                .await?
                .into_iter()
                .map(|dependency| store::transfers::TransferDependencyInput {
                    uid: Some(dependency.uid),
                    scope: dependency.scope,
                    promise_uid: dependency.promise_uid,
                    upstream_kind: dependency.upstream_kind,
                    upstream_uid: dependency.upstream_uid,
                    required_state: dependency.required_state,
                })
                .collect()
        } else {
            self.resolve_transfer_dependencies(
                Some(&transfer_uid),
                dependency_inputs,
                &promise_uids,
                actor,
            )
            .await?
        };
        if !promises.is_empty()
            && matches!(
                draft.agreement,
                nucleus::transfer::AgreementType::Dependency
            )
            && dependencies.is_empty()
        {
            return Err(EngineError::Consequence(
                "dependency agreement requires at least one structured dependency".into(),
            ));
        }
        Ok(store::transfers::WholeDraftRevisionInput {
            transfer_uid,
            expected_revision,
            idempotency_key: request_id,
            creator_person_uid,
            proposal_author_person_uid,
            terms: store::transfers::TransferDraftTermsInput {
                slug,
                head,
                agreement_type: draft.agreement.as_str().into(),
                agreement_pct: draft.agreement_pct.map(i64::from),
                settlement: transfer.settlement,
                visibility: draft.visibility.as_str().into(),
                max_proximity: draft.max_proximity.map(i64::from),
                satiation: draft.satiation.as_option(),
                parent_uid,
                source_uid,
                reserve_default: effective_reserve_default.as_str().into(),
                require_confirmation: draft.require_confirmation,
                default_place: normalize_transfer_place(draft.default_place)?,
            },
            retained_invitation_uids,
            promises,
            dependencies,
            successor: None,
            authorization_intent_uid: None,
        })
    }

    async fn resolve_transfer_draft_creator_person(
        &self,
        transfer_uid: &str,
        token: &str,
        expected_revision: u64,
        actor: Option<&str>,
    ) -> Result<String, EngineError> {
        let token = token.trim();
        if token.is_empty() {
            return Err(EngineError::Consequence(
                "the complete transfer draft must identify its creator Person".into(),
            ));
        }
        let person_uid = self.resolve(token).await?;
        let record = store::records::get(&self.store.pool, &person_uid)
            .await?
            .ok_or_else(|| EngineError::UnknownRecord(person_uid.clone()))?;
        if record.kind != RecordKind::Person.as_str() {
            return Err(EngineError::Consequence(
                "transfer creator must be a Person record".into(),
            ));
        }
        if store::transfers::party_for_actor(&self.store.pool, transfer_uid, &person_uid)
            .await?
            .is_none()
        {
            return Err(EngineError::Consequence(
                "transfer creator must already be a transfer party".into(),
            ));
        }
        if let Some(mapped) = self.actor_person(actor).await?
            && mapped != person_uid
        {
            return Err(EngineError::Forbidden(
                "authenticated creator identity is derived from the session".into(),
            ));
        }
        if expected_revision > 0 {
            let existing = store::transfers::creator_party_actor(&self.store.pool, transfer_uid)
                .await?
                .ok_or_else(|| EngineError::Conflict {
                    code: "transfer_creator_evidence_missing",
                    message: "revisioned transfer has no signed creator Person marker".into(),
                })?;
            if existing != person_uid {
                return Err(EngineError::Conflict {
                    code: "transfer_creator_immutable",
                    message: "transfer creator Person cannot change in a revision".into(),
                });
            }
        }
        Ok(person_uid)
    }

    pub(crate) async fn apply_transfer_revision_commit(
        &self,
        commit: store::transfers::RevisionCommit,
        expected_revision: u64,
        transfer_uid: &str,
        outcome: &mut ActionOutcome,
    ) -> Result<(), EngineError> {
        match commit {
            store::transfers::RevisionCommit::Committed { fact, parent_facts, .. } => {
                outcome.facts = self.publish_committed_fact(fact);
                for parent in parent_facts { outcome.facts.extend(self.publish_committed_fact(parent)); }
            }
            store::transfers::RevisionCommit::Replayed { .. } => {}
            store::transfers::RevisionCommit::Stale { current_revision } => {
                return Err(EngineError::Conflict {
                    code: "transfer_revision_stale",
                    message: format!(
                        "expected transfer revision {expected_revision}, current revision is {current_revision}"
                    ),
                });
            }
        }
        outcome.created = Some(transfer_uid.to_string());
        Ok(())
    }

    fn apply_transfer_invitation_commit(
        &self,
        commit: store::transfers::InvitationCommit,
        expected_revision: u64,
        outcome: &mut ActionOutcome,
    ) -> Result<(), EngineError> {
        match commit {
            store::transfers::InvitationCommit::Committed(committed) => {
                if let Some(fact) = committed.revision_fact {
                    outcome.facts.extend(self.publish_committed_fact(fact));
                }
                outcome
                    .facts
                    .extend(self.publish_committed_fact(committed.event_fact));
                outcome.created = Some(committed.invitation.uid);
            }
            store::transfers::InvitationCommit::Replayed(replayed) => {
                outcome.created = Some(replayed.invitation.uid);
            }
            store::transfers::InvitationCommit::Stale {
                current_revision, ..
            } => {
                return Err(EngineError::Conflict {
                    code: "transfer_revision_stale",
                    message: format!(
                        "expected transfer revision {expected_revision}, current revision is {current_revision}"
                    ),
                });
            }
        }
        Ok(())
    }

    fn apply_open_promise_claim_commit(
        &self,
        commit: store::transfers::OpenPromiseClaimCommit,
        expected_revision: u64,
        outcome: &mut ActionOutcome,
    ) -> Result<(), EngineError> {
        match commit {
            store::transfers::OpenPromiseClaimCommit::Committed(committed) => {
                outcome.facts = self.publish_committed_fact(committed.fact);
                outcome.created = Some(committed.promise_uid);
            }
            store::transfers::OpenPromiseClaimCommit::Replayed(replayed) => {
                outcome.created = Some(replayed.promise_uid);
            }
            store::transfers::OpenPromiseClaimCommit::Stale {
                current_revision, ..
            } => {
                return Err(EngineError::Conflict {
                    code: "transfer_revision_stale",
                    message: format!(
                        "expected transfer revision {expected_revision}, current revision is {current_revision}"
                    ),
                });
            }
        }
        Ok(())
    }

    pub(crate) async fn reject_direct_transfer_record_mutation(
        &self,
        record_uid: &str,
    ) -> Result<(), EngineError> {
        if store::transfers::get(&self.store.pool, record_uid)
            .await?
            .is_some()
        {
            return Err(EngineError::Conflict {
                code: "transfer_revision_required",
                message: "Transfer records may only change through revision-safe Transfer Actions"
                    .into(),
            });
        }
        Ok(())
    }

    async fn append_transfer_delivery_evidence(
        &self,
        transfer_uid: &str,
        actor_person_uid: &str,
        request_id: &str,
        operation: &str,
        payload: serde_json::Value,
        now: DateTime<Utc>,
        verified_authorship: Option<&VerifiedActionAuthorship>,
    ) -> Result<(String, Vec<Fact>), EngineError> {
        let signer = self
            .transfer_person_signer(actor_person_uid, verified_authorship)
            .await?;
        let fact_uid = format!("tdf:{operation}:{request_id}");
        let new = NewFact {
            uid: Some(fact_uid.clone()),
            record_uid: transfer_uid.to_string(),
            delta: nucleus::fact::zero_delta(),
            at: None,
            actor_uid: Some(actor_person_uid.to_string()),
            cause: Cause::user_edit(),
            payload: Some(payload.to_string()),
        };
        let Some(fact) = crate::append::append_one(&self.store, new, now, signer.as_ref()).await?
        else {
            let existing = store::facts::get(&self.store.pool, &fact_uid)
                .await?
                .ok_or_else(|| EngineError::UnknownRecord(fact_uid.clone()))?;
            if existing.record_uid != transfer_uid
                || existing.actor_uid.as_deref() != Some(actor_person_uid)
                || existing.payload.as_deref() != Some(payload.to_string().as_str())
            {
                return Err(transfer_request_id_conflict());
            }
            return Ok((fact_uid, Vec::new()));
        };
        let facts = self.observe_committed_fact(fact, now).await?;
        Ok((fact_uid, facts))
    }

    async fn require_transfer_editor(
        &self,
        transfer_uid: &str,
        actor: Option<&str>,
    ) -> Result<(), EngineError> {
        self.require_transfer_origin_authority(transfer_uid).await?;
        let Some(actor) = actor else {
            return Ok(());
        };
        self.require_permission(Some(actor), "transfer:update")
            .await?;
        let person = self
            .actor_person(Some(actor))
            .await?
            .expect("authenticated actor");
        if store::facts::creator_uid(&self.store.pool, transfer_uid)
            .await?
            .as_deref()
            == Some(actor)
        {
            return Ok(());
        }
        if store::transfers::party_for_actor(&self.store.pool, transfer_uid, &person)
            .await?
            .is_some()
        {
            return Ok(());
        }
        Err(EngineError::Forbidden(
            "transfer update requires its creator or a participant".into(),
        ))
    }

    async fn require_transfer_participant(
        &self,
        transfer_uid: &str,
        actor: Option<&str>,
    ) -> Result<(), EngineError> {
        self.require_transfer_origin_authority(transfer_uid).await?;
        let Some(actor) = actor else {
            return Ok(());
        };
        self.require_permission(Some(actor), "transfer:update")
            .await?;
        let person = self
            .actor_person(Some(actor))
            .await?
            .expect("authenticated actor");
        if store::transfers::party_for_actor(&self.store.pool, transfer_uid, &person)
            .await?
            .is_some()
        {
            return Ok(());
        }
        Err(EngineError::Forbidden(
            "transfer action requires a participant".into(),
        ))
    }

    async fn require_transfer_person(
        &self,
        transfer_uid: &str,
        person_uid: &str,
        actor: Option<&str>,
    ) -> Result<(), EngineError> {
        self.require_transfer_origin_authority(transfer_uid).await?;
        let Some(actor) = actor else {
            return Ok(());
        };
        self.require_permission(Some(actor), "transfer:update")
            .await?;
        let expected = self
            .actor_person(Some(actor))
            .await?
            .expect("authenticated actor");
        if expected == person_uid
            && store::transfers::party_for_actor(&self.store.pool, transfer_uid, person_uid)
                .await?
                .is_some()
        {
            return Ok(());
        }
        Err(EngineError::Forbidden(
            "cannot settle as another transfer participant".into(),
        ))
    }

    async fn check_delete_permission(
        &self,
        record_uid: &str,
        actor: Option<&str>,
    ) -> Result<(), EngineError> {
        self.social_require_safe_generic_deletion(record_uid).await?;
        let Some(actor) = actor else {
            return Ok(());
        };
        let user = self.actor_user(actor).await?;
        if user.permissions.iter().any(|p| p == "record:delete") {
            return Ok(());
        }
        if user.permissions.iter().any(|p| p == "record:delete_own") {
            let creator = store::facts::creator_uid(&self.store.pool, record_uid).await?;
            if creator.as_deref() == Some(actor) {
                return Ok(());
            }
        }
        Err(EngineError::Forbidden(
            "missing record:delete or record:delete_own permission".into(),
        ))
    }

    fn generic_write_permission(action: &Action) -> Option<&'static str> {
        Some(match action {
            Action::Social { request } => request.permission(),
            Action::DiscardTransferDraft { .. } => "transfer:update",
            Action::InspectFioteActivations { .. } => "record:read",
            Action::CreateRecord { .. }
            | Action::CreateCustomComponent { .. }
            | Action::CreateRecordDraft { .. }
            | Action::CreateRecordWithTags { .. }
            | Action::CreateAgent { .. }
            | Action::CreateMessageDraft { .. }
            | Action::SendMessageDraft { .. }
            | Action::ImportInstinct | Action::ImportKarmaHabit { .. } => "record:create",
            Action::SetQuantity { .. }
            | Action::PreviewAreaTransition { .. }
            | Action::ApplyAreaTransition { .. }
            | Action::SetQuantityExact { .. }
            | Action::TransitionRecord { .. }
            | Action::AddQuantityExact { .. }
            | Action::AddQuantityGroupExact { .. }
            | Action::SetRecordStockLimit { .. }
            | Action::AddQuantity { .. }
            | Action::CaptureEntry { .. }
            | Action::ReviseEntry { .. }
            | Action::VoidEntry { .. }
            | Action::ClassifyFact { .. }
            | Action::Activate { .. }
            | Action::Deactivate { .. }
            | Action::EditRecordText { .. }
            | Action::PresentComponent { .. }
            | Action::ActivateFiote { .. }
            | Action::ReportFioteChild { .. }
            | Action::ChangeRecord { .. }
            | Action::SetSlug { .. }
            | Action::SetUnit { .. }
            | Action::SetExtension { .. }
            | Action::ReviseMessage { .. }
            | Action::ReviseMessageDraft { .. }
            | Action::AssertRecord { .. }
            | Action::RetractAssertion { .. }
            | Action::RefineAssertion { .. }
            | Action::RetractRecord { .. }
            | Action::ConfigureFiote { .. }
            | Action::SetIdentity { .. }
            | Action::SetAssertionOrder { .. }
            | Action::SetPlace { .. }
            | Action::GrantVisibility { .. }
            | Action::SaveProtein { .. }
            | Action::AdoptConcepts { .. }
            | Action::DeclareEquivalence { .. }
            | Action::RenameConcept { .. }
            | Action::AdoptConcept { .. }
            | Action::RemoveConceptFromLingua { .. }
            | Action::AddConceptParent { .. }
            | Action::RemoveConceptParent { .. }
            | Action::RenameLingua { .. } => "record:update",
            Action::CreateThread { .. }
            | Action::CreateMessage { .. }
            | Action::SendRecordCopy { .. }
            | Action::CreateTransferThread { .. }
            | Action::CreateTransferMessage { .. }
            | Action::CreateConcept { .. }
            | Action::CreateLingua { .. }
            | Action::CreateSignal { .. }
            | Action::CreateMatchRule { .. } => "record:create",
            Action::DeleteConcept { .. }
            | Action::DeleteLingua { .. }
            | Action::DeleteConversation { .. } => "record:delete",

            Action::ProposeGroup { .. } => "record:create",
            Action::SetGroupPerson { .. } => "user:update",
            Action::RemoveGroupOrgan { .. } => "record:update",
            Action::PreviewKarmaReading { .. } | Action::PreviewKarmaProposal { .. } | Action::InspectKarmaRuleHistory { .. } | Action::ReadMessageAttachment { .. } => "record:read",
            Action::InspectTransferKarma { .. } | Action::InspectKarmaSchedules { .. } | Action::PreviewKarmaScheduleDates { .. } | Action::PreviewKarmaHabit { .. } => "frequency:read",
            Action::SaveKarmaSchedule { schedule: None, .. } => "frequency:create",
            Action::SaveKarmaSchedule { schedule: Some(_), .. } | Action::CancelKarmaSchedule { .. } | Action::RetryKarmaSchedule { .. } => "frequency:update",
            Action::SaveKarmaRule { rule: None, .. } | Action::CreateFrequency { .. } | Action::CreateRecurrence { .. } => "frequency:create",
            Action::SaveKarmaRule { rule: Some(_), .. } | Action::ReviseKarmaField { .. } => "frequency:update",
            Action::ReviseRecurrence { .. }
            | Action::SetRecurrencePaused { .. }
            | Action::ApplyRecurrenceOccurrence { .. }
            | Action::SkipRecurrenceOccurrence { .. }
            | Action::UnskipRecurrenceOccurrence { .. } => "frequency:update",
            Action::DeleteFrequency { .. } | Action::DeleteRecurrence { .. } => "frequency:delete",

            Action::AuditContact { .. }
            | Action::SyncNow
            | Action::RosterStatus
            | Action::MailboxStatus
            | Action::MailboxSavedStatus
            | Action::MailboxPickupPoints
            | Action::MailboxOutbound
            | Action::MailboxRequests
            | Action::FileSyncStatus { .. } => "organ:read",

            Action::MailboxRetrySaved { .. } | Action::MailboxSetCopies { .. } => "organ:update",
            Action::AddKnownOrgan { .. } => "organ:create",
            Action::RenameOrganContact { .. }
            | Action::SetSyncPolicy { .. }
            | Action::SetContactScope { .. }
            | Action::SetContactShare { .. }
            | Action::MoveRecordTo { .. }
            | Action::CancelRecordMove { .. }
            | Action::SetContactAcceptScope { .. }
            | Action::HideRecordFromContact { .. }
            | Action::ShareMyKey { .. }
            | Action::StartConversation { .. }
            | Action::OpenThread { .. }
            | Action::GrantOrganLogin { .. }
            | Action::RevokeOrganLogin { .. }
            | Action::AcceptThreadInvite { .. }
            | Action::DeclineThreadInvite { .. }
            | Action::RootKeyExport { .. }
            | Action::RootKeyDetach { .. }
            | Action::SetContactTrust { .. }
            | Action::SetContactProximity { .. }
            | Action::RosterCreateOrgan
            | Action::RosterSetKarmaExecution { .. }
            | Action::RosterRenameCell { .. }
            | Action::RosterEnrolToken
            | Action::SaveKarmaCommand { .. } | Action::RunKarmaCommand { .. } | Action::InspectKarmaCommands
            | Action::SetCellConfig { .. }
            | Action::MailboxCarryFor { .. }
            | Action::MailboxAddPickup { .. }
            | Action::MailboxRemovePickup { .. }
            | Action::MailboxCollectNow
            | Action::MailboxMailNow { .. }
            | Action::MailboxAnswerRequest { .. }
            | Action::MailboxIssueInvite { .. }
            | Action::MailboxAskCarry { .. }
            | Action::MailboxUseInvite { .. }
            | Action::RosterJoinOrgan { .. } => "organ:update",
            Action::ForgetOrganContact { .. }
            | Action::RosterRevokeCell { .. }
            | Action::MailboxStopCarrying { .. } => "organ:delete",

            Action::CreatePromise { .. }
            | Action::CreateTransferDraft { .. }
            | Action::CreateTransferRemainderDraft { .. } => "transfer:update",
            Action::PromiseTransition { .. }
            | Action::EditPromiseDelta { .. }
            | Action::Decide { .. }
            | Action::ReviseTransferPromise { .. }
            | Action::ReviseTransferDraft { .. }
            | Action::AdoptTransferDraft { .. }
            | Action::ConfigureTransferDelivery { .. }
            | Action::SetTransferDeliveryMode { .. }
            | Action::EnqueueTransferDelivery { .. }
            | Action::RetryTransferDelivery { .. }
            | Action::RevokeTransferDelivery { .. }
            | Action::DesignateTransferExecutor { .. }
            | Action::RefreshTransferDelivery { .. }
            | Action::BeginTransferSettlement { .. }
            | Action::CompensateTransferApplication { .. }
            | Action::SetTransferPrivateApplicationPolicy { .. }
            | Action::SetTransferChildRequirement { .. }
            | Action::ProposeTransferCancellation { .. }
            | Action::ProposeTransferLoanExtension { .. }
            | Action::ApplyTransferCancellation { .. }
            | Action::ApplyTransferApplication { .. }
            | Action::ConfirmTransfer { .. }
            | Action::AddParty { .. }
            | Action::AddPromiseToTransfer { .. }
            | Action::AgreeTransfer { .. }
            | Action::ActivateTransfer { .. }
            | Action::SettleTransfer { .. }
            | Action::Compensate { .. } => "transfer:update",

            Action::CreateKarmaProgram { .. } => "karma:create",
            Action::ReviseKarmaProgram { .. }
            | Action::ActivateKarmaProgram { .. }
            | Action::PauseKarmaProgram { .. }
            | Action::SetKarmaExecution { .. }
            | Action::DesignateKarmaExecutor { .. }
            | Action::RespondKarmaCandidate { .. }
            | Action::CreateKarmaFrequency { .. }
            | Action::SaveKarmaFrequency { .. }
            | Action::ReviseKarmaFrequency { .. }
            | Action::ActivateKarmaFrequency { .. }
            | Action::SetKarmaFrequencyParameters { .. }
            | Action::ResetKarmaFrequencyParameters { .. }
            | Action::PauseKarmaFrequency { .. } => "karma:update",

            Action::SetFileSyncEnabled { .. }
            | Action::ConfigureFileSync { .. }
            | Action::DeleteRecord { .. }
            | Action::DeleteMessageDraft { .. }
            | Action::CreateTransfer { .. }
            | Action::AddressTransferInvitation { .. }
            | Action::AcceptTransferInvitation { .. }
            | Action::RejectTransferInvitation { .. }
            | Action::WithdrawTransferInvitation { .. }
            | Action::ReopenTransferInvitation { .. }
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
            | Action::SetTransferOccurrenceApplicationFormula { .. }
            | Action::SettleTransferOccurrence { .. }
            | Action::CreateReversingTransferDraft { .. }
            | Action::ReopenTransferPromise { .. }
            | Action::CompensateTransferOccurrenceSettlement { .. }
            | Action::CreateRole { .. }
            | Action::RenameRole { .. }
            | Action::DeleteRole { .. }
            | Action::CreateUser { .. }
            | Action::UpdateUser { .. }
            | Action::DeleteUser { .. }
            | Action::AssignRole { .. }
            | Action::SetPersonStanding { .. }
            | Action::SetPersonReadFilter { .. }
            | Action::SetRoleReadRules { .. }
            | Action::GrantPermission { .. }
            | Action::RevokePermission { .. } => return None,
        })
    }

    pub(crate) async fn canonical_condition(
        &self,
        condition: Option<String>,
    ) -> Result<Option<String>, EngineError> {
        let Some(source) = condition else {
            return Ok(None);
        };
        let tokens = concept_tokens_in(&source);
        if tokens.is_empty() {
            return Ok(Some(source));
        }
        let mut rewritten = String::with_capacity(source.len());
        let mut copied = 0usize;
        for (start, end, name) in tokens {
            let uid = self.resolve_concept(&name).await?;
            rewritten.push_str(&source[copied..start]);
            rewritten.push('#');
            rewritten.push_str(&uid);
            copied = end;
        }
        rewritten.push_str(&source[copied..]);
        Ok(Some(rewritten))
    }

    pub(crate) async fn resolve_consequences(
        &self,
        declared: Vec<nucleus::karma::Consequence>,
    ) -> Result<nucleus::karma::Consequences, EngineError> {
        use nucleus::karma::Consequence;
        let mut resolved = Vec::with_capacity(declared.len());
        for consequence in declared {
            resolved.push(match consequence {
                Consequence::CaptureEntry { amount, concept } => Consequence::CaptureEntry {
                    amount,
                    concept: self.resolve_concept_opt(concept).await?,
                },
                Consequence::SetQuantityWhere { assertion, value } => {
                    Consequence::SetQuantityWhere {
                        assertion: self.resolve_concept(&assertion).await?,
                        value,
                    }
                }
                Consequence::SetConcept { concept } => Consequence::SetConcept {
                    concept: self.resolve_concept(&concept).await?,
                },
                Consequence::AddConcept { concept } => Consequence::AddConcept {
                    concept: self.resolve_concept(&concept).await?,
                },
                Consequence::RemoveConcept { concept } => Consequence::RemoveConcept {
                    concept: self.resolve_concept(&concept).await?,
                },
                Consequence::ShowComponent { component } => Consequence::ShowComponent { component: self.resolve_component(component).await? },
                mut other if other.transfer_target().is_some() => {
                    if let Some((transfer, person)) = other.transfer_references_mut() {
                        *transfer = self.resolve_karma_transfer(transfer).await?;
                        *person = self.resolve(person).await?;
                    }
                    other
                }
                other => other,
            });
        }
        nucleus::karma::Consequences::new(resolved).map_err(|error| EngineError::Conflict {
            code: "recurrence_consequences_invalid",
            message: error.to_string(),
        })
    }

    pub(crate) async fn evaluate_rule_condition(
        &self,
        condition: &store::recurrence::RuleCondition,
        at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<Option<nucleus::DecimalValue>, EngineError> {
        self.ask_condition(condition, at, now, 0).await
    }

    async fn ask_condition(
        &self,
        condition: &store::recurrence::RuleCondition,
        at: DateTime<Utc>,
        now: DateTime<Utc>,
        depth: usize,
    ) -> Result<Option<nucleus::DecimalValue>, EngineError> {
        let parsed = condition.parsed().map_err(|e| {
            EngineError::Conflict {
                code: "rule_condition_invalid",
                message: e.to_string(),
            }
        })?;

        let mut readings = GatheredReadings { values: Default::default(), demanded: None };
        let unreadable = |e: nucleus::karma::ConditionError| EngineError::Conflict {
            code: "rule_condition_unreadable",
            message: e.to_string(),
        };
        let computed = loop {
            match parsed.evaluate(&mut readings) {
                Ok(value) => break value,
                Err(error) => {
                    let Some(token) = readings.demanded.take() else { return Err(unreadable(error)) };
                    let value = self.read_for_condition(&token.func, &token.slug, token.dur_secs, at, now, depth).await;
                    crate::karma_history::reading(&token.func, &token.slug, token.dur_secs, depth, &value);
                    readings.values.insert(reading_key(&token.func, &token.slug, token.dur_secs), value?);
                }
            }
        };
        if depth == 0 {
            crate::karma_history::computed(computed);
        }
        let passed = condition.gate.passes(computed).map_err(unreadable)?;
        let carried = passed.then(|| condition.carry.apply(computed));
        if depth == 0 {
            crate::karma_history::decision(computed, passed, carried);
        }
        Ok(carried)
    }

    async fn read_for_condition(
        &self,
        func: &str,
        slug: &str,
        window_secs: Option<i64>,
        at: DateTime<Utc>,
        now: DateTime<Utc>,
        depth: usize,
    ) -> Result<nucleus::DecimalValue, EngineError> {
        let zero =
            nucleus::DecimalValue::from_mantissa(0, 0).expect("scale zero is always constructible");
        if let Some((namespace, property)) = nucleus::expr::extension_parts(func) {
            let uid = self.resolve(slug).await?;
            return Ok(store::records::extension_number(&self.store.pool, &uid, &namespace, &property).await?);
        }
        if nucleus::transfer::karma::is_reading(func) {
            return self.transfer_condition_reading(func, slug, now.timestamp_millis()).await;
        }
        match func {
            "query_command" => self.query_command_reading(slug).await,
            "signal" => self.saved_command_reading(slug).await,
            "quantity" => {
                let uid = self.resolve(slug).await?;
                Ok(store::facts::level(&self.store.pool, &uid).await?)
            }
            nucleus::expr::ASSERTION => {
                let concept = self.resolve_concept(slug).await?;
                let mut total = zero;
                for uid in store::ledger::records_with_concept(&self.store.pool, &concept).await? {
                    let level = store::facts::level(&self.store.pool, &uid).await?;
                    total = total
                        .aligned_add(level)
                        .ok_or_else(|| EngineError::Conflict {
                            code: "rule_condition_unreadable",
                            message: format!("#{slug} adds up to more than fits"),
                        })?;
                }
                Ok(total)
            }
            "freq" => {
                let uid = self.resolve_frequency_uid(slug).await?;
                Ok(if crate::rule_runtime::frequency_pulse(&uid) {
                    nucleus::DecimalValue::from_mantissa(0, 1).expect("scale zero is valid")
                } else {
                    zero
                })
            }
            "value" => {
                let uid = self.resolve(slug).await?;
                Box::pin(self.derived_value(&uid, at, now, depth)).await
            }
            "sum" | "sum_pos" | "sum_neg" => {
                let uid = self.resolve(slug).await?;
                let seconds = window_secs.ok_or_else(|| EngineError::Conflict {
                    code: "rule_condition_invalid",
                    message: format!("{func}(@{slug}) needs a period, like 30d"),
                })?;
                Ok(match func {
                    "sum_pos" => {
                        store::facts::sum_pos_window(&self.store.pool, &uid, seconds, now).await?
                    }
                    "sum_neg" => {
                        store::facts::sum_neg_window(&self.store.pool, &uid, seconds, now).await?
                    }
                    _ => store::facts::sum_window(&self.store.pool, &uid, seconds, now).await?,
                })
            }
            "promise_state" => inexact(
                store::misc::promise_state(&self.store.pool, slug)
                    .await?
                    .map(nucleus::PromiseState::ordinal)
                    .unwrap_or(0.0),
            ),
            "hours_since_fact" => {
                let uid = self.resolve(slug).await?;
                inexact(
                    store::facts::hours_since_last(&self.store.pool, &uid, now)
                        .await?
                        .unwrap_or(1.0e9),
                )
            }
            "confidence" => inexact(crate::imagination::confidence(&self.store, slug).await?),
            "demand" => inexact(crate::imagination::demand(&self.store, slug, now).await?),
            "projected" => {
                let seconds = window_secs.ok_or_else(|| EngineError::Conflict {
                    code: "rule_condition_invalid",
                    message: format!("projected(@{slug}) needs a horizon, like 7d"),
                })?;
                let uid = self.resolve(slug).await?;
                let snapshot = crate::imagination::build_snapshot(&self.store, now).await?;
                let timeline = nucleus::imagination::project(
                    &snapshot,
                    now + chrono::TimeDelta::seconds(seconds),
                );
                inexact(timeline.projected(&uid).unwrap_or(0.0))
            }
            "distance" => {
                let mut places = Vec::new();
                for token in slug.split('|') {
                    let uid = self.resolve(token).await?;
                    let place = store::places::of_record(&self.store.pool, &uid)
                        .await?
                        .ok_or_else(|| EngineError::Conflict {
                            code: "rule_condition_invalid",
                            message: format!("`{token}` has no place"),
                        })?;
                    places.push(place);
                }
                if places.len() != 2 {
                    return Err(EngineError::Conflict {
                        code: "rule_condition_invalid",
                        message: "distance() needs exactly two @records".into(),
                    });
                }
                inexact(nucleus::place::distance(places[0], places[1]))
            }
            other => Err(EngineError::Conflict {
                code: "rule_condition_unknown_reading",
                message: format!(
                    "`{other}()` is not something a rule can read yet; available: \
                     quantity, signal, freq, value, sum, sum_pos, sum_neg, promise_state, \
                     hours_since_fact, confidence, demand, projected, distance"
                ),
            })
            .map(|_: ()| zero),
        }
    }

    pub(crate) async fn commit_outward_consequence(
        &self,
        rule: &store::recurrence::Recurrence,
        consequence: &nucleus::karma::Consequence,
        carried: Option<&nucleus::DecimalValue>,
        now: DateTime<Utc>,
    ) -> Result<(), EngineError> {
        let carried_number = carried.map(|value| value.to_f64()).unwrap_or(0.0);
        match consequence {
            nucleus::karma::Consequence::EmitPromise {
                delta,
                window_end,
                party,
            } => {
                let delta = delta
                    .as_ref()
                    .map(|value| value.to_f64())
                    .unwrap_or(carried_number);
                store::misc::insert_promise(
                    &self.store.pool,
                    store::misc::NewPromise {
                        record_uid: Some(rule.record_uid.clone()),
                        delta,
                        window_end: window_end.clone(),
                        party_uid: party.clone(),
                        state: Some(nucleus::PromiseState::Proposed),
                        rule_uid: Some(rule.uid.clone()),
                        ..Default::default()
                    },
                )
                .await?;
            }
            nucleus::karma::Consequence::Ask { question, options } => {
                let question = question
                    .clone()
                    .unwrap_or_else(|| format!("{}?", rule.note.as_deref().unwrap_or("this rule")));
                let offered = if options.is_empty() {
                    vec!["yes".to_string(), "no".to_string()]
                } else {
                    options.clone()
                };
                let offered = serde_json::Value::Array(
                    offered
                        .into_iter()
                        .map(|label| serde_json::json!({ "label": label }))
                        .collect(),
                );
                store::misc::create_decision(
                    &self.store.pool,
                    &rule.uid,
                    "ask",
                    &question,
                    &offered,
                )
                .await?;
            }
            nucleus::karma::Consequence::Notify { message } => {
                let message = message
                    .clone()
                    .unwrap_or_else(|| match rule.note.as_deref() {
                        Some(note) => note.to_string(),
                        None => "a rule fired".to_string(),
                    });
                self.queue_rule_effect(
                    rule,
                    "notify",
                    serde_json::json!({ "message": message, "carried": carried_number }),
                )
                .await?;
            }
            nucleus::karma::Consequence::InvokeCommand { command } => {
                let snapshot = self.command_snapshot(command, rule.actor_uid.as_deref()).await?;
                self.queue_rule_effect(rule, "saved-command", serde_json::json!({"saved_command":command,"command_snapshot":snapshot})).await?;
            }
            nucleus::karma::Consequence::RunCommand { command } => {
                self.queue_rule_effect(
                    rule,
                    "command",
                    serde_json::json!({ "command": command, "carried": carried_number }),
                )
                .await?;
            }
            nucleus::karma::Consequence::RunQuery { query, params } => {
                self.queue_rule_effect(
                    rule,
                    "query",
                    serde_json::json!({
                        "target": query,
                        "params": parse_effect_payload(params.as_deref(), "run-query")?,
                        "carried": carried_number,
                    }),
                )
                .await?;
            }
            nucleus::karma::Consequence::RunAction { action } => {
                self.queue_rule_effect(
                    rule,
                    "action",
                    serde_json::json!({
                        "action": parse_effect_payload(Some(action), "run-action")?,
                        "carried": carried_number,
                    }),
                )
                .await?;
            }
            nucleus::karma::Consequence::SetVisibility {
                subject_kind,
                subject,
            } => {
                store::visibility::grant(
                    &self.store.pool,
                    subject_kind,
                    subject.as_deref(),
                    &rule.record_uid,
                )
                .await?;
            }
            _ => {}
        }
        let _ = now;
        Ok(())
    }

    async fn queue_rule_effect(
        &self,
        rule: &store::recurrence::Recurrence,
        kind: &str,
        mut payload: serde_json::Value,
    ) -> Result<(), EngineError> {
        payload["actor"] = serde_json::json!(rule.actor_uid);
        payload["rule"] = serde_json::json!(rule.uid);
        payload["revision"] = serde_json::json!(rule.revision);
        if let Ok(occurrence) = crate::rule_runtime::EFFECT_OCCURRENCE.try_with(Clone::clone) {
            payload["occurrence"] = serde_json::to_value(&occurrence).map_err(EngineError::Json)?;
            let encoded = serde_json::to_string(&payload).map_err(EngineError::Json)?;
            let hash = nucleus::karma::canonical_hash("lince.karma-nested-effect.v1", &encoded).map_err(|error| EngineError::Consequence(error.to_string()))?;
            let mut tx = store::write_tx(&self.store.pool).await?;
            crate::rule_runtime::queue_effect_tx(&mut tx, &format!("{}:{}:{}:{kind}:{}", occurrence.event_id, rule.uid, rule.revision, hash.as_str()), kind, payload, &rule.record_uid, nucleus::execution::now()).await?;
            tx.commit().await?;
        } else {
            store::misc::queue_effect(&self.store.pool, kind, &payload, Some(&rule.record_uid)).await?;
        }
        self.effects_changed.send_modify(|revision| *revision = revision.wrapping_add(1));
        Ok(())
    }

    async fn derived_value(
        &self,
        record_uid: &str,
        at: DateTime<Utc>,
        now: DateTime<Utc>,
        depth: usize,
    ) -> Result<nucleus::DecimalValue, EngineError> {
        if depth >= VALUE_DEPTH_CAP {
            return Err(EngineError::Conflict {
                code: "rule_condition_cyclic",
                message: "these rules read each other in a circle".into(),
            });
        }
        let rule = store::recurrence::for_record(&self.store.pool, record_uid)
            .await?
            .into_iter()
            .find(|rule| rule.condition.is_some() && !rule.is_paused())
            .ok_or_else(|| EngineError::Conflict {
                code: "rule_condition_unknown_reading",
                message: format!("`{record_uid}` has no rule with a value to read"),
            })?;
        let condition = rule.condition.clone().expect("filtered on Some");
        let asked = store::recurrence::RuleCondition {
            source: condition.source,
            bindings: condition.bindings,
            gate: nucleus::karma::Gate::Always,
            carry: nucleus::karma::Carry::Value,
        };
        self.ask_condition(&asked, at, now, depth + 1)
            .await?
            .ok_or_else(|| EngineError::Conflict {
                code: "rule_condition_unreadable",
                message: "an always-gate cannot block".into(),
            })
    }



    async fn resolve_concept(&self, token: &str) -> Result<String, EngineError> {
        store::concepts::resolve(&self.store.pool, token.trim())
            .await?
            .ok_or_else(|| EngineError::UnknownRecord(token.to_string()))
    }

    async fn resolve_concept_opt(
        &self,
        token: Option<String>,
    ) -> Result<Option<String>, EngineError> {
        match token.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
            Some(t) => Ok(Some(
                store::concepts::resolve(&self.store.pool, t)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(t.to_string()))?,
            )),
            None => Ok(None),
        }
    }

    async fn annotate(
        &self,
        record_uid: String,
        actor: Option<String>,
        payload: serde_json::Value,
        now: chrono::DateTime<Utc>,
    ) -> Result<Vec<Fact>, EngineError> {
        self.append(
            NewFact {
                uid: None,
                record_uid,
                delta: nucleus::fact::zero_delta(),
                at: None,
                actor_uid: actor,
                cause: Cause::user_edit(),
                payload: Some(payload.to_string()),
            },
            now,
        )
        .await
    }

    async fn annotate_many(
        &self,
        record_uids: Vec<String>,
        actor: Option<String>,
        payload: serde_json::Value,
        now: chrono::DateTime<Utc>,
    ) -> Result<Vec<Fact>, EngineError> {
        let mut out = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for record_uid in record_uids {
            if !seen.insert(record_uid.clone()) {
                continue;
            }
            out.extend(
                self.annotate(record_uid, actor.clone(), payload.clone(), now)
                    .await?,
            );
        }
        Ok(out)
    }
}

async fn is_order_like(
    pool: &store::sqlx::SqlitePool,
    kind_uid: &str,
) -> Result<bool, EngineError> {
    const ORDER_NAMES: [&str; 3] = ["precedes", "before", "order"];
    for ancestor in store::concepts::ancestors_including(pool, kind_uid).await? {
        if let Some(name) = store::concepts::canonical_name(pool, &ancestor).await? {
            if ORDER_NAMES.contains(&name.as_str()) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

async fn kind_cycles(
    pool: &store::sqlx::SqlitePool,
    kind_uid: &str,
) -> Result<Vec<Vec<String>>, EngineError> {
    let edges = store::assertions::edges_of_predicate(pool, kind_uid).await?;
    let mut nodes: Vec<String> = Vec::new();
    let mut tuples: Vec<(String, String)> = Vec::with_capacity(edges.len());
    for edge in &edges {
        if !nodes.contains(&edge.from) {
            nodes.push(edge.from.clone());
        }
        if !nodes.contains(&edge.to) {
            nodes.push(edge.to.clone());
        }
        tuples.push((edge.from.clone(), edge.to.clone()));
    }
    Ok(nucleus::graph::cycles(&nodes, &tuples))
}

fn widens_scope(before: Option<&[String]>, after: Option<&[String]>) -> bool {
    match (before, after) {
        (None, _) => false,
        (Some(_), None) => true,
        (Some(before), Some(after)) => after.iter().any(|field| !before.contains(field)),
    }
}

fn validate_scope(fields: Option<&[String]>) -> Result<(), EngineError> {
    let Some(fields) = fields else {
        return Ok(());
    };
    if fields.iter().any(|f| f.trim().is_empty()) {
        return Err(EngineError::Consequence(
            "a scope cannot contain a blank column name".into(),
        ));
    }
    let head = fields.iter().any(|f| f == "head");
    let body = fields.iter().any(|f| f == "body");
    if head != body {
        return Err(EngineError::Consequence(
            "head and body are one collaborative document and cannot be separated — \
             name both or neither"
                .into(),
        ));
    }
    Ok(())
}

fn message_head(body: &str) -> String {
    let first = body
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or(body)
        .trim();
    let mut out = String::new();
    for ch in first.chars().take(72) {
        out.push(ch);
    }
    if out.is_empty() {
        "Message".into()
    } else {
        out
    }
}

fn message_draft_head(body: &str) -> String {
    let head = message_head(body);
    if head == "Message" {
        "Message draft".into()
    } else {
        head
    }
}

fn parse_effect_payload(
    text: Option<&str>,
    kind: &'static str,
) -> Result<serde_json::Value, EngineError> {
    let Some(text) = text.map(str::trim).filter(|text| !text.is_empty()) else {
        return Ok(serde_json::Value::Null);
    };
    serde_json::from_str(text).map_err(|error| EngineError::Conflict {
        code: "rule_consequence_payload_invalid",
        message: format!("`{kind}` payload is not readable JSON: {error}"),
    })
}

fn inexact(value: f64) -> Result<nucleus::DecimalValue, EngineError> {
    nucleus::DecimalValue::from_f64_lossy(value).map_err(|_| EngineError::Conflict {
        code: "rule_condition_unreadable",
        message: "that reading is not a number a rule can use".into(),
    })
}

impl Engine {
    pub(crate) async fn publish_karma_definition(
        &self,
        outcome: &ActionOutcome,
        kind: KarmaKind,
    ) -> Result<(), EngineError> {
        let Some(uid) = outcome.created.as_deref() else {
            return Ok(());
        };
        match kind {
            KarmaKind::Program => {
                store::karma::sync::publish_program(&self.store.pool, uid).await?;
            }
            KarmaKind::Frequency => {
                store::karma::sync::publish_frequency(&self.store.pool, uid).await?;
            }
        }
        Ok(())
    }
}
