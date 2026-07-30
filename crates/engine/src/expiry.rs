//! Promise expiry (blueprint V.2, decision 3): a heartbeat arm moves
//! past-window promises to `broken` (if agreed/active — a commitment was not
//! kept) or `withdrawn` (if open/proposed — the offer simply lapsed). Each
//! transition drops a zero-delta annotation fact on the target record, and a
//! broken promise enqueues a decision-record (XIII) — someone should look.

use chrono::{DateTime, Utc};
use nucleus::{Cause, CauseKind, Fact, NewFact, PromiseState};

use crate::Engine;
use crate::error::EngineError;

impl Engine {
    /// Close open decisions whose deadline passed (blueprint XIII.1): the
    /// answer becomes 'expired' and the record's quantity drops to 0 through
    /// the Ledger, so Karma can react to expired decisions like anything else.
    pub async fn expire_decisions(&self, now: DateTime<Utc>) -> Result<Vec<Fact>, EngineError> {
        let mut out = Vec::new();
        for decision in
            store::misc::expired_open_decisions(&self.store.pool, &now.to_rfc3339()).await?
        {
            store::misc::answer_decision(&self.store.pool, &decision, "expired").await?;
            let current = store::records::quantity(&self.store.pool, &decision)
                .await?
                .unwrap_or_else(store::exact::zero);
            if !current.is_zero() {
                out.extend(
                    self.append(
                        NewFact {
                            uid: None,
                            record_uid: decision,
                            delta: store::exact::negate(current)?,
                            at: None,
                            actor_uid: None,
                            cause: Cause {
                                kind: CauseKind::Action,
                                uid: None,
                            },
                            payload: Some(serde_json::json!({ "answer": "expired" }).to_string()),
                        },
                        now,
                    )
                    .await?,
                );
            }
        }
        Ok(out)
    }

    /// Sweep past-window promises. Returns the annotation facts committed.
    pub async fn expire_promises(&self, now: DateTime<Utc>) -> Result<Vec<Fact>, EngineError> {
        let mut out = Vec::new();
        for promise in store::misc::expired_promises(&self.store.pool, &now.to_rfc3339()).await? {
            let next = match promise.state {
                PromiseState::Agreed | PromiseState::Active => PromiseState::Broken,
                _ => PromiseState::Withdrawn,
            };
            store::misc::set_promise_state(&self.store.pool, &promise.uid, next).await?;

            if let Some(record_uid) = promise.record_uid.clone() {
                out.extend(
                    self.append(
                        NewFact {
                            uid: None,
                            record_uid,
                            delta: nucleus::fact::zero_delta(),
                            at: None,
                            actor_uid: None,
                            cause: Cause {
                                kind: CauseKind::Action,
                                uid: Some(promise.uid.clone()),
                            },
                            payload: Some(
                                serde_json::json!({
                                    "promise": {
                                        "from": promise.state.as_str(),
                                        "to": next.as_str(),
                                    },
                                    "expired": true,
                                })
                                .to_string(),
                            ),
                        },
                        now,
                    )
                    .await?,
                );
            }

            // A broken commitment deserves attention; a lapsed offer does not.
            if next == PromiseState::Broken {
                store::misc::create_decision(
                    &self.store.pool,
                    &promise.uid,
                    "expiry",
                    &format!("Promise {} broke its window", promise.uid),
                    &serde_json::json!([
                        { "label": "acknowledge" },
                        { "label": "re-propose" },
                    ]),
                )
                .await?;
            }
        }
        Ok(out)
    }
}
