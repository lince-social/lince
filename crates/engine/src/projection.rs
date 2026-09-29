use std::collections::BTreeMap;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};

use nucleus::execution::Execution;
use nucleus::karma::{ReferenceKind, TypedUid};
use nucleus::projection::{Context, Incomplete, MAX_SPANS, MAX_STEPS, Span};
use nucleus::simulation::{Cause, Quantity, RuleOccurrence};

use crate::{Engine, EngineError};

#[derive(Clone, PartialEq, Eq)]
struct Request {
    context: Context,
    revision: i64,
}

#[derive(Default)]
struct Queue {
    active: Option<Request>,
    pending: Option<(Request, crate::karma_runtime::KarmaDeadlineDirectorConfig)>,
}

#[derive(Default)]
pub struct Metrics {
    pub snapshots: AtomicU64,
    pub steps: AtomicU64,
    pub hits: AtomicU64,
}

#[derive(Default)]
pub struct Controller {
    queue: Arc<Mutex<Queue>>,
    task: Mutex<Option<tokio::task::JoinHandle<()>>>,
    expiry: Arc<Mutex<Option<(i64, tokio::task::JoinHandle<()>)>>>,
    pub metrics: Arc<Metrics>,
}

impl Drop for Controller {
    fn drop(&mut self) {
        if let Some(task) = self.task.get_mut().expect("projection task").take() {
            task.abort();
        }
        if let Some((_, task)) = self.expiry.lock().expect("projection expiry").take() {
            task.abort();
        }
    }
}

impl Engine {
    pub async fn request_projection(self: &Arc<Self>, context: Context) -> Result<(), EngineError> {
        context
            .window
            .validate()
            .map_err(EngineError::Consequence)?;
        let now = nucleus::execution::now().timestamp_millis();
        if context.actor.is_some()
            || context.window.until_ms <= now
            || context.window.until_ms.saturating_sub(now) > 366 * 86_400_000
        {
            return Ok(());
        }
        let config = match self.configured_karma_runtime() {
            Ok(config) => config,
            Err(_) => {
                crate::karma_runtime::KarmaDeadlineDirectorConfig::for_host("projection".into())?
            }
        };
        store::projection::set_runtime(&self.store.pool, config.projection_identity()?.as_str())
            .await?;
        if let Some(cached) = store::projection::read(&self.store.pool, &context, now).await? {
            arm_expiry(
                &self.projection.expiry,
                cached.expires_ms,
                self.query_changed.clone(),
            );
            self.projection.metrics.hits.fetch_add(1, Ordering::Relaxed);
            return Ok(());
        }
        let mut context = context;
        context.window.from_ms = context.window.from_ms.max(now);
        if let Some((from, until)) =
            store::projection::covered_window(&self.store.pool, &context, now).await?
        {
            extend_window(&mut context.window, from, until);
        }
        let mut request = Request {
            context,
            revision: store::projection::revision(&self.store.pool).await?,
        };
        let mut queue = self.projection.queue.lock().expect("projection queue");
        if queue
            .active
            .as_ref()
            .is_some_and(|active| covers(active, &request))
            || queue
                .pending
                .as_ref()
                .is_some_and(|(pending, _)| covers(pending, &request))
        {
            return Ok(());
        }
        if queue.active.is_some() {
            for previous in queue
                .active
                .iter()
                .chain(queue.pending.iter().map(|(pending, _)| pending))
            {
                if previous.revision == request.revision
                    && previous.context.actor == request.context.actor
                {
                    extend_window(
                        &mut request.context.window,
                        previous.context.window.from_ms,
                        previous.context.window.until_ms,
                    );
                }
            }
            queue.pending = Some((request, config));
            return Ok(());
        }
        queue.active = Some(request.clone());
        let queue = self.projection.queue.clone();
        let store = self.store.clone();
        let changed = self.query_changed.clone();
        let metrics = self.projection.metrics.clone();
        let expiry = self.projection.expiry.clone();
        let task = tokio::spawn(async move {
            let mut request = request;
            let mut config = config;
            loop {
                let base = nucleus::execution::now().timestamp_millis();
                let computed = calculate(
                    &store,
                    &request.context,
                    base,
                    Some(config.clone()),
                    &metrics,
                )
                .await;
                match computed {
                    Ok(calculated) => {
                        if calculated.source_revision == request.revision {
                            match store::projection::publish(
                                &store.pool,
                                &request.context,
                                calculated.source_revision,
                                base,
                                calculated.expires_ms,
                                calculated.incomplete.as_ref(),
                                &calculated.spans,
                            )
                            .await
                            {
                                Ok(true) => {
                                    arm_expiry(&expiry, calculated.expires_ms, changed.clone())
                                }
                                Ok(false) => {}
                                Err(error) => {
                                    tracing::warn!(%error, "projection publication failed")
                                }
                            }
                        }
                    }
                    Err(error) => {
                        tracing::warn!(%error, "projection calculation failed");
                        let _ = store::projection::publish(
                            &store.pool,
                            &request.context,
                            request.revision,
                            base,
                            base + 60_000,
                            Some(&Incomplete::UnavailableRuntime {}),
                            &[],
                        )
                        .await;
                        arm_expiry(&expiry, base + 60_000, changed.clone());
                    }
                }
                changed.send_modify(|revision| *revision = revision.wrapping_add(1));
                let next = {
                    let mut queue = queue.lock().expect("projection queue");
                    let next = queue.pending.take();
                    queue.active = next.as_ref().map(|(request, _)| request.clone());
                    next
                };
                match next {
                    Some((next, next_config)) => {
                        request = next;
                        config = next_config;
                    }
                    None => break,
                }
            }
        });
        *self.projection.task.lock().expect("projection task") = Some(task);
        Ok(())
    }
}

fn covers(existing: &Request, requested: &Request) -> bool {
    existing.revision == requested.revision
        && existing.context.actor == requested.context.actor
        && existing.context.window.from_ms <= requested.context.window.from_ms
        && existing.context.window.until_ms >= requested.context.window.until_ms
}

fn extend_window(window: &mut nucleus::projection::Window, from_ms: i64, until_ms: i64) {
    window.until_ms = window.until_ms.max(until_ms);
    window.from_ms = window
        .from_ms
        .min(from_ms)
        .max(window.until_ms - 366 * 86_400_000);
}

pub struct Calculated {
    pub source_revision: i64,
    pub expires_ms: i64,
    pub incomplete: Option<Incomplete>,
    pub spans: Vec<Span>,
}

fn arm_expiry(
    expiry: &Arc<Mutex<Option<(i64, tokio::task::JoinHandle<()>)>>>,
    deadline: i64,
    changed: tokio::sync::watch::Sender<u64>,
) {
    let mut expiry = expiry.lock().expect("projection expiry");
    if expiry
        .as_ref()
        .is_some_and(|(at, task)| *at == deadline && !task.is_finished())
    {
        return;
    }
    if let Some((_, task)) = expiry.take() {
        task.abort();
    }
    let task = tokio::spawn(async move {
        loop {
            let remaining = deadline.saturating_sub(nucleus::execution::now().timestamp_millis());
            if remaining <= 0 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(
                remaining.min(60_000) as u64
            ))
            .await;
        }
        changed.send_modify(|revision| *revision = revision.wrapping_add(1));
    });
    *expiry = Some((deadline, task));
}

pub async fn calculate(
    source: &store::Store,
    context: &Context,
    base_ms: i64,
    config: Option<crate::karma_runtime::KarmaDeadlineDirectorConfig>,
    metrics: &Metrics,
) -> Result<Calculated, EngineError> {
    context
        .window
        .validate()
        .map_err(EngineError::Consequence)?;
    let revision = store::projection::revision(&source.pool).await?;
    let mut result = Calculated {
        source_revision: revision,
        expires_ms: base_ms + 86_400_000,
        incomplete: None,
        spans: Vec::new(),
    };
    if context.actor.is_some() {
        result.incomplete = Some(Incomplete::UnavailableRuntime {});
        return Ok(result);
    }
    if context.window.until_ms <= base_ms {
        result.incomplete = Some(Incomplete::PastWindow {});
        return Ok(result);
    }
    if context.window.until_ms.saturating_sub(base_ms) > 366 * 86_400_000 {
        result.incomplete = Some(Incomplete::Budget {});
        return Ok(result);
    }
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("projection.sqlite");
    metrics.snapshots.fetch_add(1, Ordering::Relaxed);
    source.snapshot_into(&path).await?;
    let private =
        store::Store::open_existing_durable(&format!("sqlite://{}", path.display())).await?;
    result.source_revision = store::projection::revision(&private.pool).await?;
    let key = context
        .key()
        .map_err(|error| EngineError::Consequence(error.to_string()))?;
    let entropy = nucleus::fact::sha256_hex(
        format!("{key:?}:{}:{base_ms}", result.source_revision).as_bytes(),
    );
    let seed = std::array::from_fn(|index| {
        u8::from_str_radix(&entropy[index * 2..index * 2 + 2], 16).unwrap()
    });
    let execution =
        Execution::new(seed, base_ms).map_err(|error| EngineError::Consequence(error.into()))?;
    let outcome = execution.scope(async {
        let engine = Engine::new(private.clone()).await?;
        let cell = store::cells::local(&private.pool).await?.ok_or_else(|| EngineError::Consequence("projection has no Cell".into()))?;
        let organ = store::organs::local(&private.pool).await?.ok_or_else(|| EngineError::Consequence("projection has no Organ".into()))?;
        let signer = crate::trust::Signer::from_bytes(&organ.uid, "projection", seed);
        engine.set_organ_signer(signer.clone()).await?;
        engine.set_signer(signer).await?;
        let config = match config { Some(config) => config, None => crate::karma_runtime::KarmaDeadlineDirectorConfig::for_host(format!("{}:projection", cell.uid))? };
        engine.install_karma_runtime_config(config)?;
        for grant in store::karma::grants::list_revisions(&private.pool).await? {
            for boundary in [grant.revision.spec.valid_from.as_millis(), grant.revision.spec.expires_at.as_millis()] {
                if boundary > base_ms { result.expires_ms = result.expires_ms.min(boundary); }
            }
        }
        let mut position: i64 = store::sqlx::query_scalar("SELECT COALESCE(MAX(rowid), 0) FROM fact").fetch_one(&private.pool).await?;
        let application_position: i64 = store::sqlx::query_scalar("SELECT COALESCE(MAX(rowid), 0) FROM karma_rule_application").fetch_one(&private.pool).await?;
        let program_position: i64 = store::sqlx::query_scalar("SELECT COALESCE(MAX(rowid), 0) FROM karma_run").fetch_one(&private.pool).await?;
        let mut active = BTreeMap::new();
        let mut changed = std::collections::BTreeSet::new();
        for record in store::records::list_all(&private.pool).await? {
            let span = Span {
                id: format!("initial:{}", record.uid),
                record: TypedUid::new(ReferenceKind::Record, record.uid.clone()).map_err(boundary)?,
                from_ms: base_ms.max(context.window.from_ms), until_ms: context.window.until_ms,
                quantity: Quantity { value: record.quantity, unit: record.unit_uid.map(|uid| TypedUid::new(ReferenceKind::Unit, uid)).transpose().map_err(boundary)? }, cause: Cause::Seed {},
            };
            active.insert(record.uid, span);
        }
        if active.len() > MAX_SPANS { result.incomplete = Some(Incomplete::Budget {}); return Ok(()); }
        let mut now = base_ms;
        let mut complete = false;
        for _ in 0..MAX_STEPS {
            execution.set_time(now).map_err(|error| EngineError::Consequence(error.into()))?;
            metrics.steps.fetch_add(1, Ordering::Relaxed);
            let step = engine.step_karma_time(execution.now(), 1).await?;
            let database_effects = engine.run_database_effects().await?;
            loop {
                let facts = store::facts::after_position(&private.pool, position, 256).await?;
                if facts.is_empty() { break; }
                for (cursor, fact) in facts {
                    position = cursor;
                    if fact.delta.is_zero() { continue; }
                    if !fact.delta.is_zero() { changed.insert(fact.record_uid.clone()); }
                    let old = active.remove(&fact.record_uid);
                    let quantity = match &old { Some(span) => Quantity { value: store::exact::sum_exact([span.quantity.value, fact.delta])?, unit: span.quantity.unit.clone() }, None => Quantity { value: fact.delta, unit: None } };
                    if let Some(mut span) = old {
                        span.until_ms = now;
                        if span.until_ms > span.from_ms { result.spans.push(span); }
                    }
                    let cause = if fact.cause.kind == nucleus::CauseKind::Rule {
                        let payload: RuleCommit = serde_json::from_str(fact.payload.as_deref().ok_or_else(|| EngineError::Consequence("missing rule occurrence".into()))?).map_err(EngineError::Json)?;
                        let frequency: Option<String> = store::sqlx::query_scalar("SELECT frequency_uid FROM karma_rule_application WHERE event_id = ? AND rule_uid = ? AND rule_revision = ?").bind(&payload.event).bind(&payload.rule).bind(payload.revision as i64).fetch_one(&private.pool).await?;
                        Cause::Rule { occurrence: RuleOccurrence { rule_uid: payload.rule, revision: payload.revision, event_id: payload.event, frequency: frequency.map(|uid| TypedUid::new(ReferenceKind::Frequency, uid)).transpose().map_err(boundary)?, intended_at_ms: Some(chrono::DateTime::parse_from_rfc3339(&payload.intended_at).map_err(|error| EngineError::Consequence(error.to_string()))?.timestamp_millis()) }, consequence: payload.consequence }
                    } else { entry_cause(&private.pool, &fact.uid).await?.unwrap_or(Cause::Fact { cause_kind: fact.cause.kind, uid: fact.cause.uid }) };
                    active.insert(fact.record_uid.clone(), Span { id: fact.uid, record: TypedUid::new(ReferenceKind::Record, fact.record_uid).map_err(boundary)?, from_ms: now.max(context.window.from_ms), until_ms: context.window.until_ms, quantity, cause });
                }
                if result.spans.len() + active.len() > MAX_SPANS { result.incomplete = Some(Incomplete::Budget {}); break; }
            }
            if result.incomplete.is_some() { break; }
            let blocked: bool = store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM karma_rule_application WHERE rowid > ? AND status = 'failed') OR EXISTS(SELECT 1 FROM karma_run WHERE rowid > ? AND status NOT IN ('succeeded', 'not-applicable'))").bind(application_position).bind(program_position).fetch_one(&private.pool).await?;
            let effects: bool = store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM transfer_delivery_outbox WHERE status IN ('queued', 'failed')) OR EXISTS(SELECT 1 FROM karma_intent_state WHERE status = 'authorized')").fetch_one(&private.pool).await?;
            let effects = effects || database_effects.unsupported;
            if blocked || effects || database_effects.outcomes.iter().any(|effect| !effect.ok) { result.incomplete = Some(if effects { Incomplete::ExternalEffects {} } else { Incomplete::RuleFailure {} }); break; }
            if database_effects.pending { continue; }
            match step.next_at_ms {
                Some(next) if next <= context.window.until_ms => {
                    if next > base_ms { result.expires_ms = result.expires_ms.min(next); }
                    now = next.max(now);
                }
                _ => { complete = true; break; }
            }
        }
        if !complete && result.incomplete.is_none() { result.incomplete = Some(Incomplete::Budget {}); }
        for mut span in active.into_values() {
            if !complete { span.until_ms = now; }
            if span.from_ms < span.until_ms && result.spans.len() < MAX_SPANS { result.spans.push(span); }
        }
        result.spans.retain(|span| changed.contains(span.record.as_str()));
        Ok::<_, EngineError>(())
    }).await;
    private.pool.close().await;
    outcome?;
    Ok(result)
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleCommit {
    rule: String,
    revision: u64,
    event: String,
    intended_at: String,
    consequence: u32,
}

fn boundary(error: nucleus::karma::KarmaBoundaryError) -> EngineError {
    EngineError::Consequence(error.to_string())
}

pub async fn entry_cause(
    pool: &store::sqlx::SqlitePool,
    fact: &str,
) -> Result<Option<Cause>, EngineError> {
    let row: Option<(String, String, i64, String, Option<String>, String)> = store::sqlx::query_as(
        "SELECT a.event_id, a.rule_uid, a.rule_revision, a.intended_at, a.frequency_uid, e.request_id
         FROM entry_revision r JOIN effect_queue e ON e.request_id = r.request_id
         JOIN karma_rule_application a ON a.rule_uid = json_extract(e.payload, '$.rule')
             AND a.rule_revision = json_extract(e.payload, '$.revision')
         WHERE r.fact_uid = ? AND substr(e.request_id, 1, length(a.event_id || ':' || a.rule_uid || ':' || a.rule_revision || ':'))
             = a.event_id || ':' || a.rule_uid || ':' || a.rule_revision || ':'"
    ).bind(fact).fetch_optional(pool).await?;
    row.map(
        |(event_id, rule_uid, revision, intended_at, frequency, request)| {
            let invalid = |error: String| EngineError::Consequence(error);
            let consequence = request
                .rsplit(':')
                .next()
                .unwrap_or("")
                .parse::<u32>()
                .map_err(|error| invalid(error.to_string()))?;
            Ok(Cause::Rule {
                occurrence: RuleOccurrence {
                    event_id,
                    rule_uid,
                    revision: revision
                        .try_into()
                        .map_err(|error: std::num::TryFromIntError| invalid(error.to_string()))?,
                    intended_at_ms: Some(
                        chrono::DateTime::parse_from_rfc3339(&intended_at)
                            .map_err(|error| invalid(error.to_string()))?
                            .timestamp_millis(),
                    ),
                    frequency: frequency
                        .map(|uid| TypedUid::new(ReferenceKind::Frequency, uid))
                        .transpose()
                        .map_err(boundary)?,
                },
                consequence,
            })
        },
    )
    .transpose()
}
