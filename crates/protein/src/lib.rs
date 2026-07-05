//! The Protein layer (blueprint Part VII): how any interface asks Lince for
//! data. DNA is what you store; Protein is how it comes out and shows its
//! power. Sands and every first-party surface speak only Protein (reads) and
//! Actions (writes, in `engine`); they never see tables or SQL — which is what
//! makes the storage engine replaceable underneath.
//!
//! **Protein never mutates.** This crate has no write path at all.
//!
//! v1 scope: sources `record | promise | decision`; boolean predicate tree with
//! Lingua-DAG `concept_in`; includes `facts` (provenance), `promises`, `links`;
//! ordering `topo(kind)` + field asc/desc; limit. Rows come out as JSON — the
//! wire shape sands consume. Live subscriptions ride the engine's `fact_bus`
//! (see `affects`): snapshot, then re-execute on relevant commits; the
//! streaming transport itself lands with the transport crate.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use store::Store;

pub type ProteinError = store::StoreError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Protein {
    pub source: Source,
    #[serde(default, rename = "where")]
    pub filter: Vec<Predicate>, // top level is an implicit `all`
    #[serde(default)]
    pub include: Include,
    #[serde(default)]
    pub aggregate: Option<Aggregate>,
    #[serde(default)]
    pub order: Vec<Order>,
    #[serde(default)]
    pub limit: Option<usize>,
}

/// `aggregate: { op: sum, by: concept }` — the finance/statistics workhorse
/// (blueprint VII.1). Applied after filtering, instead of row output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Aggregate {
    pub op: AggregateOp,
    pub by: GroupBy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AggregateOp {
    Sum,
    Count,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupBy {
    Concept,
    Kind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Record,
    Promise,
    Decision,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Predicate {
    All(Vec<Predicate>),
    Any(Vec<Predicate>),
    Not(Box<Predicate>),
    QuantityLt(f64),
    QuantityGt(f64),
    QuantityEq(f64),
    KindEq(String),
    SlugEq(String),
    /// Lingua-DAG aware: `concept_in("food")` matches records tagged `@apple`
    /// through `apple -> fruit -> food` (blueprint III.1/VII.1).
    ConceptIn(String),
    /// Promise-source only: state is one of these.
    StateIn(Vec<String>),
    /// Place Instinct (IX): the record's place is within `meters` of the
    /// anchor record's place. Records without a place never match.
    Near { of: String, meters: f64 },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Include {
    pub facts: Option<FactsInclude>,
    pub promises: Option<PromisesInclude>,
    pub links: Option<LinksInclude>,
    /// Availability projections (blueprint V.3): `available`, `planned`.
    #[serde(default)]
    pub availability: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactsInclude {
    #[serde(default = "default_fact_limit")]
    pub limit: i64,
}

fn default_fact_limit() -> i64 {
    10
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PromisesInclude {
    #[serde(default)]
    pub state: Vec<String>, // empty = all states
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinksInclude {
    pub kind: String, // Lingua concept name/uid
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Order {
    /// Topological sort over the named link-kind graph, restricted to the
    /// result set — the focus queue (blueprint Window 1b). Ties keep the
    /// order produced by the remaining keys.
    Topo(String),
    Asc(String),
    Desc(String),
}

/// Execute a Protein snapshot for the local Cell. Output rows are JSON — the
/// sand wire shape.
pub async fn execute(store: &Store, protein: &Protein) -> Result<Vec<Value>, ProteinError> {
    execute_for(store, protein, None).await
}

/// Execute for a subject (blueprint XV.1): the ONE visibility enforcement
/// point. `None` = the local Cell (sees everything). `Some(subject)` = a
/// remote Organ/actor: default hidden, whole-row grants only in v1; the
/// Decision Queue never leaves the Cell.
pub async fn execute_for(
    store: &Store,
    protein: &Protein,
    subject: Option<&str>,
) -> Result<Vec<Value>, ProteinError> {
    let visible = match subject {
        None => None,
        Some(s) => Some(store::visibility::visible_targets(&store.pool, s).await?),
    };
    let mut rows = match protein.source {
        Source::Record => execute_records(store, protein).await?,
        Source::Promise => execute_promises(store, protein).await?,
        Source::Decision => {
            if visible.is_some() {
                return Ok(vec![]); // attention is never exported
            }
            execute_decisions(store, protein).await?
        }
    };
    if let Some(visible) = visible {
        rows.retain(|row| {
            let gate_uid = match protein.source {
                Source::Record => row.get("uid"),
                Source::Promise => row.get("record"),
                Source::Decision => None,
            };
            gate_uid.and_then(Value::as_str).is_some_and(|uid| visible.contains(uid))
        });
    }
    Ok(rows)
}

/// Execute a saved Protein (a record of kind='protein' whose AST lives in the
/// `lince.protein` extension) — the old `view` table's successor.
pub async fn execute_saved(
    store: &Store,
    slug_or_uid: &str,
    subject: Option<&str>,
) -> Result<Vec<Value>, ProteinError> {
    let record = store::records::resolve(&store.pool, slug_or_uid)
        .await?
        .ok_or_else(|| store::sqlx::Error::Protocol(format!("no saved protein {slug_or_uid}")))?;
    let ast = store::records::get_extension(&store.pool, &record.uid, "lince.protein")
        .await?
        .ok_or_else(|| store::sqlx::Error::Protocol(format!("{slug_or_uid} has no protein AST")))?;
    let protein: Protein = serde_json::from_value(ast)
        .map_err(|e| store::sqlx::Error::Protocol(format!("bad protein AST: {e}")))?;
    execute_for(store, &protein, subject).await
}

/// Coarse live-subscription invalidation: does a committed fact possibly
/// change this Protein's result? v1 answer: any fact touching the source
/// domain does. The transport re-executes on `true`; refinement comes later.
pub fn affects(protein: &Protein, _fact: &nucleus::Fact) -> bool {
    matches!(protein.source, Source::Record | Source::Promise | Source::Decision)
}

// ------------------------------------------------------------------- records

async fn execute_records(store: &Store, protein: &Protein) -> Result<Vec<Value>, ProteinError> {
    let all = store::records::list_all(&store.pool).await?;
    let ctx = PredicateCtx::prepare(store, &protein.filter).await?;
    let mut rows: Vec<store::records::RecordRow> = Vec::new();
    for r in all {
        if ctx.matches_record(store, &r, &protein.filter).await? {
            rows.push(r);
        }
    }

    // aggregation short-circuits row output (blueprint VII.1)
    if let Some(agg) = &protein.aggregate {
        return Ok(aggregate_records(&rows, agg));
    }

    rows = order_records(store, rows, &protein.order).await?;
    if let Some(limit) = protein.limit {
        rows.truncate(limit);
    }

    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let mut row = json!({
            "uid": r.uid,
            "slug": r.slug,
            "kind": r.kind,
            "head": r.head,
            "body": r.body,
            "quantity": r.quantity,
            "concept": r.concept_uid,
            "unit": r.unit_uid,
        });
        attach_includes(store, &mut row, &r.uid, r.quantity, &protein.include).await?;
        out.push(row);
    }
    Ok(out)
}

fn aggregate_records(rows: &[store::records::RecordRow], agg: &Aggregate) -> Vec<Value> {
    let mut buckets: std::collections::BTreeMap<String, f64> = std::collections::BTreeMap::new();
    for r in rows {
        let key = match agg.by {
            GroupBy::Concept => r.concept_uid.clone().unwrap_or_else(|| "(none)".into()),
            GroupBy::Kind => r.kind.clone(),
        };
        let entry = buckets.entry(key).or_insert(0.0);
        match agg.op {
            AggregateOp::Sum => *entry += r.quantity,
            AggregateOp::Count => *entry += 1.0,
        }
    }
    buckets.into_iter().map(|(group, value)| json!({ "group": group, "value": value })).collect()
}

async fn order_records(
    store: &Store,
    mut rows: Vec<store::records::RecordRow>,
    order: &[Order],
) -> Result<Vec<store::records::RecordRow>, ProteinError> {
    // apply field keys first (stable sorts in reverse order), topo last so the
    // graph wins and field keys become the tie-break inside/among chains
    for key in order.iter().rev() {
        match key {
            Order::Asc(f) | Order::Desc(f) => {
                let desc = matches!(key, Order::Desc(_));
                rows.sort_by(|a, b| {
                    let ord = match f.as_str() {
                        "quantity" => a.quantity.total_cmp(&b.quantity),
                        "slug" => a.slug.cmp(&b.slug),
                        _ => std::cmp::Ordering::Equal, // created_at: list_all is already oldest-first
                    };
                    if desc { ord.reverse() } else { ord }
                });
            }
            Order::Topo(_) => {}
        }
    }
    if let Some(Order::Topo(kind)) = order.iter().find(|o| matches!(o, Order::Topo(_))) {
        if let Some(kind_uid) = store::concepts::resolve(&store.pool, kind).await? {
            let edges = store::links::edges_of_kind(&store.pool, &kind_uid).await?;
            let uids: Vec<String> = rows.iter().map(|r| r.uid.clone()).collect();
            let ordered = nucleus::graph::topo_order(&uids, &edges);
            let mut by_uid: HashMap<String, store::records::RecordRow> =
                rows.into_iter().map(|r| (r.uid.clone(), r)).collect();
            rows = ordered.into_iter().filter_map(|uid| by_uid.remove(&uid)).collect();
        }
    }
    Ok(rows)
}

async fn attach_includes(
    store: &Store,
    row: &mut Value,
    record_uid: &str,
    quantity: f64,
    include: &Include,
) -> Result<(), ProteinError> {
    if include.availability {
        // available = quantity - Σ |delta| of active outgoing promises;
        // planned = quantity + Σ delta of agreed/active promises (blueprint V.3)
        let promises = store::misc::promises_for_record(&store.pool, record_uid).await?;
        let mut reserved = 0.0;
        let mut planned_delta = 0.0;
        for p in &promises {
            use nucleus::PromiseState::*;
            match p.state {
                Active if p.delta < 0.0 => reserved += -p.delta,
                _ => {}
            }
            if matches!(p.state, Agreed | Active) {
                planned_delta += p.delta;
            }
        }
        row["available"] = json!(quantity - reserved);
        row["planned"] = json!(quantity + planned_delta);
    }
    if let Some(facts) = &include.facts {
        // provenance in one line: this is "the end of custom plumbing" (VII.1)
        let list = store::facts::for_record(&store.pool, record_uid, facts.limit).await?;
        row["facts"] = Value::Array(
            list.into_iter()
                .map(|f| {
                    json!({
                        "delta": f.delta,
                        "at": f.at.to_rfc3339(),
                        "cause_kind": f.cause.kind.as_str(),
                        "cause": f.cause.uid,
                        "actor": f.actor_uid,
                    })
                })
                .collect(),
        );
    }
    if let Some(promises) = &include.promises {
        let list = store::misc::promises_for_record(&store.pool, record_uid).await?;
        row["promises"] = Value::Array(
            list.into_iter()
                .filter(|p| {
                    promises.state.is_empty()
                        || promises.state.iter().any(|s| s == p.state.as_str())
                })
                .map(|p| {
                    json!({
                        "uid": p.uid,
                        "delta": p.delta,
                        "state": p.state.as_str(),
                        "window_end": p.window_end,
                        "party": p.party_uid,
                    })
                })
                .collect(),
        );
    }
    if let Some(links) = &include.links {
        if let Some(kind_uid) = store::concepts::resolve(&store.pool, &links.kind).await? {
            let edges = store::links::edges_of_kind(&store.pool, &kind_uid).await?;
            row["links"] = Value::Array(
                edges
                    .into_iter()
                    .filter(|e| e.from == record_uid || e.to == record_uid)
                    .map(|e| {
                        let out = e.from == record_uid;
                        json!({
                            "kind": links.kind,
                            "direction": if out { "out" } else { "in" },
                            "other": if out { e.to } else { e.from },
                            "quantity": e.quantity,
                        })
                    })
                    .collect(),
            );
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- predicates

struct PredicateCtx {
    /// concept name/uid -> the DAG family (root + descendants) as a set.
    concept_families: HashMap<String, HashSet<String>>,
    /// `near.of` anchor token -> the anchor's place (None if it has none).
    anchors: HashMap<String, Option<nucleus::place::Place>>,
}

impl PredicateCtx {
    async fn prepare(store: &Store, preds: &[Predicate]) -> Result<Self, ProteinError> {
        let mut ctx = PredicateCtx { concept_families: HashMap::new(), anchors: HashMap::new() };
        for p in preds {
            ctx.prepare_one(store, p).await?;
        }
        Ok(ctx)
    }

    async fn prepare_one(&mut self, store: &Store, p: &Predicate) -> Result<(), ProteinError> {
        match p {
            Predicate::ConceptIn(name) => {
                let family = match store::concepts::resolve(&store.pool, name).await? {
                    Some(uid) => store::concepts::descendants_including(&store.pool, &uid)
                        .await?
                        .into_iter()
                        .collect(),
                    None => HashSet::new(),
                };
                self.concept_families.insert(name.clone(), family);
            }
            Predicate::Near { of, .. } => {
                let place = match store::records::resolve(&store.pool, of).await? {
                    Some(r) => store::places::of_record(&store.pool, &r.uid).await?,
                    None => None,
                };
                self.anchors.insert(of.clone(), place);
            }
            Predicate::All(ps) | Predicate::Any(ps) => {
                for inner in ps {
                    Box::pin(self.prepare_one(store, inner)).await?;
                }
            }
            Predicate::Not(inner) => Box::pin(self.prepare_one(store, inner)).await?,
            _ => {}
        }
        Ok(())
    }

    async fn matches_record(
        &self,
        store: &Store,
        r: &store::records::RecordRow,
        preds: &[Predicate],
    ) -> Result<bool, ProteinError> {
        for p in preds {
            if !self.matches_one(store, r, p).await? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn matches_one<'a>(
        &'a self,
        store: &'a Store,
        r: &'a store::records::RecordRow,
        p: &'a Predicate,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<bool, ProteinError>> + Send + 'a>>
    {
        Box::pin(async move {
            Ok(match p {
                Predicate::All(ps) => {
                    for p in ps {
                        if !self.matches_one(store, r, p).await? {
                            return Ok(false);
                        }
                    }
                    true
                }
                Predicate::Any(ps) => {
                    for p in ps {
                        if self.matches_one(store, r, p).await? {
                            return Ok(true);
                        }
                    }
                    false
                }
                Predicate::Not(p) => !self.matches_one(store, r, p).await?,
                Predicate::QuantityLt(n) => r.quantity < *n,
                Predicate::QuantityGt(n) => r.quantity > *n,
                Predicate::QuantityEq(n) => r.quantity == *n,
                Predicate::KindEq(k) => r.kind == *k,
                Predicate::SlugEq(s) => r.slug.as_deref() == Some(s.as_str()),
                Predicate::ConceptIn(name) => {
                    matches!((&r.concept_uid, self.concept_families.get(name)),
                        (Some(c), Some(family)) if family.contains(c))
                }
                Predicate::StateIn(_) => true, // promise-source predicate: vacuous on records
                Predicate::Near { of, meters } => {
                    match (self.anchors.get(of).and_then(|a| *a),
                           store::places::of_record(&store.pool, &r.uid).await?)
                    {
                        (Some(anchor), Some(here)) => nucleus::place::near(here, anchor, *meters),
                        _ => false,
                    }
                }
            })
        })
    }
}

// ------------------------------------------------------------------ promises

async fn execute_promises(store: &Store, protein: &Protein) -> Result<Vec<Value>, ProteinError> {
    let states: Option<&Vec<String>> = protein.filter.iter().find_map(|p| match p {
        Predicate::StateIn(s) => Some(s),
        _ => None,
    });
    let mut out = Vec::new();
    for p in store::misc::list_promises(&store.pool).await? {
        if let Some(states) = states {
            if !states.iter().any(|s| s == p.state.as_str()) {
                continue;
            }
        }
        out.push(json!({
            "uid": p.uid,
            "record": p.record_uid,
            "delta": p.delta,
            "state": p.state.as_str(),
            "window_end": p.window_end,
            "party": p.party_uid,
            "transfer": p.transfer_uid,
            "rule": p.rule_uid,
        }));
        if protein.limit.is_some_and(|l| out.len() >= l) {
            break;
        }
    }
    Ok(out)
}

// ----------------------------------------------------------------- decisions

async fn execute_decisions(store: &Store, protein: &Protein) -> Result<Vec<Value>, ProteinError> {
    let mut out = Vec::new();
    for d in store::misc::list_decisions(&store.pool).await? {
        if !d.open {
            continue; // the Decision Queue shows what awaits a human (XIII.1)
        }
        out.push(json!({
            "uid": d.record_uid,
            "subject": d.subject_uid,
            "kind": d.kind,
            "question": d.question,
            "options": d.options,
        }));
        if protein.limit.is_some_and(|l| out.len() >= l) {
            break;
        }
    }
    Ok(out)
}

// -------------------------------------------------------- canned Proteins

/// The focus queue (blueprint Window 1b), as the Protein it always was:
/// active plain Needs, `topo(order_kind)`, oldest-first tie-break.
pub fn focus_queue(order_kind: &str) -> Protein {
    Protein {
        source: Source::Record,
        filter: vec![Predicate::QuantityLt(0.0), Predicate::KindEq("plain".into())],
        include: Include::default(),
        aggregate: None,
        order: vec![Order::Topo(order_kind.into()), Order::Asc("created_at".into())],
        limit: None,
    }
}

/// The Decision Queue (blueprint XIII): everything awaiting a human choice.
pub fn decision_queue() -> Protein {
    Protein {
        source: Source::Decision,
        filter: vec![],
        include: Include::default(),
        aggregate: None,
        order: vec![],
        limit: None,
    }
}
