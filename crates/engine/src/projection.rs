use std::collections::BTreeMap;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};

use nucleus::execution::Execution;
use nucleus::karma::{ReferenceKind, TypedUid};
use nucleus::projection::{Context, Incomplete, MAX_SPANS, MAX_STEPS, Scheduled, Span};
use nucleus::simulation::{Cause, Quantity, RuleOccurrence};

use crate::{Engine, EngineError};

mod schedule;

pub fn execution_identity() -> &'static nucleus::projection::ExecutionIdentity {
    static IDENTITY: std::sync::LazyLock<nucleus::projection::ExecutionIdentity> =
        std::sync::LazyLock::new(|| {
            nucleus::projection::ExecutionIdentity::parse(env!("LINCE_EXECUTION_BUILD_HASH"))
                .expect("execution build identity")
        });
    &IDENTITY
}

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

fn finish_request(
    queue: &Mutex<Queue>,
    superseded: bool,
    changed: &tokio::sync::watch::Sender<crate::QueryChanges>,
) -> Option<(Request, crate::karma_runtime::KarmaDeadlineDirectorConfig)> {
    let next = {
        let mut queue = queue.lock().expect("projection queue");
        let next = queue.pending.take();
        queue.active = next.as_ref().map(|(request, _)| request.clone());
        next
    };
    if superseded && next.is_some() {
        crate::performance::add("schedule-projection-notifications-coalesced", 1);
    } else {
        crate::performance::add("schedule-projection-notifications", 1);
        changed.send_modify(|revision| revision.projection = revision.projection.wrapping_add(1));
    }
    next
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
    context: Mutex<Option<Context>>,
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
    pub(crate) async fn refresh_projection(&self) -> Result<(), EngineError> {
        let context = self
            .projection
            .context
            .lock()
            .expect("projection context")
            .clone();
        if let Some(context) = context {
            self.request_projection(context).await?;
        }
        Ok(())
    }

    pub async fn request_projection(&self, context: Context) -> Result<(), EngineError> {
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
        {
            let mut previous = self.projection.context.lock().expect("projection context");
            let mut retained = context.clone();
            if let Some(previous) = previous.as_ref()
                && previous.actor == context.actor
                && previous.window.timezone == context.window.timezone
                && previous.window.until_ms > now
            {
                extend_window(
                    &mut retained.window,
                    previous.window.from_ms,
                    previous.window.until_ms,
                );
            }
            *previous = Some(retained);
        }
        let config = match self.configured_karma_runtime() {
            Ok(config) => config,
            Err(_) => {
                crate::karma_runtime::KarmaDeadlineDirectorConfig::for_host("projection".into())?
            }
        };
        store::projection::set_runtime(&self.store.pool, config.projection_identity()?.as_str())
            .await?;
        if let Some(cached) = store::projection::read(&self.store, &context, now).await? {
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
            store::projection::covered_window(&self.store, &context, now).await?
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
        let execution = nucleus::execution::current();
        let task = tokio::spawn(async move {
            let work = async move {
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
                    let mut superseded = false;
                    match computed {
                        Ok(calculated) => {
                            if calculated.source_revision == request.revision {
                                match store::projection::publish_schedule(
                                    &store,
                                    &request.context,
                                    calculated.source_revision,
                                    base,
                                    calculated.expires_ms,
                                    calculated.incomplete.as_ref(),
                                    &calculated.spans,
                                    &calculated.schedule,
                                )
                                .await
                                {
                                    Ok(true) => {
                                        crate::performance::add("schedule-projection-publications", 1);
                                        arm_expiry(&expiry, calculated.expires_ms, changed.clone())
                                    }
                                    Ok(false) => {
                                        crate::performance::add("schedule-projection-superseded", 1);
                                        crate::performance::add("schedule-projection-publication-superseded", 1);
                                        superseded = true;
                                    }
                                    Err(error) => {
                                        tracing::warn!(%error, "projection publication failed")
                                    }
                                }
                            } else {
                                crate::performance::add("schedule-projection-superseded", 1);
                                crate::performance::add("schedule-projection-snapshot-superseded", 1);
                                superseded = true;
                            }
                        }
                        Err(error) => {
                            tracing::warn!(%error, "projection calculation failed");
                            let _ = store::projection::publish(
                                &store,
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
                    let next = finish_request(&queue, superseded, &changed);
                    match next {
                        Some((next, next_config)) => {
                            request = next;
                            config = next_config;
                        }
                        None => break,
                    }
                }
            };
            if let Some(execution) = execution {
                execution.scope(work).await;
            } else {
                work.await;
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
    pub schedule: Vec<Scheduled>,
}

fn arm_expiry(
    expiry: &Arc<Mutex<Option<(i64, tokio::task::JoinHandle<()>)>>>,
    deadline: i64,
    changed: tokio::sync::watch::Sender<crate::QueryChanges>,
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
        crate::performance::add("schedule-projection-notifications", 1);
        changed.send_modify(|revision| revision.projection = revision.projection.wrapping_add(1));
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
    let _calculation = crate::performance::Span::new("schedule-projection-calculation-await");
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
        schedule: Vec::new(),
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
    crate::performance::add("schedule-projection-snapshots", 1);
    let snapshot = crate::performance::Span::new("schedule-projection-snapshot-await");
    source.snapshot_into(&path).await?;
    drop(snapshot);
    let open = crate::performance::Span::new("schedule-projection-open-snapshot-await");
    let private =
        store::Store::open_existing_durable(&format!("sqlite://{}", path.display())).await?;
    drop(open);
    result.source_revision = store::projection::revision(&private.pool).await?;
    let key = context
        .key(execution_identity())
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
        let setup = crate::performance::Span::new("schedule-projection-runtime-setup-await");
        let engine = Engine::new(private.clone()).await?;
        let cell = store::cells::local(&private.pool).await?.ok_or_else(|| EngineError::Consequence("projection has no Cell".into()))?;
        let organ = store::organs::local(&private.pool).await?.ok_or_else(|| EngineError::Consequence("projection has no Organ".into()))?;
        let signer = crate::trust::Signer::from_bytes(&organ.uid, "projection", seed);
        engine.set_organ_signer(signer.clone()).await?;
        engine.set_signer(signer).await?;
        let config = match config { Some(config) => config, None => crate::karma_runtime::KarmaDeadlineDirectorConfig::for_host(format!("{}:projection", cell.uid))? };
        engine.install_karma_runtime_config(config)?;
        drop(setup);
        let preparation = crate::performance::Span::new("schedule-projection-prepare-await");
        let mut position: i64 = store::sqlx::query_scalar("SELECT COALESCE(MAX(rowid), 0) FROM fact").fetch_one(&private.pool).await?;
        let application_position: i64 = store::sqlx::query_scalar("SELECT COALESCE(MAX(rowid), 0) FROM karma_rule_application").fetch_one(&private.pool).await?;
        let mut schedule_position = application_position;
        let existing_records: std::collections::HashSet<String> = store::sqlx::query_scalar("SELECT uid FROM record WHERE deleted_at IS NULL").fetch_all(&private.pool).await?.into_iter().collect();
        schedule::manual(&private.pool, context, &existing_records, &mut result.schedule, &std::collections::HashMap::new()).await?;
        schedule::occurrences(&private.pool, context, 0, &existing_records, &mut result.schedule).await?;
        let program_position: i64 = store::sqlx::query_scalar("SELECT COALESCE(MAX(rowid), 0) FROM karma_run").fetch_one(&private.pool).await?;
        let mut active = BTreeMap::new();
        let mut changed = std::collections::BTreeSet::new();
        let mut loan_offsets = BTreeMap::new();
        for record in store::records::list_all(&private.pool).await? {
            let span = Span {
                id: format!("initial:{}", record.uid),
                record: TypedUid::new(ReferenceKind::Record, record.uid.clone()).map_err(boundary)?,
                from_ms: base_ms.max(context.window.from_ms), until_ms: context.window.until_ms,
                quantity: Quantity { value: record.quantity, unit: record.unit_uid.map(|uid| TypedUid::new(ReferenceKind::Unit, uid)).transpose().map_err(boundary)? }, cause: Cause::Seed {},
            };
            active.insert(record.uid, span);
        }
        drop(preparation);
        if active.len() > MAX_SPANS { result.incomplete = Some(Incomplete::Budget {}); return Ok(()); }
        let mut now = base_ms;
        let mut complete = false;
        for _ in 0..MAX_STEPS {
            let _step = crate::performance::Span::new("schedule-projection-simulation-step-await");
            execution.set_time(now).map_err(|error| EngineError::Consequence(error.into()))?;
            if !engine.karma_device_execution().await?.executing { result.incomplete = Some(Incomplete::UnavailableRuntime {}); break; }
            metrics.steps.fetch_add(1, Ordering::Relaxed);
            let step = engine.step_karma_time(execution.now(), 1).await?;
            let database_effects = engine.run_database_effects().await?;
            schedule_position = schedule::occurrences(&private.pool, context, schedule_position, &existing_records, &mut result.schedule).await?;
            if result.schedule.len() >= MAX_SPANS { result.incomplete = Some(Incomplete::Budget {}); break; }
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
            let loans = store::transfer_loans::adjustments(&private.pool, now).await?;
            let next_loan = loans.iter().filter_map(|loan| loan.next_ms).min();
            let mut current_offsets = BTreeMap::new();
            for loan in loans {
                if loan.unit_changed { result.incomplete = Some(Incomplete::UnsupportedUnit {}); continue; }
                current_offsets.insert(loan.record.clone(),loan.delta);
                let delta = store::exact::difference(loan.delta,loan_offsets.get(&loan.record).copied().unwrap_or_else(store::exact::zero))?;
                if delta.is_zero() {continue;}
                if let Some(mut old) = active.remove(&loan.record) {
                    let mut next = old.clone();
                    next.id = format!("loan:{}:{now}",loan.record);
                    next.from_ms = now.max(context.window.from_ms);
                    next.quantity.value = store::exact::sum_exact([next.quantity.value,delta])?;
                    next.cause = Cause::Timer {};
                    old.until_ms = now;
                    if old.until_ms > old.from_ms {result.spans.push(old);}
                    changed.insert(loan.record.clone());
                    active.insert(loan.record,next);
                }
            }
            loan_offsets = current_offsets;
            if result.incomplete.is_some() { break; }
            let blocked: bool = store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM karma_rule_application WHERE rowid > ? AND status = 'failed') OR EXISTS(SELECT 1 FROM karma_run WHERE rowid > ? AND status NOT IN ('succeeded', 'not-applicable'))").bind(application_position).bind(program_position).fetch_one(&private.pool).await?;
            let effects: bool = store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM transfer_delivery_outbox WHERE status IN ('queued', 'failed'))").fetch_one(&private.pool).await?;
            let effects = effects || database_effects.unsupported;
            if blocked || effects || database_effects.outcomes.iter().any(|effect| !effect.ok) { result.incomplete = Some(if effects { Incomplete::ExternalEffects {} } else { Incomplete::RuleFailure {} }); break; }
            if database_effects.pending { continue; }
            match step.next_at_ms.into_iter().chain(next_loan).min() {
                Some(next) if next <= context.window.until_ms => {
                    if next > base_ms { result.expires_ms = result.expires_ms.min(next); }
                    now = next.max(now);
                }
                _ => { complete = true; break; }
            }
        }
        if !complete && result.incomplete.is_none() { result.incomplete = Some(Incomplete::Budget {}); }
        let schedule_origins = active.iter().filter(|(uid, _)| !existing_records.contains(*uid)).map(|(uid, span)| (uid.clone(), span.cause.clone())).collect();
        for mut span in active.into_values() {
            if !complete { span.until_ms = now; }
            if span.from_ms < span.until_ms && result.spans.len() < MAX_SPANS { result.spans.push(span); }
        }
        result.spans.retain(|span| changed.contains(span.record.as_str()));
        schedule::manual(&private.pool, context, &existing_records, &mut result.schedule, &schedule_origins).await?;
        result.schedule.sort_by(|a, b| a.id.cmp(&b.id));
        result.schedule.dedup_by(|a, b| a.id == b.id);
        Ok::<_, EngineError>(())
    }).await;
    let close = crate::performance::Span::new("schedule-projection-close-snapshot-await");
    private.pool.close().await;
    drop(close);
    outcome?;
    result
        .spans
        .sort_by(|a, b| a.from_ms.cmp(&b.from_ms).then_with(|| a.id.cmp(&b.id)));
    result.schedule.sort_by(|a, b| {
        a.time
            .from_ms
            .cmp(&b.time.from_ms)
            .then_with(|| a.id.cmp(&b.id))
    });
    let mut bytes = 0;
    let spans_limited = retain_payload_budget(&mut result.spans, &mut bytes)?;
    let schedule_limited = retain_payload_budget(&mut result.schedule, &mut bytes)?;
    if spans_limited || schedule_limited {
        result.incomplete = Some(Incomplete::Budget {});
    }
    Ok(result)
}

fn retain_payload_budget<T: serde::Serialize>(
    entries: &mut Vec<T>,
    bytes: &mut usize,
) -> Result<bool, EngineError> {
    let mut limited = false;
    let mut error = None;
    entries.retain(|entry| match serde_json::to_vec(entry) {
        Ok(encoded)
            if encoded.len() <= nucleus::projection::MAX_CACHE_BYTES.saturating_sub(*bytes) =>
        {
            *bytes += encoded.len();
            true
        }
        Ok(_) => {
            limited = true;
            false
        }
        Err(failure) => {
            error = Some(failure);
            false
        }
    });
    if let Some(error) = error {
        return Err(EngineError::Json(error));
    }
    Ok(limited)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn changing_sources_eventually_publish_a_current_reusable_forecast() {
        tokio::time::timeout(std::time::Duration::from_secs(30), async {
            let engine = Engine::open_memory().await.unwrap();
            let now = nucleus::execution::now().timestamp_millis();
            let context = Context {
                actor: None,
                window: nucleus::projection::Window {
                    from_ms: now,
                    until_ms: now + 86_400_000,
                    timezone: "UTC".into(),
                },
            };
            let mut changes = engine.watch_query_changes();
            for _ in 0..3 {
                store::sqlx::query("UPDATE projection_source SET revision = revision + 1 WHERE id = 1")
                    .execute(&engine.store.pool).await.unwrap();
                engine.request_projection(context.clone()).await.unwrap();
            }
            loop {
                changes.changed().await.unwrap();
                if store::projection::read(
                    &engine.store,
                    &context,
                    nucleus::execution::now().timestamp_millis(),
                )
                .await
                .unwrap()
                .is_some()
                {
                    break;
                }
                engine.request_projection(context.clone()).await.unwrap();
            }
            let snapshots = engine.projection.metrics.snapshots.load(Ordering::Relaxed);
            let hits = engine.projection.metrics.hits.load(Ordering::Relaxed);
            assert!(snapshots > 0);
            engine.request_projection(context).await.unwrap();
            assert_eq!(engine.projection.metrics.snapshots.load(Ordering::Relaxed), snapshots);
            assert_eq!(engine.projection.metrics.hits.load(Ordering::Relaxed), hits + 1);
            engine.store.pool.close().await;
        })
        .await
        .expect("The final forecast must become current without polling or another source edit");
    }

    fn request(revision: i64) -> Request {
        Request {
            revision,
            context: Context {
                actor: None,
                window: nucleus::projection::Window {
                    from_ms: 1_800_000_000_000,
                    until_ms: 1_800_086_400_000,
                    timezone: "UTC".into(),
                },
            },
        }
    }

    #[test]
    fn superseded_replacements_progress_without_refreshing_unchanged_results() {
        let queue = Mutex::new(Queue {
            active: Some(request(1)),
            pending: None,
        });
        let (changed, mut subscriber) = tokio::sync::watch::channel(crate::QueryChanges::default());
        for revision in 2..=4 {
            let mut next = request(revision);
            next.context.window.until_ms += revision * 60_000;
            next.context.window.timezone = "America/Sao_Paulo".into();
            let config = crate::karma_runtime::KarmaDeadlineDirectorConfig::for_host(
                format!("forecast-{revision}"),
            )
            .unwrap();
            let identity = config.projection_identity().unwrap();
            queue.lock().unwrap().pending = Some((next.clone(), config));
            let (running, config) = finish_request(&queue, true, &changed).unwrap();
            assert!(running == next);
            assert_eq!(config.projection_identity().unwrap(), identity);
            assert!(queue.lock().unwrap().active.as_ref() == Some(&next));
            assert!(queue.lock().unwrap().pending.is_none());
            assert!(!subscriber.has_changed().unwrap());
        }
        assert!(finish_request(&queue, false, &changed).is_none());
        assert!(queue.lock().unwrap().active.is_none());
        assert!(subscriber.has_changed().unwrap());
        assert_eq!(*subscriber.borrow_and_update(), crate::QueryChanges { general: 0, projection: 1 });
    }

    #[test]
    fn last_superseded_request_wakes_subscribers_for_a_fresh_retry() {
        let queue = Mutex::new(Queue {
            active: Some(request(1)),
            pending: None,
        });
        let (changed, mut subscriber) = tokio::sync::watch::channel(crate::QueryChanges::default());
        assert!(finish_request(&queue, true, &changed).is_none());
        assert!(queue.lock().unwrap().active.is_none());
        assert!(subscriber.has_changed().unwrap());
        assert_eq!(*subscriber.borrow_and_update(), crate::QueryChanges { general: 0, projection: 1 });
        queue.lock().unwrap().active = Some(request(2));
        assert!(finish_request(&queue, false, &changed).is_none());
        assert!(subscriber.has_changed().unwrap());
        assert_eq!(*subscriber.borrow_and_update(), crate::QueryChanges { general: 0, projection: 2 });
    }

    #[test]
    fn non_superseded_results_notify_while_replacement_work_remains() {
        let config = crate::karma_runtime::KarmaDeadlineDirectorConfig::for_host("next".into())
            .unwrap();
        let queue = Mutex::new(Queue {
            active: Some(request(1)),
            pending: Some((request(2), config)),
        });
        let (changed, mut subscriber) = tokio::sync::watch::channel(crate::QueryChanges::default());
        assert!(finish_request(&queue, false, &changed).is_some());
        assert!(queue.lock().unwrap().active.as_ref() == Some(&request(2)));
        assert!(subscriber.has_changed().unwrap());
        assert_eq!(*subscriber.borrow_and_update(), crate::QueryChanges { general: 0, projection: 1 });
        assert!(finish_request(&queue, false, &changed).is_none());
        assert!(subscriber.has_changed().unwrap());
        assert_eq!(*subscriber.borrow_and_update(), crate::QueryChanges { general: 0, projection: 2 });
    }

    #[tokio::test]
    async fn forecast_expiry_notifies_the_same_scoped_subscribers() {
        let expiry = Arc::new(Mutex::new(None));
        let (changed, mut subscriber) = tokio::sync::watch::channel(crate::QueryChanges::default());
        arm_expiry(&expiry, nucleus::execution::now().timestamp_millis() - 1, changed);
        tokio::time::timeout(std::time::Duration::from_secs(1), subscriber.changed())
            .await.unwrap().unwrap();
        assert_eq!(*subscriber.borrow_and_update(), crate::QueryChanges { general: 0, projection: 1 });
        let task = expiry.lock().unwrap().take().unwrap().1;
        task.await.unwrap();
    }

    #[test]
    fn projection_budget_counts_json_escaping_and_preserves_entries_that_fit() {
        let mut entries = vec![
            "\\".repeat(nucleus::projection::MAX_CACHE_BYTES / 2),
            "available".into(),
        ];
        let mut bytes = 0;
        assert!(retain_payload_budget(&mut entries, &mut bytes).unwrap());
        assert_eq!(entries, vec!["available"]);
        assert_eq!(bytes, serde_json::to_vec("available").unwrap().len());
        let mut entries = vec!["fits".to_owned()];
        assert!(!retain_payload_budget(&mut entries, &mut bytes).unwrap());
        assert_eq!(entries, vec!["fits"]);
        assert!(bytes <= nucleus::projection::MAX_CACHE_BYTES);
    }
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
