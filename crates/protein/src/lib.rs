//! The Protein layer (blueprint Part VII): how any interface asks Lince for
//! data. DNA is what you store; Protein is how it comes out and shows its
//! power. Sands and every first-party surface speak only Protein (reads) and
//! Actions (writes, in `engine`); they never see tables or SQL — which is what
//! makes the storage engine replaceable underneath.
//!
//! **Protein never mutates.** This crate has no write path at all.
//!
//! Sources: `record | promise | decision | fact | concept | transfer |
//! transfer_settlement_preview | transfer_bulk_completion_preview`.
//! Boolean predicate tree with Lingua-DAG `concept_in` (what a Record IS) and
//! `classified_in` (what a CHANGE was for); includes `facts` (provenance),
//! `promises`, `links` (with tree `depth`), `threads`, `extension`,
//! `availability`, and `projection` (promise fold); aggregation (`sum`/`count`
//! by total/concept/classification/kind/cause_kind/day, exact and
//! unit-separated — the statistics workhorse, and equally at home totalling
//! spending, stock consumption, or hours);
//! ordering `topo(kind)` + field asc/desc; limit. Rows come out as JSON — the
//! wire shape sands consume. Live subscriptions ride the engine's `fact_bus`
//! (see `affects`): snapshot, then re-execute on relevant commits.
//!
//! `include: projection` folds **promises** forward (the planned trajectory
//! from agreed/active commitments). Full rule simulation needs the Karma
//! registry and stays engine-side (`Engine::project`) so this crate keeps its
//! read-only-by-construction guarantee.
#![recursion_limit = "512"]

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use ed25519_dalek::{Signature, Verifier as _, VerifyingKey};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use store::Store;

pub type ProteinError = store::StoreError;

/// Stable wire code for Protein validation failures. Store failures remain
/// uncoded; only deliberately typed Protein protocol errors are exposed.
pub fn error_code(error: &ProteinError) -> Option<String> {
    let store::sqlx::Error::Protocol(message) = error else {
        return None;
    };
    let code = message
        .split_once(':')
        .map_or(message.as_str(), |(code, _)| code);
    if code.starts_with("protein_")
        && code.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
        })
    {
        Some(code.to_string())
    } else {
        None
    }
}

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
    /// No grouping: one bucket for everything the filter matched (still split
    /// by unit). "What is my net this month" is a real question and expressing
    /// it as a group-by over some arbitrary key and re-summing on the client
    /// would put exact arithmetic back in JavaScript.
    Total,
    /// The Record's own concept — what the thing IS.
    Concept,
    /// Fact source: the concept the CHANGE was classified with — what it was
    /// FOR. `concept` groups a food purchase under the wallet it came from;
    /// `classification` groups it under food. Both are needed, and conflating
    /// them silently answers the wrong question.
    Classification,
    Kind,
    /// Fact source: group by the fact's cause_kind (W-finance).
    CauseKind,
    /// Fact source: group by calendar day (`YYYY-MM-DD` of `at`).
    Day,
    /// Group by calendar month (`YYYY-MM` of `at`). The natural bucket for a
    /// timeline that spans a year, where a daily point is noise.
    Month,
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
    /// Private, quantity-sensitive settlement review for one occurrence. This
    /// source requires direct `uid_eq` and `quantity_eq` predicates and emits
    /// no row unless the caller is the signed source-promise owner.
    TransferSettlementPreview,
    /// Reviewed, read-only plan for asserting this Person's currently missing
    /// delivery/receipt claims across an explicit occurrence selection.
    TransferBulkCompletionPreview,
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
    /// Durable Program/Frequency definitions, activation epochs, scheduler
    /// cursors/batches, semantic occurrences, frozen Program epochs, and
    /// deterministic runs. Remote subjects see no rows until fine-grained
    /// Karma visibility grants exist.
    Karma,
    /// One classified quantity axis through time: what settled, where it stands
    /// now, and what is declared ahead.
    ///
    /// This exists because those three are one question, not three panels. A
    /// person asking "how is `@rent` going" wants the months behind, the
    /// running position, and the months ahead on a single line — and stitching
    /// that together on the client would require summing exact decimals in
    /// JavaScript, which is where exactness goes to die.
    ///
    /// The future is *declared*, never invented: it is the dates recurring
    /// rules produce plus promises already made. Nothing here forecasts, and
    /// nothing here writes a Fact.
    Timeline,
    /// Authored changes with the handle needed to correct them.
    ///
    /// [`Source::Fact`] answers what the Ledger holds; this answers what a
    /// person typed and may still fix. The difference that matters to a surface
    /// is `uid` and `revision`: revising or voiding requires both, and a Fact
    /// has neither because a Fact is not editable.
    Entry,
    /// Standing recurring declarations and the dates they produce.
    ///
    /// Rows are heterogeneous by `kind`: `"recurrence"` for a rule, and
    /// `"occurrence"` for one derived date with what became of it. Occurrences
    /// are derived on read, never stored.
    Recurrence,
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
    /// Focused Transfer bulk-preview selection. This predicate is not a
    /// general Record/Transfer filter.
    OccurrenceIn(Vec<String>),
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
    /// Promise source: state is one of these. Transfer source: at least one
    /// bundled promise currently has one of these states.
    StateIn(Vec<String>),
    /// Transfer source: match the exact signed terms revision.
    RevisionEq(u64),
    /// Transfer source: signed terms revision range filters.
    RevisionLt(u64),
    RevisionLte(u64),
    RevisionGt(u64),
    RevisionGte(u64),
    /// Transfer source: derived Transfer status is one of these.
    StatusIn(Vec<String>),
    /// Transfer source: the requesting viewer has one of these server-derived
    /// roles (`local`, `creator`, `participant`, `invitee`, `observer`).
    ViewerRoleIn(Vec<String>),
    /// Transfer source: at least one addressed invitation has one of these
    /// lifecycle states.
    InvitationStateIn(Vec<String>),
    /// Transfer source: a Person is a participant, addressee, inviter, or the
    /// assigned Person of one of the Transfer's promises.
    PersonEq(String),
    /// Transfer source: a bundled promise's signed unit equals this Lingua
    /// concept. Name and uid tokens resolve through Lingua.
    UnitEq(String),
    /// Transfer source: at least one promise window ends before/after the
    /// supplied RFC3339 instant.
    WindowEndBefore(String),
    WindowEndAfter(String),
    /// Fact source: `at` within the trailing window (`"30d"`, `"2h"`) or at or
    /// after an absolute RFC3339 instant.
    AtSince(String),
    /// Fact source: the exclusive end of the window, same spellings as
    /// `at_since`. Half-open `[since, before)` on purpose — adjacent periods
    /// must tile without one change being counted in both.
    AtBefore(String),
    /// Fact source: the concept the CHANGE ITSELF was classified with,
    /// DAG-expanded.
    ///
    /// Deliberately distinct from `concept_in`, which asks about the Record the
    /// change happened to. The two answer different questions and a query needs
    /// both: "what did I spend on food" selects Records by `@budget` and changes
    /// by `@food`. Collapsing them into one predicate is how a query silently
    /// answers something other than what was asked.
    ClassifiedIn(String),
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
    execute_for_with_signer(store, protein, subject, None).await
}

/// Execute for a subject while projecting the signing authority actually
/// available to the calling process. Public identity keys only prove that a
/// signature can be verified; they must never advertise write capability.
pub async fn execute_for_with_signer(
    store: &Store,
    protein: &Protein,
    subject: Option<&str>,
    installed_signer_actor: Option<&str>,
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
        Source::Timeline => execute_timeline(store, protein, visible).await?,
        Source::Entry => execute_entries(store, protein, visible).await?,
        Source::Recurrence => execute_recurrence(store, protein, visible).await?,
        // Lingua is shared vocabulary by design (III): concepts travel freely.
        Source::Concept => execute_concepts(store, protein).await?,
        Source::Transfer => {
            execute_transfers(
                store,
                protein,
                visible,
                subject,
                installed_signer_actor,
                None,
            )
            .await?
        }
        Source::TransferSettlementPreview => {
            execute_transfer_settlement_preview(store, protein, subject, installed_signer_actor)
                .await?
        }
        Source::TransferBulkCompletionPreview => {
            execute_transfer_bulk_completion_preview(
                store,
                protein,
                subject,
                installed_signer_actor,
            )
            .await?
        }
        Source::Auth => {
            if let Some(actor) = subject {
                if !actor_can_read_auth(store, actor).await? {
                    return Ok(vec![]); // same "hidden, not an error" shape as Decision
                }
            }
            execute_auth(store).await?
        }
        Source::Karma => {
            if visible.is_some() {
                return Ok(vec![]);
            }
            execute_karma(store, protein).await?
        }
    })
}

/// Build the recipient-specific hosted/replica payload at the same visibility
/// boundary as normal Transfer reads. Recipient-specific capabilities and
/// action templates remain so signed commands can be sent back to the origin;
/// raw proof signatures are intentionally absent.
pub async fn transfer_delivery_projection(
    store: &Store,
    transfer_uid: &str,
    recipient_person_uid: &str,
    recipient_organ_uid: &str,
) -> Result<Value, ProteinError> {
    let policies =
        store::transfer_delivery::policies_for_transfer(&store.pool, transfer_uid).await?;
    if !policies.iter().any(|policy| {
        policy.state == "active"
            && policy.recipient_person_uid == recipient_person_uid
            && policy.recipient_organ_uid == recipient_organ_uid
    }) {
        return Err(transfer_query_error(
            "protein_transfer_delivery_policy_missing",
            "recipient has no active Transfer delivery policy",
        ));
    }
    let protein = Protein {
        source: Source::Transfer,
        filter: vec![Predicate::UidEq(transfer_uid.to_string())],
        include: Include::default(),
        aggregate: None,
        order: Vec::new(),
        limit: Some(1),
    };
    let visible = HashSet::from([transfer_uid.to_string()]);
    let rows = execute_transfers(
        store,
        &protein,
        Some(&visible),
        Some(recipient_organ_uid),
        Some(recipient_person_uid),
        Some(TransferViewer::delivery(
            recipient_organ_uid,
            recipient_person_uid,
        )),
    )
    .await?;
    let mut row = rows
        .into_iter()
        .find(|row| row.get("kind").and_then(Value::as_str) == Some("transfer"))
        .ok_or_else(|| {
            transfer_query_error(
                "protein_transfer_delivery_not_visible",
                "recipient cannot view this Transfer",
            )
        })?;
    let projected_revision = row
        .get("revision")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    if let Some(occurrences) = row.get_mut("occurrences").and_then(Value::as_array_mut) {
        for occurrence in occurrences {
            let Some(occurrence_uid) = occurrence
                .get("uid")
                .and_then(Value::as_str)
                .map(str::to_string)
            else {
                continue;
            };
            let source_owner: Option<(String,)> = store::sqlx::query_as(
                "SELECT p.party_uid FROM transfer_occurrence o
                 JOIN promise p ON p.uid = o.promise_uid WHERE o.uid = ?",
            )
            .bind(&occurrence_uid)
            .fetch_optional(&store.pool)
            .await?;
            if source_owner.as_ref().map(|row| row.0.as_str()) != Some(recipient_person_uid) {
                continue;
            }
            let remaining = occurrence
                .pointer("/settlement_progress/remaining_quantity")
                .and_then(Value::as_f64)
                .unwrap_or_default();
            let ready = remaining > 0.0
                && occurrence.get("delivery_claimed").and_then(Value::as_bool) == Some(true)
                && occurrence.get("receipt_claimed").and_then(Value::as_bool) == Some(true)
                && occurrence.get("disputed").and_then(Value::as_bool) != Some(true);
            if let Some(object) = occurrence.as_object_mut() {
                object.insert("remote_settlement_preview".into(), json!({
                    "canonical_quantity": remaining,
                    "expected_remaining_quantity": remaining,
                    "capabilities": { "begin": ready },
                    "blocking_reasons": { "begin": if ready { Vec::<&str>::new() } else { vec!["occurrence_not_ready"] } },
                    "action_payload": {
                        "action": "begin-remote-transfer-settlement",
                        "transfer": transfer_uid,
                        "occurrence": occurrence_uid,
                        "expected_revision": projected_revision,
                        "expected_remaining_quantity": remaining,
                        "canonical_quantity": remaining,
                        "request_id": Value::Null,
                        "person": recipient_person_uid,
                    }
                }));
            }
        }
    }
    strip_delivery_mutation_and_proof(&mut row);
    if let Some(object) = row.as_object_mut() {
        object.insert("delivery_read_only".into(), Value::Bool(true));
    }
    Ok(row)
}

fn strip_delivery_mutation_and_proof(value: &mut Value) {
    match value {
        Value::Object(object) => {
            object.remove("proof");
            object.remove("fact_signature");
            object.remove("action_intent");
            for child in object.values_mut() {
                strip_delivery_mutation_and_proof(child);
            }
        }
        Value::Array(values) => {
            for child in values {
                strip_delivery_mutation_and_proof(child);
            }
        }
        _ => {}
    }
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
    execute_saved_with_signer(store, slug_or_uid, subject, None).await
}

/// Saved-Protein counterpart of [`execute_for_with_signer`].
pub async fn execute_saved_with_signer(
    store: &Store,
    slug_or_uid: &str,
    subject: Option<&str>,
    installed_signer_actor: Option<&str>,
) -> Result<Vec<Value>, ProteinError> {
    let record = store::records::resolve(&store.pool, slug_or_uid)
        .await?
        .ok_or_else(|| store::sqlx::Error::Protocol(format!("no saved protein {slug_or_uid}")))?;
    let ast = store::records::get_extension(&store.pool, &record.uid, "lince.protein")
        .await?
        .ok_or_else(|| store::sqlx::Error::Protocol(format!("{slug_or_uid} has no protein AST")))?;
    let protein: Protein = serde_json::from_value(ast)
        .map_err(|e| store::sqlx::Error::Protocol(format!("bad protein AST: {e}")))?;
    execute_for_with_signer(store, &protein, subject, installed_signer_actor).await
}

/// Coarse live-subscription invalidation: does a committed fact possibly
/// change this Protein's result? v1 answer: any fact touching the source
/// domain does. The transport re-executes on `true`; refinement comes later.
pub fn affects(protein: &Protein, _fact: &nucleus::Fact) -> bool {
    matches!(
        protein.source,
        Source::Record
            | Source::Promise
            | Source::Decision
            | Source::Transfer
            | Source::TransferSettlementPreview
            | Source::TransferBulkCompletionPreview
            | Source::Karma
            // A capture, a correction, and an applied occurrence all commit
            // Facts, and all three of these read them. Leaving them out would
            // leave a surface showing a total that stopped being true.
            | Source::Timeline
            | Source::Entry
            | Source::Recurrence
    )
}

async fn execute_karma(store: &Store, protein: &Protein) -> Result<Vec<Value>, ProteinError> {
    if protein.aggregate.is_some() {
        return Err(karma_query_error(
            "protein_karma_aggregate_unsupported",
            "Karma object unions cannot be aggregated",
        ));
    }
    if protein.include.facts.is_some()
        || protein.include.promises.is_some()
        || protein.include.links.is_some()
        || protein.include.threads.is_some()
        || protein.include.availability
        || protein.include.extension.is_some()
        || protein.include.projection.is_some()
    {
        return Err(karma_query_error(
            "protein_karma_include_unsupported",
            "Karma rows are already complete typed projections",
        ));
    }

    let mut rows = Vec::new();
    for handle in store::karma::programs::list_handles(&store.pool).await? {
        let can_activate = handle.active_revision_hash.as_ref() != Some(&handle.head_revision_hash);
        let can_pause = handle.status == nucleus::karma::DefinitionStatus::Active;
        rows.push(json!({
            "object_kind": "program",
            "uid": handle.record_uid,
            "slug": handle.slug,
            "handle_revision": handle.handle_revision,
            "status": handle.status.as_str(),
            "head_revision_hash": handle.head_revision_hash,
            "active_revision_hash": handle.active_revision_hash,
            "owner_person_uid": handle.owner_person_uid,
            "created_at": handle.created_at,
            "updated_at": handle.updated_at,
            "capabilities": {
                "revise": true,
                "activate": can_activate,
                "pause": can_pause,
            },
            "blocking_reasons": {
                "revise": Vec::<&str>::new(),
                "activate": if can_activate { Vec::<&str>::new() } else { vec!["head_already_active"] },
                "pause": if can_pause { Vec::<&str>::new() } else { vec!["program_not_active"] },
            },
            "action_templates": {
                "revise": {
                    "action": "revise-karma-program",
                    "request_id": Value::Null,
                    "program_uid": handle.record_uid,
                    "expected_handle_revision": handle.handle_revision,
                    "program": Value::Null,
                },
                "activate": {
                    "action": "activate-karma-program",
                    "request_id": Value::Null,
                    "program_uid": handle.record_uid,
                    "expected_handle_revision": handle.handle_revision,
                    "revision_hash": handle.head_revision_hash,
                },
                "pause": {
                    "action": "pause-karma-program",
                    "request_id": Value::Null,
                    "program_uid": handle.record_uid,
                    "expected_handle_revision": handle.handle_revision,
                },
            },
        }));
    }
    for revision in store::karma::programs::list_revisions(&store.pool).await? {
        rows.push(json!({
            "object_kind": "program_revision",
            "uid": revision.revision_hash,
            "program_uid": revision.program_uid,
            "revision_hash": revision.revision_hash,
            "program": revision.program,
            "canonical_dsl": revision.canonical_dsl,
            "proof": revision.proof,
            "created_at": revision.created_at,
        }));
    }
    for handle in store::karma::frequencies::list_handles(&store.pool).await? {
        let can_activate = handle.active_revision_hash.as_ref() != Some(&handle.head_revision_hash);
        let can_pause = handle.status == nucleus::karma::DefinitionStatus::Active;
        rows.push(json!({
            "object_kind": "frequency",
            "uid": handle.record_uid,
            "slug": handle.slug,
            "handle_revision": handle.handle_revision,
            "status": handle.status.as_str(),
            "head_revision_hash": handle.head_revision_hash,
            "active_revision_hash": handle.active_revision_hash,
            "active_activation_hash": handle.active_activation_hash,
            "latest_activation_hash": handle.latest_activation_hash,
            "owner_person_uid": handle.owner_person_uid,
            "created_at": handle.created_at,
            "updated_at": handle.updated_at,
            "capabilities": {
                "revise": true,
                "activate": can_activate,
                "pause": can_pause,
                "set_parameters": can_pause,
                "reset_parameters": can_pause,
            },
            "blocking_reasons": {
                "revise": Vec::<&str>::new(),
                "activate": if can_activate { Vec::<&str>::new() } else { vec!["head_already_active"] },
                "pause": if can_pause { Vec::<&str>::new() } else { vec!["frequency_not_active"] },
                "set_parameters": if can_pause { Vec::<&str>::new() } else { vec!["frequency_not_active"] },
                "reset_parameters": if can_pause { Vec::<&str>::new() } else { vec!["frequency_not_active"] },
            },
            "requires_runtime_admission": true,
            "action_templates": {
                "revise": {
                    "action": "revise-karma-frequency",
                    "request_id": Value::Null,
                    "frequency_uid": handle.record_uid,
                    "expected_handle_revision": handle.handle_revision,
                    "frequency": Value::Null,
                },
                "activate": {
                    "action": "activate-karma-frequency",
                    "request_id": Value::Null,
                    "frequency_uid": handle.record_uid,
                    "expected_handle_revision": handle.handle_revision,
                    "revision_hash": handle.head_revision_hash,
                    "parameter_overrides": {},
                },
                "pause": {
                    "action": "pause-karma-frequency",
                    "request_id": Value::Null,
                    "frequency_uid": handle.record_uid,
                    "expected_handle_revision": handle.handle_revision,
                },
            },
        }));
    }
    for revision in store::karma::frequencies::list_revisions(&store.pool).await? {
        rows.push(json!({
            "object_kind": "frequency_revision",
            "uid": revision.revision_hash,
            "frequency_uid": revision.frequency_uid,
            "revision_hash": revision.revision_hash,
            "frequency": revision.frequency,
            "canonical_dsl": revision.canonical_dsl,
            "default_compiled": revision.default_compiled,
            "created_at": revision.created_at,
        }));
    }
    for activation in store::karma::frequencies::list_activations(&store.pool).await? {
        rows.push(json!({
            "object_kind": "frequency_activation",
            "uid": activation.activation_hash,
            "activation_hash": activation.activation_hash,
            "frequency_uid": activation.epoch.frequency_uid(),
            "epoch": activation.epoch,
        }));
    }
    for cursor in store::karma::schedules::list_cursors(&store.pool).await? {
        rows.push(json!({
            "object_kind": "schedule_cursor",
            "uid": cursor.activation_hash,
            "activation_hash": cursor.activation_hash,
            "frequency_uid": cursor.frequency_uid,
            "cursor_revision": cursor.cursor_revision,
            "lifecycle": cursor.lifecycle.as_str(),
            "cursor": cursor.cursor,
            "deadline": cursor.deadline,
            "timer": cursor.timer,
            "overload_policy": cursor.overload_policy,
            "demand": cursor.demand,
            "admitted_resolution_ms": cursor.admitted_resolution_ms.map(|value| value.get()),
            "admission_degraded": cursor.admission_degraded,
            "admitted_at": cursor.admitted_at,
            "lease_fencing_token": cursor.lease_fencing_token,
            "lease_owner": cursor.lease_owner,
            "lease_expires_at": cursor.lease_expires_at,
            "last_occurrence_sequence": cursor.last_occurrence_sequence,
            "last_error": cursor.last_error_json
                .map(|value| serde_json::from_str::<Value>(&value))
                .transpose()
                .map_err(|error| karma_query_error("protein_karma_stored_json_invalid", error))?,
            "created_at": cursor.created_at,
            "updated_at": cursor.updated_at,
        }));
    }
    for occurrence in store::karma::schedules::list_occurrences(&store.pool).await? {
        let sequence = occurrence.occurrence.sequence();
        rows.push(json!({
            "object_kind": "schedule_occurrence",
            "uid": occurrence.occurrence_hash,
            "occurrence_hash": occurrence.occurrence_hash,
            "sequence": sequence,
            "occurrence": occurrence.occurrence,
            "created_at": occurrence.created_at,
        }));
    }
    for expansion in store::karma::expansions::list_cursors(&store.pool).await? {
        rows.push(json!({
            "object_kind": "schedule_occurrence_expansion",
            "uid": expansion.schedule_occurrence_hash,
            "schedule_occurrence_hash": expansion.schedule_occurrence_hash,
            "cadence": expansion.cadence.as_str(),
            "emission": expansion.emission.as_str(),
            "next_ordinal": expansion.next_ordinal,
            "total_items": expansion.total_items,
            "status": if expansion.completed { "completed" } else { "pending" },
            "created_at": expansion.created_at,
            "updated_at": expansion.updated_at,
        }));
    }
    for occurrence in store::karma::occurrences::list(&store.pool).await? {
        rows.push(json!({
            "object_kind": "occurrence",
            "uid": occurrence.occurrence_hash,
            "occurrence_hash": occurrence.occurrence_hash,
            "cell_sequence": occurrence.cell_sequence,
            "source_kind": occurrence.source_kind,
            "source_identity": occurrence.source_identity,
            "logical_at": occurrence.logical_at,
            "parent_occurrence_hash": occurrence.parent_occurrence_hash,
            "envelope": occurrence.envelope,
            "received_at": occurrence.received_at,
        }));
    }
    for epoch in store::karma::runs::list_epochs(&store.pool).await? {
        rows.push(json!({
            "object_kind": "program_epoch",
            "uid": epoch.epoch_hash,
            "epoch_hash": epoch.epoch_hash,
            "occurrence_hash": epoch.epoch.occurrence_hash,
            "cell_sequence": epoch.epoch.cell_sequence,
            "members": epoch.epoch.members,
            "next_member_ordinal": epoch.next_member_ordinal,
            "status": if epoch.completed { "completed" } else { "pending" },
            "created_at": epoch.created_at,
            "updated_at": epoch.updated_at,
            "completed_at": epoch.completed_at,
        }));
    }
    for run in store::karma::runs::list_runs(&store.pool).await? {
        rows.push(json!({
            "object_kind": "run",
            "uid": run.run_hash,
            "run_hash": run.run_hash,
            "occurrence_hash": run.run.occurrence_hash,
            "cell_sequence": run.run.cell_sequence,
            "logical_at": run.run.logical_at,
            "program_epoch_hash": run.run.program_epoch_hash,
            "member_ordinal": run.run.member_ordinal,
            "program_uid": run.run.program_uid,
            "program_revision_hash": run.run.program_revision_hash,
            "status": run.run.outcome.status_name(),
            "fuel_used": run.run.outcome.fuel_used(),
            "outcome": run.run.outcome,
            "created_at": run.created_at,
        }));
    }
    for state in store::karma::states::list_node_states(&store.pool).await? {
        rows.push(json!({
            "object_kind": "program_state",
            "uid": state.current_event_hash,
            "program_uid": state.program_uid,
            "node_id": state.node_id,
            "state_revision": state.state_revision,
            "current_event_hash": state.current_event_hash,
            "definition_revision_hash": state.definition_revision_hash,
            "activation_handle_revision": state.activation_handle_revision,
            "status": if state.state.is_some() { "value" } else { "reset" },
            "state": state.state,
            "updated_at": state.updated_at,
        }));
    }
    for event in store::karma::states::list_events(&store.pool).await? {
        rows.push(json!({
            "object_kind": "program_state_event",
            "uid": event.event_hash,
            "event_hash": event.event_hash,
            "program_uid": event.event.program_uid,
            "node_id": event.event.node_id,
            "state_revision": event.event.state_revision,
            "previous_event_hash": event.event.previous_event_hash,
            "source_run_hash": event.event.source_run_hash,
            "definition_revision_hash": event.event.definition_revision_hash,
            "activation_handle_revision": event.event.activation_handle_revision,
            "reset_reason": event.event.reset_reason,
            "status": if event.event.state.is_some() { "value" } else { "reset" },
            "state": event.event.state,
            "created_at": event.created_at,
        }));
    }
    for candidate in store::karma::candidates::list(&store.pool).await? {
        let state = store::karma::candidates::get_state(&store.pool, &candidate.candidate_hash)
            .await?
            .ok_or_else(|| {
                karma_query_error(
                    "protein_karma_candidate_state_missing",
                    candidate.candidate_hash.as_str(),
                )
            })?;
        rows.push(json!({
            "object_kind": "candidate",
            "uid": candidate.candidate_hash,
            "candidate_hash": candidate.candidate_hash,
            "source_run_hash": candidate.proposal.source_run_hash,
            "occurrence_hash": candidate.proposal.occurrence_hash,
            "program_uid": candidate.proposal.program_uid,
            "program_revision_hash": candidate.proposal.program_revision_hash,
            "node_id": candidate.proposal.node_id,
            "output": candidate.proposal.output,
            "route": candidate.proposal.route,
            "template": candidate.proposal.template,
            "fields": candidate.proposal.fields,
            "status": match state.status {
                nucleus::karma::CandidateStatus::Proposed => "proposed",
                nucleus::karma::CandidateStatus::Accepted => "accepted",
                nucleus::karma::CandidateStatus::Dismissed => "dismissed",
                nucleus::karma::CandidateStatus::Snoozed => "snoozed",
                _ => "unsupported",
            },
            "state_revision": state.state_revision,
            "snoozed_until": state.snoozed_until,
            "current_event_hash": state.current_event_hash,
            "actor_person_uid": state.actor_person_uid,
            "updated_at": state.updated_at,
            "created_at": candidate.created_at,
            "capabilities": {
                "accept": true,
                "dismiss": true,
                "snooze": true,
            },
            "creates_intent": false,
            // K5.1 made authority real, but accepting a proposal still translates
            // into no work: the candidate-to-intent bridge does not exist yet.
            "intent_blocking_reasons": ["karma_intent_not_implemented"],
            "action_templates": {
                "accept": {
                    "action": "respond-karma-candidate",
                    "request_id": Value::Null,
                    "candidate_hash": candidate.candidate_hash,
                    "expected_state_revision": state.state_revision,
                    "response": { "action": "accept" },
                },
                "dismiss": {
                    "action": "respond-karma-candidate",
                    "request_id": Value::Null,
                    "candidate_hash": candidate.candidate_hash,
                    "expected_state_revision": state.state_revision,
                    "response": { "action": "dismiss" },
                },
                "snooze": {
                    "action": "respond-karma-candidate",
                    "request_id": Value::Null,
                    "candidate_hash": candidate.candidate_hash,
                    "expected_state_revision": state.state_revision,
                    "response": { "action": "snooze", "until": Value::Null },
                },
            },
        }));
    }
    for handle in store::karma::grants::list_handles(&store.pool).await? {
        let revoked = handle.status == nucleus::karma::GrantStatus::Revoked;
        let head_is_active = handle.active_revision_hash.as_ref() == Some(&handle.head_revision_hash);
        let can_activate = !revoked && !head_is_active;
        let head = store::karma::grants::get_revision(&store.pool, &handle.record_uid, &handle.head_revision_hash)
            .await?
            .ok_or_else(|| {
                karma_query_error(
                    "protein_karma_grant_revision_missing",
                    handle.head_revision_hash.as_str(),
                )
            })?;
        rows.push(json!({
            "object_kind": "grant",
            "uid": handle.record_uid,
            "slug": handle.slug,
            "handle_revision": handle.handle_revision,
            "status": handle.status.as_str(),
            "head_revision_hash": handle.head_revision_hash,
            "active_revision_hash": handle.active_revision_hash,
            "principal_person_uid": handle.principal_person_uid,
            "purpose": head.revision.spec.purpose,
            "program_uid": head.revision.spec.program_uid,
            "program_revision": head.revision.spec.program_revision,
            "candidate_templates": head.revision.spec.candidate_templates,
            "capabilities_granted": head.revision.spec.capabilities,
            "targets": head.revision.spec.targets,
            "valid_from": head.revision.spec.valid_from,
            "expires_at": head.revision.spec.expires_at,
            "budget": head.revision.spec.budget,
            // Who vouched for the authority currently on offer, and with which key.
            "signature_provenance": {
                "signer_person_uid": head.signature.signer_person_uid,
                "key_id": head.signature.key_id,
                "revision_hash": head.revision_hash,
            },
            "created_at": handle.created_at,
            "updated_at": handle.updated_at,
            "capabilities": {
                "narrow": !revoked,
                "activate": can_activate,
                "revoke": !revoked,
            },
            "blocking_reasons": {
                "narrow": if revoked { vec!["grant_revoked"] } else { Vec::<&str>::new() },
                "activate": if revoked {
                    vec!["grant_revoked"]
                } else if head_is_active {
                    vec!["head_already_active"]
                } else {
                    Vec::<&str>::new()
                },
                "revoke": if revoked { vec!["grant_revoked"] } else { Vec::<&str>::new() },
            },
            // An active grant can now authorize a durable intent, but nothing
            // executes one: there is still no worker, lease, or dispatch.
            "authorizes_effects": false,
            "effect_blocking_reasons": ["karma_execution_not_implemented"],
            "action_templates": {
                "narrow": {
                    "action": "narrow-karma-grant",
                    "request_id": Value::Null,
                    "grant_uid": handle.record_uid,
                    "expected_handle_revision": handle.handle_revision,
                    "grant": head.revision.spec,
                },
                "activate": {
                    "action": "activate-karma-grant",
                    "request_id": Value::Null,
                    "grant_uid": handle.record_uid,
                    "expected_handle_revision": handle.handle_revision,
                    "revision_hash": handle.head_revision_hash,
                },
                "revoke": {
                    "action": "revoke-karma-grant",
                    "request_id": Value::Null,
                    "grant_uid": handle.record_uid,
                    "expected_handle_revision": handle.handle_revision,
                },
            },
        }));
    }
    for revision in store::karma::grants::list_revisions(&store.pool).await? {
        rows.push(json!({
            "object_kind": "grant_revision",
            "uid": revision.revision_hash,
            "grant_uid": revision.grant_uid,
            "revision_hash": revision.revision_hash,
            "principal_person_uid": revision.revision.principal_person_uid,
            "grant": revision.revision.spec,
            "signature_provenance": {
                "signer_person_uid": revision.signature.signer_person_uid,
                "key_id": revision.signature.key_id,
                "signature": revision.signature.signature,
            },
            "created_at": revision.created_at,
        }));
    }

    for intent in store::karma::intents::list(&store.pool).await? {
        let state = store::karma::intents::get_state(&store.pool, &intent.intent_hash)
            .await?
            .ok_or_else(|| {
                karma_query_error(
                    "protein_karma_intent_state_missing",
                    intent.intent_hash.as_str(),
                )
            })?;
        rows.push(json!({
            "object_kind": "intent",
            "uid": intent.intent_hash,
            "intent_hash": intent.intent_hash,
            "candidate_hash": intent.candidate_hash,
            "grant_uid": intent.grant_uid,
            "grant_revision_hash": intent.grant_revision_hash,
            "program_uid": intent.intent.program_uid,
            "program_revision_hash": intent.intent.program_revision_hash,
            "template": intent.intent.template,
            "capability": intent.intent.capability,
            "target": intent.intent.target,
            "fields": intent.intent.fields,
            "quantity": intent.intent.quantity,
            "idempotency_key": intent.intent.idempotency_key,
            "deadline": intent.intent.deadline,
            "status": state.status.as_str(),
            "state_revision": state.state_revision,
            // The head of this intent's transition chain, so an audit can walk
            // its whole lifecycle without trusting the projection.
            "current_event_hash": state.current_event_hash,
            "cancelled_reason": state.cancelled_reason,
            "actor_person_uid": state.actor_person_uid,
            // The whole reason this was permitted, auditable as one object.
            "policy_proof": intent.intent.authorization,
            "created_at": intent.created_at,
            "updated_at": state.updated_at,
            // K5.2 authorizes work and stops. No worker may claim this.
            "executable": false,
            "execution_blocking_reasons": ["karma_execution_not_implemented"],
            "action_templates": {},
        }));
    }

    let mut filtered = Vec::new();
    for row in rows {
        if karma_predicates_match(&row, &protein.filter)? {
            filtered.push(row);
        }
    }
    validate_karma_order(&protein.order)?;
    for order in protein.order.iter().rev() {
        let (field, descending) = match order {
            Order::Asc(field) => (field, false),
            Order::Desc(field) => (field, true),
            Order::Topo(_) => unreachable!("validated Karma order rejects topo"),
        };
        filtered.sort_by(|left, right| {
            let ordering = karma_sort_value(left, field).cmp(&karma_sort_value(right, field));
            if descending {
                ordering.reverse()
            } else {
                ordering
            }
        });
    }
    if let Some(limit) = protein.limit {
        filtered.truncate(limit);
    }
    Ok(filtered)
}

fn karma_predicates_match(row: &Value, predicates: &[Predicate]) -> Result<bool, ProteinError> {
    for predicate in predicates {
        let matches = match predicate {
            Predicate::All(children) => karma_predicates_match(row, children)?,
            Predicate::Any(children) => {
                let mut any = false;
                for child in children {
                    if karma_predicates_match(row, std::slice::from_ref(child))? {
                        any = true;
                        break;
                    }
                }
                any
            }
            Predicate::Not(child) => !karma_predicates_match(row, std::slice::from_ref(child))?,
            Predicate::KindEq(expected) => {
                row.get("object_kind").and_then(Value::as_str) == Some(expected)
            }
            Predicate::UidEq(expected) => row.get("uid").and_then(Value::as_str) == Some(expected),
            Predicate::SlugEq(expected) => {
                row.get("slug").and_then(Value::as_str) == Some(expected)
            }
            Predicate::StatusIn(expected) => row
                .get("status")
                .and_then(Value::as_str)
                .is_some_and(|status| expected.iter().any(|value| value == status)),
            _ => {
                return Err(karma_query_error(
                    "protein_karma_unsupported_predicate",
                    "predicate is not defined for Karma objects",
                ));
            }
        };
        if !matches {
            return Ok(false);
        }
    }
    Ok(true)
}

fn validate_karma_order(order: &[Order]) -> Result<(), ProteinError> {
    for value in order {
        let field = match value {
            Order::Asc(field) | Order::Desc(field) => field.as_str(),
            Order::Topo(_) => {
                return Err(karma_query_error(
                    "protein_karma_unsupported_order",
                    "topological ordering is not defined for Karma object rows",
                ));
            }
        };
        if !matches!(
            field,
            "object_kind" | "uid" | "slug" | "status" | "created_at" | "updated_at"
        ) {
            return Err(karma_query_error("protein_karma_unsupported_order", field));
        }
    }
    Ok(())
}

fn karma_sort_value(row: &Value, field: &str) -> String {
    row.get(field)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn karma_query_error(code: &str, message: impl std::fmt::Display) -> ProteinError {
    store::sqlx::Error::Protocol(format!("{code}:{message}"))
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
        let record_unit_uid = r.unit_uid.clone();
        let mut row = json!({
            "uid": r.uid,
            "slug": r.slug,
            "kind": r.kind,
            "head": r.head,
            "body": r.body,
            "quantity": r.quantity_f64(),
            "concept": r.concept_uid,
            "unit": r.unit_uid,
            "organ": r.organ_uid,
        });
        attach_includes(
            store,
            &mut row,
            &r.uid,
            r.quantity_f64(),
            record_unit_uid.as_deref(),
            &protein.include,
        )
        .await?;
        out.push(row);
    }
    Ok(out)
}

fn aggregate_records(rows: &[store::records::RecordRow], agg: &Aggregate) -> Vec<Value> {
    // Summing Record quantities is summing LEVELS, not changes, so there is no
    // gain/loss split here — a level has no direction. It is still exact, and
    // still unit-separated for the same reason the Fact side is.
    let mut buckets: std::collections::BTreeMap<(String, String), (nucleus::DecimalValue, i64)> =
        std::collections::BTreeMap::new();
    for r in rows {
        let key = match agg.by {
            GroupBy::Concept => r.concept_uid.clone().unwrap_or_else(|| UNCLASSIFIED.into()),
            GroupBy::Kind => r.kind.clone(),
            GroupBy::Total => TOTAL.into(),
            // fact-source group keys are meaningless on records
            GroupBy::CauseKind | GroupBy::Day | GroupBy::Month | GroupBy::Classification => {
                "(n/a)".into()
            }
        };
        let unit = r.unit_uid.clone().unwrap_or_default();
        let entry = buckets
            .entry((key, unit))
            .or_insert_with(|| (store::exact::zero(), 0));
        entry.0 = entry.0.aligned_add(r.quantity).unwrap_or(entry.0);
        entry.1 += 1;
    }
    buckets
        .into_iter()
        .map(|((group, unit), (total, count))| {
            let unit = if unit.is_empty() {
                Value::Null
            } else {
                Value::String(unit)
            };
            match agg.op {
                AggregateOp::Sum => json!({
                    "group": group,
                    "unit_uid": unit,
                    "value": total.to_string(),
                    "count": count,
                }),
                AggregateOp::Count => json!({
                    "group": group,
                    "unit_uid": unit,
                    "count": count,
                }),
            }
        })
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
                        "quantity" => a.quantity_f64().total_cmp(&b.quantity_f64()),
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
    record_unit_uid: Option<&str>,
    include: &Include,
) -> Result<(), ProteinError> {
    if include.availability {
        let availability =
            derive_record_availability(store, record_uid, quantity, record_unit_uid).await?;
        if let (Some(target), Some(source)) = (row.as_object_mut(), availability.as_object()) {
            target.extend(source.clone());
        }
    }
    if let Some(facts) = &include.facts {
        // provenance in one line: this is "the end of custom plumbing" (VII.1)
        let list = store::facts::for_record(&store.pool, record_uid, facts.limit).await?;
        row["facts"] = Value::Array(
            list.into_iter()
                .map(|f| {
                    json!({
                        "delta": f.delta.to_f64(),
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

async fn derive_record_availability(
    store: &Store,
    record_uid: &str,
    quantity: f64,
    record_unit_uid: Option<&str>,
) -> Result<Value, ProteinError> {
    // Lingua conversion is deliberately opt-in. Incomparable or unknown units
    // stay explicit instead of contaminating a numeric inventory bucket.
    let promises = store::misc::promises_for_record(&store.pool, record_uid).await?;
    let mut reserved = 0.0;
    let mut planned_delta = 0.0;
    let mut unknown_units = Vec::new();
    for promise in &promises {
        use nucleus::PromiseState::*;
        let converted_delta = match (promise.unit_uid.as_deref(), record_unit_uid) {
            (None, None) => Some(promise.delta),
            (Some(from), Some(to)) => {
                store::concepts::convert(&store.pool, from, to, promise.delta).await?
            }
            _ => None,
        };
        let Some(delta) = converted_delta else {
            unknown_units.push(json!({
                "promise": promise.uid,
                "unit": promise.unit_uid,
                "record_unit": record_unit_uid,
            }));
            continue;
        };
        let reserves = match promise.reserve_from.as_str() {
            "none" => false,
            "proposed" => matches!(promise.state, Proposed | Agreed | Active),
            "agreed" => matches!(promise.state, Agreed | Active),
            "active" => matches!(promise.state, Active),
            _ => false,
        };
        if reserves && delta < 0.0 {
            reserved += -delta;
        }
        if matches!(promise.state, Proposed | Agreed | Active) {
            planned_delta += delta;
        }
    }
    let available = quantity - reserved;
    Ok(json!({
        "actual": quantity,
        "reserved": reserved,
        "available": available,
        "planned": quantity + planned_delta,
        "surplus": available.max(0.0),
        "availability_unknown_units": unknown_units,
    }))
}

/// Resolve a projection instant: `"+7d"` relative to now, or absolute RFC3339.
fn resolve_at(value: &str) -> Option<String> {
    if let Some(rest) = value.strip_prefix('+') {
        let secs = nucleus::parse_duration(rest)?;
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
    if let Some(kind) = links
        .kind
        .as_deref()
        .map(str::trim)
        .filter(|kind| !kind.is_empty())
    {
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
    let references = store::concepts::resolve(&store.pool, "references").await?;
    let threads = store::links::records_to(&store.pool, &thread_of, record_uid).await?;
    let mut out = Vec::new();
    for thread in threads {
        if thread.kind != "thread" || !thread.quantity.is_positive() {
            continue;
        }
        let mut messages = Vec::new();
        for message in store::links::records_to(&store.pool, &message_in, &thread.uid).await? {
            if message.kind != "message" || !message.quantity.is_positive() {
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
            let record_references = match &references {
                Some(references) => {
                    store::links::records_from(&store.pool, &message.uid, references)
                        .await?
                        .into_iter()
                        .map(|record| {
                            json!({
                                "uid": record.uid,
                                "slug": record.slug,
                                "kind": record.kind,
                                "head": record.head,
                                "body": record.body,
                            })
                        })
                        .collect::<Vec<_>>()
                }
                None => Vec::new(),
            };
            let created_at = store::records::created_at(&store.pool, &message.uid).await?;
            let (created_by, sender) = creator_info(store, &message.uid).await?;
            messages.push(json!({
                "uid": message.uid,
                "head": message.head,
                "body": message.body,
                "quantity": message.quantity_f64(),
                "parent_message_uid": parent_message_uid,
                "created_at": created_at,
                "created_by": created_by,
                "sender": sender,
                "references": record_references,
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
            "quantity": thread.quantity_f64(),
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
                self.link_sources
                    .insert((kind.clone(), to.clone()), sources);
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
                Predicate::QuantityLt(n) => r.quantity_f64() < *n,
                Predicate::QuantityLte(n) => r.quantity_f64() <= *n,
                Predicate::QuantityGt(n) => r.quantity_f64() > *n,
                Predicate::QuantityGte(n) => r.quantity_f64() >= *n,
                Predicate::QuantityEq(n) => r.quantity_f64() == *n,
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
                // Transfer-source predicates are validated and evaluated by
                // `execute_transfers`; they remain vacuous on Record queries.
                Predicate::RevisionEq(_)
                | Predicate::RevisionLt(_)
                | Predicate::RevisionLte(_)
                | Predicate::RevisionGt(_)
                | Predicate::RevisionGte(_)
                | Predicate::StatusIn(_)
                | Predicate::ViewerRoleIn(_)
                | Predicate::InvitationStateIn(_)
                | Predicate::PersonEq(_)
                | Predicate::UnitEq(_)
                | Predicate::WindowEndBefore(_)
                | Predicate::WindowEndAfter(_)
                | Predicate::OccurrenceIn(_) => true,
                // fact-source predicates: vacuous on records
                Predicate::AtSince(_) | Predicate::CauseKindEq(_) | Predicate::RecordEq(_) => true,
                // ledger-window predicates: vacuous on records
                Predicate::AtBefore(_) | Predicate::ClassifiedIn(_) => true,
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
                    match (
                        self.organs.get(token).and_then(|o| o.as_deref()),
                        &r.organ_uid,
                    ) {
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
            if !p
                .record_uid
                .as_deref()
                .is_some_and(|uid| visible.contains(uid))
            {
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
            "direction": if p.delta < 0.0 {
                "give"
            } else if p.delta > 0.0 {
                "receive"
            } else {
                "invalid"
            },
            "state": p.state.as_str(),
            "open": p.state == nucleus::PromiseState::Open,
            "proposer": if p.state == nucleus::PromiseState::Open {
                p.party_uid.as_deref()
            } else {
                None
            },
            "reuse_policy": p.open_reuse_policy.as_str(),
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
    if let Some(secs) = nucleus::parse_duration(value) {
        return Some((chrono::Utc::now() - chrono::TimeDelta::seconds(secs)).to_rfc3339());
    }
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|_| value.to_string())
}

/// The bucket every change falls into when nothing classifies it. Named and
/// always emitted rather than dropped: a bucket that disappears when empty is
/// indistinguishable from one that was never computed, and "everything is
/// accounted for" is exactly the claim a person needs before trusting a total.
const UNCLASSIFIED: &str = "(unclassified)";

/// The single bucket `group_by: total` puts everything in.
const TOTAL: &str = "(total)";

/// One aggregation bucket, summed exactly.
///
/// Gains, losses, and net come back together from a single pass, because
/// direction is the sign of the delta and nothing else — a refund classified
/// `@cost` correctly *reduces* the cost total, and a separate direction field
/// would get that backwards. Reporting only the net would hide the difference
/// between a quiet month and a busy one that happened to balance.
struct DeltaBucket {
    net: nucleus::DecimalValue,
    gains: nucleus::DecimalValue,
    losses: nucleus::DecimalValue,
    count: i64,
}

impl Default for DeltaBucket {
    fn default() -> Self {
        Self {
            net: store::exact::zero(),
            gains: store::exact::zero(),
            losses: store::exact::zero(),
            count: 0,
        }
    }
}

impl DeltaBucket {
    fn add(&mut self, delta: nucleus::DecimalValue) -> Result<(), ProteinError> {
        let overflow = || {
            store::sqlx::Error::Protocol(
                "protein_fact_total_overflow:total exceeds the exact range".to_string(),
            )
        };
        self.net = self.net.aligned_add(delta).ok_or_else(overflow)?;
        if delta.is_positive() {
            self.gains = self.gains.aligned_add(delta).ok_or_else(overflow)?;
        } else if delta.is_negative() {
            self.losses = self.losses.aligned_add(delta).ok_or_else(overflow)?;
        }
        self.count += 1;
        Ok(())
    }

    fn row(&self, group: &str, unit: &str, op: AggregateOp) -> Value {
        let unit = if unit.is_empty() {
            Value::Null
        } else {
            Value::String(unit.to_string())
        };
        match op {
            // Sums cross the wire as canonical decimal TEXT. A JSON number is
            // an IEEE double, and letting one in here would undo the Ledger's
            // exactness at the very last step — the hardest place to notice.
            AggregateOp::Sum => json!({
                "group": group,
                "unit_uid": unit,
                "net": self.net.to_string(),
                "gains": self.gains.to_string(),
                "losses": self.losses.to_string(),
                "count": self.count,
            }),
            // A count has no net, gains, or losses. Emitting zeros for them
            // would be stating something false rather than omitting it.
            AggregateOp::Count => json!({
                "group": group,
                "unit_uid": unit,
                "count": self.count,
            }),
        }
    }
}

/// A concept and everything below it in the Lingua DAG. An unknown name yields
/// an empty family, which matches nothing — the honest answer for a filter on a
/// vocabulary word that does not exist.
async fn concept_descendants(store: &Store, name: &str) -> Result<HashSet<String>, ProteinError> {
    Ok(match store::concepts::resolve(&store.pool, name).await? {
        Some(uid) => store::concepts::descendants_including(&store.pool, &uid)
            .await?
            .into_iter()
            .collect(),
        None => HashSet::new(),
    })
}

async fn execute_facts(
    store: &Store,
    protein: &Protein,
    visible: Option<&HashSet<String>>,
) -> Result<Vec<Value>, ProteinError> {
    // Record metadata for concept filters and aggregation keys. `concept_in`
    // reads the union of a Record's identity concept and the ones it merely
    // counts as, because a toothbrush that counts as `@health` must answer a
    // `@health` query about its changes just as it does about itself.
    let counts_as = store::ledger::all_record_concepts(&store.pool).await?;
    let mut record_concepts: HashMap<String, Vec<String>> = HashMap::new();
    let mut record_unit: HashMap<String, Option<String>> = HashMap::new();
    for r in store::records::list_all(&store.pool).await? {
        record_unit.insert(r.uid.clone(), r.unit_uid.clone());
        // Identity concept first, so `group_by: concept` keys on what the thing
        // IS rather than on whichever tag happens to sort first.
        let mut concepts: Vec<String> = r.concept_uid.clone().into_iter().collect();
        if let Some(extra) = counts_as.get(&r.uid) {
            for concept in extra {
                if !concepts.contains(concept) {
                    concepts.push(concept.clone());
                }
            }
        }
        record_concepts.insert(r.uid.clone(), concepts);
    }

    // walk the flat predicate list (fact predicates don't nest in v1)
    let mut since: Option<String> = None;
    let mut before: Option<String> = None;
    let mut cause_kind: Option<&str> = None;
    let mut record: Option<String> = None;
    let mut concept_family: Option<HashSet<String>> = None;
    let mut classification_family: Option<HashSet<String>> = None;
    for p in &protein.filter {
        match p {
            Predicate::AtSince(v) => since = resolve_since(v),
            Predicate::AtBefore(v) => before = resolve_since(v),
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
                concept_family = Some(concept_descendants(store, name).await?);
            }
            Predicate::ClassifiedIn(name) => {
                classification_family = Some(concept_descendants(store, name).await?);
            }
            _ => {}
        }
    }
    let before_instant = match &before {
        Some(value) => Some(chrono::DateTime::parse_from_rfc3339(value).map_err(|_| {
            store::sqlx::Error::Protocol(format!(
                "protein_fact_instant_invalid:`{value}` is not an instant"
            ))
        })?),
        None => None,
    };

    // Loaded once when the query cares about it, never per Fact.
    let needs_classification = classification_family.is_some()
        || matches!(&protein.aggregate, Some(a) if a.by == GroupBy::Classification);
    let fact_classification = if needs_classification {
        store::ledger::all_fact_concepts(&store.pool).await?
    } else {
        Default::default()
    };

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
        // Half-open: a change exactly at `at_before` belongs to the next
        // window, so adjacent periods tile without double-counting.
        if before_instant.is_some_and(|end| f.at >= end) {
            continue;
        }
        if let Some(family) = &concept_family {
            let matches = record_concepts
                .get(&f.record_uid)
                .is_some_and(|concepts| concepts.iter().any(|c| family.contains(c)));
            if !matches {
                continue;
            }
        }
        if let Some(family) = &classification_family {
            // An unclassified change never matches a classification filter:
            // "what did I spend on food" must not silently include the ones
            // nobody said were food.
            let matches = fact_classification
                .get(&f.uid)
                .is_some_and(|c| family.contains(c));
            if !matches {
                continue;
            }
        }
        facts.push(f);
    }

    if let Some(agg) = &protein.aggregate {
        // Buckets are keyed by (group, unit) rather than group alone. Two
        // Records measured in different units share a concept all the time —
        // flour in kilograms and milk in litres are both `@stock` — and adding
        // them produces a number that means nothing. Separating is the true
        // answer; refusing would make the query useless, and summing would make
        // it wrong.
        let mut buckets: std::collections::BTreeMap<(String, String), DeltaBucket> =
            Default::default();
        for f in &facts {
            let key = match agg.by {
                GroupBy::Total => TOTAL.to_string(),
                GroupBy::CauseKind | GroupBy::Kind => f.cause.kind.as_str().to_string(),
                GroupBy::Day => f.at.format("%Y-%m-%d").to_string(),
                GroupBy::Month => f.at.format("%Y-%m").to_string(),
                // What the Record IS.
                GroupBy::Concept => record_concepts
                    .get(&f.record_uid)
                    .and_then(|concepts| concepts.first().cloned())
                    .unwrap_or_else(|| UNCLASSIFIED.into()),
                // What the CHANGE was — a different question, and the one an
                // expense breakdown or a usage report actually asks.
                GroupBy::Classification => fact_classification
                    .get(&f.uid)
                    .cloned()
                    .unwrap_or_else(|| UNCLASSIFIED.into()),
            };
            let unit = record_unit
                .get(&f.record_uid)
                .cloned()
                .flatten()
                .unwrap_or_default();
            buckets.entry((key, unit)).or_default().add(f.delta)?;
        }
        return Ok(buckets
            .into_iter()
            .map(|((group, unit), bucket)| bucket.row(&group, &unit, agg.op))
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
                "delta": f.delta.to_f64(),
                "at": f.at.to_rfc3339(),
                "cause_kind": f.cause.kind.as_str(),
                "cause": f.cause.uid,
                "actor": f.actor_uid,
                "payload": f.payload,
            })
        })
        .collect())
}

// ------------------------------------------------------- entries & recurrence

/// Authored changes, with the classification each was given and the revision a
/// correction has to quote.
async fn execute_entries(
    store: &Store,
    protein: &Protein,
    visible: Option<&HashSet<String>>,
) -> Result<Vec<Value>, ProteinError> {
    let mut record: Option<String> = None;
    let mut classification_family: Option<HashSet<String>> = None;
    for p in &protein.filter {
        match p {
            Predicate::RecordEq(token) => {
                record = store::records::resolve(&store.pool, token)
                    .await?
                    .map(|r| r.uid);
                if record.is_none() {
                    return Ok(vec![]);
                }
            }
            Predicate::ClassifiedIn(name) => {
                classification_family = Some(concept_descendants(store, name).await?);
            }
            _ => {}
        }
    }

    let fact_classification = store::ledger::all_fact_concepts(&store.pool).await?;
    // Scan wide, then cut. Applying the caller's limit in SQL would take the
    // newest N rows and filter *those*, so a rare category would look almost
    // empty while plenty of matching changes existed just past the cut.
    const ENTRY_SCAN_CAP: i64 = 100_000;
    let limit = protein.limit.unwrap_or(500);
    let mut out = Vec::new();
    for entry in store::entries::list_all(&store.pool, ENTRY_SCAN_CAP).await? {
        if out.len() >= limit {
            break;
        }
        if visible.is_some_and(|v| !v.contains(&entry.record_uid)) {
            continue;
        }
        if record.as_ref().is_some_and(|r| &entry.record_uid != r) {
            continue;
        }
        let concept = entry
            .fact_uid
            .as_deref()
            .and_then(|uid| fact_classification.get(uid))
            .cloned();
        if let Some(family) = &classification_family
            && !concept.as_deref().is_some_and(|c| family.contains(c))
        {
            continue;
        }
        out.push(json!({
            "kind": "entry",
            "uid": entry.uid,
            "record": entry.record_uid,
            // Exact text, never a float: this is the number a person typed.
            "amount": entry.amount.to_string(),
            "note": entry.note,
            "occurred_at": entry.occurred_at,
            "state": entry.state,
            // Both are required to revise or void; a stale one is refused.
            "revision": entry.revision,
            "fact": entry.fact_uid,
            "concept": concept,
            "void": entry.is_void(),
        }));
    }
    Ok(out)
}

/// Standing rules and the dates they imply.
async fn execute_recurrence(
    store: &Store,
    protein: &Protein,
    visible: Option<&HashSet<String>>,
) -> Result<Vec<Value>, ProteinError> {
    use chrono::{DateTime, Duration, Utc};

    let now = Utc::now();
    let mut since: Option<String> = None;
    let mut before: Option<String> = None;
    let mut record: Option<String> = None;
    for p in &protein.filter {
        match p {
            Predicate::AtSince(v) => since = resolve_since(v),
            Predicate::AtBefore(v) => before = resolve_since(v),
            Predicate::RecordEq(token) => {
                record = store::records::resolve(&store.pool, token)
                    .await?
                    .map(|r| r.uid);
                if record.is_none() {
                    return Ok(vec![]);
                }
            }
            _ => {}
        }
    }
    let parse = |value: &str, fallback: DateTime<Utc>| {
        DateTime::parse_from_rfc3339(value)
            .map(|v| v.with_timezone(&Utc))
            .unwrap_or(fallback)
    };
    // A default window that shows what was recently missed and what is coming.
    let from = since
        .as_deref()
        .map(|v| parse(v, now - Duration::days(60)))
        .unwrap_or(now - Duration::days(60));
    let to = before
        .as_deref()
        .map(|v| parse(v, now + Duration::days(90)))
        .unwrap_or(now + Duration::days(90));

    let mut out = Vec::new();
    for rule in store::recurrence::all(&store.pool).await? {
        if visible.is_some_and(|v| !v.contains(&rule.record_uid)) {
            continue;
        }
        if record.as_ref().is_some_and(|r| &rule.record_uid != r) {
            continue;
        }
        let derived = store::recurrence::occurrences(&store.pool, &rule, from, to, now).await?;
        out.push(json!({
            "kind": "recurrence",
            "uid": rule.uid,
            "record": rule.record_uid,
            // What the rule does, in full. `amount` and `concept` remain
            // beside it as the *summary* a list view reads, because scanning
            // rules should not mean parsing a consequence tree — but they are
            // derived from `consequences`, never a second place to edit.
            "consequences": rule.consequences,
            "amount": rule.consequences.declared_delta().map(|a| a.to_string()),
            // A rule stepping in milliseconds produces more dates than any
            // window can hold. Saying so is the difference between a list a
            // person can trust and a page that merely looks complete.
            "truncated": derived.truncated,
            "concept": rule.consequences.capture_concept(),
            // The *if* half, as the three fields the form asked for. Null when
            // the rule is unconditional, which is most of them.
            "condition": rule.condition.as_ref().map(|c| c.source.clone()),
            "gate": rule.condition.as_ref().map(|c| c.gate.as_text()),
            "carry": rule.condition.as_ref().map(|c| c.carry.as_text()),
            "note": rule.note,
            "cadence": rule.cadence,
            "anchor_at": rule.anchor_at,
            "state": rule.state,
            "paused": rule.is_paused(),
            // Quoted back on every edit, so a stale surface loses rather than
            // silently overwriting someone else's change.
            "revision": rule.revision,
        }));
        for occurrence in derived {
            out.push(json!({
                "kind": "occurrence",
                "recurrence": rule.uid,
                "record": rule.record_uid,
                "due_at": occurrence.due_at.to_rfc3339(),
                "state": occurrence.state.as_str(),
                // Absent for a rule that only changes concepts: nothing about a
                // quantity is expected to move, and a zero there would read as
                // an expectation of no change rather than of no amount.
                "amount": occurrence.amount.map(|a| a.to_string()),
                "entry": occurrence.entry_uid,
                "concept": rule.consequences.capture_concept(),
                "note": rule.note,
            }));
        }
    }
    Ok(out)
}

// ------------------------------------------------------------------ timeline

/// The bucket granularity a timeline reports in.
fn timeline_bucket(at: chrono::DateTime<chrono::Utc>, by: GroupBy) -> String {
    match by {
        GroupBy::Day => at.format("%Y-%m-%d").to_string(),
        _ => at.format("%Y-%m").to_string(),
    }
}

/// One classified axis through time: settled past, position now, declared future.
///
/// Requires a `classified_in` predicate — a timeline is *of* something, and a
/// timeline of everything is just the Ledger. `at_since`/`at_before` bound it;
/// both default to a year around now.
///
/// Every number leaves here as exact decimal text, including the running
/// cumulative, because the whole point of the source is that the client never
/// does the arithmetic.
///
/// **On the future's exactness:** recurring amounts are exact, since a rule
/// stores the same mantissa/scale pair the Ledger does. Promise deltas are
/// `REAL` in schema 0001 and are converted on the way out; a promise-derived
/// point is therefore only as exact as that column ever was. Points say which
/// they came from so a reader is never guessing.
async fn execute_timeline(
    store: &Store,
    protein: &Protein,
    visible: Option<&HashSet<String>>,
) -> Result<Vec<Value>, ProteinError> {
    use chrono::{DateTime, Duration, Utc};

    let now = Utc::now();
    let mut concept_token: Option<&str> = None;
    let mut since: Option<String> = None;
    let mut before: Option<String> = None;
    for p in &protein.filter {
        match p {
            Predicate::ClassifiedIn(name) => concept_token = Some(name),
            Predicate::AtSince(v) => since = resolve_since(v),
            Predicate::AtBefore(v) => before = resolve_since(v),
            _ => {}
        }
    }
    let Some(concept_token) = concept_token else {
        return Err(store::sqlx::Error::Protocol(
            "protein_timeline_concept_required:a timeline needs `classified_in`".to_string(),
        )
        .into());
    };
    let family = concept_descendants(store, concept_token).await?;
    if family.is_empty() {
        return Ok(vec![]); // unknown concept: nothing can match
    }

    let parse = |value: &str| -> Result<DateTime<Utc>, ProteinError> {
        DateTime::parse_from_rfc3339(value)
            .map(|v| v.with_timezone(&Utc))
            .map_err(|_| {
                store::sqlx::Error::Protocol(format!(
                    "protein_timeline_instant_invalid:`{value}` is not an instant"
                ))
                .into()
            })
    };
    let from = match since.as_deref() {
        Some(value) => parse(value)?,
        None => now - Duration::days(180),
    };
    let to = match before.as_deref() {
        Some(value) => parse(value)?,
        None => now + Duration::days(180),
    };
    let by = protein
        .aggregate
        .as_ref()
        .map(|a| a.by)
        .unwrap_or(GroupBy::Month);

    // ---------------------------------------------------------------- actuals
    let fact_classification = store::ledger::all_fact_concepts(&store.pool).await?;
    let mut record_unit: HashMap<String, Option<String>> = HashMap::new();
    for r in store::records::list_all(&store.pool).await? {
        record_unit.insert(r.uid.clone(), r.unit_uid.clone());
    }

    const TIMELINE_SCAN_CAP: i64 = 100_000;
    // `opening` is everything this concept did before the window. Without it a
    // cumulative line would restart at zero at the window's edge and show a
    // position the person has never been in.
    // Both are per unit, for the same reason the buckets are: a concept can
    // span kilograms and hours at once, and one scalar running total
    // across both would add them. That is the number this whole module refuses
    // to produce, and it would be the most prominent one on the screen.
    let mut opening: std::collections::BTreeMap<String, nucleus::DecimalValue> = Default::default();
    let mut settled_through: std::collections::BTreeMap<String, nucleus::DecimalValue> =
        Default::default();
    let mut actual: std::collections::BTreeMap<(String, String), DeltaBucket> = Default::default();
    for f in store::facts::list_since(&store.pool, None, TIMELINE_SCAN_CAP).await? {
        if visible.is_some_and(|v| !v.contains(&f.record_uid)) {
            continue;
        }
        // An unclassified change is not this concept's business.
        if !fact_classification
            .get(&f.uid)
            .is_some_and(|c| family.contains(c))
        {
            continue;
        }
        let unit = record_unit
            .get(&f.record_uid)
            .cloned()
            .flatten()
            .unwrap_or_default();
        let accumulate = |totals: &mut std::collections::BTreeMap<
            String,
            nucleus::DecimalValue,
        >|
         -> Result<(), ProteinError> {
            let slot = totals.entry(unit.clone()).or_insert_with(store::exact::zero);
            *slot = slot.aligned_add(f.delta).ok_or_else(timeline_overflow)?;
            Ok(())
        };
        if f.at < from {
            accumulate(&mut opening)?;
            accumulate(&mut settled_through)?;
            continue;
        }
        if f.at >= to {
            continue;
        }
        if f.at <= now {
            accumulate(&mut settled_through)?;
        }
        actual
            .entry((timeline_bucket(f.at, by), unit))
            .or_default()
            .add(f.delta)?;
    }

    // ---------------------------------------------------------------- declared
    // Everything ahead is something somebody already stated: a rule's date or a
    // promise. Nothing is extrapolated from the past.
    let mut expected: std::collections::BTreeMap<(String, String), DeltaBucket> = Default::default();
    let mut contributors: Vec<Value> = Vec::new();
    let forward_from = if now > from { now } else { from };
    let mut projection_truncated = false;

    for rule in store::recurrence::all(&store.pool).await? {
        if !rule
            .consequences
            .capture_concept()
            .is_some_and(|c| family.contains(c))
        {
            continue;
        }
        if visible.is_some_and(|v| !v.contains(&rule.record_uid)) {
            continue;
        }
        let unit = record_unit
            .get(&rule.record_uid)
            .cloned()
            .flatten()
            .unwrap_or_default();
        let derived =
            store::recurrence::occurrences(&store.pool, &rule, forward_from, to, now).await?;
        // A rule too fast to enumerate makes the declared half of the line a
        // lower bound rather than the whole of what is coming.
        projection_truncated |= derived.truncated;
        for occurrence in derived {
            // An applied date is already a Fact and was counted above; counting
            // it here too would double every rule-driven month. A skipped date
            // was declined and is not expected.
            if !matches!(
                occurrence.state,
                store::recurrence::OccurrenceState::Planned
                    | store::recurrence::OccurrenceState::Due
            ) {
                continue;
            }
            // A rule that only changes concepts expects no quantity to move, so
            // it contributes no point. Folding a zero here would draw a
            // deliberate "no change" onto the line, which is a different claim.
            let Some(amount) = occurrence.amount else {
                continue;
            };
            let bucket = timeline_bucket(occurrence.due_at, by);
            expected
                .entry((bucket.clone(), unit.clone()))
                .or_default()
                .add(amount)?;
            contributors.push(json!({
                "kind": "timeline_source",
                "bucket": bucket,
                "origin": "recurrence",
                "uid": rule.uid,
                "record": rule.record_uid,
                "amount": amount.to_string(),
                "at": occurrence.due_at.to_rfc3339(),
                "state": occurrence.state.as_str(),
                "note": rule.note,
                "unit": unit,
            }));
        }
    }

    for promise in store::misc::list_promises(&store.pool).await? {
        if !promise
            .concept_uid
            .as_deref()
            .is_some_and(|c| family.contains(c))
        {
            continue;
        }
        if let Some(visible) = visible
            && !promise
                .record_uid
                .as_deref()
                .is_some_and(|uid| visible.contains(uid))
        {
            continue;
        }
        // Only what is still outstanding is ahead of you. A settled promise
        // already produced its Fact and is in the actuals.
        if !matches!(
            promise.state,
            nucleus::PromiseState::Open | nucleus::PromiseState::Proposed
        ) {
            continue;
        }
        let Some(window_end) = promise.window_end.as_deref() else {
            continue; // a promise with no date cannot be placed on a timeline
        };
        let Ok(due) = DateTime::parse_from_rfc3339(window_end).map(|v| v.with_timezone(&Utc))
        else {
            continue;
        };
        if due < forward_from || due >= to {
            continue;
        }
        let unit = promise
            .record_uid
            .as_deref()
            .and_then(|uid| record_unit.get(uid).cloned().flatten())
            .unwrap_or_default();
        let amount = store::exact::from_f64(promise.delta);
        let bucket = timeline_bucket(due, by);
        expected
            .entry((bucket.clone(), unit.clone()))
            .or_default()
            .add(amount)?;
        contributors.push(json!({
            "kind": "timeline_source",
            "bucket": bucket,
            "origin": "promise",
            "uid": promise.uid,
            "record": promise.record_uid,
            "amount": amount.to_string(),
            "at": due.to_rfc3339(),
            "state": promise.state.as_str(),
            "note": promise.condition,
            "unit": unit,
        }));
    }

    // ------------------------------------------------------------- assemble
    // One ordered line per unit. Two units under one concept — flour in
    // kilograms and milk in litres are both `@stock` — are two lines, never one
    // sum, because adding them produces a number that means nothing.
    let mut units: Vec<String> = actual
        .keys()
        .map(|(_, unit)| unit.clone())
        .chain(expected.keys().map(|(_, unit)| unit.clone()))
        .collect();
    units.sort();
    units.dedup();

    let per_unit = |totals: &std::collections::BTreeMap<String, nucleus::DecimalValue>| {
        let mut map = serde_json::Map::new();
        for unit in &units {
            let value = totals.get(unit).copied().unwrap_or_else(store::exact::zero);
            map.insert(unit.clone(), Value::String(value.to_string()));
        }
        Value::Object(map)
    };
    // A single scalar is offered ONLY when there is one unit to be scalar about.
    // With two, `current` is null and a reader must use `current_by_unit` — the
    // alternative is a headline number that added kilograms to hours.
    let single_unit = if units.len() <= 1 {
        units.first().cloned().or_else(|| Some(String::new()))
    } else {
        None
    };
    let scalar = |totals: &std::collections::BTreeMap<String, nucleus::DecimalValue>| match &single_unit
    {
        Some(unit) => Value::String(
            totals
                .get(unit)
                .copied()
                .unwrap_or_else(store::exact::zero)
                .to_string(),
        ),
        None => Value::Null,
    };

    let mut rows = vec![json!({
        "kind": "timeline_context",
        "concept": concept_token,
        "from": from.to_rfc3339(),
        "to": to.to_rfc3339(),
        "now": now.to_rfc3339(),
        "bucket": match by { GroupBy::Day => "day", _ => "month" },
        // Where this concept stands right now, counting everything settled:
        // the "current state" a timeline is read to find.
        "current": scalar(&settled_through),
        "opening": scalar(&opening),
        "current_by_unit": per_unit(&settled_through),
        "opening_by_unit": per_unit(&opening),
        "units": units.clone(),
        // The settled half is always complete — it is read from Facts. Only the
        // declared half can run out of room, so this qualifies the future of
        // the line and never its past.
        "projection_truncated": projection_truncated,
    })];

    for unit in &units {
        let mut buckets: Vec<String> = actual
            .keys()
            .filter(|(_, u)| u == unit)
            .map(|(b, _)| b.clone())
            .chain(
                expected
                    .keys()
                    .filter(|(_, u)| u == unit)
                    .map(|(b, _)| b.clone()),
            )
            .collect();
        buckets.sort();
        buckets.dedup();

        // The line starts where the concept already stood *in this unit*, so
        // the first point is a position rather than a fresh zero, and a second
        // unit's history never seeds it.
        let mut running = opening
            .get(unit)
            .copied()
            .unwrap_or_else(store::exact::zero);
        let present = timeline_bucket(now, by);
        for bucket in buckets {
            let settled = actual.get(&(bucket.clone(), unit.clone()));
            let declared = expected.get(&(bucket.clone(), unit.clone()));
            // A bucket can hold both: the month you are standing in has days
            // that already happened and dates still to come.
            if let Some(settled) = settled {
                running = running
                    .aligned_add(settled.net)
                    .ok_or_else(timeline_overflow)?;
            }
            if let Some(declared) = declared {
                running = running
                    .aligned_add(declared.net)
                    .ok_or_else(timeline_overflow)?;
            }
            let phase = if bucket.as_str() < present.as_str() {
                "actual"
            } else if bucket == present {
                "present"
            } else {
                "expected"
            };
            rows.push(json!({
                "kind": "timeline_point",
                "bucket": bucket,
                "unit": unit,
                "phase": phase,
                "actual_net": settled.map(|b| b.net.to_string()),
                "actual_gains": settled.map(|b| b.gains.to_string()),
                "actual_losses": settled.map(|b| b.losses.to_string()),
                "actual_count": settled.map(|b| b.count).unwrap_or(0),
                "expected_net": declared.map(|b| b.net.to_string()),
                "expected_gains": declared.map(|b| b.gains.to_string()),
                "expected_losses": declared.map(|b| b.losses.to_string()),
                "expected_count": declared.map(|b| b.count).unwrap_or(0),
                "cumulative": running.to_string(),
            }));
        }
    }

    contributors.sort_by(|a, b| {
        a["at"]
            .as_str()
            .unwrap_or_default()
            .cmp(b["at"].as_str().unwrap_or_default())
    });
    rows.extend(contributors);
    Ok(rows)
}

fn timeline_overflow() -> ProteinError {
    store::sqlx::Error::Protocol("protein_timeline_overflow:a total grew past its bounds".into())
        .into()
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
            nucleus::transfer::policy_satisfied(t, agreement_pct.map(|p| p as u8), party_levels)
        })
        .unwrap_or(false);
    let agreement_promises = promise_states
        .iter()
        .filter(|state| !matches!(state, Open | Withdrawn))
        .collect::<Vec<_>>();
    if policy
        && !agreement_promises.is_empty()
        && agreement_promises
            .iter()
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

fn derive_transfer_primary_status(
    revision: u64,
    active: bool,
    promise_states: &[nucleus::PromiseState],
    occurrences: &[store::transfers::TransferOccurrenceRow],
    progress: &HashMap<String, store::transfers::OccurrenceSettlementProgress>,
    source_group: Option<&store::transfers::TransferSourceGroupState>,
    agreement_ready: bool,
    expired: bool,
    cancelled: bool,
    awaiting_me: bool,
    awaiting_others: bool,
) -> &'static str {
    use nucleus::PromiseState::*;
    if revision == 0 {
        return "legacy";
    }
    if occurrences
        .iter()
        .any(|occurrence| occurrence.system_disputed)
    {
        return "system_disputed";
    }
    if occurrences.iter().any(|occurrence| occurrence.disputed) {
        return "disputed";
    }
    if promise_states.iter().any(|state| matches!(state, Broken)) {
        return "broken";
    }
    if expired {
        return "expired";
    }
    if cancelled {
        return "cancelled";
    }
    let all_occurrences_settled = !occurrences.is_empty()
        && occurrences.iter().all(|occurrence| {
            progress
                .get(&occurrence.uid)
                .is_some_and(|progress| progress.settled)
        });
    let concrete_states = promise_states
        .iter()
        .filter(|state| !matches!(state, Open | Withdrawn))
        .collect::<Vec<_>>();
    let completed = all_occurrences_settled
        || (!concrete_states.is_empty()
            && concrete_states.iter().all(|state| matches!(state, Kept)));
    if !completed
        && (progress
            .values()
            .any(|progress| progress.settled_quantity > 0.0)
            || promise_states.iter().any(|state| matches!(state, Kept)))
    {
        return "partially_settled";
    }
    if source_group.is_some_and(|group| group.satiated) {
        return "satiated";
    }
    if completed {
        return "completed";
    }
    if awaiting_me {
        return "awaiting_me";
    }
    if !occurrences.is_empty() || promise_states.iter().any(|state| matches!(state, Active)) {
        return "active";
    }
    if awaiting_others {
        return "awaiting_others";
    }
    if agreement_ready && !completed {
        return "agreed";
    }
    if !completed
        && promise_states
            .iter()
            .any(|state| matches!(state, Proposed | Agreed))
    {
        return "proposed";
    }
    if !completed && promise_states.iter().any(|state| matches!(state, Open)) {
        return "open";
    }
    if !active && promise_states.is_empty() {
        return "inactive";
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

const PHASE_1_DRAFT_ACTIONS_BLOCKER: &str = "phase_1_draft_actions_not_available";
#[derive(Default)]
struct AgreementReadinessProjection {
    ready: bool,
    blockers: Vec<Value>,
    promises: HashMap<String, PromiseAgreementReadiness>,
    people: HashMap<String, bool>,
}

#[derive(Default)]
struct PromiseAgreementReadiness {
    ready: bool,
    eligible: bool,
    blockers: Vec<Value>,
}

fn with_promise_context(promise_uid: &str, blocker: &Value) -> Value {
    let mut projected = blocker.clone();
    if let Some(object) = projected.as_object_mut() {
        object
            .entry("promise")
            .or_insert_with(|| Value::String(promise_uid.into()));
    } else {
        projected = json!({ "code": blocker, "promise": promise_uid });
    }
    projected
}

fn agreement_path_matches(
    left: &store::transfers::AgreementPromiseReadinessRow,
    right: &store::transfers::AgreementPromiseReadinessRow,
) -> bool {
    if left.uid == right.uid
        || left.person_uid == right.person_uid
        || left.unit_uid != right.unit_uid
    {
        return false;
    }
    let same_subject = match (&left.concept_uid, &right.concept_uid) {
        (Some(left), Some(right)) => left == right,
        _ => left.record_uid.is_some() && left.record_uid == right.record_uid,
    };
    same_subject
        && (left.delta + right.delta).abs() < 1e-9
        && left.window_start == right.window_start
        && left.window_end == right.window_end
        && left.location == right.location
}

fn promise_state_reaches(actual: &str, required: &str) -> bool {
    let rank = |state: &str| match state {
        "open" => Some(0),
        "proposed" => Some(1),
        "agreed" => Some(2),
        "active" => Some(3),
        "kept" => Some(4),
        _ => None,
    };
    match required {
        "broken" | "withdrawn" => actual == required,
        _ => rank(actual)
            .zip(rank(required))
            .is_some_and(|(actual, required)| actual >= required),
    }
}

struct OccurrenceApplicationResolver {
    incoming: f64,
}

impl nucleus::expr::Resolver for OccurrenceApplicationResolver {
    fn call(
        &mut self,
        name: &str,
        args: &[nucleus::expr::Value],
    ) -> Result<nucleus::expr::Value, nucleus::NucleusError> {
        if name == "incoming" && args.is_empty() {
            return Ok(nucleus::expr::Value::Num(self.incoming));
        }
        Err(nucleus::NucleusError::Eval(format!(
            "application formula supports only incoming(), not {name}()"
        )))
    }
}

fn evaluate_occurrence_application_formula(
    formula: &str,
    incoming: f64,
) -> Result<f64, nucleus::NucleusError> {
    use nucleus::expr::{BinOp, Expr, UnOp};

    if formula.is_empty() || formula.chars().count() > 2_000 {
        return Err(nucleus::NucleusError::Eval(
            "application formula must contain 1 to 2000 characters".into(),
        ));
    }

    fn validate(expr: &Expr) -> Result<(), nucleus::NucleusError> {
        match expr {
            // The literal is kept as its source text so an exact evaluator can
            // read it without going through a double. A lexed number is always
            // finite, so parsing back is the whole check.
            Expr::Num(text) if text.parse::<f64>().is_ok_and(f64::is_finite) => Ok(()),
            Expr::Fn(name, args) if name == "incoming" && args.is_empty() => Ok(()),
            Expr::Unary(UnOp::Neg, value) => validate(value),
            Expr::Bin(
                BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem,
                left,
                right,
            ) => {
                validate(left)?;
                validate(right)
            }
            _ => Err(nucleus::NucleusError::Eval(
                "application formula contains unsupported syntax".into(),
            )),
        }
    }

    let expr = Expr::parse(formula)?;
    validate(&expr)?;
    let value = expr.eval(&mut OccurrenceApplicationResolver { incoming })?;
    if !value.is_finite() {
        return Err(nucleus::NucleusError::Eval(
            "application formula produced a non-finite number".into(),
        ));
    }
    Ok(value)
}

struct EffectiveSettlementApplication {
    formula: String,
    formula_hash: String,
    version: u64,
    source: &'static str,
    error: Option<String>,
}

async fn effective_settlement_application(
    store: &Store,
    occurrence: &store::transfers::TransferOccurrenceRow,
    source_promise: &store::misc::PromiseRow,
    owner_person_uid: &str,
    cell_formula: &str,
) -> Result<EffectiveSettlementApplication, ProteinError> {
    let policy = if source_promise.delta > 0.0 {
        store::transfers::occurrence_application_policy(
            &store.pool,
            &occurrence.uid,
            owner_person_uid,
        )
        .await?
    } else {
        None
    };
    let (mut formula, version, mut source) = if source_promise.delta < 0.0 {
        ("-incoming()".to_string(), 0, "code_default")
    } else if let Some(policy) = policy.as_ref() {
        (
            policy.formula.clone(),
            policy.version,
            "occurrence_override",
        )
    } else {
        (cell_formula.to_string(), 0, "cell_default")
    };
    let mut evaluated = evaluate_occurrence_application_formula(&formula, occurrence.quantity);
    if policy.is_none() && source_promise.delta > 0.0 && evaluated.is_err() {
        formula = "incoming()".into();
        source = "code_default";
        evaluated = evaluate_occurrence_application_formula(&formula, occurrence.quantity);
    }
    Ok(EffectiveSettlementApplication {
        formula_hash: nucleus::transfer::occurrence_application_formula_hash(&formula),
        formula,
        version,
        source,
        error: evaluated.err().map(|error| error.to_string()),
    })
}

async fn dependency_satisfied(
    store: &Store,
    dependency: &nucleus::transfer::TransferRevisionDependency,
) -> Result<(bool, Value), ProteinError> {
    match dependency.upstream_kind {
        nucleus::transfer::TransferDependencyUpstreamKind::Promise => {
            let actual = store::misc::get_promise(&store.pool, &dependency.upstream_uid)
                .await?
                .map(|promise| promise.state.as_str().to_string());
            let satisfied = actual
                .as_deref()
                .is_some_and(|state| promise_state_reaches(state, &dependency.required_state));
            Ok((
                satisfied,
                json!({
                    "uid": dependency.uid,
                    "scope": dependency.scope.as_str(),
                    "promise": dependency.promise_uid,
                    "upstream_kind": dependency.upstream_kind.as_str(),
                    "upstream": dependency.upstream_uid,
                    "required_state": dependency.required_state,
                    "actual_state": actual,
                    "satisfied": satisfied,
                }),
            ))
        }
        nucleus::transfer::TransferDependencyUpstreamKind::Transfer => {
            let upstream = store::transfers::get(&store.pool, &dependency.upstream_uid).await?;
            let upstream_promises = if upstream.is_some() {
                store::transfers::promises_of(&store.pool, &dependency.upstream_uid).await?
            } else {
                Vec::new()
            };
            let relevant = upstream_promises
                .iter()
                .filter(|promise| {
                    matches!(dependency.required_state.as_str(), "open" | "withdrawn")
                        || !matches!(
                            promise.state,
                            nucleus::PromiseState::Open | nucleus::PromiseState::Withdrawn
                        )
                })
                .collect::<Vec<_>>();
            let satisfied = !relevant.is_empty()
                && relevant.iter().all(|promise| {
                    promise_state_reaches(promise.state.as_str(), &dependency.required_state)
                });
            let actual = relevant
                .iter()
                .map(|promise| promise.state.as_str())
                .collect::<Vec<_>>();
            Ok((
                satisfied,
                json!({
                    "uid": dependency.uid,
                    "scope": dependency.scope.as_str(),
                    "promise": dependency.promise_uid,
                    "upstream_kind": dependency.upstream_kind.as_str(),
                    "upstream": dependency.upstream_uid,
                    "required_state": dependency.required_state,
                    "actual_states": actual,
                    "satisfied": satisfied,
                }),
            ))
        }
    }
}

async fn derive_agreement_readiness(
    store: &Store,
    input: &store::transfers::TransferAgreementReadinessInput,
) -> Result<(AgreementReadinessProjection, Vec<Value>), ProteinError> {
    let levels_by_person = input
        .parties
        .iter()
        .map(|party| (party.person_uid.as_str(), party))
        .collect::<HashMap<_, _>>();
    let levels_by_party = input
        .parties
        .iter()
        .map(|party| (party.party_uid.as_str(), party.level))
        .collect::<HashMap<_, _>>();
    let coalition_members = input
        .coalition
        .as_ref()
        .map(|coalition| coalition.party_uids.iter().collect::<HashSet<_>>());
    let mut dependency_status = Vec::with_capacity(input.dependencies.len());
    let mut satisfied_dependencies = HashMap::new();
    for dependency in &input.dependencies {
        let (satisfied, projection) = dependency_satisfied(store, dependency).await?;
        satisfied_dependencies.insert(dependency.uid.as_str(), satisfied);
        dependency_status.push(projection);
    }

    let mut projection = AgreementReadinessProjection::default();
    for promise in &input.promises {
        let mut readiness = PromiseAgreementReadiness {
            eligible: !matches!(promise.state.as_str(), "open" | "withdrawn"),
            ..Default::default()
        };
        if !readiness.eligible {
            readiness.blockers.push(json!({
                "code": if promise.state == "open" {
                    "open_promise_template"
                } else {
                    "promise_withdrawn"
                },
            }));
            projection.promises.insert(promise.uid.clone(), readiness);
            continue;
        }
        let Some(person_uid) = promise.person_uid.as_deref() else {
            readiness
                .blockers
                .push(json!({ "code": "promise_person_unassigned" }));
            projection.promises.insert(promise.uid.clone(), readiness);
            continue;
        };
        let party = levels_by_person.get(person_uid).copied();
        if party.is_none_or(|party| party.level < 2) {
            readiness.blockers.push(json!({
                "code": "party_agreement_required",
                "person": person_uid,
                "party": party.map(|party| party.party_uid.as_str()),
                "level": party.map_or(0, |party| party.level),
            }));
        }
        match input.agreement_type.as_str() {
            "full" => {
                for blocker in input.parties.iter().filter(|party| party.level < 2) {
                    readiness.blockers.push(json!({
                        "code": "party_agreement_required",
                        "person": blocker.person_uid,
                        "party": blocker.party_uid,
                        "level": blocker.level,
                    }));
                }
            }
            "percentage" => match (&input.coalition, &coalition_members, party) {
                (None, _, _) => readiness.blockers.push(json!({
                    "code": "percentage_coalition_not_frozen",
                    "required": input.agreement_pct,
                })),
                (Some(_), Some(members), Some(party)) if !members.contains(&party.party_uid) => {
                    readiness.eligible = false;
                    readiness.blockers.push(json!({
                        "code": "percentage_coalition_excludes_party",
                        "party": party.party_uid,
                    }));
                }
                (Some(_), Some(members), _) => {
                    for party_uid in members.iter().filter(|party| {
                        levels_by_party.get(party.as_str()).copied().unwrap_or(0) < 2
                    }) {
                        readiness.blockers.push(json!({
                            "code": "coalition_party_agreement_required",
                            "party": party_uid,
                            "level": levels_by_party
                                .get(party_uid.as_str())
                                .copied()
                                .unwrap_or(0),
                        }));
                    }
                }
                _ => {}
            },
            "individual" | "dependency" => {
                let matches = input
                    .promises
                    .iter()
                    .filter(|counterpart| agreement_path_matches(promise, counterpart))
                    .collect::<Vec<_>>();
                let relevant_people = if matches.is_empty() {
                    input
                        .parties
                        .iter()
                        .filter(|party| party.person_uid != person_uid)
                        .map(|party| (Some(party.person_uid.as_str()), Some(party), None))
                        .collect::<Vec<_>>()
                } else {
                    matches
                        .iter()
                        .map(|counterpart| {
                            let counterparty = counterpart
                                .person_uid
                                .as_deref()
                                .and_then(|person| levels_by_person.get(person).copied());
                            (
                                counterpart.person_uid.as_deref(),
                                counterparty,
                                Some(counterpart.uid.as_str()),
                            )
                        })
                        .collect::<Vec<_>>()
                };
                for (counterperson, counterparty, counterpart_promise) in relevant_people {
                    if counterparty.is_none_or(|party| party.level < 2) {
                        readiness.blockers.push(json!({
                            "code": "counterparty_agreement_required",
                            "promise": counterpart_promise,
                            "person": counterperson,
                            "party": counterparty.map(|party| party.party_uid.as_str()),
                            "level": counterparty.map_or(0, |party| party.level),
                        }));
                    }
                }
            }
            _ => readiness
                .blockers
                .push(json!({ "code": "unknown_agreement_policy" })),
        }
        for dependency in input.dependencies.iter().filter(|dependency| {
            dependency.scope == nucleus::transfer::TransferDependencyScope::Transfer
                || dependency.promise_uid.as_deref() == Some(promise.uid.as_str())
        }) {
            if !satisfied_dependencies
                .get(dependency.uid.as_str())
                .copied()
                .unwrap_or(false)
            {
                readiness.blockers.push(json!({
                    "code": "dependency_not_satisfied",
                    "dependency": dependency.uid,
                    "upstream_kind": dependency.upstream_kind.as_str(),
                    "upstream": dependency.upstream_uid,
                    "required_state": dependency.required_state,
                }));
            }
        }
        readiness.ready = readiness.eligible && readiness.blockers.is_empty();
        projection.promises.insert(promise.uid.clone(), readiness);
    }
    for party in &input.parties {
        let owned = input.promises.iter().filter(|promise| {
            promise.person_uid.as_deref() == Some(party.person_uid.as_str())
                && projection
                    .promises
                    .get(&promise.uid)
                    .is_some_and(|readiness| readiness.eligible)
        });
        let owned = owned.collect::<Vec<_>>();
        projection.people.insert(
            party.person_uid.clone(),
            !owned.is_empty()
                && owned.iter().all(|promise| {
                    projection
                        .promises
                        .get(&promise.uid)
                        .is_some_and(|readiness| readiness.ready)
                }),
        );
    }
    let eligible = projection
        .promises
        .values()
        .filter(|readiness| readiness.eligible)
        .collect::<Vec<_>>();
    if eligible.is_empty() {
        projection
            .blockers
            .push(json!({ "code": "no_agreeable_promises" }));
    }
    for (promise_uid, readiness) in &projection.promises {
        if readiness.eligible && !readiness.ready {
            projection.blockers.extend(
                readiness
                    .blockers
                    .iter()
                    .map(|blocker| with_promise_context(promise_uid, blocker)),
            );
        }
    }
    projection.ready = !eligible.is_empty() && eligible.iter().all(|readiness| readiness.ready);
    Ok((projection, dependency_status))
}

/// Server-derived Phase 3 gate used by later engine stages. This reads only
/// signed revision terms and their current store projections; it never grants
/// one Person's agreement to another.
pub async fn transfer_agreement_ready(
    store: &Store,
    transfer_uid: &str,
) -> Result<bool, ProteinError> {
    let input = store::transfers::agreement_readiness_input(&store.pool, transfer_uid).await?;
    Ok(derive_agreement_readiness(store, &input).await?.0.ready)
}

/// Exact Phase 4 activation candidates for one Person. This is intentionally
/// narrower than bundle readiness: individual agreement may unlock one path
/// while another path remains blocked.
pub async fn transfer_ready_promises_for_person(
    store: &Store,
    transfer_uid: &str,
    person_uid: &str,
) -> Result<Vec<String>, ProteinError> {
    let input = store::transfers::agreement_readiness_input(&store.pool, transfer_uid).await?;
    let readiness = derive_agreement_readiness(store, &input).await?.0;
    Ok(input
        .promises
        .iter()
        .filter(|promise| promise.person_uid.as_deref() == Some(person_uid))
        .filter(|promise| promise.state == "agreed")
        .filter(|promise| {
            readiness
                .promises
                .get(&promise.uid)
                .is_some_and(|state| state.ready)
        })
        .map(|promise| promise.uid.clone())
        .collect())
}

/// Resolve a directed occurrence without guessing among multiple people.
/// Exact opposite signed promises win. An unmatched promise is safe only when
/// exactly one other accepted Person can be its counterparty.
pub fn transfer_occurrence_roles(
    input: &store::transfers::TransferAgreementReadinessInput,
    promise_uid: &str,
) -> Result<(Option<String>, String, String), &'static str> {
    let promise = input
        .promises
        .iter()
        .find(|promise| promise.uid == promise_uid)
        .ok_or("promise_not_in_current_revision")?;
    if promise.state == "open" {
        return Err("open_promise_template");
    }
    let actor = promise
        .person_uid
        .as_deref()
        .ok_or("promise_person_unassigned")?;
    let coalition_people = input.coalition.as_ref().map(|coalition| {
        input
            .parties
            .iter()
            .filter(|party| coalition.party_uids.contains(&party.party_uid))
            .map(|party| party.person_uid.as_str())
            .collect::<HashSet<_>>()
    });
    let eligible_counterparty = |person: Option<&str>| {
        person.is_some_and(|person| {
            person != actor
                && coalition_people
                    .as_ref()
                    .is_none_or(|members| members.contains(person))
        })
    };
    let exact = input
        .promises
        .iter()
        .filter(|other| {
            agreement_path_matches(promise, other)
                && matches!(other.state.as_str(), "agreed" | "active")
                && eligible_counterparty(other.person_uid.as_deref())
        })
        .collect::<Vec<_>>();
    let (opposite, counterparty) = match exact.as_slice() {
        [other] => (
            Some(other.uid.clone()),
            other
                .person_uid
                .clone()
                .ok_or("occurrence_counterparty_missing")?,
        ),
        [] => {
            let others = input
                .parties
                .iter()
                .filter(|party| eligible_counterparty(Some(&party.person_uid)))
                .map(|party| party.person_uid.as_str())
                .collect::<HashSet<_>>();
            if others.len() != 1 {
                return Err("occurrence_counterparty_ambiguous");
            }
            (
                None,
                (*others.iter().next().expect("one counterparty")).into(),
            )
        }
        _ => return Err("occurrence_counterparty_ambiguous"),
    };
    if promise.delta < 0.0 {
        Ok((opposite, actor.into(), counterparty))
    } else if promise.delta > 0.0 {
        Ok((opposite, counterparty, actor.into()))
    } else {
        Err("occurrence_quantity_zero")
    }
}

const TRANSFER_STATUSES: &[&str] = &[
    "inactive",
    "draft",
    "withdrawn",
    "settled",
    "partially_settled",
    "satiated",
    "system_disputed",
    "disputed",
    "broken",
    "in_transfer",
    "agreed",
    "proposed",
];
const TRANSFER_VIEWER_ROLES: &[&str] = &["local", "creator", "participant", "invitee", "observer"];
const TRANSFER_INVITATION_STATES: &[&str] =
    &["pending", "accepted", "rejected", "withdrawn", "expired"];
const TRANSFER_PROMISE_STATES: &[&str] = &[
    "open",
    "proposed",
    "agreed",
    "active",
    "kept",
    "broken",
    "withdrawn",
];

fn transfer_query_error(code: &str, detail: impl std::fmt::Display) -> ProteinError {
    store::sqlx::Error::Protocol(format!("{code}: {detail}"))
}

fn transfer_predicate_name(predicate: &Predicate) -> &'static str {
    match predicate {
        Predicate::All(_) => "all",
        Predicate::Any(_) => "any",
        Predicate::Not(_) => "not",
        Predicate::UidEq(_) => "uid_eq",
        Predicate::OccurrenceIn(_) => "occurrence_in",
        Predicate::SlugEq(_) => "slug_eq",
        Predicate::RevisionEq(_) => "revision_eq",
        Predicate::RevisionLt(_) => "revision_lt",
        Predicate::RevisionLte(_) => "revision_lte",
        Predicate::RevisionGt(_) => "revision_gt",
        Predicate::RevisionGte(_) => "revision_gte",
        Predicate::StatusIn(_) => "status_in",
        Predicate::ViewerRoleIn(_) => "viewer_role_in",
        Predicate::InvitationStateIn(_) => "invitation_state_in",
        Predicate::PersonEq(_) => "person_eq",
        Predicate::RecordEq(_) => "record_eq",
        Predicate::ConceptIn(_) => "concept_in",
        Predicate::UnitEq(_) => "unit_eq",
        Predicate::WindowEndBefore(_) => "window_end_before",
        Predicate::WindowEndAfter(_) => "window_end_after",
        Predicate::QuantityLt(_) => "quantity_lt",
        Predicate::QuantityLte(_) => "quantity_lte",
        Predicate::QuantityGt(_) => "quantity_gt",
        Predicate::QuantityGte(_) => "quantity_gte",
        Predicate::QuantityEq(_) => "quantity_eq",
        Predicate::KindEq(_) => "kind_eq",
        Predicate::LinkedTo { .. } => "linked_to",
        Predicate::StateIn(_) => "state_in",
        Predicate::AtSince(_) => "at_since",
        Predicate::AtBefore(_) => "at_before",
        Predicate::ClassifiedIn(_) => "classified_in",
        Predicate::CauseKindEq(_) => "cause_kind_eq",
        Predicate::Near { .. } => "near",
        Predicate::OrganEq(_) => "organ_eq",
        Predicate::OrganIn(_) => "organ_in",
    }
}

#[derive(Default)]
struct TransferPredicateCtx {
    record_tokens: HashMap<String, Option<String>>,
    person_tokens: HashMap<String, Option<String>>,
    unit_tokens: HashMap<String, Option<String>>,
    concept_families: HashMap<String, HashSet<String>>,
    instants: HashMap<String, chrono::DateTime<chrono::FixedOffset>>,
}

impl TransferPredicateCtx {
    async fn prepare(store: &Store, predicates: &[Predicate]) -> Result<Self, ProteinError> {
        let mut context = Self::default();
        for predicate in predicates {
            context.prepare_one(store, predicate).await?;
        }
        Ok(context)
    }

    fn prepare_one<'a>(
        &'a mut self,
        store: &'a Store,
        predicate: &'a Predicate,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ProteinError>> + Send + 'a>>
    {
        Box::pin(async move {
            match predicate {
                Predicate::All(inner) | Predicate::Any(inner) => {
                    for predicate in inner {
                        self.prepare_one(store, predicate).await?;
                    }
                }
                Predicate::Not(inner) => self.prepare_one(store, inner).await?,
                Predicate::UidEq(_)
                | Predicate::SlugEq(_)
                | Predicate::RevisionEq(_)
                | Predicate::RevisionLt(_)
                | Predicate::RevisionLte(_)
                | Predicate::RevisionGt(_)
                | Predicate::RevisionGte(_) => {}
                Predicate::StatusIn(statuses) => {
                    Self::validate_values("status_in", statuses, TRANSFER_STATUSES)?;
                }
                Predicate::ViewerRoleIn(roles) => {
                    Self::validate_values("viewer_role_in", roles, TRANSFER_VIEWER_ROLES)?;
                }
                Predicate::InvitationStateIn(states) => {
                    Self::validate_values(
                        "invitation_state_in",
                        states,
                        TRANSFER_INVITATION_STATES,
                    )?;
                }
                Predicate::StateIn(states) => {
                    Self::validate_values("state_in", states, TRANSFER_PROMISE_STATES)?;
                }
                Predicate::PersonEq(token) => {
                    let resolved = store::records::resolve(&store.pool, token)
                        .await?
                        .filter(|record| record.kind == nucleus::RecordKind::Person.as_str())
                        .map(|record| record.uid);
                    self.person_tokens.insert(token.clone(), resolved);
                }
                Predicate::RecordEq(token) => {
                    let resolved = store::records::resolve(&store.pool, token)
                        .await?
                        .map(|record| record.uid);
                    self.record_tokens.insert(token.clone(), resolved);
                }
                Predicate::ConceptIn(token) => {
                    let family = match store::concepts::resolve(&store.pool, token).await? {
                        Some(uid) => store::concepts::descendants_including(&store.pool, &uid)
                            .await?
                            .into_iter()
                            .collect(),
                        None => HashSet::new(),
                    };
                    self.concept_families.insert(token.clone(), family);
                }
                Predicate::UnitEq(token) => {
                    let resolved = store::concepts::resolve(&store.pool, token).await?;
                    self.unit_tokens.insert(token.clone(), resolved);
                }
                Predicate::WindowEndBefore(value) | Predicate::WindowEndAfter(value) => {
                    let instant = chrono::DateTime::parse_from_rfc3339(value).map_err(|_| {
                        transfer_query_error(
                            "protein_transfer_invalid_window",
                            format!("`{value}` is not an RFC3339 instant"),
                        )
                    })?;
                    self.instants.insert(value.clone(), instant);
                }
                unsupported => {
                    return Err(transfer_query_error(
                        "protein_transfer_unsupported_predicate",
                        transfer_predicate_name(unsupported),
                    ));
                }
            }
            Ok(())
        })
    }

    fn validate_values(
        predicate: &str,
        values: &[String],
        supported: &[&str],
    ) -> Result<(), ProteinError> {
        if let Some(value) = values
            .iter()
            .find(|value| !supported.contains(&value.as_str()))
        {
            return Err(transfer_query_error(
                "protein_transfer_invalid_predicate_value",
                format!("{predicate} does not support `{value}`"),
            ));
        }
        Ok(())
    }

    fn matches(&self, row: &TransferPredicateRow<'_>, predicates: &[Predicate]) -> bool {
        predicates
            .iter()
            .all(|predicate| self.matches_one(row, predicate))
    }

    fn matches_one(&self, row: &TransferPredicateRow<'_>, predicate: &Predicate) -> bool {
        match predicate {
            Predicate::All(inner) => inner.iter().all(|item| self.matches_one(row, item)),
            Predicate::Any(inner) => inner.iter().any(|item| self.matches_one(row, item)),
            Predicate::Not(inner) => !self.matches_one(row, inner),
            Predicate::UidEq(uid) => row.uid == uid,
            Predicate::SlugEq(slug) => row.slug == Some(slug.as_str()),
            Predicate::RevisionEq(revision) => row.revision == *revision,
            Predicate::RevisionLt(revision) => row.revision < *revision,
            Predicate::RevisionLte(revision) => row.revision <= *revision,
            Predicate::RevisionGt(revision) => row.revision > *revision,
            Predicate::RevisionGte(revision) => row.revision >= *revision,
            Predicate::StatusIn(statuses) => statuses.iter().any(|status| status == row.status),
            Predicate::ViewerRoleIn(roles) => roles
                .iter()
                .any(|role| row.viewer_roles.contains(role.as_str())),
            Predicate::InvitationStateIn(states) => row.invitations.iter().any(|invitation| {
                states
                    .iter()
                    .any(|state| state == invitation.status.as_str())
            }),
            Predicate::StateIn(states) => row
                .promises
                .iter()
                .any(|promise| states.iter().any(|state| state == promise.state.as_str())),
            Predicate::PersonEq(token) => self
                .person_tokens
                .get(token)
                .and_then(|person| person.as_deref())
                .is_some_and(|person| {
                    row.parties.iter().any(|(_, actor, _)| actor == person)
                        || row.invitations.iter().any(|invitation| {
                            invitation.addressed_person_uid == person
                                || invitation.invited_by_person_uid == person
                        })
                        || row.revision_promises.map_or_else(
                            || {
                                row.promises
                                    .iter()
                                    .any(|promise| promise.party_uid.as_deref() == Some(person))
                            },
                            |promises| {
                                promises
                                    .iter()
                                    .any(|promise| promise.person_uid.as_deref() == Some(person))
                            },
                        )
                }),
            Predicate::RecordEq(token) => self
                .record_tokens
                .get(token)
                .and_then(|record| record.as_deref())
                .is_some_and(|record| {
                    row.revision_promises.map_or_else(
                        || {
                            row.promises
                                .iter()
                                .any(|promise| promise.record_uid.as_deref() == Some(record))
                        },
                        |promises| {
                            promises
                                .iter()
                                .any(|promise| promise.record_uid.as_deref() == Some(record))
                        },
                    )
                }),
            Predicate::ConceptIn(token) => self.concept_families.get(token).is_some_and(|family| {
                row.revision_promises.map_or_else(
                    || {
                        row.promises.iter().any(|promise| {
                            promise
                                .record_uid
                                .as_ref()
                                .and_then(|uid| row.records_by_uid.get(uid))
                                .and_then(|record| record.concept_uid.as_ref())
                                .is_some_and(|concept| family.contains(concept))
                        })
                    },
                    |promises| {
                        promises.iter().any(|promise| {
                            promise
                                .concept_uid
                                .as_ref()
                                .or_else(|| {
                                    promise
                                        .record_uid
                                        .as_ref()
                                        .and_then(|uid| row.records_by_uid.get(uid))
                                        .and_then(|record| record.concept_uid.as_ref())
                                })
                                .is_some_and(|concept| family.contains(concept))
                        })
                    },
                )
            }),
            Predicate::UnitEq(token) => self
                .unit_tokens
                .get(token)
                .and_then(|unit| unit.as_deref())
                .is_some_and(|unit| {
                    row.revision_promises.map_or_else(
                        || {
                            row.promises.iter().any(|promise| {
                                promise
                                    .record_uid
                                    .as_ref()
                                    .and_then(|uid| row.records_by_uid.get(uid))
                                    .and_then(|record| record.unit_uid.as_deref())
                                    == Some(unit)
                            })
                        },
                        |promises| {
                            promises
                                .iter()
                                .any(|promise| promise.unit_uid.as_deref() == Some(unit))
                        },
                    )
                }),
            Predicate::WindowEndBefore(value) => self.instants.get(value).is_some_and(|until| {
                let matches = |end: Option<&str>| {
                    end.and_then(|end| chrono::DateTime::parse_from_rfc3339(end).ok())
                        .is_some_and(|end| end < *until)
                };
                row.revision_promises.map_or_else(
                    || {
                        row.promises
                            .iter()
                            .any(|promise| matches(promise.window_end.as_deref()))
                    },
                    |promises| {
                        promises
                            .iter()
                            .any(|promise| matches(promise.window_end.as_deref()))
                    },
                )
            }),
            Predicate::WindowEndAfter(value) => self.instants.get(value).is_some_and(|since| {
                let matches = |end: Option<&str>| {
                    end.and_then(|end| chrono::DateTime::parse_from_rfc3339(end).ok())
                        .is_some_and(|end| end > *since)
                };
                row.revision_promises.map_or_else(
                    || {
                        row.promises
                            .iter()
                            .any(|promise| matches(promise.window_end.as_deref()))
                    },
                    |promises| {
                        promises
                            .iter()
                            .any(|promise| matches(promise.window_end.as_deref()))
                    },
                )
            }),
            _ => false,
        }
    }
}

struct TransferPredicateRow<'a> {
    uid: &'a str,
    slug: Option<&'a str>,
    revision: u64,
    status: &'a str,
    viewer_roles: &'a HashSet<&'static str>,
    invitations: &'a [store::transfers::TransferInvitationRow],
    parties: &'a [(String, String, i64)],
    promises: &'a [store::misc::PromiseRow],
    revision_promises: Option<&'a [nucleus::transfer::TransferRevisionPromise]>,
    records_by_uid: &'a HashMap<String, store::records::RecordRow>,
}

fn parse_transfer_revision_evidence(
    fact: &nucleus::Fact,
) -> Option<nucleus::transfer::TransferRevisionEvidence> {
    serde_json::from_str(fact.payload.as_deref()?).ok()
}

fn transfer_revision_summary(
    fact: &nucleus::Fact,
    evidence: &nucleus::transfer::TransferRevisionEvidence,
    include_terms: bool,
) -> Value {
    let mut value = json!({
        "revision": evidence.terms.revision,
        "previous_revision": evidence.previous_revision,
        "action": evidence.action,
        "fact": fact.uid,
        "actor": fact.actor_uid,
        "at": fact.at.to_rfc3339(),
        "hash": fact.hash,
        "signature": fact.signature,
        "signed": fact.signature.is_some(),
        "party_count": evidence.terms.parties.len(),
        "invitation_count": evidence.terms.invitations.len(),
        "promise_count": evidence.terms.promises.len(),
    });
    if include_terms {
        value["terms"] = json!(evidence.terms);
    }
    value
}

fn collect_changed_fields(before: &Value, after: &Value, path: &str, out: &mut Vec<String>) {
    match (before, after) {
        (Value::Object(before), Value::Object(after)) => {
            let keys: std::collections::BTreeSet<_> = before
                .keys()
                .chain(after.keys())
                .map(String::as_str)
                .collect();
            for key in keys {
                if path.is_empty() && key == "revision" {
                    continue;
                }
                let next = if path.is_empty() {
                    key.to_string()
                } else {
                    format!("{path}.{key}")
                };
                match (before.get(key), after.get(key)) {
                    (Some(before), Some(after)) => {
                        collect_changed_fields(before, after, &next, out);
                    }
                    _ => out.push(next),
                }
            }
        }
        // Collections in a revision snapshot are canonically uid-sorted. A
        // collection-level path is more useful to clients than unstable array
        // indexes; the current terms carry the exact signed replacement.
        (Value::Array(_), Value::Array(_)) if before != after => out.push(path.to_string()),
        _ if before != after => out.push(path.to_string()),
        _ => {}
    }
}

async fn transfer_revision_projection(
    store: &Store,
    transfer_uid: &str,
    revision: u64,
    include_terms: bool,
) -> Result<Value, ProteinError> {
    if revision == 0 {
        return Ok(json!({
            "current": null,
            "previous": null,
            "changed_fields": [],
            "blocking_reason": "legacy_transfer_requires_adoption",
        }));
    }
    let current = store::transfers::revision_fact(&store.pool, transfer_uid, revision).await?;
    let current_evidence = current.as_ref().and_then(parse_transfer_revision_evidence);
    let previous = if revision > 1 {
        store::transfers::revision_fact(&store.pool, transfer_uid, revision - 1).await?
    } else {
        None
    };
    let previous_evidence = previous.as_ref().and_then(parse_transfer_revision_evidence);

    let mut changed_fields = Vec::new();
    if let (Some(previous), Some(current)) = (&previous_evidence, &current_evidence) {
        let before = serde_json::to_value(&previous.terms)
            .map_err(|error| transfer_query_error("protein_transfer_evidence_invalid", error))?;
        let after = serde_json::to_value(&current.terms)
            .map_err(|error| transfer_query_error("protein_transfer_evidence_invalid", error))?;
        collect_changed_fields(&before, &after, "", &mut changed_fields);
    }

    Ok(json!({
        "current": current.as_ref().zip(current_evidence.as_ref())
            .map(|(fact, evidence)| transfer_revision_summary(fact, evidence, include_terms)),
        "previous": previous.as_ref().zip(previous_evidence.as_ref())
            .map(|(fact, evidence)| transfer_revision_summary(fact, evidence, false)),
        "changed_fields": changed_fields,
        "blocking_reason": if current_evidence.is_none() {
            Some("transfer_revision_evidence_missing")
        } else {
            None
        },
    }))
}

struct TransferTimelineSeed {
    uid: String,
    kind: String,
    at: String,
    fact_uid: String,
    detail: Value,
}

async fn transfer_fact_proof(store: &Store, fact_uid: &str) -> Result<Value, ProteinError> {
    let Some(fact) = store::facts::get(&store.pool, fact_uid).await? else {
        return Ok(json!({
            "fact": fact_uid,
            "state": "missing",
            "authoritative": false,
        }));
    };
    let intent = store::action_intents::for_fact(&store.pool, fact_uid).await?;
    let keys = match fact.actor_uid.as_deref() {
        Some(actor) => store::action_intents::published_keys_for_actor(&store.pool, actor).await?,
        None => Vec::new(),
    };
    let verify = |bytes: &[u8], signature: &str, key_id: Option<&str>| {
        let Ok(signature) = B64.decode(signature) else {
            return false;
        };
        let Ok(signature) = Signature::from_slice(&signature) else {
            return false;
        };
        keys.iter()
            .filter(|(published_id, _)| key_id.is_none_or(|key_id| published_id == key_id))
            .any(|(_, public_key)| {
                let Ok(public_key) = B64.decode(public_key) else {
                    return false;
                };
                let Ok(public_key) = <[u8; 32]>::try_from(public_key.as_slice()) else {
                    return false;
                };
                VerifyingKey::from_bytes(&public_key)
                    .is_ok_and(|key| key.verify(bytes, &signature).is_ok())
            })
    };
    let direct_valid = fact
        .signature
        .as_deref()
        .is_some_and(|signature| verify(fact.hash.as_bytes(), signature, None));
    let intent_valid = intent.as_ref().is_some_and(|intent| {
        fact.actor_uid.as_deref() == Some(intent.actor_person_uid.as_str())
            && intent.status == "committed"
            && verify(
                &nucleus::action_intent::signing_bytes(
                    &intent.session_id,
                    &intent.session_challenge,
                    intent.sequence,
                    &intent.message_id,
                    &intent.action_base64,
                ),
                &intent.signature,
                Some(&intent.key_id),
            )
    });
    let mechanism = if direct_valid {
        "direct_fact_signature"
    } else if intent_valid {
        "verified_action_intent"
    } else if fact.signature.is_some() || intent.is_some() {
        "invalid"
    } else if fact.actor_uid.is_none() {
        "unsigned_system"
    } else {
        "unsigned_local"
    };
    Ok(json!({
        "fact": fact.uid,
        "record": fact.record_uid,
        "actor": fact.actor_uid,
        "at": fact.at.to_rfc3339(),
        "hash": fact.hash,
        "previous_hash": fact.prev_hash,
        "mechanism": mechanism,
        "state": mechanism,
        "authoritative": direct_valid || intent_valid,
        "fact_signature": fact.signature.as_ref().map(|signature| json!({
            "present": true,
            "signature": signature,
        })),
        "action_intent": intent.as_ref().map(|intent| json!({
            "uid": intent.uid,
            "actor": intent.actor_person_uid,
            "key_id": intent.key_id,
            "message_id": intent.message_id,
            "sequence": intent.sequence,
            "status": intent.status,
            "signature": intent.signature,
        })),
    }))
}

async fn transfer_timeline(
    store: &Store,
    transfer_uid: &str,
    revision: u64,
    invitations: &[store::transfers::TransferInvitationRow],
    agreement_events: &[store::transfers::AgreementTransitionEventRow],
    occurrences: &[store::transfers::TransferOccurrenceRow],
    source_group: Option<&store::transfers::TransferSourceGroupState>,
    correction_lineage: &[store::transfers::TransferCorrectionLinkRow],
    promise_successors: &[store::transfers::PromiseSuccessorRow],
    recipient_details_authorized: bool,
    viewer_person: Option<&str>,
) -> Result<Vec<Value>, ProteinError> {
    let mut seeds = Vec::new();
    for number in 1..=revision {
        let Some(fact) = store::transfers::revision_fact(&store.pool, transfer_uid, number).await?
        else {
            continue;
        };
        let evidence = parse_transfer_revision_evidence(&fact);
        seeds.push(TransferTimelineSeed {
            uid: format!("revision:{number}"),
            kind: "revision".into(),
            at: fact.at.to_rfc3339(),
            fact_uid: fact.uid,
            detail: json!({
                "revision": number,
                "action": evidence.as_ref().map(|value| value.action.as_str()),
                "previous_revision": evidence.as_ref().and_then(|value| value.previous_revision),
            }),
        });
    }
    for invitation in invitations {
        for event in store::transfers::invitation_events(&store.pool, &invitation.uid).await? {
            seeds.push(TransferTimelineSeed {
                uid: event.uid,
                kind: "invitation".into(),
                at: event.created_at,
                fact_uid: event.fact_uid,
                detail: json!({
                    "invitation": event.invitation_uid,
                    "event": event.kind,
                    "attempt": event.attempt,
                    "revision": event.revision,
                    "person": (recipient_details_authorized
                        || viewer_person == Some(invitation.addressed_person_uid.as_str()))
                        .then_some(invitation.addressed_person_uid.as_str()),
                    "request_id": event.idempotency_key,
                }),
            });
        }
    }
    for event in agreement_events {
        seeds.push(TransferTimelineSeed {
            uid: event.uid.clone(),
            kind: "agreement".into(),
            at: event.created_at.clone(),
            fact_uid: event.fact_uid.clone(),
            detail: json!({
                "revision": event.revision,
                "party": recipient_details_authorized
                    .then_some(event.party_uid.as_str()),
                "person": (recipient_details_authorized
                    || viewer_person == Some(event.person_uid.as_str()))
                    .then_some(event.person_uid.as_str()),
                "from_level": event.from_level,
                "to_level": event.to_level,
                "request_id": event.idempotency_key,
            }),
        });
    }
    let mut activation_facts = HashSet::new();
    for occurrence in occurrences {
        if activation_facts.insert(occurrence.activation_fact_uid.as_str()) {
            seeds.push(TransferTimelineSeed {
                uid: occurrence.activation_event_uid.clone(),
                kind: "activation".into(),
                at: occurrence.created_at.clone(),
                fact_uid: occurrence.activation_fact_uid.clone(),
                detail: json!({ "revision": occurrence.revision }),
            });
        }
        for event in store::transfers::occurrence_claim_events(&store.pool, &occurrence.uid).await?
        {
            seeds.push(TransferTimelineSeed {
                uid: event.uid,
                kind: "claim".into(),
                at: event.created_at,
                fact_uid: event.fact_uid,
                detail: json!({
                    "occurrence": event.occurrence_uid,
                    "role": event.role.as_str(),
                    "asserted": event.asserted,
                    "person": event.actor_person_uid,
                    "request_id": event.idempotency_key,
                }),
            });
        }
        for slice in
            store::transfers::occurrence_settlement_slices(&store.pool, &occurrence.uid).await?
        {
            seeds.push(TransferTimelineSeed {
                uid: slice.uid,
                kind: "settlement".into(),
                at: slice.created_at,
                fact_uid: slice.evidence_fact_uid,
                detail: json!({
                    "occurrence": slice.occurrence_uid,
                    "promise": slice.promise_uid,
                    "person": slice.owner_person_uid,
                    "canonical_quantity": slice.canonical_quantity,
                    "canonical_unit": slice.canonical_unit_uid,
                    "remaining_after": slice.remaining_after,
                    "request_id": slice.idempotency_key,
                }),
            });
        }
        for event in
            store::transfers::occurrence_dispute_events(&store.pool, &occurrence.uid).await?
        {
            seeds.push(TransferTimelineSeed {
                uid: event.uid,
                kind: "dispute".into(),
                at: event.created_at,
                fact_uid: event.fact_uid,
                detail: json!({
                    "occurrence": event.occurrence_uid,
                    "person": event.actor_person_uid,
                    "asserted": event.disputed,
                    "request_id": event.idempotency_key,
                }),
            });
        }
        for correction in
            store::transfers::occurrence_settlement_compensations(&store.pool, &occurrence.uid)
                .await?
        {
            seeds.push(TransferTimelineSeed {
                uid: correction.uid,
                kind: "settlement_compensation".into(),
                at: correction.created_at,
                fact_uid: correction.compensation_fact_uid,
                detail: json!({
                    "occurrence": correction.occurrence_uid,
                    "settlement": correction.settlement_uid,
                    "person": correction.owner_person_uid,
                    "inverse_delta": correction.inverse_delta,
                    "request_id": correction.idempotency_key,
                }),
            });
        }
    }
    if let Some(group) = source_group {
        seeds.push(TransferTimelineSeed {
            uid: group.result.uid.clone(),
            kind: "first_completes_result".into(),
            at: group.result.created_at.clone(),
            fact_uid: group.result.fact_uid.clone(),
            detail: json!({
                "source": group.result.source_uid,
                "winner": group.result.winner_transfer_uid,
                "winner_revision": group.result.winner_revision,
                "settlement": group.result.settlement_uid,
            }),
        });
        if let Some(loser) = &group.loser {
            seeds.push(TransferTimelineSeed {
                uid: loser.uid.clone(),
                kind: "first_completes_loser".into(),
                at: loser.created_at.clone(),
                fact_uid: loser.fact_uid.clone(),
                detail: json!({
                    "winner": group.result.winner_transfer_uid,
                    "loser": loser.transfer_uid,
                    "loser_revision": loser.transfer_revision,
                }),
            });
        }
    }
    if recipient_details_authorized {
        for correction in correction_lineage {
            seeds.push(TransferTimelineSeed {
                uid: format!("correction:{}:{transfer_uid}", correction.uid),
                kind: format!("correction_{}", correction.kind),
                at: correction.created_at.clone(),
                fact_uid: correction.fact_uid.clone(),
                detail: json!({
                    "role": if correction.source_transfer_uid == transfer_uid {
                        "source"
                    } else {
                        "created"
                    },
                    "source_transfer": correction.source_transfer_uid,
                    "source_occurrence": correction.source_occurrence_uid,
                    "created_transfer": correction.created_transfer_uid,
                    "source_revision": correction.source_revision,
                    "canonical_quantity": correction.canonical_quantity,
                    "actor": correction.actor_person_uid,
                    "request_id": correction.idempotency_key,
                }),
            });
        }
        for successor in promise_successors {
            seeds.push(TransferTimelineSeed {
                uid: format!("promise-successor:{}", successor.uid),
                kind: "promise_successor".into(),
                at: successor.created_at.clone(),
                fact_uid: successor.fact_uid.clone(),
                detail: json!({
                    "predecessor": successor.predecessor_promise_uid,
                    "successor": successor.successor_promise_uid,
                    "revision": successor.revision,
                    "actor": successor.actor_person_uid,
                    "request_id": successor.idempotency_key,
                }),
            });
        }
    }
    for fact in store::facts::for_record(&store.pool, transfer_uid, 10_000).await? {
        let Some(payload) = fact
            .payload
            .as_deref()
            .and_then(|payload| serde_json::from_str::<Value>(payload).ok())
        else {
            continue;
        };
        let Some(confirmation) = payload.get("confirmation").and_then(Value::as_str) else {
            continue;
        };
        seeds.push(TransferTimelineSeed {
            uid: format!("confirmation:{}", fact.uid),
            kind: "legacy_confirmation".into(),
            at: fact.at.to_rfc3339(),
            fact_uid: fact.uid,
            detail: json!({ "confirmation": confirmation }),
        });
    }
    seeds.sort_by(|left, right| {
        left.at
            .cmp(&right.at)
            .then_with(|| left.uid.cmp(&right.uid))
    });
    let mut timeline = Vec::with_capacity(seeds.len());
    for seed in seeds {
        timeline.push(json!({
            "uid": seed.uid,
            "kind": seed.kind,
            "at": seed.at,
            "fact": seed.fact_uid,
            "detail": seed.detail,
            "proof": transfer_fact_proof(store, &seed.fact_uid).await?,
        }));
    }
    Ok(timeline)
}

fn transfer_settlement_preview_filter(protein: &Protein) -> Result<(&str, f64), ProteinError> {
    if protein.aggregate.is_some()
        || !protein.order.is_empty()
        || protein.limit.is_some()
        || protein.include.facts.is_some()
        || protein.include.promises.is_some()
        || protein.include.links.is_some()
        || protein.include.threads.is_some()
        || protein.include.availability
        || protein.include.extension.is_some()
        || protein.include.projection.is_some()
    {
        return Err(transfer_query_error(
            "protein_transfer_settlement_preview_invalid_query",
            "include, aggregate, order, and limit are not supported",
        ));
    }
    let mut occurrence_uid = None;
    let mut canonical_quantity = None;
    for predicate in &protein.filter {
        match predicate {
            Predicate::UidEq(uid) if occurrence_uid.is_none() => {
                occurrence_uid = Some(uid.as_str());
            }
            Predicate::QuantityEq(quantity) if canonical_quantity.is_none() => {
                canonical_quantity = Some(*quantity);
            }
            predicate => {
                return Err(transfer_query_error(
                    "protein_transfer_settlement_preview_unsupported_predicate",
                    transfer_predicate_name(predicate),
                ));
            }
        }
    }
    match (occurrence_uid, canonical_quantity) {
        (Some(occurrence_uid), Some(canonical_quantity)) => {
            Ok((occurrence_uid, canonical_quantity))
        }
        _ => Err(transfer_query_error(
            "protein_transfer_settlement_preview_invalid_query",
            "one direct uid_eq and one direct quantity_eq predicate are required",
        )),
    }
}

async fn execute_transfer_settlement_preview(
    store: &Store,
    protein: &Protein,
    subject: Option<&str>,
    installed_signer_actor: Option<&str>,
) -> Result<Vec<Value>, ProteinError> {
    let (occurrence_uid, requested_quantity) = transfer_settlement_preview_filter(protein)?;
    if !requested_quantity.is_finite() || requested_quantity <= 0.0 {
        return Ok(Vec::new());
    }
    let Some(occurrence) = store::transfers::occurrence(&store.pool, occurrence_uid).await? else {
        return Ok(Vec::new());
    };
    let Some(source_promise) =
        store::misc::get_promise(&store.pool, &occurrence.promise_uid).await?
    else {
        return Ok(Vec::new());
    };
    let Some(owner_person_uid) = source_promise.party_uid.as_deref() else {
        return Ok(Vec::new());
    };
    let viewer = TransferViewer::resolve(store, subject).await?;
    if !viewer.update_identity_blockers().is_empty()
        || installed_signer_actor != Some(owner_person_uid)
        || (!viewer.local && viewer.person.as_deref() != Some(owner_person_uid))
    {
        return Ok(Vec::new());
    }
    let Some(local_record_uid) = source_promise.record_uid.as_deref() else {
        return Ok(Vec::new());
    };
    if occurrence.record_uid.as_deref() != Some(local_record_uid)
        || source_promise.state != nucleus::PromiseState::Active
        || !occurrence.delivery_claimed
        || !occurrence.receipt_claimed
        || occurrence.disputed
        || !source_promise.delta.is_finite()
        || !((source_promise.delta < 0.0 && occurrence.giver_person_uid == owner_person_uid)
            || (source_promise.delta > 0.0 && occurrence.receiver_person_uid == owner_person_uid))
    {
        return Ok(Vec::new());
    }
    let Some(local_record) = store::records::get(&store.pool, local_record_uid).await? else {
        return Ok(Vec::new());
    };
    let local_organ_uid = store::organs::local(&store.pool)
        .await?
        .map(|organ| organ.uid);
    if local_record
        .organ_uid
        .as_deref()
        .is_some_and(|organ| Some(organ) != local_organ_uid.as_deref())
    {
        return Ok(Vec::new());
    }
    let Some(progress) =
        store::transfers::occurrence_settlement_progress(&store.pool, occurrence_uid).await?
    else {
        return Ok(Vec::new());
    };
    if progress.remaining_quantity <= 0.0 || requested_quantity > progress.remaining_quantity {
        return Ok(Vec::new());
    }
    let cell_formula = store::config::transfer_application_formula(&store.pool).await?;
    let application = effective_settlement_application(
        store,
        &occurrence,
        &source_promise,
        owner_person_uid,
        &cell_formula,
    )
    .await?;
    if application.error.is_some() {
        return Ok(Vec::new());
    }
    let canonical_cumulative_after = progress.settled_quantity + requested_quantity;
    let Ok(local_cumulative_after) =
        evaluate_occurrence_application_formula(&application.formula, canonical_cumulative_after)
    else {
        return Ok(Vec::new());
    };
    let prior_local_applied = progress
        .slices
        .iter()
        .map(|slice| slice.local_delta)
        .sum::<f64>();
    let local_delta = local_cumulative_after - prior_local_applied;
    if !local_delta.is_finite() {
        return Ok(Vec::new());
    }
    let remainder_policy = store::transfers::effective_occurrence_remainder_policy(
        &store.pool,
        occurrence_uid,
        owner_person_uid,
    )
    .await?;
    Ok(vec![json!({
        "kind": "transfer_settlement_preview",
        "uid": occurrence.uid,
        "transfer": occurrence.transfer_uid,
        "occurrence": occurrence.uid,
        "promise": occurrence.promise_uid,
        "person": owner_person_uid,
        "revision": occurrence.revision,
        "status": "ready",
        "canonical_unit": occurrence.unit_uid,
        "canonical_quantity": requested_quantity,
        "settled_quantity": progress.settled_quantity,
        "remaining_quantity": progress.remaining_quantity,
        "remaining_after": progress.remaining_quantity - requested_quantity,
        "local_record": local_record_uid,
        "local_delta": local_delta,
        "local_cumulative_after": local_cumulative_after,
        "application_formula_hash": application.formula_hash,
        "application_formula_version": application.version,
        "remainder_policy": remainder_policy.as_str(),
        "expected_remaining_quantity": progress.remaining_quantity,
        "expected_local_delta": local_delta,
        "expected_application_formula_hash": application.formula_hash,
        "expected_application_formula_version": application.version,
        "expected_remainder_policy": remainder_policy.as_str(),
        "capabilities": { "settle": true },
        "blocking_reasons": { "settle": Vec::<&str>::new() },
    })])
}

fn transfer_bulk_completion_filter(protein: &Protein) -> Result<(&str, Vec<String>), ProteinError> {
    if protein.aggregate.is_some()
        || !protein.order.is_empty()
        || protein.limit.is_some()
        || protein.include.facts.is_some()
        || protein.include.promises.is_some()
        || protein.include.links.is_some()
        || protein.include.threads.is_some()
        || protein.include.availability
        || protein.include.extension.is_some()
        || protein.include.projection.is_some()
    {
        return Err(transfer_query_error(
            "protein_transfer_bulk_completion_invalid_query",
            "include, aggregate, order, and limit are not supported",
        ));
    }
    let mut root = None;
    let mut occurrences = None;
    for predicate in &protein.filter {
        match predicate {
            Predicate::UidEq(uid) if root.is_none() => root = Some(uid.as_str()),
            Predicate::OccurrenceIn(selected) if occurrences.is_none() => {
                occurrences = Some(selected.clone())
            }
            predicate => {
                return Err(transfer_query_error(
                    "protein_transfer_bulk_completion_unsupported_predicate",
                    transfer_predicate_name(predicate),
                ));
            }
        }
    }
    let (Some(root), Some(mut occurrences)) = (root, occurrences) else {
        return Err(transfer_query_error(
            "protein_transfer_bulk_completion_invalid_query",
            "one direct uid_eq root and one direct occurrence_in selection are required",
        ));
    };
    occurrences.sort();
    occurrences.dedup();
    if occurrences.is_empty() {
        return Err(transfer_query_error(
            "protein_transfer_bulk_completion_invalid_query",
            "occurrence_in must select at least one occurrence",
        ));
    }
    Ok((root, occurrences))
}

fn transfer_is_in_tree(
    transfer_uid: &str,
    root_uid: &str,
    parents: &HashMap<String, Option<String>>,
) -> bool {
    let mut current = Some(transfer_uid);
    let mut seen = HashSet::new();
    while let Some(uid) = current {
        if uid == root_uid {
            return true;
        }
        if !seen.insert(uid.to_string()) {
            return false;
        }
        current = parents.get(uid).and_then(|parent| parent.as_deref());
    }
    false
}

async fn transfer_visible_for_viewer(
    store: &Store,
    transfer_uid: &str,
    viewer: &TransferViewer,
    granted: Option<&HashSet<String>>,
) -> Result<bool, ProteinError> {
    if viewer.local || granted.is_some_and(|targets| targets.contains(transfer_uid)) {
        return Ok(true);
    }
    if store::facts::creator_uid(&store.pool, transfer_uid)
        .await?
        .as_deref()
        == viewer.subject.as_deref()
    {
        return Ok(true);
    }
    let Some(person) = viewer.person.as_deref() else {
        return Ok(false);
    };
    if store::transfers::creator_party_actor(&store.pool, transfer_uid)
        .await?
        .as_deref()
        == Some(person)
        || store::transfers::party_for_actor(&store.pool, transfer_uid, person)
            .await?
            .is_some()
    {
        return Ok(true);
    }
    Ok(
        store::transfers::invitations_for_transfer(&store.pool, transfer_uid)
            .await?
            .iter()
            .any(|invitation| invitation.addressed_person_uid == person),
    )
}

async fn execute_transfer_bulk_completion_preview(
    store: &Store,
    protein: &Protein,
    subject: Option<&str>,
    installed_signer_actor: Option<&str>,
) -> Result<Vec<Value>, ProteinError> {
    let (root_uid, selected_occurrences) = transfer_bulk_completion_filter(protein)?;
    let viewer = TransferViewer::resolve(store, subject).await?;
    let acting_person = if viewer.local {
        installed_signer_actor
    } else if viewer.person.as_deref() == installed_signer_actor {
        viewer.person.as_deref()
    } else {
        None
    };
    let Some(acting_person) = acting_person else {
        return Ok(Vec::new());
    };
    if !viewer.update_identity_blockers().is_empty() {
        return Ok(Vec::new());
    }
    let transfers = store::transfers::list_all(&store.pool).await?;
    let parents = transfers
        .iter()
        .map(|transfer| {
            (
                transfer.transfer.record_uid.clone(),
                transfer.transfer.parent_uid.clone(),
            )
        })
        .collect::<HashMap<_, _>>();
    let revisions = transfers
        .iter()
        .map(|transfer| {
            (
                transfer.transfer.record_uid.as_str(),
                transfer.transfer.revision as u64,
            )
        })
        .collect::<HashMap<_, _>>();
    let Some(root_revision) = revisions.get(root_uid).copied() else {
        return Ok(Vec::new());
    };
    let granted = match subject {
        Some(subject) => Some(store::visibility::visible_targets(&store.pool, subject).await?),
        None => None,
    };
    if !transfer_visible_for_viewer(store, root_uid, &viewer, granted.as_ref()).await? {
        return Ok(Vec::new());
    }

    let mut items = Vec::new();
    let mut review_items = Vec::new();
    for selected_uid in &selected_occurrences {
        let Some(occurrence) = store::transfers::occurrence(&store.pool, selected_uid).await?
        else {
            items.push(json!({
                "occurrence": selected_uid,
                "role": null,
                "eligible": false,
                "blockers": ["occurrence_missing"],
            }));
            continue;
        };
        let in_tree = transfer_is_in_tree(&occurrence.transfer_uid, root_uid, &parents);
        let transfer_visible =
            transfer_visible_for_viewer(store, &occurrence.transfer_uid, &viewer, granted.as_ref())
                .await?;
        let transfer_revision = revisions
            .contains_key(occurrence.transfer_uid.as_str())
            .then_some(occurrence.revision);
        if !transfer_visible {
            items.push(json!({
                "occurrence": selected_uid,
                "role": null,
                "eligible": false,
                "blockers": ["occurrence_not_visible"],
            }));
            continue;
        }
        let roles = [
            (
                nucleus::transfer::OccurrenceClaimRole::Delivery,
                occurrence.giver_person_uid.as_str(),
                occurrence.delivery_claimed,
            ),
            (
                nucleus::transfer::OccurrenceClaimRole::Receipt,
                occurrence.receiver_person_uid.as_str(),
                occurrence.receipt_claimed,
            ),
        ];
        let participant_roles = roles
            .into_iter()
            .filter(|(_, person, _)| *person == acting_person)
            .collect::<Vec<_>>();
        if participant_roles.is_empty() {
            items.push(json!({
                "occurrence": occurrence.uid,
                "transfer": occurrence.transfer_uid,
                "transfer_revision": transfer_revision,
                "role": null,
                "eligible": false,
                "blockers": if in_tree {
                    vec!["not_occurrence_participant"]
                } else {
                    vec!["occurrence_outside_selected_tree", "not_occurrence_participant"]
                },
            }));
            continue;
        }
        let missing_roles = participant_roles
            .iter()
            .copied()
            .filter(|(_, _, claimed)| !*claimed)
            .collect::<Vec<_>>();
        let reviewed_roles = if missing_roles.is_empty() {
            participant_roles.first().copied().into_iter().collect()
        } else {
            missing_roles
        };
        let claim_events =
            store::transfers::occurrence_claim_events(&store.pool, &occurrence.uid).await?;
        let progress =
            store::transfers::occurrence_settlement_progress(&store.pool, &occurrence.uid).await?;
        let source_group_state = store::transfers::source_group_state_for_transfer(
            &store.pool,
            &occurrence.transfer_uid,
        )
        .await?;
        for (role, _, claimed) in reviewed_roles {
            let role_events = claim_events
                .iter()
                .filter(|event| event.role == role)
                .collect::<Vec<_>>();
            let latest = role_events.last().copied();
            let mut blockers = Vec::new();
            if !in_tree {
                blockers.push("occurrence_outside_selected_tree");
            }
            if transfer_revision.is_none() {
                blockers.push("transfer_missing");
            }
            if claimed {
                blockers.push("claim_already_asserted");
            }
            if occurrence.disputed || occurrence.system_disputed {
                blockers.push("occurrence_disputed");
            }
            if progress.as_ref().is_some_and(|progress| progress.settled) {
                blockers.push("occurrence_already_settled");
            }
            if source_group_state
                .as_ref()
                .is_some_and(|state| state.satiated)
            {
                blockers.push("transfer_satiated");
            }
            let eligible = blockers.is_empty();
            if eligible {
                review_items.push(nucleus::transfer::TransferBulkClaimReviewItem {
                    occurrence_uid: occurrence.uid.clone(),
                    transfer_uid: occurrence.transfer_uid.clone(),
                    transfer_revision: transfer_revision
                        .expect("eligible occurrence has a Transfer revision"),
                    role,
                    delivery_claimed: occurrence.delivery_claimed,
                    receipt_claimed: occurrence.receipt_claimed,
                });
            }
            items.push(json!({
                "occurrence": occurrence.uid,
                "transfer": occurrence.transfer_uid,
                "transfer_revision": transfer_revision,
                "role": role.as_str(),
                "current_claimed": claimed,
                "claim_version": role_events.len(),
                "claim_token": latest
                    .map(|event| event.uid.as_str())
                    .unwrap_or(occurrence.activation_fact_uid.as_str()),
                "occurrence_state_token": claim_events
                    .last()
                    .map(|event| event.uid.as_str())
                    .unwrap_or(occurrence.activation_fact_uid.as_str()),
                "delivery_claim_token": claim_events
                    .iter()
                    .rev()
                    .find(|event| event.role == nucleus::transfer::OccurrenceClaimRole::Delivery)
                    .map(|event| event.uid.as_str()),
                "receipt_claim_token": claim_events
                    .iter()
                    .rev()
                    .find(|event| event.role == nucleus::transfer::OccurrenceClaimRole::Receipt)
                    .map(|event| event.uid.as_str()),
                "occurrence_state": {
                    "delivery_claimed": occurrence.delivery_claimed,
                    "receipt_claimed": occurrence.receipt_claimed,
                    "disputed": occurrence.disputed,
                },
                "eligible": eligible,
                "blockers": blockers,
                "action": if eligible {
                    Some(json!({
                        "occurrence": occurrence.uid,
                        "transfer": occurrence.transfer_uid,
                        "expected_revision": transfer_revision,
                        "role": role.as_str(),
                        "expected_delivery_claimed": occurrence.delivery_claimed,
                        "expected_receipt_claimed": occurrence.receipt_claimed,
                    }))
                } else {
                    None
                },
            }));
        }
    }
    items.sort_by(|left, right| {
        left.get("occurrence")
            .and_then(Value::as_str)
            .cmp(&right.get("occurrence").and_then(Value::as_str))
            .then_with(|| {
                left.get("role")
                    .and_then(Value::as_str)
                    .cmp(&right.get("role").and_then(Value::as_str))
            })
    });
    let eligible_count = items
        .iter()
        .filter(|item| item.get("eligible").and_then(Value::as_bool) == Some(true))
        .count();
    let blocked_count = items.len() - eligible_count;
    let review_token =
        nucleus::transfer::transfer_bulk_claim_review_token(acting_person, &review_items);
    Ok(vec![json!({
        "kind": "transfer_bulk_completion_preview",
        "root": root_uid,
        "root_revision": root_revision,
        "person": acting_person,
        "review_token": review_token,
        "selection": selected_occurrences,
        "eligible": eligible_count > 0 && blocked_count == 0,
        "item_count": items.len(),
        "eligible_count": eligible_count,
        "blocked_count": blocked_count,
        "blockers": if eligible_count == 0 {
            vec!["no_eligible_selected_occurrence_claims"]
        } else if blocked_count > 0 {
            vec!["selection_contains_blocked_items"]
        } else {
            Vec::new()
        },
        "items": items,
    })])
}

async fn origin_social_delivery_projection(
    store: &Store,
    transfer_uid: &str,
    transfer_revision: u64,
    acting_person: Option<&str>,
    can_admin_delivery: bool,
) -> Result<Value, ProteinError> {
    let local_organ = store::organs::local(&store.pool)
        .await?
        .ok_or(store::sqlx::Error::RowNotFound)?;
    let policies =
        store::transfer_delivery::policies_for_transfer(&store.pool, transfer_uid).await?;
    let eligible = store::sqlx::query_as::<
        _,
        (
            String,
            String,
            Option<String>,
            String,
            String,
            Option<String>,
            String,
        ),
    >(
        "SELECT DISTINCT person.uid, person.head, person.slug, person.organ_uid,
                organ.head, organ.slug, contact.trust
         FROM record person
         JOIN record organ ON organ.uid = person.organ_uid
         JOIN organ_contact contact ON contact.record_uid = organ.uid
         JOIN transfer_party party ON party.actor_uid = person.uid AND party.transfer_uid = ?
         WHERE contact.sync_out = 1 AND contact.trust != 'blocked'
         ORDER BY person.head, person.uid",
    )
    .bind(transfer_uid)
    .fetch_all(&store.pool)
    .await?;
    let mut recipients = Vec::with_capacity(policies.len());
    let mut receipt_values = Vec::new();
    for policy in policies {
        let person = store::records::get(&store.pool, &policy.recipient_person_uid).await?;
        let organ = store::records::get(&store.pool, &policy.recipient_organ_uid).await?;
        let latest = store::sqlx::query_as::<_, (
            String, i64, Option<String>, Option<String>, Option<String>, Option<i64>, String,
        )>(
            "SELECT status, attempts, last_error, last_attempt_at, sent_at, acknowledged_cursor, envelope_uid
             FROM transfer_delivery_outbox WHERE delivery_uid = ? ORDER BY cursor DESC LIMIT 1",
        )
        .bind(&policy.uid)
        .fetch_optional(&store.pool)
        .await?;
        let receipts = store::sqlx::query_as::<_, (String, String, i64, String, String)>(
            "SELECT uid, kind, cursor, actor_organ_uid, created_at
             FROM transfer_delivery_receipt WHERE delivery_uid = ? ORDER BY created_at, uid",
        )
        .bind(&policy.uid)
        .fetch_all(&store.pool)
        .await?;
        receipt_values.extend(receipts.into_iter().map(|row| {
            json!({
                "uid": row.0,
                "kind": row.1,
                "cursor": row.2,
                "organ": row.3,
                "at": row.4,
            })
        }));
        let active = policy.state == "active";
        let latest_status = latest
            .as_ref()
            .map(|row| row.0.as_str())
            .unwrap_or("not_queued");
        recipients.push(json!({
            "uid": policy.uid,
            "delivery_uid": policy.uid,
            "person": policy.recipient_person_uid,
            "person_head": person.as_ref().map(|record| record.head.as_str()),
            "person_slug": person.as_ref().and_then(|record| record.slug.as_deref()),
            "organ": policy.recipient_organ_uid,
            "organ_head": organ.as_ref().map(|record| record.head.as_str()),
            "organ_slug": organ.as_ref().and_then(|record| record.slug.as_deref()),
            "mode": policy.mode.as_str(),
            "state": policy.state,
            "revision": policy.revision,
            "delivery": {
                "status": latest_status,
                "attempts": latest.as_ref().map(|row| row.1).unwrap_or(0),
                "last_error": latest.as_ref().and_then(|row| row.2.as_deref()),
                "last_attempt_at": latest.as_ref().and_then(|row| row.3.as_deref()),
                "acknowledged_at": latest.as_ref().and_then(|row| row.4.as_deref()),
                "acknowledged_cursor": latest.as_ref().and_then(|row| row.5),
                "envelope": latest.as_ref().map(|row| row.6.as_str()),
            },
            "available_modes": ["hosted", "replicated"],
            "capabilities": {
                "set_mode": can_admin_delivery && active,
                "enqueue": can_admin_delivery && active,
                "retry": can_admin_delivery && active && latest_status == "failed",
                "revoke": can_admin_delivery && active,
            },
            "blocking_reasons": {
                "set_mode": if active { Vec::<&str>::new() } else { vec!["delivery_revoked"] },
                "enqueue": if active { Vec::<&str>::new() } else { vec!["delivery_revoked"] },
                "retry": if active && latest_status == "failed" { Vec::<&str>::new() } else { vec!["delivery_not_failed"] },
                "revoke": if active { Vec::<&str>::new() } else { vec!["delivery_revoked"] },
            },
            "action_payloads": if can_admin_delivery { json!({
                "set_mode": {
                    "action": "set-transfer-delivery-mode",
                    "transfer": transfer_uid,
                    "delivery": policy.uid,
                    "expected_revision": policy.revision,
                    "person": acting_person,
                    "request_id": Value::Null,
                    "mode": policy.mode.as_str(),
                },
                "enqueue": {
                    "action": "enqueue-transfer-delivery",
                    "transfer": transfer_uid,
                    "delivery": policy.uid,
                    "person": acting_person,
                    "request_id": Value::Null,
                },
                "retry": {
                    "action": "retry-transfer-delivery",
                    "transfer": transfer_uid,
                    "delivery": policy.uid,
                    "person": acting_person,
                    "request_id": Value::Null,
                },
                "revoke": {
                    "action": "revoke-transfer-delivery",
                    "transfer": transfer_uid,
                    "delivery": policy.uid,
                    "expected_revision": policy.revision,
                    "person": acting_person,
                    "request_id": Value::Null,
                },
            }) } else { json!({}) },
        }));
    }
    let configured_people = recipients
        .iter()
        .filter_map(|row| {
            row.get("person")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect::<HashSet<_>>();
    let eligible_recipients = eligible
        .into_iter()
        .filter(|row| !configured_people.contains(&row.0))
        .map(|row| {
            json!({
                "person": row.0,
                "person_head": row.1,
                "person_slug": row.2,
                "organ": row.3,
                "organ_head": row.4,
                "organ_slug": row.5,
                "contact_state": row.6,
                "default_mode": "hosted",
                "available_modes": ["hosted", "replicated"],
            })
        })
        .collect::<Vec<_>>();
    let settlement_handoffs = store::sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            String,
            f64,
            Option<String>,
            String,
            Option<String>,
            String,
        ),
    >(
        "SELECT h.uid, h.participant_person_uid, h.participant_organ_uid,
                h.occurrence_uid, d.canonical_quantity, d.canonical_unit_uid,
                h.state, h.attestation_uid, h.created_at
         FROM transfer_application_handoff h
         JOIN transfer_application_handoff_detail d ON d.handoff_uid = h.uid
         WHERE h.transfer_uid = ? ORDER BY h.created_at, h.uid",
    )
    .bind(transfer_uid)
    .fetch_all(&store.pool)
    .await?
    .into_iter()
    .map(|row| {
        json!({
            "uid": row.0,
            "person": row.1,
            "organ": row.2,
            "occurrence": row.3,
            "canonical_quantity": row.4,
            "canonical_unit": row.5,
            "state": row.6,
            "attestation": row.7,
            "at": row.8,
        })
    })
    .collect::<Vec<_>>();
    Ok(json!({
        "authority": {
            "role": "origin",
            "origin_organ": local_organ.uid,
            "local_organ": local_organ.uid,
            "canonical_writes": "local",
        },
        "revision": transfer_revision,
        "recipients": recipients,
        "eligible_recipients": eligible_recipients,
        "package_receipts": receipt_values,
        "application_handoffs": settlement_handoffs,
        "capabilities": { "configure_recipient": can_admin_delivery },
        "blocking_reasons": { "configure_recipient": if can_admin_delivery { Vec::<&str>::new() } else { vec!["delivery_admin_identity_required"] } },
        "action_payloads": if can_admin_delivery { json!({
            "configure_recipient": {
                "action": "configure-transfer-delivery",
                "transfer": transfer_uid,
                "recipient_person": Value::Null,
                "recipient_organ": Value::Null,
                "person": acting_person,
                "request_id": Value::Null,
                "mode": "hosted",
            },
        }) } else { json!({}) },
    }))
}

struct TransferOutput {
    uid: String,
    slug: Option<String>,
    head: String,
    revision: u64,
    status: String,
    value: Value,
}

fn phase6_descendants(uid: &str, children: &HashMap<String, Vec<String>>) -> Vec<String> {
    fn visit(
        uid: &str,
        children: &HashMap<String, Vec<String>>,
        seen: &mut HashSet<String>,
        out: &mut Vec<String>,
    ) {
        if !seen.insert(uid.to_string()) {
            return;
        }
        out.push(uid.to_string());
        if let Some(child_uids) = children.get(uid) {
            for child in child_uids {
                visit(child, children, seen, out);
            }
        }
    }
    let mut out = Vec::new();
    visit(uid, children, &mut HashSet::new(), &mut out);
    out
}

fn phase6_hierarchy_path(
    uid: &str,
    parents: &HashMap<String, Option<String>>,
    visible: &HashSet<String>,
) -> (String, Vec<String>, bool) {
    let mut reverse = Vec::new();
    let mut current = Some(uid);
    let mut seen = HashSet::new();
    let mut cycle = false;
    while let Some(node) = current {
        if !seen.insert(node.to_string()) {
            cycle = true;
            break;
        }
        reverse.push(node.to_string());
        current = parents
            .get(node)
            .and_then(|parent| parent.as_deref())
            .filter(|parent| visible.contains(*parent));
    }
    reverse.reverse();
    let root = reverse.first().cloned().unwrap_or_else(|| uid.to_string());
    (root, reverse, cycle)
}

fn phase6_rollup(
    uid: &str,
    values: &HashMap<String, Value>,
    children: &HashMap<String, Vec<String>>,
) -> (Vec<String>, Value, bool, Vec<String>) {
    let descendants = phase6_descendants(uid, children);
    let mut status_counts: std::collections::BTreeMap<String, usize> = Default::default();
    let mut resource_rollup: std::collections::BTreeMap<
        (Option<String>, Option<String>, Option<String>),
        (Option<String>, Option<String>, f64, f64, f64),
    > = Default::default();
    let mut ready = true;
    let mut blocked_transfers = Vec::new();
    for descendant in &descendants {
        let Some(value) = values.get(descendant) else {
            continue;
        };
        if let Some(status) = value.get("status").and_then(Value::as_str) {
            *status_counts.entry(status.to_string()).or_default() += 1;
        }
        let terminal = value
            .get("status")
            .and_then(Value::as_str)
            .is_some_and(|status| matches!(status, "settled" | "satiated" | "withdrawn"));
        let transfer_ready = terminal
            || value
                .pointer("/readiness/ready")
                .and_then(Value::as_bool)
                .unwrap_or(false);
        if !transfer_ready {
            ready = false;
            blocked_transfers.push(descendant.clone());
        }
        for (resource_index, resource) in value
            .pointer("/settlement_progress/by_resource")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .enumerate()
        {
            let concept = resource
                .get("concept")
                .and_then(Value::as_str)
                .map(str::to_string);
            let unit = resource
                .get("unit")
                .and_then(Value::as_str)
                .map(str::to_string);
            // Unknown resource identities stay separate; only canonical
            // (concept, unit) pairs may be summed across Transfers.
            let discriminator = (concept.is_none() || unit.is_none())
                .then(|| format!("{descendant}:{resource_index}"));
            let totals = resource_rollup
                .entry((concept, unit, discriminator))
                .or_insert_with(|| {
                    (
                        resource
                            .get("concept_name")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                        resource
                            .get("unit_name")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                        0.0,
                        0.0,
                        0.0,
                    )
                });
            totals.2 += resource
                .get("canonical_quantity")
                .and_then(Value::as_f64)
                .unwrap_or(0.0);
            totals.3 += resource
                .get("settled_quantity")
                .and_then(Value::as_f64)
                .unwrap_or(0.0);
            totals.4 += resource
                .get("remaining_quantity")
                .and_then(Value::as_f64)
                .unwrap_or(0.0);
        }
    }
    blocked_transfers.sort();
    let remaining_by_resource = resource_rollup
        .into_iter()
        .map(
            |((concept, unit, _), (concept_name, unit_name, canonical, settled, remaining))| {
                json!({
                    "concept": concept,
                    "concept_name": concept_name,
                    "unit": unit,
                    "unit_name": unit_name,
                    "canonical_quantity": canonical,
                    "settled_quantity": settled,
                    "remaining_quantity": remaining,
                })
            },
        )
        .collect::<Vec<_>>();
    (
        descendants,
        json!({
            "status_counts": status_counts,
            "ready": ready,
            "remaining_by_resource": remaining_by_resource,
        }),
        ready,
        blocked_transfers,
    )
}

fn attach_phase6_transfer_projection(rows: &mut [TransferOutput]) {
    let visible = rows
        .iter()
        .map(|row| row.uid.clone())
        .collect::<HashSet<_>>();
    let values = rows
        .iter()
        .map(|row| (row.uid.clone(), row.value.clone()))
        .collect::<HashMap<_, _>>();
    let parents = rows
        .iter()
        .map(|row| {
            (
                row.uid.clone(),
                row.value
                    .get("parent")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            )
        })
        .collect::<HashMap<_, _>>();
    let mut children: HashMap<String, Vec<String>> = HashMap::new();
    for (uid, parent) in &parents {
        if let Some(parent) = parent.as_deref().filter(|parent| visible.contains(*parent)) {
            children
                .entry(parent.to_string())
                .or_default()
                .push(uid.clone());
        }
    }
    for children in children.values_mut() {
        children.sort();
    }

    let promise_owners = values
        .iter()
        .flat_map(|(transfer, value)| {
            value
                .get("promises")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(move |promise| {
                    promise
                        .get("uid")
                        .and_then(Value::as_str)
                        .map(|promise| (promise.to_string(), transfer.clone()))
                })
        })
        .collect::<HashMap<_, _>>();
    let mut outgoing: HashMap<String, HashSet<String>> = HashMap::new();
    let mut indegree = visible
        .iter()
        .map(|uid| (uid.clone(), 0usize))
        .collect::<HashMap<_, _>>();
    let mut add_edge = |upstream: &str, downstream: &str| {
        if upstream == downstream || !visible.contains(upstream) || !visible.contains(downstream) {
            return;
        }
        if outgoing
            .entry(upstream.to_string())
            .or_default()
            .insert(downstream.to_string())
        {
            *indegree.entry(downstream.to_string()).or_default() += 1;
        }
    };
    for (uid, value) in &values {
        for dependency in value
            .get("dependencies")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let upstream = dependency.get("upstream").and_then(Value::as_str);
            let upstream_kind = dependency.get("upstream_kind").and_then(Value::as_str);
            let upstream_transfer = match (upstream_kind, upstream) {
                (Some("transfer"), Some(upstream)) => Some(upstream),
                (Some("promise"), Some(upstream)) => {
                    promise_owners.get(upstream).map(String::as_str)
                }
                _ => None,
            };
            if let Some(upstream) = upstream_transfer {
                add_edge(upstream, uid);
            }
        }
    }
    let mut ready = indegree
        .iter()
        .filter(|(_, degree)| **degree == 0)
        .map(|(uid, _)| uid.clone())
        .collect::<std::collections::BTreeSet<_>>();
    let mut dependency_order = HashMap::new();
    while let Some(uid) = ready.pop_first() {
        let order = dependency_order.len();
        dependency_order.insert(uid.clone(), order);
        let mut next = outgoing
            .get(&uid)
            .into_iter()
            .flatten()
            .cloned()
            .collect::<Vec<_>>();
        next.sort();
        for downstream in next {
            if let Some(degree) = indegree.get_mut(&downstream) {
                *degree = degree.saturating_sub(1);
                if *degree == 0 {
                    ready.insert(downstream);
                }
            }
        }
    }
    let cycle_nodes = visible
        .iter()
        .filter(|uid| !dependency_order.contains_key(*uid))
        .cloned()
        .collect::<HashSet<_>>();
    let mut ordered_transfers = dependency_order
        .iter()
        .map(|(uid, order)| (*order, uid.clone()))
        .collect::<Vec<_>>();
    ordered_transfers.sort();
    let dependency_ordered_uids = ordered_transfers
        .into_iter()
        .map(|(_, uid)| uid)
        .collect::<Vec<_>>();

    let mut first_completes_groups: HashMap<String, Vec<String>> = HashMap::new();
    for (uid, value) in &values {
        if value.get("satiation").and_then(Value::as_str) == Some("first_completes") {
            if let Some(source) = value.get("source").and_then(Value::as_str) {
                first_completes_groups
                    .entry(source.to_string())
                    .or_default()
                    .push(uid.clone());
            }
        }
    }
    for members in first_completes_groups.values_mut() {
        members.sort();
    }

    for row in rows {
        let (descendants, mut rollup, subtree_ready, blocked_transfers) =
            phase6_rollup(&row.uid, &values, &children);
        rollup["transfers"] = json!(descendants.len());
        let direct_children = children.get(&row.uid).cloned().unwrap_or_default();
        let branches = direct_children
            .iter()
            .filter_map(|child| {
                values.get(child).map(|value| {
                    let (branch_descendants, mut branch_rollup, branch_ready, branch_blockers) =
                        phase6_rollup(child, &values, &children);
                    branch_rollup["transfers"] = json!(branch_descendants.len());
                    json!({
                        "uid": child,
                        "status": value.get("status"),
                        "revision": value.get("revision"),
                        "ready": branch_ready,
                        "blockers": branch_blockers,
                        "remaining_by_resource": branch_rollup.get("remaining_by_resource"),
                        "rollup": branch_rollup,
                    })
                })
            })
            .collect::<Vec<_>>();
        let (root, path, parent_cycle) = phase6_hierarchy_path(&row.uid, &parents, &visible);
        let dependency_cycle = cycle_nodes.contains(&row.uid);
        let self_terminal = row
            .value
            .get("status")
            .and_then(Value::as_str)
            .is_some_and(|status| matches!(status, "settled" | "satiated" | "withdrawn"));
        let self_ready = self_terminal
            || row
                .value
                .pointer("/readiness/ready")
                .and_then(Value::as_bool)
                .unwrap_or(false);
        let dependency_blockers = row
            .value
            .get("dependencies")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|dependency| {
                dependency.get("satisfied").and_then(Value::as_bool) == Some(false)
            })
            .map(|dependency| {
                json!({
                    "code": "dependency_unsatisfied",
                    "dependency": dependency.get("uid"),
                    "scope": dependency.get("scope"),
                    "promise": dependency.get("promise"),
                    "upstream_kind": dependency.get("upstream_kind"),
                    "upstream": dependency.get("upstream"),
                    "required_state": dependency.get("required_state"),
                    "actual_states": dependency.get("actual_states"),
                })
            })
            .collect::<Vec<_>>();
        let mut phase6_blockers = dependency_blockers;
        if dependency_cycle {
            phase6_blockers.push(json!({ "code": "dependency_cycle" }));
        }
        if parent_cycle {
            phase6_blockers.push(json!({ "code": "hierarchy_cycle" }));
        }
        if !subtree_ready {
            phase6_blockers.push(json!({
                "code": "descendant_not_ready",
                "transfers": blocked_transfers,
            }));
        }

        let visible_first_completes =
            row.value
                .get("source")
                .and_then(Value::as_str)
                .and_then(|source| {
                    first_completes_groups.get(source).map(|members| {
                        let winners = members
                            .iter()
                            .filter(|member| {
                                values
                                    .get(*member)
                                    .and_then(|value| value.get("status"))
                                    .and_then(Value::as_str)
                                    == Some("settled")
                            })
                            .cloned()
                            .collect::<Vec<_>>();
                        let (state, winner) = match winners.as_slice() {
                            [] => ("pending", None),
                            [winner] => ("completed", Some(winner.as_str())),
                            _ => ("conflict", None),
                        };
                        json!({
                            "policy": "first_completes",
                            "source": source,
                            "scope": "visible_projection",
                            "state": state,
                            "winner": winner,
                            "members": members,
                            "role": if winner == Some(row.uid.as_str()) {
                                "winner"
                            } else if state == "completed" {
                                "satiated"
                            } else {
                                "candidate"
                            },
                        })
                    })
                });
        let first_completes = row
            .value
            .get("first_completes_evidence")
            .filter(|evidence| !evidence.is_null())
            .map(|evidence| {
                let source = evidence.pointer("/result/source").and_then(Value::as_str);
                let winner = evidence.pointer("/result/winner").and_then(Value::as_str);
                json!({
                    "policy": "first_completes",
                    "source": source,
                    "scope": "visible_projection_with_authoritative_result",
                    "state": evidence.get("state"),
                    "winner": winner,
                    "members": source
                        .and_then(|source| first_completes_groups.get(source))
                        .cloned()
                        .unwrap_or_default(),
                    "role": if winner == Some(row.uid.as_str()) {
                        "winner"
                    } else {
                        "loser"
                    },
                    "evidence": evidence,
                })
            })
            .or(visible_first_completes);
        if let Some(object) = row.value.as_object_mut() {
            object.insert(
                "hierarchy".into(),
                json!({
                    "scope": "visible_projection",
                    "root": root,
                    "parent": parents.get(&row.uid).and_then(|parent| parent.as_deref()),
                    "parent_visible": parents.get(&row.uid)
                        .and_then(|parent| parent.as_deref())
                        .is_none_or(|parent| visible.contains(parent)),
                    "children": direct_children,
                    "depth": path.len().saturating_sub(1),
                    "path": path,
                    "leaf": children.get(&row.uid).is_none_or(Vec::is_empty),
                    "rollup": rollup,
                    "branches": branches,
                }),
            );
            let phase6_ready = self_ready && subtree_ready && phase6_blockers.is_empty();
            object.insert(
                "phase6".into(),
                json!({
                    "order": dependency_order.get(&row.uid),
                    "dependency_order": dependency_ordered_uids,
                    "dependency_cycle": dependency_cycle,
                    "hierarchy_cycle": parent_cycle,
                    "ready": phase6_ready,
                    "readiness": {
                        "ready": phase6_ready,
                        "blockers": phase6_blockers,
                    },
                    "blockers": phase6_blockers,
                }),
            );
            object.insert(
                "first_completes".into(),
                first_completes.unwrap_or(Value::Null),
            );
        }
    }
}

fn validate_transfer_order(order: &[Order]) -> Result<(), ProteinError> {
    for key in order {
        let field = match key {
            Order::Asc(field) | Order::Desc(field) => field.as_str(),
            Order::Topo(_) => {
                return Err(transfer_query_error(
                    "protein_transfer_unsupported_order",
                    "topo ordering is not defined for Transfers",
                ));
            }
        };
        if !matches!(field, "uid" | "slug" | "head" | "revision" | "status") {
            return Err(transfer_query_error(
                "protein_transfer_unsupported_order",
                field,
            ));
        }
    }
    Ok(())
}

fn compare_transfer_field(
    left: &TransferOutput,
    right: &TransferOutput,
    field: &str,
) -> std::cmp::Ordering {
    match field {
        "uid" => left.uid.cmp(&right.uid),
        "slug" => left.slug.cmp(&right.slug),
        "head" => left.head.cmp(&right.head),
        "revision" => left.revision.cmp(&right.revision),
        "status" => left.status.cmp(&right.status),
        _ => std::cmp::Ordering::Equal,
    }
}

fn order_transfers(rows: &mut [TransferOutput], order: &[Order]) {
    rows.sort_by(|left, right| {
        for key in order {
            let (field, descending) = match key {
                Order::Asc(field) => (field.as_str(), false),
                Order::Desc(field) => (field.as_str(), true),
                Order::Topo(_) => continue,
            };
            let compared = compare_transfer_field(left, right, field);
            if !compared.is_eq() {
                return if descending {
                    compared.reverse()
                } else {
                    compared
                };
            }
        }
        // A uid tie-break makes both explicit ordering and the no-order
        // default deterministic across SQLite query plans and Cells.
        left.uid.cmp(&right.uid)
    });
}

async fn execute_transfers(
    store: &Store,
    protein: &Protein,
    visible: Option<&HashSet<String>>,
    subject: Option<&str>,
    installed_signer_actor: Option<&str>,
    viewer_override: Option<TransferViewer>,
) -> Result<Vec<Value>, ProteinError> {
    let filter_context = TransferPredicateCtx::prepare(store, &protein.filter).await?;
    validate_transfer_order(&protein.order)?;
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
    let viewer = match viewer_override {
        Some(viewer) => viewer,
        None => TransferViewer::resolve(store, subject).await?,
    };
    let mut create_blockers = viewer.create_blockers();
    let create_person = if viewer.local {
        installed_signer_actor
    } else {
        viewer.person.as_deref()
    };
    if create_person.is_none() || installed_signer_actor != create_person {
        create_blockers.push("missing_person_signer");
    }
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
    let mut transfer_rows = Vec::new();
    for t in store::transfers::list_all(&store.pool).await? {
        let uid = &t.transfer.record_uid;
        let creator = store::facts::creator_uid(&store.pool, uid).await?;
        let creator_person = store::transfers::creator_party_actor(&store.pool, uid).await?;
        let invitations = store::transfers::invitations_for_transfer(&store.pool, uid).await?;
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
            let viewer_was_addressed = viewer.person.as_deref().is_some_and(|person| {
                invitations
                    .iter()
                    .any(|invitation| invitation.addressed_person_uid == person)
            });
            // Hidden remains the default, but creator and named parties are
            // intrinsic recipients of the commitment. An addressed Person
            // also retains access to the invitation decision and its history,
            // without being promoted to a participant before acceptance.
            if !viewer_created && !viewer_participates && !viewer_was_addressed {
                continue;
            }
        }
        let promises = store::transfers::promises_of(&store.pool, uid).await?;
        let correction_lineage =
            store::transfers::correction_links_for_transfer(&store.pool, uid).await?;
        let promise_successors =
            store::transfers::promise_successors_for_transfer(&store.pool, uid).await?;
        let mut open_claim_pairs_by_source = HashMap::new();
        for promise in &promises {
            let pairs =
                store::transfers::open_claim_pairs_for_source(&store.pool, &promise.uid).await?;
            if !pairs.is_empty() {
                open_claim_pairs_by_source.insert(promise.uid.clone(), pairs);
            }
        }
        let occurrences = store::transfers::occurrences_of(&store.pool, uid).await?;
        let mut settlement_progress_by_occurrence = HashMap::new();
        for occurrence in &occurrences {
            if let Some(progress) =
                store::transfers::occurrence_settlement_progress(&store.pool, &occurrence.uid)
                    .await?
            {
                settlement_progress_by_occurrence.insert(occurrence.uid.clone(), progress);
            }
        }
        let occurred_promises = occurrences
            .iter()
            .map(|occurrence| occurrence.promise_uid.as_str())
            .collect::<HashSet<_>>();
        let mut availability_by_record = HashMap::new();
        for record_uid in promises
            .iter()
            .filter_map(|promise| promise.record_uid.as_deref())
        {
            if availability_by_record.contains_key(record_uid) {
                continue;
            }
            let Some(record) = records_by_uid.get(record_uid) else {
                continue;
            };
            availability_by_record.insert(
                record_uid.to_string(),
                derive_record_availability(
                    store,
                    record_uid,
                    record.quantity_f64(),
                    record.unit_uid.as_deref(),
                )
                .await?,
            );
        }
        let parties = store::transfers::party_levels(&store.pool, uid).await?;
        let readiness_input = store::transfers::agreement_readiness_input(&store.pool, uid).await?;
        let agreement_events = store::transfers::agreement_events(&store.pool, uid, None).await?;
        let (agreement_readiness, dependency_status) =
            derive_agreement_readiness(store, &readiness_input).await?;
        let occurrence_roles = readiness_input
            .promises
            .iter()
            .map(|promise| {
                (
                    promise.uid.clone(),
                    transfer_occurrence_roles(&readiness_input, &promise.uid),
                )
            })
            .collect::<HashMap<_, _>>();
        let levels: Vec<i64> = parties.iter().map(|(_, _, level)| *level).collect();
        let states: Vec<nucleus::PromiseState> = promises.iter().map(|p| p.state).collect();
        let mut status = derive_transfer_status(
            t.transfer.active,
            &t.transfer.agreement_type,
            t.transfer.agreement_pct,
            &levels,
            &states,
        );
        let agreement_states = states
            .iter()
            .filter(|state| {
                !matches!(
                    state,
                    nucleus::PromiseState::Open | nucleus::PromiseState::Withdrawn
                )
            })
            .collect::<Vec<_>>();
        let all_agreed = !agreement_states.is_empty()
            && agreement_states.iter().all(|state| {
                matches!(
                    state,
                    nucleus::PromiseState::Agreed | nucleus::PromiseState::Kept
                )
            });
        if status == "agreed" && !agreement_readiness.ready {
            status = "proposed";
        } else if status == "proposed" && agreement_readiness.ready && all_agreed {
            status = "agreed";
        }
        if occurrences
            .iter()
            .any(|occurrence| occurrence.system_disputed)
        {
            status = "system_disputed";
        } else if occurrences.iter().any(|occurrence| occurrence.disputed) {
            status = "disputed";
        } else if settlement_progress_by_occurrence
            .values()
            .any(|progress| progress.partially_settled)
        {
            status = "partially_settled";
        }
        let source_group_state =
            store::transfers::source_group_state_for_transfer(&store.pool, uid).await?;
        if source_group_state
            .as_ref()
            .is_some_and(|state| state.satiated)
        {
            status = "satiated";
        }
        let current_revision_fact = if t.transfer.revision > 0 {
            store::transfers::revision_fact(&store.pool, uid, t.transfer.revision as u64).await?
        } else {
            None
        };
        let revision_signed = match current_revision_fact.as_ref() {
            Some(fact) => {
                fact.signature.is_some()
                    || store::action_intents::fact_has_committed_intent(&store.pool, &fact.uid)
                        .await?
            }
            None => false,
        };
        let current_revision_evidence = current_revision_fact
            .as_ref()
            .and_then(parse_transfer_revision_evidence);
        let revision_promises = current_revision_evidence
            .as_ref()
            .map(|evidence| evidence.terms.promises.as_slice());
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
        let viewer_party = viewer.person.as_deref().and_then(|person| {
            parties
                .iter()
                .find(|(_, actor, _)| actor == person)
                .map(|(party, _, _)| party.clone())
        });
        let is_creator = viewer.local
            || viewer.person.as_deref() == creator_person.as_deref()
            || viewer
                .subject
                .as_deref()
                .is_some_and(|subject| creator.as_deref() == Some(subject));
        let is_participant = viewer_party.is_some();
        let is_invitee = viewer.person.as_deref().is_some_and(|person| {
            invitations.iter().any(|invitation| {
                invitation.addressed_person_uid == person
                    && invitation.status == store::transfers::TransferInvitationStatus::Pending
                    && !invitation.expires_at.as_deref().is_some_and(|value| {
                        chrono::DateTime::parse_from_rfc3339(value).is_ok_and(|expiry| {
                            expiry.with_timezone(&chrono::Utc) <= chrono::Utc::now()
                        })
                    })
            })
        });
        let viewer_person = viewer.person.as_deref().or(installed_signer_actor);
        let inbox_mine = viewer_person.is_some_and(|person| {
            creator_person.as_deref() == Some(person)
                || parties.iter().any(|(_, actor, _)| actor == person)
        });
        let recipient_details_authorized = viewer.local || inbox_mine;
        let inbox_invited = viewer_person.is_some_and(|person| {
            invitations.iter().any(|invitation| {
                invitation.addressed_person_uid == person
                    && invitation.status == store::transfers::TransferInvitationStatus::Pending
            })
        });
        let viewer_agreement_pending = viewer_person.is_some_and(|person| {
            parties
                .iter()
                .any(|(_, actor, level)| actor == person && *level < 2)
        });
        let viewer_claim_pending = viewer_person.is_some_and(|person| {
            occurrences.iter().any(|occurrence| {
                (occurrence.giver_person_uid == person && !occurrence.delivery_claimed)
                    || (occurrence.receiver_person_uid == person && !occurrence.receipt_claimed)
            })
        });
        let other_agreement_pending = viewer_person.is_some_and(|person| {
            parties
                .iter()
                .any(|(_, actor, level)| actor != person && *level < 2)
        });
        let other_invitation_pending = viewer_person.is_some_and(|person| {
            invitations.iter().any(|invitation| {
                invitation.addressed_person_uid != person
                    && invitation.status == store::transfers::TransferInvitationStatus::Pending
            })
        });
        let other_claim_pending = viewer_person.is_some_and(|person| {
            occurrences.iter().any(|occurrence| {
                (occurrence.giver_person_uid != person && !occurrence.delivery_claimed)
                    || (occurrence.receiver_person_uid != person && !occurrence.receipt_claimed)
            })
        });
        let expired = promises.iter().any(|promise| {
            matches!(
                promise.state,
                nucleus::PromiseState::Open
                    | nucleus::PromiseState::Proposed
                    | nucleus::PromiseState::Agreed
                    | nucleus::PromiseState::Active
            ) && promise.window_end.as_deref().is_some_and(|window_end| {
                chrono::DateTime::parse_from_rfc3339(window_end).is_ok_and(|window_end| {
                    window_end.with_timezone(&chrono::Utc) <= chrono::Utc::now()
                })
            })
        });
        let invitation_cancelled = !invitations.is_empty()
            && invitations.iter().all(|invitation| {
                matches!(
                    invitation.status,
                    store::transfers::TransferInvitationStatus::Rejected
                        | store::transfers::TransferInvitationStatus::Withdrawn
                        | store::transfers::TransferInvitationStatus::Expired
                )
            })
            && parties.len() <= 1
            && occurrences.is_empty();
        let all_withdrawn = !states.is_empty()
            && states
                .iter()
                .all(|state| *state == nucleus::PromiseState::Withdrawn);
        let structurally_terminal = expired
            || invitation_cancelled
            || all_withdrawn
            || states
                .iter()
                .any(|state| *state == nucleus::PromiseState::Broken)
            || source_group_state
                .as_ref()
                .is_some_and(|group| group.satiated)
            || (!occurrences.is_empty()
                && settlement_progress_by_occurrence
                    .values()
                    .all(|progress| progress.settled));
        let awaiting_me = !structurally_terminal
            && (inbox_invited
                || (inbox_mine && (viewer_agreement_pending || viewer_claim_pending)));
        let awaiting_others = !structurally_terminal
            && inbox_mine
            && (other_invitation_pending || other_agreement_pending || other_claim_pending);
        let primary_status = derive_transfer_primary_status(
            t.transfer.revision as u64,
            t.transfer.active,
            &states,
            &occurrences,
            &settlement_progress_by_occurrence,
            source_group_state.as_ref(),
            agreement_readiness.ready,
            expired,
            invitation_cancelled || all_withdrawn,
            awaiting_me,
            awaiting_others,
        );
        let coalition_members = readiness_input.coalition.as_ref().map(|coalition| {
            coalition
                .party_uids
                .iter()
                .map(String::as_str)
                .collect::<HashSet<_>>()
        });
        let agreement_candidates = readiness_input
            .parties
            .iter()
            .filter(|party| {
                viewer.local || viewer.person.as_deref() == Some(party.person_uid.as_str())
            })
            .collect::<Vec<_>>();
        let review_options = agreement_candidates
            .iter()
            .filter(|party| party.level == 0)
            .map(|party| {
                viewer.agreement_transition_blockers(
                    &party.person_uid,
                    &party.party_uid,
                    party.level,
                    1,
                    readiness_input.revision,
                    revision_signed,
                    coalition_members.as_ref(),
                    installed_signer_actor == Some(party.person_uid.as_str()),
                )
            })
            .collect::<Vec<_>>();
        let review_blockers = if review_options.iter().any(Vec::is_empty) {
            Vec::new()
        } else {
            review_options
                .into_iter()
                .next()
                .unwrap_or_else(|| vec!["no_party_can_enter_checked_level"])
        };
        let commit_options = agreement_candidates
            .iter()
            .filter(|party| party.level == 1)
            .map(|party| {
                viewer.agreement_transition_blockers(
                    &party.person_uid,
                    &party.party_uid,
                    party.level,
                    2,
                    readiness_input.revision,
                    revision_signed,
                    coalition_members.as_ref(),
                    installed_signer_actor == Some(party.person_uid.as_str()),
                )
            })
            .collect::<Vec<_>>();
        let commit_blockers = if commit_options.iter().any(Vec::is_empty) {
            Vec::new()
        } else {
            commit_options
                .into_iter()
                .next()
                .unwrap_or_else(|| vec!["no_party_can_enter_agreed_level"])
        };
        let back_options = agreement_candidates
            .iter()
            .filter(|party| party.level > 0)
            .map(|party| {
                viewer.agreement_transition_blockers(
                    &party.person_uid,
                    &party.party_uid,
                    party.level,
                    party.level - 1,
                    readiness_input.revision,
                    revision_signed,
                    coalition_members.as_ref(),
                    installed_signer_actor == Some(party.person_uid.as_str()),
                )
            })
            .collect::<Vec<_>>();
        let back_blockers = if back_options.iter().any(Vec::is_empty) {
            Vec::new()
        } else {
            back_options
                .into_iter()
                .next()
                .unwrap_or_else(|| vec!["no_party_can_move_agreement_back"])
        };
        let can_review = review_blockers.is_empty();
        let can_commit = commit_blockers.is_empty();
        let can_move_agreement_back = back_blockers.is_empty();
        let mut viewer_roles = HashSet::new();
        if viewer.local {
            viewer_roles.insert("local");
        }
        if is_creator {
            viewer_roles.insert("creator");
        }
        if is_participant {
            viewer_roles.insert("participant");
        }
        if is_invitee {
            viewer_roles.insert("invitee");
        }
        if viewer_roles.is_empty() {
            viewer_roles.insert("observer");
        }
        if !filter_context.matches(
            &TransferPredicateRow {
                uid,
                slug: t.slug.as_deref(),
                revision: t.transfer.revision as u64,
                status,
                viewer_roles: &viewer_roles,
                invitations: &invitations,
                parties: &parties,
                promises: &promises,
                revision_promises,
                records_by_uid: &records_by_uid,
            },
            &protein.filter,
        ) {
            continue;
        }
        let draft_terms_editable = states.iter().all(|state| {
            matches!(
                state,
                nucleus::PromiseState::Open
                    | nucleus::PromiseState::Proposed
                    | nucleus::PromiseState::Agreed
                    | nucleus::PromiseState::Withdrawn
            )
        });
        let mut edit_blockers = viewer.draft_edit_blockers(
            is_creator,
            is_participant,
            t.transfer.revision as u64,
            draft_terms_editable,
        );
        if installed_signer_actor != creator_person.as_deref() {
            edit_blockers.push("missing_person_signer");
        }
        let mut adopt_blockers = viewer.draft_adopt_blockers(
            is_creator,
            is_participant,
            t.transfer.revision as u64,
            draft_terms_editable,
        );
        if installed_signer_actor != creator_person.as_deref() {
            adopt_blockers.push("missing_person_signer");
        }
        let can_edit = edit_blockers.is_empty();
        let can_adopt = adopt_blockers.is_empty();
        let mut address_invitation_blockers = viewer.update_identity_blockers();
        if !is_creator {
            address_invitation_blockers.push("not_transfer_creator");
        }
        if t.transfer.revision == 0 {
            address_invitation_blockers.push("legacy_transfer_requires_adoption");
        }
        if !draft_terms_editable {
            address_invitation_blockers.push("transfer_terms_are_no_longer_draft_editable");
        }
        if installed_signer_actor != creator_person.as_deref() {
            address_invitation_blockers.push("missing_person_signer");
        }
        let can_address_invitation = address_invitation_blockers.is_empty();
        let mut counteroffer_blockers = viewer.update_identity_blockers();
        if !viewer.local && !is_participant {
            counteroffer_blockers.push("not_transfer_participant");
        }
        if t.transfer.revision == 0 {
            counteroffer_blockers.push("legacy_transfer_requires_adoption");
        }
        if !draft_terms_editable {
            counteroffer_blockers.push("transfer_terms_are_no_longer_draft_editable");
        }
        let counteroffer_person = if viewer.local {
            installed_signer_actor
        } else {
            viewer.person.as_deref()
        };
        if counteroffer_person.is_none()
            || installed_signer_actor != counteroffer_person
            || !counteroffer_person
                .is_some_and(|person| parties.iter().any(|(_, actor, _)| actor.as_str() == person))
        {
            counteroffer_blockers.push("missing_person_signer");
        }
        let can_counteroffer = counteroffer_blockers.is_empty();
        let mut claim_open_blockers = viewer.update_identity_blockers();
        let claim_person = if viewer.local {
            installed_signer_actor
        } else {
            viewer.person.as_deref()
        };
        let open_promises = promises
            .iter()
            .filter(|promise| promise.state == nucleus::PromiseState::Open)
            .collect::<Vec<_>>();
        if open_promises.is_empty() {
            claim_open_blockers.push("no_open_promise");
        } else if claim_person.is_some_and(|claimant| {
            !open_promises.iter().any(|promise| {
                promise
                    .party_uid
                    .as_deref()
                    .is_some_and(|proposer| proposer != claimant)
            })
        }) {
            claim_open_blockers.push("no_counterparty_open_promise");
        }
        if !viewer.local && is_invitee {
            claim_open_blockers.push("invitation_acceptance_required");
        } else if !viewer.local && !is_participant && t.transfer.visibility != "public" {
            claim_open_blockers.push("open_claim_requires_public_visibility");
        }
        if claim_person.is_none() || installed_signer_actor != claim_person {
            claim_open_blockers.push("missing_person_signer");
        }
        let can_claim_open = claim_open_blockers.is_empty();
        let can_activate_occurrence = readiness_input.promises.iter().any(|promise| {
            let viewer_owns =
                viewer.local || viewer.person.as_deref() == promise.person_uid.as_deref();
            viewer_owns
                && promise.state == "agreed"
                && promise
                    .person_uid
                    .as_deref()
                    .is_some_and(|person| installed_signer_actor == Some(person))
                && agreement_readiness
                    .promises
                    .get(&promise.uid)
                    .is_some_and(|readiness| readiness.ready)
                && occurrence_roles
                    .get(&promise.uid)
                    .is_some_and(Result::is_ok)
                && !occurred_promises.contains(promise.uid.as_str())
        });
        let activate_occurrence_blockers = if can_activate_occurrence {
            Vec::new()
        } else {
            vec!["no_policy_ready_occurrence"]
        };
        let revision_evidence = transfer_revision_projection(
            store,
            uid,
            t.transfer.revision as u64,
            recipient_details_authorized,
        )
        .await?;
        let negotiation_write_blockers =
            viewer.negotiation_write_blockers(is_creator, is_participant, is_invitee);
        let can_write_negotiation = negotiation_write_blockers.is_empty();
        let mut threads = threads_for_record(store, uid, 100).await?;
        for thread in &mut threads {
            if let Some(object) = thread.as_object_mut() {
                object.insert(
                    "capabilities".into(),
                    json!({ "create_message": can_write_negotiation }),
                );
                object.insert(
                    "blocking_reasons".into(),
                    json!({ "create_message": negotiation_write_blockers }),
                );
            }
        }
        let mut invitation_history = HashMap::new();
        for invitation in &invitations {
            let events = store::transfers::invitation_events(&store.pool, &invitation.uid).await?;
            let mut attempts: std::collections::BTreeMap<u64, Vec<Value>> = Default::default();
            for event in events {
                attempts.entry(event.attempt).or_default().push(json!({
                    "uid": event.uid,
                    "kind": event.kind,
                    "actor_person": event.actor_uid,
                    "revision": event.revision,
                    "fact": event.fact_uid,
                    "request_id": event.idempotency_key,
                    "at": event.created_at,
                }));
            }
            invitation_history.insert(
                invitation.uid.clone(),
                attempts
                    .into_iter()
                    .map(|(attempt, events)| {
                        json!({
                            "attempt": attempt,
                            "events": events,
                        })
                    })
                    .collect::<Vec<_>>(),
            );
        }
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
        let cell_application_formula =
            store::config::transfer_application_formula(&store.pool).await?;
        let viewer_has_private_transfer_data = viewer.local
            || viewer.person.as_deref().is_some_and(|person| {
                promises
                    .iter()
                    .any(|promise| promise.party_uid.as_deref() == Some(person))
            });
        let local_organ_uid = store::organs::local(&store.pool)
            .await?
            .map(|organ| organ.uid);
        let mut occurrence_projection = Vec::with_capacity(occurrences.len());
        let mut can_confirm_delivery = false;
        let mut can_confirm_receipt = false;
        let mut can_settle_occurrence = false;
        let mut can_dispute_occurrence = false;
        let mut can_retract_dispute = false;
        let mut can_compensate_settlement = false;
        let mut can_create_remainder_draft = false;
        let mut can_create_reversing_transfer = false;
        let mut private_settlement_slices = 0usize;
        let mut private_compensated_slices = 0usize;
        let mut settlement_by_resource: std::collections::BTreeMap<
            (Option<String>, Option<String>),
            (f64, f64, f64),
        > = Default::default();
        let mut settlement_status_counts: std::collections::BTreeMap<&str, usize> =
            Default::default();
        for occurrence in &occurrences {
            let origin_promise = promises
                .iter()
                .find(|promise| promise.uid == occurrence.promise_uid);
            let source_owner = origin_promise.and_then(|promise| promise.party_uid.as_deref());
            let origin_record = occurrence
                .record_uid
                .as_deref()
                .and_then(|record_uid| records_by_uid.get(record_uid));
            let viewer_owns_origin = viewer.local || viewer.person.as_deref() == source_owner;
            let viewer_is_source_owner = source_owner.is_some_and(|owner| {
                if viewer.local {
                    installed_signer_actor == Some(owner)
                } else {
                    viewer.person.as_deref() == Some(owner)
                }
            });
            let claim_events =
                store::transfers::occurrence_claim_events(&store.pool, &occurrence.uid).await?;
            let dispute_events =
                store::transfers::occurrence_dispute_events(&store.pool, &occurrence.uid).await?;
            let settlement_compensations =
                store::transfers::occurrence_settlement_compensations(&store.pool, &occurrence.uid)
                    .await?;
            let compensations_by_settlement = settlement_compensations
                .iter()
                .map(|compensation| (compensation.settlement_uid.as_str(), compensation))
                .collect::<HashMap<_, _>>();
            let viewer_is_giver = viewer.local
                || viewer.person.as_deref() == Some(occurrence.giver_person_uid.as_str());
            let viewer_is_receiver = viewer.local
                || viewer.person.as_deref() == Some(occurrence.receiver_person_uid.as_str());
            let mut delivery_blockers = viewer.update_identity_blockers();
            if !viewer_is_giver {
                delivery_blockers.push("not_occurrence_giver");
            }
            if installed_signer_actor != Some(occurrence.giver_person_uid.as_str()) {
                delivery_blockers.push("missing_person_signer");
            }
            let mut receipt_blockers = viewer.update_identity_blockers();
            if !viewer_is_receiver {
                receipt_blockers.push("not_occurrence_receiver");
            }
            if installed_signer_actor != Some(occurrence.receiver_person_uid.as_str()) {
                receipt_blockers.push("missing_person_signer");
            }
            can_confirm_delivery |= delivery_blockers.is_empty();
            can_confirm_receipt |= receipt_blockers.is_empty();
            let progress = settlement_progress_by_occurrence
                .get(&occurrence.uid)
                .expect("settlement progress exists for every stored occurrence");
            let resource_progress = settlement_by_resource
                .entry((occurrence.concept_uid.clone(), occurrence.unit_uid.clone()))
                .or_default();
            resource_progress.0 += progress.canonical_quantity;
            resource_progress.1 += progress.settled_quantity;
            resource_progress.2 += progress.remaining_quantity;
            let occurrence_status = if occurrence.system_disputed {
                "system_disputed"
            } else if occurrence.disputed {
                "disputed"
            } else if progress.settled {
                "settled"
            } else if progress.partially_settled {
                "partially_settled"
            } else if occurrence.delivery_claimed && occurrence.receipt_claimed {
                "confirmed"
            } else {
                "active"
            };
            *settlement_status_counts
                .entry(occurrence_status)
                .or_default() += 1;

            let correction_actor = if viewer.local {
                installed_signer_actor
            } else if viewer.person.as_deref() == installed_signer_actor {
                viewer.person.as_deref()
            } else {
                None
            };
            let effective_remainder_policy = match source_owner {
                Some(owner) => Some(
                    store::transfers::effective_occurrence_remainder_policy(
                        &store.pool,
                        &occurrence.uid,
                        owner,
                    )
                    .await?,
                ),
                None => None,
            };
            let mut remainder_draft_blockers = viewer.update_identity_blockers();
            if !viewer_is_source_owner {
                remainder_draft_blockers.push("not_source_promise_owner");
            }
            if correction_actor.is_none() || correction_actor != source_owner {
                remainder_draft_blockers.push("missing_person_signer");
            }
            if !progress.partially_settled || progress.remaining_quantity <= 0.0 {
                remainder_draft_blockers.push("occurrence_has_no_partial_remainder");
            }
            if effective_remainder_policy
                != Some(nucleus::transfer::TransferRemainderPolicy::LocalDraft)
            {
                remainder_draft_blockers.push("remainder_policy_not_local_draft");
            }
            let can_create_remainder = remainder_draft_blockers.is_empty();
            can_create_remainder_draft |= can_create_remainder;

            let mut reversing_transfer_blockers = viewer.update_identity_blockers();
            if correction_actor.is_none()
                || correction_actor != creator_person.as_deref()
                || installed_signer_actor != creator_person.as_deref()
            {
                reversing_transfer_blockers.push("not_transfer_creator_with_signer");
            }
            if !correction_actor.is_some_and(|actor| {
                actor == occurrence.giver_person_uid || actor == occurrence.receiver_person_uid
            }) {
                reversing_transfer_blockers.push("creator_not_occurrence_participant");
            }
            if source_owner != correction_actor && occurrence.concept_uid.is_none() {
                reversing_transfer_blockers.push("canonical_concept_required_for_reversal");
            }
            if !occurrence.quantity.is_finite() || occurrence.quantity <= 0.0 {
                reversing_transfer_blockers.push("occurrence_quantity_invalid");
            }
            let can_create_reversing = reversing_transfer_blockers.is_empty();
            can_create_reversing_transfer |= can_create_reversing;
            let correction_actor_is_participant = correction_actor.is_some_and(|person| {
                person == occurrence.giver_person_uid.as_str()
                    || person == occurrence.receiver_person_uid.as_str()
            });
            let actor_disputes = correction_actor.is_some_and(|person| {
                dispute_events
                    .iter()
                    .rev()
                    .find(|event| event.actor_person_uid == person)
                    .is_some_and(|event| event.disputed)
            });
            let mut dispute_base_blockers = viewer.update_identity_blockers();
            if !correction_actor_is_participant {
                dispute_base_blockers.push("not_occurrence_participant");
            }
            if correction_actor.is_none() || correction_actor != installed_signer_actor {
                dispute_base_blockers.push("missing_person_signer");
            }
            let mut dispute_blockers = dispute_base_blockers.clone();
            if actor_disputes {
                dispute_blockers.push("viewer_already_disputes_occurrence");
            }
            let mut retract_dispute_blockers = dispute_base_blockers;
            if !actor_disputes {
                retract_dispute_blockers.push("viewer_has_no_active_dispute");
            }
            let can_dispute = dispute_blockers.is_empty();
            let can_retract = retract_dispute_blockers.is_empty();
            can_dispute_occurrence |= can_dispute;
            can_retract_dispute |= can_retract;

            let mut settlement_blockers = viewer.update_identity_blockers();
            if !viewer_is_source_owner {
                settlement_blockers.push("not_source_promise_owner");
            }
            if source_owner.is_none()
                || installed_signer_actor.is_none()
                || installed_signer_actor != source_owner
            {
                settlement_blockers.push("missing_person_signer");
            }
            let concrete_record_matches = origin_promise
                .and_then(|promise| promise.record_uid.as_deref())
                .is_some_and(|record_uid| {
                    occurrence.record_uid.as_deref() == Some(record_uid) && origin_record.is_some()
                });
            if !concrete_record_matches {
                settlement_blockers.push("concrete_source_record_required");
            }
            if origin_record.is_some_and(|record| {
                record
                    .organ_uid
                    .as_deref()
                    .is_some_and(|organ| Some(organ) != local_organ_uid.as_deref())
            }) {
                settlement_blockers.push("source_record_not_local");
            }
            if !origin_promise.is_some_and(|promise| promise.state == nucleus::PromiseState::Active)
            {
                settlement_blockers.push("occurrence_not_active");
            }
            let source_direction_matches = origin_promise.is_some_and(|promise| {
                promise.delta.is_finite()
                    && ((promise.delta < 0.0
                        && source_owner == Some(occurrence.giver_person_uid.as_str()))
                        || (promise.delta > 0.0
                            && source_owner == Some(occurrence.receiver_person_uid.as_str())))
            });
            if !source_direction_matches {
                settlement_blockers.push("occurrence_source_direction_invalid");
            }
            if !occurrence.delivery_claimed || !occurrence.receipt_claimed {
                settlement_blockers.push("occurrence_confirmation_required");
            }
            if occurrence.disputed {
                settlement_blockers.push("occurrence_disputed");
            }
            if progress.remaining_quantity <= 0.0 {
                settlement_blockers.push("occurrence_already_settled");
            }

            let private_application = if viewer_owns_origin {
                match (origin_promise, source_owner) {
                    (Some(promise), Some(owner)) => {
                        let application = effective_settlement_application(
                            store,
                            occurrence,
                            promise,
                            owner,
                            &cell_application_formula,
                        )
                        .await?;
                        let applied_local_delta = progress
                            .slices
                            .iter()
                            .map(|slice| slice.local_delta)
                            .sum::<f64>();
                        Some((
                            application.formula,
                            application.formula_hash,
                            application.version,
                            application.source,
                            application.error,
                            applied_local_delta,
                        ))
                    }
                    _ => None,
                }
            } else {
                None
            };

            let mut settlement_preview = None;
            if viewer_is_source_owner {
                if let Some((formula, formula_hash, version, _, formula_error, prior_local)) =
                    private_application.as_ref()
                {
                    if formula_error.is_some() {
                        settlement_blockers.push("application_formula_invalid");
                    } else if progress.remaining_quantity > 0.0 {
                        let canonical_cumulative_after =
                            progress.settled_quantity + progress.remaining_quantity;
                        match evaluate_occurrence_application_formula(
                            formula,
                            canonical_cumulative_after,
                        ) {
                            Ok(local_cumulative_after) => {
                                let local_delta = local_cumulative_after - prior_local;
                                if local_delta.is_finite() {
                                    let remainder_policy =
                                        store::transfers::effective_occurrence_remainder_policy(
                                            &store.pool,
                                            &occurrence.uid,
                                            source_owner.expect("source owner checked above"),
                                        )
                                        .await?;
                                    settlement_preview = Some(json!({
                                        "transfer": uid,
                                        "occurrence": occurrence.uid,
                                        "promise": occurrence.promise_uid,
                                        "person": source_owner,
                                        "local_record": occurrence.record_uid,
                                        "canonical_quantity": progress.remaining_quantity,
                                        "remaining_quantity": progress.remaining_quantity,
                                        "settled_quantity": progress.settled_quantity,
                                        "local_delta": local_delta,
                                        "local_cumulative_after": local_cumulative_after,
                                        "application_formula_hash": formula_hash,
                                        "application_formula_version": version,
                                        "remainder_policy": remainder_policy.as_str(),
                                    }));
                                } else {
                                    settlement_blockers.push("application_formula_invalid");
                                }
                            }
                            Err(_) => settlement_blockers.push("application_formula_invalid"),
                        }
                    }
                } else {
                    settlement_blockers.push("application_formula_unavailable");
                }
            }
            let can_settle = settlement_blockers.is_empty() && settlement_preview.is_some();
            can_settle_occurrence |= can_settle;

            let mut compensation_base_blockers = viewer.update_identity_blockers();
            if !viewer_is_source_owner {
                compensation_base_blockers.push("not_source_promise_owner");
            }
            if source_owner.is_none() || installed_signer_actor != source_owner {
                compensation_base_blockers.push("missing_person_signer");
            }
            let settlement_history = progress
                .slices
                .iter()
                .map(|slice| {
                    let compensation = compensations_by_settlement
                        .get(slice.uid.as_str())
                        .copied();
                    let mut item = json!({
                        "uid": slice.uid,
                        "occurrence": slice.occurrence_uid,
                        "promise": slice.promise_uid,
                        "owner_person": slice.owner_person_uid,
                        "canonical_quantity": slice.canonical_quantity,
                        "canonical_unit": slice.canonical_unit_uid,
                        "cumulative_before": slice.cumulative_before,
                        "cumulative_after": slice.cumulative_after,
                        "remaining_after": slice.remaining_after,
                        "evidence_fact": slice.evidence_fact_uid,
                        "application_formula_hash": slice.application_formula_hash,
                        "application_formula_version": slice.application_formula_version,
                        "remainder_policy": slice.remainder_policy.as_str(),
                        "request_id": slice.idempotency_key,
                        "at": slice.created_at,
                    });
                    if viewer_owns_origin {
                        private_settlement_slices += 1;
                        let object = item
                            .as_object_mut()
                            .expect("settlement history item is an object");
                        object.insert("application_fact".into(), json!(slice.application_fact_uid));
                        object.insert("local_record".into(), json!(slice.local_record_uid));
                        object.insert("local_delta".into(), json!(slice.local_delta));
                        object.insert(
                            "local_cumulative_before".into(),
                            json!(slice.local_cumulative_before),
                        );
                        object.insert(
                            "local_cumulative_after".into(),
                            json!(slice.local_cumulative_after),
                        );
                        object.insert(
                            "application_formula".into(),
                            json!(slice.application_formula),
                        );
                        let mut compensate_blockers = compensation_base_blockers.clone();
                        if compensation.is_some() {
                            compensate_blockers.push("settlement_already_compensated");
                            private_compensated_slices += 1;
                        }
                        let can_compensate = compensate_blockers.is_empty();
                        can_compensate_settlement |= can_compensate;
                        object.insert(
                            "compensation_status".into(),
                            json!(if compensation.is_some() {
                                "compensated"
                            } else {
                                "applied"
                            }),
                        );
                        object.insert(
                            "compensation".into(),
                            compensation.map(|compensation| json!({
                                "uid": compensation.uid,
                                "fact": compensation.compensation_fact_uid,
                                "original_application_fact": compensation.original_application_fact_uid,
                                "local_record": compensation.local_record_uid,
                                "inverse_delta": compensation.inverse_delta,
                                "request_id": compensation.idempotency_key,
                                "at": compensation.created_at,
                            })).unwrap_or(Value::Null),
                        );
                        object.insert(
                            "capabilities".into(),
                            json!({ "compensate": can_compensate }),
                        );
                        object.insert(
                            "blocking_reasons".into(),
                            json!({ "compensate": compensate_blockers }),
                        );
                    }
                    item
                })
                .collect::<Vec<_>>();
            let mut participant_dispute_state = HashMap::new();
            for event in &dispute_events {
                participant_dispute_state.insert(event.actor_person_uid.as_str(), event.disputed);
            }
            let participant_disputed = participant_dispute_state.values().any(|asserted| *asserted);

            let mut projected_occurrence = json!({
                "uid": occurrence.uid,
                "exchange_path": occurrence.exchange_path_uid,
                "promise": occurrence.promise_uid,
                "opposite_promise": occurrence.opposite_promise_uid,
                "revision": occurrence.revision,
                "concept": occurrence.concept_uid,
                "concept_name": occurrence.concept_uid.as_deref()
                    .and_then(|concept| concept_names.get(concept)),
                "unit": occurrence.unit_uid,
                "unit_name": occurrence.unit_uid.as_deref()
                    .and_then(|unit| concept_names.get(unit)),
                "quantity": occurrence.quantity,
                "status": occurrence_status,
                "availability": if viewer_owns_origin {
                    occurrence.record_uid.as_deref()
                        .and_then(|record_uid| availability_by_record.get(record_uid))
                } else {
                    None
                },
                "reservation": origin_promise.map(|promise| json!({
                    "reserve_from": promise.reserve_from,
                    "source": "frozen_transfer_policy",
                })),
                "giver": occurrence.giver_person_uid,
                "receiver": occurrence.receiver_person_uid,
                "window_start": occurrence.window_start,
                "window_end": occurrence.window_end,
                "place": occurrence.location,
                "activation": {
                    "event": occurrence.activation_event_uid,
                    "fact": occurrence.activation_fact_uid,
                    "at": occurrence.created_at,
                },
                "disputed": occurrence.disputed,
                "dispute": {
                    "participant_disputed": participant_disputed,
                    "history": dispute_events.iter().map(|event| json!({
                        "uid": event.uid,
                        "disputed": event.disputed,
                        "person": event.actor_person_uid,
                        "fact": event.fact_uid,
                        "request_id": event.idempotency_key,
                        "at": event.created_at,
                    })).collect::<Vec<_>>(),
                },
                "system_dispute": {
                    "disputed": occurrence.system_disputed,
                    "fact": occurrence.system_dispute_fact_uid,
                    "at": occurrence.system_disputed_at,
                },
                "delivery": {
                    "claimed": occurrence.delivery_claimed,
                    "history": claim_events.iter()
                        .filter(|event| event.role == nucleus::transfer::OccurrenceClaimRole::Delivery)
                        .map(|event| json!({
                            "uid": event.uid,
                            "claimed": event.asserted,
                            "person": event.actor_person_uid,
                            "fact": event.fact_uid,
                            "request_id": event.idempotency_key,
                            "at": event.created_at,
                        })).collect::<Vec<_>>(),
                },
                "delivery_claimed": occurrence.delivery_claimed,
                "receipt": {
                    "claimed": occurrence.receipt_claimed,
                    "history": claim_events.iter()
                        .filter(|event| event.role == nucleus::transfer::OccurrenceClaimRole::Receipt)
                        .map(|event| json!({
                            "uid": event.uid,
                            "claimed": event.asserted,
                            "person": event.actor_person_uid,
                            "fact": event.fact_uid,
                            "request_id": event.idempotency_key,
                            "at": event.created_at,
                        })).collect::<Vec<_>>(),
                },
                "receipt_claimed": occurrence.receipt_claimed,
                "confirmed_conclusion": occurrence.delivery_claimed && occurrence.receipt_claimed,
                "claim_history": claim_events.iter().map(|event| json!({
                    "uid": event.uid,
                    "role": event.role.as_str(),
                    "claimed": event.asserted,
                    "person": event.actor_person_uid,
                    "fact": event.fact_uid,
                    "request_id": event.idempotency_key,
                    "at": event.created_at,
                })).collect::<Vec<_>>(),
                "settlement_progress": {
                    "canonical_quantity": progress.canonical_quantity,
                    "settled_quantity": progress.settled_quantity,
                    "remaining_quantity": progress.remaining_quantity,
                    "partially_settled": progress.partially_settled,
                    "settled": progress.settled,
                    "slices": settlement_history,
                },
                "capabilities": {
                    "confirm_delivery": !occurrence.delivery_claimed && delivery_blockers.is_empty(),
                    "correct_delivery": occurrence.delivery_claimed && delivery_blockers.is_empty(),
                    "confirm_receipt": !occurrence.receipt_claimed && receipt_blockers.is_empty(),
                    "correct_receipt": occurrence.receipt_claimed && receipt_blockers.is_empty(),
                    "set_application_formula": receipt_blockers.is_empty(),
                    "settle": can_settle,
                    "dispute": can_dispute,
                    "retract_dispute": can_retract,
                    "create_remainder_draft": can_create_remainder,
                    "create_reversing_transfer": can_create_reversing,
                },
                "blocking_reasons": {
                    "delivery": delivery_blockers,
                    "receipt": receipt_blockers,
                    "set_application_formula": receipt_blockers,
                    "settle": settlement_blockers,
                    "dispute": dispute_blockers,
                    "retract_dispute": retract_dispute_blockers,
                    "create_remainder_draft": remainder_draft_blockers,
                    "create_reversing_transfer": reversing_transfer_blockers,
                },
                "action_payloads": {
                    "create_remainder_draft": {
                        "action": "create-transfer-remainder-draft",
                        "occurrence": occurrence.uid,
                        "expected_revision": t.transfer.revision,
                        "expected_remaining_quantity": progress.remaining_quantity,
                        "request_id": Value::Null,
                        "person": correction_actor,
                    },
                    "create_reversing_transfer": {
                        "action": "create-reversing-transfer-draft",
                        "occurrence": occurrence.uid,
                        "expected_revision": t.transfer.revision,
                        "canonical_quantity": occurrence.quantity,
                        "request_id": Value::Null,
                        "person": correction_actor,
                    },
                },
            });
            if viewer_owns_origin {
                let object = projected_occurrence
                    .as_object_mut()
                    .expect("occurrence projection is an object");
                object.insert("record".into(), json!(occurrence.record_uid));
                object.insert(
                    "record_head".into(),
                    json!(origin_record.map(|record| record.head.as_str())),
                );
                object.insert(
                    "record_slug".into(),
                    json!(origin_record.and_then(|record| record.slug.as_deref())),
                );
                if let Some((formula, formula_hash, version, source, error, applied_local_delta)) =
                    private_application
                {
                    object.insert(
                        "application".into(),
                        json!({
                            "formula": formula,
                            "formula_hash": formula_hash,
                            "version": version,
                            "source": source,
                            "applied_local_delta": applied_local_delta,
                            "error": error,
                        }),
                    );
                }
                if let Some(preview) = settlement_preview {
                    object.insert("settlement_preview".into(), preview);
                }
                object.insert(
                    "correction_status".into(),
                    json!(if occurrence.system_disputed {
                        "system_disputed"
                    } else if participant_disputed {
                        "participant_disputed"
                    } else if !progress.slices.is_empty()
                        && settlement_compensations.len() == progress.slices.len()
                    {
                        "private_applications_compensated"
                    } else if !settlement_compensations.is_empty() {
                        "private_applications_partially_compensated"
                    } else {
                        "none"
                    }),
                );
                object.insert(
                    "private_compensated_slices".into(),
                    json!(settlement_compensations.len()),
                );
            }
            occurrence_projection.push(projected_occurrence);
        }
        let all_occurrences_settled = !occurrences.is_empty()
            && settlement_progress_by_occurrence
                .values()
                .all(|progress| progress.settled);
        let settle_transfer_blockers = if can_settle_occurrence {
            Vec::new()
        } else if occurrences.is_empty() {
            vec!["transfer_has_no_occurrences"]
        } else if all_occurrences_settled {
            vec!["all_occurrences_settled"]
        } else {
            vec!["no_settleable_occurrence"]
        };
        let dispute_transfer_blockers = if can_dispute_occurrence {
            Vec::new()
        } else {
            vec!["no_authorized_occurrence_dispute"]
        };
        let retract_dispute_transfer_blockers = if can_retract_dispute {
            Vec::new()
        } else {
            vec!["no_authorized_dispute_retraction"]
        };
        let compensate_transfer_blockers = if can_compensate_settlement {
            Vec::new()
        } else if private_settlement_slices > 0
            && private_settlement_slices == private_compensated_slices
        {
            vec!["all_private_settlements_compensated"]
        } else {
            vec!["no_compensatable_private_settlement"]
        };
        let remainder_draft_transfer_blockers = if can_create_remainder_draft {
            Vec::new()
        } else {
            vec!["no_remainder_draft_available"]
        };
        let reversing_transfer_blockers = if can_create_reversing_transfer {
            Vec::new()
        } else {
            vec!["no_reversing_transfer_available"]
        };
        let settlement_resource_progress = settlement_by_resource
            .into_iter()
            .map(|((concept, unit), (canonical, settled, remaining))| {
                let concept_name = concept.as_deref().and_then(|uid| concept_names.get(uid));
                let unit_name = unit.as_deref().and_then(|uid| concept_names.get(uid));
                json!({
                    "concept": concept,
                    "concept_name": concept_name,
                    "unit": unit,
                    "unit_name": unit_name,
                    "canonical_quantity": canonical,
                    "settled_quantity": settled,
                    "remaining_quantity": remaining,
                })
            })
            .collect::<Vec<_>>();
        let inbox_facets = json!({
            "mine": inbox_mine,
            "invited": inbox_invited,
            "awaiting_me": awaiting_me,
            "awaiting_others": awaiting_others,
            "active": matches!(primary_status,
                "active" | "partially_settled" | "disputed" | "system_disputed"),
            "completed": matches!(primary_status, "completed" | "satiated"),
            "cancelled_or_broken": matches!(primary_status, "cancelled" | "broken" | "expired"),
            "discoverable_open": t.transfer.visibility == "public"
                && promises.iter().any(|promise| promise.state == nucleus::PromiseState::Open)
                && !inbox_mine,
        });
        let timeline = transfer_timeline(
            store,
            uid,
            t.transfer.revision as u64,
            &invitations,
            &agreement_events,
            &occurrences,
            source_group_state.as_ref(),
            &correction_lineage,
            &promise_successors,
            recipient_details_authorized,
            viewer_person,
        )
        .await?;
        let current_proof = match current_revision_fact.as_ref() {
            Some(fact) => Some(transfer_fact_proof(store, &fact.uid).await?),
            None => None,
        };
        let visibility_rules = store::visibility::rules_for_target(&store.pool, uid).await?;
        let mut recipient_keys = HashSet::new();
        let mut recipients = Vec::new();
        let mut add_recipient =
            |kind: &str, recipient_uid: Option<&str>, reason: &str, state: Option<&str>| {
                let key = format!("{kind}:{}:{reason}", recipient_uid.unwrap_or("*"));
                if recipient_keys.insert(key) {
                    recipients.push(json!({
                        "kind": kind,
                        "uid": recipient_uid,
                        "reason": reason,
                        "state": state,
                    }));
                }
            };
        if let Some(creator) = creator_person
            .as_deref()
            .filter(|creator| recipient_details_authorized || viewer_person == Some(*creator))
        {
            add_recipient("person", Some(creator), "creator", Some("accepted"));
        }
        for (_, person, _) in parties.iter().filter(|(_, person, _)| {
            recipient_details_authorized || viewer_person == Some(person.as_str())
        }) {
            add_recipient("person", Some(person), "party", Some("accepted"));
        }
        for invitation in invitations.iter().filter(|invitation| {
            recipient_details_authorized
                || viewer_person == Some(invitation.addressed_person_uid.as_str())
        }) {
            add_recipient(
                "person",
                Some(&invitation.addressed_person_uid),
                "invitation",
                Some(invitation.status.as_str()),
            );
        }
        for rule in visibility_rules.iter().filter(|rule| {
            recipient_details_authorized
                || rule.subject_kind == "public"
                || rule.subject_uid.as_deref() == viewer.subject.as_deref()
        }) {
            add_recipient(
                &rule.subject_kind,
                rule.subject_uid.as_deref(),
                "explicit_rule",
                Some(&rule.grant_level),
            );
        }
        let visibility_projection = json!({
            "policy": t.transfer.visibility,
            "max_proximity": t.transfer.max_proximity,
            "scope": "whole_transfer",
            "recipients": recipients,
            "explicit_rules": visibility_rules.iter().filter(|rule| {
                recipient_details_authorized
                    || rule.subject_kind == "public"
                    || rule.subject_uid.as_deref() == viewer.subject.as_deref()
            }).map(|rule| json!({
                "uid": rule.uid,
                "subject_kind": rule.subject_kind,
                "subject_uid": rule.subject_uid,
                "field": rule.field,
                "grant_level": rule.grant_level,
            })).collect::<Vec<_>>(),
            "disclosure": {
                "included": [
                    "signed_terms", "parties", "invitations", "agreements",
                    "occurrences", "claims", "public_settlements", "disputes",
                    "source_group_evidence", "threads"
                ],
                "excluded": [
                    "private_record_quantity", "private_application_formula",
                    "private_application_delta", "unrelated_records"
                ],
                "field_overrides_supported": false,
            },
        });
        let output_uid = uid.clone();
        let output_slug = t.slug.clone();
        let output_head = t.head.clone();
        let output_revision = t.transfer.revision as u64;
        let output_status = status.to_string();
        let successors_by_predecessor = promise_successors
            .iter()
            .map(|lineage| (lineage.predecessor_promise_uid.as_str(), lineage))
            .collect::<HashMap<_, _>>();
        let predecessors_by_successor = promise_successors
            .iter()
            .map(|lineage| (lineage.successor_promise_uid.as_str(), lineage))
            .collect::<HashMap<_, _>>();
        let mut value = json!({
            "kind": "transfer",
            "uid": uid,
            "slug": t.slug,
            "head": t.head,
            "revision": t.transfer.revision,
            "status": status,
            "primary_status": primary_status,
            "operational_status": primary_status,
            "inbox_facets": inbox_facets,
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
            "first_completes_evidence": source_group_state.as_ref().map(|state| json!({
                "state": if state.satiated { "satiated" } else { "completed" },
                "satiated": state.satiated,
                "result": {
                    "uid": state.result.uid,
                    "source": state.result.source_uid,
                    "policy": state.result.policy,
                    "winner": state.result.winner_transfer_uid,
                    "winner_revision": state.result.winner_revision,
                    "settlement": state.result.settlement_uid,
                    "fact": state.result.fact_uid,
                    "at": state.result.created_at,
                },
                "loser": state.loser.as_ref().map(|loser| json!({
                    "uid": loser.uid,
                    "transfer": loser.transfer_uid,
                    "revision": loser.transfer_revision,
                    "fact": loser.fact_uid,
                    "at": loser.created_at,
                })),
            })),
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
            "correction_lineage": correction_lineage.iter().map(|lineage| {
                let role = if lineage.source_transfer_uid.as_str() == uid.as_str() {
                    "source"
                } else {
                    "created"
                };
                if recipient_details_authorized {
                    json!({
                        "uid": lineage.uid,
                        "kind": lineage.kind,
                        "role": role,
                        "source_transfer": lineage.source_transfer_uid,
                        "source_occurrence": lineage.source_occurrence_uid,
                        "created_transfer": lineage.created_transfer_uid,
                        "source_revision": lineage.source_revision,
                        "canonical_quantity": lineage.canonical_quantity,
                        "actor": lineage.actor_person_uid,
                        "fact": lineage.fact_uid,
                        "request_id": lineage.idempotency_key,
                        "at": lineage.created_at,
                    })
                } else {
                    json!({ "kind": lineage.kind, "role": role })
                }
            }).collect::<Vec<_>>(),
            "reserve_default": t.transfer.reserve_default,
            "require_confirmation": t.transfer.require_confirmation,
            "default_place": t.transfer.default_place,
            "viewer_party": viewer_party,
            "viewer_roles": viewer_roles,
            "revision_evidence": revision_evidence,
            "proof": current_proof,
            "timeline": timeline,
            "visibility_projection": visibility_projection,
            "threads": threads,
            "capabilities": {
                "edit_terms": can_edit,
                "adopt_terms": can_adopt,
                "address_invitation": can_address_invitation,
                "counteroffer": can_counteroffer,
                "claim_open": can_claim_open,
                "create_thread": can_write_negotiation,
                "create_message": can_write_negotiation,
                "add_party": can_address_invitation,
                "add_promise": false,
                "review": can_review,
                "commit": can_commit,
                "agreement_back": can_move_agreement_back,
                "activate": can_activate_occurrence,
                "confirm": can_confirm_delivery || can_confirm_receipt,
                "confirm_delivery": can_confirm_delivery,
                "confirm_receipt": can_confirm_receipt,
                "settle": can_settle_occurrence,
                "dispute": can_dispute_occurrence,
                "retract_dispute": can_retract_dispute,
                "compensate_settlement": can_compensate_settlement,
                "create_remainder_draft": can_create_remainder_draft,
                "create_reversing_transfer": can_create_reversing_transfer,
            },
            "blocking_reasons": {
                "edit_terms": edit_blockers,
                "adopt_terms": adopt_blockers,
                "address_invitation": address_invitation_blockers,
                "counteroffer": counteroffer_blockers,
                "claim_open": claim_open_blockers,
                "create_thread": negotiation_write_blockers,
                "create_message": negotiation_write_blockers,
                "add_party": [],
                "add_promise": [PHASE_1_DRAFT_ACTIONS_BLOCKER],
                "review": review_blockers,
                "commit": commit_blockers,
                "agreement_back": back_blockers,
                "activate": activate_occurrence_blockers,
                "confirm": if can_confirm_delivery || can_confirm_receipt {
                    Vec::<&str>::new()
                } else {
                    vec!["no_authorized_occurrence_claim"]
                },
                "confirm_delivery": if can_confirm_delivery {
                    Vec::<&str>::new()
                } else {
                    vec!["no_authorized_delivery_claim"]
                },
                "confirm_receipt": if can_confirm_receipt {
                    Vec::<&str>::new()
                } else {
                    vec!["no_authorized_receipt_claim"]
                },
                "settle": settle_transfer_blockers,
                "dispute": dispute_transfer_blockers,
                "retract_dispute": retract_dispute_transfer_blockers,
                "compensate_settlement": compensate_transfer_blockers,
                "create_remainder_draft": remainder_draft_transfer_blockers,
                "create_reversing_transfer": reversing_transfer_blockers,
            },
            "agreement": {
                "reviewed": reviewed,
                "committed": committed,
                "required": required,
                "total": parties.len(),
                "policy_satisfied": agreement_readiness.ready,
                "ready": agreement_readiness.ready,
                "readiness": {
                    "ready": agreement_readiness.ready,
                    "blockers": agreement_readiness.blockers,
                },
                "coalition": readiness_input.coalition.as_ref().map(|coalition| json!({
                    "revision": coalition.revision,
                    "threshold_pct": coalition.threshold_pct,
                    "eligible_count": coalition.eligible_count,
                    "frozen_by_event": coalition.frozen_by_event_uid,
                    "frozen_at": coalition.frozen_at,
                    "party_uids": coalition.party_uids,
                })),
                "history": agreement_events.iter().filter(|event| {
                    recipient_details_authorized
                        || viewer_person == Some(event.person_uid.as_str())
                }).map(|event| json!({
                    "uid": event.uid,
                    "revision": event.revision,
                    "party": event.party_uid,
                    "person": event.person_uid,
                    "from_level": event.from_level,
                    "to_level": event.to_level,
                    "fact": event.fact_uid,
                    "request_id": event.idempotency_key,
                    "at": event.created_at,
                })).collect::<Vec<_>>(),
            },
            "readiness": {
                "ready": agreement_readiness.ready,
                "blockers": agreement_readiness.blockers,
            },
            "dependencies": dependency_status,
            "progress": state_counts,
            "settlement_progress": {
                "occurrences": occurrences.len(),
                "partially_settled": settlement_progress_by_occurrence.values()
                    .any(|progress| progress.partially_settled),
                "settled": all_occurrences_settled,
                "occurrence_statuses": settlement_status_counts,
                "by_resource": settlement_resource_progress,
            },
            "correction_status": if occurrences.iter()
                .any(|occurrence| occurrence.system_disputed) {
                    "system_disputed"
                } else if occurrences.iter().any(|occurrence| occurrence.disputed) {
                    "participant_disputed"
                } else if private_settlement_slices > 0
                    && private_compensated_slices == private_settlement_slices {
                    "private_applications_compensated"
                } else if private_compensated_slices > 0 {
                    "private_applications_partially_compensated"
                } else {
                    "none"
                },
            "private_compensated_slices": if viewer_has_private_transfer_data {
                Some(private_compensated_slices)
            } else {
                None
            },
            "private_settlement_slices": if viewer_has_private_transfer_data {
                Some(private_settlement_slices)
            } else {
                None
            },
            "confirmations": confirmations,
            "occurrences": occurrence_projection,
            "invitations": invitations
                .iter()
                .filter(|invitation| {
                    recipient_details_authorized
                        || viewer_person == Some(invitation.addressed_person_uid.as_str())
                })
                .map(|invitation| {
                    let addressed = records_by_uid.get(&invitation.addressed_person_uid);
                    let inviter = records_by_uid.get(&invitation.invited_by_person_uid);
                    let expired = invitation.expires_at.as_deref().is_some_and(|value| {
                        chrono::DateTime::parse_from_rfc3339(value)
                            .is_ok_and(|instant| {
                                instant.with_timezone(&chrono::Utc) <= chrono::Utc::now()
                            })
                    });
                    let pending = invitation.status
                        == store::transfers::TransferInvitationStatus::Pending;
                    let viewer_is_addressed = viewer.local || viewer.person.as_deref()
                        == Some(invitation.addressed_person_uid.as_str());
                    let mut accept_blockers = viewer.update_identity_blockers();
                    if !pending {
                        accept_blockers.push("invitation_not_pending");
                    }
                    if expired {
                        accept_blockers.push("invitation_expired");
                    }
                    if !viewer_is_addressed {
                        accept_blockers.push("not_invitation_addressee");
                    }
                    if installed_signer_actor
                        != Some(invitation.addressed_person_uid.as_str())
                    {
                        accept_blockers.push("missing_person_signer");
                    }
                    let reject_blockers = accept_blockers.clone();
                    let mut withdraw_blockers = viewer.update_identity_blockers();
                    if !pending {
                        withdraw_blockers.push("invitation_not_pending");
                    }
                    if !is_creator {
                        withdraw_blockers.push("not_transfer_creator");
                    }
                    if installed_signer_actor != creator_person.as_deref() {
                        withdraw_blockers.push("missing_person_signer");
                    }
                    let mut reopen_blockers = viewer.update_identity_blockers();
                    if !matches!(invitation.status,
                        store::transfers::TransferInvitationStatus::Rejected
                        | store::transfers::TransferInvitationStatus::Withdrawn
                        | store::transfers::TransferInvitationStatus::Expired)
                    {
                        reopen_blockers.push("invitation_not_closed");
                    }
                    if !is_creator {
                        reopen_blockers.push("not_transfer_creator");
                    }
                    if installed_signer_actor != creator_person.as_deref() {
                        reopen_blockers.push("missing_person_signer");
                    }
                    json!({
                        "uid": invitation.uid,
                        "status": invitation.status.as_str(),
                        "addressed_person": invitation.addressed_person_uid,
                        "addressed_person_head": addressed.map(|person| person.head.as_str()),
                        "addressed_person_slug": addressed
                            .and_then(|person| person.slug.as_deref()),
                        "invited_by_person": invitation.invited_by_person_uid,
                        "invited_by_person_head": inviter.map(|person| person.head.as_str()),
                        "invited_by_person_slug": inviter
                            .and_then(|person| person.slug.as_deref()),
                        "viewer_is_addressed": viewer.person.as_deref()
                            == Some(invitation.addressed_person_uid.as_str()),
                        "viewer_is_inviter": viewer.person.as_deref()
                            == Some(invitation.invited_by_person_uid.as_str()),
                        "expires_at": invitation.expires_at,
                        "party": invitation.party_uid,
                        "created_at": invitation.created_at,
                        "updated_at": invitation.updated_at,
                        "attempt": invitation.attempt,
                        "attempts": invitation_history
                            .get(&invitation.uid)
                            .cloned()
                            .unwrap_or_default(),
                        "capabilities": {
                            "accept": accept_blockers.is_empty(),
                            "reject": reject_blockers.is_empty(),
                            "withdraw": withdraw_blockers.is_empty(),
                            "reopen": reopen_blockers.is_empty(),
                        },
                        "blocking_reasons": {
                            "accept": accept_blockers,
                            "reject": reject_blockers,
                            "withdraw": withdraw_blockers,
                            "reopen": reopen_blockers,
                        },
                    })
                })
                .collect::<Vec<_>>(),
            "parties": parties
                .iter()
                .filter(|(_, actor, _)| {
                    recipient_details_authorized || viewer_person == Some(actor.as_str())
                })
                .map(|(party, actor, level)| {
                    let level = u8::try_from(*level).unwrap_or(0);
                    let review_blockers = if level == 0 {
                        viewer.agreement_transition_blockers(
                            actor,
                            party,
                            level,
                            1,
                            readiness_input.revision,
                            revision_signed,
                            coalition_members.as_ref(),
                            installed_signer_actor == Some(actor.as_str()),
                        )
                    } else {
                        vec!["party_is_not_at_unchecked_level"]
                    };
                    let commit_blockers = if level == 1 {
                        viewer.agreement_transition_blockers(
                            actor,
                            party,
                            level,
                            2,
                            readiness_input.revision,
                            revision_signed,
                            coalition_members.as_ref(),
                            installed_signer_actor == Some(actor.as_str()),
                        )
                    } else {
                        vec!["party_is_not_at_checked_level"]
                    };
                    let back_blockers = if level > 0 {
                        viewer.agreement_transition_blockers(
                            actor,
                            party,
                            level,
                            level - 1,
                            readiness_input.revision,
                            revision_signed,
                            coalition_members.as_ref(),
                            installed_signer_actor == Some(actor.as_str()),
                        )
                    } else {
                        vec!["party_is_already_at_unchecked_level"]
                    };
                    let person_readiness_blockers = readiness_input
                        .promises
                        .iter()
                        .filter(|promise| promise.person_uid.as_deref() == Some(actor.as_str()))
                        .filter_map(|promise| {
                            agreement_readiness
                                .promises
                                .get(&promise.uid)
                                .filter(|readiness| readiness.eligible && !readiness.ready)
                                .map(|readiness| {
                                    readiness
                                        .blockers
                                        .iter()
                                        .map(|blocker| {
                                            with_promise_context(&promise.uid, blocker)
                                        })
                                        .collect::<Vec<_>>()
                                })
                        })
                        .flatten()
                        .collect::<Vec<_>>();
                    json!({
                        "uid": party,
                        "actor": actor,
                        "actor_head": records_by_uid.get(actor).map(|record| record.head.as_str()),
                        "actor_slug": records_by_uid.get(actor).and_then(|record| record.slug.as_deref()),
                        "level": level,
                        "level_label": match level {
                            0 => "Unchecked",
                            1 => "Checked · ready to agree",
                            2 => "Agreed",
                            _ => "Unknown",
                        },
                        "ready": agreement_readiness.people.get(actor).copied().unwrap_or(false),
                        "readiness": {
                            "ready": agreement_readiness.people.get(actor).copied().unwrap_or(false),
                            "blockers": person_readiness_blockers,
                        },
                        "coalition_member": coalition_members
                            .as_ref()
                            .is_some_and(|members| members.contains(party.as_str())),
                        "capabilities": {
                            "review": review_blockers.is_empty(),
                            "commit": commit_blockers.is_empty(),
                            "agreement_back": back_blockers.is_empty(),
                        },
                        "blocking_reasons": {
                            "review": review_blockers,
                            "commit": commit_blockers,
                            "agreement_back": back_blockers,
                        },
                    })
                })
                .collect::<Vec<_>>(),
            "promises": promises
                .iter()
                .map(|p| {
                    let signed = revision_promises.and_then(|promises| {
                        promises.iter().find(|promise| promise.uid == p.uid)
                    });
                    let record = p.record_uid.as_ref().and_then(|uid| records_by_uid.get(uid));
                    let concept = signed
                        .and_then(|promise| promise.concept_uid.as_deref())
                        .or_else(|| record.and_then(|record| record.concept_uid.as_deref()));
                    let unit = signed.and_then(|promise| promise.unit_uid.as_deref());
                    let signed_delta = signed.map_or(p.delta, |promise| promise.delta);
                    let person = signed
                        .and_then(|promise| promise.person_uid.as_deref())
                        .or(p.party_uid.as_deref());
                    // Promise stage advances through signed agreement events
                    // without creating a new terms revision. The sidecar is
                    // therefore the current projection; the revision snapshot
                    // remains the immutable terms evidence.
                    let state = p.state.as_str();
                    let is_open = state == "open";
                    let proposer = is_open.then_some(person).flatten();
                    let mut claim_blockers = viewer.update_identity_blockers();
                    let agreement = agreement_readiness.promises.get(&p.uid);
                    if !is_open {
                        claim_blockers.push("promise_not_open");
                    }
                    if !viewer.local && is_invitee {
                        claim_blockers.push("invitation_acceptance_required");
                    } else if !viewer.local && !is_participant && t.transfer.visibility != "public" {
                        claim_blockers.push("open_claim_requires_public_visibility");
                    }
                    let claim_person = if viewer.local {
                        installed_signer_actor
                    } else {
                        viewer.person.as_deref()
                    };
                    if claim_person.is_none() || installed_signer_actor != claim_person {
                        claim_blockers.push("missing_person_signer");
                    }
                    if proposer.is_none() {
                        claim_blockers.push("open_proposer_missing");
                    } else if claim_person == proposer {
                        claim_blockers.push("open_proposer_cannot_claim_own_promise");
                    }
                    let mut activate_blockers = viewer.update_identity_blockers();
                    if !viewer.local && viewer.person.as_deref() != person {
                        activate_blockers.push("cannot_activate_another_party_promise");
                    }
                    if !person.is_some_and(|person| installed_signer_actor == Some(person)) {
                        activate_blockers.push("missing_person_signer");
                    }
                    if !agreement.is_some_and(|value| value.ready) {
                        activate_blockers.push("promise_agreement_not_ready");
                    }
                    if state != "agreed" {
                        activate_blockers.push("promise_not_agreed");
                    }
                    if is_open {
                        activate_blockers.push("open_promise_template");
                    }
                    if occurred_promises.contains(p.uid.as_str()) {
                        activate_blockers.push("promise_already_activated");
                    }
                    if let Some(Err(code)) = occurrence_roles.get(&p.uid) {
                        activate_blockers.push(code);
                    }
                    let projected_window_end = signed
                        .and_then(|promise| promise.window_end.as_deref())
                        .or(p.window_end.as_deref());
                    let expired = matches!(state, "proposed" | "agreed")
                        && projected_window_end.is_some_and(|window_end| {
                            chrono::DateTime::parse_from_rfc3339(window_end).is_ok_and(
                                |window_end| {
                                    window_end.with_timezone(&chrono::Utc) <= chrono::Utc::now()
                                },
                            )
                        });
                    let remaining_quantity = occurrences
                        .iter()
                        .find(|occurrence| occurrence.promise_uid == p.uid)
                        .and_then(|occurrence| {
                            settlement_progress_by_occurrence
                                .get(&occurrence.uid)
                                .map(|progress| progress.remaining_quantity)
                        })
                        .unwrap_or(signed_delta.abs());
                    let successor_lineage = successors_by_predecessor.get(p.uid.as_str()).copied();
                    let predecessor_lineage =
                        predecessors_by_successor.get(p.uid.as_str()).copied();
                    let mut reopen_blockers = viewer.update_identity_blockers();
                    if person != installed_signer_actor
                        || !person.is_some_and(|person| {
                            parties.iter().any(|(_, actor, _)| actor == person)
                        })
                    {
                        reopen_blockers.push("not_promise_owner_with_signer");
                    }
                    if !matches!(state, "broken" | "withdrawn") && !expired {
                        reopen_blockers.push("promise_not_reopenable");
                    }
                    if !remaining_quantity.is_finite() || remaining_quantity <= 0.0 {
                        reopen_blockers.push("promise_fully_settled");
                    }
                    if successor_lineage.is_some() {
                        reopen_blockers.push("promise_already_has_successor");
                    }
                    json!({
                        "uid": p.uid,
                        "record": p.record_uid,
                        "record_head": record.map(|record| record.head.as_str()),
                        "record_slug": record.and_then(|record| record.slug.as_deref()),
                        "record_quantity": if viewer.local || viewer.person.as_deref() == person {
                            record.map(|record| record.quantity)
                        } else {
                            None
                        },
                        "availability": if viewer.local || viewer.person.as_deref() == person {
                            p.record_uid.as_deref()
                                .and_then(|uid| availability_by_record.get(uid))
                        } else {
                            None
                        },
                        "concept": concept,
                        "concept_name": concept.and_then(|uid| concept_names.get(uid)),
                        "unit": unit,
                        "unit_name": unit.and_then(|uid| concept_names.get(uid)),
                        "delta": signed_delta,
                        "direction": if signed_delta < 0.0 {
                            "give"
                        } else if signed_delta > 0.0 {
                            "receive"
                        } else {
                            "invalid"
                        },
                        "proposer_delta": is_open.then_some(signed_delta),
                        "state": state,
                        "party": person,
                        "open": is_open,
                        "proposer": proposer,
                        "proposer_head": proposer
                            .and_then(|person| records_by_uid.get(person))
                            .map(|record| record.head.as_str()),
                        "proposer_slug": proposer
                            .and_then(|person| records_by_uid.get(person))
                            .and_then(|record| record.slug.as_deref()),
                        "withdrawn": state == "withdrawn",
                        "window_start": signed.and_then(|promise| promise.window_start.as_deref()),
                        "window_end": projected_window_end,
                        "place": signed.and_then(|promise| promise.location.as_ref()),
                        "reuse_policy": signed
                            .map(|promise| promise.open_reuse_policy.as_str())
                            .unwrap_or_else(|| p.open_reuse_policy.as_str()),
                        "source_promise": signed
                            .and_then(|promise| promise.source_promise_uid.as_deref()),
                        "predecessor": recipient_details_authorized.then(|| predecessor_lineage)
                            .flatten().map(|lineage| json!({
                            "lineage": lineage.uid,
                            "promise": lineage.predecessor_promise_uid,
                            "revision": lineage.revision,
                            "actor": lineage.actor_person_uid,
                            "fact": lineage.fact_uid,
                            "request_id": lineage.idempotency_key,
                            "at": lineage.created_at,
                        })),
                        "successor": recipient_details_authorized.then(|| successor_lineage)
                            .flatten().map(|lineage| json!({
                            "lineage": lineage.uid,
                            "promise": lineage.successor_promise_uid,
                            "revision": lineage.revision,
                            "actor": lineage.actor_person_uid,
                            "fact": lineage.fact_uid,
                            "request_id": lineage.idempotency_key,
                            "at": lineage.created_at,
                        })),
                        "claim_pairs": open_claim_pairs_by_source
                            .get(&p.uid)
                            .into_iter()
                            .flatten()
                            .map(|pair| {
                                let proposer_person = records_by_uid.get(&pair.proposer_person_uid);
                                let claimant_person = records_by_uid.get(&pair.claimant_person_uid);
                                json!({
                                    "uid": pair.uid,
                                    "source": pair.source_promise_uid,
                                    "proposer_promise": pair.proposer_promise_uid,
                                    "claimant_promise": pair.claimant_promise_uid,
                                    "proposer": pair.proposer_person_uid,
                                    "proposer_head": proposer_person
                                        .map(|person| person.head.as_str()),
                                    "proposer_slug": proposer_person
                                        .and_then(|person| person.slug.as_deref()),
                                    "claimant": pair.claimant_person_uid,
                                    "claimant_head": claimant_person
                                        .map(|person| person.head.as_str()),
                                    "claimant_slug": claimant_person
                                        .and_then(|person| person.slug.as_deref()),
                                    "revision": pair.revision,
                                    "reuse_policy": pair.reuse_policy.as_str(),
                                    "request_id": pair.idempotency_key,
                                    "at": pair.created_at,
                                })
                            })
                            .collect::<Vec<_>>(),
                        "condition": p.condition,
                        "reserve_from": p.reserve_from,
                        "agreement_ready": agreement.is_some_and(|value| value.ready),
                        "agreement_eligible": agreement.is_some_and(|value| value.eligible),
                        "agreement_blockers": agreement
                            .map(|value| value.blockers.as_slice())
                            .unwrap_or_default(),
                        "capabilities": {
                            "claim": claim_blockers.is_empty(),
                            "activate": activate_blockers.is_empty(),
                            "reopen": reopen_blockers.is_empty(),
                        },
                        "blocking_reasons": {
                            "claim": claim_blockers,
                            "activate": activate_blockers,
                            "reopen": reopen_blockers,
                        },
                        "action_payloads": {
                            "reopen": {
                                "action": "reopen-transfer-promise",
                                "transfer": uid,
                                "promise": p.uid,
                                "expected_revision": t.transfer.revision,
                                "request_id": Value::Null,
                                "person": person,
                                "window_end": Value::Null,
                                "open": false,
                            },
                        },
                    })
                })
                .collect::<Vec<_>>(),
        });
        if recipient_details_authorized
            && local_organ_uid.as_deref()
                == records_by_uid
                    .get(uid)
                    .and_then(|record| record.organ_uid.as_deref())
        {
            let social_delivery = origin_social_delivery_projection(
                store,
                uid,
                t.transfer.revision as u64,
                viewer_person,
                (is_creator || is_participant)
                    && viewer_person.is_some()
                    && installed_signer_actor == viewer_person,
            )
            .await?;
            if let Some(object) = value.as_object_mut() {
                object.insert("social_delivery".into(), social_delivery);
            }
        }
        transfer_rows.push(TransferOutput {
            uid: output_uid,
            slug: output_slug,
            head: output_head,
            revision: output_revision,
            status: output_status,
            value,
        });
    }
    append_remote_transfer_delivery_rows(
        store,
        protein,
        &viewer,
        installed_signer_actor,
        &mut transfer_rows,
    )
    .await?;
    order_transfers(&mut transfer_rows, &protein.order);
    if let Some(limit) = protein.limit {
        transfer_rows.truncate(limit);
    }
    attach_phase6_transfer_projection(&mut transfer_rows);
    out.extend(transfer_rows.into_iter().map(|row| row.value));
    Ok(out)
}

async fn append_remote_transfer_delivery_rows(
    store: &Store,
    protein: &Protein,
    viewer: &TransferViewer,
    installed_signer_actor: Option<&str>,
    rows: &mut Vec<TransferOutput>,
) -> Result<(), ProteinError> {
    let Some(local_organ) = store::organs::local(&store.pool).await? else {
        return Ok(());
    };
    let references = store::sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            i64,
            i64,
            i64,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
        ),
    >(
        "SELECT uid, origin_organ_uid, transfer_uid, recipient_person_uid,
                recipient_organ_uid, mode, state, policy_revision, last_cursor,
                last_transfer_revision, projection, last_fetched_at, last_error,
                last_envelope_uid
         FROM transfer_remote_reference WHERE recipient_organ_uid = ?
         ORDER BY updated_at, uid",
    )
    .bind(&local_organ.uid)
    .fetch_all(&store.pool)
    .await?;
    for reference in references {
        if !viewer.local && viewer.person.as_deref() != Some(reference.3.as_str()) {
            continue;
        }
        if rows.iter().any(|row| row.uid == reference.2) {
            continue;
        }
        if protein.filter.iter().any(|predicate| match predicate {
            Predicate::UidEq(uid) => uid != &reference.2,
            Predicate::RevisionEq(revision) => *revision != reference.9 as u64,
            Predicate::RevisionLt(revision) => (reference.9 as u64) >= *revision,
            Predicate::RevisionLte(revision) => (reference.9 as u64) > *revision,
            Predicate::RevisionGt(revision) => (reference.9 as u64) <= *revision,
            Predicate::RevisionGte(revision) => (reference.9 as u64) < *revision,
            Predicate::PersonEq(person) => person != &reference.3,
            _ => false,
        }) {
            continue;
        }
        let mut value = reference
            .10
            .as_deref()
            .and_then(|payload| serde_json::from_str::<Value>(payload).ok())
            .unwrap_or_else(|| {
                json!({
                    "kind": "transfer",
                    "uid": reference.2,
                    "head": "Remote Transfer",
                    "revision": reference.9,
                    "status": "remote",
                    "primary_status": "remote",
                })
            });
        let history = store::sqlx::query_as::<_, (String, i64, i64, String)>(
            "SELECT envelope_uid, cursor, transfer_revision, received_at
             FROM transfer_replica_envelope WHERE reference_uid = ? ORDER BY cursor DESC",
        )
        .bind(&reference.0)
        .fetch_all(&store.pool)
        .await?
        .into_iter()
        .map(|entry| {
            json!({
                "envelope": entry.0,
                "cursor": entry.1,
                "revision": entry.2,
                "received_at": entry.3,
            })
        })
        .collect::<Vec<_>>();
        let commands = store::sqlx::query_as::<
            _,
            (
                String,
                String,
                String,
                Option<String>,
                Option<String>,
                String,
            ),
        >(
            "SELECT command_uid, request_id, status, last_error_code, last_error, created_at
             FROM transfer_remote_command
             WHERE direction = 'outgoing' AND origin_organ_uid = ? AND transfer_uid = ?
               AND actor_person_uid = ? ORDER BY created_at, command_uid",
        )
        .bind(&reference.1)
        .bind(&reference.2)
        .bind(&reference.3)
        .fetch_all(&store.pool)
        .await?
        .into_iter()
        .map(|entry| {
            json!({
                "uid": entry.0,
                "request_id": entry.1,
                "status": entry.2,
                "code": entry.3,
                "message": entry.4,
                "at": entry.5,
            })
        })
        .collect::<Vec<_>>();
        let conflicts = store::sqlx::query_as::<
            _,
            (
                String,
                Option<String>,
                Option<i64>,
                i64,
                String,
                String,
                String,
            ),
        >(
            "SELECT uid, request_id, submitted_revision, authoritative_revision,
                    code, reviewed_payload, created_at
             FROM transfer_remote_conflict
             WHERE origin_organ_uid = ? AND transfer_uid = ? AND recipient_person_uid = ?
             ORDER BY created_at, uid",
        )
        .bind(&reference.1)
        .bind(&reference.2)
        .bind(&reference.3)
        .fetch_all(&store.pool)
        .await?
        .into_iter()
        .map(|entry| {
            json!({
                "uid": entry.0,
                "request_id": entry.1,
                "submitted_revision": entry.2,
                "authoritative_revision": entry.3,
                "code": entry.4,
                "retained_input": serde_json::from_str::<Value>(&entry.5).unwrap_or(Value::Null),
                "at": entry.6,
                "capabilities": { "refresh": reference.6 == "active" },
                "action_payloads": {
                    "refresh": {
                        "action": "refresh-transfer-delivery",
                        "transfer": reference.2,
                        "delivery": reference.0,
                        "person": reference.3,
                        "request_id": Value::Null,
                    }
                }
            })
        })
        .collect::<Vec<_>>();
        let configured_formula = store::config::transfer_application_formula(&store.pool).await?;
        let local_records = store::sqlx::query_as::<_, (String, String, String, i64)>(
            "SELECT uid, head, quantity_mantissa, quantity_scale FROM record
             WHERE organ_uid = ? AND deleted_at IS NULL
               AND kind NOT IN ('transfer', 'person', 'organ', 'thread', 'message')
             ORDER BY head, uid",
        )
        .bind(&local_organ.uid)
        .fetch_all(&store.pool)
        .await?
        .into_iter()
        .map(|record| {
            let quantity = store::exact::parse_decimal(&record.2, record.3)?;
            Ok(json!({
                "uid": record.0,
                "head": record.1,
                "quantity": quantity.to_f64(),
            }))
        })
        .collect::<Result<Vec<_>, store::StoreError>>()?;
        let application_handoffs = store::sqlx::query_as::<_, (
            String, String, f64, Option<String>, f64, f64, f64, i64, String, String, Option<String>,
            String,
        )>(
            "SELECT uid, occurrence_uid, canonical_quantity, canonical_unit_uid,
                    canonical_cumulative_before, canonical_cumulative_after,
                    canonical_remaining_after, application_direction,
                    canonical_slice_hash, state, local_application_uid, origin_state
             FROM transfer_remote_application_handoff
             WHERE reference_uid = ? ORDER BY origin_created_at, uid",
        )
        .bind(&reference.0)
        .fetch_all(&store.pool)
        .await?
        .into_iter()
        .map(|handoff| {
            let formula = if handoff.7 < 0 { "-incoming()" } else { configured_formula.as_str() };
            let formula_hash = nucleus::transfer::occurrence_application_formula_hash(formula);
            let can_apply = handoff.9 == "pending"
                && installed_signer_actor == Some(reference.3.as_str());
            json!({
                "uid": handoff.0,
                "occurrence": handoff.1,
                "canonical_quantity": handoff.2,
                "canonical_unit": handoff.3,
                "canonical_cumulative_before": handoff.4,
                "canonical_cumulative_after": handoff.5,
                "canonical_remaining_after": handoff.6,
                "canonical_slice_hash": handoff.8,
                "state": if handoff.11 == "accepted" { handoff.11.as_str() } else { handoff.9.as_str() },
                "local_state": handoff.9,
                "origin_state": handoff.11,
                "local_application": handoff.10,
                "local_record_options": local_records,
                "private_preview": {
                    "formula_hash": formula_hash,
                    "formula_version": 0,
                },
                "capabilities": { "apply": can_apply },
                "blocking_reasons": {
                    "apply": if can_apply { Vec::<&str>::new() } else if handoff.9 != "pending" { vec!["application_already_applied"] } else { vec!["missing_person_signer"] },
                },
                "action_payloads": {
                    "apply": {
                        "action": "apply-remote-transfer-application",
                        "transfer": reference.2,
                        "handoff": handoff.0,
                        "local_record": Value::Null,
                        "expected_formula_hash": formula_hash,
                        "expected_formula_version": 0,
                        "request_id": Value::Null,
                        "person": reference.3,
                    }
                }
            })
        })
        .collect::<Vec<_>>();
        if let Some(object) = value.as_object_mut() {
            object.insert("application_handoffs".into(), json!(application_handoffs));
            object.insert("delivery_read_only".into(), Value::Bool(true));
            object.insert("social_delivery".into(), json!({
                "authority": {
                    "role": if reference.5 == "replicated" { "replica" } else { "hosted_reference" },
                    "origin_organ": reference.1,
                    "local_organ": local_organ.uid,
                    "canonical_writes": "remote",
                },
                "local_view": {
                    "uid": reference.0,
                    "person": reference.3,
                    "mode": reference.5,
                    "state": reference.6,
                    "freshness": {
                        "state": if reference.12.is_some() { "failed" } else if reference.8 == 0 { "never_fetched" } else { "fresh" },
                        "cursor": reference.8,
                        "remote_revision": reference.9,
                        "last_pull_at": reference.11,
                        "last_error": reference.12,
                    },
                    "replica_history": history,
                },
                "mode": reference.5,
                "state": reference.6,
                "revision": reference.7,
                "replica_history": history,
                "outgoing_commands": commands,
                "conflicts": conflicts,
                "application_handoffs": application_handoffs,
                "capabilities": { "refresh": reference.6 == "active" },
                "blocking_reasons": {
                    "refresh": if reference.6 == "active" { Vec::<&str>::new() } else { vec!["delivery_revoked"] },
                },
                "action_payloads": {
                    "refresh": {
                        "action": "refresh-transfer-delivery",
                        "transfer": reference.2,
                        "delivery": reference.0,
                        "person": reference.3,
                        "request_id": Value::Null,
                    },
                },
            }));
        }
        let head = value
            .get("head")
            .and_then(Value::as_str)
            .unwrap_or("Remote Transfer")
            .to_string();
        let slug = value
            .get("slug")
            .and_then(Value::as_str)
            .map(str::to_string);
        let status = value
            .get("primary_status")
            .or_else(|| value.get("status"))
            .and_then(Value::as_str)
            .unwrap_or("remote")
            .to_string();
        rows.push(TransferOutput {
            uid: reference.2,
            slug,
            head,
            revision: reference.9 as u64,
            status,
            value,
        });
    }
    Ok(())
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
    fn delivery(organ_uid: &str, person_uid: &str) -> Self {
        Self {
            local: false,
            recognized: true,
            subject: Some(organ_uid.to_string()),
            person: Some(person_uid.to_string()),
            permissions: HashSet::new(),
        }
    }

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

    fn update_identity_blockers(&self) -> Vec<&'static str> {
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
        blockers
    }

    fn negotiation_write_blockers(
        &self,
        is_creator: bool,
        is_participant: bool,
        is_pending_addressee: bool,
    ) -> Vec<&'static str> {
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
        if !is_creator && !is_participant && !is_pending_addressee {
            blockers.push("not_transfer_negotiation_participant");
        }
        blockers
    }

    fn agreement_transition_blockers(
        &self,
        party_person_uid: &str,
        party_uid: &str,
        current_level: u8,
        target_level: u8,
        revision: u64,
        revision_signed: bool,
        coalition_members: Option<&HashSet<&str>>,
        has_installed_signer: bool,
    ) -> Vec<&'static str> {
        let mut blockers = self.update_identity_blockers();
        if !self.local && self.person.as_deref() != Some(party_person_uid) {
            blockers.push("cannot_author_another_party_agreement");
        }
        if revision == 0 {
            blockers.push("legacy_transfer_requires_adoption");
        } else if !revision_signed {
            blockers.push("transfer_revision_signature_required");
        }
        if !has_installed_signer {
            blockers.push("missing_person_signer");
        }
        if current_level.abs_diff(target_level) != 1 {
            blockers.push("agreement_transition_must_be_adjacent");
        }
        if target_level > current_level
            && coalition_members.is_some_and(|members| !members.contains(party_uid))
        {
            blockers.push("percentage_coalition_frozen_without_party");
        }
        blockers
    }

    fn draft_edit_blockers(
        &self,
        is_creator: bool,
        is_participant: bool,
        revision: u64,
        terms_editable: bool,
    ) -> Vec<&'static str> {
        let mut blockers = self.draft_creator_blockers(is_creator, is_participant);
        if revision == 0 {
            blockers.push("legacy_transfer_requires_adoption");
        }
        if !terms_editable {
            blockers.push("transfer_terms_are_no_longer_draft_editable");
        }
        blockers
    }

    fn draft_adopt_blockers(
        &self,
        is_creator: bool,
        is_participant: bool,
        revision: u64,
        terms_editable: bool,
    ) -> Vec<&'static str> {
        let mut blockers = self.draft_creator_blockers(is_creator, is_participant);
        if revision != 0 {
            blockers.push("transfer_already_revisioned");
        }
        if !terms_editable {
            blockers.push("transfer_terms_are_no_longer_draft_editable");
        }
        blockers
    }

    fn draft_creator_blockers(&self, is_creator: bool, _is_participant: bool) -> Vec<&'static str> {
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
        if !is_creator {
            blockers.push("not_transfer_creator");
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
