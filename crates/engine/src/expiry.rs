use chrono::{DateTime, Utc};
use nucleus::{Cause, CauseKind, Fact, NewFact, PromiseState};

use crate::Engine;
use crate::error::EngineError;

impl Engine {
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
