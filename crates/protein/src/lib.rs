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
#![recursion_limit = "256"]

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
    /// Transfers with their derived status (blueprint VIII.1). The first row
    /// is `kind: "transfer_context"`, carrying server-derived viewer identity,
    /// creation capability, and blocker codes even when no transfers exist;
    /// actual rows are `kind: "transfer"` and carry per-transfer capabilities.
    Transfer,
    /// The permission/role/user system (`store::auth` — native SQL state,
    /// not Ledger records, per blueprint's split). Rows are heterogeneous,
    /// distinguished by `kind`: `"role"` (id, name, permissions), `"user"`
    /// (id, username, name, role — never a password hash), and one
    /// `"permission_catalog"` row (the full static key list, for building a
    /// grant UI). No filter/include support — it's a small, fixed listing.
    /// Gated at the session boundary (`execute_for`): a remote/authenticated
    /// subject needs `role:read`, `user:read`, or `permission:read`; the
    /// local Cell (`subject: None`) always sees it, like every other source.
    Auth,
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
    /// The record's origin organ (slug or uid, resolved like any `@token`)
    /// equals this one. Records with no known origin never match. The
    /// selection primitive behind Protein-driven Sync/File Sync: "every
    /// record belonging to organ X".
    OrganEq(String),
    /// Like `organ_eq`, but any of these organs (slug or uid each).
    OrganIn(Vec<String>),
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
    /// Explicit link kinds to include. Empty means no links; a `"*"` entry
    /// means EVERY kind (Record's all-links view).
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
        Source::Transfer => execute_transfers(store, protein, visible, subject).await?,
        Source::Auth => {
            if let Some(actor) = subject {
                if !actor_can_read_auth(store, actor).await? {
                    return Ok(vec![]); // same "hidden, not an error" shape as Decision
                }
            }
            execute_auth(store).await?
        }
    })
}

/// Best-effort: any failure to resolve the actor (not a numeric app_user id,
/// no such user) reads as "can't read", not an error — a Protein snapshot
/// should never fail just because of who's asking.
async fn actor_can_read_auth(store: &Store, actor: &str) -> Result<bool, ProteinError> {
    let Ok(user_id) = actor.parse::<i64>() else {
        return Ok(false);
    };
    let Some(user) = store::auth::user_by_id(&store.pool, user_id).await? else {
        return Ok(false);
    };
    Ok(user
        .permissions
        .iter()
        .any(|p| p == "role:read" || p == "user:read" || p == "permission:read"))
}

async fn execute_auth(store: &Store) -> Result<Vec<Value>, ProteinError> {
    let mut out = Vec::new();
    for (id, name, permissions) in store::auth::list_roles(&store.pool).await? {
        out.push(json!({
            "kind": "role",
            "id": id.to_string(),
            "name": name,
            "permissions": permissions,
        }));
    }
    for (id, username, name, role) in store::auth::list_users(&store.pool).await? {
        let person = store::auth::person_for_user(&store.pool, id).await?;
        let person_record = match person.as_deref() {
            Some(uid) => store::records::get(&store.pool, uid).await?,
            None => None,
        };
        out.push(json!({
            "kind": "user",
            "id": id.to_string(),
            "username": username,
            "name": name,
            "role": role,
            "person": person,
            "person_head": person_record.as_ref().map(|record| record.head.as_str()),
            "person_slug": person_record.as_ref().and_then(|record| record.slug.as_deref()),
        }));
    }
    out.push(json!({
        "kind": "permission_catalog",
        "keys": utils::auth::all_permission_keys(),
    }));
    Ok(out)
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
        Source::Record | Source::Promise | Source::Decision | Source::Transfer
    )
}

// ------------------------------------------------------------------- records

/// Records matching a Protein's filter, ignoring aggregate/order/limit — the
/// selection primitive shared by `execute_records` and any consumer that
/// needs actual rows rather than the JSON wire shape (Protein-driven Sync and
/// File Sync: resolve WHICH records travel by evaluating a saved Protein).
pub async fn matching_records(
    store: &Store,
    protein: &Protein,
    visible: Option<&HashSet<String>>,
) -> Result<Vec<store::records::RecordRow>, ProteinError> {
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
    Ok(rows)
}

async fn execute_records(
    store: &Store,
    protein: &Protein,
    visible: Option<&HashSet<String>>,
) -> Result<Vec<Value>, ProteinError> {
    let mut rows = matching_records(store, protein, visible).await?;

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
            "organ": r.organ_uid,
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
    let wants_all = links.kind.as_deref().map(str::trim) == Some("*")
        || links.kinds.iter().any(|kind| kind.trim() == "*");
    let rows = if wants_all {
        store::links::all_links(&store.pool).await?
    } else {
        let kind_uids = resolve_link_kind_uids(store, links).await?;
        if kind_uids.is_empty() {
            return Ok(vec![]);
        }
        store::links::links_of_kinds(&store.pool, &kind_uids).await?
    };

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
            let created_at = store::records::created_at(&store.pool, &message.uid).await?;
            let (created_by, sender) = creator_info(store, &message.uid).await?;
            messages.push(json!({
                "uid": message.uid,
                "head": message.head,
                "body": message.body,
                "quantity": message.quantity,
                "parent_message_uid": parent_message_uid,
                "created_at": created_at,
                "created_by": created_by,
                "sender": sender,
            }));
            if messages.len() >= messages_limit {
                break;
            }
        }
        let thread_created_at = store::records::created_at(&store.pool, &thread.uid).await?;
        let (thread_created_by, thread_sender) = creator_info(store, &thread.uid).await?;
        out.push(json!({
            "uid": thread.uid,
            "head": thread.head,
            "body": thread.body,
            "quantity": thread.quantity,
            "created_at": thread_created_at,
            "created_by": thread_created_by,
            "sender": thread_sender,
            "messages": messages,
        }));
    }
    Ok(out)
}

/// A record's creator (its earliest fact's actor_uid, the logged-in
/// `app_user.id` as a string — a DIFFERENT identity namespace than the
/// Ledger's record/concept uids): the raw id (for the client's "is this
/// mine" delete-button check, matched against its own viewer id) and a
/// best-effort display name (falling back to username). Both are `None`
/// when there's no creator (local-no-auth mode, every action's actor is
/// `None`) or the actor_uid isn't a numeric app_user id. Never an error: a
/// name is a nicety, not something a message should fail to render over.
async fn creator_info(
    store: &Store,
    record_uid: &str,
) -> Result<(Option<String>, Option<String>), ProteinError> {
    let Some(actor_uid) = store::facts::creator_uid(&store.pool, record_uid).await? else {
        return Ok((None, None));
    };
    let Ok(user_id) = actor_uid.parse::<i64>() else {
        return Ok((None, None));
    };
    let Some(user) = store::auth::user_by_id(&store.pool, user_id).await? else {
        return Ok((None, None));
    };
    let name = if user.name.trim().is_empty() {
        user.username
    } else {
        user.name
    };
    Ok((Some(actor_uid), Some(name)))
}

// ---------------------------------------------------------------- predicates

struct PredicateCtx {
    /// concept name/uid -> the DAG family (root + descendants) as a set.
    concept_families: HashMap<String, HashSet<String>>,
    /// `near.of` anchor token -> the anchor's place (None if it has none).
    anchors: HashMap<String, Option<nucleus::place::Place>>,
    /// (kind, to) token -> set of record uids that have a `kind`-link to `to`.
    link_sources: HashMap<(String, String), HashSet<String>>,
    /// organ token (slug or uid) -> resolved organ uid (`None` = unresolvable).
    organs: HashMap<String, Option<String>>,
}

impl PredicateCtx {
    async fn prepare(store: &Store, preds: &[Predicate]) -> Result<Self, ProteinError> {
        let mut ctx = PredicateCtx {
            concept_families: HashMap::new(),
            anchors: HashMap::new(),
            link_sources: HashMap::new(),
            organs: HashMap::new(),
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
            Predicate::OrganEq(token) => {
                let resolved = store::records::resolve(&store.pool, token)
                    .await?
                    .map(|r| r.uid);
                self.organs.insert(token.clone(), resolved);
            }
            Predicate::OrganIn(tokens) => {
                for token in tokens {
                    if self.organs.contains_key(token) {
                        continue;
                    }
                    let resolved = store::records::resolve(&store.pool, token)
                        .await?
                        .map(|r| r.uid);
                    self.organs.insert(token.clone(), resolved);
                }
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
                Predicate::OrganEq(token) => {
                    match (self.organs.get(token).and_then(|o| o.as_deref()), &r.organ_uid) {
                        (Some(resolved), Some(actual)) => resolved == actual,
                        _ => false,
                    }
                }
                Predicate::OrganIn(tokens) => match &r.organ_uid {
                    Some(actual) => tokens.iter().any(|token| {
                        self.organs.get(token).and_then(|o| o.as_deref()) == Some(actual.as_str())
                    }),
                    None => false,
                },
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
    if promise_states.is_empty() {
        return "draft";
    }
    if promise_states.iter().all(|s| matches!(s, Withdrawn)) {
        return "withdrawn";
    }
    if promise_states.iter().all(|s| matches!(s, Kept)) {
        return "settled";
    }
    if promise_states.iter().any(|s| matches!(s, Kept)) {
        return "partially_settled";
    }
    if promise_states.iter().any(|s| matches!(s, Broken)) {
        return "broken";
    }
    if promise_states.iter().any(|s| matches!(s, Active)) {
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
    if policy
        && promise_states
            .iter()
            .filter(|state| !matches!(state, Withdrawn))
            .all(|state| matches!(state, Agreed | Kept))
    {
        return "agreed";
    }
    if promise_states
        .iter()
        .any(|s| matches!(s, Proposed | Agreed))
    {
        return "proposed";
    }
    "draft"
}

fn agreement_required(agreement_type: &str, agreement_pct: Option<i64>, parties: usize) -> usize {
    match agreement_type {
        "full" => parties,
        "percentage" => {
            let pct = agreement_pct.unwrap_or(100).clamp(0, 100) as usize;
            parties.saturating_mul(pct).div_ceil(100)
        }
        // Individual/dependency readiness is path-derived. The overview still
        // reports how many people have committed without inventing a bundle
        // quorum for those policies.
        _ => 0,
    }
}

async fn execute_transfers(
    store: &Store,
    protein: &Protein,
    visible: Option<&HashSet<String>>,
    subject: Option<&str>,
) -> Result<Vec<Value>, ProteinError> {
    let records = store::records::list_all(&store.pool).await?;
    let records_by_uid: HashMap<_, _> = records
        .iter()
        .cloned()
        .map(|record| (record.uid.clone(), record))
        .collect();
    let concept_names: HashMap<_, _> = store::concepts::list_all(&store.pool)
        .await?
        .into_iter()
        .map(|concept| (concept.uid, concept.canonical_name))
        .collect();
    let viewer = TransferViewer::resolve(store, subject).await?;
    let create_blockers = viewer.create_blockers();
    let create = create_blockers.is_empty();
    let person_record = match viewer.person.as_deref() {
        Some(uid) => records_by_uid.get(uid),
        None => None,
    };
    // Transfer is a mixed Protein source: the context row remains available
    // even when a new Cell has no transfers, so creation controls never infer
    // authority from an empty list or from client-side viewer state.
    let mut out = vec![json!({
        "kind": "transfer_context",
        "viewer": {
            "local": viewer.local,
            "recognized": viewer.recognized,
            "app_user": viewer.subject,
            "person": viewer.person,
            "person_head": person_record.map(|record| record.head.as_str()),
            "person_slug": person_record.and_then(|record| record.slug.as_deref()),
        },
        "capabilities": { "create": create },
        "blocking_reasons": { "create": create_blockers },
    })];
    let mut transfer_count = 0usize;
    for t in store::transfers::list_all(&store.pool).await? {
        let uid = &t.transfer.record_uid;
        let creator = store::facts::creator_uid(&store.pool, uid).await?;
        if visible.is_some_and(|targets| !targets.contains(uid)) {
            let viewer_created = viewer
                .subject
                .as_deref()
                .is_some_and(|subject| creator.as_deref() == Some(subject));
            let viewer_participates = match viewer.person.as_deref() {
                Some(person) => store::transfers::party_for_actor(&store.pool, uid, person)
                    .await?
                    .is_some(),
                None => false,
            };
            // Hidden remains the default, but creator and named parties are
            // intrinsic recipients of the commitment and cannot be hidden
            // from their own Transfer by a missing legacy visibility rule.
            if !viewer_created && !viewer_participates {
                continue;
            }
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
                .and_then(|r| records_by_uid.get(r))
                .and_then(|record| record.concept_uid.clone())
                .unwrap_or_else(|| "(none)".into());
            *balance.entry(key).or_insert(0.0) += p.delta;
        }
        let balanced = !balance.is_empty() && balance.values().all(|v| v.abs() < 1e-9);
        let balance_detail = balance
            .iter()
            .map(|(concept, delta)| {
                json!({
                    "concept": if concept == "(none)" { None } else { Some(concept) },
                    "concept_name": if concept == "(none)" {
                        None
                    } else {
                        concept_names.get(concept)
                    },
                    "delta": delta,
                })
            })
            .collect::<Vec<_>>();
        let committed = levels.iter().filter(|level| **level >= 2).count();
        let reviewed = levels.iter().filter(|level| **level >= 1).count();
        let required = agreement_required(
            &t.transfer.agreement_type,
            t.transfer.agreement_pct,
            parties.len(),
        );
        let policy_satisfied = nucleus::transfer::AgreementType::parse(
            &t.transfer.agreement_type,
        )
        .map(|agreement| {
            nucleus::transfer::policy_satisfied(
                agreement,
                t.transfer.agreement_pct.map(|pct| pct as u8),
                &levels,
            )
        })
        .unwrap_or(false);
        let viewer_party = viewer.person.as_deref().and_then(|person| {
            parties
                .iter()
                .find(|(_, actor, _)| actor == person)
                .map(|(party, _, _)| party.clone())
        });
        let is_creator = viewer.local
            || viewer
                .subject
                .as_deref()
                .is_some_and(|subject| creator.as_deref() == Some(subject));
        let is_participant = viewer.local || viewer_party.is_some();
        let has_identity = viewer.local || viewer.person.is_some();
        let can_update = viewer.local || viewer.has_permission("transfer:update");
        let can_edit = has_identity && can_update && (is_creator || is_participant);
        let can_agree = has_identity && can_update && is_participant;
        let can_activate = can_edit && policy_satisfied;

        let edit_blockers = viewer.update_blockers(is_creator || is_participant);
        let agree_blockers = viewer.update_blockers(is_participant);
        let mut activate_blockers = edit_blockers.clone();
        if !policy_satisfied {
            activate_blockers.push("agreement_policy_not_satisfied");
        }
        let mut confirmation_blockers = agree_blockers.clone();
        confirmation_blockers.push("confirmation_scope_not_modeled");
        let mut settlement_blockers = agree_blockers.clone();
        settlement_blockers.push("settlement_scope_not_modeled");
        let mut state_counts: std::collections::BTreeMap<&str, usize> = Default::default();
        for state in &states {
            *state_counts.entry(state.as_str()).or_default() += 1;
        }
        let confirmations = store::facts::for_record(&store.pool, uid, 10_000)
            .await?
            .into_iter()
            .filter_map(|fact| {
                let payload = serde_json::from_str::<Value>(fact.payload.as_deref()?).ok()?;
                let kind = payload.get("confirmation")?.as_str()?;
                Some(json!({
                    "kind": kind,
                    "actor": fact.actor_uid,
                    "at": fact.at,
                    "fact": fact.uid,
                }))
            })
            .collect::<Vec<_>>();
        out.push(json!({
            "kind": "transfer",
            "uid": uid,
            "slug": t.slug,
            "head": t.head,
            "status": status,
            "balance": balance,
            "balance_detail": balance_detail,
            "balanced": balanced,
            "active": t.transfer.active,
            "agreement_type": t.transfer.agreement_type,
            "agreement_pct": t.transfer.agreement_pct,
            "settlement": t.transfer.settlement,
            "visibility": t.transfer.visibility,
            "max_proximity": t.transfer.max_proximity,
            "satiation": t.transfer.satiation,
            "parent": t.transfer.parent_uid,
            "parent_head": t.transfer.parent_uid.as_ref()
                .and_then(|uid| records_by_uid.get(uid))
                .map(|record| record.head.as_str()),
            "parent_slug": t.transfer.parent_uid.as_ref()
                .and_then(|uid| records_by_uid.get(uid))
                .and_then(|record| record.slug.as_deref()),
            "source": t.transfer.source_uid,
            "source_head": t.transfer.source_uid.as_ref()
                .and_then(|uid| records_by_uid.get(uid))
                .map(|record| record.head.as_str()),
            "source_slug": t.transfer.source_uid.as_ref()
                .and_then(|uid| records_by_uid.get(uid))
                .and_then(|record| record.slug.as_deref()),
            "reserve_default": t.transfer.reserve_default,
            "require_confirmation": t.transfer.require_confirmation,
            "viewer_party": viewer_party,
            "capabilities": {
                "edit_terms": can_edit,
                "add_party": can_edit,
                "add_promise": can_edit,
                "review": can_agree,
                "commit": can_agree,
                "activate": can_activate,
                // Confirmation and settlement are deliberately not advertised
                // until evidence is occurrence- and side-specific.
                "confirm_delivery": false,
                "confirm_receipt": false,
                "settle": false,
            },
            "blocking_reasons": {
                "edit_terms": edit_blockers,
                "add_party": edit_blockers,
                "add_promise": edit_blockers,
                "review": agree_blockers,
                "commit": agree_blockers,
                "activate": activate_blockers,
                "confirm_delivery": confirmation_blockers,
                "confirm_receipt": confirmation_blockers,
                "settle": settlement_blockers,
            },
            "agreement": {
                "reviewed": reviewed,
                "committed": committed,
                "required": required,
                "total": parties.len(),
                "policy_satisfied": policy_satisfied,
            },
            "progress": state_counts,
            "confirmations": confirmations,
            "parties": parties
                .iter()
                .map(|(party, actor, level)| json!({
                    "uid": party,
                    "actor": actor,
                    "actor_head": records_by_uid.get(actor).map(|record| record.head.as_str()),
                    "actor_slug": records_by_uid.get(actor).and_then(|record| record.slug.as_deref()),
                    "level": level,
                }))
                .collect::<Vec<_>>(),
            "promises": promises
                .iter()
                .map(|p| {
                    let record = p.record_uid.as_ref().and_then(|uid| records_by_uid.get(uid));
                    json!({
                        "uid": p.uid,
                        "record": p.record_uid,
                        "record_head": record.map(|record| record.head.as_str()),
                        "record_slug": record.and_then(|record| record.slug.as_deref()),
                        "record_quantity": record.map(|record| record.quantity),
                        "concept": record.and_then(|record| record.concept_uid.as_deref()),
                        "concept_name": record
                            .and_then(|record| record.concept_uid.as_ref())
                            .and_then(|uid| concept_names.get(uid)),
                        "unit": record.and_then(|record| record.unit_uid.as_deref()),
                        "unit_name": record
                            .and_then(|record| record.unit_uid.as_ref())
                            .and_then(|uid| concept_names.get(uid)),
                        "delta": p.delta,
                        "state": p.state.as_str(),
                        "party": p.party_uid,
                        "window_end": p.window_end,
                        "condition": p.condition,
                        "reserve_from": p.reserve_from,
                    })
                })
                .collect::<Vec<_>>(),
        }));
        transfer_count += 1;
        if protein.limit.is_some_and(|limit| transfer_count >= limit) {
            break;
        }
    }
    Ok(out)
}

#[derive(Debug, Default)]
struct TransferViewer {
    local: bool,
    recognized: bool,
    subject: Option<String>,
    person: Option<String>,
    permissions: HashSet<String>,
}

impl TransferViewer {
    async fn resolve(store: &Store, subject: Option<&str>) -> Result<Self, ProteinError> {
        let Some(subject) = subject else {
            return Ok(Self {
                local: true,
                recognized: true,
                ..Self::default()
            });
        };
        let Ok(user_id) = subject.parse::<i64>() else {
            return Ok(Self {
                subject: Some(subject.to_string()),
                ..Self::default()
            });
        };
        let Some(user) = store::auth::user_by_id(&store.pool, user_id).await? else {
            return Ok(Self {
                subject: Some(subject.to_string()),
                ..Self::default()
            });
        };
        let person = store::auth::person_for_user(&store.pool, user.id).await?;
        Ok(Self {
            local: false,
            recognized: true,
            subject: Some(subject.to_string()),
            person,
            permissions: user.permissions.into_iter().collect(),
        })
    }

    fn has_permission(&self, permission: &str) -> bool {
        self.permissions.contains(permission)
    }

    fn create_blockers(&self) -> Vec<&'static str> {
        if self.local {
            return Vec::new();
        }
        let mut blockers = Vec::new();
        if !self.recognized {
            blockers.push("auth_subject_unrecognized");
        }
        if self.person.is_none() {
            blockers.push("missing_person_identity");
        }
        if !self.has_permission("transfer:create") {
            blockers.push("missing_transfer_create_permission");
        }
        blockers
    }

    fn update_blockers(&self, has_relationship: bool) -> Vec<&'static str> {
        if self.local {
            return Vec::new();
        }
        let mut blockers = Vec::new();
        if !self.recognized {
            blockers.push("auth_subject_unrecognized");
        }
        if self.person.is_none() {
            blockers.push("missing_person_identity");
        }
        if !self.has_permission("transfer:update") {
            blockers.push("missing_transfer_update_permission");
        }
        if !has_relationship {
            blockers.push("not_transfer_creator_or_participant");
        }
        blockers
    }
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
