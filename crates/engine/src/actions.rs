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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "kebab-case")]
pub enum Action {
    CreateRecord {
        slug: Option<String>,
        kind: RecordKind,
        head: String,
        #[serde(default)]
        body: String,
        #[serde(default)]
        quantity: f64,
    },
    SetQuantity {
        target: String,
        value: f64,
    },
    SetQuantityExact {
        target: String,
        amount: String,
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
    RosterRevokeCell {
        cell_uid: String,
    },
    RosterStatus,
    MailboxStatus,
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
    SetTransferOccurrenceApplicationFormula {
        occurrence: String,
        request_id: String,
        #[serde(default)]
        person: Option<String>,
        formula: String,
    },
    SettleTransferOccurrence {
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
    BeginRemoteTransferSettlement {
        transfer: String,
        occurrence: String,
        expected_revision: u64,
        expected_remaining_quantity: f64,
        canonical_quantity: f64,
        request_id: String,
        person: String,
    },
    ApplyRemoteTransferApplication {
        transfer: String,
        handoff: String,
        local_record: String,
        expected_formula_hash: String,
        expected_formula_version: u64,
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
        program_uid: String,
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
        #[serde(default)]
        authorizing_grant_uid: Option<String>,
    },
    CreateKarmaGrant {
        request_id: String,
        slug: nucleus::karma::Slug,
        grant: nucleus::karma::DelegationGrantSpec,
    },
    NarrowKarmaGrant {
        request_id: String,
        grant_uid: String,
        expected_handle_revision: u64,
        grant: nucleus::karma::DelegationGrantSpec,
    },
    ActivateKarmaGrant {
        request_id: String,
        grant_uid: String,
        expected_handle_revision: u64,
        revision_hash: CanonicalHash,
    },
    RevokeKarmaGrant {
        request_id: String,
        grant_uid: String,
        expected_handle_revision: u64,
    },
    CreateKarmaFrequency {
        request_id: String,
        frequency: FrequencyAst,
        #[serde(default)]
        owner_person_uid: Option<String>,
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
    CreateUser {
        username: String,
        name: String,
        password: String,
        role: String,
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
    GrantPermission {
        role: String,
        permission: String,
    },
    RevokePermission {
        role: String,
        permission: String,
    },
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferPromiseInput {
    #[serde(default)]
    pub uid: Option<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferDependencyScopeInput {
    Transfer,
    Promise,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferDependencyUpstreamKindInput {
    Transfer,
    Promise,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferOccurrenceClaimRole {
    Delivery,
    Receipt,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferOccurrenceBulkClaimInput {
    pub occurrence: String,
    pub transfer: String,
    pub expected_revision: u64,
    pub role: TransferOccurrenceClaimRole,
    pub expected_delivery_claimed: bool,
    pub expected_receipt_claimed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferPlaceInput {
    #[serde(default)]
    pub lat: Option<f64>,
    #[serde(default)]
    pub lon: Option<f64>,
    #[serde(default)]
    pub address: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

struct TransferApplicationFormulaResolver {
    incoming: f64,
}

impl nucleus::expr::Resolver for TransferApplicationFormulaResolver {
    fn call(
        &mut self,
        name: &str,
        args: &[nucleus::expr::Value],
    ) -> Result<nucleus::expr::Value, nucleus::error::NucleusError> {
        if name == "incoming" && args.is_empty() {
            return Ok(nucleus::expr::Value::Num(self.incoming));
        }
        Err(nucleus::error::NucleusError::Eval(format!(
            "application formula supports only incoming(), not {name}()"
        )))
    }
}

fn validate_transfer_application_formula(formula: &str) -> Result<String, EngineError> {
    let formula = formula.trim();
    if formula.is_empty() || formula.chars().count() > 2_000 {
        return Err(EngineError::Consequence(
            "application formula must contain 1 to 2000 characters".into(),
        ));
    }
    let expr = nucleus::expr::Expr::parse(formula).map_err(|error| {
        EngineError::Consequence(format!("invalid application formula: {error}"))
    })?;
    fn validate_node(expr: &nucleus::expr::Expr) -> Result<(), EngineError> {
        use nucleus::expr::{BinOp, Expr, UnOp};
        match expr {
            Expr::Num(text) if text.parse::<f64>().is_ok_and(f64::is_finite) => Ok(()),
            Expr::Fn(name, args) if name == "incoming" && args.is_empty() => Ok(()),
            Expr::Unary(UnOp::Neg, value) => validate_node(value),
            Expr::Bin(
                BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem,
                left,
                right,
            ) => {
                validate_node(left)?;
                validate_node(right)
            }
            _ => Err(EngineError::Consequence(
                "application formula permits only finite numbers, incoming(), and arithmetic"
                    .into(),
            )),
        }
    }
    validate_node(&expr)?;
    for incoming in [-1.0, 0.0, 1.0] {
        let value = expr
            .eval(&mut TransferApplicationFormulaResolver { incoming })
            .map_err(|error| {
                EngineError::Consequence(format!("invalid application formula: {error}"))
            })?;
        if !value.is_finite() {
            return Err(EngineError::Consequence(
                "application formula must produce a finite number".into(),
            ));
        }
    }
    Ok(formula.to_string())
}

fn evaluate_transfer_application_formula(formula: &str, incoming: f64) -> Result<f64, EngineError> {
    let expr = nucleus::expr::Expr::parse(formula).map_err(|error| {
        EngineError::Consequence(format!("invalid application formula: {error}"))
    })?;
    let value = expr
        .eval(&mut TransferApplicationFormulaResolver { incoming })
        .map_err(|error| {
            EngineError::Consequence(format!("invalid application formula: {error}"))
        })?;
    if !value.is_finite() {
        return Err(EngineError::Consequence(
            "application formula must produce a finite local delta for this occurrence".into(),
        ));
    }
    Ok(value)
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
    values: std::collections::HashMap<String, nucleus::DecimalValue>,
}

impl nucleus::karma::ExactResolver for GatheredReadings {
    fn lookup(
        &mut self,
        func: &str,
        slug: &str,
        window_secs: Option<i64>,
    ) -> Result<nucleus::DecimalValue, nucleus::karma::ConditionError> {
        self.values
            .get(&reading_key(func, slug, window_secs))
            .copied()
            .ok_or_else(|| nucleus::karma::ConditionError::UnknownReference(slug.to_string()))
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
    let scale = left.abs().max(right.abs()).max(1.0);
    (left - right).abs() <= scale * 1e-9
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
        store::karma::candidates::CandidateReviewCommit::Committed {
            state,
            fact,
            intent,
            intent_fact,
        } => {
            outcome.created = Some(
                intent
                    .map(|hash| hash.as_str().to_string())
                    .unwrap_or_else(|| state.candidate_hash.as_str().to_string()),
            );
            outcome.facts.push(fact);
            outcome.facts.extend(intent_fact);
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

pub(crate) fn apply_grant_mutation(
    commit: store::karma::grants::GrantMutationCommit,
    outcome: &mut ActionOutcome,
) -> Result<(), EngineError> {
    match commit {
        store::karma::grants::GrantMutationCommit::Committed { handle, fact } => {
            outcome.created = Some(handle.record_uid);
            outcome.facts.push(fact);
            Ok(())
        }
        store::karma::grants::GrantMutationCommit::Replayed { handle, .. } => {
            outcome.created = Some(handle.record_uid);
            Ok(())
        }
        store::karma::grants::GrantMutationCommit::Stale {
            current_handle_revision,
        } => Err(stale_karma_handle(current_handle_revision)),
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

pub(crate) struct VerifiedActionAuthorship {
    pub person_uid: String,
    pub intent_uid: String,
}

impl Engine {
    pub async fn act(
        &self,
        action: Action,
        actor: Option<String>,
    ) -> Result<ActionOutcome, EngineError> {
        self.act_at(action, actor, Utc::now()).await
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

    pub(crate) async fn act_at_with_authorship(
        &self,
        action: Action,
        actor: Option<String>,
        now: DateTime<Utc>,
        verified_authorship: Option<VerifiedActionAuthorship>,
    ) -> Result<ActionOutcome, EngineError> {
        if matches!(action, Action::ApplyRecurrenceOccurrence { .. }) && !crate::already_firing() {
            return Box::pin(crate::as_one_firing(self.act_at_inner(
                action,
                actor,
                now,
                verified_authorship,
            )))
            .await;
        }
        Box::pin(self.act_at_inner(action, actor, now, verified_authorship)).await
    }

    async fn act_at_inner(
        &self,
        action: Action,
        actor: Option<String>,
        now: DateTime<Utc>,
        verified_authorship: Option<VerifiedActionAuthorship>,
    ) -> Result<ActionOutcome, EngineError> {
        for transfer_uid in self.canonical_transfer_action_targets(&action).await? {
            self.require_transfer_origin_authority(&transfer_uid)
                .await?;
        }
        if let Some(permission) = Self::generic_write_permission(&action) {
            self.require_permission_lenient(actor.as_deref(), permission)
                .await?;
        }
        let touched = self.record_targets_of(&action).await?;
        self.refuse_unreadable(actor.as_deref(), &touched).await?;
        let mut outcome = ActionOutcome::default();
        match action {
            Action::CreateRecord {
                slug,
                kind,
                head,
                body,
                quantity,
            } => {
                let rec = store::records::create(
                    &self.store.pool,
                    store::records::NewRecord {
                        slug: slug.as_deref(),
                        kind,
                        head: &head,
                        body: &body,
                        quantity: store::exact::zero(),
                    },
                )
                .await?;
                outcome.facts = self
                    .append(
                        NewFact {
                            actor_uid: actor,
                            ..NewFact::quantity_f64(rec.uid.clone(), quantity, Cause::user_edit())
                        },
                        now,
                    )
                    .await?;
                outcome.created = Some(rec.uid);
            }
            Action::SetQuantityExact { target, amount } => {
                let uid = self.resolve(&target).await?;
                self.reject_direct_transfer_record_mutation(&uid).await?;
                let target_value =
                    nucleus::DecimalValue::parse_inferred(amount.trim()).map_err(|_| {
                        EngineError::Conflict {
                            code: "quantity_invalid",
                            message: format!("`{amount}` is not an exact decimal amount"),
                        }
                    })?;
                let current = store::records::quantity(&self.store.pool, &uid)
                    .await?
                    .unwrap_or_else(store::exact::zero);
                if target_value != current {
                    outcome.facts = self
                        .append(
                            NewFact {
                                actor_uid: actor,
                                ..NewFact::quantity(
                                    uid,
                                    store::exact::difference(target_value, current)?,
                                    Cause::user_edit(),
                                )
                            },
                            now,
                        )
                        .await?;
                }
            }
            Action::SetQuantity { target, value } => {
                let uid = self.resolve(&target).await?;
                self.reject_direct_transfer_record_mutation(&uid).await?;
                let current = store::records::quantity(&self.store.pool, &uid)
                    .await?
                    .unwrap_or_else(store::exact::zero);
                let target_value = store::exact::from_f64(value);
                if target_value != current {
                    outcome.facts = self
                        .append(
                            NewFact {
                                actor_uid: actor,
                                ..NewFact::quantity(
                                    uid,
                                    store::exact::difference(target_value, current)?,
                                    Cause::user_edit(),
                                )
                            },
                            now,
                        )
                        .await?;
                }
            }
            Action::TransitionRecord {
                subject,
                retract,
                assert,
                quantity,
            } => {
                let subject_uid = self.resolve(&subject).await?;
                self.reject_direct_transfer_record_mutation(&subject_uid)
                    .await?;
                let mut retract_uids = Vec::new();
                for concept in retract {
                    let uid = store::concepts::resolve(&self.store.pool, &concept)
                        .await?
                        .ok_or_else(|| EngineError::UnknownRecord(concept))?;
                    if !retract_uids.contains(&uid) {
                        retract_uids.push(uid);
                    }
                }
                let mut assert_uids = Vec::new();
                for concept in assert {
                    let uid = store::concepts::resolve(&self.store.pool, &concept)
                        .await?
                        .ok_or_else(|| EngineError::UnknownRecord(concept))?;
                    if !assert_uids.contains(&uid) {
                        assert_uids.push(uid);
                    }
                }
                retract_uids.retain(|uid| !assert_uids.contains(uid));

                let signer = self.signer.lock().await.clone();
                let mut tx = store::write_tx(&self.store.pool).await?;
                store::assertions::transition_unary(
                    &mut tx,
                    &subject_uid,
                    &retract_uids,
                    &assert_uids,
                    actor.as_deref(),
                )
                .await?;
                let fact = if let Some(value) = quantity {
                    let current = store::records::quantity_in_transaction(&mut tx, &subject_uid)
                        .await?
                        .ok_or_else(|| EngineError::UnknownRecord(subject_uid.clone()))?;
                    let target = store::exact::from_f64(value);
                    if target == current {
                        None
                    } else {
                        crate::append::append_one_in_transaction(
                            &mut tx,
                            NewFact::quantity(
                                subject_uid.clone(),
                                store::exact::difference(target, current)?,
                                Cause::user_edit(),
                            ),
                            now,
                            signer.as_ref(),
                        )
                        .await?
                    }
                } else {
                    None
                };
                tx.commit().await?;
                if let Some(fact) = fact {
                    outcome.facts = self.observe_committed_fact(fact, now).await?;
                }
            }
            Action::CaptureEntry {
                target,
                amount,
                concept,
                note,
                at,
                request_id,
            } => {
                let request_id = request_id
                    .map(|id| id.trim().to_string())
                    .filter(|id| !id.is_empty())
                    .unwrap_or_else(|| nucleus::new_uid("req"));
                if store::entries::replayed(&self.store.pool, &request_id)
                    .await?
                    .is_some()
                {
                    return Ok(outcome);
                }
                let uid = self.resolve(&target).await?;
                self.reject_direct_transfer_record_mutation(&uid).await?;
                let delta = nucleus::DecimalValue::parse_inferred(amount.trim()).map_err(|_| {
                    EngineError::Conflict {
                        code: "entry_amount_invalid",
                        message: format!("`{amount}` is not an exact decimal amount"),
                    }
                })?;
                let occurred_at = match at.as_deref() {
                    Some(text) => Some(
                        chrono::DateTime::parse_from_rfc3339(text)
                            .map_err(|_| EngineError::Conflict {
                                code: "entry_at_invalid",
                                message: format!("`{text}` is not an RFC3339 instant"),
                            })?
                            .with_timezone(&Utc),
                    ),
                    None => None,
                };
                let concept_uid = self.resolve_concept_opt(concept).await?;
                outcome.facts = self
                    .append(
                        NewFact {
                            actor_uid: actor,
                            at: occurred_at,
                            ..NewFact::quantity(uid, delta, Cause::user_edit())
                        },
                        now,
                    )
                    .await?;
                if let Some(fact) = outcome.facts.first() {
                    store::ledger::classify_fact(
                        &self.store.pool,
                        &fact.uid,
                        concept_uid.as_deref(),
                        fact.actor_uid.as_deref(),
                        note.as_deref(),
                    )
                    .await?;
                    let commit = store::entries::create(
                        &self.store.pool,
                        store::entries::NewEntry {
                            record_uid: &fact.record_uid,
                            amount: delta,
                            note: note.as_deref(),
                            occurred_at: fact.at,
                            fact_uid: &fact.uid,
                            request_id: &request_id,
                            actor_uid: fact.actor_uid.as_deref(),
                        },
                        now,
                    )
                    .await?;
                    outcome.created = Some(commit.entry().uid.clone());
                }
            }
            Action::CreateFrequency {
                slug,
                head,
                every,
                anchor_at,
                request_id,
            } => {
                let request_id = request_id
                    .map(|id| id.trim().to_string())
                    .filter(|id| !id.is_empty())
                    .unwrap_or_else(|| nucleus::new_uid("req"));
                let anchor = parse_optional_instant(anchor_at.as_deref())?.unwrap_or(now);
                let head = head.unwrap_or_default();
                let frequency = store::frequency::create(
                    &self.store.pool,
                    store::frequency::NewFrequency {
                        slug: &slug,
                        head: &head,
                        every,
                        anchor_at: anchor,
                        request_id: &request_id,
                        actor_uid: actor.as_deref(),
                    },
                    now,
                )
                .await
                .map_err(|error| EngineError::Conflict {
                    code: "frequency_invalid",
                    message: error.to_string(),
                })?;
                outcome.created = Some(frequency.uid);
            }
            Action::DeleteFrequency { frequency } => {
                let found = store::frequency::resolve(&self.store.pool, &frequency)
                    .await?
                    .ok_or_else(|| EngineError::Conflict {
                        code: "frequency_unknown",
                        message: format!("nothing here is called {frequency}"),
                    })?;
                store::frequency::delete(&self.store.pool, &found.uid)
                    .await
                    .map_err(|error| EngineError::Conflict {
                        code: "frequency_in_use",
                        message: error.to_string(),
                    })?;
            }
            Action::CreateRecurrence {
                target,
                consequences,
                condition,
                gate,
                carry,
                note,
                cadence,
                anchor_at,
                request_id,
            } => {
                let request_id = request_id
                    .map(|id| id.trim().to_string())
                    .filter(|id| !id.is_empty())
                    .unwrap_or_else(|| nucleus::new_uid("req"));
                let uid = self.resolve(&target).await?;
                let declared = self.resolve_consequences(consequences).await?;
                let condition = self.canonical_condition(condition).await?;
                let declared_condition = parse_rule_condition(condition, gate, carry)?;
                let anchor = parse_optional_instant(anchor_at.as_deref())?.unwrap_or(now);
                let commit = store::recurrence::create(
                    &self.store.pool,
                    store::recurrence::NewRecurrence {
                        record_uid: &uid,
                        consequences: declared,
                        condition: declared_condition,
                        note: note.as_deref(),
                        cadence,
                        anchor_at: anchor,
                        request_id: &request_id,
                        actor_uid: actor.as_deref(),
                    },
                    now,
                )
                .await
                .map_err(|error| EngineError::Conflict {
                    code: "recurrence_invalid",
                    message: error.to_string(),
                })?;
                outcome.created = Some(commit.rule().uid.clone());
            }
            Action::ReviseRecurrence {
                recurrence,
                expected_revision,
                request_id,
                consequences,
                condition,
                gate,
                carry,
                note,
                cadence,
                anchor_at,
            } => {
                let current = store::recurrence::get(&self.store.pool, &recurrence)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(recurrence.clone()))?;
                let declared = self.resolve_consequences(consequences).await?;
                let condition = self.canonical_condition(condition).await?;
                let declared_condition = parse_rule_condition(condition, gate, carry)?;
                let anchor = match parse_optional_instant(anchor_at.as_deref())? {
                    Some(value) => value,
                    None => parse_instant_field(&current.anchor_at)?,
                };
                store::recurrence::revise(
                    &self.store.pool,
                    store::recurrence::ReviseRecurrence {
                        recurrence_uid: &recurrence,
                        expected_revision,
                        consequences: declared,
                        condition: declared_condition,
                        note: note.as_deref(),
                        cadence,
                        anchor_at: anchor,
                        request_id: &request_id,
                        actor_uid: actor.as_deref(),
                    },
                    now,
                )
                .await
                .map_err(|error| EngineError::Conflict {
                    code: "recurrence_revision_stale",
                    message: error.to_string(),
                })?;
            }
            Action::DeleteRecurrence { recurrence } => {
                let uid = store::recurrence::get(&self.store.pool, &recurrence)
                    .await?
                    .map(|rule| rule.uid)
                    .ok_or_else(|| EngineError::UnknownRecord(recurrence.clone()))?;
                store::recurrence::delete(&self.store.pool, &uid).await?;
            }
            Action::SetRecurrencePaused {
                recurrence,
                expected_revision,
                request_id,
                paused,
            } => {
                store::recurrence::set_state(
                    &self.store.pool,
                    &recurrence,
                    expected_revision,
                    paused,
                    &request_id,
                    actor.as_deref(),
                    now,
                )
                .await
                .map_err(|error| EngineError::Conflict {
                    code: "recurrence_revision_stale",
                    message: error.to_string(),
                })?;
            }
            Action::ApplyRecurrenceOccurrence {
                recurrence,
                due_at,
                amount,
                note,
            } => {
                let rule = store::recurrence::get(&self.store.pool, &recurrence)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(recurrence.clone()))?;
                let due = parse_instant_field(&due_at)?;
                let produced = rule
                    .cadence
                    .between(
                        parse_instant_field(&rule.anchor_at)?,
                        due,
                        due + chrono::Duration::nanoseconds(1),
                    )
                    .map_err(|error| EngineError::Conflict {
                        code: "recurrence_cadence_invalid",
                        message: error.to_string(),
                    })?;
                if !produced.dates.contains(&due) {
                    return Err(EngineError::Conflict {
                        code: "recurrence_occurrence_unknown",
                        message: format!("`{due_at}` is not a date this rule produces"),
                    });
                }
                if let Some(existing) =
                    store::recurrence::applied(&self.store.pool, &rule.uid, due).await?
                {
                    outcome.created = Some(existing);
                    return Ok(outcome);
                }

                let carried = match rule.condition.as_ref() {
                    None => None,
                    Some(condition) => {
                        match self
                            .evaluate_rule_condition(&rule, condition, due, now)
                            .await?
                        {
                            None => return Ok(outcome),
                            Some(value) => Some(value),
                        }
                    }
                };

                let capture_concept = rule.consequences.capture_concept().map(str::to_string);
                let declared = match amount.as_deref() {
                    Some(text) => text.trim().to_string(),
                    None => match rule.consequences.capture_amount() {
                        None => "0".to_string(),
                        Some(declared) => match carried {
                            Some(value) => value.to_string(),
                            None => declared.to_string(),
                        },
                    },
                };
                let capture = Action::CaptureEntry {
                    target: rule.record_uid.clone(),
                    amount: declared,
                    concept: capture_concept,
                    note: note.or_else(|| rule.note.clone()),
                    at: Some(due.to_rfc3339()),
                    request_id: Some(store::recurrence::occurrence_request_id(&rule.uid, due)),
                };
                let applied =
                    Box::pin(self.act_at_with_authorship(capture, actor.clone(), now, None))
                        .await?;
                outcome.facts = applied.facts;
                outcome.created = applied.created;

                for consequence in rule.consequences.iter() {
                    let next = match consequence {
                        nucleus::karma::Consequence::CaptureEntry { .. } => continue,
                        nucleus::karma::Consequence::SetQuantity { value } => {
                            let Some(figure) = value.or(carried) else {
                                continue;
                            };
                            Action::SetQuantity {
                                target: rule.record_uid.clone(),
                                value: figure.to_f64(),
                            }
                        }
                        nucleus::karma::Consequence::SetQuantityWhere { assertion, value } => {
                            let Some(figure) = value.or(carried) else {
                                continue;
                            };
                            let concept = self.resolve_concept(assertion).await?;
                            let targets =
                                store::ledger::records_with_concept(&self.store.pool, &concept)
                                    .await?;
                            for target in targets {
                                let ran = Box::pin(self.act_at_with_authorship(
                                    Action::SetQuantity {
                                        target,
                                        value: figure.to_f64(),
                                    },
                                    actor.clone(),
                                    now,
                                    None,
                                ))
                                .await?;
                                outcome.facts.extend(ran.facts);
                            }
                            continue;
                        }
                        nucleus::karma::Consequence::AddQuantity { delta } => {
                            let Some(figure) = delta.or(carried) else {
                                continue;
                            };
                            Action::AddQuantity {
                                target: rule.record_uid.clone(),
                                delta: figure.to_f64(),
                            }
                        }
                        nucleus::karma::Consequence::SetConcept { concept } => {
                            Action::SetIdentity {
                                subject: rule.record_uid.clone(),
                                predicate: Some(concept.clone()),
                            }
                        }
                        nucleus::karma::Consequence::AddConcept { concept } => {
                            Action::AssertRecord {
                                subject: rule.record_uid.clone(),
                                predicate: concept.clone(),
                                object: None,
                                quantity: None,
                                unit: None,
                            }
                        }
                        nucleus::karma::Consequence::RemoveConcept { concept } => {
                            Action::RetractRecord {
                                subject: rule.record_uid.clone(),
                                predicate: concept.clone(),
                                object: None,
                            }
                        }
                        outward => {
                            self.commit_outward_consequence(&rule, outward, carried.as_ref(), now)
                                .await?;
                            continue;
                        }
                    };
                    let ran = Box::pin(self.act_at_with_authorship(next, actor.clone(), now, None))
                        .await?;
                    outcome.facts.extend(ran.facts);
                }
            }
            Action::SkipRecurrenceOccurrence {
                recurrence,
                due_at,
                note,
            } => {
                let due = parse_instant_field(&due_at)?;
                store::recurrence::skip(
                    &self.store.pool,
                    &recurrence,
                    due,
                    note.as_deref(),
                    actor.as_deref(),
                    now,
                )
                .await?;
            }
            Action::UnskipRecurrenceOccurrence { recurrence, due_at } => {
                let due = parse_instant_field(&due_at)?;
                store::recurrence::unskip(&self.store.pool, &recurrence, due).await?;
            }
            Action::ReviseEntry {
                entry,
                expected_revision,
                request_id,
                amount,
                note,
                at,
            } => {
                if store::entries::replayed(&self.store.pool, &request_id)
                    .await?
                    .is_some()
                {
                    return Ok(outcome);
                }
                let current = store::entries::get(&self.store.pool, &entry)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(entry.clone()))?;
                if current.is_void() {
                    return Err(EngineError::Conflict {
                        code: "entry_void",
                        message: "a voided entry cannot be revised".to_string(),
                    });
                }
                if current.revision != expected_revision {
                    return Err(EngineError::Conflict {
                        code: "entry_revision_stale",
                        message: "this entry was changed by someone else".to_string(),
                    });
                }
                let delta = nucleus::DecimalValue::parse_inferred(amount.trim()).map_err(|_| {
                    EngineError::Conflict {
                        code: "entry_amount_invalid",
                        message: format!("`{amount}` is not an exact decimal amount"),
                    }
                })?;
                let occurred_at = match at.as_deref() {
                    Some(text) => chrono::DateTime::parse_from_rfc3339(text)
                        .map_err(|_| EngineError::Conflict {
                            code: "entry_at_invalid",
                            message: format!("`{text}` is not an RFC3339 instant"),
                        })?
                        .with_timezone(&Utc),
                    None => chrono::DateTime::parse_from_rfc3339(&current.occurred_at)
                        .map_err(|_| EngineError::Conflict {
                            code: "entry_at_invalid",
                            message: "stored entry instant is unreadable".to_string(),
                        })?
                        .with_timezone(&Utc),
                };

                let amount_changed = delta
                    .aligned_sub(current.amount)
                    .is_none_or(|difference| !difference.is_zero());
                let moved =
                    amount_changed || store::facts::instant(occurred_at) != current.occurred_at;
                let (compensated, replacement) = if moved {
                    let old_fact_uid =
                        current
                            .fact_uid
                            .clone()
                            .ok_or_else(|| EngineError::Conflict {
                                code: "entry_fact_missing",
                                message: "this entry has no Fact to correct".to_string(),
                            })?;
                    let old_fact = store::facts::get(&self.store.pool, &old_fact_uid)
                        .await?
                        .ok_or_else(|| EngineError::UnknownRecord(old_fact_uid.clone()))?;
                    let concept_uid =
                        store::ledger::fact_concept(&self.store.pool, &old_fact_uid).await?;

                    let mut appended = self
                        .append(
                            NewFact {
                                uid: None,
                                record_uid: old_fact.record_uid.clone(),
                                delta: store::exact::negate(old_fact.delta)?,
                                at: Some(old_fact.at),
                                actor_uid: actor.clone(),
                                cause: Cause {
                                    kind: CauseKind::Compensation,
                                    uid: Some(old_fact_uid.clone()),
                                },
                                payload: None,
                            },
                            now,
                        )
                        .await?;
                    let compensation_uid = appended.first().map(|f| f.uid.clone());
                    let replacement_facts = self
                        .append(
                            NewFact {
                                actor_uid: actor.clone(),
                                at: Some(occurred_at),
                                ..NewFact::quantity(
                                    old_fact.record_uid.clone(),
                                    delta,
                                    Cause::user_edit(),
                                )
                            },
                            now,
                        )
                        .await?;
                    let replacement_uid = replacement_facts.first().map(|f| f.uid.clone());
                    for uid in [&compensation_uid, &replacement_uid].into_iter().flatten() {
                        store::ledger::classify_fact(
                            &self.store.pool,
                            uid,
                            concept_uid.as_deref(),
                            actor.as_deref(),
                            None,
                        )
                        .await?;
                    }
                    appended.extend(replacement_facts);
                    outcome.facts = appended;
                    (compensation_uid, replacement_uid)
                } else {
                    (None, None)
                };

                store::entries::revise(
                    &self.store.pool,
                    store::entries::ReviseEntry {
                        entry_uid: &entry,
                        expected_revision,
                        amount: delta,
                        note: note.as_deref(),
                        occurred_at,
                        compensated_fact_uid: compensated.as_deref(),
                        replacement_fact_uid: replacement.as_deref(),
                        request_id: &request_id,
                        actor_uid: actor.as_deref(),
                    },
                    now,
                )
                .await?;
            }
            Action::VoidEntry {
                entry,
                expected_revision,
                request_id,
            } => {
                if store::entries::replayed(&self.store.pool, &request_id)
                    .await?
                    .is_some()
                {
                    return Ok(outcome);
                }
                let current = store::entries::get(&self.store.pool, &entry)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(entry.clone()))?;
                if current.is_void() {
                    return Err(EngineError::Conflict {
                        code: "entry_already_void",
                        message: "this entry is already void".to_string(),
                    });
                }
                if current.revision != expected_revision {
                    return Err(EngineError::Conflict {
                        code: "entry_revision_stale",
                        message: "this entry was changed by someone else".to_string(),
                    });
                }

                let mut compensation_uid = None;
                if let Some(old_fact_uid) = current.fact_uid.clone() {
                    let old_fact = store::facts::get(&self.store.pool, &old_fact_uid)
                        .await?
                        .ok_or_else(|| EngineError::UnknownRecord(old_fact_uid.clone()))?;
                    if !old_fact.delta.is_zero() {
                        let concept_uid =
                            store::ledger::fact_concept(&self.store.pool, &old_fact_uid).await?;
                        outcome.facts = self
                            .append(
                                NewFact {
                                    uid: None,
                                    record_uid: old_fact.record_uid.clone(),
                                    delta: store::exact::negate(old_fact.delta)?,
                                    at: Some(old_fact.at),
                                    actor_uid: actor.clone(),
                                    cause: Cause {
                                        kind: CauseKind::Compensation,
                                        uid: Some(old_fact_uid.clone()),
                                    },
                                    payload: None,
                                },
                                now,
                            )
                            .await?;
                        if let Some(fact) = outcome.facts.first() {
                            compensation_uid = Some(fact.uid.clone());
                            store::ledger::classify_fact(
                                &self.store.pool,
                                &fact.uid,
                                concept_uid.as_deref(),
                                actor.as_deref(),
                                None,
                            )
                            .await?;
                        }
                    }
                }

                store::entries::void(
                    &self.store.pool,
                    store::entries::VoidEntry {
                        entry_uid: &entry,
                        expected_revision,
                        compensated_fact_uid: compensation_uid.as_deref(),
                        request_id: &request_id,
                        actor_uid: actor.as_deref(),
                    },
                    now,
                )
                .await?;
            }
            Action::ClassifyFact {
                fact,
                concept,
                note,
            } => {
                if store::facts::get(&self.store.pool, &fact).await?.is_none() {
                    return Err(EngineError::UnknownRecord(fact));
                }
                let concept_uid = self.resolve_concept_opt(concept).await?;
                store::ledger::classify_fact(
                    &self.store.pool,
                    &fact,
                    concept_uid.as_deref(),
                    actor.as_deref(),
                    note.as_deref(),
                )
                .await?;
            }
            Action::AddQuantity { target, delta } => {
                let uid = self.resolve(&target).await?;
                self.reject_direct_transfer_record_mutation(&uid).await?;
                if delta != 0.0 {
                    outcome.facts = self
                        .append(
                            NewFact {
                                actor_uid: actor,
                                ..NewFact::quantity_f64(uid, delta, Cause::user_edit())
                            },
                            now,
                        )
                        .await?;
                }
            }
            Action::Activate { target } => {
                return Box::pin(self.act(Action::SetQuantity { target, value: 1.0 }, actor)).await;
            }
            Action::Deactivate { target } => {
                return Box::pin(self.act(Action::SetQuantity { target, value: 0.0 }, actor)).await;
            }
            Action::DeleteRecord { target } => {
                let uid = self.resolve(&target).await?;
                self.reject_direct_transfer_record_mutation(&uid).await?;
                self.check_delete_permission(&uid, actor.as_deref()).await?;
                let old_slug = store::records::get(&self.store.pool, &uid)
                    .await?
                    .and_then(|r| r.slug);
                outcome.facts = self
                    .annotate(
                        uid.clone(),
                        actor,
                        serde_json::json!({ "deleted": true, "slug": old_slug }),
                        now,
                    )
                    .await?;
                store::records::mark_deleted(&self.store.pool, &uid).await?;
            }
            Action::EditRecordText { target, head, body } => {
                let uid = self.resolve(&target).await?;
                self.reject_direct_transfer_record_mutation(&uid).await?;
                self.write_record_text(&uid, head.as_deref(), body.as_deref())
                    .await?;
                outcome.facts = self
                    .annotate(
                        uid,
                        actor,
                        serde_json::json!({ "edit": { "head": head, "body": body } }),
                        now,
                    )
                    .await?;
            }
            Action::SetSlug { target, slug } => {
                let uid = self.resolve(&target).await?;
                self.reject_direct_transfer_record_mutation(&uid).await?;
                let slug = slug.filter(|s| !s.is_empty());
                store::records::set_slug(&self.store.pool, &uid, slug.as_deref()).await?;
                outcome.facts = self
                    .annotate(uid, actor, serde_json::json!({ "slug": slug }), now)
                    .await?;
            }
            Action::SetUnit { target, unit } => {
                let uid = self.resolve(&target).await?;
                let unit_uid = self.resolve_concept_opt(unit).await?;
                store::records::set_unit(&self.store.pool, &uid, unit_uid.as_deref()).await?;
                outcome.facts = self
                    .annotate(uid, actor, serde_json::json!({ "unit": unit_uid }), now)
                    .await?;
            }
            Action::SetExtension {
                target,
                namespace,
                fds,
            } => {
                let uid = self.resolve(&target).await?;
                store::records::set_extension(&self.store.pool, &uid, &namespace, &fds).await?;
                outcome.facts = self
                    .annotate(
                        uid,
                        actor,
                        serde_json::json!({ "extension": namespace }),
                        now,
                    )
                    .await?;
            }
            Action::RenameOrganContact { target, name } => {
                let uid = self.resolve(&target).await?;
                let name = name.trim();
                if name.is_empty() {
                    return Err(EngineError::Consequence(
                        "give this contact a name you will recognise".into(),
                    ));
                }
                if store::organs::contact(&self.store.pool, &uid)
                    .await?
                    .is_none()
                {
                    return Err(EngineError::Consequence(
                        "not a contact — this Cell's own Organ is renamed like any record".into(),
                    ));
                }
                store::organs::rename_contact(&self.store.pool, &uid, name).await?;
            }
            Action::SetSyncPolicy {
                target,
                sync_out,
                sync_in,
            } => {
                let uid = self.resolve(&target).await?;
                if store::organs::contact(&self.store.pool, &uid)
                    .await?
                    .is_none()
                {
                    return Err(EngineError::Consequence(
                        "not a contact — there is no feed to open with this Cell's own Organ"
                            .into(),
                    ));
                }
                store::organs::set_sync_policy(&self.store.pool, &uid, sync_out, sync_in).await?;
                outcome.facts = self
                    .annotate(
                        uid,
                        actor,
                        serde_json::json!({ "sync_out": sync_out, "sync_in": sync_in }),
                        now,
                    )
                    .await?;
            }
            Action::SetContactAcceptScope { target, fields } => {
                let uid = self.resolve(&target).await?;
                if store::organs::contact(&self.store.pool, &uid)
                    .await?
                    .is_none()
                {
                    return Err(EngineError::Consequence(
                        "not a contact — this Cell's own Organ sends us nothing to accept".into(),
                    ));
                }
                validate_scope(fields.as_deref())?;
                store::organs::set_contact_accept_scope(&self.store.pool, &uid, fields.as_deref())
                    .await?;
                outcome.facts = self
                    .annotate(
                        uid,
                        actor,
                        serde_json::json!({ "accept_fields": fields }),
                        now,
                    )
                    .await?;
            }
            Action::DeleteConversation { conversation } => {
                let uid = self.resolve(&conversation).await?;
                let root = store::replica::root_of(&self.store.pool, &uid)
                    .await?
                    .ok_or_else(|| {
                        EngineError::Consequence("that is not part of a conversation".into())
                    })?;
                let removed = store::replica::delete_root_locally(&self.store.pool, &root).await?;
                outcome.data = Some(serde_json::json!({ "removed": removed }));
            }
            Action::SendRecordCopy { thread, record } => {
                let thread_uid = self.resolve(&thread).await?;
                let source_uid = self.resolve(&record).await?;
                let root = store::replica::root_of(&self.store.pool, &thread_uid)
                    .await?
                    .ok_or_else(|| {
                        EngineError::Consequence("that thread is not inside a conversation".into())
                    })?;
                let source = store::records::get(&self.store.pool, &source_uid)
                    .await?
                    .ok_or_else(|| EngineError::Consequence("no such record to copy".into()))?;
                if let Some(existing) =
                    store::replica::root_of(&self.store.pool, &source_uid).await?
                {
                    if existing != root {
                        return Err(EngineError::Consequence(
                            "that record belongs to another conversation and cannot be copied \
                             into this one"
                                .into(),
                        ));
                    }
                }
                let copy = store::records::create_in_root(
                    &self.store.pool,
                    store::records::NewRecord {
                        slug: None,
                        kind: nucleus::RecordKind::parse(&source.kind)
                            .unwrap_or(nucleus::RecordKind::Plain),
                        head: &source.head,
                        body: &source.body,
                        quantity: store::exact::zero(),
                    },
                    Some(&root),
                )
                .await?;
                self.link_in(&copy.uid, &thread_uid, crate::threads::MESSAGE_IN_PREDICATE)
                    .await?;
                outcome.created = Some(copy.uid.clone());
                outcome.facts = self
                    .annotate(
                        copy.uid,
                        actor,
                        serde_json::json!({ "copied_from": source_uid }),
                        now,
                    )
                    .await?;
            }
            Action::HideRecordFromContact {
                target,
                record,
                hidden,
            } => {
                let uid = self.resolve(&target).await?;
                if store::organs::contact(&self.store.pool, &uid)
                    .await?
                    .is_none()
                {
                    return Err(EngineError::Consequence(
                        "not a contact — this Cell's own Organ has no feed to hide from".into(),
                    ));
                }
                let record_uid = self.resolve(&record).await?;
                if store::records::get(&self.store.pool, &record_uid)
                    .await?
                    .is_none()
                {
                    return Err(EngineError::Consequence("no such record to hide".into()));
                }
                store::visibility::set_hidden_from_organ(
                    &self.store.pool,
                    &uid,
                    &record_uid,
                    hidden,
                )
                .await?;
                if !hidden {
                    store::sync_ops::enqueue_record_for_contact(
                        &self.store.pool,
                        &uid,
                        &record_uid,
                    )
                    .await?;
                }
                outcome.facts = self
                    .annotate(
                        uid,
                        actor,
                        serde_json::json!({ "hidden_record": record_uid, "hidden": hidden }),
                        now,
                    )
                    .await?;
            }
            Action::SetContactScope { target, fields } => {
                let uid = self.resolve(&target).await?;
                let Some(contact) = store::organs::contact(&self.store.pool, &uid).await? else {
                    return Err(EngineError::Consequence(
                        "not a contact — this Cell's own Organ has no scope to narrow".into(),
                    ));
                };
                let before = contact.scope_fields;
                validate_scope(fields.as_deref())?;
                store::organs::set_contact_scope(&self.store.pool, &uid, fields.as_deref()).await?;
                if widens_scope(before.as_deref(), fields.as_deref()) {
                    store::sync_ops::enqueue_widened_for_contact(
                        &self.store.pool,
                        &uid,
                        before.as_deref(),
                        fields.as_deref(),
                    )
                    .await?;
                }
                outcome.facts = self
                    .annotate(
                        uid,
                        actor,
                        serde_json::json!({ "scope_fields": fields }),
                        now,
                    )
                    .await?;
            }
            Action::SetContactShare { target, protein } => {
                let uid = self.resolve(&target).await?;
                if store::organs::contact(&self.store.pool, &uid)
                    .await?
                    .is_none()
                {
                    return Err(EngineError::Consequence(
                        "not a contact — this Cell's own Organ shares with nobody".into(),
                    ));
                }
                let raw = match &protein {
                    Some(value) => {
                        let raw = value.to_string();
                        let Some(parsed) = crate::share::parse(&raw) else {
                            return Err(EngineError::Consequence(
                                "this selection is not a Protein query".into(),
                            ));
                        };
                        protein::matching_records(&self.store, &parsed, None).await?;
                        Some(raw)
                    }
                    None => None,
                };
                store::organs::set_contact_share_protein(&self.store.pool, &uid, raw.as_deref())
                    .await?;
                if let Some(contact) = store::organs::contact(&self.store.pool, &uid).await? {
                    crate::share::reconcile_contact(self, &contact).await?;
                }
                outcome.facts = self
                    .annotate(uid, actor, serde_json::json!({ "share": protein }), now)
                    .await?;
            }
            Action::MoveRecordTo { record, target } => {
                let record_uid = self.resolve(&record).await?;
                let contact_uid = self.resolve(&target).await?;
                if store::organs::contact(&self.store.pool, &contact_uid)
                    .await?
                    .is_none()
                {
                    return Err(EngineError::Consequence(
                        "a Record can only be handed to a contact".into(),
                    ));
                }
                if store::records::get(&self.store.pool, &record_uid)
                    .await?
                    .is_none()
                {
                    return Err(EngineError::Consequence("no such Record".into()));
                }
                if let Some(existing) =
                    store::record_move::of_record(&self.store.pool, &record_uid).await?
                {
                    if existing.contact_organ != contact_uid {
                        return Err(EngineError::Consequence(
                            "this Record is already on its way to somebody else — cancel that \
                             move first"
                                .into(),
                        ));
                    }
                }
                store::record_move::begin(&self.store.pool, &record_uid, &contact_uid).await?;
                store::sync_ops::enqueue_record_for_contact(
                    &self.store.pool,
                    &contact_uid,
                    &record_uid,
                )
                .await?;
                outcome.facts = self
                    .annotate(
                        record_uid,
                        actor,
                        serde_json::json!({ "moving_to": contact_uid }),
                        now,
                    )
                    .await?;
            }
            Action::CancelRecordMove { record } => {
                let record_uid = self.resolve(&record).await?;
                if let Some(moving) =
                    store::record_move::of_record(&self.store.pool, &record_uid).await?
                {
                    store::offers::refuse(
                        &self.store.pool,
                        store::offers::OfferKind::RecordMove,
                        &record_uid,
                        &moving.contact_organ,
                    )
                    .await?;
                }
                store::record_move::forget(&self.store.pool, &record_uid).await?;
                outcome.facts = self
                    .annotate(
                        record_uid,
                        actor,
                        serde_json::json!({ "moving_to": null }),
                        now,
                    )
                    .await?;
            }
            Action::ForgetOrganContact { target } => {
                let uid = self.resolve(&target).await?;
                if store::organs::contact(&self.store.pool, &uid)
                    .await?
                    .is_none()
                {
                    return Err(EngineError::Consequence(
                        "not a contact — this Cell's own Organ cannot be forgotten".into(),
                    ));
                }
                store::organs::forget_contact(&self.store.pool, &uid).await?;
            }
            Action::AddKnownOrgan { invite, name } => {
                let invite = crate::pairing::PairingInvite::decode(&invite)?;
                let name = name.trim();
                if name.is_empty() {
                    return Err(EngineError::Consequence(
                        "give this contact a name you will recognise".into(),
                    ));
                }
                let existing =
                    store::organs::contact_by_node_id(&self.store.pool, &invite.node_id).await?;
                if let Some(contact) = &existing {
                    if contact.trust == "blocked" {
                        return Err(EngineError::Consequence(
                            "this Organ is blocked. Unblock them first if that is what you \
                             meant — adding by code must not undo a block."
                                .into(),
                        ));
                    }
                }
                let organ_uid = match &existing {
                    Some(contact) => contact.record_uid.clone(),
                    None => {
                        let organ_uid = format!("o-{}", &invite.node_id);
                        store::organs::add_contact(&self.store.pool, &organ_uid, None, name, "", 1)
                            .await?;
                        store::organs::set_node_id(
                            &self.store.pool,
                            &organ_uid,
                            Some(&invite.node_id),
                        )
                        .await?;
                        organ_uid
                    }
                };
                if existing.is_some() {
                    store::organs::rename_contact(&self.store.pool, &organ_uid, name).await?;
                }
                if let Some(root_key) = &invite.root_key {
                    crate::trust::adopt_key(
                        &self.store,
                        &organ_uid,
                        crate::roster::ROOT_KEY_ID,
                        root_key,
                    )
                    .await?;
                } else {
                    outcome.warnings.push(
                        "this code carried no identity key, so future key changes cannot be \
                         verified against it. Prefer a code that includes one."
                            .into(),
                    );
                }
                store::organs::set_trust(&self.store.pool, &organ_uid, "known").await?;
                if existing.is_none() {
                    store::organs::set_pending_introduction(&self.store.pool, &organ_uid, true)
                        .await?;
                    outcome.warnings.push(
                        "added — but they are not reachable for sync until this Cell has \
                         connected to them once and learned their identity."
                            .into(),
                    );
                }
                outcome.created = Some(organ_uid);
            }
            Action::StartConversation { contact, title } => {
                let contact_uid = self.resolve(&contact).await?;
                let (conversation, thread) =
                    self.start_conversation(&contact_uid, title.trim()).await?;
                outcome.facts = self
                    .append(
                        NewFact {
                            actor_uid: actor,
                            ..NewFact::quantity(
                                contact_uid,
                                store::exact::zero(),
                                Cause {
                                    kind: CauseKind::Sync,
                                    uid: Some(conversation.clone()),
                                },
                            )
                        },
                        now,
                    )
                    .await?;
                outcome.created = Some(
                    serde_json::json!({
                        "conversation": conversation,
                        "thread": thread,
                    })
                    .to_string(),
                );
            }
            Action::OpenThread {
                conversation,
                title,
            } => {
                let conversation_uid = self.resolve(&conversation).await?;
                outcome.created = Some(self.open_thread(&conversation_uid, title.trim()).await?);
            }
            Action::GrantOrganLogin { organ, person_name } => {
                let organ_uid = self.resolve(&organ).await?;
                let contact = store::organs::contact(&self.store.pool, &organ_uid)
                    .await?
                    .ok_or_else(|| EngineError::Consequence("not a contact".into()))?;
                if contact.trust != "known" {
                    return Err(EngineError::Consequence(
                        "only a known contact may be given a login".into(),
                    ));
                }
                let person_name = person_name.trim();
                if person_name.is_empty() {
                    return Err(EngineError::Consequence(
                        "give the Person a name you will recognise".into(),
                    ));
                }
                let person = store::records::create(
                    &self.store.pool,
                    store::records::NewRecord {
                        slug: None,
                        kind: nucleus::RecordKind::Person,
                        head: person_name,
                        body: "",
                        quantity: store::exact::zero(),
                    },
                )
                .await?;
                store::logins::grant(&self.store.pool, &organ_uid, &person.uid).await?;
                outcome.warnings.push(
                    "they can now read and edit as this Person whatever that Person can see. \
                     Nothing is shared until you grant visibility."
                        .into(),
                );
                outcome.created = Some(person.uid);
            }
            Action::RevokeOrganLogin { organ } => {
                let organ_uid = self.resolve(&organ).await?;
                store::logins::revoke(&self.store.pool, &organ_uid).await?;
            }
            Action::AcceptThreadInvite { invite } => {
                let root = self.accept_invite(&invite).await?;
                outcome.created = Some(root);
            }
            Action::DeclineThreadInvite { invite } => {
                self.decline_invite(&invite).await?;
            }
            Action::ShareMyKey { thread } => {
                let thread_uid = self.resolve(&thread).await?;
                let local = store::organs::local(&self.store.pool)
                    .await?
                    .ok_or_else(|| EngineError::Consequence("no local Organ".into()))?;
                let invite =
                    store::records::get_extension(&self.store.pool, &local.uid, "lince.pairing")
                        .await?
                        .and_then(|fields| {
                            fields
                                .get("invite")
                                .and_then(serde_json::Value::as_str)
                                .map(str::to_string)
                        })
                        .ok_or_else(|| {
                            EngineError::Consequence(
                                "this Cell has no pairing code yet — it needs a network endpoint"
                                    .into(),
                            )
                        })?;
                let uid = self.send_message(&thread_uid, &local.head, &invite).await?;
                outcome.created = Some(uid);
            }
            Action::RosterEnrolToken => {
                if self.root_signer().await?.is_none() {
                    return Err(EngineError::Consequence(
                        "the root key is not on this Cell — bring it back to enrol a device".into(),
                    ));
                }
                let token = self.issue_enrolment_token().await?;
                let organ = store::organs::local(&self.store.pool)
                    .await?
                    .ok_or_else(|| EngineError::Consequence("no local Organ".into()))?;
                let root_key =
                    crate::trust::key_of(&self.store, &organ.uid, crate::roster::ROOT_KEY_ID)
                        .await?
                        .unwrap_or_default();
                let pairing =
                    store::records::get_extension(&self.store.pool, &organ.uid, "lince.pairing")
                        .await?
                        .and_then(|fields| {
                            fields
                                .get("invite")
                                .and_then(serde_json::Value::as_str)
                                .map(str::to_string)
                        })
                        .and_then(|encoded| crate::pairing::PairingInvite::decode(&encoded).ok());
                match pairing {
                    Some(pairing) => {
                        let invite = crate::pairing::EnrolmentInvite {
                            node_id: pairing.node_id,
                            organ_uid: organ.uid.clone(),
                            root_key,
                            token: token.clone(),
                            addrs: pairing.addrs,
                        };
                        outcome.created = Some(invite.encode());
                        outcome.data = Some(serde_json::json!({
                            "code": invite.encode(),
                            "qr_svg": invite.qr_svg().unwrap_or_default(),
                            "expires_in_minutes": crate::roster::ENROLMENT_TOKEN_TTL_MINUTES,
                        }));
                    }
                    None => {
                        return Err(EngineError::Consequence(
                            "this Cell has no network identity yet, so a device cannot be \
                             told where to reach it. Wait for the endpoint to bind and try \
                             again."
                                .into(),
                        ));
                    }
                }
            }
            Action::MailboxStatus => {
                let swept = self.sweep_mailbox().await?;
                let mut carrying = Vec::new();
                for registration in store::mailbox::registrations(&self.store.pool).await? {
                    let held =
                        store::mailbox::carried_for(&self.store.pool, &registration.organ_uid)
                            .await?;
                    let known_as =
                        store::organs::contact(&self.store.pool, &registration.organ_uid)
                            .await?
                            .map(|contact| contact.head)
                            .filter(|name| !name.is_empty())
                            .unwrap_or_else(|| registration.label.clone());
                    carrying.push(serde_json::json!({
                        "organ_uid": registration.organ_uid,
                        "known_as": known_as,
                        "quota_bytes": registration.quota_bytes,
                        "held_bundles": held.bundles,
                        "held_bytes": held.bytes,
                        "registered_at": registration.registered_at,
                    }));
                }
                let pending = store::mailbox::pending_notices(&self.store.pool).await?;
                outcome.data = Some(serde_json::json!({
                    "carrying_for": carrying,
                    "swept_just_now": swept,
                    "expiry_notices_pending": pending.len(),
                    "retention_days": crate::seal::RETENTION_DAYS,
                    "max_bundle_bytes": crate::mailbox::MAX_BUNDLE_BYTES,
                }));
            }
            Action::MailboxCarryFor {
                organ_uid,
                label,
                quota_bytes,
            } => {
                let root_key =
                    crate::trust::key_of(&self.store, &organ_uid, crate::roster::ROOT_KEY_ID)
                        .await?
                        .ok_or_else(|| {
                            EngineError::Consequence(
                        "no root key is held for that Organ: pair with them before offering to \
                         carry their mail"
                            .into(),
                    )
                        })?;
                let quota = if quota_bytes > 0 {
                    quota_bytes
                } else {
                    crate::mailbox::DEFAULT_QUOTA_BYTES
                };
                store::mailbox::register(&self.store.pool, &organ_uid, &root_key, &label, quota)
                    .await?;
                outcome.data = Some(serde_json::json!({
                    "organ_uid": organ_uid,
                    "quota_bytes": quota,
                }));
            }
            Action::MailboxStopCarrying { organ_uid } => {
                let held = store::mailbox::carried_for(&self.store.pool, &organ_uid).await?;
                store::mailbox::deregister(&self.store.pool, &organ_uid).await?;
                outcome.data = Some(serde_json::json!({
                    "organ_uid": organ_uid,
                    "discarded_bundles": held.bundles,
                }));
            }
            Action::MailboxPickupPoints => {
                let points = self.own_pickup_points().await?;
                let mut published = Vec::new();
                for point in &points {
                    let probe = self.carrier_probe(&point.node_id).await;
                    let known_as = store::organs::contact(&self.store.pool, &point.organ_uid)
                        .await?
                        .map(|contact| contact.head)
                        .filter(|name| !name.is_empty())
                        .unwrap_or_else(|| point.label.clone());
                    let (state, waiting) = match &probe {
                        crate::wire::CarrierProbe::Carrying(waiting) => (
                            "carrying",
                            serde_json::json!({
                                "bundles": waiting.bundles,
                                "bytes": waiting.bytes,
                                "oldest_expires_at": waiting.oldest_expires_at,
                            }),
                        ),
                        crate::wire::CarrierProbe::Refused => ("refused", serde_json::Value::Null),
                        crate::wire::CarrierProbe::Unreachable => {
                            ("unreachable", serde_json::Value::Null)
                        }
                    };
                    published.push(serde_json::json!({
                        "organ_uid": point.organ_uid,
                        "node_id": point.node_id,
                        "label": point.label,
                        "known_as": known_as,
                        "state": state,
                        "waiting": waiting,
                    }));
                }
                let candidates: Vec<serde_json::Value> = store::organs::contacts(&self.store.pool)
                    .await?
                    .into_iter()
                    .filter(|contact| contact.trust == "known")
                    .filter(|contact| {
                        !points
                            .iter()
                            .any(|point| point.organ_uid == contact.record_uid)
                    })
                    .map(|contact| {
                        serde_json::json!({
                            "organ_uid": contact.record_uid,
                            "known_as": contact.head,
                            "node_id": contact.node_id,
                        })
                    })
                    .collect();
                outcome.data = Some(serde_json::json!({
                    "pickup": published,
                    "candidates": candidates,
                    "retention_days": crate::seal::RETENTION_DAYS,
                    "may_change": self.root_signer().await?.is_some(),
                }));
            }
            Action::MailboxAddPickup {
                organ_uid,
                node_id,
                label,
            } => {
                let contact = store::organs::contact(&self.store.pool, &organ_uid).await?;
                let node_id = if node_id.is_empty() {
                    contact
                        .as_ref()
                        .and_then(|contact| contact.node_id.clone())
                        .filter(|id| !id.is_empty())
                        .ok_or_else(|| {
                            EngineError::Consequence(
                                "we have no address for that contact, so there is nothing to \
                                 publish."
                                    .into(),
                            )
                        })?
                } else {
                    node_id
                };
                match self.carrier_probe(&node_id).await {
                    crate::wire::CarrierProbe::Carrying(_) => {}
                    crate::wire::CarrierProbe::Refused => {
                        return Err(EngineError::Consequence(
                            "they are not carrying mail for you. They have to add you first, \
                             and asking them from here is not built yet — nothing was \
                             published."
                                .into(),
                        ));
                    }
                    crate::wire::CarrierProbe::Unreachable => {
                        return Err(EngineError::Consequence(
                            "they did not answer just now, so we cannot tell whether they \
                             carry mail for you. Nothing was published — try again."
                                .into(),
                        ));
                    }
                }
                let root = self.root_signer().await?.ok_or_else(|| {
                    EngineError::Consequence(
                        "publishing a pickup point re-signs the roster, which needs the root \
                         key this Cell does not currently hold."
                            .into(),
                    )
                })?;
                let label = if label.is_empty() {
                    contact
                        .map(|contact| contact.head)
                        .filter(|name| !name.is_empty())
                        .unwrap_or_else(|| organ_uid.clone())
                } else {
                    label
                };
                let mut points = self.own_pickup_points().await?;
                points.retain(|point| point.organ_uid != organ_uid);
                points.push(crate::roster::PickupPoint {
                    organ_uid: organ_uid.clone(),
                    node_id,
                    label,
                });
                let published = points.len();
                self.set_pickup_points(&root, points).await?;
                outcome.data = Some(serde_json::json!({
                    "organ_uid": organ_uid,
                    "pickup_points": published,
                    "single_point_of_failure": published < 2,
                }));
            }
            Action::MailboxRemovePickup { organ_uid } => {
                let mut points = self.own_pickup_points().await?;
                let Some(going) = points
                    .iter()
                    .find(|point| point.organ_uid == organ_uid)
                    .cloned()
                else {
                    return Err(EngineError::Consequence(
                        "that is not one of your pickup points".into(),
                    ));
                };
                let stranded = match self.carrier_probe(&going.node_id).await {
                    crate::wire::CarrierProbe::Carrying(waiting) => Some(waiting.bundles),
                    _ => None,
                };
                let root = self.root_signer().await?.ok_or_else(|| {
                    EngineError::Consequence(
                        "removing a pickup point re-signs the roster, which needs the root \
                         key this Cell does not currently hold."
                            .into(),
                    )
                })?;
                points.retain(|point| point.organ_uid != organ_uid);
                self.set_pickup_points(&root, points).await?;
                outcome.data = Some(serde_json::json!({
                    "organ_uid": organ_uid,
                    "stranded_bundles": stranded,
                }));
            }
            Action::MailboxCollectNow => {
                let imported = self.collect_mail_now().await?;
                outcome.data = Some(serde_json::json!({ "imported_ops": imported }));
            }
            Action::MailboxRequests => {
                let pool = &self.store.pool;
                let mut asks = Vec::new();
                for row in store::mailbox::requests(pool).await? {
                    let known_as = store::organs::contact(pool, &row.organ_uid)
                        .await?
                        .and_then(|contact| contact.slug);
                    asks.push(serde_json::json!({
                        "organ_uid": row.organ_uid,
                        "known_as": known_as,
                        "claims_to_be": row.label,
                        "asked_at": row.asked_at,
                        "already_carried": store::mailbox::registration(pool, &row.organ_uid)
                            .await?
                            .is_some(),
                    }));
                }
                let invites: Vec<serde_json::Value> = store::mailbox::invites(pool)
                    .await?
                    .into_iter()
                    .map(|row| {
                        serde_json::json!({
                            "label": row.label,
                            "quota_bytes": row.quota_bytes,
                            "expires_at": row.expires_at,
                            "created_at": row.created_at,
                            "used_at": row.used_at,
                            "used_by": row.used_by,
                        })
                    })
                    .collect();
                outcome.data = Some(serde_json::json!({
                    "requests": asks,
                    "invites": invites,
                    "invite_days": crate::mailbox::INVITE_TTL_DAYS,
                    "default_quota_bytes": crate::mailbox::DEFAULT_QUOTA_BYTES,
                }));
            }
            Action::MailboxAnswerRequest {
                organ_uid,
                accept,
                quota_bytes,
            } => {
                let pool = &self.store.pool;
                let ask = store::mailbox::request(pool, &organ_uid)
                    .await?
                    .ok_or_else(|| EngineError::Consequence("no such request".into()))?;
                if accept {
                    let quota = if quota_bytes > 0 {
                        quota_bytes
                    } else {
                        crate::mailbox::DEFAULT_QUOTA_BYTES
                    };
                    store::mailbox::register(
                        pool,
                        &ask.organ_uid,
                        &ask.root_key,
                        &ask.label,
                        quota,
                    )
                    .await?;
                }
                store::mailbox::answer_request(pool, &organ_uid).await?;
                outcome.data = Some(serde_json::json!({
                    "organ_uid": organ_uid,
                    "accepted": accept,
                }));
            }
            Action::MailboxIssueInvite { label, quota_bytes } => {
                let token = self.issue_mailbox_invite(&label, quota_bytes).await?;
                let node_id = self.own_node_id().await?.ok_or_else(|| {
                    EngineError::Consequence(
                        "this Cell has published no address, so an invite would have nowhere \
                         to point. Publish a device list first."
                            .into(),
                    )
                })?;
                let code = crate::pairing::MailboxInviteCode { node_id, token }.encode();
                outcome.data = Some(serde_json::json!({
                    "code": code,
                    "expires_in_days": crate::mailbox::INVITE_TTL_DAYS,
                }));
            }
            Action::MailboxAskCarry { organ_uid } => {
                let contact = store::organs::contact(&self.store.pool, &organ_uid)
                    .await?
                    .ok_or_else(|| EngineError::Consequence("no such contact".into()))?;
                let node_id = contact.node_id.clone().or_else(|| None).ok_or_else(|| {
                    EngineError::Consequence(
                        "we hold no address for them, so there is nobody to ask".into(),
                    )
                })?;
                self.ask_carrier(&node_id).await?;
                outcome.data = Some(serde_json::json!({
                    "organ_uid": organ_uid,
                    "asked": true,
                }));
            }
            Action::MailboxUseInvite { code } => {
                let (label, quota_bytes) = self.redeem_carry_code(&code).await?;
                outcome.data = Some(serde_json::json!({
                    "label": label,
                    "quota_bytes": quota_bytes,
                }));
            }
            Action::FileSyncStatus { organ } => {
                let organ_uid = self.resolve(&organ).await?;
                outcome.data = Some(serde_json::json!({
                    "checked": self.file_sync_has_ticked(&organ_uid),
                    "conflicts": self
                        .file_sync_conflicts(&organ_uid)
                        .into_iter()
                        .map(|c| serde_json::json!({ "path": c.path, "reason": c.reason }))
                        .collect::<Vec<_>>(),
                }));
            }
            Action::MailboxOutbound => {
                let pool = &self.store.pool;
                let queued = store::sync_ops::outbox_due(pool).await?;
                let now = chrono::Utc::now();
                let minutes = |stamp: &Option<String>| -> Option<i64> {
                    stamp
                        .as_deref()
                        .and_then(|when| chrono::DateTime::parse_from_rfc3339(when).ok())
                        .map(|when| (now - when.with_timezone(&chrono::Utc)).num_minutes())
                };
                let mut rows = Vec::new();
                for contact in store::organs::contacts(pool).await? {
                    let Some(waiting) = minutes(&contact.unreachable_since) else {
                        continue;
                    };
                    let publishes = self
                        .roster_of(&contact.record_uid)
                        .await?
                        .map(|signed| !signed.roster.pickup.is_empty())
                        .unwrap_or(false);
                    let ops = queued
                        .iter()
                        .filter(|row| row.contact_organ == contact.record_uid)
                        .count();
                    rows.push(serde_json::json!({
                        "organ_uid": contact.record_uid,
                        "known_as": contact.slug,
                        "unreachable_minutes": waiting,
                        "queued_ops": ops,
                        "mailed_minutes_ago": minutes(&contact.mailed_at),
                        "can_be_mailed": publishes,
                    }));
                }
                let mut never_picked_up = Vec::new();
                for gone in store::mail_left::expired(pool, 20).await? {
                    let known_as = store::organs::contact(pool, &gone.to_organ)
                        .await?
                        .map(|contact| contact.head)
                        .filter(|name| !name.is_empty())
                        .unwrap_or_else(|| gone.to_organ.clone());
                    never_picked_up.push(serde_json::json!({
                        "organ_uid": gone.to_organ,
                        "known_as": known_as,
                        "carrier": gone.carrier_organ,
                        "left_at": gone.left_at,
                        "expired_at": gone.expired_at,
                    }));
                }
                let mail_key_published = self.own_sealing_key_is_published().await.unwrap_or(true);
                outcome.data = Some(serde_json::json!({
                    "contacts": rows,
                    "window_minutes": crate::wire::Wire::MAIL_AFTER.num_minutes(),
                    "never_picked_up": never_picked_up,
                    "outstanding_deposits": store::mail_left::outstanding(pool).await?,
                    "mail_key_published": mail_key_published,
                }));
            }
            Action::MailboxMailNow { organ_uid } => {
                if store::organs::contact(&self.store.pool, organ_uid.as_str())
                    .await?
                    .is_none()
                {
                    return Err(EngineError::Consequence("no such contact".into()));
                }
                let past = (chrono::Utc::now() - crate::wire::Wire::MAIL_AFTER).to_rfc3339();
                store::organs::backdate_unreachable(&self.store.pool, organ_uid.as_str(), &past)
                    .await?;
                store::organs::mark_mailed_clear(&self.store.pool, organ_uid.as_str()).await?;
                let moved = self.sync_now().await?;
                let after = store::organs::contact(&self.store.pool, organ_uid.as_str()).await?;
                outcome.data = Some(serde_json::json!({
                    "organ_uid": organ_uid,
                    "batches": moved,
                    "reached": after
                        .as_ref()
                        .map(|c| c.unreachable_since.is_none())
                        .unwrap_or(false),
                    "mailed": after.and_then(|c| c.mailed_at).is_some(),
                }));
            }
            Action::RosterStatus => {
                let held = store::door::held(&self.store.pool, 50).await?;
                let waiting: Vec<serde_json::Value> = held
                    .into_iter()
                    .map(|row| {
                        let claimed = serde_json::from_str::<serde_json::Value>(&row.intro)
                            .ok()
                            .and_then(|intro| {
                                intro
                                    .get("display_name")
                                    .and_then(serde_json::Value::as_str)
                                    .map(str::to_string)
                            })
                            .unwrap_or_default();
                        serde_json::json!({
                            "uid": row.uid,
                            "node_id": row.node_id,
                            "organ_uid": row.organ_uid,
                            "claimed_name": claimed,
                            "received_at": row.received_at,
                        })
                    })
                    .collect();
                let stale: Vec<serde_json::Value> = self
                    .stale_siblings
                    .lock()
                    .expect("stale siblings")
                    .iter()
                    .map(|cell| {
                        serde_json::json!({
                            "cell_uid": cell.cell_uid,
                            "label": cell.label,
                            "node_id": cell.node_id,
                            "their_epoch": cell.their_epoch,
                            "our_epoch": cell.our_epoch,
                        })
                    })
                    .collect();
                let this_cell = store::cells::local(&self.store.pool).await?;
                let held = match store::organs::local(&self.store.pool).await? {
                    Some(organ) => self.roster_of(&organ.uid).await?,
                    None => None,
                };
                let has_roster = held.is_some();
                let capabilities: Vec<String> = match (&this_cell, held) {
                    (Some(cell), Some(signed)) => signed
                        .roster
                        .cells
                        .into_iter()
                        .find(|member| member.cell_uid == cell.uid)
                        .map(|member| member.capabilities)
                        .unwrap_or_default(),
                    _ => Vec::new(),
                };
                outcome.data = Some(serde_json::json!({
                    "waiting_at_the_door": waiting,
                    "devices_needing_update": stale,
                    "this_cell": this_cell.as_ref().map(|cell| cell.uid.clone()),
                    "capabilities": capabilities,
                    "has_roster": has_roster,
                }));
            }
            Action::SetCellConfig { namespace, fds } => {
                store::cells::set_config(&self.store.pool, &namespace, &fds).await?;
                self.notify_config_changed();
            }
            Action::AuditContact { contact } => {
                let contact_uid = self.resolve(&contact).await?;
                match self.audit_contact(&contact_uid).await? {
                    Some(report) => {
                        outcome.data = Some(serde_json::json!({
                            "contact_organ": report.contact_organ,
                            "they_lack": report.they_lack,
                            "unknown_cells": report.unknown_cells,
                            "reached": true,
                        }));
                    }
                    None => {
                        outcome.data = Some(serde_json::json!({ "reached": false }));
                    }
                }
            }
            Action::RosterJoinOrgan { code } => {
                let roster = self.join_from_code(code.trim()).await?;
                outcome.created = Some(roster.roster.organ_uid.clone());
                outcome.data = Some(serde_json::json!({
                    "organ_uid": roster.roster.organ_uid,
                    "version": roster.roster.version,
                    "cells": roster.roster.cells.len(),
                }));
                outcome.warnings.push(format!(
                    "this device is now part of that Organ, alongside {} other device(s). \
                     Its own previous identity is gone.",
                    roster.roster.cells.len().saturating_sub(1)
                ));
            }
            Action::RosterRevokeCell { cell_uid } => {
                let root = self.root_signer().await?.ok_or_else(|| {
                    EngineError::Consequence(
                        "the root key is not on this Cell — bring it back to revoke a device"
                            .into(),
                    )
                })?;
                let roster = self.revoke_cell(&root, &cell_uid).await?;
                outcome.warnings.push(format!(
                    "roster v{} published without {cell_uid}",
                    roster.roster.version
                ));
            }
            Action::RootKeyExport { destination } => {
                let path = self
                    .root_key_path
                    .lock()
                    .expect("root key path")
                    .clone()
                    .ok_or_else(|| {
                        EngineError::Consequence("this Cell has no root key path".into())
                    })?;
                crate::roster::export_root_key(&path, std::path::Path::new(&destination))?;
                outcome.warnings.push(format!(
                    "root key copied to {destination}. Keep it offline; this Cell can now be \
                     detached from it."
                ));
            }
            Action::RootKeyDetach { copy_at } => {
                let path = self
                    .root_key_path
                    .lock()
                    .expect("root key path")
                    .clone()
                    .ok_or_else(|| {
                        EngineError::Consequence("this Cell has no root key path".into())
                    })?;
                crate::roster::detach_root_key(&path, std::path::Path::new(&copy_at))?;
                outcome.warnings.push(
                    "root key removed from this Cell. Enrolling or revoking a device now needs \
                     it back; everything else keeps working."
                        .into(),
                );
            }
            Action::SetContactTrust { target, trust } => {
                let uid = self.resolve(&target).await?;
                if !matches!(trust.as_str(), "unknown" | "known" | "blocked") {
                    return Err(EngineError::Consequence(format!(
                        "invalid trust `{trust}`: must be unknown, known, or blocked"
                    )));
                }
                store::organs::set_trust(&self.store.pool, &uid, &trust).await?;
                outcome.facts = self
                    .annotate(
                        uid,
                        actor,
                        serde_json::json!({ "contact_trust": trust }),
                        now,
                    )
                    .await?;
            }
            Action::SetContactProximity { target, proximity } => {
                let uid = self.resolve(&target).await?;
                store::organs::set_proximity(&self.store.pool, &uid, proximity).await?;
                outcome.facts = self
                    .annotate(
                        uid,
                        actor,
                        serde_json::json!({ "contact_proximity": proximity }),
                        now,
                    )
                    .await?;
            }
            Action::Compensate { fact } => {
                let original = store::facts::get(&self.store.pool, &fact)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(fact.clone()))?;
                if store::transfers::occurrence_settlement_for_application_fact(
                    &self.store.pool,
                    &original.uid,
                )
                .await?
                .is_some()
                    || store::transfers::occurrence_settlement_compensation_for_fact(
                        &self.store.pool,
                        &original.uid,
                    )
                    .await?
                    .is_some()
                {
                    return Err(EngineError::Conflict {
                        code: "typed_transfer_settlement_compensation_required",
                        message: "Transfer settlement applications and their corrections cannot be changed through generic compensation"
                            .into(),
                    });
                }
                if store::entries::for_fact(&self.store.pool, &original.uid)
                    .await?
                    .is_some()
                {
                    return Err(EngineError::Conflict {
                        code: "entry_void_required",
                        message: "this Fact belongs to an Entry; use void-entry".into(),
                    });
                }
                if !original.delta.is_zero() {
                    outcome.facts = self
                        .append(
                            NewFact {
                                uid: None,
                                record_uid: original.record_uid,
                                delta: store::exact::negate(original.delta)?,
                                at: None,
                                actor_uid: actor,
                                cause: Cause {
                                    kind: CauseKind::Compensation,
                                    uid: Some(original.uid),
                                },
                                payload: None,
                            },
                            now,
                        )
                        .await?;
                }
            }
            Action::CreateLingua { name, visibility } => {
                outcome.created =
                    Some(store::linguas::create(&self.store.pool, &name, None, &visibility).await?);
            }
            Action::RenameLingua { lingua, name } => {
                let lingua_uid = store::linguas::resolve(&self.store.pool, &lingua)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(lingua))?;
                store::linguas::rename(&self.store.pool, &lingua_uid, &name).await?;
            }
            Action::DeleteLingua { lingua } => {
                let lingua_uid = store::linguas::resolve(&self.store.pool, &lingua)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(lingua))?;
                if lingua_uid == store::linguas::LOCAL_UID {
                    return Err(EngineError::Consequence(
                        "the local Lingua is the ontology's permanent private home".into(),
                    ));
                }
                store::linguas::delete(&self.store.pool, &lingua_uid).await?;
            }
            Action::CreateConcept {
                lingua,
                name,
                parents,
            } => {
                let lingua_uid = store::linguas::resolve(&self.store.pool, &lingua)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(lingua))?;
                let mut parent_uids = Vec::new();
                for p in &parents {
                    parent_uids.push(
                        store::concepts::resolve(&self.store.pool, p)
                            .await?
                            .ok_or_else(|| EngineError::UnknownRecord(p.clone()))?,
                    );
                }
                let refs: Vec<&str> = parent_uids.iter().map(String::as_str).collect();
                outcome.created = Some(
                    store::concepts::create_in(&self.store.pool, &lingua_uid, &name, &refs).await?,
                );
            }
            Action::RenameConcept { concept, name } => {
                let concept_uid = store::concepts::resolve(&self.store.pool, &concept)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(concept))?;
                store::concepts::rename(&self.store.pool, &concept_uid, &name).await?;
            }
            Action::DeleteConcept { concept } => {
                let concept_uid = store::concepts::resolve(&self.store.pool, &concept)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(concept))?;
                store::concepts::delete(&self.store.pool, &concept_uid).await?;
            }
            Action::AdoptConcept { lingua, concept } => {
                let lingua_uid = store::linguas::resolve(&self.store.pool, &lingua)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(lingua))?;
                let concept_uid = store::concepts::resolve(&self.store.pool, &concept)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(concept))?;
                store::linguas::adopt(&self.store.pool, &lingua_uid, &concept_uid).await?;
            }
            Action::RemoveConceptFromLingua { lingua, concept } => {
                let lingua_uid = store::linguas::resolve(&self.store.pool, &lingua)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(lingua))?;
                let concept_uid = store::concepts::resolve(&self.store.pool, &concept)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(concept))?;
                store::linguas::remove_concept(&self.store.pool, &lingua_uid, &concept_uid).await?;
            }
            Action::AddConceptParent { concept, parent } => {
                let concept_uid = store::concepts::resolve(&self.store.pool, &concept)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(concept))?;
                let parent_uid = store::concepts::resolve(&self.store.pool, &parent)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(parent))?;
                store::concepts::add_parent(&self.store.pool, &concept_uid, &parent_uid).await?;
            }
            Action::RemoveConceptParent { concept, parent } => {
                let concept_uid = store::concepts::resolve(&self.store.pool, &concept)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(concept))?;
                let parent_uid = store::concepts::resolve(&self.store.pool, &parent)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(parent))?;
                store::concepts::remove_parent(&self.store.pool, &concept_uid, &parent_uid).await?;
            }
            Action::AssertRecord {
                subject,
                predicate,
                object,
                quantity,
                unit,
            } => {
                let subject_uid = self.resolve(&subject).await?;
                let object_uid = match object {
                    Some(object) => Some(self.resolve(&object).await?),
                    None => None,
                };
                let predicate_uid = store::concepts::resolve(&self.store.pool, &predicate)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(predicate))?;
                let unit_uid = self.resolve_concept_opt(unit).await?;
                let quantity = quantity
                    .map(|value| {
                        nucleus::DecimalValue::parse_inferred(&value)
                            .map_err(|error| EngineError::Consequence(error.to_string()))
                    })
                    .transpose()?;
                outcome.created = Some(
                    store::assertions::assert(
                        &self.store.pool,
                        store::assertions::NewAssertion {
                            subject_uid: &subject_uid,
                            predicate_uid: &predicate_uid,
                            object_uid: object_uid.as_deref(),
                            role: store::assertions::AssertionRole::Ordinary,
                            quantity,
                            unit_uid: unit_uid.as_deref(),
                            asserted_by: actor.as_deref(),
                        },
                    )
                    .await?,
                );
                if object_uid.is_some() && is_order_like(&self.store.pool, &predicate_uid).await? {
                    for cycle in kind_cycles(&self.store.pool, &predicate_uid).await? {
                        if cycle.contains(&subject_uid)
                            || object_uid
                                .as_ref()
                                .is_some_and(|object| cycle.contains(object))
                        {
                            outcome.warnings.push(format!(
                                "these {} records form a loop: {}",
                                cycle.len(),
                                cycle.join(" -> ")
                            ));
                        }
                    }
                }
                let mut targets = vec![subject_uid];
                targets.extend(object_uid);
                outcome.facts = self
                    .annotate_many(
                        targets,
                        actor,
                        serde_json::json!({ "assertion": outcome.created }),
                        now,
                    )
                    .await?;
            }
            Action::ImportInstinct => {
                for name in crate::instinct::VOCABULARY {
                    store::concepts::ensure(&self.store.pool, name).await?;
                }
                let bundle = crate::instinct::records();
                let mut fresh = Vec::new();
                for record in &bundle {
                    let uid = record.projection.uid.trim();
                    if store::records::get(&self.store.pool, uid).await?.is_some() {
                        continue;
                    }
                    store::records::create_with_uid(
                        &self.store.pool,
                        store::records::NewRecord {
                            slug: record.slug.as_deref(),
                            kind: RecordKind::Plain,
                            head: &record.head,
                            body: &record.body,
                            quantity: store::exact::zero(),
                        },
                        uid,
                    )
                    .await?;
                    fresh.push(record);
                }
                for record in fresh {
                    let uid = record.projection.uid.trim().to_string();
                    for line in &record.projection.assertions {
                        self.act(
                            Action::AssertRecord {
                                subject: uid.clone(),
                                predicate: line.predicate.clone(),
                                object: line.object.as_ref().map(|link| link.uid.clone()),
                                quantity: line.quantity.clone(),
                                unit: line.unit.clone(),
                            },
                            actor.clone(),
                        )
                        .await?;
                        if line.identity {
                            self.act(
                                Action::SetIdentity {
                                    subject: uid.clone(),
                                    predicate: Some(line.predicate.clone()),
                                },
                                actor.clone(),
                            )
                            .await?;
                        }
                    }
                    if let Some(amount) = record.quantity() {
                        self.act(
                            Action::SetQuantityExact {
                                target: uid.clone(),
                                amount,
                            },
                            actor.clone(),
                        )
                        .await?;
                    }
                    outcome.created = Some(uid);
                }
            }
            Action::CreateAgent { head, operated_by } => {
                let head = head.trim();
                if head.is_empty() {
                    return Err(EngineError::Consequence(
                        "give the Agent a name you will recognise in an assignee list".into(),
                    ));
                }
                let operator = match &operated_by {
                    Some(person) => {
                        let uid = self.resolve(person).await?;
                        let row = store::records::get(&self.store.pool, &uid)
                            .await?
                            .ok_or_else(|| EngineError::UnknownRecord(uid.clone()))?;
                        if row.kind != RecordKind::Person.as_str() {
                            return Err(EngineError::Consequence(
                                "an Agent is operated by a Person".into(),
                            ));
                        }
                        Some(uid)
                    }
                    None => None,
                };
                let actor_concept = store::concepts::ensure(&self.store.pool, "actor").await?;
                for child in ["person", "agent"] {
                    let uid = store::concepts::ensure(&self.store.pool, child).await?;
                    store::concepts::add_parent(&self.store.pool, &uid, &actor_concept).await?;
                }
                let record = store::records::create(
                    &self.store.pool,
                    store::records::NewRecord {
                        slug: None,
                        kind: RecordKind::Person,
                        head,
                        body: "",
                        quantity: store::exact::zero(),
                    },
                )
                .await?;
                self.act(
                    Action::AssertRecord {
                        subject: record.uid.clone(),
                        predicate: "agent".into(),
                        object: None,
                        quantity: None,
                        unit: None,
                    },
                    actor.clone(),
                )
                .await?;
                self.act(
                    Action::SetIdentity {
                        subject: record.uid.clone(),
                        predicate: Some("agent".into()),
                    },
                    actor.clone(),
                )
                .await?;
                if let Some(operator) = operator {
                    store::concepts::ensure(&self.store.pool, "operated-by").await?;
                    self.act(
                        Action::AssertRecord {
                            subject: record.uid.clone(),
                            predicate: "operated-by".into(),
                            object: Some(operator),
                            quantity: None,
                            unit: None,
                        },
                        actor.clone(),
                    )
                    .await?;
                }
                outcome.created = Some(record.uid);
            }
            Action::RetractAssertion { assertion } => {
                let row = store::assertions::get(&self.store.pool, &assertion)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(assertion.clone()))?;
                store::assertions::retract(&self.store.pool, &assertion, actor.as_deref()).await?;
                let mut targets = vec![row.subject_uid];
                targets.extend(row.object_uid);
                outcome.facts = self
                    .annotate_many(
                        targets,
                        actor,
                        serde_json::json!({ "assertion_retracted": assertion }),
                        now,
                    )
                    .await?;
            }
            Action::RefineAssertion {
                subject,
                predicate,
                object,
            } => {
                let subject_uid = self.resolve(&subject).await?;
                let object_uid = self.resolve(&object).await?;
                let predicate_uid = store::concepts::resolve(&self.store.pool, &predicate)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(predicate))?;
                outcome.created = Some(
                    store::assertions::refine(
                        &self.store.pool,
                        &subject_uid,
                        &predicate_uid,
                        &object_uid,
                        actor.as_deref(),
                    )
                    .await?,
                );
                outcome.facts = self
                    .annotate_many(
                        vec![subject_uid, object_uid],
                        actor,
                        serde_json::json!({ "assertion_refined": outcome.created }),
                        now,
                    )
                    .await?;
            }
            Action::RetractRecord {
                subject,
                predicate,
                object,
            } => {
                let subject_uid = self.resolve(&subject).await?;
                let object_uid = match object {
                    Some(object) => Some(self.resolve(&object).await?),
                    None => None,
                };
                let predicate_uid = store::concepts::resolve(&self.store.pool, &predicate)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(predicate))?;
                store::assertions::retract_tuple(
                    &self.store.pool,
                    &subject_uid,
                    &predicate_uid,
                    object_uid.as_deref(),
                    actor.as_deref(),
                )
                .await?;
                let mut targets = vec![subject_uid];
                targets.extend(object_uid);
                outcome.facts = self
                    .annotate_many(
                        targets,
                        actor,
                        serde_json::json!({ "assertion_retracted": {
                            "predicate": predicate_uid
                        }}),
                        now,
                    )
                    .await?;
            }
            Action::SetIdentity { subject, predicate } => {
                let subject_uid = self.resolve(&subject).await?;
                let predicate_uid = self.resolve_concept_opt(predicate).await?;
                outcome.created = store::assertions::set_identity(
                    &self.store.pool,
                    &subject_uid,
                    predicate_uid.as_deref(),
                    actor.as_deref(),
                )
                .await?;
                outcome.facts = self
                    .annotate(
                        subject_uid,
                        actor,
                        serde_json::json!({ "identity": predicate_uid }),
                        now,
                    )
                    .await?;
            }
            Action::SetAssertionOrder {
                predicate,
                ordered,
                reverse,
            } => {
                let kind_uid = store::concepts::resolve(&self.store.pool, &predicate)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(predicate.clone()))?;
                let mut resolved = Vec::new();
                for token in ordered {
                    let uid = self.resolve(&token).await?;
                    if !resolved.iter().any(|existing| existing == &uid) {
                        resolved.push(uid);
                    }
                }
                if resolved.len() < 2 {
                    return Err(EngineError::Consequence(
                        "set-assertion-order needs at least two records".into(),
                    ));
                }
                store::assertions::retract_predicate_within_set(
                    &self.store.pool,
                    &kind_uid,
                    &resolved,
                    actor.as_deref(),
                )
                .await?;
                for pair in resolved.windows(2) {
                    let (from, to) = if reverse {
                        (&pair[1], &pair[0])
                    } else {
                        (&pair[0], &pair[1])
                    };
                    store::assertions::assert(
                        &self.store.pool,
                        store::assertions::NewAssertion {
                            subject_uid: from,
                            predicate_uid: &kind_uid,
                            object_uid: Some(to),
                            role: store::assertions::AssertionRole::Ordinary,
                            quantity: None,
                            unit_uid: None,
                            asserted_by: actor.as_deref(),
                        },
                    )
                    .await?;
                }
                outcome.facts = self
                    .annotate_many(
                        resolved.clone(),
                        actor,
                        serde_json::json!({
                            "action": "set-assertion-order",
                            "predicate": predicate,
                            "ordered": resolved,
                            "reverse": reverse,
                        }),
                        now,
                    )
                    .await?;
            }
            Action::CreateThread { target, head } => {
                let target_uid = self.resolve(&target).await?;
                if store::transfers::get(&self.store.pool, &target_uid)
                    .await?
                    .is_some()
                {
                    self.require_transfer_thread_writer(&target_uid, actor.as_deref(), now)
                        .await?;
                }
                let title = head.trim();
                if title.is_empty() {
                    return Err(EngineError::Consequence(
                        "thread title cannot be empty".into(),
                    ));
                }
                let replica_root = store::replica::root_of(&self.store.pool, &target_uid).await?;
                let thread = store::records::create_in_root(
                    &self.store.pool,
                    store::records::NewRecord {
                        slug: None,
                        kind: RecordKind::Thread,
                        head: title,
                        body: "",
                        quantity: if replica_root.is_some() {
                            store::exact::one()
                        } else {
                            store::exact::zero()
                        },
                    },
                    replica_root.as_deref(),
                )
                .await?;
                let thread_of = store::concepts::ensure(&self.store.pool, "thread-of").await?;
                store::assertions::assert(
                    &self.store.pool,
                    store::assertions::NewAssertion {
                        subject_uid: &thread.uid,
                        predicate_uid: &thread_of,
                        object_uid: Some(&target_uid),
                        role: store::assertions::AssertionRole::Ordinary,
                        quantity: None,
                        unit_uid: None,
                        asserted_by: actor.as_deref(),
                    },
                )
                .await?;
                if replica_root.is_some() {
                    outcome.facts = self
                        .append(
                            NewFact {
                                actor_uid: actor,
                                ..NewFact::quantity(
                                    target_uid,
                                    store::exact::zero(),
                                    Cause {
                                        kind: CauseKind::Sync,
                                        uid: Some(thread.uid.clone()),
                                    },
                                )
                            },
                            now,
                        )
                        .await?;
                } else {
                    outcome.facts = self
                        .append(
                            NewFact {
                                actor_uid: actor.clone(),
                                ..NewFact::quantity(
                                    thread.uid.clone(),
                                    store::exact::one(),
                                    Cause::user_edit(),
                                )
                            },
                            now,
                        )
                        .await?;
                    outcome.facts.extend(
                        self.annotate(
                            target_uid,
                            actor,
                            serde_json::json!({ "thread": { "created": thread.uid } }),
                            now,
                        )
                        .await?,
                    );
                }
                outcome.created = Some(thread.uid);
            }
            Action::CreateMessage {
                thread,
                body,
                author,
                state,
                parent,
                references,
            } => {
                if state == MessageState::Interrupted {
                    return Err(EngineError::Consequence(
                        "a new message may be writing or finished, not interrupted".into(),
                    ));
                }
                let thread_uid = self.resolve(&thread).await?;
                let thread_row = store::records::get(&self.store.pool, &thread_uid)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(thread.clone()))?;
                if thread_row.kind != RecordKind::Thread.as_str() {
                    return Err(EngineError::Consequence(format!(
                        "`{thread}` is a {} record, not a thread",
                        thread_row.kind
                    )));
                }
                if let Some(transfer_uid) = self.transfer_for_thread(&thread_uid).await? {
                    self.require_transfer_thread_writer(&transfer_uid, actor.as_deref(), now)
                        .await?;
                }
                let references = self.resolve_message_references(references).await?;
                let body = body.trim();
                if state == MessageState::Finished && body.is_empty() && references.is_empty() {
                    return Err(EngineError::Consequence(
                        "message body and Record references cannot both be empty".into(),
                    ));
                }
                let head = if body.is_empty() {
                    format!(
                        "Shared {} Record{}",
                        references.len(),
                        if references.len() == 1 { "" } else { "s" }
                    )
                } else {
                    message_head(body)
                };
                let replica_root = store::replica::root_of(&self.store.pool, &thread_uid).await?;
                let message = store::records::create_in_root(
                    &self.store.pool,
                    store::records::NewRecord {
                        slug: None,
                        kind: RecordKind::Message,
                        head: &head,
                        body,
                        quantity: if replica_root.is_some() {
                            store::exact::one()
                        } else {
                            store::exact::zero()
                        },
                    },
                    replica_root.as_deref(),
                )
                .await?;
                let message_in = store::concepts::ensure(&self.store.pool, "message-in").await?;
                store::assertions::assert(
                    &self.store.pool,
                    store::assertions::NewAssertion {
                        subject_uid: &message.uid,
                        predicate_uid: &message_in,
                        object_uid: Some(&thread_uid),
                        role: store::assertions::AssertionRole::Ordinary,
                        quantity: None,
                        unit_uid: None,
                        asserted_by: actor.as_deref(),
                    },
                )
                .await?;
                if !references.is_empty() {
                    let references_kind =
                        store::concepts::ensure(&self.store.pool, "references").await?;
                    for reference in &references {
                        store::assertions::assert(
                            &self.store.pool,
                            store::assertions::NewAssertion {
                                subject_uid: &message.uid,
                                predicate_uid: &references_kind,
                                object_uid: Some(reference),
                                role: store::assertions::AssertionRole::Ordinary,
                                quantity: None,
                                unit_uid: None,
                                asserted_by: actor.as_deref(),
                            },
                        )
                        .await?;
                    }
                }
                if let Some(parent) = parent {
                    let parent_uid = self.resolve(&parent).await?;
                    let parent_row = store::records::get(&self.store.pool, &parent_uid)
                        .await?
                        .ok_or_else(|| EngineError::UnknownRecord(parent.clone()))?;
                    if parent_row.kind != RecordKind::Message.as_str() {
                        return Err(EngineError::Consequence(format!(
                            "`{parent}` is a {} record, not a message",
                            parent_row.kind
                        )));
                    }
                    let parent_threads = store::assertions::objects_from_subject(
                        &self.store.pool,
                        &parent_uid,
                        &message_in,
                    )
                    .await?;
                    if !parent_threads.iter().any(|row| row.uid == thread_uid) {
                        return Err(EngineError::Consequence(
                            "reply parent is not in the target thread".into(),
                        ));
                    }
                    let reply_to = store::concepts::ensure(&self.store.pool, "reply-to").await?;
                    store::assertions::assert(
                        &self.store.pool,
                        store::assertions::NewAssertion {
                            subject_uid: &message.uid,
                            predicate_uid: &reply_to,
                            object_uid: Some(&parent_uid),
                            role: store::assertions::AssertionRole::Ordinary,
                            quantity: None,
                            unit_uid: None,
                            asserted_by: actor.as_deref(),
                        },
                    )
                    .await?;
                }
                let (author, operator) = self
                    .message_authorship(author.as_deref(), actor.as_deref())
                    .await?;
                store::records::set_extension(
                    &self.store.pool,
                    &message.uid,
                    "lince.message",
                    &serde_json::json!({
                        "author": author,
                        "operator": operator,
                        "state": state.as_str(),
                    }),
                )
                .await?;
                if replica_root.is_some() {
                    outcome.facts = self
                        .append(
                            NewFact {
                                actor_uid: actor,
                                ..NewFact::quantity(
                                    thread_uid,
                                    store::exact::zero(),
                                    Cause {
                                        kind: CauseKind::Sync,
                                        uid: Some(message.uid.clone()),
                                    },
                                )
                            },
                            now,
                        )
                        .await?;
                } else {
                    outcome.facts = self
                        .append(
                            NewFact {
                                actor_uid: actor.clone(),
                                ..NewFact::quantity(
                                    message.uid.clone(),
                                    store::exact::one(),
                                    Cause::user_edit(),
                                )
                            },
                            now,
                        )
                        .await?;
                    outcome.facts.extend(
                        self.annotate(
                            thread_uid,
                            actor,
                            serde_json::json!({
                                "message": {
                                    "created": message.uid,
                                    "references": references,
                                }
                            }),
                            now,
                        )
                        .await?,
                    );
                }
                outcome.created = Some(message.uid);
            }
            Action::ReviseMessage {
                message,
                body,
                state,
            } => {
                let message_uid = self.resolve(&message).await?;
                let message_row = store::records::get(&self.store.pool, &message_uid)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(message.clone()))?;
                if message_row.kind != RecordKind::Message.as_str() {
                    return Err(EngineError::Consequence(format!(
                        "`{message}` is a {} record, not a message",
                        message_row.kind
                    )));
                }
                let thread_uid = self.thread_for_message(&message_uid).await?;
                if let Some(transfer_uid) = self.transfer_for_thread(&thread_uid).await? {
                    self.require_transfer_thread_writer(&transfer_uid, actor.as_deref(), now)
                        .await?;
                }
                let mut metadata =
                    store::records::get_extension(&self.store.pool, &message_uid, "lince.message")
                        .await?
                        .and_then(|value| value.as_object().cloned())
                        .ok_or_else(|| EngineError::Conflict {
                            code: "message_lifecycle_missing",
                            message: "the message has no persisted lifecycle metadata".into(),
                        })?;
                if metadata.get("state").and_then(serde_json::Value::as_str) != Some("writing") {
                    return Err(EngineError::Conflict {
                        code: "message_not_writing",
                        message: "only a writing message may grow, finish or be interrupted".into(),
                    });
                }
                if let Some(actor) = actor.as_deref()
                    && metadata.get("operator").and_then(serde_json::Value::as_str) != Some(actor)
                {
                    return Err(EngineError::Forbidden(
                        "only the persisted message operator may revise it".into(),
                    ));
                }
                if state == MessageState::Finished && body.trim().is_empty() {
                    let references =
                        store::concepts::resolve(&self.store.pool, "references").await?;
                    let has_references = match references {
                        Some(predicate) => !store::assertions::objects_from_subject(
                            &self.store.pool,
                            &message_uid,
                            &predicate,
                        )
                        .await?
                        .is_empty(),
                        None => false,
                    };
                    if !has_references {
                        return Err(EngineError::Consequence(
                            "a finished message body and Record references cannot both be empty"
                                .into(),
                        ));
                    }
                }
                let head = if body.trim().is_empty() {
                    match state {
                        MessageState::Writing => "Writing message".into(),
                        MessageState::Finished => "Shared Records".into(),
                        MessageState::Interrupted => "Interrupted message".into(),
                    }
                } else {
                    message_head(&body)
                };
                self.write_record_text(&message_uid, Some(&head), Some(&body))
                    .await?;
                metadata.insert("state".into(), serde_json::json!(state.as_str()));
                store::records::set_extension(
                    &self.store.pool,
                    &message_uid,
                    "lince.message",
                    &serde_json::Value::Object(metadata),
                )
                .await?;
                outcome.facts = self
                    .annotate(
                        message_uid,
                        actor,
                        serde_json::json!({ "message": { "state": state.as_str() } }),
                        now,
                    )
                    .await?;
            }
            Action::CreateMessageDraft {
                conversation,
                thread,
                body,
                pinned,
                timing,
                position,
            } => {
                let (conversation_uid, thread_uid) = self
                    .validate_message_draft_target(&conversation, &thread)
                    .await?;
                let (author, operator) = self.message_authorship(None, actor.as_deref()).await?;
                let head = message_draft_head(&body);
                let draft = store::records::create(
                    &self.store.pool,
                    store::records::NewRecord {
                        slug: None,
                        kind: RecordKind::MessageDraft,
                        head: &head,
                        body: &body,
                        quantity: store::exact::zero(),
                    },
                )
                .await?;
                store::records::set_extension(
                    &self.store.pool,
                    &draft.uid,
                    "lince.message-draft",
                    &serde_json::json!({
                        "conversation": conversation_uid,
                        "thread": thread_uid,
                        "author": author,
                        "operator": operator,
                        "pinned": pinned,
                        "timing": timing.as_str(),
                        "position": position,
                    }),
                )
                .await?;
                outcome.facts = self
                    .append(
                        NewFact {
                            actor_uid: actor,
                            ..NewFact::quantity(
                                draft.uid.clone(),
                                store::exact::one(),
                                Cause::user_edit(),
                            )
                        },
                        now,
                    )
                    .await?;
                outcome.created = Some(draft.uid);
            }
            Action::ReviseMessageDraft {
                draft,
                body,
                pinned,
                timing,
                position,
            } => {
                let (draft_uid, mut metadata) = self
                    .message_draft_metadata(&draft, actor.as_deref())
                    .await?;
                let head = message_draft_head(&body);
                self.write_record_text(&draft_uid, Some(&head), Some(&body))
                    .await?;
                metadata.insert("pinned".into(), serde_json::json!(pinned));
                metadata.insert("timing".into(), serde_json::json!(timing.as_str()));
                metadata.insert("position".into(), serde_json::json!(position));
                store::records::set_extension(
                    &self.store.pool,
                    &draft_uid,
                    "lince.message-draft",
                    &serde_json::Value::Object(metadata),
                )
                .await?;
                outcome.facts = self
                    .annotate(
                        draft_uid,
                        actor,
                        serde_json::json!({ "message_draft": "revised" }),
                        now,
                    )
                    .await?;
            }
            Action::DeleteMessageDraft { draft } => {
                let (draft_uid, _) = self
                    .message_draft_metadata(&draft, actor.as_deref())
                    .await?;
                self.check_delete_permission(&draft_uid, actor.as_deref())
                    .await?;
                outcome.facts = self
                    .annotate(
                        draft_uid.clone(),
                        actor,
                        serde_json::json!({ "message_draft": "deleted" }),
                        now,
                    )
                    .await?;
                store::records::mark_deleted(&self.store.pool, &draft_uid).await?;
            }
            Action::SendMessageDraft { draft } => {
                let (draft_uid, metadata) = self
                    .message_draft_metadata(&draft, actor.as_deref())
                    .await?;
                let draft_row = store::records::get(&self.store.pool, &draft_uid)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(draft_uid.clone()))?;
                let thread = metadata
                    .get("thread")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| EngineError::Conflict {
                        code: "message_draft_invalid",
                        message: "the draft has no target thread".into(),
                    })?
                    .to_string();
                let pinned = metadata
                    .get("pinned")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false);
                let operator = metadata.get("operator").and_then(serde_json::Value::as_str);
                let author = metadata
                    .get("author")
                    .and_then(serde_json::Value::as_str)
                    .filter(|author| Some(*author) != operator)
                    .map(str::to_string);
                outcome = Box::pin(self.act_at_with_authorship(
                    Action::CreateMessage {
                        thread,
                        body: draft_row.body,
                        author,
                        state: MessageState::Finished,
                        parent: None,
                        references: Vec::new(),
                    },
                    actor.clone(),
                    now,
                    verified_authorship,
                ))
                .await?;
                if !pinned {
                    outcome.facts.extend(
                        self.annotate(
                            draft_uid.clone(),
                            actor,
                            serde_json::json!({ "message_draft": "sent" }),
                            now,
                        )
                        .await?,
                    );
                    store::records::mark_deleted(&self.store.pool, &draft_uid).await?;
                }
            }
            Action::CreateTransferThread {
                transfer,
                head,
                request_id: _,
                person: _,
            } => {
                outcome = Box::pin(self.act_at_with_authorship(
                    Action::CreateThread {
                        target: transfer,
                        head,
                    },
                    actor,
                    now,
                    verified_authorship,
                ))
                .await?;
            }
            Action::CreateTransferMessage {
                transfer,
                thread,
                body,
                parent,
                references,
                request_id: _,
                person: _,
            } => {
                let transfer_uid = self.resolve(&transfer).await?;
                let thread_uid = self.resolve(&thread).await?;
                if self.transfer_for_thread(&thread_uid).await?.as_deref()
                    != Some(transfer_uid.as_str())
                {
                    return Err(EngineError::Conflict {
                        code: "transfer_thread_target_mismatch",
                        message: "message thread belongs to another Transfer".into(),
                    });
                }
                outcome = Box::pin(self.act_at_with_authorship(
                    Action::CreateMessage {
                        thread: thread_uid,
                        body,
                        author: None,
                        state: MessageState::Finished,
                        parent,
                        references,
                    },
                    actor,
                    now,
                    verified_authorship,
                ))
                .await?;
            }
            Action::CreatePromise {
                record,
                delta,
                window_end,
                party,
                open,
            } => {
                let record_uid = self.resolve(&record).await?;
                let state = if open {
                    PromiseState::Open
                } else {
                    PromiseState::Proposed
                };
                let party = if open {
                    Some(
                        self.transfer_action_person(actor.as_deref(), party.as_deref(), None)
                            .await?,
                    )
                } else {
                    match party {
                        Some(token) => Some(self.resolve(token.trim()).await?),
                        None => None,
                    }
                };
                outcome.created = Some(
                    store::misc::insert_promise(
                        &self.store.pool,
                        store::misc::NewPromise {
                            record_uid: Some(record_uid),
                            delta,
                            window_end,
                            party_uid: party,
                            state: Some(state),
                            ..Default::default()
                        },
                    )
                    .await?,
                );
            }
            Action::PromiseTransition { promise, to } => {
                let row = store::misc::get_promise(&self.store.pool, &promise)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(promise.clone()))?;
                if row.transfer_uid.is_some() {
                    return Err(EngineError::Conflict {
                        code: "transfer_phase_3_not_available",
                        message:
                            "bundled promise state follows the revision-bound Transfer workflow"
                                .into(),
                    });
                }
                let next = PromiseState::transition(row.state, to)?;
                store::misc::set_promise_state(&self.store.pool, &promise, next).await?;
                if let Some(record_uid) = row.record_uid {
                    outcome.facts = self
                        .append(
                            NewFact {
                                uid: None,
                                record_uid,
                                delta: nucleus::fact::zero_delta(),
                                at: None,
                                actor_uid: actor,
                                cause: Cause { kind: CauseKind::Action, uid: Some(promise) },
                                payload: Some(
                                    serde_json::json!({
                                        "promise": { "from": row.state.as_str(), "to": next.as_str() }
                                    })
                                    .to_string(),
                                ),
                            },
                            now,
                        )
                        .await?;
                }
            }
            Action::EditPromiseDelta { promise, delta } => {
                if !delta.is_finite() || delta == 0.0 {
                    return Err(EngineError::Consequence(
                        "promise delta must be finite and non-zero".into(),
                    ));
                }
                let row = store::misc::get_promise(&self.store.pool, &promise)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(promise.clone()))?;
                if let Some(transfer_uid) = row.transfer_uid.as_deref() {
                    self.require_transfer_editor(transfer_uid, actor.as_deref())
                        .await?;
                    return Err(EngineError::Conflict {
                        code: "transfer_revision_required",
                        message: "bundled promises must use revise-transfer-promise with expected_revision and request_id".into(),
                    });
                }
                store::misc::set_promise_delta(&self.store.pool, &promise, delta).await?;
            }
            Action::CreateTransfer {
                slug,
                head,
                agreement,
                agreement_pct,
                satiation,
                mut source,
                reserve_default,
                require_confirmation,
            } => {
                if transfer_phase_locked() {
                    return Err(EngineError::Conflict {
                        code: "transfer_draft_action_required",
                        message: "use create-transfer-draft so the initial terms are atomic and revisioned".into(),
                    });
                }
                if head.trim().is_empty() || head.chars().count() > 200 {
                    return Err(EngineError::Consequence(
                        "transfer title must contain 1 to 200 characters".into(),
                    ));
                }
                let agreement_kind = nucleus::transfer::AgreementType::parse(&agreement)
                    .ok_or_else(|| {
                        EngineError::Consequence(format!(
                            "unknown transfer agreement `{agreement}`"
                        ))
                    })?;
                match agreement_kind {
                    nucleus::transfer::AgreementType::Percentage => {
                        if !agreement_pct.is_some_and(|pct| (1..=100).contains(&pct)) {
                            return Err(EngineError::Consequence(
                                "percentage agreement requires a threshold from 1 to 100".into(),
                            ));
                        }
                    }
                    _ if agreement_pct.is_some() => {
                        return Err(EngineError::Consequence(
                            "agreement_pct is valid only for percentage agreement".into(),
                        ));
                    }
                    _ => {}
                }
                if satiation
                    .as_deref()
                    .is_some_and(|value| value != "first_completes")
                {
                    return Err(EngineError::Consequence(
                        "satiation must be `first_completes` or omitted".into(),
                    ));
                }
                if satiation.as_deref() == Some("first_completes") && source.is_none() {
                    return Err(EngineError::Consequence(
                        "first_completes requires a source record".into(),
                    ));
                }
                if reserve_default.as_deref().is_some_and(|value| {
                    !matches!(value, "none" | "proposed" | "agreed" | "active")
                }) {
                    return Err(EngineError::Consequence(
                        "reserve_default must be none, proposed, agreed, active, or omitted".into(),
                    ));
                }
                if let Some(token) = source.as_deref() {
                    source = Some(self.resolve(token).await?);
                }
                let creator_person = self.require_transfer_creator(actor.as_deref()).await?;
                let visibility_actor = actor.clone();
                let transfer = store::transfers::create(
                    &self.store.pool,
                    store::transfers::NewTransfer {
                        slug: slug.as_deref(),
                        head: &head,
                        agreement_type: &agreement,
                        agreement_pct,
                        satiation: satiation.as_deref(),
                        source_uid: source.as_deref(),
                        reserve_default: reserve_default.as_deref(),
                        require_confirmation,
                    },
                )
                .await?;
                if let Some(person) = creator_person {
                    store::transfers::add_party(&self.store.pool, &transfer, &person).await?;
                }
                if let Some(subject) = visibility_actor.as_deref() {
                    store::visibility::grant(&self.store.pool, "actor", Some(subject), &transfer)
                        .await?;
                }
                outcome.facts = self
                    .append(
                        NewFact {
                            actor_uid: actor,
                            ..NewFact::quantity(
                                transfer.clone(),
                                store::exact::one(),
                                Cause::user_edit(),
                            )
                        },
                        now,
                    )
                    .await?;
                outcome.created = Some(transfer);
            }
            Action::CreateTransferDraft {
                request_id,
                creator,
                slug,
                head,
                agreement,
                agreement_pct,
                satiation,
                parent,
                source,
                visibility,
                max_proximity,
                reserve_default,
                require_confirmation,
                default_place,
                invitees,
                promises,
                dependencies,
            } => {
                let mapped_creator = self.require_transfer_creator(actor.as_deref()).await?;
                let visibility_actor = actor.clone();
                let request_id = request_id.trim().to_string();
                if request_id.is_empty() || request_id.chars().count() > 200 {
                    return Err(EngineError::Consequence(
                        "transfer request_id must contain 1 to 200 characters".into(),
                    ));
                }
                let creator_person = match (mapped_creator, creator) {
                    (Some(mapped), Some(token)) => {
                        let requested = self.resolve(token.trim()).await?;
                        if requested != mapped {
                            return Err(EngineError::Forbidden(
                                "an authenticated transfer creator is derived from the session"
                                    .into(),
                            ));
                        }
                        mapped
                    }
                    (Some(mapped), None) => mapped,
                    (None, Some(token)) => {
                        let requested = self.resolve(token.trim()).await?;
                        let row = store::records::get(&self.store.pool, &requested)
                            .await?
                            .ok_or_else(|| EngineError::UnknownRecord(requested.clone()))?;
                        if row.kind != RecordKind::Person.as_str() {
                            return Err(EngineError::Consequence(
                                "the local transfer creator must be a Person record".into(),
                            ));
                        }
                        requested
                    }
                    (None, None) => {
                        return Err(EngineError::Consequence(
                            "trusted local mode requires an explicit creator Person".into(),
                        ));
                    }
                };
                if let Some((replayed_transfer, _, replayed_action)) =
                    store::transfers::revision_for_request(&self.store.pool, &request_id).await?
                {
                    let replayed_creator =
                        store::transfers::creator_party_actor(&self.store.pool, &replayed_transfer)
                            .await?;
                    if replayed_action != "create-transfer-draft"
                        || replayed_creator.as_deref() != Some(creator_person.as_str())
                    {
                        return Err(EngineError::Conflict {
                            code: "transfer_request_id_conflict",
                            message: "transfer request id belongs to another creator or transfer"
                                .into(),
                        });
                    }
                    outcome.created = Some(replayed_transfer);
                    return Ok(outcome);
                }
                let head = head.trim().to_string();
                if head.is_empty() || head.chars().count() > 200 {
                    return Err(EngineError::Consequence(
                        "transfer title must contain 1 to 200 characters".into(),
                    ));
                }
                let slug = slug
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty());
                if let Some(value) = slug.as_deref() {
                    if !nucleus::valid_slug(value) {
                        return Err(EngineError::Consequence(format!(
                            "invalid transfer slug `{value}`"
                        )));
                    }
                }

                match agreement {
                    nucleus::transfer::AgreementType::Percentage => {
                        if !agreement_pct.is_some_and(|pct| (1..=100).contains(&pct)) {
                            return Err(EngineError::Consequence(
                                "percentage agreement requires a threshold from 1 to 100".into(),
                            ));
                        }
                    }
                    _ if agreement_pct.is_some() => {
                        return Err(EngineError::Consequence(
                            "agreement_pct is valid only for percentage agreement".into(),
                        ));
                    }
                    _ => {}
                }
                match visibility {
                    TransferVisibility::Proximity => {
                        if !max_proximity.is_some_and(|value| value > 0) {
                            return Err(EngineError::Consequence(
                                "proximity visibility requires max_proximity greater than zero"
                                    .into(),
                            ));
                        }
                    }
                    _ if max_proximity.is_some() => {
                        return Err(EngineError::Consequence(
                            "max_proximity is valid only for proximity visibility".into(),
                        ));
                    }
                    _ => {}
                }
                if invitees.len() > 63 {
                    return Err(EngineError::Consequence(
                        "a transfer draft supports at most 63 invited people".into(),
                    ));
                }
                if promises.is_empty() || promises.len() > 256 {
                    return Err(EngineError::Consequence(
                        "a transfer draft requires 1 to 256 promises".into(),
                    ));
                }
                let cell_reserve_default = self.transfer_reservation_cell_default().await?;
                let effective_reserve_default = reserve_default.resolve(cell_reserve_default);

                let parent_uid = match parent.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
                    Some(token) => {
                        let uid = self.resolve(token).await?;
                        let row = store::records::get(&self.store.pool, &uid)
                            .await?
                            .ok_or_else(|| EngineError::UnknownRecord(uid.clone()))?;
                        if row.kind != RecordKind::Transfer.as_str() {
                            return Err(EngineError::Consequence(
                                "a transfer parent must be another transfer".into(),
                            ));
                        }
                        Some(uid)
                    }
                    None => None,
                };
                let source_uid = match source.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
                    Some(token) => Some(self.resolve(token).await?),
                    None => None,
                };
                if matches!(satiation, TransferSatiation::FirstCompletes) && source_uid.is_none() {
                    return Err(EngineError::Consequence(
                        "first_completes requires a source record shared with its siblings".into(),
                    ));
                }

                let mut seen_people = std::collections::HashSet::new();
                seen_people.insert(creator_person.clone());
                let mut invited_people = Vec::with_capacity(invitees.len());
                for token in invitees {
                    let person_uid = self.resolve(token.trim()).await?;
                    let row = store::records::get(&self.store.pool, &person_uid)
                        .await?
                        .ok_or_else(|| EngineError::UnknownRecord(person_uid.clone()))?;
                    if row.kind != RecordKind::Person.as_str() {
                        return Err(EngineError::Consequence(format!(
                            "transfer invitee `{}` is not a Person record",
                            row.slug.as_deref().unwrap_or(&person_uid)
                        )));
                    }
                    if !seen_people.insert(person_uid.clone()) {
                        return Err(EngineError::Consequence(
                            "the creator or an invitee cannot appear twice".into(),
                        ));
                    }
                    invited_people.push(person_uid);
                }

                let mut draft_promises = Vec::with_capacity(promises.len());
                for input in promises {
                    if !input.delta.is_finite() || input.delta == 0.0 {
                        return Err(EngineError::Consequence(
                            "every promise delta must be finite and non-zero".into(),
                        ));
                    }
                    if input.withdrawn {
                        return Err(EngineError::Consequence(
                            "new transfer promises cannot be withdrawn".into(),
                        ));
                    }
                    if let Some(uid) = input.uid.as_deref()
                        && store::misc::get_promise(&self.store.pool, uid)
                            .await?
                            .is_some()
                    {
                        return Err(EngineError::Conflict {
                            code: "transfer_promise_uid_conflict",
                            message: "a new Transfer promise uid is already in use".into(),
                        });
                    }
                    let record_uid = self.resolve(input.record.trim()).await?;
                    let concept_uid = store::records::get(&self.store.pool, &record_uid)
                        .await?
                        .ok_or_else(|| EngineError::UnknownRecord(record_uid.clone()))?
                        .identity_predicate_uid;
                    let person_uid = if input.open {
                        if let Some(token) = input.party.as_deref() {
                            let submitted = self.resolve(token.trim()).await?;
                            if submitted != creator_person {
                                return Err(EngineError::Consequence(
                                    "an OPEN promise is owned by the creating Person; its counterparty remains unnamed"
                                        .into(),
                                ));
                            }
                        }
                        Some(creator_person.clone())
                    } else {
                        let token = input.party.as_deref().ok_or_else(|| {
                            EngineError::Consequence(
                                "a non-OPEN promise requires a reviewed Person".into(),
                            )
                        })?;
                        let person_uid = self.resolve(token.trim()).await?;
                        if !seen_people.contains(&person_uid) {
                            return Err(EngineError::Consequence(
                                "every promise Person must be the creator or a reviewed invitee"
                                    .into(),
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
                    let (window_start, window_end) =
                        normalize_transfer_window(input.window_start, input.window_end, now, None)?;
                    let location = normalize_transfer_place(input.place)?;
                    let condition = input
                        .condition
                        .map(|value| value.trim().to_string())
                        .filter(|value| !value.is_empty());
                    if let Some(value) = condition.as_deref() {
                        nucleus::expr::Expr::parse(value).map_err(|error| {
                            EngineError::Consequence(format!("invalid promise condition: {error}"))
                        })?;
                    }
                    draft_promises.push(store::transfers::DraftPromise {
                        uid: Some(input.uid.unwrap_or_else(|| nucleus::new_uid("p"))),
                        record_uid: Some(record_uid),
                        concept_uid,
                        unit_uid,
                        person_uid,
                        open: input.open,
                        delta: input.delta,
                        window_start,
                        window_end,
                        location,
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
                let promise_uids = draft_promises
                    .iter()
                    .filter_map(|promise| promise.uid.clone())
                    .collect::<HashSet<_>>();
                let dependencies = self
                    .resolve_transfer_dependencies(None, dependencies, &promise_uids)
                    .await?;
                if matches!(agreement, nucleus::transfer::AgreementType::Dependency)
                    && dependencies.is_empty()
                {
                    return Err(EngineError::Consequence(
                        "dependency agreement requires at least one structured dependency".into(),
                    ));
                }

                let organ_uid = store::organs::local(&self.store.pool)
                    .await?
                    .map(|organ| organ.uid);
                let signer = self
                    .transfer_person_signer(&creator_person, verified_authorship.as_ref())
                    .await?;
                let created = store::transfers::create_draft(
                    &self.store.pool,
                    store::transfers::NewTransferDraft {
                        idempotency_key: request_id,
                        slug,
                        head,
                        agreement_type: agreement.as_str().into(),
                        agreement_pct: agreement_pct.map(i64::from),
                        satiation: satiation.as_option(),
                        parent_uid,
                        source_uid,
                        visibility: visibility.as_str().into(),
                        max_proximity: max_proximity.map(i64::from),
                        reserve_default: effective_reserve_default.as_str().into(),
                        require_confirmation,
                        default_place: normalize_transfer_place(default_place)?,
                        creator_person: creator_person.clone(),
                        invitees: invited_people,
                        promises: draft_promises,
                        dependencies,
                        organ_uid,
                        evidence_action: "create-transfer-draft".into(),
                        correction: None,
                        authorization_intent_uid: verified_authorship
                            .as_ref()
                            .map(|value| value.intent_uid.clone()),
                    },
                    now,
                    Some(creator_person),
                    visibility_actor,
                    |hash| signer.as_ref().map(|value| value.sign_hash(hash)),
                )
                .await?;
                if !created.replayed {
                    outcome.facts = self.publish_committed_fact(created.fact);
                    for fact in created.invitation_event_facts {
                        outcome.facts.extend(self.publish_committed_fact(fact));
                    }
                }
                outcome.created = Some(created.transfer_uid);
            }
            Action::ReviseTransferPromise {
                transfer,
                promise,
                expected_revision,
                request_id,
                terms,
            } => {
                if transfer_phase_locked() {
                    return Err(EngineError::Conflict {
                        code: "transfer_draft_action_required",
                        message:
                            "use revise-transfer-draft so every public term is reviewed together"
                                .into(),
                    });
                }
                if !terms.delta.is_finite() || terms.delta == 0.0 {
                    return Err(EngineError::Consequence(
                        "promise delta must be finite and non-zero".into(),
                    ));
                }
                let transfer = self.resolve(&transfer).await?;
                self.require_transfer_editor(&transfer, actor.as_deref())
                    .await?;
                let promise = promise.trim().to_string();
                let source = store::misc::get_promise(&self.store.pool, &promise)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(promise.clone()))?;
                if source.transfer_uid.as_deref() != Some(transfer.as_str()) {
                    return Err(EngineError::Consequence(
                        "promise does not belong to the selected Transfer".into(),
                    ));
                }
                let record_uid = self.resolve(terms.record.trim()).await?;
                let person_token = terms.party.as_deref().ok_or_else(|| {
                    EngineError::Consequence(
                        "a single-promise edit cannot turn a promise OPEN; use revise-transfer-draft"
                            .into(),
                    )
                })?;
                let person_uid = self.resolve(person_token.trim()).await?;
                let person = store::records::get(&self.store.pool, &person_uid)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(person_uid.clone()))?;
                if person.kind != RecordKind::Person.as_str() {
                    return Err(EngineError::Consequence(
                        "promise Person must be a Person record".into(),
                    ));
                }
                let is_known =
                    store::transfers::party_for_actor(&self.store.pool, &transfer, &person_uid)
                        .await?
                        .is_some()
                        || store::transfers::invitations_for_transfer(&self.store.pool, &transfer)
                            .await?
                            .iter()
                            .any(|invitation| invitation.addressed_person_uid == person_uid);
                if !is_known {
                    return Err(EngineError::Consequence(
                        "promise Person must be a participant or addressed invitee".into(),
                    ));
                }
                let window_end = terms
                    .window_end
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty());
                if let Some(value) = window_end.as_deref() {
                    let parsed = DateTime::parse_from_rfc3339(value).map_err(|_| {
                        EngineError::Consequence(format!(
                            "promise window `{value}` must be an RFC3339 date and time"
                        ))
                    })?;
                    if parsed.with_timezone(&Utc) <= now {
                        return Err(EngineError::Consequence(
                            "promise window must end in the future".into(),
                        ));
                    }
                }
                let condition = terms
                    .condition
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty());
                if let Some(value) = condition.as_deref() {
                    nucleus::expr::Expr::parse(value).map_err(|error| {
                        EngineError::Consequence(format!("invalid promise condition: {error}"))
                    })?;
                }
                let request_id = request_id.trim().to_string();
                let signer = self.signer.lock().await.clone();
                let fact_actor = actor
                    .clone()
                    .or_else(|| signer.as_ref().map(|value| value.actor_uid.clone()));
                match store::transfers::revise_promise(
                    &self.store.pool,
                    store::transfers::PromiseRevisionInput {
                        transfer_uid: transfer,
                        promise_uid: promise,
                        expected_revision,
                        idempotency_key: request_id,
                        record_uid,
                        person_uid,
                        delta: terms.delta,
                        window_end,
                        condition,
                        reserve_from: terms
                            .reserve_from
                            .map(|value| value.as_str().to_string())
                            .unwrap_or(source.reserve_from),
                    },
                    now,
                    fact_actor,
                    |hash| signer.as_ref().map(|value| value.sign_hash(hash)),
                )
                .await?
                {
                    store::transfers::RevisionCommit::Committed { fact, .. } => {
                        outcome.facts = self.observe_committed_fact(fact, now).await?;
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
            }
            Action::ReviseTransferDraft {
                transfer,
                expected_revision,
                request_id,
                draft,
            } => {
                let transfer = self.resolve(&transfer).await?;
                self.require_transfer_draft_creator(&transfer, actor.as_deref())
                    .await?;
                if let Some((replayed_transfer, _, replayed_action)) =
                    store::transfers::revision_for_request(&self.store.pool, request_id.trim())
                        .await?
                {
                    if replayed_transfer != transfer || replayed_action != "revise-transfer-draft" {
                        return Err(EngineError::Conflict {
                            code: "transfer_request_id_conflict",
                            message: "transfer request id belongs to another transfer".into(),
                        });
                    }
                    outcome.created = Some(transfer);
                    return Ok(outcome);
                }
                let current = store::transfers::get(&self.store.pool, &transfer)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(transfer.clone()))?;
                if current.revision == 0 {
                    return Err(EngineError::Conflict {
                        code: "transfer_legacy_adoption_required",
                        message: "legacy revision-0 terms require explicit review and adoption"
                            .into(),
                    });
                }
                let creator_person = self
                    .resolve_transfer_draft_creator_person(
                        &transfer,
                        &draft.creator,
                        expected_revision,
                        actor.as_deref(),
                    )
                    .await?;
                let mut input = self
                    .resolve_whole_transfer_draft(
                        transfer.clone(),
                        expected_revision,
                        request_id,
                        draft,
                        creator_person.clone(),
                        creator_person.clone(),
                        now,
                        false,
                    )
                    .await?;
                input.authorization_intent_uid = verified_authorship
                    .as_ref()
                    .map(|value| value.intent_uid.clone());
                let signer = self
                    .transfer_person_signer(&creator_person, verified_authorship.as_ref())
                    .await?;
                let commit = store::transfers::revise_whole_draft(
                    &self.store.pool,
                    input,
                    now,
                    Some(creator_person),
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
            Action::AdoptTransferDraft {
                transfer,
                request_id,
                draft,
            } => {
                let transfer = self.resolve(&transfer).await?;
                self.require_transfer_draft_creator(&transfer, actor.as_deref())
                    .await?;
                if let Some((replayed_transfer, _, replayed_action)) =
                    store::transfers::revision_for_request(&self.store.pool, request_id.trim())
                        .await?
                {
                    if replayed_transfer != transfer
                        || replayed_action != "adopt-legacy-transfer-draft"
                    {
                        return Err(EngineError::Conflict {
                            code: "transfer_request_id_conflict",
                            message: "transfer request id belongs to another transfer".into(),
                        });
                    }
                    outcome.created = Some(transfer);
                    return Ok(outcome);
                }
                let current = store::transfers::get(&self.store.pool, &transfer)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(transfer.clone()))?;
                if current.revision != 0 {
                    return Err(EngineError::Conflict {
                        code: "transfer_already_revisioned",
                        message: format!(
                            "transfer is already sealed at revision {}",
                            current.revision
                        ),
                    });
                }
                let creator_person = self
                    .resolve_transfer_draft_creator_person(
                        &transfer,
                        &draft.creator,
                        0,
                        actor.as_deref(),
                    )
                    .await?;
                let mut input = self
                    .resolve_whole_transfer_draft(
                        transfer.clone(),
                        0,
                        request_id,
                        draft,
                        creator_person.clone(),
                        creator_person.clone(),
                        now,
                        false,
                    )
                    .await?;
                input.authorization_intent_uid = verified_authorship
                    .as_ref()
                    .map(|value| value.intent_uid.clone());
                let signer = self
                    .transfer_person_signer(&creator_person, verified_authorship.as_ref())
                    .await?;
                let commit = store::transfers::adopt_legacy_draft(
                    &self.store.pool,
                    input,
                    now,
                    Some(creator_person),
                    |hash| signer.as_ref().map(|value| value.sign_hash(hash)),
                )
                .await?;
                self.apply_transfer_revision_commit(commit, 0, &transfer, &mut outcome)
                    .await?;
            }
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
                    return Ok(outcome);
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
                    return Ok(outcome);
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
                    return Ok(outcome);
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
                    return Ok(outcome);
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
                    return Ok(outcome);
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
                    return Ok(outcome);
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
                let submitted_creator = self.resolve(draft.creator.trim()).await?;
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
            Action::ClaimOpenTransferPromise {
                transfer,
                promise,
                expected_revision,
                request_id,
                person,
                terms,
            } => {
                let transfer = self.resolve(&transfer).await?;
                self.require_permission(actor.as_deref(), "transfer:update")
                    .await?;
                let claimant = self
                    .transfer_action_person(actor.as_deref(), person.as_deref(), None)
                    .await?;
                let replaying = if let Some((replayed_transfer, _, replayed_action)) =
                    store::transfers::revision_for_request(&self.store.pool, request_id.trim())
                        .await?
                {
                    if replayed_transfer != transfer
                        || replayed_action != "claim-open-transfer-promise"
                    {
                        return Err(transfer_request_id_conflict());
                    }
                    let Some((target_transfer, target_source, target_claimant, _)) =
                        store::transfers::open_claim_target_for_request(
                            &self.store.pool,
                            request_id.trim(),
                        )
                        .await?
                    else {
                        return Err(transfer_request_id_conflict());
                    };
                    if target_transfer != transfer
                        || target_source != promise
                        || target_claimant != claimant
                    {
                        return Err(transfer_request_id_conflict());
                    }
                    true
                } else {
                    false
                };
                if !replaying
                    && store::transfers::invitation_event_for_request(
                        &self.store.pool,
                        request_id.trim(),
                    )
                    .await?
                    .is_some()
                {
                    return Err(transfer_request_id_conflict());
                }
                let transfer_row = store::transfers::get(&self.store.pool, &transfer)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(transfer.clone()))?;
                let source = store::misc::get_promise(&self.store.pool, &promise)
                    .await?
                    .filter(|source| source.transfer_uid.as_deref() == Some(transfer.as_str()))
                    .ok_or_else(|| {
                        EngineError::Consequence("unknown OPEN transfer promise".into())
                    })?;
                if !replaying && (source.state != PromiseState::Open || source.party_uid.is_none())
                {
                    return Err(EngineError::Conflict {
                        code: "transfer_promise_not_open",
                        message: "only a current OPEN promise may be claimed".into(),
                    });
                }
                if !replaying && source.party_uid.as_deref() == Some(claimant.as_str()) {
                    return Err(EngineError::Conflict {
                        code: "transfer_open_self_claim_forbidden",
                        message: "an OPEN proposer cannot claim their own proposal".into(),
                    });
                }
                let invitations =
                    store::transfers::invitations_for_transfer(&self.store.pool, &transfer).await?;
                if !replaying
                    && invitations.iter().any(|invitation| {
                        invitation.addressed_person_uid == claimant
                            && invitation.status
                                == store::transfers::TransferInvitationStatus::Pending
                    })
                {
                    return Err(EngineError::Conflict {
                        code: "transfer_invitation_acceptance_required",
                        message: "an addressed Person must accept the invitation before claiming an OPEN promise"
                            .into(),
                    });
                }
                let participant =
                    store::transfers::party_for_actor(&self.store.pool, &transfer, &claimant)
                        .await?
                        .is_some();
                if !replaying && !participant && transfer_row.visibility != "public" {
                    return Err(EngineError::Forbidden(
                        "a non-participant may claim only a public OPEN promise".into(),
                    ));
                }
                if terms.withdrawn {
                    return Err(EngineError::Consequence(
                        "an OPEN claim cannot withdraw its refined promise".into(),
                    ));
                }
                if terms.open {
                    return Err(EngineError::Consequence(
                        "an OPEN claim must create a concrete claimant promise".into(),
                    ));
                }
                if terms.reuse_policy != source.open_reuse_policy {
                    return Err(EngineError::Conflict {
                        code: "transfer_open_reuse_policy_mismatch",
                        message: "the claim must use the OPEN proposal's signed reuse policy"
                            .into(),
                    });
                }
                if terms
                    .party
                    .as_deref()
                    .is_some_and(|token| token.trim() != claimant)
                {
                    let party = self.resolve(terms.party.as_deref().unwrap().trim()).await?;
                    if party != claimant {
                        return Err(EngineError::Consequence(
                            "a claimed promise belongs to the claiming Person".into(),
                        ));
                    }
                }
                if !terms.delta.is_finite() || terms.delta == 0.0 {
                    return Err(EngineError::Consequence(
                        "claimed promise delta must be finite and non-zero".into(),
                    ));
                }
                if source.delta.signum() == terms.delta.signum() {
                    return Err(EngineError::Conflict {
                        code: "transfer_open_claim_direction_mismatch",
                        message: "claimant quantity must have the opposite direction from the OPEN proposal"
                            .into(),
                    });
                }
                let resolved_record_uid = self.resolve(terms.record.trim()).await?;
                let concept_uid = store::records::get(&self.store.pool, &resolved_record_uid)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(resolved_record_uid.clone()))?
                    .identity_predicate_uid;
                let record_uid = Some(resolved_record_uid);
                let unit_uid = self.resolve_concept_opt(terms.unit).await?;
                let replay_window_end = terms.window_end.clone();
                let (window_start, window_end) = normalize_transfer_window(
                    terms.window_start,
                    terms.window_end,
                    now,
                    if replaying {
                        replay_window_end.as_deref()
                    } else {
                        source.window_end.as_deref()
                    },
                )?;
                let condition = terms
                    .condition
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty());
                if let Some(value) = condition.as_deref() {
                    nucleus::expr::Expr::parse(value).map_err(|error| {
                        EngineError::Consequence(format!("invalid promise condition: {error}"))
                    })?;
                }
                let signer = if replaying {
                    None
                } else {
                    self.transfer_person_signer(&claimant, verified_authorship.as_ref())
                        .await?
                };
                let commit = store::transfers::claim_open_promise(
                    &self.store.pool,
                    store::transfers::OpenPromiseClaimInput {
                        transfer_uid: transfer,
                        source_promise_uid: promise,
                        expected_revision,
                        idempotency_key: request_id.trim().to_string(),
                        claimant_person_uid: claimant,
                        record_uid,
                        concept_uid,
                        unit_uid,
                        delta: terms.delta,
                        window_start,
                        window_end,
                        location: normalize_transfer_place(terms.place)?,
                        condition,
                        reserve_from: terms
                            .reserve_from
                            .map(|value| value.as_str().to_string())
                            .unwrap_or(source.reserve_from),
                        authorization_intent_uid: verified_authorship
                            .as_ref()
                            .map(|value| value.intent_uid.clone()),
                    },
                    now,
                    |hash| signer.as_ref().map(|value| value.sign_hash(hash)),
                )
                .await?;
                self.apply_open_promise_claim_commit(commit, expected_revision, &mut outcome)?;
            }
            Action::SetTransferAgreementLevel {
                transfer,
                expected_revision,
                request_id,
                person,
                level,
            } => {
                if level > 2 {
                    return Err(EngineError::Consequence(
                        "agreement level must be 0, 1, or 2".into(),
                    ));
                }
                let transfer = self.resolve(&transfer).await?;
                self.require_permission(actor.as_deref(), "transfer:update")
                    .await?;
                let acting = self
                    .transfer_action_person(actor.as_deref(), person.as_deref(), None)
                    .await?;
                if let Some(event) =
                    store::transfers::agreement_event_for_request(&self.store.pool, &request_id)
                        .await?
                {
                    if event.transfer_uid != transfer
                        || event.revision != expected_revision
                        || event.person_uid != acting
                        || event.to_level != level
                    {
                        return Err(transfer_request_id_conflict());
                    }
                    outcome.created = Some(event.uid);
                    return Ok(outcome);
                }
                if store::transfers::party_for_actor(&self.store.pool, &transfer, &acting)
                    .await?
                    .is_none()
                {
                    return Err(EngineError::Forbidden(
                        "agreement may be authored only by an accepted Transfer participant".into(),
                    ));
                }
                if store::transfers::revision_for_request(&self.store.pool, &request_id)
                    .await?
                    .is_some()
                    || store::transfers::invitation_event_for_request(&self.store.pool, &request_id)
                        .await?
                        .is_some()
                {
                    return Err(transfer_request_id_conflict());
                }
                self.require_verified_transfer_revision(&transfer, expected_revision)
                    .await?;
                let signer = self
                    .transfer_person_signer(&acting, verified_authorship.as_ref())
                    .await?;
                let commit = store::transfers::transition_agreement(
                    &self.store.pool,
                    store::transfers::AgreementTransitionInput {
                        transfer_uid: transfer.clone(),
                        expected_revision,
                        idempotency_key: request_id,
                        person_uid: acting,
                        to_level: level,
                        authorization_intent_uid: verified_authorship
                            .as_ref()
                            .map(|value| value.intent_uid.clone()),
                    },
                    now,
                    |hash| signer.as_ref().map(|value| value.sign_hash(hash)),
                )
                .await?;
                match commit {
                    store::transfers::AgreementTransitionCommit::Committed(committed) => {
                        outcome.facts = self.publish_committed_fact(committed.fact);
                        outcome.created = Some(committed.event.uid);
                    }
                    store::transfers::AgreementTransitionCommit::Replayed(replayed) => {
                        outcome.created = Some(replayed.event.uid);
                    }
                    store::transfers::AgreementTransitionCommit::Stale {
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
            }
            Action::ActivateTransferOccurrence {
                transfer,
                promise,
                expected_revision,
                request_id,
                person,
            } => {
                let transfer = self.resolve(&transfer).await?;
                self.require_permission(actor.as_deref(), "transfer:update")
                    .await?;
                let acting = self
                    .transfer_action_person(actor.as_deref(), person.as_deref(), None)
                    .await?;
                let promise = promise.trim().to_string();
                if let Some(replayed) = store::transfers::occurrences_for_activation_request(
                    &self.store.pool,
                    &request_id,
                )
                .await?
                {
                    if replayed.transfer_uid != transfer
                        || replayed.revision != expected_revision
                        || replayed.actor_person_uid != acting
                        || replayed.occurrences.len() != 1
                        || replayed.occurrences[0].promise_uid != promise
                    {
                        return Err(transfer_request_id_conflict());
                    }
                    outcome.created = Some(replayed.occurrences[0].uid.clone());
                    return Ok(outcome);
                }
                if store::transfers::phase4_request_for_request(&self.store.pool, &request_id)
                    .await?
                    .is_some()
                {
                    return Err(transfer_request_id_conflict());
                }
                if store::transfers::source_group_state_for_transfer(&self.store.pool, &transfer)
                    .await?
                    .is_some_and(|state| state.satiated)
                {
                    return Err(EngineError::Conflict {
                        code: "transfer_satiated",
                        message:
                            "another Transfer already completed this first-completes source group"
                                .into(),
                    });
                }
                if store::transfers::party_for_actor(&self.store.pool, &transfer, &acting)
                    .await?
                    .is_none()
                {
                    return Err(EngineError::Forbidden(
                        "occurrence activation requires an accepted Transfer participant".into(),
                    ));
                }
                self.require_verified_transfer_revision(&transfer, expected_revision)
                    .await?;
                let ready =
                    protein::transfer_ready_promises_for_person(&self.store, &transfer, &acting)
                        .await?;
                if !ready.contains(&promise) {
                    return Err(EngineError::Conflict {
                        code: "transfer_promise_not_ready",
                        message: "the selected promise is not policy-ready for the acting Person"
                            .into(),
                    });
                }
                let readiness =
                    store::transfers::agreement_readiness_input(&self.store.pool, &transfer)
                        .await?;
                if readiness.revision != expected_revision {
                    return Err(EngineError::Conflict {
                        code: "transfer_revision_stale",
                        message: format!(
                            "expected transfer revision {expected_revision}, current revision is {}",
                            readiness.revision
                        ),
                    });
                }
                let (opposite_promise_uid, giver_person_uid, receiver_person_uid) =
                    protein::transfer_occurrence_roles(&readiness, &promise).map_err(|code| {
                        EngineError::Conflict {
                            code,
                            message: "the selected promise does not have one unambiguous directed counterparty"
                                .into(),
                        }
                    })?;
                let signer = self
                    .transfer_person_signer(&acting, verified_authorship.as_ref())
                    .await?;
                let commit = store::transfers::activate_occurrences(
                    &self.store.pool,
                    store::transfers::ActivateOccurrencesInput {
                        transfer_uid: transfer,
                        expected_revision,
                        idempotency_key: request_id,
                        actor_person_uid: acting,
                        occurrences: vec![store::transfers::OccurrenceActivationInput {
                            promise_uid: promise,
                            opposite_promise_uid,
                            giver_person_uid,
                            receiver_person_uid,
                        }],
                        authorization_intent_uid: verified_authorship
                            .as_ref()
                            .map(|value| value.intent_uid.clone()),
                    },
                    now,
                    |hash| signer.as_ref().map(|value| value.sign_hash(hash)),
                )
                .await?;
                match commit {
                    store::transfers::OccurrenceActivationCommit::Committed(committed) => {
                        outcome.created = committed
                            .occurrences
                            .first()
                            .map(|occurrence| occurrence.uid.clone());
                        outcome.facts = self.publish_committed_fact(committed.fact);
                    }
                    store::transfers::OccurrenceActivationCommit::Replayed(replayed) => {
                        outcome.created = replayed
                            .occurrences
                            .first()
                            .map(|occurrence| occurrence.uid.clone());
                    }
                    store::transfers::OccurrenceActivationCommit::Stale {
                        current_revision,
                        ..
                    } => {
                        return Err(EngineError::Conflict {
                            code: "transfer_revision_stale",
                            message: format!(
                                "expected transfer revision {expected_revision}, current revision is {current_revision}"
                            ),
                        });
                    }
                    store::transfers::OccurrenceActivationCommit::Satiated {
                        winner_transfer_uid,
                    } => {
                        return Err(EngineError::Conflict {
                            code: "transfer_satiated",
                            message: format!(
                                "Transfer cannot activate because `{winner_transfer_uid}` completed its first-completes source group"
                            ),
                        });
                    }
                }
            }
            Action::SetTransferOccurrenceClaim {
                occurrence,
                request_id,
                person,
                role,
                claimed,
            } => {
                let acting = self
                    .transfer_action_person(actor.as_deref(), person.as_deref(), None)
                    .await?;
                self.require_permission(actor.as_deref(), "transfer:update")
                    .await?;
                let role = match role {
                    TransferOccurrenceClaimRole::Delivery => {
                        nucleus::transfer::OccurrenceClaimRole::Delivery
                    }
                    TransferOccurrenceClaimRole::Receipt => {
                        nucleus::transfer::OccurrenceClaimRole::Receipt
                    }
                };
                if let Some(replayed) =
                    store::transfers::occurrence_claim_for_request(&self.store.pool, &request_id)
                        .await?
                {
                    if replayed.event.occurrence_uid != occurrence
                        || replayed.event.actor_person_uid != acting
                        || replayed.event.role != role
                        || replayed.event.asserted != claimed
                    {
                        return Err(transfer_request_id_conflict());
                    }
                    outcome.created = Some(replayed.event.uid);
                    return Ok(outcome);
                }
                if store::transfers::phase4_request_for_request(&self.store.pool, &request_id)
                    .await?
                    .is_some()
                {
                    return Err(transfer_request_id_conflict());
                }
                let signer = self
                    .transfer_person_signer(&acting, verified_authorship.as_ref())
                    .await?;
                let commit = store::transfers::set_occurrence_claim(
                    &self.store.pool,
                    store::transfers::OccurrenceClaimInput {
                        occurrence_uid: occurrence,
                        idempotency_key: request_id,
                        actor_person_uid: acting,
                        role,
                        asserted: claimed,
                        authorization_intent_uid: verified_authorship
                            .as_ref()
                            .map(|value| value.intent_uid.clone()),
                    },
                    now,
                    |hash| signer.as_ref().map(|value| value.sign_hash(hash)),
                )
                .await?;
                match commit {
                    store::transfers::OccurrenceClaimCommit::Committed(committed) => {
                        outcome.created = Some(committed.event.uid);
                        outcome.facts = self.publish_committed_fact(committed.fact);
                    }
                    store::transfers::OccurrenceClaimCommit::Replayed(replayed) => {
                        outcome.created = Some(replayed.event.uid);
                    }
                }
            }
            Action::CompleteTransferOccurrenceClaimsBulk {
                request_id,
                person,
                review_token,
                items,
            } => {
                let acting = self
                    .transfer_action_person(actor.as_deref(), person.as_deref(), None)
                    .await?;
                self.require_permission(actor.as_deref(), "transfer:update")
                    .await?;
                let items = items
                    .into_iter()
                    .map(|item| store::transfers::ReviewedBulkOccurrenceClaim {
                        occurrence_uid: item.occurrence,
                        transfer_uid: item.transfer,
                        expected_revision: item.expected_revision,
                        role: match item.role {
                            TransferOccurrenceClaimRole::Delivery => {
                                nucleus::transfer::OccurrenceClaimRole::Delivery
                            }
                            TransferOccurrenceClaimRole::Receipt => {
                                nucleus::transfer::OccurrenceClaimRole::Receipt
                            }
                        },
                        expected_delivery_claimed: item.expected_delivery_claimed,
                        expected_receipt_claimed: item.expected_receipt_claimed,
                    })
                    .collect();
                let signer = self
                    .transfer_person_signer(&acting, verified_authorship.as_ref())
                    .await?;
                let commit = store::transfers::complete_occurrence_claims_bulk(
                    &self.store.pool,
                    store::transfers::BulkOccurrenceClaimInput {
                        idempotency_key: request_id,
                        actor_person_uid: acting,
                        review_token,
                        items,
                        authorization_intent_uid: verified_authorship
                            .as_ref()
                            .map(|value| value.intent_uid.clone()),
                    },
                    now,
                    |hash| signer.as_ref().map(|value| value.sign_hash(hash)),
                )
                .await?;
                match commit {
                    store::transfers::BulkOccurrenceClaimCommit::Committed(committed) => {
                        outcome.created = Some(committed.uid);
                        for fact in committed.facts {
                            outcome.facts.extend(self.publish_committed_fact(fact));
                        }
                    }
                    store::transfers::BulkOccurrenceClaimCommit::Replayed(replayed) => {
                        outcome.created = Some(replayed.uid);
                    }
                    store::transfers::BulkOccurrenceClaimCommit::Rejected(failures) => {
                        let message = failures
                            .into_iter()
                            .map(|failure| {
                                format!(
                                    "{} [{}]: {}",
                                    failure.occurrence_uid, failure.code, failure.message
                                )
                            })
                            .collect::<Vec<_>>()
                            .join("; ");
                        return Err(EngineError::Conflict {
                            code: "transfer_bulk_preflight_failed",
                            message,
                        });
                    }
                }
            }
            Action::SetTransferOccurrenceDispute {
                occurrence,
                request_id,
                person,
                disputed,
            } => {
                let acting = self
                    .transfer_action_person(actor.as_deref(), person.as_deref(), None)
                    .await?;
                self.require_permission(actor.as_deref(), "transfer:update")
                    .await?;
                if let Some(replayed) =
                    store::transfers::occurrence_dispute_for_request(&self.store.pool, &request_id)
                        .await?
                {
                    if replayed.event.occurrence_uid != occurrence
                        || replayed.event.actor_person_uid != acting
                        || replayed.event.disputed != disputed
                    {
                        return Err(transfer_request_id_conflict());
                    }
                    outcome.created = Some(replayed.event.uid);
                    return Ok(outcome);
                }
                if store::transfers::phase5_correction_request_for_request(
                    &self.store.pool,
                    &request_id,
                )
                .await?
                .is_some()
                {
                    return Err(transfer_request_id_conflict());
                }
                let occurrence_row = store::transfers::occurrence(&self.store.pool, &occurrence)
                    .await?
                    .ok_or_else(|| EngineError::Conflict {
                        code: "transfer_occurrence_missing",
                        message: "the occurrence does not exist".into(),
                    })?;
                if occurrence_row.giver_person_uid != acting
                    && occurrence_row.receiver_person_uid != acting
                {
                    return Err(EngineError::Conflict {
                        code: "transfer_occurrence_dispute_not_participant",
                        message: "only the occurrence giver or receiver may assert a dispute"
                            .into(),
                    });
                }
                let signer = self
                    .transfer_person_signer(&acting, verified_authorship.as_ref())
                    .await?;
                let commit = store::transfers::set_occurrence_dispute(
                    &self.store.pool,
                    store::transfers::OccurrenceDisputeInput {
                        occurrence_uid: occurrence,
                        idempotency_key: request_id,
                        actor_person_uid: acting,
                        disputed,
                        authorization_intent_uid: verified_authorship
                            .as_ref()
                            .map(|value| value.intent_uid.clone()),
                    },
                    now,
                    |hash| signer.as_ref().map(|value| value.sign_hash(hash)),
                )
                .await?;
                match commit {
                    store::transfers::OccurrenceDisputeCommit::Committed(committed) => {
                        outcome.created = Some(committed.event.uid);
                        outcome.facts = self.publish_committed_fact(committed.fact);
                    }
                    store::transfers::OccurrenceDisputeCommit::Replayed(replayed) => {
                        outcome.created = Some(replayed.event.uid);
                    }
                }
            }
            Action::SetTransferOccurrenceApplicationFormula {
                occurrence,
                request_id,
                person,
                formula,
            } => {
                let acting = self
                    .transfer_action_person(actor.as_deref(), person.as_deref(), None)
                    .await?;
                self.require_permission(actor.as_deref(), "transfer:update")
                    .await?;
                let formula = validate_transfer_application_formula(&formula)?;
                if let Some(replayed) =
                    store::transfers::occurrence_application_formula_for_request(
                        &self.store.pool,
                        &request_id,
                    )
                    .await?
                {
                    let formula_hash =
                        nucleus::transfer::occurrence_application_formula_hash(&formula);
                    if replayed.event.occurrence_uid != occurrence
                        || replayed.event.receiver_person_uid != acting
                        || replayed.event.formula_hash != formula_hash
                    {
                        return Err(transfer_request_id_conflict());
                    }
                    outcome.created = Some(replayed.event.uid);
                    return Ok(outcome);
                }
                if store::transfers::phase4_request_for_request(&self.store.pool, &request_id)
                    .await?
                    .is_some()
                {
                    return Err(transfer_request_id_conflict());
                }
                let occurrence_row = store::transfers::occurrence(&self.store.pool, &occurrence)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(occurrence.clone()))?;
                if occurrence_row.receiver_person_uid != acting {
                    return Err(EngineError::Forbidden(
                        "only the occurrence receiver may set its private application formula"
                            .into(),
                    ));
                }
                evaluate_transfer_application_formula(&formula, occurrence_row.quantity)?;
                let signer = self
                    .transfer_person_signer(&acting, verified_authorship.as_ref())
                    .await?;
                let commit = store::transfers::set_occurrence_application_formula(
                    &self.store.pool,
                    store::transfers::OccurrenceApplicationFormulaInput {
                        occurrence_uid: occurrence,
                        idempotency_key: request_id,
                        actor_person_uid: acting,
                        formula,
                        authorization_intent_uid: verified_authorship
                            .as_ref()
                            .map(|value| value.intent_uid.clone()),
                    },
                    now,
                    |hash| signer.as_ref().map(|value| value.sign_hash(hash)),
                )
                .await?;
                match commit {
                    store::transfers::OccurrenceApplicationFormulaCommit::Committed(committed) => {
                        outcome.created = Some(committed.event.uid);
                        outcome.facts = self.publish_committed_fact(committed.fact);
                    }
                    store::transfers::OccurrenceApplicationFormulaCommit::Replayed(replayed) => {
                        outcome.created = Some(replayed.event.uid);
                    }
                }
            }
            Action::SettleTransferOccurrence {
                occurrence,
                request_id,
                person,
                canonical_quantity,
                expected_remaining_quantity,
                expected_local_delta,
                expected_application_formula_hash,
                expected_application_formula_version,
                expected_remainder_policy,
            } => {
                let acting = self
                    .transfer_action_person(actor.as_deref(), person.as_deref(), None)
                    .await?;
                self.require_permission(actor.as_deref(), "transfer:update")
                    .await?;
                if !canonical_quantity.is_finite() || canonical_quantity <= 0.0 {
                    return Err(transfer_settlement_conflict(
                        "transfer_settlement_quantity_invalid",
                        "settlement quantity must be finite and positive",
                    ));
                }
                if !expected_remaining_quantity.is_finite() || !expected_local_delta.is_finite() {
                    return Err(transfer_settlement_conflict(
                        "transfer_settlement_preview_invalid",
                        "settlement preview quantities must be finite",
                    ));
                }

                if let Some(replayed) = store::transfers::occurrence_settlement_for_request(
                    &self.store.pool,
                    &request_id,
                )
                .await?
                {
                    let slice = &replayed.slice;
                    let replay_remaining_before = slice.remaining_after + slice.canonical_quantity;
                    if slice.occurrence_uid != occurrence
                        || slice.owner_person_uid != acting
                        || !transfer_settlement_values_match(
                            slice.canonical_quantity,
                            canonical_quantity,
                        )
                        || !transfer_settlement_values_match(
                            replay_remaining_before,
                            expected_remaining_quantity,
                        )
                        || !transfer_settlement_values_match(
                            slice.local_delta,
                            expected_local_delta,
                        )
                        || slice.application_formula_hash != expected_application_formula_hash
                        || slice.application_formula_version != expected_application_formula_version
                        || slice.remainder_policy != expected_remainder_policy
                    {
                        return Err(transfer_request_id_conflict());
                    }
                    outcome.created = Some(slice.uid.clone());
                    return Ok(outcome);
                }
                if store::transfers::phase5_request_for_request(&self.store.pool, &request_id)
                    .await?
                    .is_some()
                    || store::transfers::phase4_request_for_request(&self.store.pool, &request_id)
                        .await?
                        .is_some()
                {
                    return Err(transfer_request_id_conflict());
                }

                let occurrence_row = store::transfers::occurrence(&self.store.pool, &occurrence)
                    .await?
                    .ok_or_else(|| {
                        transfer_settlement_conflict(
                            "transfer_settlement_occurrence_missing",
                            "the settlement occurrence no longer exists",
                        )
                    })?;
                let source =
                    store::misc::get_promise(&self.store.pool, &occurrence_row.promise_uid)
                        .await?
                        .ok_or_else(|| {
                            transfer_settlement_conflict(
                                "transfer_settlement_source_missing",
                                "the occurrence source promise no longer exists",
                            )
                        })?;
                let Some(local_record_uid) = source.record_uid.as_deref() else {
                    return Err(transfer_settlement_conflict(
                        "transfer_settlement_concrete_record_required",
                        "a concept-only promise must be refined to a concrete Record before settlement",
                    ));
                };
                if occurrence_row.record_uid.as_deref() != Some(local_record_uid) {
                    return Err(transfer_settlement_conflict(
                        "transfer_settlement_source_mismatch",
                        "the occurrence no longer matches its signed source promise",
                    ));
                }
                let local_record = store::records::get(&self.store.pool, local_record_uid)
                    .await?
                    .ok_or_else(|| {
                        transfer_settlement_conflict(
                            "transfer_settlement_local_record_missing",
                            "the source promise's local Record is unavailable",
                        )
                    })?;
                let local_organ_uid = store::organs::local(&self.store.pool)
                    .await?
                    .map(|organ| organ.uid);
                if local_record
                    .organ_uid
                    .as_deref()
                    .is_some_and(|origin| Some(origin) != local_organ_uid.as_deref())
                {
                    return Err(transfer_settlement_conflict(
                        "transfer_settlement_foreign_record",
                        "settlement cannot alter a Record originating in another Cell",
                    ));
                }
                if source.party_uid.as_deref() != Some(acting.as_str()) {
                    return Err(transfer_settlement_conflict(
                        "transfer_settlement_not_source_owner",
                        "only the concrete source-promise owner may apply this occurrence",
                    ));
                }
                if source.delta == 0.0 || !source.delta.is_finite() {
                    return Err(transfer_settlement_conflict(
                        "transfer_settlement_direction_invalid",
                        "the source promise must have a finite non-zero direction",
                    ));
                }
                if source.state != PromiseState::Active {
                    return Err(transfer_settlement_conflict(
                        "transfer_settlement_not_active",
                        "only an active occurrence may be settled",
                    ));
                }
                if !occurrence_row.delivery_claimed || !occurrence_row.receipt_claimed {
                    return Err(transfer_settlement_conflict(
                        "transfer_settlement_confirmation_required",
                        "both delivery and receipt must currently be confirmed",
                    ));
                }
                if occurrence_row.disputed {
                    return Err(transfer_settlement_conflict(
                        "transfer_settlement_disputed",
                        "a disputed occurrence cannot be settled",
                    ));
                }

                let progress =
                    store::transfers::occurrence_settlement_progress(&self.store.pool, &occurrence)
                        .await?
                        .ok_or_else(|| {
                            transfer_settlement_conflict(
                                "transfer_settlement_occurrence_missing",
                                "the settlement occurrence no longer exists",
                            )
                        })?;
                if !transfer_settlement_values_match(
                    progress.remaining_quantity,
                    expected_remaining_quantity,
                ) {
                    return Err(transfer_settlement_conflict(
                        "transfer_settlement_remaining_stale",
                        format!(
                            "reviewed remaining quantity was {expected_remaining_quantity}, current remaining quantity is {}",
                            progress.remaining_quantity
                        ),
                    ));
                }
                if canonical_quantity > progress.remaining_quantity {
                    return Err(transfer_settlement_conflict(
                        "transfer_settlement_quantity_exceeds_remaining",
                        "settlement quantity exceeds the occurrence's remaining quantity",
                    ));
                }

                let (application_formula, application_formula_version) = if source.delta < 0.0 {
                    if occurrence_row.giver_person_uid != acting {
                        return Err(transfer_settlement_conflict(
                            "transfer_settlement_direction_mismatch",
                            "the giving source promise does not match the occurrence giver",
                        ));
                    }
                    ("-incoming()".to_string(), 0)
                } else {
                    if occurrence_row.receiver_person_uid != acting {
                        return Err(transfer_settlement_conflict(
                            "transfer_settlement_direction_mismatch",
                            "the receiving source promise does not match the occurrence receiver",
                        ));
                    }
                    match store::transfers::occurrence_application_policy(
                        &self.store.pool,
                        &occurrence,
                        &acting,
                    )
                    .await?
                    {
                        Some(policy) => (policy.formula, policy.version),
                        None => {
                            let configured =
                                store::config::transfer_application_formula(&self.store.pool)
                                    .await?;
                            let inherited = validate_transfer_application_formula(&configured)
                                .unwrap_or_else(|_| "incoming()".into());
                            (inherited, 0)
                        }
                    }
                };
                let application_formula =
                    validate_transfer_application_formula(&application_formula).map_err(|_| {
                        transfer_settlement_conflict(
                            "transfer_settlement_formula_invalid",
                            "the effective private application formula is invalid",
                        )
                    })?;
                let application_formula_hash =
                    nucleus::transfer::occurrence_application_formula_hash(&application_formula);
                if application_formula_hash != expected_application_formula_hash
                    || application_formula_version != expected_application_formula_version
                {
                    return Err(transfer_settlement_conflict(
                        "transfer_settlement_formula_stale",
                        "the private application formula changed after settlement review",
                    ));
                }
                let remainder_policy = store::transfers::effective_occurrence_remainder_policy(
                    &self.store.pool,
                    &occurrence,
                    &acting,
                )
                .await?;
                if remainder_policy != expected_remainder_policy {
                    return Err(transfer_settlement_conflict(
                        "transfer_settlement_remainder_policy_stale",
                        "the remainder policy changed after settlement review",
                    ));
                }

                let canonical_cumulative_after = progress.settled_quantity + canonical_quantity;
                let local_cumulative_before = progress
                    .slices
                    .iter()
                    .map(|slice| slice.local_delta)
                    .sum::<f64>();
                let local_cumulative_after = if source.delta < 0.0 {
                    -canonical_cumulative_after
                } else {
                    evaluate_transfer_application_formula(
                        &application_formula,
                        canonical_cumulative_after,
                    )
                    .map_err(|_| {
                        transfer_settlement_conflict(
                            "transfer_settlement_formula_evaluation_failed",
                            "the private application formula cannot evaluate this cumulative quantity",
                        )
                    })?
                };
                let local_delta = local_cumulative_after - local_cumulative_before;
                if !local_delta.is_finite()
                    || !transfer_settlement_values_match(local_delta, expected_local_delta)
                {
                    return Err(transfer_settlement_conflict(
                        "transfer_settlement_preview_stale",
                        format!(
                            "reviewed local delta was {expected_local_delta}, current local delta is {local_delta}"
                        ),
                    ));
                }

                let signer = self
                    .transfer_person_signer(&acting, verified_authorship.as_ref())
                    .await?;
                let commit = store::transfers::settle_occurrence(
                    &self.store.pool,
                    store::transfers::OccurrenceSettlementInput {
                        occurrence_uid: occurrence,
                        idempotency_key: request_id,
                        actor_person_uid: acting,
                        canonical_quantity,
                        local_record_uid: local_record_uid.to_string(),
                        local_delta,
                        local_cumulative_after,
                        application_formula,
                        application_formula_version,
                        remainder_policy,
                        authorization_intent_uid: verified_authorship
                            .as_ref()
                            .map(|value| value.intent_uid.clone()),
                    },
                    now,
                    |hash| signer.as_ref().map(|value| value.sign_hash(hash)),
                )
                .await?;
                match commit {
                    store::transfers::OccurrenceSettlementCommit::Committed(committed) => {
                        outcome.created = Some(committed.slice.uid);
                        outcome.facts = self.publish_committed_fact(committed.evidence_fact);
                        outcome
                            .facts
                            .extend(self.publish_committed_fact(committed.application_fact));
                        for fact in committed.satiation_facts {
                            outcome.facts.extend(self.publish_committed_fact(fact));
                        }
                    }
                    store::transfers::OccurrenceSettlementCommit::Replayed(replayed) => {
                        outcome.created = Some(replayed.slice.uid);
                    }
                }
            }
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
            Action::BeginRemoteTransferSettlement {
                transfer,
                occurrence,
                expected_revision,
                expected_remaining_quantity,
                canonical_quantity,
                request_id,
                person,
            } => {
                let transfer = self.resolve(&transfer).await?;
                let acting = self
                    .transfer_action_person(actor.as_deref(), Some(&person), Some(&person))
                    .await?;
                let handoff = self
                    .begin_remote_transfer_settlement(
                        &transfer,
                        &occurrence,
                        &acting,
                        expected_revision,
                        expected_remaining_quantity,
                        canonical_quantity,
                        &request_id,
                        now,
                    )
                    .await?;
                outcome.created = Some(handoff.uid);
            }
            Action::ApplyRemoteTransferApplication {
                transfer,
                handoff,
                local_record,
                expected_formula_hash,
                expected_formula_version,
                request_id,
                person,
            } => {
                let acting = self
                    .transfer_action_person(actor.as_deref(), Some(&person), Some(&person))
                    .await?;
                let remote = store::transfer_delivery::remote_application_handoff(
                    &self.store.pool,
                    &handoff,
                )
                .await?
                .ok_or_else(|| EngineError::UnknownRecord(handoff.clone()))?;
                if remote.transfer_uid != transfer
                    || remote.participant_person_uid != acting
                    || remote.state != "pending"
                {
                    return Err(EngineError::Conflict {
                        code: "transfer_remote_application_not_pending",
                        message: "application handoff is not pending for this Person".into(),
                    });
                }
                let local_record = self.resolve(&local_record).await?;
                let formula = if remote.application_direction < 0 {
                    "-incoming()".to_string()
                } else {
                    validate_transfer_application_formula(
                        &store::config::transfer_application_formula(&self.store.pool).await?,
                    )?
                };
                let formula_version = 0_u64;
                let formula_hash = nucleus::transfer::occurrence_application_formula_hash(&formula);
                if formula_hash != expected_formula_hash
                    || formula_version != expected_formula_version
                {
                    return Err(EngineError::Conflict {
                        code: "transfer_remote_application_formula_stale",
                        message: "private application formula changed after review".into(),
                    });
                }
                let local_before: f64 = store::sqlx::query_scalar(
                    "SELECT COALESCE(SUM(a.local_delta), 0.0)
                     FROM transfer_local_application a
                     JOIN transfer_remote_application_handoff h ON h.uid = a.handoff_uid
                     WHERE h.occurrence_uid = ? AND h.participant_person_uid = ?",
                )
                .bind(&remote.occurrence_uid)
                .bind(&acting)
                .fetch_one(&self.store.pool)
                .await?;
                let local_after = evaluate_transfer_application_formula(
                    &formula,
                    remote.canonical_cumulative_after,
                )?;
                let local_delta = local_after - local_before;
                let signer = self
                    .transfer_person_signer(&acting, verified_authorship.as_ref())
                    .await?
                    .ok_or_else(|| EngineError::Conflict {
                        code: "missing_person_signer",
                        message:
                            "application attestation requires the participant Person signing key"
                                .into(),
                    })?;
                let commit = store::transfer_delivery::apply_remote_transfer_locally(
                    &self.store.pool,
                    store::transfer_delivery::NewLocalTransferApplication {
                        handoff_uid: handoff.clone(),
                        participant_person_uid: acting.clone(),
                        local_record_uid: local_record,
                        local_delta,
                        local_cumulative_before: local_before,
                        local_cumulative_after: local_after,
                        application_formula: formula,
                        application_formula_hash: formula_hash.clone(),
                        application_formula_version: formula_version,
                        authorization_intent_uid: verified_authorship
                            .as_ref()
                            .map(|value| value.intent_uid.clone()),
                        request_id: request_id.clone(),
                    },
                    now,
                    |hash| Some(signer.sign_hash(hash)),
                )
                .await?;
                let mut attestation =
                    nucleus::transfer_delivery::TransferApplicationAttestationV1 {
                        version: nucleus::transfer_delivery::TRANSFER_ENVELOPE_VERSION,
                        attestation_uid: format!("taa:{}", commit.application.uid),
                        origin_organ_uid: remote.origin_organ_uid.clone(),
                        participant_organ_uid: remote.participant_organ_uid.clone(),
                        participant_person_uid: acting,
                        transfer_uid: remote.transfer_uid,
                        occurrence_uid: remote.occurrence_uid,
                        settlement_slice_uid: remote.settlement_slice_uid,
                        origin_revision: remote.origin_revision,
                        canonical_slice_hash: remote.canonical_slice_hash,
                        formula_commitment: formula_hash,
                        formula_version: formula_version.to_string(),
                        application_fact_uid: commit.fact.uid.clone(),
                        applied_at: now.to_rfc3339(),
                        key_id: signer.key_id.clone(),
                        signature: String::new(),
                    };
                attestation.signature = signer.sign_bytes(&attestation.signing_bytes());
                attestation
                    .validate_shape()
                    .map_err(EngineError::Consequence)?;
                store::transfer_delivery::enqueue_application_attestation(
                    &self.store.pool,
                    &handoff,
                    &remote.reference_uid,
                    &remote.origin_organ_uid,
                    &attestation,
                    now,
                )
                .await?;
                outcome.created = Some(commit.application.uid);
                outcome.facts = self.publish_committed_fact(commit.fact);
            }
            Action::ConfirmTransfer {
                transfer,
                confirmation,
            } => {
                if transfer_phase_locked() {
                    return Err(EngineError::Conflict {
                        code: "transfer_phase_4_not_available",
                        message: "occurrence-specific confirmation is not available yet".into(),
                    });
                }
                if !matches!(confirmation.as_str(), "delivery" | "receipt") {
                    return Err(EngineError::Consequence(format!(
                        "confirmation must be `delivery` or `receipt`, not `{confirmation}`"
                    )));
                }
                let transfer = self.resolve(&transfer).await?;
                self.require_transfer_participant(&transfer, actor.as_deref())
                    .await?;
                outcome.facts = self
                    .append(
                        NewFact {
                            uid: None,
                            record_uid: transfer.clone(),
                            delta: nucleus::fact::zero_delta(),
                            at: None,
                            actor_uid: actor,
                            cause: Cause::settlement(transfer),
                            payload: Some(
                                serde_json::json!({ "confirmation": confirmation }).to_string(),
                            ),
                        },
                        now,
                    )
                    .await?;
            }
            Action::AddParty {
                transfer,
                actor: person,
            } => {
                if transfer_phase_locked() {
                    return Err(EngineError::Conflict {
                        code: "transfer_invitation_required",
                        message:
                            "a Person becomes a transfer party only by accepting an invitation"
                                .into(),
                    });
                }
                let transfer = self.resolve(&transfer).await?;
                self.require_transfer_editor(&transfer, actor.as_deref())
                    .await?;
                let person = self.resolve(&person).await?;
                let actor_record = store::records::get(&self.store.pool, &person)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(person.clone()))?;
                if actor_record.kind != RecordKind::Person.as_str() {
                    return Err(EngineError::Consequence(
                        "transfer parties must be person records".into(),
                    ));
                }
                let party =
                    store::transfers::add_party(&self.store.pool, &transfer, &person).await?;
                outcome.facts = self
                    .annotate(
                        transfer,
                        actor,
                        serde_json::json!({
                            "action": "add-party",
                            "party": party,
                            "person": person,
                        }),
                        now,
                    )
                    .await?;
                outcome.created = Some(party);
            }
            Action::AddPromiseToTransfer {
                transfer,
                record,
                delta,
                party,
                window_end,
                condition,
            } => {
                if transfer_phase_locked() {
                    return Err(EngineError::Conflict {
                        code: "transfer_phase_1_not_available",
                        message: "adding a promise requires the revision-safe draft edit workflow"
                            .into(),
                    });
                }
                if !delta.is_finite() || delta == 0.0 {
                    return Err(EngineError::Consequence(
                        "promise delta must be finite and non-zero".into(),
                    ));
                }
                let transfer = self.resolve(&transfer).await?;
                self.require_transfer_editor(&transfer, actor.as_deref())
                    .await?;
                let record = self.resolve(&record).await?;
                let party = self.resolve(&party).await?;
                let party_record = store::records::get(&self.store.pool, &party)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(party.clone()))?;
                if party_record.kind != RecordKind::Person.as_str() {
                    return Err(EngineError::Consequence(
                        "promise parties must be person records".into(),
                    ));
                }
                let window_end = window_end
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty());
                if let Some(value) = window_end.as_deref() {
                    let parsed = DateTime::parse_from_rfc3339(value).map_err(|_| {
                        EngineError::Consequence(format!(
                            "promise window `{value}` must be an RFC3339 date and time"
                        ))
                    })?;
                    if parsed.with_timezone(&Utc) <= now {
                        return Err(EngineError::Consequence(
                            "promise window must end in the future".into(),
                        ));
                    }
                }
                let condition = condition
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty());
                if let Some(value) = condition.as_deref() {
                    nucleus::expr::Expr::parse(value).map_err(|error| {
                        EngineError::Consequence(format!("invalid promise condition: {error}"))
                    })?;
                }
                let promise = store::misc::insert_promise(
                    &self.store.pool,
                    store::misc::NewPromise {
                        record_uid: Some(record.clone()),
                        delta,
                        window_end,
                        party_uid: Some(party.clone()),
                        state: Some(PromiseState::Proposed),
                        condition,
                        transfer_uid: Some(transfer.clone()),
                        ..Default::default()
                    },
                )
                .await?;
                outcome.facts = self
                    .annotate_many(
                        vec![transfer, record],
                        actor,
                        serde_json::json!({
                            "action": "add-promise-to-transfer",
                            "promise": promise,
                            "party": party,
                            "delta": delta,
                        }),
                        now,
                    )
                    .await?;
                outcome.created = Some(promise);
            }
            Action::AgreeTransfer { .. } => {
                return Err(EngineError::Conflict {
                    code: "signed_transfer_agreement_action_required",
                    message: "use set-transfer-agreement-level with expected_revision, request_id, and the acting Person"
                        .into(),
                });
            }
            Action::ActivateTransfer { transfer } => {
                if transfer_phase_locked() {
                    return Err(EngineError::Conflict {
                        code: "transfer_phase_4_not_available",
                        message: "transfer activation is not available before occurrence modeling"
                            .into(),
                    });
                }
                let transfer = self.resolve(&transfer).await?;
                self.require_transfer_editor(&transfer, actor.as_deref())
                    .await?;
                let activated = crate::transfer::activate_promises(&self.store, &transfer).await?;
                outcome.facts = self
                    .annotate(
                        transfer,
                        actor,
                        serde_json::json!({
                            "action": "activate-transfer",
                            "activated_promises": activated,
                        }),
                        now,
                    )
                    .await?;
            }
            Action::SettleTransfer {
                transfer,
                actor: person,
            } => {
                if transfer_phase_locked() {
                    return Err(EngineError::Conflict {
                        code: "transfer_phase_5_not_available",
                        message:
                            "transfer settlement is not available before occurrence confirmation"
                                .into(),
                    });
                }
                let transfer = self.resolve(&transfer).await?;
                let person = self.resolve(&person).await?;
                self.require_transfer_person(&transfer, &person, actor.as_deref())
                    .await?;
                outcome.facts = self.settle_all_local(&transfer, &person, now).await?;
            }
            Action::SetPlace {
                target,
                lat,
                lon,
                address,
            } => {
                let target = self.resolve(&target).await?;
                let place =
                    store::places::create(&self.store.pool, lat, lon, address.as_deref()).await?;
                store::places::set_record_place(&self.store.pool, &target, &place).await?;
                outcome.created = Some(place);
            }
            Action::GrantVisibility {
                subject_kind,
                subject,
                target,
            } => {
                let target = self.resolve(&target).await?;
                outcome.created = Some(
                    store::visibility::grant(
                        &self.store.pool,
                        &subject_kind,
                        subject.as_deref(),
                        &target,
                    )
                    .await?,
                );
            }
            Action::SaveProtein { slug, head, ast } => {
                validate_saved_protein_shape(&ast)?;
                let parsed: protein::Protein =
                    serde_json::from_value(ast.clone()).map_err(|error| {
                        EngineError::Consequence(format!("invalid Protein: {error}"))
                    })?;
                protein::validate(&parsed).map_err(|error| {
                    EngineError::Consequence(format!("invalid Protein: {error}"))
                })?;
                let body = serde_json::to_string_pretty(&ast).unwrap_or_default();
                let uid = match store::records::resolve(&self.store.pool, &slug).await? {
                    Some(existing) => {
                        if existing.kind != RecordKind::Protein.as_str() {
                            return Err(EngineError::Consequence(format!(
                                "slug `{slug}` is a {} record, not a saved protein",
                                existing.kind
                            )));
                        }
                        self.write_record_text(&existing.uid, Some(&head), Some(&body))
                            .await?;
                        if existing.quantity.is_zero() {
                            Box::pin(self.act(
                                Action::SetQuantity {
                                    target: existing.uid.clone(),
                                    value: 1.0,
                                },
                                actor.clone(),
                            ))
                            .await?;
                        }
                        existing.uid
                    }
                    None => {
                        store::records::create(
                            &self.store.pool,
                            store::records::NewRecord {
                                slug: Some(&slug),
                                kind: RecordKind::Protein,
                                head: &head,
                                body: &body,
                                quantity: store::exact::one(),
                            },
                        )
                        .await?
                        .uid
                    }
                };
                store::records::set_extension(&self.store.pool, &uid, "lince.protein", &ast)
                    .await?;
                outcome.created = Some(uid);
            }
            Action::CreateKarmaProgram {
                request_id,
                program,
                owner_person_uid,
            } => {
                let commit = self
                    .create_karma_program(
                        store::karma::programs::CreateProgramInput {
                            request_id,
                            program,
                            owner_person_uid,
                            actor_person_uid: actor,
                        },
                        now,
                    )
                    .await?;
                apply_program_mutation(commit, &mut outcome)?;
                self.publish_karma_definition(&outcome, KarmaKind::Program)
                    .await?;
            }
            Action::ReviseKarmaProgram {
                request_id,
                program_uid,
                expected_handle_revision,
                program,
            } => {
                let commit = self
                    .revise_karma_program(
                        store::karma::programs::ReviseProgramInput {
                            request_id,
                            program_uid,
                            expected_handle_revision,
                            program,
                            actor_person_uid: actor,
                        },
                        now,
                    )
                    .await?;
                apply_program_mutation(commit, &mut outcome)?;
                self.publish_karma_definition(&outcome, KarmaKind::Program)
                    .await?;
            }
            Action::ActivateKarmaProgram {
                request_id,
                program_uid,
                expected_handle_revision,
                revision_hash,
            } => {
                let commit = self
                    .activate_karma_program(
                        store::karma::programs::ActivateProgramInput {
                            request_id,
                            program_uid,
                            expected_handle_revision,
                            revision_hash,
                            actor_person_uid: actor,
                        },
                        now,
                    )
                    .await?;
                apply_program_mutation(commit, &mut outcome)?;
                self.publish_karma_definition(&outcome, KarmaKind::Program)
                    .await?;
            }
            Action::PauseKarmaProgram {
                request_id,
                program_uid,
                expected_handle_revision,
            } => {
                let commit = self
                    .pause_karma_program(
                        store::karma::programs::PauseProgramInput {
                            request_id,
                            program_uid,
                            expected_handle_revision,
                            actor_person_uid: actor,
                        },
                        now,
                    )
                    .await?;
                apply_program_mutation(commit, &mut outcome)?;
                self.publish_karma_definition(&outcome, KarmaKind::Program)
                    .await?;
            }
            Action::SetKarmaExecution {
                program_uid,
                executes,
                note,
            } => {
                store::karma::execution::set_executes(
                    &self.store.pool,
                    &program_uid,
                    executes,
                    note.as_deref(),
                    now,
                )
                .await?;
                outcome.warnings.push(if executes {
                    "This Cell now runs that rule.".into()
                } else {
                    "This Cell now holds that rule without running it. Other Cells are unchanged."
                        .into()
                });
            }
            Action::DesignateKarmaExecutor {
                program_uid,
                cell_uid,
            } => {
                store::executor::designate(&self.store.pool, &program_uid, cell_uid.as_deref())
                    .await?;
                outcome.warnings.push(match cell_uid {
                    Some(_) => "Only that Cell will run this rule. If it is off, the rule does not run until you move it."
                        .into(),
                    None => "Every Cell holding this rule will run it again.".into(),
                });
            }
            Action::DesignateTransferExecutor {
                transfer_uid,
                cell_uid,
            } => {
                let transfer = self.resolve(&transfer_uid).await?;
                self.require_transfer_editor(&transfer, actor.as_deref())
                    .await?;
                store::executor::designate(&self.store.pool, &transfer, cell_uid.as_deref())
                    .await?;
                outcome.warnings.push(match cell_uid {
                    Some(_) => "Only that Cell will deliver this Transfer. If it is off, deliveries wait until you move it."
                        .into(),
                    None => "Every Cell holding this Transfer will deliver it again.".into(),
                });
            }
            Action::RespondKarmaCandidate {
                request_id,
                candidate_hash,
                expected_state_revision,
                response,
                authorizing_grant_uid,
            } => {
                let commit = self
                    .respond_karma_candidate(
                        store::karma::candidates::RespondCandidateInput {
                            request_id,
                            candidate_hash,
                            expected_state_revision,
                            response,
                            actor_person_uid: actor,
                            authorizing_grant_uid,
                        },
                        now,
                    )
                    .await?;
                apply_candidate_review(commit, &mut outcome)?;
            }
            grant_action @ (Action::CreateKarmaGrant { .. }
            | Action::NarrowKarmaGrant { .. }
            | Action::ActivateKarmaGrant { .. }
            | Action::RevokeKarmaGrant { .. }) => {
                Box::pin(self.apply_karma_grant_action(
                    grant_action,
                    actor.as_deref(),
                    now,
                    &mut outcome,
                ))
                .await?;
            }
            Action::CreateKarmaFrequency {
                request_id,
                frequency,
                owner_person_uid,
            } => {
                let commit = self
                    .create_karma_frequency(
                        store::karma::frequencies::CreateFrequencyInput {
                            request_id,
                            frequency,
                            owner_person_uid,
                            actor_person_uid: actor,
                        },
                        now,
                    )
                    .await?;
                apply_frequency_mutation(commit, &mut outcome)?;
                self.publish_karma_definition(&outcome, KarmaKind::Frequency)
                    .await?;
            }
            Action::ReviseKarmaFrequency {
                request_id,
                frequency_uid,
                expected_handle_revision,
                frequency,
            } => {
                let commit = self
                    .revise_karma_frequency(
                        store::karma::frequencies::ReviseFrequencyInput {
                            request_id,
                            frequency_uid,
                            expected_handle_revision,
                            frequency,
                            actor_person_uid: actor,
                        },
                        now,
                    )
                    .await?;
                apply_frequency_mutation(commit, &mut outcome)?;
                self.publish_karma_definition(&outcome, KarmaKind::Frequency)
                    .await?;
            }
            Action::ActivateKarmaFrequency {
                request_id,
                frequency_uid,
                expected_handle_revision,
                revision_hash,
                parameter_overrides,
            } => {
                let runtime = self.configured_karma_runtime()?;
                let commit = self
                    .activate_karma_frequency(
                        store::karma::frequencies::ActivateFrequencyInput {
                            request_id,
                            frequency_uid,
                            expected_handle_revision,
                            revision_hash,
                            parameter_overrides,
                            actor_person_uid: actor,
                        },
                        &runtime,
                        now,
                    )
                    .await?;
                apply_frequency_mutation(commit, &mut outcome)?;
                self.publish_karma_definition(&outcome, KarmaKind::Frequency)
                    .await?;
            }
            Action::SetKarmaFrequencyParameters {
                request_id,
                frequency_uid,
                expected_handle_revision,
                expected_active_revision_hash,
                parameter_overrides,
            } => {
                let runtime = self.configured_karma_runtime()?;
                let commit = self
                    .set_karma_frequency_parameters(
                        store::karma::frequencies::SetFrequencyParametersInput {
                            request_id,
                            frequency_uid,
                            expected_handle_revision,
                            expected_active_revision_hash,
                            parameter_overrides,
                            actor_person_uid: actor,
                        },
                        &runtime,
                        now,
                    )
                    .await?;
                apply_frequency_mutation(commit, &mut outcome)?;
                self.publish_karma_definition(&outcome, KarmaKind::Frequency)
                    .await?;
            }
            Action::ResetKarmaFrequencyParameters {
                request_id,
                frequency_uid,
                expected_handle_revision,
                expected_active_revision_hash,
            } => {
                let runtime = self.configured_karma_runtime()?;
                let commit = self
                    .reset_karma_frequency_parameters(
                        store::karma::frequencies::ResetFrequencyParametersInput {
                            request_id,
                            frequency_uid,
                            expected_handle_revision,
                            expected_active_revision_hash,
                            actor_person_uid: actor,
                        },
                        &runtime,
                        now,
                    )
                    .await?;
                apply_frequency_mutation(commit, &mut outcome)?;
                self.publish_karma_definition(&outcome, KarmaKind::Frequency)
                    .await?;
            }
            Action::PauseKarmaFrequency {
                request_id,
                frequency_uid,
                expected_handle_revision,
            } => {
                let commit = self
                    .pause_karma_frequency(
                        store::karma::frequencies::PauseFrequencyInput {
                            request_id,
                            frequency_uid,
                            expected_handle_revision,
                            actor_person_uid: actor,
                        },
                        now,
                    )
                    .await?;
                apply_frequency_mutation(commit, &mut outcome)?;
                self.publish_karma_definition(&outcome, KarmaKind::Frequency)
                    .await?;
            }
            Action::Decide { decision, answer } => {
                store::misc::answer_decision(&self.store.pool, &decision, &answer).await?;
                let chosen_action = store::misc::list_decisions(&self.store.pool)
                    .await?
                    .into_iter()
                    .find(|d| d.record_uid == decision)
                    .and_then(|d| {
                        d.options.as_array()?.iter().find_map(|option| {
                            (option.get("label")?.as_str()? == answer)
                                .then(|| option.get("action").cloned())
                                .flatten()
                        })
                    });
                let current = store::records::quantity(&self.store.pool, &decision)
                    .await?
                    .unwrap_or_else(store::exact::zero);
                if !current.is_zero() {
                    outcome.facts = self
                        .append(
                            NewFact {
                                uid: None,
                                record_uid: decision,
                                delta: store::exact::negate(current)?,
                                at: None,
                                actor_uid: actor.clone(),
                                cause: Cause {
                                    kind: CauseKind::Action,
                                    uid: None,
                                },
                                payload: Some(serde_json::json!({ "answer": answer }).to_string()),
                            },
                            now,
                        )
                        .await?;
                }
                if let Some(action_json) = chosen_action {
                    let action: Action = serde_json::from_value(action_json)
                        .map_err(|e| EngineError::Consequence(format!("bad option action: {e}")))?;
                    let inner = Box::pin(self.act_at(action, actor, now)).await?;
                    outcome.facts.extend(inner.facts);
                    outcome.warnings.extend(inner.warnings);
                    if outcome.created.is_none() {
                        outcome.created = inner.created;
                    }
                }
            }
            Action::CreateSignal {
                slug,
                head,
                source_kind,
                source,
                schedule,
            } => {
                outcome.created = Some(
                    store::misc::create_signal(
                        &self.store.pool,
                        store::misc::NewSignal {
                            slug: &slug,
                            head: &head,
                            source_kind: &source_kind,
                            source: &source,
                            schedule: &schedule,
                        },
                    )
                    .await?,
                );
            }
            Action::CreateMatchRule {
                slug,
                head,
                watch_concept,
                max_proximity,
                min_confidence,
                auto,
            } => {
                if !matches!(auto.as_str(), "draft_only" | "ask" | "auto_propose") {
                    return Err(EngineError::Consequence(format!(
                        "auto must be draft_only | ask | auto_propose, not `{auto}`"
                    )));
                }
                outcome.created = Some(
                    store::senses::create_sense_rule(
                        &self.store.pool,
                        store::senses::NewSenseRule {
                            slug: &slug,
                            head: &head,
                            watch_concept: watch_concept.as_deref(),
                            max_proximity,
                            min_confidence,
                            auto: &auto,
                        },
                    )
                    .await?,
                );
            }
            Action::AdoptConcepts { concepts } => {
                for seed in &concepts {
                    store::concepts::adopt(
                        &self.store.pool,
                        &seed.uid,
                        &seed.name,
                        seed.origin.as_deref(),
                        &seed.parents,
                    )
                    .await?;
                }
            }
            correction_action @ (Action::CreateTransferRemainderDraft { .. }
            | Action::CreateReversingTransferDraft { .. }) => {
                let (
                    occurrence,
                    expected_revision,
                    expected_remaining_quantity,
                    request_id,
                    person,
                    reversing,
                ) = match correction_action {
                    Action::CreateTransferRemainderDraft {
                        occurrence,
                        expected_revision,
                        expected_remaining_quantity,
                        request_id,
                        person,
                    } => (
                        occurrence,
                        expected_revision,
                        expected_remaining_quantity,
                        request_id,
                        person,
                        false,
                    ),
                    Action::CreateReversingTransferDraft {
                        occurrence,
                        expected_revision,
                        canonical_quantity,
                        request_id,
                        person,
                    } => (
                        occurrence,
                        expected_revision,
                        canonical_quantity,
                        request_id,
                        person,
                        true,
                    ),
                    _ => unreachable!("matched Transfer correction draft action"),
                };
                let kind = if reversing { "reversal" } else { "remainder" };
                let acting = self
                    .transfer_action_person(actor.as_deref(), person.as_deref(), None)
                    .await?;
                self.require_permission(actor.as_deref(), "transfer:update")
                    .await?;
                if let Some(existing) =
                    store::transfers::correction_link_for_request(&self.store.pool, &request_id)
                        .await?
                {
                    if existing.kind != kind
                        || existing.source_occurrence_uid != occurrence
                        || existing.source_revision != expected_revision
                        || existing.actor_person_uid != acting
                        || !transfer_settlement_values_match(
                            existing.canonical_quantity,
                            expected_remaining_quantity,
                        )
                    {
                        return Err(transfer_request_id_conflict());
                    }
                    outcome.created = Some(existing.created_transfer_uid);
                    return Ok(outcome);
                }
                let occurrence_row = store::transfers::occurrence(&self.store.pool, &occurrence)
                    .await?
                    .ok_or_else(|| EngineError::Conflict {
                        code: "transfer_occurrence_missing",
                        message: "correction occurrence no longer exists".into(),
                    })?;
                let source_transfer =
                    store::transfers::get(&self.store.pool, &occurrence_row.transfer_uid)
                        .await?
                        .ok_or_else(|| {
                            EngineError::UnknownRecord(occurrence_row.transfer_uid.clone())
                        })?;
                if source_transfer.revision as u64 != expected_revision {
                    return Err(EngineError::Conflict {
                        code: "transfer_revision_stale",
                        message: format!(
                            "expected transfer revision {expected_revision}, current revision is {}",
                            source_transfer.revision
                        ),
                    });
                }
                let source_record =
                    store::records::get(&self.store.pool, &source_transfer.record_uid)
                        .await?
                        .ok_or_else(|| {
                            EngineError::UnknownRecord(source_transfer.record_uid.clone())
                        })?;
                let source_promise =
                    store::transfers::promises_of(&self.store.pool, &occurrence_row.transfer_uid)
                        .await?
                        .into_iter()
                        .find(|promise| promise.uid == occurrence_row.promise_uid)
                        .ok_or_else(|| EngineError::Conflict {
                            code: "transfer_promise_missing",
                            message: "correction source promise no longer exists".into(),
                        })?;
                if !reversing && source_promise.party_uid.as_deref() != Some(acting.as_str()) {
                    return Err(EngineError::Forbidden(
                        "only the source-promise owner may create its correction draft".into(),
                    ));
                }
                let progress =
                    store::transfers::occurrence_settlement_progress(&self.store.pool, &occurrence)
                        .await?
                        .ok_or_else(|| EngineError::Conflict {
                            code: "transfer_occurrence_missing",
                            message: "correction occurrence no longer exists".into(),
                        })?;
                let quantity = if reversing {
                    if self
                        .transfer_creator_person(&occurrence_row.transfer_uid)
                        .await?
                        != acting
                    {
                        return Err(EngineError::Forbidden(
                            "only the original Transfer creator may propose a reversal".into(),
                        ));
                    }
                    if !expected_remaining_quantity.is_finite()
                        || expected_remaining_quantity <= 0.0
                        || !transfer_settlement_values_match(
                            expected_remaining_quantity,
                            occurrence_row.quantity,
                        )
                    {
                        return Err(EngineError::Conflict {
                            code: "transfer_reversal_quantity_stale",
                            message: "reversal must use the exact canonical occurrence quantity"
                                .into(),
                        });
                    }
                    expected_remaining_quantity
                } else {
                    if store::transfers::effective_occurrence_remainder_policy(
                        &self.store.pool,
                        &occurrence,
                        &acting,
                    )
                    .await?
                        != nucleus::transfer::TransferRemainderPolicy::LocalDraft
                    {
                        return Err(EngineError::Conflict {
                            code: "transfer_remainder_policy_changed",
                            message: "occurrence remainder policy is not local_draft".into(),
                        });
                    }
                    if progress.settled_quantity <= 0.0
                        || progress.remaining_quantity <= 0.0
                        || !transfer_settlement_values_match(
                            expected_remaining_quantity,
                            progress.remaining_quantity,
                        )
                    {
                        return Err(EngineError::Conflict {
                            code: "transfer_remainder_stale",
                            message: "reviewed remainder is no longer the exact partial remainder"
                                .into(),
                        });
                    }
                    progress.remaining_quantity
                };
                let delta = if reversing {
                    if occurrence_row.giver_person_uid == acting {
                        quantity
                    } else if occurrence_row.receiver_person_uid == acting {
                        -quantity
                    } else {
                        return Err(EngineError::Forbidden(
                            "reversal creator must be an occurrence participant".into(),
                        ));
                    }
                } else {
                    source_promise.delta.signum() * quantity
                };
                let (correction_record_uid, correction_concept_uid) = if !reversing
                    || source_promise.party_uid.as_deref() == Some(acting.as_str())
                {
                    (
                        occurrence_row.record_uid.clone(),
                        occurrence_row.concept_uid.clone(),
                    )
                } else if occurrence_row.concept_uid.is_some() {
                    (None, occurrence_row.concept_uid.clone())
                } else {
                    return Err(EngineError::Conflict {
                        code: "transfer_reversal_private_record_without_concept",
                        message: "reversal needs a canonical concept when the source Record belongs to another participant"
                            .into(),
                    });
                };
                let organ_uid = store::organs::local(&self.store.pool)
                    .await?
                    .map(|organ| organ.uid);
                let signer = self
                    .transfer_person_signer(&acting, verified_authorship.as_ref())
                    .await?;
                let created = store::transfers::create_draft(
                    &self.store.pool,
                    store::transfers::NewTransferDraft {
                        idempotency_key: request_id,
                        slug: None,
                        head: format!(
                            "{}: {}",
                            if reversing { "Reversal" } else { "Remainder" },
                            source_record.head
                        ),
                        agreement_type: nucleus::transfer::AgreementType::Full.as_str().into(),
                        agreement_pct: None,
                        satiation: None,
                        parent_uid: None,
                        source_uid: None,
                        visibility: "hidden".into(),
                        max_proximity: None,
                        reserve_default: "none".into(),
                        require_confirmation: source_transfer.require_confirmation,
                        default_place: occurrence_row.location.clone(),
                        creator_person: acting.clone(),
                        invitees: Vec::new(),
                        promises: vec![store::transfers::DraftPromise {
                            uid: None,
                            record_uid: correction_record_uid,
                            concept_uid: correction_concept_uid,
                            unit_uid: occurrence_row.unit_uid.clone(),
                            person_uid: Some(acting.clone()),
                            open: false,
                            delta,
                            window_start: None,
                            window_end: None,
                            location: occurrence_row.location.clone(),
                            condition: None,
                            reserve_from: "none".into(),
                            open_reuse_policy: nucleus::transfer::OpenPromiseReusePolicy::Duplicate,
                        }],
                        dependencies: Vec::new(),
                        organ_uid,
                        evidence_action: format!("create-transfer-{kind}-draft"),
                        correction: Some(store::transfers::NewTransferCorrectionLink {
                            kind: kind.into(),
                            source_transfer_uid: occurrence_row.transfer_uid,
                            source_occurrence_uid: occurrence,
                            source_revision: expected_revision,
                            canonical_quantity: quantity,
                        }),
                        authorization_intent_uid: verified_authorship
                            .as_ref()
                            .map(|value| value.intent_uid.clone()),
                    },
                    now,
                    Some(acting),
                    actor.clone(),
                    |hash| signer.as_ref().map(|value| value.sign_hash(hash)),
                )
                .await?;
                outcome.created = Some(created.transfer_uid);
                if !created.replayed {
                    outcome.facts = self.publish_committed_fact(created.fact);
                    for fact in created.invitation_event_facts {
                        outcome.facts.extend(self.publish_committed_fact(fact));
                    }
                }
            }
            Action::ReopenTransferPromise {
                transfer,
                promise,
                expected_revision,
                request_id,
                person,
                window_end,
                open,
            } => {
                let transfer = self.resolve(&transfer).await?;
                let acting = self
                    .transfer_action_person(actor.as_deref(), person.as_deref(), None)
                    .await?;
                self.require_permission(actor.as_deref(), "transfer:update")
                    .await?;
                if let Some(existing) =
                    store::transfers::promise_successor_for_request(&self.store.pool, &request_id)
                        .await?
                {
                    let requested_window_end = window_end
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(|value| DateTime::parse_from_rfc3339(value).map(|_| value.to_string()))
                        .transpose()
                        .map_err(|_| EngineError::Conflict {
                            code: "transfer_window_invalid",
                            message: "reopened promise window end is not RFC 3339".into(),
                        })?;
                    let existing_successor =
                        store::transfers::promises_of(&self.store.pool, &transfer)
                            .await?
                            .into_iter()
                            .find(|candidate| candidate.uid == existing.successor_promise_uid)
                            .ok_or_else(|| EngineError::Conflict {
                                code: "transfer_promise_successor_missing",
                                message: "reopened promise successor no longer exists".into(),
                            })?;
                    if existing.transfer_uid != transfer
                        || existing.predecessor_promise_uid != promise
                        || existing.actor_person_uid != acting
                        || existing.revision != expected_revision.saturating_add(1)
                        || (existing_successor.state == PromiseState::Open) != open
                        || existing_successor.window_end != requested_window_end
                    {
                        return Err(transfer_request_id_conflict());
                    }
                    outcome.created = Some(existing.successor_promise_uid);
                    return Ok(outcome);
                }
                let transfer_row = store::transfers::get(&self.store.pool, &transfer)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(transfer.clone()))?;
                if transfer_row.revision as u64 != expected_revision {
                    return Err(EngineError::Conflict {
                        code: "transfer_revision_stale",
                        message: format!(
                            "expected transfer revision {expected_revision}, current revision is {}",
                            transfer_row.revision
                        ),
                    });
                }
                let transfer_record = store::records::get(&self.store.pool, &transfer)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(transfer.clone()))?;
                let current_promises =
                    store::transfers::promises_of(&self.store.pool, &transfer).await?;
                let predecessor = current_promises
                    .iter()
                    .find(|candidate| candidate.uid == promise)
                    .ok_or_else(|| EngineError::Conflict {
                        code: "transfer_promise_missing",
                        message: "predecessor promise no longer exists".into(),
                    })?;
                let predecessor_expired =
                    matches!(
                        predecessor.state,
                        PromiseState::Proposed | PromiseState::Agreed
                    ) && predecessor.window_end.as_deref().is_some_and(|window_end| {
                        DateTime::parse_from_rfc3339(window_end)
                            .is_ok_and(|window_end| window_end.with_timezone(&Utc) <= now)
                    });
                if !matches!(
                    predecessor.state,
                    PromiseState::Broken | PromiseState::Withdrawn
                ) && !predecessor_expired
                {
                    return Err(EngineError::Conflict {
                        code: "transfer_promise_not_reopenable",
                        message:
                            "only a broken, withdrawn, or expired proposed promise may be reopened"
                                .into(),
                    });
                }
                if predecessor.party_uid.as_deref() != Some(acting.as_str())
                    || store::transfers::party_for_actor(&self.store.pool, &transfer, &acting)
                        .await?
                        .is_none()
                {
                    return Err(EngineError::Forbidden(
                        "only the predecessor promise owner may reopen it".into(),
                    ));
                }
                let remaining =
                    match store::transfers::occurrence_for_promise(&self.store.pool, &promise)
                        .await?
                    {
                        Some(occurrence) => store::transfers::occurrence_settlement_progress(
                            &self.store.pool,
                            &occurrence.uid,
                        )
                        .await?
                        .map(|progress| progress.remaining_quantity)
                        .unwrap_or(occurrence.quantity),
                        None => predecessor.delta.abs(),
                    };
                if !remaining.is_finite() || remaining <= 0.0 {
                    return Err(EngineError::Conflict {
                        code: "transfer_promise_fully_settled",
                        message: "a fully settled promise has no remainder to reopen".into(),
                    });
                }
                let (_, successor_window_end) =
                    normalize_transfer_window(None, window_end, now, None)?;
                let current_fact =
                    store::transfers::revision_fact(&self.store.pool, &transfer, expected_revision)
                        .await?
                        .ok_or_else(|| EngineError::Conflict {
                            code: "transfer_revision_evidence_missing",
                            message: "current signed Transfer revision is unavailable".into(),
                        })?;
                let current_evidence: nucleus::transfer::TransferRevisionEvidence =
                    serde_json::from_str(current_fact.payload.as_deref().unwrap_or("")).map_err(
                        |_| EngineError::Conflict {
                            code: "transfer_revision_evidence_invalid",
                            message: "current signed Transfer revision evidence is invalid".into(),
                        },
                    )?;
                let signed_by_uid = current_evidence
                    .terms
                    .promises
                    .iter()
                    .map(|promise| (promise.uid.as_str(), promise))
                    .collect::<std::collections::HashMap<_, _>>();
                let mut promises = current_promises
                    .iter()
                    .filter(|promise| {
                        promise.uid != predecessor.uid
                            && matches!(
                                promise.state,
                                PromiseState::Open | PromiseState::Proposed | PromiseState::Agreed
                            )
                    })
                    .map(|promise| {
                        let signed = signed_by_uid.get(promise.uid.as_str()).copied();
                        store::transfers::DraftPromiseRevisionInput {
                            uid: Some(promise.uid.clone()),
                            source_promise_uid: signed
                                .and_then(|promise| promise.source_promise_uid.clone()),
                            record_uid: promise.record_uid.clone(),
                            concept_uid: promise.concept_uid.clone(),
                            unit_uid: promise.unit_uid.clone(),
                            person_uid: promise.party_uid.clone(),
                            open: promise.state == PromiseState::Open,
                            delta: promise.delta,
                            window_start: promise.window_start.clone(),
                            window_end: promise.window_end.clone(),
                            location: promise.location.clone(),
                            condition: promise.condition.clone(),
                            reserve_from: promise.reserve_from.clone(),
                            open_reuse_policy: promise.open_reuse_policy,
                        }
                    })
                    .collect::<Vec<_>>();
                let successor_uid = nucleus::new_uid("p");
                promises.push(store::transfers::DraftPromiseRevisionInput {
                    uid: Some(successor_uid.clone()),
                    source_promise_uid: Some(promise.clone()),
                    record_uid: predecessor.record_uid.clone(),
                    concept_uid: predecessor.concept_uid.clone(),
                    unit_uid: predecessor.unit_uid.clone(),
                    person_uid: Some(acting.clone()),
                    open,
                    delta: predecessor.delta.signum() * remaining,
                    window_start: None,
                    window_end: successor_window_end,
                    location: predecessor.location.clone(),
                    condition: predecessor.condition.clone(),
                    reserve_from: predecessor.reserve_from.clone(),
                    open_reuse_policy: predecessor.open_reuse_policy,
                });
                let dependencies = store::transfers::dependencies_of(&self.store.pool, &transfer)
                    .await?
                    .into_iter()
                    .map(|dependency| store::transfers::TransferDependencyInput {
                        uid: Some(dependency.uid),
                        scope: dependency.scope,
                        promise_uid: dependency.promise_uid.map(|scoped| {
                            if scoped == promise {
                                successor_uid.clone()
                            } else {
                                scoped
                            }
                        }),
                        upstream_kind: dependency.upstream_kind,
                        upstream_uid: dependency.upstream_uid,
                        required_state: dependency.required_state,
                    })
                    .collect();
                let retained_invitation_uids =
                    store::transfers::invitations_for_transfer(&self.store.pool, &transfer)
                        .await?
                        .into_iter()
                        .filter(|invitation| {
                            invitation.status == store::transfers::TransferInvitationStatus::Pending
                        })
                        .map(|invitation| invitation.uid)
                        .collect();
                let signer = self
                    .transfer_person_signer(&acting, verified_authorship.as_ref())
                    .await?;
                let commit = store::transfers::reopen_promise_revision(
                    &self.store.pool,
                    store::transfers::WholeDraftRevisionInput {
                        transfer_uid: transfer.clone(),
                        expected_revision,
                        idempotency_key: request_id,
                        creator_person_uid: acting.clone(),
                        proposal_author_person_uid: acting.clone(),
                        terms: store::transfers::TransferDraftTermsInput {
                            slug: transfer_record.slug,
                            head: transfer_record.head,
                            agreement_type: transfer_row.agreement_type,
                            agreement_pct: transfer_row.agreement_pct,
                            settlement: transfer_row.settlement,
                            visibility: transfer_row.visibility,
                            max_proximity: transfer_row.max_proximity,
                            satiation: transfer_row.satiation,
                            parent_uid: transfer_row.parent_uid,
                            source_uid: transfer_row.source_uid,
                            reserve_default: transfer_row
                                .reserve_default
                                .unwrap_or_else(|| "none".into()),
                            require_confirmation: transfer_row.require_confirmation,
                            default_place: transfer_row.default_place,
                        },
                        retained_invitation_uids,
                        promises,
                        dependencies,
                        successor: Some(store::transfers::PromiseSuccessorInput {
                            predecessor_promise_uid: promise,
                            successor_promise_uid: successor_uid.clone(),
                        }),
                        authorization_intent_uid: verified_authorship
                            .as_ref()
                            .map(|value| value.intent_uid.clone()),
                    },
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
                outcome.created = Some(successor_uid);
            }
            Action::CompensateTransferOccurrenceSettlement {
                settlement,
                request_id,
                person,
            } => {
                let acting = self
                    .transfer_action_person(actor.as_deref(), person.as_deref(), None)
                    .await?;
                self.require_permission(actor.as_deref(), "transfer:update")
                    .await?;
                if let Some(replayed) =
                    store::transfers::occurrence_settlement_compensation_for_request(
                        &self.store.pool,
                        &request_id,
                    )
                    .await?
                {
                    if replayed.correction.settlement_uid != settlement
                        || replayed.correction.owner_person_uid != acting
                    {
                        return Err(transfer_request_id_conflict());
                    }
                    outcome.created = Some(replayed.correction.uid);
                    return Ok(outcome);
                }
                if store::transfers::phase5_correction_request_for_request(
                    &self.store.pool,
                    &request_id,
                )
                .await?
                .is_some()
                {
                    return Err(transfer_request_id_conflict());
                }
                let slice =
                    store::transfers::occurrence_settlement_slice(&self.store.pool, &settlement)
                        .await?
                        .ok_or_else(|| EngineError::Conflict {
                            code: "transfer_settlement_missing",
                            message: "the settlement slice does not exist".into(),
                        })?;
                if slice.owner_person_uid != acting {
                    return Err(EngineError::Conflict {
                        code: "transfer_settlement_compensation_not_owner",
                        message:
                            "only the settlement owner may reverse its private Record application"
                                .into(),
                    });
                }
                if store::transfers::occurrence_settlement_compensation_for_settlement(
                    &self.store.pool,
                    &settlement,
                )
                .await?
                .is_some()
                {
                    return Err(EngineError::Conflict {
                        code: "transfer_settlement_already_compensated",
                        message:
                            "this settlement's private Record application was already compensated"
                                .into(),
                    });
                }
                let signer = self
                    .transfer_person_signer(&acting, verified_authorship.as_ref())
                    .await?;
                let commit = store::transfers::compensate_occurrence_settlement(
                    &self.store.pool,
                    store::transfers::OccurrenceSettlementCompensationInput {
                        settlement_uid: settlement,
                        idempotency_key: request_id,
                        actor_person_uid: acting,
                        authorization_intent_uid: verified_authorship
                            .as_ref()
                            .map(|value| value.intent_uid.clone()),
                    },
                    now,
                    |hash| signer.as_ref().map(|value| value.sign_hash(hash)),
                )
                .await?;
                match commit {
                    store::transfers::OccurrenceSettlementCompensationCommit::Committed(
                        committed,
                    ) => {
                        outcome.created = Some(committed.correction.uid);
                        outcome.facts = self.publish_committed_fact(committed.fact);
                    }
                    store::transfers::OccurrenceSettlementCompensationCommit::Replayed(
                        replayed,
                    ) => {
                        outcome.created = Some(replayed.correction.uid);
                    }
                }
            }
            Action::DeclareEquivalence { a, b } => {
                let a = store::concepts::resolve(&self.store.pool, &a)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(a))?;
                let b = store::concepts::resolve(&self.store.pool, &b)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(b))?;
                store::concepts::declare_equivalence(&self.store.pool, &a, &b, actor.as_deref())
                    .await?;
            }
            Action::CreateRole { name } => {
                self.require_permission(actor.as_deref(), "role:create")
                    .await?;
                let role_id = store::auth::ensure_role(&self.store.pool, &name).await?;
                outcome.created = Some(role_id.to_string());
            }
            Action::CreateUser {
                username,
                name,
                password,
                role,
            } => {
                self.require_permission(actor.as_deref(), "user:create")
                    .await?;
                let role_id = store::auth::role_by_name(&self.store.pool, &role)
                    .await?
                    .ok_or_else(|| EngineError::Consequence(format!("unknown role `{role}`")))?;
                let password_hash =
                    utils::auth::hash_password(&password).map_err(EngineError::Io)?;
                let person_uid = store::auth::create_person_login(
                    &self.store.pool,
                    &name,
                    &username,
                    &password_hash,
                    role_id,
                )
                .await?;
                outcome.facts = self
                    .append(
                        NewFact {
                            actor_uid: actor,
                            ..NewFact::quantity_f64(person_uid.clone(), 0.0, Cause::user_edit())
                        },
                        now,
                    )
                    .await?;
                outcome.created = Some(person_uid);
            }
            Action::AssignRole { user, role } => {
                self.require_permission(actor.as_deref(), "user:assign_role")
                    .await?;
                let person = self.resolve(&user).await?;
                let role_id = store::auth::role_by_name(&self.store.pool, &role)
                    .await?
                    .ok_or_else(|| EngineError::Consequence(format!("unknown role `{role}`")))?;
                if !store::auth::has_credential(&self.store.pool, &person).await? {
                    return Err(EngineError::Consequence(format!(
                        "`{user}` has no login here, so there is no role to set"
                    )));
                }
                store::auth::set_user_role(&self.store.pool, &person, role_id).await?;
            }
            Action::SetPersonReadFilter { person, filter } => {
                self.require_permission(actor.as_deref(), "user:update")
                    .await?;
                let person_uid = self.resolve(&person).await?;
                let record = store::records::get(&self.store.pool, &person_uid)
                    .await?
                    .ok_or_else(|| {
                        EngineError::Consequence(format!("no such Person `{person}`"))
                    })?;
                if record.kind != "person" {
                    return Err(EngineError::Consequence(format!(
                        "`{person}` is a {} record, not a Person",
                        record.kind
                    )));
                }
                self.set_read_filter(&person_uid, filter.as_ref()).await?;
            }
            Action::SetPersonStanding {
                person,
                active,
                note,
            } => {
                self.require_permission(actor.as_deref(), "user:update")
                    .await?;
                let person_uid = self.resolve(&person).await?;
                let record = store::records::get(&self.store.pool, &person_uid)
                    .await?
                    .ok_or_else(|| {
                        EngineError::Consequence(format!("no such Person `{person}`"))
                    })?;
                if record.kind != "person" {
                    return Err(EngineError::Consequence(format!(
                        "`{person}` is a {} record, not a Person",
                        record.kind
                    )));
                }
                if actor.as_deref() == Some(person_uid.as_str()) && !active {
                    return Err(EngineError::Consequence(
                        "you cannot deactivate yourself — ask another admin".into(),
                    ));
                }
                if !active {
                    let admins = store::auth::admins(&self.store.pool).await?;
                    if admins.iter().any(|admin| admin == &person_uid) {
                        let mut others = 0;
                        for admin in admins.iter().filter(|admin| *admin != &person_uid) {
                            if store::people::is_active(&self.store.pool, admin).await? {
                                others += 1;
                            }
                        }
                        if others == 0 {
                            return Err(EngineError::Consequence(
                                "this is the last active admin — make someone else an admin first"
                                    .into(),
                            ));
                        }
                    }
                }
                if active {
                    store::people::reactivate(&self.store.pool, &person_uid).await?;
                } else {
                    store::people::deactivate(
                        &self.store.pool,
                        &person_uid,
                        &now.to_rfc3339(),
                        note.as_deref(),
                    )
                    .await?;
                }
                outcome.created = Some(person_uid);
            }
            Action::GrantPermission { role, permission } => {
                self.require_permission(actor.as_deref(), "permission:assign")
                    .await?;
                let (subject, action_name) = permission.split_once(':').ok_or_else(|| {
                    EngineError::Consequence(format!("bad permission `{permission}`"))
                })?;
                let role_id = store::auth::role_by_name(&self.store.pool, &role)
                    .await?
                    .ok_or_else(|| EngineError::Consequence(format!("unknown role `{role}`")))?;
                let permission_id =
                    store::auth::ensure_permission(&self.store.pool, subject, action_name).await?;
                store::auth::grant(&self.store.pool, role_id, permission_id).await?;
            }
            Action::RevokePermission { role, permission } => {
                self.require_permission(actor.as_deref(), "permission:assign")
                    .await?;
                let (subject, action_name) = permission.split_once(':').ok_or_else(|| {
                    EngineError::Consequence(format!("bad permission `{permission}`"))
                })?;
                let role_id = store::auth::role_by_name(&self.store.pool, &role)
                    .await?
                    .ok_or_else(|| EngineError::Consequence(format!("unknown role `{role}`")))?;
                let permission_id =
                    store::auth::ensure_permission(&self.store.pool, subject, action_name).await?;
                store::auth::revoke(&self.store.pool, role_id, permission_id).await?;
            }
        }
        Ok(outcome)
    }

    pub(crate) async fn resolve(&self, token: &str) -> Result<String, EngineError> {
        let token = token.trim_start_matches('@');
        store::records::resolve(&self.store.pool, token)
            .await?
            .map(|r| r.uid)
            .ok_or_else(|| EngineError::UnknownRecord(token.to_string()))
    }

    async fn actor_user(&self, actor: &str) -> Result<store::auth::AuthUser, EngineError> {
        store::auth::user_by_uid(&self.store.pool, actor)
            .await?
            .ok_or_else(|| EngineError::Forbidden("unrecognized actor".into()))
    }

    pub(crate) async fn require_permission(
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

    async fn require_permission_lenient(
        &self,
        actor: Option<&str>,
        permission: &str,
    ) -> Result<(), EngineError> {
        let Some(actor) = actor else {
            return Ok(());
        };
        let Some(user) = store::auth::user_by_uid(&self.store.pool, actor).await? else {
            return Ok(());
        };
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
            | Action::ActivateTransferOccurrence { transfer, .. }
            | Action::ConfigureTransferDelivery { transfer, .. }
            | Action::SetTransferDeliveryMode { transfer, .. }
            | Action::EnqueueTransferDelivery { transfer, .. }
            | Action::RetryTransferDelivery { transfer, .. }
            | Action::RevokeTransferDelivery { transfer, .. }
            | Action::CreateTransferThread { transfer, .. }
            | Action::CreateTransferMessage { transfer, .. }
            | Action::BeginRemoteTransferSettlement { transfer, .. }
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

    async fn transfer_action_person(
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

    async fn require_verified_transfer_revision(
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

    async fn transfer_person_signer(
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

    async fn resolve_transfer_dependencies(
        &self,
        transfer_uid: Option<&str>,
        inputs: Vec<TransferDependencyInput>,
        promise_uids: &HashSet<String>,
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
                    let uid = self.resolve(upstream_token).await?;
                    if store::transfers::get(&self.store.pool, &uid)
                        .await?
                        .is_none()
                    {
                        return Err(EngineError::Consequence(
                            "a Transfer dependency upstream must be a Transfer record".into(),
                        ));
                    }
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
        draft: TransferDraftRevisionInput,
        creator_person_uid: String,
        proposal_author_person_uid: String,
        now: DateTime<Utc>,
        preserve_invitation_lifecycle: bool,
    ) -> Result<store::transfers::WholeDraftRevisionInput, EngineError> {
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
                let uid = self.resolve(token).await?;
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
            Some(token) => Some(self.resolve(token).await?),
            None => None,
        };
        if matches!(draft.satiation, TransferSatiation::FirstCompletes) && source_uid.is_none() {
            return Err(EngineError::Consequence(
                "first_completes requires a shared source record".into(),
            ));
        }

        let invitations =
            store::transfers::invitations_for_transfer(&self.store.pool, &transfer_uid).await?;
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

        let transfer = store::transfers::get(&self.store.pool, &transfer_uid)
            .await?
            .ok_or_else(|| EngineError::UnknownRecord(transfer_uid.clone()))?;
        let effective_reserve_default = draft
            .reserve_default
            .resolve(self.transfer_reservation_cell_default().await?);
        let existing_promises =
            store::transfers::promises_of(&self.store.pool, &transfer_uid).await?;
        let mut promises = Vec::with_capacity(draft.promises.len());
        for input in draft.promises {
            if input.withdrawn {
                if input.uid.is_none() {
                    return Err(EngineError::Consequence(
                        "a new promise cannot already be withdrawn".into(),
                    ));
                }
                continue;
            }
            if !input.delta.is_finite() || input.delta == 0.0 {
                return Err(EngineError::Consequence(
                    "every promise delta must be finite and non-zero".into(),
                ));
            }
            let resolved_record_uid = self.resolve(input.record.trim()).await?;
            let concept_uid = store::records::get(&self.store.pool, &resolved_record_uid)
                .await?
                .ok_or_else(|| EngineError::UnknownRecord(resolved_record_uid.clone()))?
                .identity_predicate_uid;
            let record_uid = Some(resolved_record_uid);
            let existing_promise = input
                .uid
                .as_deref()
                .and_then(|uid| existing_promises.iter().find(|promise| promise.uid == uid));
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
        let dependencies = self
            .resolve_transfer_dependencies(Some(&transfer_uid), dependency_inputs, &promise_uids)
            .await?;
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

    async fn apply_transfer_revision_commit(
        &self,
        commit: store::transfers::RevisionCommit,
        expected_revision: u64,
        transfer_uid: &str,
        outcome: &mut ActionOutcome,
    ) -> Result<(), EngineError> {
        match commit {
            store::transfers::RevisionCommit::Committed { fact, .. } => {
                outcome.facts = self.publish_committed_fact(fact);
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

    async fn reject_direct_transfer_record_mutation(
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
            Action::CreateRecord { .. }
            | Action::CreateAgent { .. }
            | Action::CreateMessageDraft { .. }
            | Action::SendMessageDraft { .. }
            | Action::ImportInstinct => "record:create",
            Action::SetQuantity { .. }
            | Action::SetQuantityExact { .. }
            | Action::TransitionRecord { .. }
            | Action::AddQuantity { .. }
            | Action::CaptureEntry { .. }
            | Action::ReviseEntry { .. }
            | Action::VoidEntry { .. }
            | Action::ClassifyFact { .. }
            | Action::Activate { .. }
            | Action::Deactivate { .. }
            | Action::EditRecordText { .. }
            | Action::SetSlug { .. }
            | Action::SetUnit { .. }
            | Action::SetExtension { .. }
            | Action::ReviseMessage { .. }
            | Action::ReviseMessageDraft { .. }
            | Action::AssertRecord { .. }
            | Action::RetractAssertion { .. }
            | Action::RefineAssertion { .. }
            | Action::RetractRecord { .. }
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

            Action::CreateFrequency { .. } | Action::CreateRecurrence { .. } => "frequency:create",
            Action::ReviseRecurrence { .. }
            | Action::SetRecurrencePaused { .. }
            | Action::ApplyRecurrenceOccurrence { .. }
            | Action::SkipRecurrenceOccurrence { .. }
            | Action::UnskipRecurrenceOccurrence { .. } => "frequency:update",
            Action::DeleteFrequency { .. } | Action::DeleteRecurrence { .. } => "frequency:delete",

            Action::AuditContact { .. }
            | Action::RosterStatus
            | Action::MailboxStatus
            | Action::MailboxPickupPoints
            | Action::MailboxOutbound
            | Action::MailboxRequests
            | Action::FileSyncStatus { .. } => "organ:read",

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
            | Action::RosterEnrolToken
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
            | Action::BeginRemoteTransferSettlement { .. }
            | Action::ApplyRemoteTransferApplication { .. }
            | Action::ConfirmTransfer { .. }
            | Action::AddParty { .. }
            | Action::AddPromiseToTransfer { .. }
            | Action::AgreeTransfer { .. }
            | Action::ActivateTransfer { .. }
            | Action::SettleTransfer { .. }
            | Action::Compensate { .. } => "transfer:update",

            Action::CreateKarmaProgram { .. } | Action::CreateKarmaGrant { .. } => "karma:create",
            Action::ReviseKarmaProgram { .. }
            | Action::ActivateKarmaProgram { .. }
            | Action::PauseKarmaProgram { .. }
            | Action::SetKarmaExecution { .. }
            | Action::DesignateKarmaExecutor { .. }
            | Action::RespondKarmaCandidate { .. }
            | Action::NarrowKarmaGrant { .. }
            | Action::ActivateKarmaGrant { .. }
            | Action::CreateKarmaFrequency { .. }
            | Action::ReviseKarmaFrequency { .. }
            | Action::ActivateKarmaFrequency { .. }
            | Action::SetKarmaFrequencyParameters { .. }
            | Action::ResetKarmaFrequencyParameters { .. }
            | Action::PauseKarmaFrequency { .. } => "karma:update",
            Action::RevokeKarmaGrant { .. } => "karma:delete",

            Action::DeleteRecord { .. }
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
            | Action::CreateUser { .. }
            | Action::AssignRole { .. }
            | Action::SetPersonStanding { .. }
            | Action::SetPersonReadFilter { .. }
            | Action::GrantPermission { .. }
            | Action::RevokePermission { .. } => return None,
        })
    }

    async fn canonical_condition(
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

    async fn resolve_consequences(
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
                other => other,
            });
        }
        nucleus::karma::Consequences::new(resolved).map_err(|error| EngineError::Conflict {
            code: "recurrence_consequences_invalid",
            message: error.to_string(),
        })
    }

    async fn evaluate_rule_condition(
        &self,
        rule: &store::recurrence::Recurrence,
        condition: &store::recurrence::RuleCondition,
        at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<Option<nucleus::DecimalValue>, EngineError> {
        self.ask_condition(rule, condition, at, now, 0).await
    }

    async fn ask_condition(
        &self,
        rule: &store::recurrence::Recurrence,
        condition: &store::recurrence::RuleCondition,
        at: DateTime<Utc>,
        now: DateTime<Utc>,
        depth: usize,
    ) -> Result<Option<nucleus::DecimalValue>, EngineError> {
        let parsed = nucleus::karma::Condition::parse(&condition.source).map_err(|e| {
            EngineError::Conflict {
                code: "rule_condition_invalid",
                message: e.to_string(),
            }
        })?;

        let since = self.rule_reading_since(rule, at)?;

        let mut values = std::collections::HashMap::new();
        for token in parsed.reads() {
            let value = self
                .read_for_condition(
                    &token.func,
                    &token.slug,
                    token.dur_secs,
                    since,
                    at,
                    now,
                    depth,
                )
                .await?;
            values.insert(reading_key(&token.func, &token.slug, token.dur_secs), value);
        }

        let mut readings = GatheredReadings { values };
        nucleus::karma::decide(&parsed, &condition.gate, &condition.carry, &mut readings).map_err(
            |e| EngineError::Conflict {
                code: "rule_condition_unreadable",
                message: e.to_string(),
            },
        )
    }

    async fn read_for_condition(
        &self,
        func: &str,
        slug: &str,
        window_secs: Option<i64>,
        since: DateTime<Utc>,
        at: DateTime<Utc>,
        now: DateTime<Utc>,
        depth: usize,
    ) -> Result<nucleus::DecimalValue, EngineError> {
        let zero =
            nucleus::DecimalValue::from_mantissa(0, 0).expect("scale zero is always constructible");
        match func {
            "quantity" | "signal" => {
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
                if let Some(frequency) = store::frequency::resolve(&self.store.pool, slug).await? {
                    let anchor = frequency.anchor()?;
                    let tick = chrono::Duration::milliseconds(1);
                    let (Some(from), Some(to)) =
                        (since.checked_add_signed(tick), at.checked_add_signed(tick))
                    else {
                        return Ok(zero);
                    };
                    let beats = frequency
                        .cadence()
                        .between(anchor, from, to)
                        .map_err(|error| EngineError::Conflict {
                            code: "frequency_cadence_invalid",
                            message: error.to_string(),
                        })?;
                    return nucleus::DecimalValue::from_mantissa(0, beats.len() as i128).map_err(
                        |_| EngineError::Conflict {
                            code: "rule_condition_unreadable",
                            message: format!("freq(@{slug}) counted more beats than fit"),
                        },
                    );
                }
                let uid = self.resolve(slug).await?;
                self.rhythm_count(&uid, since, at).await
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

    async fn commit_outward_consequence(
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
        payload: serde_json::Value,
    ) -> Result<(), EngineError> {
        store::misc::queue_effect(&self.store.pool, kind, &payload, Some(&rule.uid)).await?;
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
            gate: nucleus::karma::Gate::Always,
            carry: nucleus::karma::Carry::Value,
        };
        self.ask_condition(&rule, &asked, at, now, depth + 1)
            .await?
            .ok_or_else(|| EngineError::Conflict {
                code: "rule_condition_unreadable",
                message: "an always-gate cannot block".into(),
            })
    }

    fn rule_reading_since(
        &self,
        rule: &store::recurrence::Recurrence,
        at: DateTime<Utc>,
    ) -> Result<DateTime<Utc>, EngineError> {
        let anchor = parse_instant_field(&rule.anchor_at)?;
        let previous =
            rule.cadence
                .preceding(anchor, at)
                .map_err(|error| EngineError::Conflict {
                    code: "recurrence_cadence_invalid",
                    message: error.to_string(),
                })?;
        Ok(previous.unwrap_or(anchor))
    }

    async fn rhythm_count(
        &self,
        record_uid: &str,
        since: DateTime<Utc>,
        at: DateTime<Utc>,
    ) -> Result<nucleus::DecimalValue, EngineError> {
        let tick = chrono::Duration::milliseconds(1);
        let mut total: i128 = 0;
        for rule in store::recurrence::for_record(&self.store.pool, record_uid).await? {
            if rule.is_paused() {
                continue;
            }
            let anchor = parse_instant_field(&rule.anchor_at)?;
            let (Some(from), Some(to)) =
                (since.checked_add_signed(tick), at.checked_add_signed(tick))
            else {
                continue;
            };
            let derived =
                rule.cadence
                    .between(anchor, from, to)
                    .map_err(|error| EngineError::Conflict {
                        code: "recurrence_cadence_invalid",
                        message: error.to_string(),
                    })?;
            total = total.saturating_add(derived.len() as i128);
        }
        nucleus::DecimalValue::from_mantissa(0, total).map_err(|_| EngineError::Conflict {
            code: "rule_condition_unreadable",
            message: "that rhythm produced more dates than a number can hold".into(),
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
