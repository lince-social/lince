use chrono::{DateTime, Utc};
use nucleus::expr::Expr;
use nucleus::{Fact, PromiseState};

use crate::Engine;
use crate::error::EngineError;

pub async fn agreed(store: &store::Store, transfer_uid: &str) -> Result<bool, EngineError> {
    store::transfers::get(&store.pool, transfer_uid)
        .await?
        .ok_or_else(|| EngineError::UnknownRecord(transfer_uid.into()))?;
    Ok(protein::transfer_agreement_ready(store, transfer_uid).await?)
}

pub async fn activate_promises(
    _store: &store::Store,
    transfer_uid: &str,
) -> Result<usize, EngineError> {
    Err(EngineError::Conflict {
        code: "transfer_phase_4_not_available",
        message: format!(
            "transfer {transfer_uid}: activation is unavailable before occurrence modeling"
        ),
    })
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

    pub async fn settle_all_local(
        &self,
        transfer_uid: &str,
        _actor: &str,
        _now: DateTime<Utc>,
    ) -> Result<Vec<Fact>, EngineError> {
        Err(EngineError::Conflict {
            code: "transfer_occurrence_settlement_required",
            message: format!(
                "transfer {transfer_uid}: use reviewed occurrence settlement; legacy bundle settlement is disabled"
            ),
        })
    }

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
}

async fn eval_promise_condition(engine: &Engine, expr: &Expr) -> Result<f64, EngineError> {
    let mut map = nucleus::MapResolver::default();
    for token in expr.tokens() {
        let value = match token.func.as_str() {
            "promise_state" => store::misc::promise_state(&engine.store.pool, &token.slug)
                .await?
                .map(PromiseState::ordinal)
                .unwrap_or(0.0),
            "quantity" => match store::records::resolve(&engine.store.pool, &token.slug).await? {
                Some(r) => r.quantity_f64(),
                None => 0.0,
            },
            _ => 0.0,
        };
        map.set(&token.func, &token.slug, token.dur_secs, value);
    }
    Ok(expr.eval(&mut map)?)
}
