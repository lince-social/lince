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
                let row = store::misc::get_promise(&self.store.pool, &promise)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(promise.clone()))?;
                store::misc::set_promise_delta(&self.store.pool, &promise, delta).await?;
                if let Some(transfer_uid) = row.transfer_uid {
                    store::transfers::invalidate_agreements(&self.store.pool, &transfer_uid)
                        .await?;
                }
            }
            Action::CreateTransfer {
                slug,
                head,
                agreement,
                agreement_pct,
                satiation,
                source,
                reserve_default,
                require_confirmation,
            } => {
                outcome.created = Some(
                    store::transfers::create(
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
                    .await?,
                );
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
            Action::AddParty { transfer, actor } => {
                let transfer = self.resolve(&transfer).await?;
                let actor = self.resolve(&actor).await?;
                outcome.created =
                    Some(store::transfers::add_party(&self.store.pool, &transfer, &actor).await?);
            }
            Action::AddPromiseToTransfer {
                transfer,
                record,
                delta,
                party,
                window_end,
                condition,
            } => {
                let transfer = self.resolve(&transfer).await?;
                let record = self.resolve(&record).await?;
                let party = self.resolve(&party).await?;
                outcome.created = Some(
                    store::misc::insert_promise(
                        &self.store.pool,
                        store::misc::NewPromise {
                            record_uid: Some(record),
                            delta,
                            window_end,
                            party_uid: Some(party),
                            state: Some(PromiseState::Proposed),
                            condition,
                            transfer_uid: Some(transfer),
                            ..Default::default()
                        },
                    )
                    .await?,
                );
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
                store::transfers::set_agreement(&self.store.pool, &transfer, &party, level).await?;
                // level 2 commits the party: its proposed promises become agreed
                if level == 2 {
                    for p in store::transfers::promises_of(&self.store.pool, &transfer).await? {
                        if p.state == PromiseState::Proposed {
                            store::misc::set_promise_state(
                                &self.store.pool,
                                &p.uid,
                                PromiseState::Agreed,
                            )
                            .await?;
                        }
                    }
                }
            }
            Action::ActivateTransfer { transfer } => {
                let transfer = self.resolve(&transfer).await?;
                crate::transfer::activate_promises(&self.store, &transfer).await?;
            }
            Action::SettleTransfer { transfer, actor } => {
                let transfer = self.resolve(&transfer).await?;
                let actor = self.resolve(&actor).await?;
                outcome.facts = self.settle_all_local(&transfer, &actor, now).await?;
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
