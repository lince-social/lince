//! Protein subscription execution, invalidation, and retained result sharing.

use std::{
    collections::HashMap,
    sync::{Arc, Weak},
};

use engine::{
    Engine,
    action_intent::ActionIntentSession,
    performance::{Span, add},
};
use nucleus::{
    Fact,
    sync::{Activity, Direction, Instance, Outcome, Summary, Update},
};
use protein::{
    FactChanges, Protein, ProteinError, Source, depends_on_forecast, error_code, is_ephemeral,
    uses_signer,
};
use serde_json::{Value, from_value};

use super::{Session, session_denied};
use crate::ServerMessage;

const MAX_SUBSCRIPTIONS: usize = 64;
pub(super) const MAX_SUBSCRIPTION_ID_BYTES: usize = 512;
type Rows = Arc<Vec<Value>>;

pub(super) struct Subscription {
    query: Arc<Protein>,
    pub(super) rows: Option<Rows>,
}

struct QueryResult {
    rows: Rows,
    compared: Option<(Weak<Vec<Value>>, bool)>,
}

impl QueryResult {
    fn unchanged(&mut self, previous: Option<&Rows>, first: bool) -> bool {
        let Some(previous) = previous else {
            return false;
        };
        if Arc::ptr_eq(previous, &self.rows) {
            return true;
        }
        if let Some((compared, equal)) = &self.compared
            && compared.as_ptr() == Arc::as_ptr(previous)
        {
            add("cell-query-result-comparison-reuses", 1);
            return *equal;
        }
        let equal = {
            let _comparison = Span::new("cell-query-result-comparison");
            add("cell-query-result-comparisons", 1);
            previous.as_ref() == self.rows.as_ref()
        };
        if first && equal {
            self.rows = Arc::clone(previous);
        } else {
            self.compared = Some((Arc::downgrade(previous), equal));
        }
        equal
    }
}

#[derive(Clone, Copy)]
pub(super) enum SubscriptionChange<'a> {
    All,
    Facts(&'a [Fact]),
    Projection,
    Ephemeral,
}

struct QueryScope<'a> {
    engine: &'a Engine,
    subject: Option<&'a str>,
    action_intent: Option<&'a ActionIntentSession>,
    connection_id: &'a str,
}

impl Session {
    pub(super) async fn update_subscriptions(
        &mut self,
        change: SubscriptionChange<'_>,
        full_snapshot: bool,
    ) -> Vec<ServerMessage> {
        let _batch = Span::new("cell-query-batch-await");
        if self.subscriptions.is_empty() {
            return Vec::new();
        }
        let scope = QueryScope {
            engine: &self.engine,
            subject: self.subject.as_deref(),
            action_intent: self.action_intent.ready(),
            connection_id: &self.connection_id,
        };
        let subscriptions: Vec<_> = {
            let _selection = Span::new("cell-query-invalidation");
            let facts = match change {
                SubscriptionChange::Facts(facts) => Some(FactChanges::new(facts, scope.subject)),
                _ => None,
            };
            self.subscriptions
                .iter_mut()
                .filter(|(_, subscription)| match change {
                    SubscriptionChange::All => true,
                    SubscriptionChange::Facts(_) => facts
                        .as_ref()
                        .is_some_and(|facts| facts.affects(&subscription.query)),
                    SubscriptionChange::Projection => depends_on_forecast(&subscription.query),
                    SubscriptionChange::Ephemeral => is_ephemeral(&subscription.query),
                })
                .collect()
        };
        let mut out = Vec::with_capacity(subscriptions.len());
        let mut results: HashMap<usize, QueryResult> = HashMap::with_capacity(subscriptions.len());
        for (id, subscription) in subscriptions {
            let query = &subscription.query;
            let key = Arc::as_ptr(query).addr();
            let reused = results.get(&key).map(|result| &result.rows);
            let first_result = reused.is_none();
            let result = if matches!(change, SubscriptionChange::Ephemeral) {
                match reused {
                    Some(rows) => {
                        add("cell-query-batch-reuses", 1);
                        Ok(Arc::clone(rows))
                    }
                    None => scope.execute(query).await.map(Arc::new),
                }
            } else {
                scope.execute_sync(id, query, reused).await
            };
            match result {
                Ok(rows) => {
                    let result = results.entry(key).or_insert(QueryResult {
                        rows,
                        compared: None,
                    });
                    let changed = !result.unchanged(subscription.rows.as_ref(), first_result);
                    let rows = Arc::clone(&result.rows);
                    if full_snapshot || changed {
                        out.push(ServerMessage::Update {
                            id: id.clone(),
                            rows: Arc::clone(&rows),
                        });
                    } else {
                        add("cell-query-results-unchanged", 1);
                    }
                    subscription.rows = Some(rows);
                }
                Err(error) => {
                    subscription.rows = None;
                    out.push(ServerMessage::Error {
                        id: id.clone(),
                        message: error.to_string(),
                        code: error_code(&error),
                    });
                }
            }
        }
        out
    }

    pub(super) fn clear_subscription_rows(&mut self) {
        for subscription in self.subscriptions.values_mut() {
            subscription.rows = None;
        }
    }

    pub(super) async fn subscribe(&mut self, id: String, protein: Protein) -> Vec<ServerMessage> {
        if id.len() > MAX_SUBSCRIPTION_ID_BYTES
            || self.workspace_subscriptions.contains_key(&id)
            || (self.subscriptions.len() >= MAX_SUBSCRIPTIONS
                && !self.subscriptions.contains_key(&id))
        {
            return vec![session_denied("subscription limit")];
        }
        let scope = QueryScope {
            engine: &self.engine,
            subject: self.subject.as_deref(),
            action_intent: self.action_intent.ready(),
            connection_id: &self.connection_id,
        };
        match scope.execute_sync(&id, &protein, None).await {
            Ok(rows) => {
                let alias = self
                    .subscriptions
                    .values()
                    .find(|subscription| subscription.query.as_ref() == &protein);
                let (query, rows) = if let Some(alias) = alias {
                    let query = Arc::clone(&alias.query);
                    let rows = self
                        .subscriptions
                        .values()
                        .filter(|subscription| Arc::ptr_eq(&subscription.query, &query))
                        .filter_map(|subscription| subscription.rows.as_ref())
                        .find(|previous| previous.as_ref() == rows.as_ref())
                        .cloned()
                        .unwrap_or(rows);
                    (query, rows)
                } else {
                    (Arc::new(protein), rows)
                };
                let message = ServerMessage::Snapshot {
                    id: id.clone(),
                    rows: Arc::clone(&rows),
                };
                self.subscriptions.insert(
                    id,
                    Subscription {
                        query,
                        rows: Some(rows),
                    },
                );
                vec![message]
            }
            Err(e) => {
                if let Some(subscription) = self.subscriptions.get_mut(&id) {
                    subscription.rows = None;
                }
                vec![ServerMessage::Error {
                    id,
                    message: e.to_string(),
                    code: error_code(&e),
                }]
            }
        }
    }

    /// Reports whether nearby or collaborative updates still need periodic polling.
    pub fn has_ephemeral_subscriptions(&self) -> bool {
        self.has_joined_documents()
            || self
                .subscriptions
                .values()
                .any(|subscription| is_ephemeral(&subscription.query))
    }

    pub(super) async fn subscribe_saved(&mut self, id: String, name: &str) -> Vec<ServerMessage> {
        let protein = match load_saved(&self.engine, name).await {
            Ok(protein) => protein,
            Err(e) => {
                if let Some(subscription) = self.subscriptions.get_mut(&id) {
                    subscription.rows = None;
                }
                return vec![ServerMessage::Error {
                    id,
                    message: e,
                    code: None,
                }];
            }
        };
        self.subscribe(id, protein).await
    }
}

impl QueryScope<'_> {
    async fn execute_sync(
        &self,
        id: &str,
        protein: &Protein,
        reused: Option<&Rows>,
    ) -> Result<Rows, ProteinError> {
        self.engine
            .sync_service
            .run(
                &self.engine.store,
                Activity {
                    instance: Instance::interface(self.connection_id, id),
                    direction: Direction::Outgoing,
                    update: Update::Full,
                },
                async {
                    if let Some(rows) = reused {
                        add("cell-query-batch-reuses", 1);
                        Ok(Arc::clone(rows))
                    } else {
                        self.execute(protein).await.map(Arc::new)
                    }
                },
                |rows| {
                    let mut summary = Summary::new(Outcome::Refreshed, rows.len());
                    summary.subjects = rows
                        .iter()
                        .filter_map(|row| row.get("uid").and_then(Value::as_str).map(str::to_owned))
                        .take(32)
                        .collect();
                    summary
                },
            )
            .await
    }

    async fn execute(&self, protein: &Protein) -> Result<Vec<Value>, ProteinError> {
        let _query = Span::new("cell-query-execute-await");
        let _source = Span::new(match protein.source {
            Source::Record => "cell-query-record-await",
            Source::Promise => "cell-query-promise-await",
            Source::Decision => "cell-query-decision-await",
            Source::Fact => "cell-query-fact-await",
            Source::Concept => "cell-query-concept-await",
            Source::Lingua => "cell-query-lingua-await",
            Source::Assertion => "cell-query-assertion-await",
            Source::Transfer => "cell-query-transfer-await",
            Source::TransferSettlementPreview => "cell-query-transfer-settlement-preview-await",
            Source::TransferBulkCompletionPreview => {
                "cell-query-transfer-bulk-completion-preview-await"
            }
            Source::Auth => "cell-query-auth-await",
            Source::Karma => "cell-query-karma-await",
            Source::KarmaRule => "cell-query-karma-rule-await",
            Source::Timeline => "cell-query-timeline-await",
            Source::Calendar => "cell-query-calendar-await",
            Source::Schedule => "cell-query-schedule-await",
            Source::Entry => "cell-query-entry-await",
            Source::Frequency => "cell-query-frequency-await",
            Source::Recurrence => "cell-query-recurrence-await",
            Source::Nearby => "cell-query-nearby-await",
        });
        add("cell-queries", 1);
        if depends_on_forecast(protein) {
            let _projection = Span::new("cell-query-projection-request-await");
            let context = protein::calendar::context(protein, self.subject)?;
            self.engine
                .request_projection(context)
                .await
                .map_err(|error| store::StoreError::Protocol(error.to_string()))?;
        }
        let signer_actor = if uses_signer(protein) {
            let _signer = Span::new("cell-query-signer-context-await");
            add("cell-query-signer-lookups", 1);
            self.available_signer_actor().await
        } else {
            None
        };
        let nearby = is_ephemeral(protein).then(|| self.engine.nearby_peers());
        let rows = protein::execute_for_with_context(
            &self.engine.store,
            protein,
            self.subject,
            signer_actor.as_deref(),
            protein::Context {
                nearby: nearby.as_deref(),
            },
        )
        .await?;
        add(
            "cell-query-rows",
            u64::try_from(rows.len()).unwrap_or(u64::MAX),
        );
        Ok(rows)
    }

    async fn available_signer_actor(&self) -> Option<String> {
        if self.subject.is_some() {
            return self
                .action_intent
                .filter(|session| session.bound_key_id().is_some())
                .map(|session| session.person_uid().to_string());
        }
        self.engine.signer_actor_uid().await
    }
}

async fn load_saved(engine: &Engine, name: &str) -> Result<Protein, String> {
    let record = store::records::resolve(&engine.store.pool, name)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("no saved protein {name}"))?;
    let ast = store::records::get_extension(&engine.store.pool, &record.uid, "lince.protein")
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("{name} has no protein AST"))?;
    from_value(ast).map_err(|e| e.to_string())
}
