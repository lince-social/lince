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
    /// `SetQuantity` without the float. `amount` is exact decimal TEXT
    /// (`"12"`, `"3.50"`), parsed straight to a decimal so a level written by
    /// a person — in a `.lingua` file, say — never passes through an f64 on
    /// its way to a signed Fact.
    ///
    /// Still a FOLD, not an assignment: it appends the exact difference
    /// between what was asked for and what the Ledger currently holds. The
    /// number becomes true, and it becomes true the honest way, with a Fact
    /// saying who moved it and by how much.
    SetQuantityExact {
        target: String,
        amount: String,
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
    /// Rename a contact to what the LOCAL user calls them.
    ///
    /// Separate from `edit-record-text` because a contact's Organ record is
    /// filed under THEIR uid: editing it the ordinary way logs a CRDT op that
    /// is pushed back to them and to every other contact, publishing the
    /// private label this Cell chose for someone. This writes it locally and
    /// logs nothing (see `store::organs::rename_contact`).
    RenameOrganContact {
        target: String,
        name: String,
    },
    /// Which directions of the general feed are open with one contact.
    ///
    /// `out` is what this Cell pushes them; `in` is what it accepts from them.
    /// Both are already enforced — outbound in the outbox drain, inbound at
    /// the delivery boundary — so this is the switch, not a preference.
    /// Individually granted conversations are a narrower permission and keep
    /// flowing either way.
    SetSyncPolicy {
        target: String,
        sync_out: bool,
        sync_in: bool,
    },
    /// WHICH columns of the Records a contact can already see actually travel
    /// to them (Ontology §12).
    ///
    /// `sync_out` decides whether the feed is open at all; this decides how
    /// wide it is. `fields` is `None` for unnarrowed and `Some(list)` for a
    /// scope — including `Some([])`, which is a real answer meaning nothing
    /// but the identifying columns, NOT the same as `None`. Serialising these
    /// two the same way is the one mistake here that leaks rather than
    /// annoys, so the wire keeps them distinct.
    ///
    /// Narrowing takes effect on the next serve. WIDENING does not reach back:
    /// the ops for a newly-added column are already below the contact's
    /// version vector, so they travel from now on and existing Records need a
    /// re-snapshot that does not exist yet. Surfaces must say so.
    /// `fields` is deliberately NOT `#[serde(default)]`: an omitted field
    /// would deserialise to `None`, and `None` is the widest setting there
    /// is. A caller that forgets to send it should get an error, not silently
    /// unnarrow someone.
    SetContactScope {
        target: String,
        fields: Option<Vec<String>>,
    },
    /// WHICH Records travel to a contact, named by a saved Protein query.
    ///
    /// The sibling of `SetContactScope` on the other axis: that one narrows
    /// the COLUMNS of everything shared, this one narrows WHICH Records are
    /// shared at all. `None` clears the selection back to the unnarrowed feed.
    SetContactShare {
        target: String,
        protein: Option<serde_json::Value>,
    },
    /// Hand a Record over to a contact: they become its holder, and once the
    /// handover has actually been delivered it stops being ours.
    MoveRecordTo {
        record: String,
        target: String,
    },
    /// Call off a move that has not been handed over yet.
    CancelRecordMove {
        record: String,
    },
    /// WHICH columns we accept FROM a contact (Ontology §12).
    ///
    /// The other half of the pairing, and deliberately its own action rather
    /// than a direction flag on the one above: outbound narrowing is a
    /// privacy control, this is an integrity one. They have different reasons
    /// to be narrow and no reason to agree — a contact we tell everything is
    /// routinely one we accept little from — and one setting with two ends
    /// would invite keeping them equal.
    ///
    /// Out-of-scope ops are dropped at import, silently and permanently:
    /// unlike the outbound side there is no cursor to re-open, because we
    /// cannot ask a peer to re-send what we chose not to take. Widening
    /// therefore applies to what arrives next and to nothing already past.
    SetContactAcceptScope {
        target: String,
        fields: Option<Vec<String>>,
    },
    /// End a conversation on THIS Cell (Ontology §11, C6).
    ///
    /// Local removal plus revocation, and both halves are needed: revoking
    /// alone leaves it sitting in the list, removing alone leaves their ops
    /// still welcome so it repopulates on the next sync.
    ///
    /// It emits NO tombstones. That is the honest limit rather than a
    /// shortcut: a tombstone is a synced op, so deleting these Records the
    /// ordinary way would delete THEIR copy too, and nobody agreed to that.
    /// Their copy is theirs. What remains is exactly one thing — they may send
    /// an invite to open a new conversation, one pending at a time, which is a
    /// knock rather than a channel.
    DeleteConversation {
        conversation: String,
    },
    /// Put a COPY of a Record into a conversation (Ontology §11, C6).
    ///
    /// The deliberate opposite of a reference, and a separate verb because it
    /// is a separate decision. A reference is a pointer read live, and can be
    /// taken back by hiding the Record; a copy LEAVES this Cell, lands in the
    /// other party's store, and cannot be recalled — they run their own code
    /// and promised nothing.
    ///
    /// It is the right tool for a document two people are editing, where "the
    /// owner went offline so there is nothing to show" is the wrong answer,
    /// and the wrong default for everything else — which is why it is not the
    /// default for anything. Surfaces must present the choice AT the moment of
    /// copying, in its own wording, because that is the only moment at which
    /// it can still be declined.
    SendRecordCopy {
        thread: String,
        record: String,
    },
    /// Keep one whole Record out of one contact's feed (Ontology §12).
    ///
    /// The other half of hiding: the scope above says which COLUMNS a contact
    /// receives, this says which ROWS. Per-contact and per-record, so the same
    /// Record can be shared with one person and withheld from another without
    /// anything on the write path knowing — the filter runs where the feed is
    /// served, like everything else in this cluster.
    ///
    /// Honest about its limit, and surfaces must be too: this stops what
    /// travels NEXT. A contact who already received the Record keeps it, and
    /// no delete is sent to make them drop it — sending one would confirm the
    /// Record exists, which is most of what hiding was for.
    HideRecordFromContact {
        target: String,
        record: String,
        hidden: bool,
    },
    /// Drop a contact and the Record standing in for it — locally, and only
    /// locally. `delete-record` on the same uid would log a tombstone against
    /// THEIR Organ record and push it to them and everyone else; this forgets
    /// them here and tells nobody (see `store::organs::forget_contact`).
    ForgetOrganContact {
        target: String,
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
    /// What this Cell knows about its own identity right now: who is waiting
    /// at the front door, and which devices are running a different build.
    ///
    /// A READ shaped as an Action because a sand's only channels are Protein
    /// subscriptions and Actions, and neither the door queue nor the stale
    /// list is a Record — the first is deliberately local-only (a front door
    /// must not write into the identity) and the second is transient. Without
    /// it both are invisible, which for a queue of people waiting to reach you
    /// is the same as broken.
    RosterStatus,
    /// What this Cell carries for others, and what is waiting for it
    /// elsewhere (Ontology C4, the blind mailbox).
    ///
    /// Read-only, and it reports the two halves separately because they are
    /// different roles: an Institute VPS is usually a carrier, a phone is
    /// usually a recipient, and one Cell can be both without the panel
    /// conflating them.
    MailboxStatus,
    /// Start carrying mail for an Organ.
    ///
    /// Volunteering is PER CONTACT, when they ask — not a switch that makes
    /// you everyone's mailbox. The storage being donated stays legible because
    /// the operator named who it is for, one at a time.
    MailboxCarryFor {
        organ_uid: String,
        #[serde(default)]
        label: String,
        /// Bytes. Zero means the default.
        #[serde(default)]
        quota_bytes: i64,
    },
    /// Stop carrying for an Organ. Their held mail goes with the
    /// registration — a mailbox that keeps mail for someone it no longer
    /// serves is storing what nobody will collect.
    MailboxStopCarrying {
        organ_uid: String,
    },
    /// Where THIS Organ's mail may be left, and whether those boxes still say
    /// they are carrying it (Ontology C4).
    ///
    /// Separate from `MailboxStatus` because they are opposite roles and one
    /// panel showing both without saying which is which is how an operator
    /// ends up believing their own mail is safe because somebody else's is.
    MailboxPickupPoints,
    /// Publish a pickup point: name a contact whose Cell holds our mail when
    /// our own Cells cannot be reached.
    ///
    /// Probes before publishing. A box that never agreed to carry for us is a
    /// hole every sender falls into silently, and the roster is signed — a
    /// wrong entry costs a re-publish and a version.
    MailboxAddPickup {
        organ_uid: String,
        /// The carrier Cell to dial. Empty means "their front door", resolved
        /// from the contact row.
        #[serde(default)]
        node_id: String,
        #[serde(default)]
        label: String,
    },
    /// Stop publishing a pickup point. Senders stop using it as their rosters
    /// refresh, and anything still waiting there stops being collected — so
    /// this reports what is in the box at the moment it is dropped.
    MailboxRemovePickup {
        organ_uid: String,
    },
    /// Collect now, rather than at the next sync pass.
    ///
    /// The pass already does it; this exists because "is my mail arriving"
    /// is a question people ask at the moment they are looking at the panel,
    /// and an answer that comes minutes later answers nothing.
    MailboxCollectNow,
    /// Who we cannot reach right now, how long that has been true, and what
    /// has been done about it (Ontology C4, the retry window).
    ///
    /// Its own listing rather than a column on the contact list, because the
    /// question it answers is not "who do I know" but "is anything stuck" —
    /// and the honest answer to that includes contacts whose mail CANNOT be
    /// left anywhere, which a contact row has no room to explain.
    MailboxOutbound,
    /// What the last File Sync pass over this Organ's folder refused to act
    /// on, and why. Read-only, and in-memory rather than stored — see
    /// `Engine::file_sync_conflicts`.
    FileSyncStatus {
        organ: String,
    },
    /// The operator's inbox: who has asked to be carried here, and which
    /// invite codes are outstanding (Ontology C4).
    MailboxRequests,
    /// Answer one ask. Accepting registers them on the terms given; declining
    /// removes the row and tells them nothing — a decline that notified would
    /// make refusing socially expensive, which is how people end up saying yes.
    MailboxAnswerRequest {
        organ_uid: String,
        accept: bool,
        /// Bytes. Zero means the default. Ignored when declining.
        #[serde(default)]
        quota_bytes: i64,
    },
    /// Issue a single-use code that lets its holder register themselves. The
    /// plaintext comes back once, here, and is never stored.
    MailboxIssueInvite {
        #[serde(default)]
        label: String,
        #[serde(default)]
        quota_bytes: i64,
    },
    /// Ask a contact to carry our mail. The other half of `MailboxCarryFor`,
    /// and the direction that was missing: carrying is a favour, and a favour
    /// starts with the asking.
    MailboxAskCarry {
        organ_uid: String,
    },
    /// Spend a mailbox invite somebody sent us.
    MailboxUseInvite {
        code: String,
    },
    /// Stop waiting for one contact: try them now, and leave their mail with
    /// a carrier if they still do not answer.
    ///
    /// The window is a default, not a verdict. Somebody who knows the other
    /// person is away for a week should not have to wait it out, and somebody
    /// watching a batch not arrive wants to do something rather than read
    /// about a timer.
    MailboxMailNow {
        organ_uid: String,
    },
    /// Write a LOCAL-ONLY config namespace on this Cell's own Record
    /// (Ontology §11, C4).
    ///
    /// Separate from `SetExtension` because it logs no op and never syncs.
    /// That is what makes it usable by a relay Cell, which may not write —
    /// and discovery settings are per-DEVICE anyway, so putting them on the
    /// shared Organ Record was always the wrong shape.
    SetCellConfig {
        namespace: String,
        fds: serde_json::Value,
    },
    /// Compare logs with a contact and report what disagrees, moving no ops
    /// (Ontology §11, C2b — the cross-Organ audit).
    ///
    /// REPORTS rather than repairs, deliberately. A disagreement between two
    /// Organs is not obviously anyone's bug — a peer legitimately prunes, an
    /// outbox legitimately has not drained — so silently re-sending would hide
    /// the one case worth seeing: two logs that never converge however many
    /// passes run. That is a person's call, so a person is told.
    AuditContact {
        contact: String,
    },
    /// Join an existing Organ as a new device, from a code shown by a Cell
    /// that already belongs to it (Ontology §11, C3).
    ///
    /// The counterpart of `RosterEnrolToken`, and the reason this is an action
    /// rather than a wire detail: without it the enrolment client is reachable
    /// only from a test, which is not a feature anyone can use.
    RosterJoinOrgan {
        code: String,
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
    /// Put Lince's own documentation into this store, as Records.
    ///
    /// The Instinct sand reads the same embedded bundle it imports, so what
    /// you read and what you get are the same thing. Idempotent: a Record
    /// whose uid is already here is left exactly as it is, because the point
    /// of importing is to be able to EDIT them afterwards and a second import
    /// must never undo that.
    ImportInstinct,
    /// Create an Agent — an Actor that is not a Person.
    ///
    /// An Actor is anything that can hold work: `actor` is a Concept with
    /// `person` and `agent` beneath it, which is all the distinction needs,
    /// because the Concept DAG already answers "is this an Actor" for both.
    /// Renaming the Person type itself was considered and dropped: standing,
    /// the four login doors and dormant absorption are written about people,
    /// and widening the word would blur every one of them.
    ///
    /// `operated_by` is the Person answerable for it. The Organ half of "whose
    /// agent is this" needs nothing new — every Record already carries the
    /// Organ it originated at.
    CreateAgent {
        head: String,
        #[serde(default)]
        operated_by: Option<String>,
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
    /// C7 axis 2: does THIS Cell execute this Program.
    ///
    /// Deliberately unlike its neighbours in two ways. There is no
    /// `request_id`, because idempotency exists to stop a retried mutation
    /// minting a second revision and this mutation has no history to duplicate
    /// — setting a switch twice leaves it where it was. And there is no
    /// `expected_handle_revision`, because this changes nothing about the
    /// Program: revising the rule on another Cell must not invalidate a
    /// pending "do not run this one here".
    SetKarmaExecution {
        program_uid: String,
        executes: bool,
        #[serde(default)]
        note: Option<String>,
    },
    /// C7: name the one Cell that runs this Program, or clear the designation.
    ///
    /// `cell_uid: None` clears it and returns the Program to running wherever
    /// it is held. Unlike its neighbour above this one SYNCS — "which Cell is
    /// the one" is a fact every Cell needs, where "do I run it" is each
    /// machine's own business.
    DesignateKarmaExecutor {
        program_uid: String,
        #[serde(default)]
        cell_uid: Option<String>,
    },
    /// C7: name the one Cell that retries this Transfer's deliveries.
    ///
    /// The same designation as the Karma one — `store::executor`, the same
    /// namespace, the same last-writer-wins value — read off a Transfer Record
    /// instead of a Program. Kept as a separate variant rather than one generic
    /// `DesignateExecutor { record_uid }` because the permission differs: this
    /// one belongs to whoever may configure the Transfer's delivery, and a
    /// single variant taking any uid would have to pick one permission for
    /// every kind of Record there will ever be.
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
        /// The target Person's uid (same convention as `actor`).
        user: String,
        role: String,
    },
    /// Stop a Person acting here — they left, or simply stopped using Lince.
    ///
    /// Reversible by `SetPersonStanding { active: true }`, and NOT a delete:
    /// their Record, their Facts and everything naming them stay exactly as
    /// they are, because none of it stopped being true. Unlike the four
    /// variants above this one DOES write to the Ledger's world — standing
    /// lives on the Person Record as an extension, so it syncs to this Organ's
    /// other Cells, which is the difference between deactivating someone and
    /// deactivating them on one laptop.
    SetPersonStanding {
        /// The target Person's uid or slug.
        person: String,
        active: bool,
        /// The owner's own note ("moved out, July"). Never shown to the person
        /// refused: a login refusal says the same words whatever the cause.
        #[serde(default)]
        note: Option<String>,
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
    /// Structured result for the surface, when `created` (one uid) is not
    /// enough to render what happened.
    ///
    /// Exists because a feature is not done until a human can use it, and some
    /// results are not a uid: an enrolment code plus the QR that carries it,
    /// what a front door is holding, which of your devices is out of date. The
    /// alternative was mirroring those into a synced extension, which is what
    /// the pairing code does — and that is exactly wrong for anything secret,
    /// since an extension on the Organ Record TRAVELS.
    pub data: Option<serde_json::Value>,
}

/// Which Karma definition a mutation just changed.
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
        if let Some(permission) = Self::generic_write_permission(&action) {
            self.require_permission_lenient(actor.as_deref(), permission)
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
            Action::SetQuantityExact { target, amount } => {
                let uid = self.resolve(&target).await?;
                self.reject_direct_transfer_record_mutation(&uid).await?;
                // Exact from the text: a level someone wrote in a file is
                // parsed straight to a decimal, so `3.50` never becomes a
                // float on its way to a signed Fact.
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
                // A destination tag wins if configuration accidentally lists it
                // on both sides; otherwise this transition would retract state
                // it has just established.
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
                // The ROOT, resolved from whatever inside it was named — a
                // person deleting a conversation may well have a thread
                // selected, and deleting only the thread would leave the
                // conversation half-present and still syncing.
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
                // A Record already inside a root is not copyable into another
                // one: that is the cross-root widening `assert` refuses, and
                // going through a copy would be the same disclosure wearing a
                // different verb.
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
                // A NEW uid, deliberately. Reusing the source uid would make
                // the copy and the original the same Record to every later
                // merge, so an edit either side would flow back through the
                // grant channel — which is a shared document, not a copy, and
                // not what was agreed to.
                //
                // Slug is dropped for the same reason: it is a local
                // suggestion and a copy carrying it would collide with the
                // original on our own Cell.
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
                // Resolved, not taken on trust: a uid that names nothing would
                // store a rule that hides no Record while reading as applied,
                // and the surface accepts a slug because that is what a person
                // actually knows a Record by.
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
                // UNHIDING is a grant, and a grant has to reach back or it
                // grants nothing: the ops this contact missed are already
                // below their version vector, so ordinary catch-up will never
                // offer them again and the Record would stay permanently
                // absent while reading as shared.
                //
                // Replayed by identity rather than re-snapshotted — see
                // `sync_ops::enqueue_record_for_contact` for why that is the
                // cheaper AND the safer of the two.
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
                // Read BEFORE the write, because the repair below depends on
                // which direction the change went and the stored value is
                // about to stop saying.
                let before = contact.scope_fields;
                // A scope names COLUMNS, and a column name that matches
                // nothing narrows to nothing while looking configured. Empty
                // and blank entries are the common way that happens (a
                // trailing comma in a surface's text field), so they are
                // refused rather than stored.
                validate_scope(fields.as_deref())?;
                store::organs::set_contact_scope(&self.store.pool, &uid, fields.as_deref()).await?;
                // A WIDENING has to reach back or it widens nothing: every op
                // for a newly-named column is already below the contact's
                // version vector, so catch-up will never offer it again and
                // the column would stay permanently blank for them while the
                // panel reads as shared. Same replay-by-identity as a
                // re-grant, over the whole feed rather than one Record.
                //
                // Narrowing needs no counterpart. It stops sending; it does
                // not reach back and retract, and there is nothing to repair.
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
                // Evaluated BEFORE it is stored. A selection nobody can read
                // narrows to nothing while the panel reads as configured,
                // which is the silent-stop-sharing failure rather than an
                // error anyone would notice.
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
                // Refused rather than re-targeted. Two handovers in flight for
                // one Record is a race whose loser has already been told they
                // own it; there is no answer at this layer that is not a
                // guess, so the second one is a mistake to report.
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
                // Handing something over means sending it, whatever the
                // selection says: a Record they are about to own cannot be
                // filtered out of its own handover.
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
                // Do we already reach someone at this NodeId? Then the code is
                // for a person we have already MET — found on the network, or
                // paired earlier — and pasting it is a promotion, not a first
                // contact. Minting a second row here is not merely untidy: the
                // NodeId binding is UNIQUE, so it fails outright, which is
                // exactly the "I pasted their code and got an error" report.
                let existing =
                    store::organs::contact_by_node_id(&self.store.pool, &invite.node_id).await?;
                if let Some(contact) = &existing {
                    // `blocked` is terminal everywhere else; a pasted code must
                    // not be the one door that launders it back to `known`.
                    if contact.trust == "blocked" {
                        return Err(EngineError::Consequence(
                            "this Organ is blocked. Unblock them first if that is what you \
                             meant — adding by code must not undo a block."
                                .into(),
                        ));
                    }
                }
                // The uid is theirs to declare, and a code cannot declare it —
                // only an Introduction over a real connection can. So a row for
                // someone NOT yet met is held under a uid derived from the
                // NodeId and FLAGGED: the next sync pass dials them, learns the
                // real uid, and replaces this row with it. Until that happens
                // they cannot sync, and the flag is what stops that from being
                // a silent dead end.
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
                    // They already have a name here — from discovery, which
                    // took it from their own claim. What the user just typed is
                    // deliberate and local, so it wins.
                    store::organs::rename_contact(&self.store.pool, &organ_uid, name).await?;
                }
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
                // Only a row we just invented owes an Introduction. Someone we
                // have already met introduced themselves when we met them, and
                // re-flagging them would send a settled contact back through
                // reconciliation for nothing.
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
                // Same zero-delta wake-up `create-message` commits, and for
                // the same reason: the three levels of a conversation are
                // written straight through `store::records`, which drops no
                // Fact, and a live subscription re-runs on nothing else. The
                // Fact rides the CONTACT, because "do I already have a
                // conversation with them" is a question asked of their row.
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
                let token = self.issue_enrolment_token().await?;
                // The TOKEN ALONE IS UNUSABLE. A new device needs to know
                // where to send it, whose identity it is joining, and which
                // root key to expect back — so what the owner is shown is the
                // whole enrolment code, not the secret out of context.
                //
                // The reachable parts are taken from the pairing code this
                // Cell already mirrors (NodeId and current addresses), because
                // that is the one place they are already assembled. What must
                // NOT happen is the reverse: an enrolment code carries a live
                // secret and can never be mirrored into an extension, since
                // extensions on the Organ Record travel to every contact.
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
                        // No endpoint bound yet, so there is no address to put
                        // in a code. Say that rather than handing back a token
                        // that cannot be used.
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
                // Swept on read. A carrier that only expired mail when
                // somebody happened to run a timer would quietly hold it past
                // the retention it promised, and the panel is exactly the
                // moment the number had better be true.
                let swept = self.sweep_mailbox().await?;
                let mut carrying = Vec::new();
                for registration in store::mailbox::registrations(&self.store.pool).await? {
                    let held =
                        store::mailbox::carried_for(&self.store.pool, &registration.organ_uid)
                            .await?;
                    // The contact's own name when we have one, so an operator
                    // reads a person rather than a uid. Falls back to the
                    // label they typed, then to the uid — never to nothing.
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
                    // Senders who need telling that their mail expired
                    // uncollected. Surfaced now even though the DELIVERY of
                    // that notice is a later box, because a queue nobody can
                    // see is how a promise quietly stops being kept.
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
                // The root key is what registration is FOR: it is how a
                // recipient proves, later and from a device that may not exist
                // yet, that the mail is theirs. Without one there is nothing
                // to check a presented roster against, so this refuses rather
                // than registering something uncollectable.
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
                // Reported, not hidden: the operator has just discarded mail
                // somebody was expecting, and the number is the honest cost of
                // the decision they made.
                outcome.data = Some(serde_json::json!({
                    "organ_uid": organ_uid,
                    "discarded_bundles": held.bundles,
                }));
            }
            Action::MailboxPickupPoints => {
                let points = self.own_pickup_points().await?;
                let mut published = Vec::new();
                for point in &points {
                    // Re-asked every time the panel opens, because the carrier
                    // can stop carrying at any moment and nothing tells us: the
                    // registration lives on THEIR disk. A published point that
                    // has quietly stopped answering is the failure this panel
                    // exists to make visible.
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
                // Who could be asked. Every known contact, because whether
                // they carry for us is not something we hold — only they do,
                // and the probe is what answers it.
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
                    // The root is what signs a roster, so this is exactly the
                    // set of Cells that can change these at all.
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
                    // A real answer, and the answer is no. It is deliberately
                    // not said WHY — a carrier gives one wording for every
                    // collection failure — so this says what we know and what
                    // we do not.
                    crate::wire::CarrierProbe::Refused => {
                        return Err(EngineError::Consequence(
                            "they are not carrying mail for you. They have to add you first, \
                             and asking them from here is not built yet — nothing was \
                             published."
                                .into(),
                        ));
                    }
                    // NOT a refusal. A closed laptop is the ordinary state of
                    // most machines, and publishing on silence would be the
                    // same error as falling back to a mailbox on the first
                    // failed dial.
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
                    // Two is advice, not a rule — refusing the first would make
                    // the second unreachable. The panel says what one costs.
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
                // Asked BEFORE it is dropped: once it is out of the roster
                // nothing collects from it, so anything sitting there is
                // stranded until it expires. The number is the honest cost.
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
                    // Their own label is shown BESIDE the uid, never instead
                    // of it: a display name from a peer is an untrusted claim,
                    // and this panel is where somebody decides to hold another
                    // person's correspondence.
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
                    // The root key comes from the ask, not from a fresh
                    // lookup: it is what they presented and what was checked
                    // when the ask was taken, and re-deriving it days later
                    // would quietly accept a different key than the one the
                    // operator is looking at.
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
                // Only after the registration stuck. Accepting and failing to
                // register must not also consume the ask, or the request is
                // gone and nothing carries.
                store::mailbox::answer_request(pool, &organ_uid).await?;
                outcome.data = Some(serde_json::json!({
                    "organ_uid": organ_uid,
                    "accepted": accept,
                }));
            }
            Action::MailboxIssueInvite { label, quota_bytes } => {
                let token = self.issue_mailbox_invite(&label, quota_bytes).await?;
                // The code needs somewhere to point. A Cell with no endpoint
                // can issue nothing usable, and saying so beats handing over a
                // string that fails silently on the other person's machine.
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
                    // Said in these words on purpose: a wire round trip proves
                    // the ask arrived and nothing else. Their operator decides.
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
                    // "nothing wrong" and "nothing known yet" look identical
                    // in an empty list, and only one of them is an all-clear.
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
                    // Whether mail is even possible for them. Three states,
                    // not two: a contact who published no box is not "not
                    // mailed yet", they are unmailable until they choose
                    // somebody — and only they can.
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
                // What a carrier reported dead. Believed already — these are
                // rows this Cell wrote when it made the deposit — so the panel
                // states them plainly rather than hedging.
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
                // Whether senders can still write to THIS device. An enrolled
                // Cell rotates its own mail key but cannot publish one, so
                // this is the state that used to be invisible: mail keeps
                // working for the Organ, and stops working for the device.
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
                // Move the clock, then run the ORDINARY pass. Not a private
                // path to the carrier: if they are in fact reachable this
                // second, the pass reaches them and no mail is left at all,
                // which is the outcome the button's owner actually wants.
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
                        // The NodeId is what iroh authenticated at the door.
                        // Anything inside `intro` is a CLAIM, and the surface
                        // has to keep saying so.
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
                // This Cell's OWN standing, so a surface can say "this device
                // carries traffic and authors nothing" instead of letting a
                // person meet that fact as a failed write.
                let this_cell = store::cells::local(&self.store.pool).await?;
                let held = match store::organs::local(&self.store.pool).await? {
                    Some(organ) => self.roster_of(&organ.uid).await?,
                    None => None,
                };
                // Whether a roster EXISTS is a different question from what it
                // grants, and conflating them is a bug: a relay has a roster
                // and no capabilities, so deriving one from the other would
                // make the relay state unreportable — exactly the case this is
                // for.
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
                    // No roster yet means this Cell IS the whole Organ, which
                    // is a different state from "listed with nothing".
                    "has_roster": has_roster,
                }));
            }
            Action::SetCellConfig { namespace, fds } => {
                store::cells::set_config(&self.store.pool, &namespace, &fds).await?;
                // A raw write drops no Fact, so nothing on the bus would say
                // this happened — and the endpoint rebinds on a discovery
                // change. Without this announcement, saving a discovery
                // setting would appear to work and take effect only at the
                // next reboot.
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
                    // Unreachable is NOT a disagreement, and saying so is the
                    // whole difference between a useful audit and an alarming
                    // one. A contact with a closed laptop is the normal case.
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
            Action::ImportInstinct => {
                // The vocabulary FIRST. A file must never invent a meaning, so
                // without these every record in the bundle refuses — which is
                // the rule working, and also a useless import. The list is
                // fixed in `instinct::VOCABULARY` rather than read off the
                // files, so importing a bundle can never introduce a Concept
                // nobody chose.
                for name in crate::instinct::VOCABULARY {
                    store::concepts::ensure(&self.store.pool, name).await?;
                }
                let bundle = crate::instinct::records();
                // Two phases, for the reason the file-sync path found: the
                // records cross-link by uid, and asserting a link needs its
                // object to exist. Everything is created, then everything is
                // described.
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
                // Resolve the operator BEFORE creating anything: a named
                // Person who is not one leaves an unowned Agent behind, and
                // an Agent nobody is answerable for is the thing this field
                // exists to prevent.
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
                // No Fact and no op. This is a machine's own setting, so it has
                // nothing to say to the Ledger and nothing to send to a peer —
                // recording it as either would make one Cell's arrangement look
                // like a change to the shared rule.
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
                // The cost of a designation is stated when it is made. If the
                // named Cell is off, the rule does not run — a visible silence,
                // which is the trade taken deliberately over a heartbeat lease
                // that would hand execution to whichever Cell merely cannot see
                // the holder.
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
                // Same permission as configuring a recipient — both decide how
                // this Transfer reaches the other side.
                self.require_transfer_editor(&transfer, actor.as_deref())
                    .await?;
                store::executor::designate(&self.store.pool, &transfer, cell_uid.as_deref())
                    .await?;
                // No Fact. Which of MY Cells does the retrying is an arrangement
                // between machines I own; the Ledger records what was agreed
                // with the other side, and this changes none of it.
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
                // A user IS a Person. Creating one creates their record through
                // the ordinary path — so it has op-log history and syncs like
                // any other — and then gives it a way to log in here.
                let person_uid = store::auth::create_person_login(
                    &self.store.pool,
                    &name,
                    &username,
                    &password_hash,
                    role_id,
                )
                .await?;
                // Same creation fact `CreateRecord` drops, for the same
                // reason: without one this Person commits no fact and stays
                // invisible to every subscribed sand until something else
                // touches it.
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
                // Standing means "may this human act here", so it only makes
                // sense over a Person. Written onto a Transfer or a Cell it
                // would be a field nothing reads — an owner believing they had
                // turned something off when they had not.
                if record.kind != "person" {
                    return Err(EngineError::Consequence(format!(
                        "`{person}` is a {} record, not a Person",
                        record.kind
                    )));
                }
                // Deactivating yourself is refused rather than confirmed. It is
                // the one move that can leave an Organ with nobody able to undo
                // it — the permission to reactivate is held by the account you
                // just closed — and it is never what someone means to do from
                // an admin panel listing everybody. Leaving is done by someone
                // else turning you off, which is also what makes it reversible.
                if actor.as_deref() == Some(person_uid.as_str()) && !active {
                    return Err(EngineError::Consequence(
                        "you cannot deactivate yourself — ask another admin".into(),
                    ));
                }
                // The other half of the lockout guard. Refusing self-
                // deactivation alone does not save an Organ: an admin can turn
                // off every OTHER admin one at a time and then be turned off by
                // one of them, and `user:update` is not itself the admin role —
                // someone holding only it can close every admin account without
                // ever touching their own. So the last ACTIVE admin stays.
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
        store::auth::user_by_uid(&self.store.pool, actor)
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

    /// The blanket table's gate (2026-08-07) — deliberately more forgiving
    /// than `require_permission`. `actor: Option<String>` is overloaded
    /// across this file: for a web-originated action it is always a numeric
    /// `app_user` id (`authenticate_headers` in `web::lib` mints exactly
    /// that), but plenty of legitimate internal call sites — Karma acting on
    /// behalf of a Person, replay/test fixtures — pass a non-numeric actor
    /// (a Person uid, an organ id) purely for Ledger attribution, with no
    /// permission-bearing session behind it at all. `require_permission`
    /// treats any such actor as `Forbidden("unrecognized actor")`, which is
    /// exactly right for the actions it already gated (only ever invoked
    /// from an authenticated web session in practice) but wrong here: the
    /// blanket table now covers ~100 more variants, several of which ARE
    /// legitimately called with a Person-uid actor. Only a `Some` actor that
    /// resolves to a REAL `app_user` row is checked against `permission`;
    /// anything else (including `None`) is unrestricted, matching this
    /// action's behavior before blanket enforcement existed.
    async fn require_permission_lenient(
        &self,
        actor: Option<&str>,
        permission: &str,
    ) -> Result<(), EngineError> {
        let Some(actor) = actor else {
            return Ok(());
        };
        // A Person with no credential holds no local role — a contact, or a
        // remote Organ's granted login. Lenient means lenient: they pass here
        // and are gated by visibility instead.
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

    /// The Person an actor acts as.
    ///
    /// Now an identity function by construction: there is one human reference,
    /// so an authenticated actor IS a Person uid. This used to resolve an app
    /// user through `app_user_person` and could fail with "no assigned person
    /// identity" — a state that can no longer be represented. Local no-auth
    /// mode still returns `None`, which every caller reads as "trusted local".
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
            // Designating a deliverer is the ORIGIN's call: it names which of
            // the origin's own Cells retries, and a recipient has no Cells in
            // that answer.
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

    /// Blanket write enforcement (2026-08-07): every `Action` variant not
    /// already covered by a bespoke gate (`DeleteRecord`'s ownership-aware
    /// `check_delete_permission`, `CreateTransfer`'s `transfer:create`, the
    /// 17 transfer-lifecycle arms already gated on `transfer:update` inline,
    /// and the five role/user/permission-admin actions) is checked here
    /// against `utils::auth::ALL_PERMISSIONS` — a catalog that already
    /// declared `record:update`, `transfer:read`, `karma:create`, etc. and
    /// already lets every role toggle them in the permissions sand; nothing
    /// here invents a new permission string. `actor == None` (a local,
    /// no-auth Cell) is unrestricted, same as every other gate in this file.
    fn generic_write_permission(action: &Action) -> Option<&'static str> {
        Some(match action {
            // Record core
            Action::CreateRecord { .. }
            | Action::CreateAgent { .. }
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
            // Creating a Record inside a conversation, which is what a copy
            // IS. Not a `record:update` on the source: the original is not
            // touched, and permissioning it that way would let someone who
            // may only READ a Record be unable to pass it on, while someone
            // who may edit it could — which is backwards.
            | Action::SendRecordCopy { .. }
            | Action::CreateTransferThread { .. }
            | Action::CreateTransferMessage { .. }
            | Action::CreateConcept { .. }
            | Action::CreateLingua { .. }
            | Action::CreateSignal { .. }
            | Action::CreateMatchRule { .. } => "record:create",
            Action::DeleteConcept { .. }
            | Action::DeleteLingua { .. }
            // Ending a conversation removes Records, so it is a delete — even
            // though it logs nothing and reaches nobody. Permissioning it
            // lower because it is local would let someone who may not delete
            // a Record delete a whole conversation of them.
            | Action::DeleteConversation { .. } => "record:delete",

            // Frequency/Recurrence (Frequency's own catalog subject)
            Action::CreateFrequency { .. } | Action::CreateRecurrence { .. } => {
                "frequency:create"
            }
            Action::ReviseRecurrence { .. }
            | Action::SetRecurrencePaused { .. }
            | Action::ApplyRecurrenceOccurrence { .. }
            | Action::SkipRecurrenceOccurrence { .. }
            | Action::UnskipRecurrenceOccurrence { .. } => "frequency:update",
            Action::DeleteFrequency { .. } | Action::DeleteRecurrence { .. } => {
                "frequency:delete"
            }

            // Organ (pairing/contact management, not the sync wire itself)
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
            // A READ gated as an update, and that is a compromise rather than
            // a design: the organ permissions are create/update/delete with no
            // read tier, so there is nothing narrower to ask for. The cost is
            // real — someone allowed to look at this Organ but not change it
            // cannot see who is waiting at their own front door, and the panel
            // renders empty for them, which is the empty-state failure the
            // surface rule exists to prevent. Fix it by adding `organ:read`
            // when the permission set next moves, not by widening this.
            | Action::SetCellConfig { .. }
            | Action::AuditContact { .. }
            | Action::RosterStatus
            // Carrying mail is an ORGAN-scoped decision: it commits this
            // Cell's disk and uptime on behalf of the identity, and the
            // registration list is the carrier's most sensitive record.
            | Action::MailboxStatus
            | Action::MailboxCarryFor { .. }
            // Publishing where your own mail may be left is organ-scoped for
            // the stronger reason: it re-signs the roster every contact holds,
            // and it decides who gets to hold your unread mail.
            | Action::MailboxPickupPoints
            | Action::MailboxAddPickup { .. }
            | Action::MailboxRemovePickup { .. }
            | Action::MailboxCollectNow
            | Action::MailboxOutbound
            | Action::FileSyncStatus { .. }
            | Action::MailboxMailNow { .. }
            | Action::MailboxRequests
            | Action::MailboxAnswerRequest { .. }
            | Action::MailboxIssueInvite { .. }
            | Action::MailboxAskCarry { .. }
            | Action::MailboxUseInvite { .. }
            // Joining REPLACES this Cell's identity, which is the largest
            // organ-scoped change there is — but it is still an organ-scoped
            // change, and the real gate is the enrolment token itself.
            | Action::RosterJoinOrgan { .. } => "organ:update",
            Action::ForgetOrganContact { .. }
            | Action::RosterRevokeCell { .. }
            // Deleting: it discards mail somebody is expecting to collect.
            | Action::MailboxStopCarrying { .. } => "organ:delete",

            // Transfer (the remainder not already gated inline above)
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

            // Karma
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

            // Already bespoke-gated inline (transfer:create/update, or
            // ownership-aware) or admin-only (auth actions): no generic
            // check here, would only duplicate the existing one.
            Action::DeleteRecord { .. }
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
            // Gated on `user:update` in its own arm, not by the generic
            // record-write permission: it writes to a Person Record, and
            // `record:update` is held by people who may edit a name and must
            // not be able to close an account.
            | Action::SetPersonStanding { .. }
            | Action::GrantPermission { .. }
            | Action::RevokePermission { .. } => return None,
        })
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

/// The rules a column scope has to satisfy, in EITHER direction.
///
/// Shared because the two directions are different policies over the same
/// vocabulary: what is unsayable outbound is unsayable inbound, and letting
/// them validate separately is how one of them quietly starts accepting an
/// expression the other refuses.
/// Whether a scope change lets MORE through than it did before.
///
/// `None` is the widest setting there is, so moving to it from anything else
/// is a widening and moving away from it never is. Between two lists, one new
/// name is enough — a change that both adds and removes columns is a widening
/// for the added ones, and the repair it triggers is filtered back down by the
/// new scope anyway.
///
/// Deliberately conservative in one direction: it may answer "wider" for a
/// change that is not, which costs a redundant re-send, and it must never
/// answer "not wider" for one that is, which would leave a column permanently
/// blank on the other side with nothing to say so.
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
    // A blank matches nothing, so it narrows to nothing while looking
    // configured. A trailing comma in a text field is the ordinary way one
    // arrives, which is to say it comes straight from a surface.
    if fields.iter().any(|f| f.trim().is_empty()) {
        return Err(EngineError::Consequence(
            "a scope cannot contain a blank column name".into(),
        ));
    }
    // `head` and `body` are one Loro document, and its ops carry no field to
    // filter on — so a scope naming one of them gets the other too. That
    // cannot be enforced at serve time, but it CAN be reported here, where
    // there is somebody to tell. Silently widening a scope the user wrote is
    // the failure this refusal exists to prevent.
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

impl Engine {
    /// Publish a Karma definition to this Organ's other Cells (Ontology C7,
    /// axis 1).
    ///
    /// Called after every Program and Frequency mutation, and derived from the
    /// STORE rather than from the action: create, revise, activate and pause all
    /// publish the same thing — whatever is active now, or `null` — so the
    /// published value cannot drift from what this Cell holds, and a pause
    /// travels as surely as an activation. Without the pause travelling, turning
    /// a rule off here would leave the always-on Cell running the last
    /// definition it heard about.
    ///
    /// Both kinds, because either alone does nothing: every active Program is a
    /// member of every frozen occurrence epoch, so a Cell holding a synced
    /// Program with no Frequency has nothing to schedule it.
    pub(crate) async fn publish_karma_definition(
        &self,
        outcome: &ActionOutcome,
        kind: KarmaKind,
    ) -> Result<(), EngineError> {
        // `created` carries the mutated handle's uid for every one of these
        // actions, including the replayed ones — a replay publishes the same
        // value again, which is a redundant write and never a wrong one.
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
