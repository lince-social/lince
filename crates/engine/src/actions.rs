//! Actions (blueprint VII.2): the typed write surface. Semantic verbs,
//! validated in the engine, all terminating in `append()` and/or sidecar
//! updates, each with provenance. Protein never mutates; Actions never query.
//! Sands and Fiote speak only these — Fiote has no privileged path.

use chrono::{DateTime, Utc};
use nucleus::karma::{CanonicalHash, FrequencyAst, FrequencyParameterValue, LocalId, ProgramAst};
use nucleus::{Cause, CauseKind, Fact, NewFact, PromiseState, RecordKind};
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
    /// Set a record's quantity to a value (the delta is derived — one write path).
    SetQuantity {
        target: String,
        value: f64,
    },
    /// Atomically change ordinary unary state assertions and, optionally, a
    /// Record quantity. This is the Kanban move primitive: it never changes a
    /// Record identity, and its Fact plus assertion edits share one commit.
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
    /// Capture a classified change in one step: "ice cream, `@cost`, -10".
    /// The amount lands on the Record that actually moved, and what the change
    /// *was* is asserted about the Fact.
    ///
    /// Deliberately one action, not two. A form that made you first pick which
    /// total to affect would have reintroduced exactly the bookkeeping this
    /// design removes — totals are a query over classified changes, never a
    /// number a person maintains.
    ///
    /// `amount` is exact decimal text (`"-10"`, `"-10.50"`); its sign carries
    /// direction, so a refund is the same `@cost` concept with a positive
    /// amount and correctly *reduces* the total.
    CaptureEntry {
        /// The resource Record whose level moved — stock, hours, a balance.
        target: String,
        amount: String,
        /// What the change was. Resolved through the concept DAG, so `@food`
        /// answers a query for `@cost` when it sits under it.
        concept: Option<String>,
        #[serde(default)]
        note: Option<String>,
        /// Occurred-at, for backdating. Defaults to now.
        #[serde(default)]
        at: Option<String>,
        /// Idempotency key. Supplying one makes a retry return the first
        /// result instead of capturing the change twice — the difference
        /// between a network hiccup and the quantity moving twice. Omitting it is
        /// allowed for local one-shot callers and simply forgoes that
        /// protection.
        #[serde(default)]
        request_id: Option<String>,
    },
    /// Correct a captured change: wrong amount, wrong date, wrong note.
    ///
    /// Nothing is rewritten. The old Fact is compensated and a replacement is
    /// appended, so the chain keeps both and the correction is visible as a
    /// pair rather than as history that quietly changed. An edit that touches
    /// only the note moves no quantity and appends no Fact, but still earns a
    /// revision and an audit row.
    ///
    /// To change what a change *was*, use `classify-fact` — the quantity did not
    /// move, so there is nothing to compensate.
    ReviseEntry {
        entry: String,
        expected_revision: i64,
        request_id: String,
        /// Exact decimal text, like `capture-entry`.
        amount: String,
        #[serde(default)]
        note: Option<String>,
        #[serde(default)]
        at: Option<String>,
    },
    /// Undo a captured change entirely. The amount is returned by a
    /// compensating Fact carrying the same classification, so the category it
    /// was counted against is the category it is removed from. The event keeps
    /// its row and its history: an append-only Ledger has no delete.
    VoidEntry {
        entry: String,
        expected_revision: i64,
        request_id: String,
    },
    /// Re-assert what an already-recorded change was. Appends an assertion
    /// with an audit trail; it never touches the Fact, because the quantity did
    /// not move — only our account of what it meant.
    ClassifyFact {
        fact: String,
        concept: Option<String>,
        #[serde(default)]
        note: Option<String>,
    },
    /// Declare a named beat, read by any condition as `freq(@slug)`.
    ///
    /// Writes no Fact and moves nothing. A Frequency is a slug and a step; the
    /// beats it implies are derived from that step and its anchor on demand,
    /// which is why the one declaration serves both firing a rule and drawing a
    /// calendar without a second description of "when".
    ///
    /// Declared apart from any rule on purpose. A schedule written inline is a
    /// schedule only one rule can use, which is what made every previous
    /// cadence un-reusable.
    CreateFrequency {
        /// What a condition calls it: the `daily` in `freq(@daily)`.
        slug: String,
        /// What a person calls it. Defaults to the slug.
        #[serde(default)]
        head: Option<String>,
        /// The compound step this beat advances by.
        every: nucleus::karma::CadenceStep,
        /// Sets the beat's phase and time of day. Defaults to now.
        #[serde(default)]
        anchor_at: Option<String>,
        #[serde(default)]
        request_id: Option<String>,
    },
    /// Forget a named beat. Refused while a rule still reads it.
    DeleteFrequency {
        frequency: String,
    },
    /// Declare that a change is expected to repeat: a rent, a salary, a weekly
    /// count.
    ///
    /// This writes no Fact and moves nothing. It states what is expected, how
    /// often, and what it counts as; the dates it implies are derived on read,
    /// and each becomes real only when applied.
    CreateRecurrence {
        /// The Record this rule is about.
        target: String,
        /// What the rule does when one of its dates is applied. Ordered and
        /// non-empty; every item reduces to a typed Action a person could have
        /// performed by hand, so a rule gets no private write path.
        consequences: Vec<nucleus::karma::Consequence>,
        /// The *if* half of "when, if, then". Absent means unconditional: the
        /// date arriving is the whole reason to act.
        ///
        /// `condition` is an expression over Record readings, `gate` decides
        /// whether the number it computes means "fire", and `carry` decides
        /// what number the consequences receive.
        #[serde(default)]
        condition: Option<String>,
        #[serde(default)]
        gate: Option<String>,
        #[serde(default)]
        carry: Option<String>,
        #[serde(default)]
        note: Option<String>,
        cadence: nucleus::karma::Cadence,
        /// Sets the rule's phase and time of day. Defaults to now.
        #[serde(default)]
        anchor_at: Option<String>,
        #[serde(default)]
        request_id: Option<String>,
    },
    /// Change what a rule expects from here on.
    ///
    /// Dates already applied are Facts and keep the amount they carried — this
    /// is not a correction of history. To fix one that was applied wrongly,
    /// revise its entry.
    ReviseRecurrence {
        recurrence: String,
        expected_revision: i64,
        request_id: String,
        consequences: Vec<nucleus::karma::Consequence>,
        /// The *if* half of "when, if, then". Absent means unconditional: the
        /// date arriving is the whole reason to act.
        ///
        /// `condition` is an expression over Record readings, `gate` decides
        /// whether the number it computes means "fire", and `carry` decides
        /// what number the consequences receive.
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
    /// Stop or resume offering a rule's future dates. Disowns nothing already
    /// applied.
    SetRecurrencePaused {
        recurrence: String,
        expected_revision: i64,
        request_id: String,
        paused: bool,
    },
    /// Remove a rule for good, with its revision log and its skips.
    ///
    /// Distinct from pausing, which stops the future while keeping the rule on
    /// the list. Dates this rule already applied are ordinary entries and stay:
    /// the rule proposed them, it never owned them. What a delete removes is the
    /// rule's future, which is all a rule ever holds.
    DeleteRecurrence {
        recurrence: String,
    },
    /// Turn one expected date into a real change.
    ///
    /// This is an ordinary capture whose idempotency key names the rule and the
    /// date, so applying the same date twice is refused by the same UNIQUE that
    /// protects every other retry. `amount` overrides the rule's figure for
    /// this date alone — the bill that came in higher than the standing rule.
    ApplyRecurrenceOccurrence {
        recurrence: String,
        /// RFC3339, and it must be a date the rule actually produces.
        due_at: String,
        #[serde(default)]
        amount: Option<String>,
        #[serde(default)]
        note: Option<String>,
    },
    /// Decline one expected date. Recorded, because "decided against" and
    /// "nobody has looked yet" must not read the same.
    SkipRecurrenceOccurrence {
        recurrence: String,
        due_at: String,
        #[serde(default)]
        note: Option<String>,
    },
    /// Take a skip back, so the date is offered again.
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
    /// HARD delete (2026-07-17) — DISTINCT from `deactivate` (quantity -> 0).
    /// Tombstones the record: it vanishes from every read surface (record
    /// Proteins, slug resolution, rule inputs) and its UNIQUE slug is freed;
    /// the Ledger's facts are untouched (append-only, chain intact) and a
    /// final zero-delta annotation records the deletion + the freed slug.
    DeleteRecord {
        target: String,
    },
    /// Edit a record's text — its head (title) and/or body. Each present field
    /// is written; a zero-delta annotation fact carries provenance and refreshes
    /// live subscriptions. The CRDT relay for collaborative body editing is a
    /// separate surface (blueprint VII.4 record editor); this is the direct set.
    EditRecordText {
        target: String,
        #[serde(default)]
        head: Option<String>,
        #[serde(default)]
        body: Option<String>,
    },
    /// Rename a record's slug (`None`/empty clears it).
    SetSlug {
        target: String,
        slug: Option<String>,
    },
    /// Set a record's unit-of-measure concept (name or uid; `None` clears).
    SetUnit {
        target: String,
        unit: Option<String>,
    },
    /// Write a namespaced fds sidecar extension on a record (blueprint I.2).
    SetExtension {
        target: String,
        namespace: String,
        fds: serde_json::Value,
    },
    /// Adopt a pasted or scanned pairing code as a known Organ, under a name
    /// the LOCAL user types (Ontology §11).
    ///
    /// This is trust-on-first-use on the root key, and the UI must say so
    /// rather than implying the typing verified anything. It is safe exactly
    /// when the code came from somewhere unrelayable — a QR held up in person,
    /// or a chat app already authenticated to that human. No dial is needed,
    /// so it works with the other side offline.
    AddKnownOrgan {
        invite: String,
        name: String,
    },
    /// Post this Cell's pairing code into a thread, so the other party can add
    /// you. The promotion step happens INSIDE the conversation: you talk
    /// first, decide it is really them, and only then exchange keys.
    ShareMyKey {
        thread: String,
    },
    /// Open a conversation with a contact and offer it to them. Creates the
    /// Conversation (its own replica root) and a first Thread together —
    /// clicking "talk" needs somewhere to type immediately.
    StartConversation {
        contact: String,
        title: String,
    },
    /// A new topic inside a conversation. Needs no new grant: it is born
    /// inside the root that was already shared.
    OpenThread {
        conversation: String,
        title: String,
    },
    /// Append a message to a thread.
    SendMessage {
        thread: String,
        body: String,
    },
    /// Let a known contact Organ open live sessions on this Cell, acting as a
    /// Person here (Ontology §11 "live mode").
    ///
    /// No password: the iroh handshake already proved which Organ is on the
    /// connection, with a key rather than a secret someone could retype. What
    /// this decides is WHO they are once inside — every read they make is
    /// gated by that Person's visibility, so this grants a named identity
    /// rather than a door.
    GrantOrganLogin {
        organ: String,
        /// Name for the Person they act as. A new Person is created unless one
        /// with this name already answers to it.
        person_name: String,
    },
    /// Take a live login back. Local, immediate, and not a request the other
    /// side may decline (§12).
    RevokeOrganLogin {
        organ: String,
    },
    /// Say yes to a thread invite: keep a copy of the offered conversation.
    ///
    /// This opens the conversation and nothing else — no trust, no sync, no
    /// key. Agreeing to read what someone sends is not deciding who they are.
    AcceptThreadInvite {
        invite: String,
    },
    /// Say no: revoke the offered grant and clear the invite, which also frees
    /// this Organ's one-pending slot so they may ask again later.
    DeclineThreadInvite {
        invite: String,
    },
    /// Issue a single-use, short-lived device-enrolment token (Ontology §11).
    /// The plaintext comes back in `outcome.created` and is never stored — the
    /// database keeps only a hash, because a token sitting in a row would be a
    /// second, quieter way into the identity. Requires the root key.
    RosterEnrolToken,
    /// Remove a Cell from the roster. This IS revocation: the roster is the
    /// membership list, and the version bump stops the old one being replayed
    /// to re-add a stolen device. Requires the root key.
    RosterRevokeCell {
        cell_uid: String,
    },
    /// Copy the root key to removable media, at mode 0600. Refuses to
    /// overwrite anything already there.
    RootKeyExport {
        destination: String,
    },
    /// Delete the LOCAL root key, having verified byte-for-byte that the copy
    /// at `copy_at` matches. The verification is the whole point: detaching
    /// without it destroys an identity that no authority can restore.
    RootKeyDetach {
        copy_at: String,
    },
    /// Set a contact's trust level (blueprint XV): `unknown` | `known` |
    /// `blocked`. Blocking is just this with `trust: "blocked"` — no
    /// separate action.
    SetContactTrust {
        target: String,
        trust: String,
    },
    /// Set a contact's local proximity ranking (never exported, blueprint XV).
    SetContactProximity {
        target: String,
        proximity: u32,
    },
    /// Undo a prior fact by appending its inverse (compensation, blueprint II.3):
    /// an append-only Ledger never deletes, so undo is a new fact with the
    /// opposite delta, caused by the original. Metadata/annotation facts
    /// (delta 0) have nothing to reverse and compensate to a no-op.
    Compensate {
        fact: String,
    },
    /// Reverse only the private Record application of one immutable Transfer
    /// settlement. Public fulfillment evidence is not withdrawn.
    CompensateTransferOccurrenceSettlement {
        settlement: String,
        request_id: String,
        /// Required only in trusted local mode.
        #[serde(default)]
        person: Option<String>,
    },
    /// Create an unsent hidden draft for one exact partial remainder.
    CreateTransferRemainderDraft {
        occurrence: String,
        expected_revision: u64,
        expected_remaining_quantity: f64,
        request_id: String,
        #[serde(default)]
        person: Option<String>,
    },
    /// Propose an append-only reversing Transfer without rewriting fulfillment.
    CreateReversingTransferDraft {
        occurrence: String,
        expected_revision: u64,
        canonical_quantity: f64,
        request_id: String,
        #[serde(default)]
        person: Option<String>,
    },
    /// Add a proposed successor promise in a new signed revision while keeping
    /// the terminal predecessor and all its evidence unchanged.
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
    /// Atomically turn a unary assertion `A @task` into the binary `A @task
    /// [object]` under the same predicate — retract+assert as one step.
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
    /// Start a discussion thread attached to any record. Threads are ordinary
    /// records (`kind=thread`) linked `thread --thread-of--> target`.
    CreateThread {
        target: String,
        head: String,
    },
    /// Add a message to a thread. Messages are ordinary records
    /// (`kind=message`) linked `message --message-in--> thread`; replies add
    /// `message --reply-to--> parent_message`.
    CreateMessage {
        thread: String,
        body: String,
        #[serde(default)]
        parent: Option<String>,
        /// Existing Records deliberately disclosed by this message. These are
        /// ordinary `message --references--> record` links, not uploaded files
        /// or a Transfer-private attachment model.
        #[serde(default)]
        references: Vec<String>,
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
        /// true = published Need with an unfilled party slot (blueprint V).
        #[serde(default)]
        open: bool,
    },
    /// Drive the promise state machine (nucleus validates the transition).
    PromiseTransition {
        promise: String,
        to: PromiseState,
    },
    /// Edit a standalone promise. Bundled promises use complete signed
    /// Transfer revisions so agreement invalidation remains atomic.
    EditPromiseDelta {
        promise: String,
        delta: f64,
    },
    /// Answer a decision-record: records the answer and closes it (quantity -> 0).
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
        /// Default `reserve_from` for the bundle's promises (V.3).
        #[serde(default)]
        reserve_default: Option<String>,
        /// Settlement demands delivery+receipt confirmations (VIII.3).
        #[serde(default)]
        require_confirmation: bool,
    },
    /// Create a complete, reviewed manual draft in one typed Action. Every
    /// token is resolved and every term is validated before the store commits
    /// the transfer record, sidecar, parties, and promises together.
    CreateTransferDraft {
        request_id: String,
        /// Trusted local no-auth mode must choose its acting Person explicitly.
        /// Authenticated sessions derive this from app_user -> Person instead.
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
    /// Replace the complete public terms of one promise under an optimistic
    /// revision precondition. The store commits terms, invalidation, signed
    /// snapshot and request idempotency as one transaction.
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
    /// Address a Person without making them a party. The creator is derived
    /// from the signed Transfer rather than accepted from the client.
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
    /// Rejection is lifecycle evidence and deliberately does not revise the
    /// proposed terms.
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
    /// Replace the one canonical proposal. Counteroffers never create a
    /// competing revision branch.
    CounterofferTransfer {
        transfer: String,
        expected_revision: u64,
        request_id: String,
        /// Required in trusted local mode; authenticated mode derives it.
        #[serde(default)]
        person: Option<String>,
        draft: TransferDraftRevisionInput,
    },
    /// Refine and claim one visible OPEN promise. `duplicate` leaves the
    /// source template OPEN; `consume` assigns/replaces it.
    ClaimOpenTransferPromise {
        transfer: String,
        promise: String,
        expected_revision: u64,
        request_id: String,
        /// Required in trusted local mode; authenticated mode derives it.
        #[serde(default)]
        person: Option<String>,
        terms: TransferPromiseInput,
    },
    /// Move only the acting Person's agreement by one adjacent milestone for
    /// one exact signed Transfer revision. Every transition, including a
    /// retraction, is retained as immutable signed evidence.
    SetTransferAgreementLevel {
        transfer: String,
        expected_revision: u64,
        request_id: String,
        /// Required in trusted local mode; authenticated mode derives it.
        #[serde(default)]
        person: Option<String>,
        level: u8,
    },
    /// Materialize immutable directed occurrences for every policy-ready
    /// promise owned by one Person on the exact current signed revision.
    ActivateTransferOccurrence {
        transfer: String,
        promise: String,
        expected_revision: u64,
        request_id: String,
        /// Required only in trusted local mode. Authenticated sessions derive
        /// this from their app-user binding and may not override it.
        #[serde(default)]
        person: Option<String>,
    },
    /// Assert or correct one role-specific real-world fulfillment claim.
    /// Delivery belongs to the occurrence giver; receipt to its receiver.
    SetTransferOccurrenceClaim {
        occurrence: String,
        request_id: String,
        /// Required only in trusted local mode.
        #[serde(default)]
        person: Option<String>,
        role: TransferOccurrenceClaimRole,
        claimed: bool,
    },
    /// Assert only the authenticated Person's currently missing role across an
    /// exact, acknowledged preview. The store rejects the whole batch if any
    /// revision or claim-state token changed.
    CompleteTransferOccurrenceClaimsBulk {
        request_id: String,
        #[serde(default)]
        person: Option<String>,
        review_token: String,
        items: Vec<TransferOccurrenceBulkClaimInput>,
    },
    /// Assert or retract this participant's occurrence dispute. Current
    /// disputed state remains true while either participant's latest event is
    /// asserted.
    SetTransferOccurrenceDispute {
        occurrence: String,
        request_id: String,
        /// Required only in trusted local mode.
        #[serde(default)]
        person: Option<String>,
        disputed: bool,
    },
    /// Persist the receiver's private deterministic local-application rule.
    /// This is signed identity evidence, but it never changes public terms or
    /// invalidates agreement. The only occurrence input is `incoming()`.
    SetTransferOccurrenceApplicationFormula {
        occurrence: String,
        request_id: String,
        /// Required only in trusted local mode.
        #[serde(default)]
        person: Option<String>,
        formula: String,
    },
    /// Settle one reviewed, positive fulfillment slice. All `expected_*`
    /// fields are compare-and-set inputs from the private Protein projection;
    /// the engine independently recomputes the local Record application.
    SettleTransferOccurrence {
        occurrence: String,
        request_id: String,
        /// Required only in trusted local mode.
        #[serde(default)]
        person: Option<String>,
        canonical_quantity: f64,
        expected_remaining_quantity: f64,
        expected_local_delta: f64,
        expected_application_formula_hash: String,
        expected_application_formula_version: u64,
        expected_remainder_policy: nucleus::transfer::TransferRemainderPolicy,
    },
    /// Create an explicit recipient policy. Hosted is the conservative
    /// default; replicated must be selected explicitly by the signer.
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
    /// Queue a fresh authoritative snapshot after explicit conflict review.
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
    /// Record a delivery/receipt confirmation as an annotation fact (VIII.3).
    ConfirmTransfer {
        transfer: String,
        /// `delivery` | `receipt`
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
        /// Chain/spectator condition, same grammar as Karma conditions.
        condition: Option<String>,
    },
    AgreeTransfer {
        transfer: String,
        party: String,
        level: i64,
    },
    /// agreed -> active for the bundle's promises, within policy.
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
        subject_kind: String, // organ | actor | public | fiote
        subject: Option<String>,
        target: String,
    },
    /// Persist a Protein AST as a record (kind='protein') — the saved view.
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
    RespondKarmaCandidate {
        request_id: String,
        candidate_hash: CanonicalHash,
        expected_state_revision: u64,
        response: nucleus::karma::CandidateReviewAction,
        /// K5.2: naming one grant authorizes the accepted `act` proposal into a
        /// durable intent in the same commit. Omitted, acceptance stays inert.
        #[serde(default)]
        authorizing_grant_uid: Option<String>,
    },
    /// K5.1 delegation grants. None of these carry a principal: the grant belongs
    /// to the Person whose installed key signs it, so a payload cannot name a
    /// different holder of the authority.
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
    /// Senses match rule (blueprint X.1): a record; `activate`/`deactivate`
    /// work on it like on any rule.
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
    /// Lingua adoption (blueprint III.2): insert foreign concepts preserving
    /// uid and lineage; one tap on import.
    AdoptConcepts {
        concepts: Vec<ConceptSeed>,
    },
    DeclareEquivalence {
        a: String,
        b: String,
    },
    /// The permission/role/user system (2026-07-18): native SQL state
    /// (`store::auth`), NOT Ledger records — no facts, no `created` uid
    /// convention beyond the new row's numeric id. Each variant is gated on
    /// the matching key already in `utils::auth::ALL_PERMISSIONS`
    /// (`role:create`, `user:create`, `user:assign_role`, `permission:assign`).
    CreateRole {
        name: String,
    },
    CreateUser {
        username: String,
        name: String,
        password: String,
        /// Role name — must already exist (`CreateRole` first); this action
        /// does not silently create one (that's `role:create`'s job alone).
        role: String,
    },
    AssignRole {
        /// The target `app_user.id`, as a string (same convention as `actor`).
        user: String,
        role: String,
    },
    /// Establish which Ledger Person an authenticated app user may represent.
    /// This is an administrative identity decision, not a client-supplied
    /// Transfer field.
    AssignUserPerson {
        user: String,
        person: String,
    },
    GrantPermission {
        role: String,
        /// `"subject:action"`, e.g. `"record:delete_own"`.
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
    /// `None` is an OPEN Person slot.
    #[serde(default)]
    pub party: Option<String>,
    /// Publish the acting Person's offer/request without naming a counterparty.
    /// Ownership remains explicit; only the matching counterparty is open.
    #[serde(default)]
    pub open: bool,
    pub delta: f64,
    /// Explicit canonical unit; `None` means intentionally unitless.
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
    /// Immutable creator Person, repeated in the complete reviewed snapshot.
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

/// One adoptable concept in a package (blueprint III.2).
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

/// Keeps legacy Transfer verbs deserializable while the sequential workflow
/// replaces their non-revisioned persistence paths phase by phase.
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

/// Exact from the keystroke: typed text becomes a decimal without ever being a
/// float, so `-10.50` survives as `-10.50`.

pub(crate) fn parse_instant_field(text: &str) -> Result<DateTime<Utc>, EngineError> {
    chrono::DateTime::parse_from_rfc3339(text)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| EngineError::Conflict {
            code: "entry_at_invalid",
            message: format!("`{text}` is not an RFC3339 instant"),
        })
}

/// Every reading a rule's condition took, gathered before evaluation.
///
/// Gathered rather than looked up lazily because the kernel's evaluator is
/// synchronous and the database is not. Reading first also means the whole
/// condition sees one consistent moment, instead of each term seeing whatever
/// the world looked like when its own query happened to land.
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

/// How deep one rule may read another rule's arithmetic before the chain is
/// called a circle. Four is far past any honest spreadsheet.
const VALUE_DEPTH_CAP: usize = 4;

fn reading_key(func: &str, slug: &str, window_secs: Option<i64>) -> String {
    match window_secs {
        Some(secs) => format!("{func}:{slug}:{secs}"),
        None => format!("{func}:{slug}"),
    }
}

/// Turn the three optional condition fields into a stored condition.
///
/// Refused here rather than at fire time, for the same reason a cadence is: a
/// rule whose condition cannot be read is not a rule that fires cautiously, it
/// is a rule nobody can predict. Finding that out on a Tuesday at 3am, inside a
/// heartbeat with no one watching, is the worst place to learn it.
///
/// Gate and carry default to the pair that reproduces the oldest behaviour:
/// fire on any non-zero number, and hand that number over unchanged.
fn parse_rule_condition(
    condition: Option<String>,
    gate: Option<String>,
    carry: Option<String>,
) -> Result<Option<store::recurrence::RuleCondition>, EngineError> {
    let Some(source) = condition
        .map(|c| c.trim().to_string())
        .filter(|c| !c.is_empty())
    else {
        // A gate without a condition has nothing to gate. Silently dropping it
        // would make the rule fire always, which is the opposite of what
        // someone writing a gate wants.
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
    /// Facts committed by this action (including any Karma cascade).
    pub facts: Vec<Fact>,
    /// Uid of a created row (record, concept, link, promise), when applicable.
    pub created: Option<String>,
    /// Non-fatal advisories (blueprint IV.2: cycle warnings on save). The
    /// action succeeded; these are for the surface to show.
    pub warnings: Vec<String>,
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
            // An authorized acceptance reports the intent it created, so the
            // caller never has to guess whether authority was actually taken.
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
    /// Execute one Action with provenance. `actor` is the acting person/organ
    /// record uid; it lands on every fact this action commits.
    pub async fn act(
        &self,
        action: Action,
        actor: Option<String>,
    ) -> Result<ActionOutcome, EngineError> {
        self.act_at(action, actor, Utc::now()).await
    }

    /// Close due invitations as signed lifecycle evidence. Rejection and
    /// expiry intentionally leave the canonical terms revision unchanged.
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

    /// `act` with an explicit clock — the DST-drivable variant (Part 0): same
    /// action, virtual `now`.
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
        // Applying an occurrence is one firing, wherever it came from — the
        // heartbeat, a reaction, or a person pressing apply in the inbox.
        //
        // While it runs, the entry that marks the date done is not committed
        // yet. So a rule reacting to the facts this firing commits would look
        // at its own date, find it unspent, and apply it a second time. The
        // guard has to sit on the *apply*, not on any one caller: a manual
        // apply reaches exactly the same code by a different road.
        //
        // Chains are not lost, only deferred to where they are safe: the
        // reaction that started this follows them through its own queue, and
        // `fire_due_rules` follows them after the fact.
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
                        quantity: store::exact::zero(), // level arrives via the one write path below
                    },
                )
                .await?;
                // Always drop a creation fact — the initial level as a delta
                // (zero-delta when quantity==0, blueprint I.1/II.3 provenance).
                // Without it, a record created at quantity 0 commits no fact and
                // never invalidates live subscriptions, so it stays invisible to
                // every subscribed sand until some later fact touches it.
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
                // A destination tag wins if configuration accidentally lists it
                // on both sides; otherwise this transition would retract state
                // it has just established.
                retract_uids.retain(|uid| !assert_uids.contains(uid));

                let signer = self.signer.lock().await.clone();
                let mut tx = self.store.pool.begin().await?;
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
                // The replay guard runs BEFORE any Ledger work, and it has to.
                // `append` commits its own transaction, so a replay detected
                // later would already have moved the quantity a second time, and
                // the error afterwards would not put it back.
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
                // Exact from the keystroke: the typed text is parsed straight
                // into a decimal, so "-10.50" never becomes a float on the way
                // to a signed Fact.
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
                // Classification is an assertion ABOUT the Fact, so it happens
                // after the Fact is sealed and never enters its preimage.
                if let Some(fact) = outcome.facts.first() {
                    store::ledger::classify_fact(
                        &self.store.pool,
                        &fact.uid,
                        concept_uid.as_deref(),
                        fact.actor_uid.as_deref(),
                        note.as_deref(),
                    )
                    .await?;
                    // The entry is what makes this change editable later. The
                    // Fact and its classification cannot change; this can.
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
                    // Handed back so a surface can revise or void this change
                    // without having to search for the event it just made.
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
                let declared_condition = parse_rule_condition(condition, gate, carry)?;
                // Keeping the anchor by default matters: silently re-anchoring
                // to "now" on an edit would shift every future date of a rule
                // whose author only meant to change what it does.
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
                // A date the rule does not produce is not an occurrence of it.
                // Without this check, "apply" degenerates into a capture that
                // merely claims a rule's name, and the derived timeline would
                // show an applied date that no cadence explains.
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
                // A one-instant window is still correct with a landing rule in
                // play: the derivation widens its own scan and filters on the
                // landed instant, so a date that landed here is found here.
                if !produced.dates.contains(&due) {
                    return Err(EngineError::Conflict {
                        code: "recurrence_occurrence_unknown",
                        message: format!("`{due_at}` is not a date this rule produces"),
                    });
                }
                // Applying twice must do nothing the second time. The entry
                // carrying this date's request id is the only record that it
                // ran, and for a capture-only rule the UNIQUE index alone would
                // have been enough. It is not enough once a rule can add a
                // quantity or toggle a concept: those would run again before
                // the capture was refused. So the guard moves to the front and
                // covers every consequence.
                if let Some(existing) =
                    store::recurrence::applied(&self.store.pool, &rule.uid, due).await?
                {
                    outcome.created = Some(existing);
                    return Ok(outcome);
                }

                // The *if* half. A date arriving is only half a reason to act:
                // the condition is asked now, against the world as it stands,
                // which is what lets "every day, but only when stock is low"
                // mean what it says.
                //
                // A blocked gate is not an error and not a skip. The rule
                // looked and decided not to act, so the date is simply left
                // unapplied — it will be asked again next beat, because the
                // answer can change without the rule changing.
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

                // One entry per applied date, always — it is what marks the
                // occurrence done. A rule that captures uses its declared
                // amount; a rule that only changes concepts writes a zero
                // delta, which is the same annotation shape every metadata edit
                // in Lince already uses and is what makes live subscriptions
                // refresh.
                let capture_concept = rule.consequences.capture_concept().map(str::to_string);
                let declared = match amount.as_deref() {
                    Some(text) => text.trim().to_string(),
                    // What the condition carried, when there is one. This is
                    // the whole point of a carry: `-1 * freq(@daily)` puts -1
                    // into the Record, rather than the rule having to hardcode
                    // a number it could have computed.
                    // The number the condition carried — but only for a rule
                    // that genuinely captures. The marker entry exists for
                    // every applied date, so letting a carry into it
                    // unconditionally would move an `add-quantity` rule's
                    // figure twice: once through this entry and once through
                    // the consequence itself. A rule that does not capture
                    // marks its date with a zero.
                    None => match rule.consequences.capture_amount() {
                        None => "0".to_string(),
                        Some(declared) => match carried {
                            Some(value) => value.to_string(),
                            None => declared.to_string(),
                        },
                    },
                };
                // Applying is an ordinary capture. Reusing the same path is
                // what keeps a rule-applied change indistinguishable from a
                // hand-typed one in the Ledger afterwards — nothing downstream
                // needs to know a rule was involved to read a balance.
                let capture = Action::CaptureEntry {
                    target: rule.record_uid.clone(),
                    amount: declared,
                    concept: capture_concept,
                    note: note.or_else(|| rule.note.clone()),
                    at: Some(due.to_rfc3339()),
                    request_id: Some(store::recurrence::occurrence_request_id(&rule.uid, due)),
                };
                // Deliberately `None`: any signed authorship on this action
                // attested *applying an occurrence*, not capturing an entry.
                // Forwarding it would let one signature stand for an action
                // shape its signer never saw. The outer action has already
                // cleared its own authority, and the capture is its
                // consequence rather than a second attested request.
                let applied =
                    Box::pin(self.act_at_with_authorship(capture, actor.clone(), now, None))
                        .await?;
                outcome.facts = applied.facts;
                outcome.created = applied.created;

                // The rest of the rule, in the order its author wrote it. The
                // capture above already covered `CaptureEntry`.
                for consequence in rule.consequences.iter() {
                    let next = match consequence {
                        nucleus::karma::Consequence::CaptureEntry { .. } => continue,
                        // A written number wins; without one, the consequence
                        // receives what the condition computed. A rule with
                        // neither has no figure at all and does nothing,
                        // rather than silently assigning zero.
                        nucleus::karma::Consequence::SetQuantity { value } => {
                            let Some(figure) = value.or(carried) else {
                                continue;
                            };
                            Action::SetQuantity {
                                target: rule.record_uid.clone(),
                                value: figure.to_f64(),
                            }
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
                        // Everything that leaves the Cell, asks a person, or
                        // binds a second party. None of it runs here: it is
                        // committed as an obligation, a question or a queued
                        // effect, so the worker that carries it out can still
                        // refuse. That separation is what keeps a rule from
                        // acquiring a private way to reach the outside world.
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
                // Replay guard first, for the same reason as capture: a retry
                // that got as far as appending would compensate twice.
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

                // The quantity only moves if the amount or the instant
                // changed. A
                // note-only edit is bookkeeping about a change, not a change,
                // and appending a compensating pair for it would put two
                // meaningless entries in the chain.
                //
                // The amount is compared numerically, not representationally:
                // `DecimalValue` equality includes the scale, so re-typing
                // `-15` as `-15.00` would otherwise read as a change.
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
                    // Carry the classification onto both new Facts. Without
                    // this, correcting an amount would silently drop the
                    // change out of the category it belonged to.
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
                        // The compensation carries the same classification, so
                        // undoing a food expense removes it from food rather
                        // than leaving food overstated and an unclassified
                        // credit floating beside it.
                        let concept_uid =
                            store::ledger::fact_concept(&self.store.pool, &old_fact_uid).await?;
                        outcome.facts = self
                            .append(
                                NewFact {
                                    uid: None,
                                    record_uid: old_fact.record_uid.clone(),
                                    delta: store::exact::negate(old_fact.delta)?,
                                    // The original instant, not now — the same
                                    // rule revising uses. Voiding says the
                                    // change never happened, so it is retracted
                                    // from the period that claimed it and that
                                    // period nets to zero.
                                    //
                                    // This is NOT how a reversal is recorded. A
                                    // purchase that really happened and was
                                    // later refunded is a new capture today
                                    // with the opposite sign; that keeps both
                                    // periods honest. Voiding is for "this was
                                    // never true", and the difference matters
                                    // to anyone reading last month's totals.
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
                // Annotate FIRST (the appender needs a live record), then
                // tombstone; the fact survives the record's disappearance.
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
                // Through the record-doc (collab): converges with concurrent
                // remote edits and logs ONE cumulative crdt op.
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
            Action::AddKnownOrgan { invite, name } => {
                let invite = crate::pairing::PairingInvite::decode(&invite)?;
                let name = name.trim();
                if name.is_empty() {
                    return Err(EngineError::Consequence(
                        "give this contact a name you will recognise".into(),
                    ));
                }
                // The uid is theirs to declare, and a code cannot declare it —
                // only an Introduction over a real connection can. So the row
                // is held under a uid derived from the NodeId and FLAGGED: the
                // next sync pass dials them, learns the real uid, and replaces
                // this row with it. Until that happens they cannot sync, and
                // the flag is what stops that from being a silent dead end.
                let organ_uid = format!("o-{}", &invite.node_id);
                store::organs::add_contact(&self.store.pool, &organ_uid, None, name, "", 1).await?;
                store::organs::set_node_id(&self.store.pool, &organ_uid, Some(&invite.node_id))
                    .await?;
                if let Some(root_key) = &invite.root_key {
                    // TOFU, and the ONLY moment it happens: every roster and
                    // succession afterwards must chain from this key.
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
                store::organs::set_pending_introduction(&self.store.pool, &organ_uid, true).await?;
                outcome.warnings.push(
                    "added — but they are not reachable for sync until this Cell has connected \
                     to them once and learned their identity."
                        .into(),
                );
                outcome.created = Some(organ_uid);
            }
            Action::StartConversation { contact, title } => {
                let contact_uid = self.resolve(&contact).await?;
                let (conversation, thread) =
                    self.start_conversation(&contact_uid, title.trim()).await?;
                // Both uids come back: the caller opens the thread, but the
                // conversation is what was actually shared.
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
            Action::SendMessage { thread, body } => {
                let thread_uid = self.resolve(&thread).await?;
                let body = body.trim();
                if body.is_empty() {
                    return Err(EngineError::Consequence("nothing to send".into()));
                }
                // The head is a label for lists; the body is the message. A
                // long message gets an elided label rather than a wall of text
                // where a title belongs.
                let head: String = match body.char_indices().nth(60) {
                    Some((cut, _)) => format!("{}…", &body[..cut]),
                    None => body.to_string(),
                };
                outcome.created = Some(self.send_message(&thread_uid, &head, body).await?);
            }
            Action::GrantOrganLogin { organ, person_name } => {
                let organ_uid = self.resolve(&organ).await?;
                let contact = store::organs::contact(&self.store.pool, &organ_uid)
                    .await?
                    .ok_or_else(|| EngineError::Consequence("not a contact".into()))?;
                // `known` and nothing less. An unvetted contact reaching the
                // thread door is the design; an unvetted contact acting as a
                // Person inside this Cell is not.
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
                // Requiring the root here is the design, not an obstacle:
                // enrolling a device grants membership in the identity, and
                // that should feel deliberate.
                if self.root_signer().await?.is_none() {
                    return Err(EngineError::Consequence(
                        "the root key is not on this Cell — bring it back to enrol a device".into(),
                    ));
                }
                outcome.created = Some(self.issue_enrolment_token().await?);
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
                // Same reasoning for a Fact that belongs to an Entry:
                // generic compensation would return the quantity while leaving the
                // event reading `applied`, so the Ledger and the thing that
                // describes it would disagree with no way to tell which is
                // right. `void-entry` does both halves.
                if store::entries::for_fact(&self.store.pool, &original.uid)
                    .await?
                    .is_some()
                {
                    return Err(EngineError::Conflict {
                        code: "entry_void_required",
                        message: "this Fact belongs to an Entry; use void-entry".into(),
                    });
                }
                // Zero-delta facts (metadata/annotation) carry no quantity to
                // reverse — undoing them is a no-op, not an error.
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
                    // Individual replicas carry Record/assertion ops, not the
                    // general Fact feed. The level therefore starts active in
                    // its quantity set op; this zero-delta signal refreshes
                    // local Protein views and wakes the durable outbox without
                    // leaking conversation metadata to general-sync contacts.
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
                parent,
                references,
            } => {
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
                if body.is_empty() && references.is_empty() {
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
                // zero-delta annotation fact on the target record: promise
                // history is Ledger-visible provenance (blueprint V.2)
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
                // Upsert by slug so a saved Protein is full CRUD: saving the same
                // name again updates the title + AST (and reactivates it if it
                // had been deactivated/"deleted"), rather than colliding on the
                // UNIQUE slug. The pretty AST is also stored in `body` so a plain
                // records Protein can list saved Proteins WITH their query for the
                // editor (the `lince.protein` extension stays canonical for reads).
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
            // One boxed future for the whole grant family: `act_at`'s state machine
            // is already near the debug-build stack limit, and inlining four more
            // arms overflows it.
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
            }
            Action::Decide { decision, answer } => {
                store::misc::answer_decision(&self.store.pool, &decision, &answer).await?;
                // the chosen option may carry an Action to execute (XIII.1)
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
                // closing the decision is a fact (quantity 1 -> 0), so Karma can
                // react to answered decisions like anything else (XIII.1)
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
                let user_id = store::auth::create_user(
                    &self.store.pool,
                    &name,
                    &username,
                    &password_hash,
                    role_id,
                )
                .await?;
                outcome.created = Some(user_id.to_string());
            }
            Action::AssignRole { user, role } => {
                self.require_permission(actor.as_deref(), "user:assign_role")
                    .await?;
                let user_id: i64 = user
                    .parse()
                    .map_err(|_| EngineError::Consequence(format!("bad user id `{user}`")))?;
                let role_id = store::auth::role_by_name(&self.store.pool, &role)
                    .await?
                    .ok_or_else(|| EngineError::Consequence(format!("unknown role `{role}`")))?;
                store::auth::set_user_role(&self.store.pool, user_id, role_id).await?;
            }
            Action::AssignUserPerson { user, person } => {
                self.require_permission(actor.as_deref(), "user:assign_person")
                    .await?;
                let user_id: i64 = user
                    .parse()
                    .map_err(|_| EngineError::Consequence(format!("bad user id `{user}`")))?;
                if store::auth::user_by_id(&self.store.pool, user_id)
                    .await?
                    .is_none()
                {
                    return Err(EngineError::Consequence(format!("unknown user `{user}`")));
                }
                let person = self.resolve(&person).await?;
                let record = store::records::get(&self.store.pool, &person)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(person.clone()))?;
                if record.kind != RecordKind::Person.as_str() {
                    return Err(EngineError::Consequence(
                        "an app user can only be assigned to a person record".into(),
                    ));
                }
                store::auth::set_user_person(&self.store.pool, user_id, &person).await?;
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

    async fn resolve(&self, token: &str) -> Result<String, EngineError> {
        let token = token.trim_start_matches('@');
        store::records::resolve(&self.store.pool, token)
            .await?
            .map(|r| r.uid)
            .ok_or_else(|| EngineError::UnknownRecord(token.to_string()))
    }

    /// Resolve an actor id to the `app_user` it names. Every permission check
    /// goes through this — a `Some` actor that doesn't resolve to a real user
    /// (stale JWT, deleted account) is denied rather than silently treated as
    /// local, and it's the one place that parses the actor-id-as-string
    /// convention shared with `created_by`/provenance.
    async fn actor_user(&self, actor: &str) -> Result<store::auth::AuthUser, EngineError> {
        let user_id: i64 = actor
            .parse()
            .map_err(|_| EngineError::Forbidden("unrecognized actor".into()))?;
        store::auth::user_by_id(&self.store.pool, user_id)
            .await?
            .ok_or_else(|| EngineError::Forbidden("unrecognized actor".into()))
    }

    /// `actor == None` is the local, no-auth Cell — unrestricted, matching
    /// every other action's local-mode convention. A `Some` actor must hold
    /// `permission` (a `"subject:action"` key from `utils::auth::ALL_
    /// PERMISSIONS`) on their role.
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

    /// Resolve the authenticated app user to the Person they are allowed to
    /// represent. Local no-auth mode stays trusted and returns `None`; an
    /// authenticated session without an explicit binding is blocked instead
    /// of accepting a Person uid supplied by the client.
    pub(crate) async fn actor_person(
        &self,
        actor: Option<&str>,
    ) -> Result<Option<String>, EngineError> {
        let Some(actor) = actor else {
            return Ok(None);
        };
        let user = self.actor_user(actor).await?;
        store::auth::person_for_user(&self.store.pool, user.id)
            .await?
            .map(Some)
            .ok_or_else(|| {
                EngineError::Forbidden("authenticated user has no assigned person identity".into())
            })
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

    /// Creation needs both the role grant and a social identity. The returned
    /// Person is inserted as the creator's initial Transfer party.
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

    /// Authenticated actions derive their Person from the user binding. Local
    /// no-auth actions may name one explicitly; a lifecycle action can instead
    /// provide an unambiguous Person derived from its signed target.
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

    /// Terms may be edited by the authenticated creator or by a represented
    /// participant (counteroffers). Permission remains an independent gate.
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

    /// Legacy settlement still carries a Person field on the wire. In an
    /// authenticated session it must match the server-side identity exactly.
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

    /// Deletion is split into two grants (blueprint permission system):
    /// `record:delete` (any record) and `record:delete_own` (only records
    /// this actor created, per the earliest fact's `actor_uid`).
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

    /// Resolve a rule's consequences and prove the list is legal.
    ///
    /// Concept tokens arrive as whatever the author typed and leave as uids, so
    /// a later rename cannot change what a rule does. Validation happens here
    /// rather than at the store boundary because "a rule with nothing to do"
    /// and "the same consequence twice" are authoring mistakes, and the person
    /// who can still fix them is the one submitting this Action.
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

    /// Ask a rule's condition, against the world as it stands now.
    ///
    /// Returns what the carry hands to the consequences, or `None` when the
    /// gate blocked — which is a decision, not a failure.
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

        // The stretch of time this evaluation speaks for. Every rhythm a
        // condition reads is counted over the same window, gathered here with
        // every other reading, so one evaluation sees one consistent moment and
        // the answer cannot depend on the order the tokens happen to be listed
        // in.
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

    /// One reading a condition asked for.
    ///
    /// The vocabulary lives in the kernel; which table answers it lives here.
    /// An unknown reading is refused rather than defaulted to zero, because a
    /// zero would let a typo read as "the stock is empty" and fire a rule for
    /// the most alarming possible reason.
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
            // A rhythm, read as a number: how many times the rule on that
            // Record came round in the stretch this evaluation speaks for.
            //
            // This is what makes a schedule part of the arithmetic instead of a
            // separate kind of object. `-1 * freq(@rent)` is worth -1 on a rent
            // date and exactly zero on every other, so the ordinary `!=0` gate
            // turns a daily check into a monthly act — no second trigger
            // mechanism, no second table, and any threshold already spellable
            // works on it unchanged.
            "freq" => {
                // A declared Frequency answers first. It is the whole object —
                // a slug and a step — and its beats come from the same pure
                // `Cadence` that draws a calendar, so nothing is stored and
                // nothing has to be kept in sync with the step.
                if let Some(frequency) = store::frequency::resolve(&self.store.pool, slug).await? {
                    let anchor = frequency.anchor()?;
                    // `(since, at]` — half-open at the near edge, closed at the
                    // far one, so the instant a window ends on belongs to that
                    // window and to no other. Same edges as the rhythm this
                    // replaces, or a rule would double-count on the boundary.
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
                // Falling back to a rhythm carried by a rule on that Record,
                // which is what a frequency was before it had a table of its
                // own. Rules written the old way keep working.
                let uid = self.resolve(slug).await?;
                self.rhythm_count(&uid, since, at).await
            }
            // Another rule's arithmetic, read as a number — its gate ignored,
            // its consequences not run. This is what makes a rule usable as a
            // named cell: one rule computes "how much is left this month" and
            // several others read it, instead of each restating the formula and
            // drifting apart the first time one is edited.
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
            // Where a promise stands, as an ordinal a comparison can use.
            "promise_state" => inexact(
                store::misc::promise_state(&self.store.pool, slug)
                    .await?
                    .map(nucleus::PromiseState::ordinal)
                    .unwrap_or(0.0),
            ),
            // How long since anything happened on a Record. A Record nothing
            // has ever touched reads as an enormous number rather than zero:
            // "never" is the opposite of "just now", and zero would say the
            // opposite of the truth to every `>` a person writes.
            "hours_since_fact" => {
                let uid = self.resolve(slug).await?;
                inexact(
                    store::facts::hours_since_last(&self.store.pool, &uid, now)
                        .await?
                        .unwrap_or(1.0e9),
                )
            }
            // How reliably a party has kept what they promised.
            "confidence" => inexact(crate::imagination::confidence(&self.store, slug).await?),
            // A concept's share of activity in this hour of the day.
            "demand" => inexact(crate::imagination::demand(&self.store, slug, now).await?),
            // Where a Record's level is heading, folded forward.
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
            // How far apart two Records' places are.
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

    /// Commit one outward consequence.
    ///
    /// "Commit", not "run". Each of these lands as a durable row — an
    /// obligation, a question, or a queued effect — and a separate worker
    /// carries it out afterwards. Two reasons, and both are load-bearing. A
    /// rule that shelled out mid-evaluation could change the world and then
    /// have its own transaction rolled back. And a rule that reached the
    /// network from inside the evaluation would have no place left to check a
    /// grant, because by then it has already happened.
    ///
    /// The number the condition carried travels with each one, so an outward
    /// consequence can be as computed as an inward one.
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
                // A promise the rule did not put a number on takes the one the
                // condition computed — the same fallback a capture makes.
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
                // Yes or no is what almost every asked question is, so an
                // unspecified list means that rather than an empty prompt.
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
            // The inward variants are Actions and were handled by the caller.
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

    /// The number the rule on this Record computes, gate ignored.
    ///
    /// Reading a rule's arithmetic is not the same as letting it act, so the
    /// gate is deliberately skipped and no consequence runs. Depth is capped
    /// because a cell that reads itself — directly or around a ring of three —
    /// is a mistake a person can make in one keystroke, and the honest answer
    /// is a refusal rather than a heartbeat that never returns.
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
        // Always/value: the raw number, before any decision about whether it
        // means act. Those two belong to the rule that *owns* the condition.
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

    /// The opening edge of the stretch one evaluation speaks for.
    ///
    /// A rule that runs daily answers for one day; a rule that runs monthly
    /// answers for one month. So the window is the gap back to the rule's own
    /// previous instant — which makes consecutive evaluations tile the timeline
    /// exactly. Nothing a rule reads over time can be counted twice, and a Cell
    /// that slept still sees every rhythm it missed, because the missed dates
    /// are applied in order and each one carries its own window.
    ///
    /// Before a rule's first instant there is nothing to have missed, so the
    /// window opens at the anchor.
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

    /// How many times the rule declared on `record_uid` came round in
    /// `(since, at]`.
    ///
    /// Half-open at the near edge and closed at the far one, so the instant a
    /// window ends on belongs to that window and to no other. A Record with no
    /// rule on it is worth zero rather than an error: "that rhythm did not
    /// happen" is a true answer, and it is the one that lets a condition be
    /// written before the schedule it will eventually watch.
    async fn rhythm_count(
        &self,
        record_uid: &str,
        since: DateTime<Utc>,
        at: DateTime<Utc>,
    ) -> Result<nucleus::DecimalValue, EngineError> {
        let tick = chrono::Duration::milliseconds(1);
        let mut total: i128 = 0;
        for rule in store::recurrence::for_record(&self.store.pool, record_uid).await? {
            // A paused rhythm is silent. Counting its dates would have a rule
            // keep acting on a schedule its author stopped.
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

    /// Commit a zero-delta annotation fact on `record_uid`. Metadata edits
    /// (text/slug/concept/unit/extension) are not quantity deltas, but a fact is
    /// still appended so the edit is Ledger-visible provenance and so live
    /// subscriptions invalidate and refresh (same pattern as `CreateRecord`).
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

/// Order-like link kinds (blueprint IV): `@precedes`, `@before`, `@order`, or
/// any descendant of those concepts. Only these get cycle warnings on save.
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

/// SCCs of the given link kind's graph (nucleus::graph::cycles over the
/// stored edges) — each returned Vec is one loop of record uids.
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

/// Read an opaque JSON payload a rule stored for an outward consequence.
///
/// Kept as text on the rule so the consequence type stays comparable and does
/// not drag a whole JSON document into every equality check. It is parsed at
/// the moment it is queued rather than when it runs, so a malformed payload is
/// a visible failure of the rule that wrote it and not a mystery in a worker
/// log hours later.
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

/// Bring a reading that is natively a float into the exact world.
///
/// Only for the readings that are *measurements* — a distance, a ratio, a
/// count of hours. Those are approximate at the source, and pretending
/// otherwise by carrying them as exact decimals from the start would dress a
/// GPS reading up as an accounting figure. Everything the Ledger owns —
/// levels, sums, captured amounts — never passes through here.
fn inexact(value: f64) -> Result<nucleus::DecimalValue, EngineError> {
    nucleus::DecimalValue::from_f64_lossy(value).map_err(|_| EngineError::Conflict {
        code: "rule_condition_unreadable",
        message: "that reading is not a number a rule can use".into(),
    })
}
