//! Karma 2.0 (blueprint Part VI): the reactive scheduler.
//!
//! Rules are loaded into an in-memory registry; the dependency graph is derived
//! from parsed condition tokens (VI.4). `cascade` re-evaluates only rules whose
//! inputs changed, executes consequences (each landing as a fact with
//! `cause = rule:<uid>`), and follows the newly-changed records — capped, so
//! user-authored loops terminate. Proof warnings (rule loops) come from SCCs
//! over reads∘writes at load time.

use chrono::{DateTime, TimeDelta, Utc};
use nucleus::{
    Cause, ConsequenceKind, ConsequenceSpec, Fact, MapResolver, NewFact, PromiseState, RuleDef,
    TokenKey,
};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use store::Store;

use crate::append::append_one;
use crate::error::EngineError;

/// Iteration cap per delivery (blueprint VI.4): user loops terminate, engine survives.
const CASCADE_CAP: usize = 256;
/// value(@rule) recursion cap.
const VALUE_DEPTH_CAP: usize = 8;

#[derive(Debug, Clone)]
pub struct ProofWarning {
    pub rules: Vec<String>,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct LoadedRule {
    pub def: RuleDef,
    /// Tokens with slugs resolved to record uids (rename-safe: both stored).
    pub tokens: Vec<ResolvedToken>,
    /// Record uids this rule's consequences write (for the Proof graph).
    pub writes: Vec<String>,
    pub active: bool,
}

#[derive(Debug, Clone)]
pub struct ResolvedToken {
    pub key: TokenKey,
    pub record_uid: Option<String>,
}

#[derive(Debug, Default, Clone)]
pub struct Registry {
    pub rules: Vec<LoadedRule>,
    /// record uid -> indices of rules that read it.
    pub readers: HashMap<String, Vec<usize>>,
    /// rule record uid -> index (for value(@rule) resolution).
    pub by_uid: HashMap<String, usize>,
    pub proof_warnings: Vec<ProofWarning>,
    /// rule uid -> last firing time, for debounce (VI.1). In-memory: resets on
    /// `reload_rules` (a reloaded rule may fire immediately once). Shared
    /// across registry clones (Arc) so snapshots see the same debounce state.
    pub last_fired: Arc<Mutex<HashMap<String, DateTime<Utc>>>>,
}

impl Registry {
    pub async fn load(store: &Store) -> Result<Registry, EngineError> {
        let rows = store::rules::load_all(&store.pool).await?;
        let mut registry = Registry::default();
        for row in rows {
            let def = match RuleDef::parse(
                row.record_uid.clone(),
                row.slug.clone(),
                &row.condition,
                &row.gate,
                &row.carry,
                row.debounce.as_deref(),
                row.consequences.clone(),
            ) {
                Ok(def) => def,
                Err(e) => {
                    registry.proof_warnings.push(ProofWarning {
                        rules: vec![row.record_uid.clone()],
                        message: format!("rule does not parse: {e}"),
                    });
                    continue;
                }
            };
            // resolve read tokens
            let mut tokens = Vec::new();
            for key in def.condition.tokens() {
                let record_uid = store::records::resolve(&store.pool, &key.slug)
                    .await?
                    .map(|r| r.uid);
                // tokens that don't name a single record: promises, concepts,
                // and multi-ref place functions resolve their own way
                let non_record = matches!(
                    key.func.as_str(),
                    "promise_state" | "confidence" | "demand" | "distance" | "route_eta"
                );
                if record_uid.is_none() && !non_record {
                    registry.proof_warnings.push(ProofWarning {
                        rules: vec![row.record_uid.clone()],
                        message: format!("token @{} does not resolve to a record", key.slug),
                    });
                }
                tokens.push(ResolvedToken { key, record_uid });
            }
            // resolve write targets
            let mut writes = Vec::new();
            for c in &def.consequences {
                if matches!(
                    c.kind,
                    ConsequenceKind::SetQuantity
                        | ConsequenceKind::AddQuantity
                        | ConsequenceKind::Activate
                        | ConsequenceKind::Deactivate
                ) {
                    if let Some(target) = &c.target {
                        if let Some(rec) =
                            store::records::resolve(&store.pool, target.trim_start_matches('@'))
                                .await?
                        {
                            writes.push(rec.uid);
                        }
                    }
                }
            }
            let idx = registry.rules.len();
            registry.by_uid.insert(def.uid.clone(), idx);
            registry.rules.push(LoadedRule {
                def,
                tokens,
                writes,
                active: row.active,
            });
            let _ = idx;
        }
        registry.build_readers();
        registry.proof(&mut Vec::new());
        Ok(registry)
    }

    /// Build the readers map with derived-value expansion: a rule consuming
    /// `value(@d)` transitively reads everything @d reads (fixpoint), so a
    /// change to the derived rule's inputs re-evaluates its consumers.
    fn build_readers(&mut self) {
        let n = self.rules.len();
        let mut reads: Vec<HashSet<String>> = vec![HashSet::new(); n];
        let mut value_deps: Vec<Vec<usize>> = vec![Vec::new(); n];
        for (i, rule) in self.rules.iter().enumerate() {
            for t in &rule.tokens {
                if let Some(uid) = &t.record_uid {
                    reads[i].insert(uid.clone());
                    if t.key.func == "value" {
                        if let Some(&j) = self.by_uid.get(uid) {
                            value_deps[i].push(j);
                        }
                    }
                }
            }
        }
        let mut stable = false;
        let mut rounds = 0;
        while !stable && rounds < n + 1 {
            stable = true;
            rounds += 1;
            for i in 0..n {
                for &j in &value_deps[i].clone() {
                    let extra: Vec<String> = reads[j].difference(&reads[i]).cloned().collect();
                    if !extra.is_empty() {
                        stable = false;
                        reads[i].extend(extra);
                    }
                }
            }
        }
        self.readers.clear();
        for (i, set) in reads.iter().enumerate() {
            for uid in set {
                self.readers.entry(uid.clone()).or_default().push(i);
            }
        }
        for readers in self.readers.values_mut() {
            readers.sort_unstable();
            readers.dedup();
        }
    }

    /// Proof (blueprint VI.4): SCC over rule reads∘writes -> loop warnings.
    fn proof(&mut self, _scratch: &mut Vec<String>) {
        let nodes: Vec<String> = self.rules.iter().map(|r| r.def.uid.clone()).collect();
        let mut edges: Vec<(String, String)> = Vec::new();
        for r1 in &self.rules {
            for written in &r1.writes {
                if let Some(readers) = self.readers.get(written) {
                    for &i in readers {
                        edges.push((r1.def.uid.clone(), self.rules[i].def.uid.clone()));
                    }
                }
            }
        }
        for comp in nucleus::graph::cycles(&nodes, &edges) {
            let named: Vec<String> = comp
                .iter()
                .map(|uid| {
                    self.by_uid
                        .get(uid)
                        .and_then(|&i| self.rules[i].def.slug.clone())
                        .unwrap_or_else(|| uid.clone())
                })
                .collect();
            self.proof_warnings.push(ProofWarning {
                message: format!(
                    "these {} rules form a loop: {}",
                    named.len(),
                    named.join(" -> ")
                ),
                rules: comp,
            });
        }
    }
}

/// Values injected by the caller for this delivery (freq firings during tick).
#[derive(Debug, Default)]
pub struct Injected {
    /// frequency record uid -> elapsed periods for this delivery.
    pub freq: HashMap<String, f64>,
}

/// Prefetch every token a rule needs, then evaluate pure (blueprint VI.2).
async fn prefetch(
    store: &Store,
    registry: &Registry,
    rule: &LoadedRule,
    injected: &Injected,
    now: DateTime<Utc>,
    depth: usize,
    visiting: &mut HashSet<String>,
) -> Result<MapResolver, EngineError> {
    let mut resolver = MapResolver::default();
    for t in &rule.tokens {
        let key = &t.key;
        let value: f64 = match key.func.as_str() {
            "quantity" | "signal" => match &t.record_uid {
                Some(uid) => store::records::quantity(&store.pool, uid)
                    .await?
                    .unwrap_or(0.0),
                None => return Err(EngineError::UnknownRecord(key.slug.clone())),
            },
            "freq" => t
                .record_uid
                .as_ref()
                .and_then(|uid| injected.freq.get(uid))
                .copied()
                .unwrap_or(0.0),
            "sum" | "sum_pos" | "sum_neg" => {
                let uid = t
                    .record_uid
                    .as_ref()
                    .ok_or_else(|| EngineError::UnknownRecord(key.slug.clone()))?;
                let window = key.dur_secs.ok_or_else(|| {
                    EngineError::Consequence(format!(
                        "{}(@{}) needs a duration literal (e.g. 30d)",
                        key.func, key.slug
                    ))
                })?;
                match key.func.as_str() {
                    "sum_pos" => {
                        store::facts::sum_pos_window(&store.pool, uid, window, now).await?
                    }
                    "sum_neg" => {
                        store::facts::sum_neg_window(&store.pool, uid, window, now).await?
                    }
                    _ => store::facts::sum_window(&store.pool, uid, window, now).await?,
                }
            }
            "value" => {
                // derived-value rule: evaluate its raw condition (no gate)
                let target_uid = t
                    .record_uid
                    .as_ref()
                    .ok_or_else(|| EngineError::UnknownRecord(key.slug.clone()))?;
                derived_value(store, registry, target_uid, injected, now, depth, visiting).await?
            }
            "promise_state" => store::misc::promise_state(&store.pool, &key.slug)
                .await?
                .map(PromiseState::ordinal)
                .unwrap_or(0.0),
            "hours_since_fact" => {
                let uid = t
                    .record_uid
                    .as_ref()
                    .ok_or_else(|| EngineError::UnknownRecord(key.slug.clone()))?;
                store::facts::hours_since_last(&store.pool, uid, now)
                    .await?
                    .unwrap_or(1.0e9)
            }
            // Imagination tokens (XII): pure over the store + registry.
            "confidence" => crate::imagination::confidence(store, &key.slug).await?,
            "projected" => {
                let dur = key.dur_secs.ok_or_else(|| {
                    EngineError::Consequence(format!(
                        "projected(@{}) needs a duration literal (e.g. +7d as 7d)",
                        key.slug
                    ))
                })?;
                let uid = t
                    .record_uid
                    .as_ref()
                    .ok_or_else(|| EngineError::UnknownRecord(key.slug.clone()))?;
                let snapshot = crate::imagination::build_snapshot(store, registry, now).await?;
                let timeline =
                    nucleus::imagination::project(&snapshot, now + chrono::TimeDelta::seconds(dur));
                timeline.projected(uid).unwrap_or(0.0)
            }
            // Place Instinct (IX): distance between two records' places.
            "distance" => {
                let mut places = Vec::new();
                for token in key.slug.split('|') {
                    let rec = store::records::resolve(&store.pool, token)
                        .await?
                        .ok_or_else(|| EngineError::UnknownRecord(token.to_string()))?;
                    let place = store::places::of_record(&store.pool, &rec.uid)
                        .await?
                        .ok_or_else(|| {
                            EngineError::Consequence(format!("record {token} has no place"))
                        })?;
                    places.push(place);
                }
                if places.len() != 2 {
                    return Err(EngineError::Consequence(
                        "distance() needs exactly two @records".into(),
                    ));
                }
                nucleus::place::distance(places[0], places[1])
            }
            // Imagination demand curve (XII.2): the current hour's share of the
            // trailing-30d activity on the concept's records. Deterministic
            // given `now`.
            "demand" => crate::imagination::demand(store, &key.slug, now).await?,
            // Pending: route_eta needs loaded OSM map data (blueprint IX).
            "route_eta" => {
                return Err(EngineError::Consequence(format!(
                    "{}() is specified (blueprint IX) but needs local map data",
                    key.func
                )));
            }
            other => {
                return Err(EngineError::Consequence(format!(
                    "unknown condition function {other}()"
                )));
            }
        };
        resolver.set(&key.func, &key.slug, key.dur_secs, value);
    }
    Ok(resolver)
}

/// value(@rule): the target rule's condition value, gate ignored (VI.3:
/// zero-consequence rules are named derived values — spreadsheet cells).
async fn derived_value(
    store: &Store,
    registry: &Registry,
    rule_record_uid: &str,
    injected: &Injected,
    now: DateTime<Utc>,
    depth: usize,
    visiting: &mut HashSet<String>,
) -> Result<f64, EngineError> {
    if depth >= VALUE_DEPTH_CAP || !visiting.insert(rule_record_uid.to_string()) {
        return Err(EngineError::Consequence(format!(
            "value() recursion too deep or cyclic at {rule_record_uid}"
        )));
    }
    let result = async {
        let &idx = registry
            .by_uid
            .get(rule_record_uid)
            .ok_or_else(|| EngineError::UnknownRecord(rule_record_uid.to_string()))?;
        let rule = &registry.rules[idx];
        let mut resolver = Box::pin(prefetch(
            store,
            registry,
            rule,
            injected,
            now,
            depth + 1,
            visiting,
        ))
        .await?;
        Ok::<f64, EngineError>(rule.def.condition.eval(&mut resolver)?)
    }
    .await;
    visiting.remove(rule_record_uid);
    result
}

/// The reactive delivery (blueprint VI.4): evaluate rules reading `changed`
/// records, execute firings, follow the records they changed. Capped.
pub async fn cascade(
    store: &Store,
    registry: &Registry,
    changed: Vec<String>,
    injected: &Injected,
    now: DateTime<Utc>,
    signer: Option<&crate::trust::Signer>,
) -> Result<Vec<Fact>, EngineError> {
    let mut committed: Vec<Fact> = Vec::new();
    let mut pending: VecDeque<String> = changed.into();
    let mut iterations = 0usize;

    while let Some(record_uid) = pending.pop_front() {
        let Some(reader_idxs) = registry.readers.get(&record_uid) else {
            continue;
        };
        for &idx in reader_idxs {
            iterations += 1;
            if iterations > CASCADE_CAP {
                // Loop cap reached: stop the delivery, keep the engine alive.
                return Ok(committed);
            }
            let rule = &registry.rules[idx];
            if rule.def.is_derived_value() {
                continue;
            }
            // Live activation: quantity is the universal enable, so an
            // activate/deactivate consequence takes effect on the very next
            // delivery — not at the next registry reload.
            let live_active = store::records::quantity(&store.pool, &rule.def.uid)
                .await?
                .unwrap_or(0.0)
                != 0.0;
            if !live_active {
                continue;
            }
            // Debounce (VI.1): a rule that fired at T holds until T+debounce.
            if let Some(debounce) = rule.def.debounce_secs {
                let held = registry
                    .last_fired
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .get(&rule.def.uid)
                    .is_some_and(|last| now < *last + TimeDelta::seconds(debounce));
                if held {
                    continue;
                }
            }
            let mut visiting = HashSet::new();
            let mut resolver =
                match prefetch(store, registry, rule, injected, now, 0, &mut visiting).await {
                    Ok(r) => r,
                    Err(_) => continue, // unresolvable rule: skip, never poison the delivery
                };
            let firing = match rule.def.evaluate(&mut resolver) {
                Ok(Some(f)) => f,
                Ok(None) | Err(_) => continue,
            };
            registry
                .last_fired
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .insert(rule.def.uid.clone(), now);
            let new_facts = execute(store, rule, firing.carried, now, signer).await?;
            for f in &new_facts {
                pending.push_back(f.record_uid.clone());
            }
            committed.extend(new_facts);
        }
    }
    Ok(committed)
}

/// Execute a firing's consequences (blueprint VI.3 table). Every fact lands
/// with `cause = rule:<uid>` — automation that can always answer "why".
async fn execute(
    store: &Store,
    rule: &LoadedRule,
    carried: f64,
    now: DateTime<Utc>,
    signer: Option<&crate::trust::Signer>,
) -> Result<Vec<Fact>, EngineError> {
    let mut out = Vec::new();
    let cause = Cause::rule(rule.def.uid.clone());
    for c in &rule.def.consequences {
        match c.kind {
            ConsequenceKind::SetQuantity
            | ConsequenceKind::AddQuantity
            | ConsequenceKind::Activate
            | ConsequenceKind::Deactivate => {
                let target = resolve_target(store, c).await?;
                let current = store::records::quantity(&store.pool, &target)
                    .await?
                    .unwrap_or(0.0);
                let delta = match c.kind {
                    ConsequenceKind::SetQuantity => carried - current,
                    ConsequenceKind::AddQuantity => carried,
                    ConsequenceKind::Activate => 1.0 - current,
                    ConsequenceKind::Deactivate => 0.0 - current,
                    _ => unreachable!(),
                };
                if delta == 0.0 {
                    continue; // no zero-facts from rules: keeps cascades quiet
                }
                if let Some(fact) = append_one(
                    store,
                    NewFact::quantity(target, delta, cause.clone()),
                    now,
                    signer,
                )
                .await?
                {
                    out.push(fact);
                }
            }
            ConsequenceKind::EmitPromise => {
                let target = resolve_target(store, c).await?;
                let params = c.params.clone().unwrap_or(serde_json::json!({}));
                let delta = params
                    .get("delta")
                    .and_then(|d| d.as_f64())
                    .unwrap_or(carried);
                store::misc::insert_promise(
                    &store.pool,
                    store::misc::NewPromise {
                        record_uid: Some(target),
                        delta,
                        window_end: params
                            .get("window_end")
                            .and_then(|w| w.as_str())
                            .map(String::from),
                        party_uid: params
                            .get("party")
                            .and_then(|p| p.as_str())
                            .map(String::from),
                        state: Some(PromiseState::Proposed),
                        rule_uid: Some(rule.def.uid.clone()),
                        ..Default::default()
                    },
                )
                .await?;
            }
            ConsequenceKind::RunCommand => {
                let payload = serde_json::json!({
                    "command": c.target.clone().unwrap_or_default(),
                    "carried": carried,
                });
                store::misc::queue_effect(&store.pool, "command", &payload, Some(&rule.def.uid))
                    .await?;
            }
            ConsequenceKind::Notify => {
                let message = c
                    .params
                    .as_ref()
                    .and_then(|p| p.get("message"))
                    .and_then(|m| m.as_str())
                    .map(String::from)
                    .unwrap_or_else(|| format!("rule {} fired", display_name(rule)));
                let payload = serde_json::json!({ "message": message, "carried": carried });
                store::misc::queue_effect(&store.pool, "notify", &payload, Some(&rule.def.uid))
                    .await?;
            }
            ConsequenceKind::Ask => {
                let question = c
                    .params
                    .as_ref()
                    .and_then(|p| p.get("question"))
                    .and_then(|q| q.as_str())
                    .map(String::from)
                    .unwrap_or_else(|| format!("{}?", display_name(rule)));
                let options = c
                    .params
                    .as_ref()
                    .and_then(|p| p.get("options"))
                    .cloned()
                    .unwrap_or(serde_json::json!([{"label": "yes"}, {"label": "no"}]));
                store::misc::create_decision(
                    &store.pool,
                    &rule.def.uid,
                    "ask",
                    &question,
                    &options,
                )
                .await?;
            }
            ConsequenceKind::AdvanceTransfer => {
                // Karma advances within policy, never past it (blueprint VIII.3)
                let target = resolve_target(store, c).await?;
                crate::transfer::activate_promises(store, &target).await?;
            }
            ConsequenceKind::SetVisibility => {
                let target = resolve_target(store, c).await?;
                let params = c.params.clone().unwrap_or(serde_json::json!({}));
                let subject_kind = params
                    .get("subject_kind")
                    .and_then(|s| s.as_str())
                    .unwrap_or("public");
                let subject = params.get("subject").and_then(|s| s.as_str());
                store::visibility::grant(&store.pool, subject_kind, subject, &target).await?;
            }
            ConsequenceKind::RunQuery => {
                // executes a saved Protein when the effect runs (VII interactions)
                let payload = serde_json::json!({
                    "target": c.target.clone().unwrap_or_default(),
                    "params": c.params,
                    "carried": carried,
                });
                store::misc::queue_effect(&store.pool, "query", &payload, Some(&rule.def.uid))
                    .await?;
            }
            ConsequenceKind::RunAction => {
                // params carry the typed Action wire form, executed by the
                // effect runner through the same Engine::act as everyone else
                let payload = serde_json::json!({
                    "action": c.params,
                    "carried": carried,
                });
                store::misc::queue_effect(&store.pool, "action", &payload, Some(&rule.def.uid))
                    .await?;
            }
        }
    }
    Ok(out)
}

fn display_name(rule: &LoadedRule) -> String {
    rule.def
        .slug
        .clone()
        .unwrap_or_else(|| rule.def.uid.clone())
}

async fn resolve_target(store: &Store, c: &ConsequenceSpec) -> Result<String, EngineError> {
    let raw = c
        .target
        .as_deref()
        .ok_or_else(|| EngineError::Consequence(format!("{} needs a target", c.kind.as_str())))?;
    let token = raw.trim_start_matches('@');
    store::records::resolve(&store.pool, token)
        .await?
        .map(|r| r.uid)
        .ok_or_else(|| EngineError::UnknownRecord(token.to_string()))
}
