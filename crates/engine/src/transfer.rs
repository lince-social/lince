//! Transfer (blueprint VIII): a bundle of promises + an agreement policy +
//! a visibility policy. Settlement is the only Record mutation, and it goes
//! through the one write path with `cause = settlement:<uid>` — idempotent
//! because promises move to `kept` in the same pass.

use chrono::{DateTime, Utc};
use nucleus::expr::Expr;
use nucleus::transfer::{AgreementType, policy_satisfied};
use nucleus::{Cause, Fact, NewFact, PromiseState};

use crate::Engine;
use crate::error::EngineError;

/// Is the bundle's agreement policy satisfied right now? (Free function so the
/// Karma `advance_transfer` consequence can use it without an Engine handle.)
pub async fn agreed(store: &store::Store, transfer_uid: &str) -> Result<bool, EngineError> {
    let t = store::transfers::get(&store.pool, transfer_uid)
        .await?
        .ok_or_else(|| EngineError::UnknownRecord(transfer_uid.into()))?;
    let agreement = AgreementType::parse(&t.agreement_type)
        .ok_or_else(|| EngineError::Consequence(format!("bad agreement {}", t.agreement_type)))?;
    let levels: Vec<i64> = store::transfers::party_levels(&store.pool, transfer_uid)
        .await?
        .into_iter()
        .map(|(_, _, level)| level)
        .collect();
    Ok(policy_satisfied(
        agreement,
        t.agreement_pct.map(|p| p as u8),
        &levels,
    ))
}

/// Move a transfer's `agreed` promises to `active` — Karma's `advance_transfer`
/// lands here, and it never bypasses the agreement policy.
pub async fn activate_promises(
    store: &store::Store,
    transfer_uid: &str,
) -> Result<usize, EngineError> {
    if !agreed(store, transfer_uid).await? {
        return Err(EngineError::Consequence(format!(
            "transfer {transfer_uid}: agreement policy not satisfied"
        )));
    }
    let mut activated = 0;
    for p in store::transfers::promises_of(&store.pool, transfer_uid).await? {
        if p.state == PromiseState::Agreed {
            store::misc::set_promise_state(&store.pool, &p.uid, PromiseState::Active).await?;
            activated += 1;
        }
    }
    Ok(activated)
}

impl Engine {
    pub async fn transfer_agreed(&self, transfer_uid: &str) -> Result<bool, EngineError> {
        agreed(&self.store, transfer_uid).await
    }

    pub async fn activate_transfer_promises(
        &self,
        transfer_uid: &str,
    ) -> Result<usize, EngineError> {
        activate_promises(&self.store, transfer_uid).await
    }

    /// Settle every ACTIVE promise of this transfer whose party is `actor`
    /// (blueprint VIII.3). Facts land with `cause = settlement`; promises move
    /// to `kept`; conditional promises (chains/spectators) get their shot;
    /// satiation runs last.
    pub async fn settle_all_local(
        &self,
        transfer_uid: &str,
        actor: &str,
        now: DateTime<Utc>,
    ) -> Result<Vec<Fact>, EngineError> {
        let t = store::transfers::get(&self.store.pool, transfer_uid)
            .await?
            .ok_or_else(|| EngineError::UnknownRecord(transfer_uid.into()))?;
        if !t.active {
            return Err(EngineError::Consequence(format!(
                "transfer {transfer_uid} is inactive"
            )));
        }
        if !self.transfer_agreed(transfer_uid).await? {
            return Err(EngineError::Consequence(format!(
                "transfer {transfer_uid}: agreement policy not satisfied"
            )));
        }
        // Delivery/receipt confirmations (VIII.3): when the transfer demands
        // them, two annotation facts on the transfer record gate active -> kept.
        if t.require_confirmation {
            let log = store::facts::for_record(&self.store.pool, transfer_uid, 10_000).await?;
            let confirmed = |kind: &str| {
                log.iter().any(|f| {
                    f.payload
                        .as_deref()
                        .is_some_and(|p| p.contains(&format!("\"confirmation\":\"{kind}\"")))
                })
            };
            if !confirmed("delivery") || !confirmed("receipt") {
                return Err(EngineError::Consequence(format!(
                    "transfer {transfer_uid}: awaiting delivery/receipt confirmation"
                )));
            }
        }

        let mut committed = Vec::new();
        for p in store::transfers::promises_of(&self.store.pool, transfer_uid).await? {
            if p.state != PromiseState::Active || p.party_uid.as_deref() != Some(actor) {
                continue;
            }
            let Some(record_uid) = p.record_uid.clone() else {
                continue;
            };
            let facts = self
                .append(
                    NewFact {
                        uid: None,
                        record_uid,
                        delta: p.delta,
                        at: None,
                        actor_uid: Some(actor.to_string()),
                        cause: Cause::settlement(transfer_uid.to_string()),
                        payload: Some(serde_json::json!({ "promise": p.uid }).to_string()),
                    },
                    now,
                )
                .await?;
            store::misc::set_promise_state(&self.store.pool, &p.uid, PromiseState::Kept).await?;
            committed.extend(facts);
        }

        self.trigger_conditional_promises().await?;
        self.apply_satiation(&t, now).await?;
        Ok(committed)
    }

    /// Chains and spectators (blueprint V.3): promises whose `condition`
    /// (same expression grammar) now evaluates non-zero advance toward active.
    pub async fn trigger_conditional_promises(&self) -> Result<usize, EngineError> {
        let mut triggered = 0;
        for p in store::transfers::conditional_pending(&self.store.pool).await? {
            let Some(condition) = &p.condition else {
                continue;
            };
            let Ok(expr) = Expr::parse(condition) else {
                continue;
            };
            let fired = match eval_promise_condition(self, &expr).await {
                Ok(v) => v != 0.0,
                Err(_) => false,
            };
            if fired {
                // proposed -> agreed -> active, silently within the owner's Cell
                let mut state = p.state;
                for next in [PromiseState::Agreed, PromiseState::Active] {
                    if PromiseState::can_transition(state, next) {
                        store::misc::set_promise_state(&self.store.pool, &p.uid, next).await?;
                        state = next;
                    }
                }
                triggered += 1;
            }
        }
        Ok(triggered)
    }

    async fn apply_satiation(
        &self,
        t: &store::transfers::TransferRow,
        now: DateTime<Utc>,
    ) -> Result<(), EngineError> {
        if t.satiation.as_deref() != Some("first_completes") {
            return Ok(());
        }
        let Some(source) = &t.source_uid else {
            return Ok(());
        };
        for sibling in
            store::transfers::siblings_of_source(&self.store.pool, source, &t.record_uid).await?
        {
            // deactivate the sibling bundle; withdraw its unkept promises
            let current = store::records::quantity(&self.store.pool, &sibling)
                .await?
                .unwrap_or(0.0);
            if current != 0.0 {
                self.append(
                    NewFact {
                        uid: None,
                        record_uid: sibling.clone(),
                        delta: -current,
                        at: None,
                        actor_uid: None,
                        cause: Cause::settlement(t.record_uid.clone()),
                        payload: Some(r#"{"satiation":"first_completes"}"#.into()),
                    },
                    now,
                )
                .await?;
            }
            for p in store::transfers::promises_of(&self.store.pool, &sibling).await? {
                if PromiseState::can_transition(p.state, PromiseState::Withdrawn) {
                    store::misc::set_promise_state(
                        &self.store.pool,
                        &p.uid,
                        PromiseState::Withdrawn,
                    )
                    .await?;
                }
            }
        }
        Ok(())
    }
}

/// Evaluate a conditional promise's condition (same grammar as Karma, prefetch
/// then pure eval): `promise_state(@p_uid)` and `quantity(@x)` in v1.
async fn eval_promise_condition(engine: &Engine, expr: &Expr) -> Result<f64, EngineError> {
    let mut map = nucleus::MapResolver::default();
    for token in expr.tokens() {
        let value = match token.func.as_str() {
            "promise_state" => store::misc::promise_state(&engine.store.pool, &token.slug)
                .await?
                .map(PromiseState::ordinal)
                .unwrap_or(0.0),
            "quantity" => match store::records::resolve(&engine.store.pool, &token.slug).await? {
                Some(r) => r.quantity,
                None => 0.0,
            },
            _ => 0.0,
        };
        map.set(&token.func, &token.slug, token.dur_secs, value);
    }
    Ok(expr.eval(&mut map)?)
}
