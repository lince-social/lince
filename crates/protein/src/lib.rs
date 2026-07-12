//! The Protein layer (blueprint Part VII): how any interface asks Lince for
//! data. DNA is what you store; Protein is how it comes out and shows its
//! power. Sands and every first-party surface speak only Protein (reads) and
//! Actions (writes, in `engine`); they never see tables or SQL — which is what
//! makes the storage engine replaceable underneath.
//!
//! **Protein never mutates.** This crate has no write path at all.
//!
//! Sources: `record | promise | decision | fact | concept | transfer`.
//! Boolean predicate tree with Lingua-DAG `concept_in`; includes `facts`
//! (provenance), `promises`, `links` (with tree `depth`), `threads`,
//! `extension`, `availability`, and `projection` (promise fold); aggregation
//! (`sum`/`count` by concept/kind/cause_kind/day — the finance workhorse);
//! ordering `topo(kind)` + field asc/desc; limit. Rows come out as JSON — the
//! wire shape sands consume. Live subscriptions ride the engine's `fact_bus`
//! (see `affects`): snapshot, then re-execute on relevant commits.
//!
//! `include: projection` folds **promises** forward (the planned trajectory
//! from agreed/active commitments). Full rule simulation needs the Karma
//! registry and stays engine-side (`Engine::project`) so this crate keeps its
//! read-only-by-construction guarantee.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
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
    /// Fact source: group by the fact's cause_kind (W-finance).
    CauseKind,
    /// Fact source: group by calendar day (`YYYY-MM-DD` of `at`).
    Day,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Record,
    Promise,
    Decision,
    /// The Ledger itself — history, finance, statistics.
    Fact,
    /// The Lingua vocabulary.
    Concept,
    /// Transfers with their derived status (blueprint VIII.1).
    Transfer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Predicate {
    All(Vec<Predicate>),
    Any(Vec<Predicate>),
    Not(Box<Predicate>),
    QuantityLt(f64),
    QuantityLte(f64),
    QuantityGt(f64),
    QuantityGte(f64),
    QuantityEq(f64),
    UidEq(String),
    KindEq(String),
    SlugEq(String),
    /// Lingua-DAG aware: `concept_in("food")` matches records tagged `@apple`
    /// through `apple -> fruit -> food` (blueprint III.1/VII.1).
    ConceptIn(String),
    /// Link filter: matches records that have a link of `kind` (a Lingua
    /// concept, e.g. `tag` / `assigned-to`) pointing to the record resolved
    /// from `to` (e.g. a "Tasks" cluster record). Multi-valued — a record may
    /// carry many such links — and the natural home for cluster tags. Compose
    /// include/exclude with `any` / `not` / `all`, e.g. Tasks OR ProjectA but
    /// NOT ProjectB.
    LinkedTo {
        kind: String,
        to: String,
    },
    /// Promise-source only: state is one of these.
    StateIn(Vec<String>),
    /// Fact source: `at` within the trailing window (`"30d"`, `"2h"`) or at or
    /// after an absolute RFC3339 instant.
    AtSince(String),
    /// Fact source: the fact's cause_kind equals this (`"settlement"`, ...).
    CauseKindEq(String),
    /// Fact source: the fact belongs to this record (slug or uid) —
    /// the W-provenance standard.
    RecordEq(String),
    /// Place Instinct (IX): the record's place is within `meters` of the
    /// anchor record's place. Records without a place never match.
    Near {
        of: String,
        meters: f64,
    },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Include {
    pub facts: Option<FactsInclude>,
    pub promises: Option<PromisesInclude>,
    pub links: Option<LinksInclude>,
    pub threads: Option<ThreadsInclude>,
    /// Availability projections (blueprint V.3): `available`, `planned`.
    #[serde(default)]
    pub availability: bool,
    /// One fds namespace attached as `extension` (blueprint I.2).
    pub extension: Option<ExtensionInclude>,
    /// Promise fold to a future instant (blueprint XII via V.3): `projected`.
    pub projection: Option<ProjectionInclude>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionInclude {
    pub namespace: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectionInclude {
    /// `"+7d"` relative to now, or an absolute RFC3339 instant.
    pub at: String,
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
    /// Legacy single-kind spelling. New sands should use `kinds`.
    #[serde(default)]
    pub kind: Option<String>,
    /// Explicit link kinds to include. Empty means no links.
    #[serde(default)]
    pub kinds: Vec<String>,
    #[serde(default)]
    pub direction: LinkDirection,
    /// Reserved for tree expansion; v1 Relation uses `0`/omitted for direct
    /// links among the subscribed records.
    #[serde(default)]
    pub depth: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkDirection {
    Both,
    Out,
    In,
}

impl Default for LinkDirection {
    fn default() -> Self {
        Self::Both
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadsInclude {
    #[serde(default = "default_messages_limit")]
    pub messages_limit: usize,
}

fn default_messages_limit() -> usize {
    50
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
    // The gate applies BEFORE aggregation: hidden rows must not leak into sums.
    let visible = visible.as_ref();
    Ok(match protein.source {
        Source::Record => execute_records(store, protein, visible).await?,
        Source::Promise => execute_promises(store, protein, visible).await?,
        Source::Decision => {
            if visible.is_some() {
                return Ok(vec![]); // attention is never exported
            }
            execute_decisions(store, protein).await?
        }
        Source::Fact => execute_facts(store, protein, visible).await?,
        // Lingua is shared vocabulary by design (III): concepts travel freely.
        Source::Concept => execute_concepts(store, protein).await?,
        Source::Transfer => execute_transfers(store, protein, visible).await?,
    })
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
    matches!(
        protein.source,
        Source::Record | Source::Promise | Source::Decision
    )
}

// ------------------------------------------------------------------- records

async fn execute_records(
    store: &Store,
    protein: &Protein,
    visible: Option<&HashSet<String>>,
) -> Result<Vec<Value>, ProteinError> {
    let all = store::records::list_all(&store.pool).await?;
    let ctx = PredicateCtx::prepare(store, &protein.filter).await?;
    let mut rows: Vec<store::records::RecordRow> = Vec::new();
    for r in all {
        if visible.is_some_and(|v| !v.contains(&r.uid)) {
            continue;
        }
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
            // fact-source group keys are meaningless on records
            GroupBy::CauseKind | GroupBy::Day => "(n/a)".into(),
        };
        let entry = buckets.entry(key).or_insert(0.0);
        match agg.op {
            AggregateOp::Sum => *entry += r.quantity,
            AggregateOp::Count => *entry += 1.0,
        }
    }
    buckets
        .into_iter()
        .map(|(group, value)| json!({ "group": group, "value": value }))
        .collect()
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
            rows = ordered
                .into_iter()
                .filter_map(|uid| by_uid.remove(&uid))
                .collect();
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
        row["links"] = Value::Array(links_for_record(store, record_uid, links).await?);
    }
    if let Some(threads) = &include.threads {
        row["threads"] =
            json!(threads_for_record(store, record_uid, threads.messages_limit).await?);
    }
    if let Some(extension) = &include.extension {
        row["extension"] =
            store::records::get_extension(&store.pool, record_uid, &extension.namespace)
                .await?
                .unwrap_or(Value::Null);
    }
    if let Some(projection) = &include.projection {
        // the promise fold (V.3/XII): quantity + Σ deltas of agreed/active
        // promises whose window closes by the target instant
        let target = resolve_at(&projection.at);
        if let Some(target) = target {
            let promises = store::misc::promises_for_record(&store.pool, record_uid).await?;
            let mut projected = quantity;
            for p in &promises {
                use nucleus::PromiseState::*;
                if !matches!(p.state, Agreed | Active) {
                    continue;
                }
                let Some(end) = &p.window_end else { continue };
                if end.as_str() <= target.as_str() {
                    projected += p.delta;
                }
            }
            row["projected"] = json!({ "at": target, "quantity": projected });
        }
    }
    Ok(())
}

/// Resolve a projection instant: `"+7d"` relative to now, or absolute RFC3339.
fn resolve_at(value: &str) -> Option<String> {
    if let Some(rest) = value.strip_prefix('+') {
        let secs = nucleus::frequency::parse_duration(rest)?;
        return Some((chrono::Utc::now() + chrono::TimeDelta::seconds(secs)).to_rfc3339());
    }
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|_| value.to_string())
}

async fn resolve_link_kind_uids(
    store: &Store,
    links: &LinksInclude,
) -> Result<Vec<String>, ProteinError> {
    let mut requested = Vec::new();
    if let Some(kind) = links.kind.as_deref().map(str::trim).filter(|kind| !kind.is_empty()) {
        requested.push(kind.to_string());
    }
    for kind in &links.kinds {
        let kind = kind.trim();
        if !kind.is_empty() && !requested.iter().any(|existing| existing == kind) {
            requested.push(kind.to_string());
        }
    }
    let mut out = Vec::new();
    for kind in requested {
        if let Some(uid) = store::concepts::resolve(&store.pool, &kind).await? {
            if !out.iter().any(|existing| existing == &uid) {
                out.push(uid);
            }
        }
    }
    Ok(out)
}

async fn links_for_record(
    store: &Store,
    record_uid: &str,
    links: &LinksInclude,
) -> Result<Vec<Value>, ProteinError> {
    let kind_uids = resolve_link_kind_uids(store, links).await?;
    if kind_uids.is_empty() {
        return Ok(vec![]);
    }
    let rows = store::links::links_of_kinds(&store.pool, &kind_uids).await?;

    // BFS over the loaded kind-graph: hop 1 = direct links (depth 0/1 —
    // the default), deeper hops expand the tree (blueprint IV.2/VII.1).
    let max_hops = links.depth.max(1);
    let mut frontier: HashSet<String> = HashSet::from([record_uid.to_string()]);
    let mut visited: HashSet<String> = frontier.clone();
    let mut seen_links: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    for hop in 1..=max_hops {
        let mut next: HashSet<String> = HashSet::new();
        for link in &rows {
            let outgoing = frontier.contains(&link.from);
            let incoming = frontier.contains(&link.to);
            if !outgoing && !incoming {
                continue;
            }
            if matches!(links.direction, LinkDirection::Out) && !outgoing {
                continue;
            }
            if matches!(links.direction, LinkDirection::In) && !incoming {
                continue;
            }
            let other = if outgoing {
                link.to.clone()
            } else {
                link.from.clone()
            };
            if !visited.contains(&other) {
                next.insert(other.clone());
            }
            if !seen_links.insert(link.uid.clone()) {
                continue;
            }
            out.push(json!({
                "uid": link.uid,
                "from": link.from,
                "to": link.to,
                "kind_uid": link.kind_uid,
                "kind": link.kind,
                "direction": if outgoing { "out" } else { "in" },
                "other": other,
                "hop": hop,
                "quantity": link.quantity,
                "created_at": link.created_at,
            }));
        }
        if next.is_empty() {
            break;
        }
        visited.extend(next.iter().cloned());
        frontier = next;
    }
    Ok(out)
}

async fn threads_for_record(
    store: &Store,
    record_uid: &str,
    messages_limit: usize,
) -> Result<Vec<Value>, ProteinError> {
    let Some(thread_of) = store::concepts::resolve(&store.pool, "thread-of").await? else {
        return Ok(vec![]);
    };
    let Some(message_in) = store::concepts::resolve(&store.pool, "message-in").await? else {
        return Ok(vec![]);
    };
    let reply_to = store::concepts::resolve(&store.pool, "reply-to").await?;
    let threads = store::links::records_to(&store.pool, &thread_of, record_uid).await?;
    let mut out = Vec::new();
    for thread in threads {
        if thread.kind != "thread" || thread.quantity <= 0.0 {
            continue;
        }
        let mut messages = Vec::new();
        for message in store::links::records_to(&store.pool, &message_in, &thread.uid).await? {
            if message.kind != "message" || message.quantity <= 0.0 {
                continue;
            }
            let parent_message_uid = match &reply_to {
                Some(reply_to) => store::links::records_from(&store.pool, &message.uid, reply_to)
                    .await?
                    .into_iter()
                    .next()
                    .map(|r| r.uid),
                None => None,
            };
            messages.push(json!({
                "uid": message.uid,
                "head": message.head,
                "body": message.body,
                "quantity": message.quantity,
                "parent_message_uid": parent_message_uid,
            }));
            if messages.len() >= messages_limit {
                break;
            }
        }
        out.push(json!({
            "uid": thread.uid,
            "head": thread.head,
            "body": thread.body,
            "quantity": thread.quantity,
            "messages": messages,
        }));
    }
    Ok(out)
}

// ---------------------------------------------------------------- predicates

struct PredicateCtx {
    /// concept name/uid -> the DAG family (root + descendants) as a set.
    concept_families: HashMap<String, HashSet<String>>,
    /// `near.of` anchor token -> the anchor's place (None if it has none).
    anchors: HashMap<String, Option<nucleus::place::Place>>,
    /// (kind, to) token -> set of record uids that have a `kind`-link to `to`.
    link_sources: HashMap<(String, String), HashSet<String>>,
}

impl PredicateCtx {
    async fn prepare(store: &Store, preds: &[Predicate]) -> Result<Self, ProteinError> {
        let mut ctx = PredicateCtx {
            concept_families: HashMap::new(),
            anchors: HashMap::new(),
            link_sources: HashMap::new(),
        };
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
            Predicate::LinkedTo { kind, to } => {
                let sources = match (
                    store::concepts::resolve(&store.pool, kind).await?,
                    store::records::resolve(&store.pool, to).await?,
                ) {
                    (Some(kind_uid), Some(target)) => {
                        store::links::records_to(&store.pool, &kind_uid, &target.uid)
                            .await?
                            .into_iter()
                            .map(|r| r.uid)
                            .collect()
                    }
                    _ => HashSet::new(),
                };
                self.link_sources.insert((kind.clone(), to.clone()), sources);
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
                Predicate::QuantityLte(n) => r.quantity <= *n,
                Predicate::QuantityGt(n) => r.quantity > *n,
                Predicate::QuantityGte(n) => r.quantity >= *n,
                Predicate::QuantityEq(n) => r.quantity == *n,
                Predicate::UidEq(uid) => r.uid == *uid,
                Predicate::KindEq(k) => r.kind == *k,
                Predicate::SlugEq(s) => r.slug.as_deref() == Some(s.as_str()),
                Predicate::ConceptIn(name) => {
                    matches!((&r.concept_uid, self.concept_families.get(name)),
                        (Some(c), Some(family)) if family.contains(c))
                }
                Predicate::LinkedTo { kind, to } => self
                    .link_sources
                    .get(&(kind.clone(), to.clone()))
                    .is_some_and(|sources| sources.contains(&r.uid)),
                Predicate::StateIn(_) => true, // promise-source predicate: vacuous on records
                // fact-source predicates: vacuous on records
                Predicate::AtSince(_) | Predicate::CauseKindEq(_) | Predicate::RecordEq(_) => true,
                Predicate::Near { of, meters } => {
                    match (
                        self.anchors.get(of).and_then(|a| *a),
                        store::places::of_record(&store.pool, &r.uid).await?,
                    ) {
                        (Some(anchor), Some(here)) => nucleus::place::near(here, anchor, *meters),
                        _ => false,
                    }
                }
            })
        })
    }
}

// ------------------------------------------------------------------ promises

async fn execute_promises(
    store: &Store,
    protein: &Protein,
    visible: Option<&HashSet<String>>,
) -> Result<Vec<Value>, ProteinError> {
    let states: Option<&Vec<String>> = protein.filter.iter().find_map(|p| match p {
        Predicate::StateIn(s) => Some(s),
        _ => None,
    });
    let mut out = Vec::new();
    for p in store::misc::list_promises(&store.pool).await? {
        if let Some(visible) = visible {
            // conservative v1: a remote subject sees a promise only through a
            // visible target record (concept-level promises travel via Senses)
            if !p.record_uid.as_deref().is_some_and(|uid| visible.contains(uid)) {
                continue;
            }
        }
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

// --------------------------------------------------------------------- facts

/// Resolve an `at_since` value: a trailing duration (`"30d"`) against now, or
/// an absolute RFC3339 instant passed through.
fn resolve_since(value: &str) -> Option<String> {
    if let Some(secs) = nucleus::frequency::parse_duration(value) {
        return Some(
            (chrono::Utc::now() - chrono::TimeDelta::seconds(secs)).to_rfc3339(),
        );
    }
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|_| value.to_string())
}

async fn execute_facts(
    store: &Store,
    protein: &Protein,
    visible: Option<&HashSet<String>>,
) -> Result<Vec<Value>, ProteinError> {
    // record metadata for concept filters and aggregation keys
    let mut record_concept: HashMap<String, Option<String>> = HashMap::new();
    for r in store::records::list_all(&store.pool).await? {
        record_concept.insert(r.uid, r.concept_uid);
    }

    // walk the flat predicate list (fact predicates don't nest in v1)
    let mut since: Option<String> = None;
    let mut cause_kind: Option<&str> = None;
    let mut record: Option<String> = None;
    let mut concept_family: Option<HashSet<String>> = None;
    for p in &protein.filter {
        match p {
            Predicate::AtSince(v) => since = resolve_since(v),
            Predicate::CauseKindEq(k) => cause_kind = Some(k),
            Predicate::RecordEq(token) => {
                record = store::records::resolve(&store.pool, token)
                    .await?
                    .map(|r| r.uid);
                if record.is_none() {
                    return Ok(vec![]); // unknown record: nothing can match
                }
            }
            Predicate::ConceptIn(name) => {
                concept_family = Some(match store::concepts::resolve(&store.pool, name).await? {
                    Some(uid) => store::concepts::descendants_including(&store.pool, &uid)
                        .await?
                        .into_iter()
                        .collect(),
                    None => HashSet::new(),
                });
            }
            _ => {}
        }
    }

    const FACT_SCAN_CAP: i64 = 100_000;
    let mut facts = Vec::new();
    for f in store::facts::list_since(&store.pool, since.as_deref(), FACT_SCAN_CAP).await? {
        if visible.is_some_and(|v| !v.contains(&f.record_uid)) {
            continue;
        }
        if cause_kind.is_some_and(|k| f.cause.kind.as_str() != k) {
            continue;
        }
        if record.as_ref().is_some_and(|r| &f.record_uid != r) {
            continue;
        }
        if let Some(family) = &concept_family {
            let matches = record_concept
                .get(&f.record_uid)
                .and_then(|c| c.as_ref())
                .is_some_and(|c| family.contains(c));
            if !matches {
                continue;
            }
        }
        facts.push(f);
    }

    if let Some(agg) = &protein.aggregate {
        let mut buckets: std::collections::BTreeMap<String, f64> = Default::default();
        for f in &facts {
            let key = match agg.by {
                GroupBy::CauseKind => f.cause.kind.as_str().to_string(),
                GroupBy::Day => f.at.format("%Y-%m-%d").to_string(),
                GroupBy::Concept => record_concept
                    .get(&f.record_uid)
                    .and_then(|c| c.clone())
                    .unwrap_or_else(|| "(none)".into()),
                GroupBy::Kind => f.cause.kind.as_str().to_string(),
            };
            let entry = buckets.entry(key).or_insert(0.0);
            match agg.op {
                AggregateOp::Sum => *entry += f.delta,
                AggregateOp::Count => *entry += 1.0,
            }
        }
        return Ok(buckets
            .into_iter()
            .map(|(group, value)| json!({ "group": group, "value": value }))
            .collect());
    }

    // newest last (chain order); limit takes the most recent
    if let Some(limit) = protein.limit {
        if facts.len() > limit {
            facts.drain(..facts.len() - limit);
        }
    }
    Ok(facts
        .into_iter()
        .map(|f| {
            json!({
                "uid": f.uid,
                "record": f.record_uid,
                "delta": f.delta,
                "at": f.at.to_rfc3339(),
                "cause_kind": f.cause.kind.as_str(),
                "cause": f.cause.uid,
                "actor": f.actor_uid,
                "payload": f.payload,
            })
        })
        .collect())
}

// ------------------------------------------------------------------ concepts

async fn execute_concepts(store: &Store, protein: &Protein) -> Result<Vec<Value>, ProteinError> {
    // concept_in narrows to a family (the concept + its descendants)
    let mut family: Option<HashSet<String>> = None;
    for p in &protein.filter {
        if let Predicate::ConceptIn(name) = p {
            family = Some(match store::concepts::resolve(&store.pool, name).await? {
                Some(uid) => store::concepts::descendants_including(&store.pool, &uid)
                    .await?
                    .into_iter()
                    .collect(),
                None => HashSet::new(),
            });
        }
    }
    let mut out = Vec::new();
    for c in store::concepts::list_all(&store.pool).await? {
        if family.as_ref().is_some_and(|f| !f.contains(&c.uid)) {
            continue;
        }
        let name_matches = protein.filter.iter().all(|p| match p {
            Predicate::UidEq(uid) => c.uid == *uid,
            Predicate::SlugEq(name) => c.canonical_name == *name,
            _ => true,
        });
        if !name_matches {
            continue;
        }
        out.push(json!({
            "uid": c.uid,
            "name": c.canonical_name,
            "instinct": c.instinct,
            "parents": c.parents,
        }));
        if protein.limit.is_some_and(|l| out.len() >= l) {
            break;
        }
    }
    Ok(out)
}

// ----------------------------------------------------------------- transfers

/// Derived transfer status (blueprint VIII.1) — never stored:
/// `inactive` (quantity=0) | `draft` | `proposed` | `agreed` | `in_transfer` |
/// `settled`, from promise states + agreement levels + policy.
fn derive_transfer_status(
    active: bool,
    agreement_type: &str,
    agreement_pct: Option<i64>,
    party_levels: &[i64],
    promise_states: &[nucleus::PromiseState],
) -> &'static str {
    use nucleus::PromiseState::*;
    if !active {
        return "inactive";
    }
    let live: Vec<_> = promise_states
        .iter()
        .filter(|s| !matches!(s, Withdrawn))
        .collect();
    if live.is_empty() {
        return "draft";
    }
    if live.iter().all(|s| matches!(s, Kept)) {
        return "settled";
    }
    if live.iter().any(|s| matches!(s, Active)) {
        return "in_transfer";
    }
    let policy = nucleus::transfer::AgreementType::parse(agreement_type)
        .map(|t| {
            nucleus::transfer::policy_satisfied(
                t,
                agreement_pct.map(|p| p as u8),
                party_levels,
            )
        })
        .unwrap_or(false);
    if policy && live.iter().all(|s| matches!(s, Agreed | Kept)) {
        return "agreed";
    }
    if live.iter().any(|s| matches!(s, Proposed | Agreed)) {
        return "proposed";
    }
    "draft"
}

async fn execute_transfers(
    store: &Store,
    protein: &Protein,
    visible: Option<&HashSet<String>>,
) -> Result<Vec<Value>, ProteinError> {
    // record -> concept, for the advisory per-concept balance (VIII.1)
    let mut record_concept: HashMap<String, Option<String>> = HashMap::new();
    for r in store::records::list_all(&store.pool).await? {
        record_concept.insert(r.uid, r.concept_uid);
    }
    let mut out = Vec::new();
    for t in store::transfers::list_all(&store.pool).await? {
        let uid = &t.transfer.record_uid;
        if visible.is_some_and(|v| !v.contains(uid)) {
            continue;
        }
        let matches = protein.filter.iter().all(|p| match p {
            Predicate::UidEq(u) => uid == u,
            Predicate::SlugEq(s) => t.slug.as_deref() == Some(s.as_str()),
            _ => true,
        });
        if !matches {
            continue;
        }
        let promises = store::transfers::promises_of(&store.pool, uid).await?;
        let parties = store::transfers::party_levels(&store.pool, uid).await?;
        let levels: Vec<i64> = parties.iter().map(|(_, _, level)| *level).collect();
        let states: Vec<nucleus::PromiseState> = promises.iter().map(|p| p.state).collect();
        let status = derive_transfer_status(
            t.transfer.active,
            &t.transfer.agreement_type,
            t.transfer.agreement_pct,
            &levels,
            &states,
        );
        // Advisory balance (VIII.1): a trade sums to zero per Lingua concept
        // across parties; donations are deliberately unbalanced.
        let mut balance: std::collections::BTreeMap<String, f64> = Default::default();
        for p in &promises {
            let key = p
                .record_uid
                .as_ref()
                .and_then(|r| record_concept.get(r).cloned().flatten())
                .unwrap_or_else(|| "(none)".into());
            *balance.entry(key).or_insert(0.0) += p.delta;
        }
        let balanced = !balance.is_empty() && balance.values().all(|v| v.abs() < 1e-9);
        out.push(json!({
            "uid": uid,
            "slug": t.slug,
            "head": t.head,
            "status": status,
            "balance": balance,
            "balanced": balanced,
            "active": t.transfer.active,
            "agreement_type": t.transfer.agreement_type,
            "agreement_pct": t.transfer.agreement_pct,
            "settlement": t.transfer.settlement,
            "satiation": t.transfer.satiation,
            "parties": parties
                .iter()
                .map(|(party, actor, level)| json!({
                    "uid": party, "actor": actor, "level": level,
                }))
                .collect::<Vec<_>>(),
            "promises": promises
                .iter()
                .map(|p| json!({
                    "uid": p.uid,
                    "record": p.record_uid,
                    "delta": p.delta,
                    "state": p.state.as_str(),
                    "party": p.party_uid,
                }))
                .collect::<Vec<_>>(),
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
        filter: vec![
            Predicate::QuantityLt(0.0),
            Predicate::KindEq("plain".into()),
        ],
        include: Include::default(),
        aggregate: None,
        order: vec![
            Order::Topo(order_kind.into()),
            Order::Asc("created_at".into()),
        ],
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
