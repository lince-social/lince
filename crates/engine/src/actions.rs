//! Actions (blueprint VII.2): the typed write surface. Semantic verbs,
//! validated in the engine, all terminating in `append()` and/or sidecar
//! updates, each with provenance. Protein never mutates; Actions never query.
//! Sands and Fiote speak only these — Fiote has no privileged path.

use chrono::Utc;
use nucleus::{Cause, CauseKind, Fact, NewFact, PromiseState, RecordKind};
use serde::{Deserialize, Serialize};

use crate::error::EngineError;
use crate::Engine;

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
    SetQuantity { target: String, value: f64 },
    AddQuantity { target: String, delta: f64 },
    Activate { target: String },
    Deactivate { target: String },
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
    RemoveLink { from: String, kind: String, to: String },
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
    PromiseTransition { promise: String, to: PromiseState },
    /// Counteroffers are edits: changing a bundled promise's delta drops every
    /// party's agreement back to 0 (blueprint VIII.1).
    EditPromiseDelta { promise: String, delta: f64 },
    /// Answer a decision-record: records the answer and closes it (quantity -> 0).
    Decide { decision: String, answer: String },
    CreateTransfer {
        slug: Option<String>,
        head: String,
        #[serde(default = "default_agreement")]
        agreement: String,
        agreement_pct: Option<i64>,
        satiation: Option<String>,
        source: Option<String>,
    },
    AddParty { transfer: String, actor: String },
    AddPromiseToTransfer {
        transfer: String,
        record: String,
        delta: f64,
        party: String,
        window_end: Option<String>,
        /// Chain/spectator condition, same grammar as Karma conditions.
        condition: Option<String>,
    },
    AgreeTransfer { transfer: String, party: String, level: i64 },
    /// agreed -> active for the bundle's promises, within policy.
    ActivateTransfer { transfer: String },
    SettleTransfer { transfer: String, actor: String },
    SetPlace { target: String, lat: f64, lon: f64, address: Option<String> },
    GrantVisibility {
        subject_kind: String, // organ | actor | public | fiote
        subject: Option<String>,
        target: String,
    },
    /// Persist a Protein AST as a record (kind='protein') — the saved view.
    SaveProtein { slug: String, head: String, ast: serde_json::Value },
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
}

impl Engine {
    /// Execute one Action with provenance. `actor` is the acting person/organ
    /// record uid; it lands on every fact this action commits.
    pub async fn act(
        &self,
        action: Action,
        actor: Option<String>,
    ) -> Result<ActionOutcome, EngineError> {
        let now = Utc::now();
        let mut outcome = ActionOutcome::default();
        match action {
            Action::CreateRecord { slug, kind, head, body, quantity } => {
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
                if quantity != 0.0 {
                    outcome.facts = self
                        .append(
                            NewFact {
                                actor_uid: actor,
                                ..NewFact::quantity(rec.uid.clone(), quantity, Cause::user_edit())
                            },
                            now,
                        )
                        .await?;
                }
                outcome.created = Some(rec.uid);
            }
            Action::SetQuantity { target, value } => {
                let uid = self.resolve(&target).await?;
                let current =
                    store::records::quantity(&self.store.pool, &uid).await?.unwrap_or(0.0);
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
            Action::AddLink { from, kind, to, quantity } => {
                let from = self.resolve(&from).await?;
                let to = self.resolve(&to).await?;
                let kind_uid = store::concepts::resolve(&self.store.pool, &kind)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(kind.clone()))?;
                outcome.created =
                    Some(store::links::add(&self.store.pool, &from, &kind_uid, &to, quantity).await?);
            }
            Action::RemoveLink { from, kind, to } => {
                let from = self.resolve(&from).await?;
                let to = self.resolve(&to).await?;
                let kind_uid = store::concepts::resolve(&self.store.pool, &kind)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(kind))?;
                store::links::remove(&self.store.pool, &from, &kind_uid, &to).await?;
            }
            Action::CreatePromise { record, delta, window_end, party, open } => {
                let record_uid = self.resolve(&record).await?;
                let state = if open { PromiseState::Open } else { PromiseState::Proposed };
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
            Action::CreateTransfer { slug, head, agreement, agreement_pct, satiation, source } => {
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
                        },
                    )
                    .await?,
                );
            }
            Action::AddParty { transfer, actor } => {
                let transfer = self.resolve(&transfer).await?;
                let actor = self.resolve(&actor).await?;
                outcome.created =
                    Some(store::transfers::add_party(&self.store.pool, &transfer, &actor).await?);
            }
            Action::AddPromiseToTransfer { transfer, record, delta, party, window_end, condition } => {
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
            Action::AgreeTransfer { transfer, party, level } => {
                if !(0..=2).contains(&level) {
                    return Err(EngineError::Consequence(format!("bad agreement level {level}")));
                }
                let transfer = self.resolve(&transfer).await?;
                store::transfers::set_agreement(&self.store.pool, &transfer, &party, level)
                    .await?;
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
            Action::SetPlace { target, lat, lon, address } => {
                let target = self.resolve(&target).await?;
                let place =
                    store::places::create(&self.store.pool, lat, lon, address.as_deref()).await?;
                store::places::set_record_place(&self.store.pool, &target, &place).await?;
                outcome.created = Some(place);
            }
            Action::GrantVisibility { subject_kind, subject, target } => {
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
                let rec = store::records::create(
                    &self.store.pool,
                    store::records::NewRecord {
                        slug: Some(&slug),
                        kind: RecordKind::Protein,
                        head: &head,
                        body: "",
                        quantity: 1.0,
                    },
                )
                .await?;
                store::records::set_extension(&self.store.pool, &rec.uid, "lince.protein", &ast)
                    .await?;
                outcome.created = Some(rec.uid);
            }
            Action::Decide { decision, answer } => {
                store::misc::answer_decision(&self.store.pool, &decision, &answer).await?;
                // closing the decision is a fact (quantity 1 -> 0), so Karma can
                // react to answered decisions like anything else (XIII.1)
                let current =
                    store::records::quantity(&self.store.pool, &decision).await?.unwrap_or(0.0);
                if current != 0.0 {
                    outcome.facts = self
                        .append(
                            NewFact {
                                uid: None,
                                record_uid: decision,
                                delta: -current,
                                at: None,
                                actor_uid: actor,
                                cause: Cause { kind: CauseKind::Action, uid: None },
                                payload: Some(
                                    serde_json::json!({ "answer": answer }).to_string(),
                                ),
                            },
                            now,
                        )
                        .await?;
                }
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
}
