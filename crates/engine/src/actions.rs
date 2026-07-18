//! Actions (blueprint VII.2): the typed write surface. Semantic verbs,
//! validated in the engine, all terminating in `append()` and/or sidecar
//! updates, each with provenance. Protein never mutates; Actions never query.
//! Sands and Fiote speak only these — Fiote has no privileged path.

use chrono::{DateTime, Utc};
use nucleus::{Cause, CauseKind, Fact, NewFact, PromiseState, RecordKind};
use serde::{Deserialize, Serialize};

use crate::Engine;
use crate::error::EngineError;

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
    AddQuantity {
        target: String,
        delta: f64,
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
    /// Classify a record under a Lingua concept (name or uid; `None` clears).
    SetConcept {
        target: String,
        concept: Option<String>,
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
    /// Undo a prior fact by appending its inverse (compensation, blueprint II.3):
    /// an append-only Ledger never deletes, so undo is a new fact with the
    /// opposite delta, caused by the original. Metadata/annotation facts
    /// (delta 0) have nothing to reverse and compensate to a no-op.
    Compensate {
        fact: String,
    },
    CreateConcept {
        name: String,
        #[serde(default)]
        parents: Vec<String>,
    },
    AddLink {
        from: String,
        kind: String, // Lingua concept name/uid
        to: String,
        quantity: Option<f64>,
    },
    RemoveLink {
        from: String,
        kind: String,
        to: String,
    },
    RelinkOrder {
        kind: String,
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
    /// Counteroffers are edits: changing a bundled promise's delta drops every
    /// party's agreement back to 0 (blueprint VIII.1).
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
        parties: Vec<String>,
        #[serde(default)]
        promises: Vec<TransferPromiseInput>,
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
    /// Karma CRUD (blueprint VII.2): rules are records; the registry reloads
    /// and Proof warnings come back on the outcome.
    CreateRule {
        slug: String,
        head: String,
        condition: String,
        #[serde(default = "default_gate")]
        gate: String,
        #[serde(default = "default_carry")]
        carry: String,
        #[serde(default)]
        debounce: Option<String>,
        #[serde(default)]
        consequences: Vec<ConsequenceInput>,
    },
    UpdateRule {
        rule: String,
        #[serde(default)]
        condition: Option<String>,
        #[serde(default)]
        gate: Option<String>,
        #[serde(default)]
        carry: Option<String>,
        /// `Some(None)` clears the debounce; absent leaves it untouched.
        #[serde(default, with = "double_option")]
        debounce: Option<Option<String>>,
        #[serde(default)]
        consequences: Option<Vec<ConsequenceInput>>,
    },
    CreateSignal {
        slug: String,
        head: String,
        source_kind: String,
        source: String,
        schedule: String,
    },
    CreateFrequency {
        slug: String,
        head: String,
        #[serde(default)]
        seconds: i64,
        #[serde(default)]
        days: i64,
        #[serde(default)]
        months: i64,
        #[serde(default)]
        day_of_week: Option<u8>,
        next_at: String,
        #[serde(default)]
        catch_up: bool,
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

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferReservePoint {
    #[default]
    None,
    Proposed,
    Agreed,
    Active,
}

impl TransferReservePoint {
    fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Proposed => "proposed",
            Self::Agreed => "agreed",
            Self::Active => "active",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferPromiseInput {
    pub record: String,
    pub party: String,
    pub delta: f64,
    #[serde(default)]
    pub window_end: Option<String>,
    #[serde(default)]
    pub condition: Option<String>,
    #[serde(default)]
    pub reserve_from: Option<TransferReservePoint>,
}

/// One consequence in the rule-CRUD wire form.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsequenceInput {
    pub kind: String,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub params: Option<serde_json::Value>,
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

fn default_gate() -> String {
    "!=0".into()
}

fn default_proximity() -> u32 {
    1
}

fn default_auto() -> String {
    "draft_only".into()
}

fn default_carry() -> String {
    "value".into()
}

/// `Option<Option<T>>` through JSON: absent = untouched, `null` = clear.
mod double_option {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer, T: Serialize>(
        value: &Option<Option<T>>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match value {
            Some(inner) => inner.serialize(serializer),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
        deserializer: D,
    ) -> Result<Option<Option<T>>, D::Error> {
        Ok(Some(Option::<T>::deserialize(deserializer)?))
    }
}

fn default_agreement() -> String {
    "individual".into()
}

fn default_typed_agreement() -> nucleus::transfer::AgreementType {
    nucleus::transfer::AgreementType::Individual
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

    /// `act` with an explicit clock — the DST-drivable variant (Part 0): same
    /// action, virtual `now`.
    pub async fn act_at(
        &self,
        action: Action,
        actor: Option<String>,
        now: DateTime<Utc>,
    ) -> Result<ActionOutcome, EngineError> {
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
                        quantity: 0.0, // level arrives via the one write path below
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
                            ..NewFact::quantity(rec.uid.clone(), quantity, Cause::user_edit())
                        },
                        now,
                    )
                    .await?;
                outcome.created = Some(rec.uid);
            }
            Action::SetQuantity { target, value } => {
                let uid = self.resolve(&target).await?;
                let current = store::records::quantity(&self.store.pool, &uid)
                    .await?
                    .unwrap_or(0.0);
                if value != current {
                    outcome.facts = self
                        .append(
                            NewFact {
                                actor_uid: actor,
                                ..NewFact::quantity(uid, value - current, Cause::user_edit())
                            },
                            now,
                        )
                        .await?;
                }
            }
            Action::AddQuantity { target, delta } => {
                let uid = self.resolve(&target).await?;
                if delta != 0.0 {
                    outcome.facts = self
                        .append(
                            NewFact {
                                actor_uid: actor,
                                ..NewFact::quantity(uid, delta, Cause::user_edit())
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
                store::records::set_text(&self.store.pool, &uid, head.as_deref(), body.as_deref())
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
                let slug = slug.filter(|s| !s.is_empty());
                store::records::set_slug(&self.store.pool, &uid, slug.as_deref()).await?;
                outcome.facts = self
                    .annotate(uid, actor, serde_json::json!({ "slug": slug }), now)
                    .await?;
            }
            Action::SetConcept { target, concept } => {
                let uid = self.resolve(&target).await?;
                let concept_uid = self.resolve_concept_opt(concept).await?;
                store::records::set_concept(&self.store.pool, &uid, concept_uid.as_deref()).await?;
                outcome.facts = self
                    .annotate(
                        uid,
                        actor,
                        serde_json::json!({ "concept": concept_uid }),
                        now,
                    )
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
            Action::Compensate { fact } => {
                let original = store::facts::get(&self.store.pool, &fact)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(fact.clone()))?;
                // Zero-delta facts (metadata/annotation) carry no quantity to
                // reverse — undoing them is a no-op, not an error.
                if original.delta != 0.0 {
                    outcome.facts = self
                        .append(
                            NewFact {
                                uid: None,
                                record_uid: original.record_uid,
                                delta: -original.delta,
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
            Action::CreateConcept { name, parents } => {
                let mut parent_uids = Vec::new();
                for p in &parents {
                    parent_uids.push(
                        store::concepts::resolve(&self.store.pool, p)
                            .await?
                            .ok_or_else(|| EngineError::UnknownRecord(p.clone()))?,
                    );
                }
                let refs: Vec<&str> = parent_uids.iter().map(String::as_str).collect();
                outcome.created =
                    Some(store::concepts::create(&self.store.pool, &name, &refs).await?);
            }
            Action::AddLink {
                from,
                kind,
                to,
                quantity,
            } => {
                let from = self.resolve(&from).await?;
                let to = self.resolve(&to).await?;
                // Ensure like the thread kinds do (2026-07-17): sands link with
                // vocabulary kinds (`assigned-to`, `part-of`, `resource-of`)
                // that need no ceremony before first use.
                let kind_uid = store::concepts::ensure(&self.store.pool, &kind).await?;
                outcome.created = Some(
                    store::links::add(&self.store.pool, &from, &kind_uid, &to, quantity).await?,
                );
                // Cycle warning on save (blueprint IV.2): only for order-like
                // kinds — a loop in @needs is a recipe error the user must see,
                // but only ordering kinds make "before" cycles meaningless.
                if is_order_like(&self.store.pool, &kind_uid).await? {
                    for cycle in kind_cycles(&self.store.pool, &kind_uid).await? {
                        if cycle.contains(&from) || cycle.contains(&to) {
                            outcome.warnings.push(format!(
                                "these {} records form a loop: {}",
                                cycle.len(),
                                cycle.join(" -> ")
                            ));
                        }
                    }
                }
                outcome.facts = self
                    .annotate_many(
                        vec![from.clone(), to.clone()],
                        actor,
                        serde_json::json!({
                            "action": "add-link",
                            "kind": kind,
                            "from": from,
                            "to": to,
                        }),
                        now,
                    )
                    .await?;
            }
            Action::RemoveLink { from, kind, to } => {
                let from = self.resolve(&from).await?;
                let to = self.resolve(&to).await?;
                let kind_uid = store::concepts::resolve(&self.store.pool, &kind)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(kind))?;
                store::links::remove(&self.store.pool, &from, &kind_uid, &to).await?;
                outcome.facts = self
                    .annotate_many(
                        vec![from.clone(), to.clone()],
                        actor,
                        serde_json::json!({
                            "action": "remove-link",
                            "kind": kind_uid,
                            "from": from,
                            "to": to,
                        }),
                        now,
                    )
                    .await?;
            }
            Action::RelinkOrder {
                kind,
                ordered,
                reverse,
            } => {
                let kind_uid = store::concepts::resolve(&self.store.pool, &kind)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(kind.clone()))?;
                let mut resolved = Vec::new();
                for token in ordered {
                    let uid = self.resolve(&token).await?;
                    if !resolved.iter().any(|existing| existing == &uid) {
                        resolved.push(uid);
                    }
                }
                if resolved.len() < 2 {
                    return Err(EngineError::Consequence(
                        "relink-order needs at least two records".into(),
                    ));
                }
                store::links::remove_kind_within_set(&self.store.pool, &kind_uid, &resolved)
                    .await?;
                for pair in resolved.windows(2) {
                    let (from, to) = if reverse {
                        (&pair[1], &pair[0])
                    } else {
                        (&pair[0], &pair[1])
                    };
                    store::links::add(&self.store.pool, from, &kind_uid, to, None).await?;
                }
                outcome.facts = self
                    .annotate_many(
                        resolved.clone(),
                        actor,
                        serde_json::json!({
                            "action": "relink-order",
                            "kind": kind,
                            "ordered": resolved,
                            "reverse": reverse,
                        }),
                        now,
                    )
                    .await?;
            }
            Action::CreateThread { target, head } => {
                let target_uid = self.resolve(&target).await?;
                let title = head.trim();
                if title.is_empty() {
                    return Err(EngineError::Consequence(
                        "thread title cannot be empty".into(),
                    ));
                }
                let thread = store::records::create(
                    &self.store.pool,
                    store::records::NewRecord {
                        slug: None,
                        kind: RecordKind::Thread,
                        head: title,
                        body: "",
                        quantity: 0.0,
                    },
                )
                .await?;
                let thread_of = store::concepts::ensure(&self.store.pool, "thread-of").await?;
                store::links::add(&self.store.pool, &thread.uid, &thread_of, &target_uid, None)
                    .await?;
                outcome.facts = self
                    .append(
                        NewFact {
                            actor_uid: actor.clone(),
                            ..NewFact::quantity(thread.uid.clone(), 1.0, Cause::user_edit())
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
                outcome.created = Some(thread.uid);
            }
            Action::CreateMessage {
                thread,
                body,
                parent,
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
                let body = body.trim();
                if body.is_empty() {
                    return Err(EngineError::Consequence(
                        "message body cannot be empty".into(),
                    ));
                }
                let head = message_head(body);
                let message = store::records::create(
                    &self.store.pool,
                    store::records::NewRecord {
                        slug: None,
                        kind: RecordKind::Message,
                        head: &head,
                        body,
                        quantity: 0.0,
                    },
                )
                .await?;
                let message_in = store::concepts::ensure(&self.store.pool, "message-in").await?;
                store::links::add(
                    &self.store.pool,
                    &message.uid,
                    &message_in,
                    &thread_uid,
                    None,
                )
                .await?;
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
                    let parent_threads =
                        store::links::records_from(&self.store.pool, &parent_uid, &message_in)
                            .await?;
                    if !parent_threads.iter().any(|row| row.uid == thread_uid) {
                        return Err(EngineError::Consequence(
                            "reply parent is not in the target thread".into(),
                        ));
                    }
                    let reply_to = store::concepts::ensure(&self.store.pool, "reply-to").await?;
                    store::links::add(&self.store.pool, &message.uid, &reply_to, &parent_uid, None)
                        .await?;
                }
                outcome.facts = self
                    .append(
                        NewFact {
                            actor_uid: actor.clone(),
                            ..NewFact::quantity(message.uid.clone(), 1.0, Cause::user_edit())
                        },
                        now,
                    )
                    .await?;
                outcome.facts.extend(
                    self.annotate(
                        thread_uid,
                        actor,
                        serde_json::json!({ "message": { "created": message.uid } }),
                        now,
                    )
                    .await?,
                );
                outcome.created = Some(message.uid);
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
                                delta: 0.0,
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
                }
                store::misc::set_promise_delta(&self.store.pool, &promise, delta).await?;
                if let Some(transfer_uid) = row.transfer_uid {
                    store::transfers::invalidate_agreements(&self.store.pool, &transfer_uid)
                        .await?;
                    outcome.facts = self
                        .annotate(
                            transfer_uid,
                            actor,
                            serde_json::json!({
                                "promise": promise,
                                "action": "edit-promise-delta",
                                "delta": delta,
                            }),
                            now,
                        )
                        .await?;
                }
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
                        "reserve_default must be none, proposed, agreed, active, or omitted"
                            .into(),
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
                    store::visibility::grant(
                        &self.store.pool,
                        "actor",
                        Some(subject),
                        &transfer,
                    )
                    .await?;
                }
                outcome.facts = self
                    .append(
                        NewFact {
                            actor_uid: actor,
                            ..NewFact::quantity(
                                transfer.clone(),
                                1.0,
                                Cause::user_edit(),
                            )
                        },
                        now,
                    )
                    .await?;
                outcome.created = Some(transfer);
            }
            Action::CreateTransferDraft {
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
                parties,
                promises,
            } => {
                let creator_person = self.require_transfer_creator(actor.as_deref()).await?;
                let visibility_actor = actor.clone();
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
                if (parties.is_empty() && creator_person.is_none()) || parties.len() > 64 {
                    return Err(EngineError::Consequence(
                        "a transfer draft requires 1 to 64 parties".into(),
                    ));
                }
                if promises.is_empty() || promises.len() > 256 {
                    return Err(EngineError::Consequence(
                        "a transfer draft requires 1 to 256 promises".into(),
                    ));
                }

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
                if matches!(satiation, TransferSatiation::FirstCompletes)
                    && source_uid.is_none()
                {
                    return Err(EngineError::Consequence(
                        "first_completes requires a source record shared with its siblings".into(),
                    ));
                }

                let mut people = Vec::with_capacity(parties.len());
                let mut seen_people = std::collections::HashSet::new();
                for token in parties {
                    let person_uid = self.resolve(token.trim()).await?;
                    let row = store::records::get(&self.store.pool, &person_uid)
                        .await?
                        .ok_or_else(|| EngineError::UnknownRecord(person_uid.clone()))?;
                    if row.kind != RecordKind::Person.as_str() {
                        return Err(EngineError::Consequence(format!(
                            "transfer party `{}` is not a Person record",
                            row.slug.as_deref().unwrap_or(&person_uid)
                        )));
                    }
                    if !seen_people.insert(person_uid.clone()) {
                        return Err(EngineError::Consequence(
                            "the same Person cannot be added twice".into(),
                        ));
                    }
                    people.push(person_uid);
                }
                if let Some(person_uid) = creator_person {
                    if seen_people.insert(person_uid.clone()) {
                        people.insert(0, person_uid);
                    }
                }
                if people.len() > 64 {
                    return Err(EngineError::Consequence(
                        "a transfer draft supports at most 64 parties".into(),
                    ));
                }

                let mut draft_promises = Vec::with_capacity(promises.len());
                for input in promises {
                    if !input.delta.is_finite() || input.delta == 0.0 {
                        return Err(EngineError::Consequence(
                            "every promise delta must be finite and non-zero".into(),
                        ));
                    }
                    let record_uid = self.resolve(input.record.trim()).await?;
                    let person_uid = self.resolve(input.party.trim()).await?;
                    if !seen_people.contains(&person_uid) {
                        return Err(EngineError::Consequence(
                            "every promise party must be included in the reviewed party list"
                                .into(),
                        ));
                    }
                    let window_end = input
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
                    let condition = input
                        .condition
                        .map(|value| value.trim().to_string())
                        .filter(|value| !value.is_empty());
                    if let Some(value) = condition.as_deref() {
                        nucleus::expr::Expr::parse(value).map_err(|error| {
                            EngineError::Consequence(format!(
                                "invalid promise condition: {error}"
                            ))
                        })?;
                    }
                    draft_promises.push(store::transfers::DraftPromise {
                        record_uid,
                        person_uid,
                        delta: input.delta,
                        window_end,
                        condition,
                        reserve_from: input
                            .reserve_from
                            .unwrap_or(reserve_default)
                            .as_str()
                            .into(),
                    });
                }
                if matches!(agreement, nucleus::transfer::AgreementType::Dependency)
                    && !draft_promises.iter().any(|promise| promise.condition.is_some())
                {
                    return Err(EngineError::Consequence(
                        "dependency agreement requires at least one promise condition".into(),
                    ));
                }

                let organ_uid = store::organs::local(&self.store.pool)
                    .await?
                    .map(|organ| organ.uid);
                let signer = self.signer.lock().await.clone();
                let fact_actor = actor
                    .clone()
                    .or_else(|| signer.as_ref().map(|value| value.actor_uid.clone()));
                let created = store::transfers::create_draft(
                    &self.store.pool,
                    store::transfers::NewTransferDraft {
                        slug,
                        head,
                        agreement_type: agreement.as_str().into(),
                        agreement_pct: agreement_pct.map(i64::from),
                        satiation: satiation.as_option(),
                        parent_uid,
                        source_uid,
                        visibility: visibility.as_str().into(),
                        max_proximity: max_proximity.map(i64::from),
                        reserve_default: reserve_default.as_str().into(),
                        require_confirmation,
                        people,
                        promises: draft_promises,
                        organ_uid,
                    },
                    now,
                    fact_actor,
                    visibility_actor,
                    |hash| signer.as_ref().map(|value| value.sign_hash(hash)),
                )
                .await?;
                outcome.facts = self.observe_committed_fact(created.fact, now).await?;
                outcome.created = Some(created.transfer_uid);
            }
            Action::ConfirmTransfer {
                transfer,
                confirmation,
            } => {
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
                            delta: 0.0,
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
                        EngineError::Consequence(format!(
                            "invalid promise condition: {error}"
                        ))
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
            Action::AgreeTransfer {
                transfer,
                party,
                level,
            } => {
                if !(0..=2).contains(&level) {
                    return Err(EngineError::Consequence(format!(
                        "bad agreement level {level}"
                    )));
                }
                let transfer = self.resolve(&transfer).await?;
                self.require_transfer_party(&transfer, &party, actor.as_deref())
                    .await?;
                let party_actor = store::transfers::party_actor(
                    &self.store.pool,
                    &transfer,
                    &party,
                )
                .await?
                .ok_or_else(|| {
                    EngineError::Consequence(
                        "agreement party does not belong to this transfer".into(),
                    )
                })?;
                store::transfers::set_agreement(&self.store.pool, &transfer, &party, level).await?;
                // level 2 commits the party: its proposed promises become agreed
                if level == 2 {
                    for p in store::transfers::promises_of(&self.store.pool, &transfer).await? {
                        if p.state == PromiseState::Proposed
                            && p.party_uid.as_deref() == Some(party_actor.as_str())
                        {
                            store::misc::set_promise_state(
                                &self.store.pool,
                                &p.uid,
                                PromiseState::Agreed,
                            )
                            .await?;
                        }
                    }
                }
                outcome.facts = self
                    .annotate(
                        transfer,
                        actor,
                        serde_json::json!({
                            "action": "agree-transfer",
                            "party": party,
                            "person": party_actor,
                            "level": level,
                        }),
                        now,
                    )
                    .await?;
            }
            Action::ActivateTransfer { transfer } => {
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
                        store::records::set_text(
                            &self.store.pool,
                            &existing.uid,
                            Some(&head),
                            Some(&body),
                        )
                        .await?;
                        if existing.quantity == 0.0 {
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
                                quantity: 1.0,
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
                    .unwrap_or(0.0);
                if current != 0.0 {
                    outcome.facts = self
                        .append(
                            NewFact {
                                uid: None,
                                record_uid: decision,
                                delta: -current,
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
                    let action: Action = serde_json::from_value(action_json).map_err(|e| {
                        EngineError::Consequence(format!("bad option action: {e}"))
                    })?;
                    let inner = Box::pin(self.act_at(action, actor, now)).await?;
                    outcome.facts.extend(inner.facts);
                    outcome.warnings.extend(inner.warnings);
                    if outcome.created.is_none() {
                        outcome.created = inner.created;
                    }
                }
            }
            Action::CreateRule {
                slug,
                head,
                condition,
                gate,
                carry,
                debounce,
                consequences,
            } => {
                let consequences = parse_consequences(consequences)?;
                let uid = store::rules::create(
                    &self.store.pool,
                    store::rules::NewRule {
                        slug: &slug,
                        head: &head,
                        condition: &condition,
                        gate: &gate,
                        carry: &carry,
                        consequences,
                    },
                )
                .await?;
                if let Some(debounce) = &debounce {
                    store::rules::set_debounce(&self.store.pool, &uid, Some(debounce)).await?;
                }
                for warning in self.reload_rules().await? {
                    outcome.warnings.push(warning.message);
                }
                outcome.created = Some(uid);
            }
            Action::UpdateRule {
                rule,
                condition,
                gate,
                carry,
                debounce,
                consequences,
            } => {
                let rule = self.resolve(&rule).await?;
                let consequences = consequences.map(parse_consequences).transpose()?;
                store::rules::update(
                    &self.store.pool,
                    &rule,
                    condition.as_deref(),
                    gate.as_deref(),
                    carry.as_deref(),
                    debounce.as_ref().map(|d| d.as_deref()),
                    consequences,
                )
                .await?;
                for warning in self.reload_rules().await? {
                    outcome.warnings.push(warning.message);
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
            Action::CreateFrequency {
                slug,
                head,
                seconds,
                days,
                months,
                day_of_week,
                next_at,
                catch_up,
            } => {
                let next_at = chrono::DateTime::parse_from_rfc3339(&next_at)
                    .map_err(|e| EngineError::Consequence(format!("bad next_at: {e}")))?
                    .with_timezone(&Utc);
                outcome.created = Some(
                    store::freqs::create(
                        &self.store.pool,
                        store::freqs::NewFrequency {
                            slug: &slug,
                            head: &head,
                            seconds,
                            days,
                            months,
                            day_of_week,
                            next_at,
                            catch_up,
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
                self.require_permission(actor.as_deref(), "role:create").await?;
                let role_id = store::auth::ensure_role(&self.store.pool, &name).await?;
                outcome.created = Some(role_id.to_string());
            }
            Action::CreateUser { username, name, password, role } => {
                self.require_permission(actor.as_deref(), "user:create").await?;
                let role_id = store::auth::role_by_name(&self.store.pool, &role)
                    .await?
                    .ok_or_else(|| EngineError::Consequence(format!("unknown role `{role}`")))?;
                let password_hash = utils::auth::hash_password(&password).map_err(EngineError::Io)?;
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
                self.require_permission(actor.as_deref(), "user:assign_role").await?;
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
                    return Err(EngineError::Consequence(format!(
                        "unknown user `{user}`"
                    )));
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
                self.require_permission(actor.as_deref(), "permission:assign").await?;
                let (subject, action_name) = permission
                    .split_once(':')
                    .ok_or_else(|| EngineError::Consequence(format!("bad permission `{permission}`")))?;
                let role_id = store::auth::role_by_name(&self.store.pool, &role)
                    .await?
                    .ok_or_else(|| EngineError::Consequence(format!("unknown role `{role}`")))?;
                let permission_id =
                    store::auth::ensure_permission(&self.store.pool, subject, action_name).await?;
                store::auth::grant(&self.store.pool, role_id, permission_id).await?;
            }
            Action::RevokePermission { role, permission } => {
                self.require_permission(actor.as_deref(), "permission:assign").await?;
                let (subject, action_name) = permission
                    .split_once(':')
                    .ok_or_else(|| EngineError::Consequence(format!("bad permission `{permission}`")))?;
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
    async fn require_permission(
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
        Err(EngineError::Forbidden(format!("missing {permission} permission")))
    }

    /// Resolve the authenticated app user to the Person they are allowed to
    /// represent. Local no-auth mode stays trusted and returns `None`; an
    /// authenticated session without an explicit binding is blocked instead
    /// of accepting a Person uid supplied by the client.
    async fn actor_person(&self, actor: Option<&str>) -> Result<Option<String>, EngineError> {
        let Some(actor) = actor else {
            return Ok(None);
        };
        let user = self.actor_user(actor).await?;
        store::auth::person_for_user(&self.store.pool, user.id)
            .await?
            .map(Some)
            .ok_or_else(|| {
                EngineError::Forbidden(
                    "authenticated user has no assigned person identity".into(),
                )
            })
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

    /// Terms may be edited by the authenticated creator or by a represented
    /// participant (counteroffers). Permission remains an independent gate.
    async fn require_transfer_editor(
        &self,
        transfer_uid: &str,
        actor: Option<&str>,
    ) -> Result<(), EngineError> {
        let Some(actor) = actor else {
            return Ok(());
        };
        self.require_permission(Some(actor), "transfer:update").await?;
        let person = self.actor_person(Some(actor)).await?.expect("authenticated actor");
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
        let Some(actor) = actor else {
            return Ok(());
        };
        self.require_permission(Some(actor), "transfer:update").await?;
        let person = self.actor_person(Some(actor)).await?.expect("authenticated actor");
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

    /// Agreement is always authored for the authenticated user's own party.
    async fn require_transfer_party(
        &self,
        transfer_uid: &str,
        party_uid: &str,
        actor: Option<&str>,
    ) -> Result<(), EngineError> {
        let Some(actor) = actor else {
            return Ok(());
        };
        self.require_permission(Some(actor), "transfer:update").await?;
        let expected = self.actor_person(Some(actor)).await?.expect("authenticated actor");
        let actual = store::transfers::party_actor(&self.store.pool, transfer_uid, party_uid)
            .await?
            .ok_or_else(|| {
                EngineError::Forbidden("agreement party is not in this transfer".into())
            })?;
        if actual == expected {
            return Ok(());
        }
        Err(EngineError::Forbidden(
            "cannot author another participant's agreement".into(),
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
        let Some(actor) = actor else {
            return Ok(());
        };
        self.require_permission(Some(actor), "transfer:update").await?;
        let expected = self.actor_person(Some(actor)).await?.expect("authenticated actor");
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

    /// Resolve an optional concept token (name or uid) to a uid. `None` and the
    /// empty string both mean "clear" and resolve to `None`.
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
                delta: 0.0,
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

/// Parse rule-CRUD consequence wire rows into typed specs.
fn parse_consequences(
    inputs: Vec<ConsequenceInput>,
) -> Result<Vec<(nucleus::ConsequenceKind, Option<String>, Option<serde_json::Value>)>, EngineError>
{
    inputs
        .into_iter()
        .map(|c| {
            let kind = nucleus::ConsequenceKind::parse(&c.kind).ok_or_else(|| {
                EngineError::Consequence(format!("unknown consequence kind `{}`", c.kind))
            })?;
            Ok((kind, c.target, c.params))
        })
        .collect()
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
    let edges = store::links::edges_of_kind(pool, kind_uid).await?;
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
