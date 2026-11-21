// non-lean-yet
// Scope: mod preparation, struct ChildReadiness, struct AgreementReadinessProjection, struct
//   PromiseAgreementReadiness.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::indexing_slicing] Verify bounds or use checked access on fallible inputs.
//   Sites 116, 121, 122, 150, 151, 156, 163, 164.
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 46, 138, 218,
//   228, 368, 456, 624.
// - [Clippy: clippy::as_conversions] Use From or TryFrom when it preserves the conversion contract.
//   Sites 548, 583, 587.
// - [Clippy: clippy::if_not_else] unnecessary `!=` operation. Sites 129.
// - [Clippy: clippy::map_err_ignore] Preserve relevant causes and context at the boundary. Sites
//   638.
// - [Clippy: clippy::missing_errors_doc] State the observable failure cases for exported functions.
//   Sites 417, 421, 432.
// - [Clippy: clippy::unnecessary_wraps] this function's return value is unnecessarily wrapped by
//   `Result`. Sites 170.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 17, 18,
//   19, 20, 21, 22, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 39, 40, 41, 42, 45, 57, 417, 421, 432.

use std::{
    collections::{HashMap, HashSet},
    future::Future,
    pin::Pin,
    sync::Arc,
};

use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sqlx::{SqliteConnection, SqlitePool};

use crate::StoreError;

mod preparation;
use preparation::Dependencies;
pub use preparation::Paths;

#[derive(Clone, Debug, serde::Serialize)]
pub struct ChildReadiness {
    pub uid: String,
    pub required: bool,
    pub revision: u64,
    pub ready: bool,
    pub settled: bool,
}

#[derive(Clone, Debug, Default)]
pub struct AgreementReadinessProjection {
    pub revision: u64,
    pub settled: bool,
    pub observed: bool,
    pub evidence: Value,
    pub children: Vec<ChildReadiness>,
    pub ready: bool,
    pub blockers: Vec<Value>,
    pub promises: std::collections::BTreeMap<String, PromiseAgreementReadiness>,
    pub people: HashMap<String, bool>,
}

#[derive(Clone, Debug, Default)]
pub struct PromiseAgreementReadiness {
    pub ready: bool,
    pub eligible: bool,
    pub blockers: Vec<Value>,
}

pub fn with_promise_context(promise_uid: &str, blocker: &Value) -> Value {
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

pub fn agreement_path_matches(
    left: &crate::transfers::AgreementPromiseReadinessRow,
    right: &crate::transfers::AgreementPromiseReadinessRow,
) -> bool {
    if left.uid == right.uid
        || left.person_uid == right.person_uid
        || left.unit_uid != right.unit_uid
    {
        return false;
    }
    let same_subject = match (&left.exchange, &right.exchange) {
        (Some(left), Some(right)) => left == right,
        (None, None) => match (&left.concept_uid, &right.concept_uid) {
            (Some(left), Some(right)) => left == right,
            _ => left.record_uid.is_some() && left.record_uid == right.record_uid,
        },
        _ => false,
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

async fn dependency_satisfied(
    connection: &mut SqliteConnection,
    dependency: &nucleus::transfer::TransferRevisionDependency,
    visiting: &mut HashSet<String>,
    cache: &mut HashMap<String, Arc<Evaluated>>,
    now: DateTime<Utc>,
) -> Result<Value, StoreError> {
    let mut projection = json!({"uid":dependency.uid, "scope":dependency.scope.as_str(), "promise":dependency.promise_uid,
        "upstream_kind":dependency.upstream_kind.as_str(), "upstream":dependency.upstream_uid,
        "required_state":dependency.required_state, "satisfied":false});
    match dependency.upstream_kind {
        nucleus::transfer::TransferDependencyUpstreamKind::Promise => {
            let state: Option<String> =
                sqlx::query_scalar("SELECT state FROM promise WHERE uid = ?")
                    .bind(&dependency.upstream_uid)
                    .fetch_optional(&mut *connection)
                    .await?;
            projection["satisfied"] = json!(
                state
                    .as_deref()
                    .is_some_and(|state| promise_state_reaches(state, &dependency.required_state))
            );
            projection["actual_state"] = json!(state);
            projection["evidence"] =
                json!({"kind":"promise_state", "promise":dependency.upstream_uid, "state":state});
        }
        nucleus::transfer::TransferDependencyUpstreamKind::Transfer => {
            let origin: Option<Option<String>> = sqlx::query_scalar("SELECT r.organ_uid FROM transfer t JOIN record r ON r.uid = t.record_uid JOIN record own ON own.uid = r.organ_uid AND own.slug = 'local-organ' AND own.kind = 'organ' AND own.deleted_at IS NULL WHERE t.record_uid = ? AND r.deleted_at IS NULL")
                .bind(&dependency.upstream_uid).fetch_optional(&mut *connection).await?;
            let outcome = if let Some(origin) = origin {
                if origin != dependency.origin_organ_uid {
                    crate::transfer_outcomes::Outcome::unavailable("upstream_origin_changed")
                } else {
                    let upstream =
                        evaluate(connection, &dependency.upstream_uid, visiting, cache, now)
                            .await?;
                    crate::transfer_outcomes::Outcome {
                        agreed: upstream.0.ready,
                        settled: upstream.0.settled,
                        evidence: upstream.0.evidence.clone(),
                        unavailable: None,
                    }
                }
            } else {
                crate::transfer_outcomes::remote_on(connection, &dependency.uid, now).await?
            };
            let satisfied = match dependency.required_state.as_str() {
                "agreed" => outcome.agreed,
                "kept" | "settled" => outcome.settled,
                _ => false,
            };
            projection["satisfied"] = json!(satisfied);
            projection["required_state"] = json!(if dependency.required_state == "kept" {
                "settled"
            } else {
                &dependency.required_state
            });
            projection["actual_state"] = json!(if outcome.settled {
                "settled"
            } else if outcome.agreed {
                "agreed"
            } else {
                "unresolved"
            });
            projection["evidence"] = outcome.evidence;
            projection["blocking_reason"] = json!(outcome.unavailable);
        }
    }
    Ok(projection)
}

fn direct_readiness(
    input: &crate::transfers::TransferAgreementReadinessInput,
    dependency_status: Vec<Value>,
) -> Result<(AgreementReadinessProjection, Vec<Value>), StoreError> {
    let _direct = nucleus::performance::Span::new("transfer-agreement-direct-readiness");
    let paths = Paths::new(&input.promises);
    let dependencies = Dependencies::new(&input.dependencies);
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
    let coalition_members = input.coalition.as_ref().map(|coalition| {
        coalition
            .party_uids
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
    });
    let unagreed_parties = std::cell::OnceCell::new();
    let unagreed_coalition = std::cell::OnceCell::new();
    let satisfied_dependencies = dependency_status
        .iter()
        .filter_map(|value| {
            value["uid"]
                .as_str()
                .map(|uid| (uid, value["satisfied"] == true))
        })
        .collect::<HashMap<_, _>>();

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
        if promise.state == "broken" {
            readiness.blockers.push(json!({"code": "promise_broken"}));
        }
        let Some(person_uid) = promise.person_uid.as_deref() else {
            readiness
                .blockers
                .push(json!({ "code": "promise_person_unassigned" }));
            projection.promises.insert(promise.uid.clone(), readiness);
            continue;
        };
        let party = levels_by_person.get(person_uid).copied();
        if party.is_none()
            || (input.agreement_type != "dependency" && party.is_some_and(|party| party.level < 2))
        {
            readiness.blockers.push(json!({
                "code": "party_agreement_required",
                "person": person_uid,
                "party": party.map(|party| party.party_uid.as_str()),
                "level": party.map_or(0, |party| party.level),
            }));
        }
        let mut promise_dependencies = dependencies.matching(&promise.uid).peekable();
        match input.agreement_type.as_str() {
            "full" => {
                for blocker in unagreed_parties.get_or_init(|| {
                    input
                        .parties
                        .iter()
                        .filter(|party| party.level < 2)
                        .collect::<Vec<_>>()
                }) {
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
                    for (party_uid, level) in unagreed_coalition.get_or_init(|| {
                        members
                            .iter()
                            .filter_map(|party| {
                                let level =
                                    levels_by_party.get(party.as_str()).copied().unwrap_or(0);
                                (level < 2).then_some((party.as_str(), level))
                            })
                            .collect::<Vec<_>>()
                    }) {
                        readiness.blockers.push(json!({
                            "code": "coalition_party_agreement_required",
                            "party": party_uid,
                            "level": level,
                        }));
                    }
                }
                _ => {}
            },
            "dependency" => {
                if promise_dependencies.peek().is_none() {
                    readiness
                        .blockers
                        .push(json!({"code":"upstream_requirement_missing"}));
                }
            }
            "individual" => {
                let mut matches = paths.matching(promise).peekable();
                let mut add_counterparty =
                    |counterperson: Option<&str>,
                     counterparty: Option<&crate::transfers::AgreementPartyLevelRow>,
                     counterpart_promise: Option<&str>| {
                        if counterparty.is_none_or(|party| party.level < 2) {
                            readiness.blockers.push(json!({
                                "code": "counterparty_agreement_required",
                                "promise": counterpart_promise,
                                "person": counterperson,
                                "party": counterparty.map(|party| party.party_uid.as_str()),
                                "level": counterparty.map_or(0, |party| party.level),
                            }));
                        }
                    };
                if let Some(exchange) = &promise.exchange {
                    let other = if exchange.giver == person_uid {
                        &exchange.receiver
                    } else {
                        &exchange.giver
                    };
                    add_counterparty(
                        Some(other.as_str()),
                        levels_by_person.get(other.as_str()).copied(),
                        matches.next().map(|promise| promise.uid.as_str()),
                    );
                } else if matches.peek().is_none() {
                    for party in input
                        .parties
                        .iter()
                        .filter(|party| party.person_uid != person_uid)
                    {
                        add_counterparty(Some(party.person_uid.as_str()), Some(party), None);
                    }
                } else {
                    for counterpart in matches {
                        let counterparty = counterpart
                            .person_uid
                            .as_deref()
                            .and_then(|person| levels_by_person.get(person).copied());
                        add_counterparty(
                            counterpart.person_uid.as_deref(),
                            counterparty,
                            Some(counterpart.uid.as_str()),
                        );
                    }
                }
            }
            _ => readiness
                .blockers
                .push(json!({ "code": "unknown_agreement_policy" })),
        }
        for dependency in promise_dependencies {
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
    let mut people = input
        .parties
        .iter()
        .map(|party| (party.person_uid.as_str(), None::<bool>))
        .collect::<HashMap<_, _>>();
    for promise in &input.promises {
        let Some(person) = promise.person_uid.as_deref() else {
            continue;
        };
        let Some(ready) = people.get_mut(person) else {
            continue;
        };
        if let Some(readiness) = projection
            .promises
            .get(&promise.uid)
            .filter(|row| row.eligible)
        {
            *ready = Some(ready.unwrap_or(true) && readiness.ready);
        }
    }
    projection.people = people
        .into_iter()
        .map(|(person, ready)| (person.to_owned(), ready.unwrap_or(false)))
        .collect();
    let (has_eligible, all_ready) = projection
        .promises
        .values()
        .filter(|readiness| readiness.eligible)
        .fold((false, true), |(_, ready), row| (true, ready && row.ready));
    if !has_eligible {
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
    projection.ready = has_eligible && all_ready;
    Ok((projection, dependency_status))
}
pub async fn read(pool: &SqlitePool, transfer: &str) -> Result<Evaluated, StoreError> {
    read_at(pool, transfer, nucleus::execution::now()).await
}

pub async fn read_at(
    pool: &SqlitePool,
    transfer: &str,
    now: DateTime<Utc>,
) -> Result<Evaluated, StoreError> {
    let mut tx = pool.begin().await?;
    let result = evaluate_owned(&mut tx, transfer, now).await?;
    tx.commit().await?;
    Ok(result)
}

pub async fn read_on(
    connection: &mut SqliteConnection,
    transfer: &str,
) -> Result<Evaluated, StoreError> {
    evaluate_owned(connection, transfer, nucleus::execution::now()).await
}

type Evaluated = (AgreementReadinessProjection, Vec<Value>);

async fn evaluate_owned(
    connection: &mut SqliteConnection,
    transfer: &str,
    now: DateTime<Utc>,
) -> Result<Evaluated, StoreError> {
    let _evaluation = nucleus::performance::Span::new("transfer-agreement-evaluate-await");
    let result = {
        let mut visiting = HashSet::new();
        let mut cache = HashMap::new();
        evaluate(connection, transfer, &mut visiting, &mut cache, now).await?
    };
    match Arc::try_unwrap(result) {
        Ok(result) => Ok(result),
        Err(shared) => {
            nucleus::performance::add("transfer-agreement-root-copy-fallbacks", 1);
            Ok((*shared).clone())
        }
    }
}

fn evaluate<'a>(
    connection: &'a mut SqliteConnection,
    transfer: &'a str,
    visiting: &'a mut HashSet<String>,
    cache: &'a mut HashMap<String, Arc<Evaluated>>,
    now: DateTime<Utc>,
) -> Pin<Box<dyn Future<Output = Result<Arc<Evaluated>, StoreError>> + Send + 'a>> {
    Box::pin(async move {
        if let Some(value) = cache.get(transfer) {
            nucleus::performance::add("transfer-agreement-cache-hits", 1);
            return Ok(Arc::clone(value));
        }
        if visiting.len() >= 128 || cache.len() >= 4096 || !visiting.insert(transfer.into()) {
            return Ok(Arc::new((
                AgreementReadinessProjection {
                    blockers: vec![json!({"code": "agreement_cycle_or_limit"})],
                    ..Default::default()
                },
                vec![],
            )));
        }
        nucleus::performance::add("transfer-agreement-graph-nodes", 1);
        let input = crate::transfers::agreement_readiness_input_on(connection, transfer).await?;
        let mut statuses = Vec::new();
        for dependency in &input.dependencies {
            statuses
                .push(dependency_satisfied(connection, dependency, visiting, cache, now).await?);
        }
        let (mut result, dependencies) = direct_readiness(&input, statuses)?;
        result.revision = input.revision;
        let children = crate::transfer_children::terms_on(connection, transfer).await?;
        for child in children {
            let state = evaluate(connection, &child.uid, visiting, cache, now).await?;
            result.children.push(ChildReadiness {
                uid: child.uid,
                required: child.required,
                revision: state.0.revision,
                ready: state.0.ready,
                settled: state.0.settled,
            });
        }
        let has_own = result.promises.values().any(|promise| promise.eligible);
        let mut has_required = false;
        let mut required_settled = true;
        let mut blockers = Vec::new();
        for child in result.children.iter().filter(|child| child.required) {
            has_required = true;
            required_settled &= child.settled;
            if !child.ready {
                blockers.push(json!({"code": "required_child_not_ready", "child": child.uid}));
            }
        }
        if !has_own && has_required {
            result
                .blockers
                .retain(|blocker| blocker["code"] != "no_agreeable_promises");
            result.ready = true;
        }
        result.settled = (has_own || has_required)
            && input
                .promises
                .iter()
                .filter(|promise| {
                    result
                        .promises
                        .get(&promise.uid)
                        .is_some_and(|state| state.eligible)
                })
                .all(|promise| promise.state == "kept")
            && required_settled;
        if !has_own {
            for dependency in &dependencies {
                if dependency["scope"] == "transfer" && dependency["satisfied"] != true {
                    blockers.push(
                        json!({"code":"dependency_not_satisfied", "dependency":dependency["uid"]}),
                    );
                    result.settled = false;
                }
            }
        }
        let held: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM transfer_occurrence WHERE transfer_uid = ? AND (disputed = 1 OR system_disputed = 1))")
            .bind(transfer).fetch_one(&mut *connection).await?;
        if held {
            blockers.push(json!({"code": "occurrence_disputed"}));
            result.settled = false;
        }
        let signed: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM transfer_revision r JOIN fact f ON f.uid = r.fact_uid JOIN record record ON record.uid = r.transfer_uid WHERE record.deleted_at IS NULL AND r.transfer_uid = ? AND r.revision = ? AND (f.signature IS NOT NULL OR EXISTS(SELECT 1 FROM fact_action_intent i JOIN signed_action_intent a ON a.uid = i.intent_uid WHERE i.fact_uid = f.uid AND a.status = 'committed')))")
            .bind(transfer).bind(input.revision as i64).fetch_one(&mut *connection).await?;
        if !signed {
            blockers.push(json!({"code": "signed_revision_unavailable"}));
            result.settled = false;
        }
        if !blockers.is_empty() {
            result.ready = false;
            for promise in result
                .promises
                .values_mut()
                .filter(|promise| promise.eligible)
            {
                promise.ready = false;
                promise.blockers.extend(blockers.iter().cloned());
            }
            for ready in result.people.values_mut() {
                *ready = false;
            }
            result.blockers.extend(blockers);
        }
        result.observed = input.agreement_type == "dependency"
            && input.parties.len() == 1
            && !input.promises.is_empty()
            && input
                .promises
                .iter()
                .all(|promise| promise.record_uid.is_none() && promise.exchange.is_none())
            && result.children.is_empty();
        if result.observed {
            result.settled = result.ready;
        }
        let revision_fact: Option<String> = sqlx::query_scalar(
            "SELECT fact_uid FROM transfer_revision WHERE transfer_uid = ? AND revision = ?",
        )
        .bind(transfer)
        .bind(input.revision as i64)
        .fetch_optional(&mut *connection)
        .await?;
        let agreement_facts: Vec<String> = sqlx::query_scalar("SELECT fact_uid FROM transfer_agreement_event WHERE transfer_uid = ? AND revision = ? ORDER BY created_at,uid")
            .bind(transfer).bind(input.revision as i64).fetch_all(&mut *connection).await?;
        let settlement_facts: Vec<String> = sqlx::query_scalar("SELECT evidence_fact_uid FROM transfer_occurrence_settlement_slice WHERE transfer_uid = ? ORDER BY created_at,uid")
            .bind(transfer).fetch_all(&mut *connection).await?;
        result.evidence = json!({"kind":"local_outcome", "transfer":transfer, "revision":input.revision,
            "revision_fact":revision_fact, "agreement_facts":agreement_facts, "settlement_facts":settlement_facts,
            "at":now.to_rfc3339()});
        visiting.remove(transfer);
        let result = Arc::new((result, dependencies));
        cache.insert(transfer.into(), Arc::clone(&result));
        Ok(result)
    })
}

pub(crate) async fn validate_graph_on(
    connection: &mut SqliteConnection,
    edges: &mut Vec<nucleus::transfer::TransferDependencyEdge>,
) -> Result<(), StoreError> {
    use nucleus::transfer::{TransferDependencyEdge as Edge, TransferDependencyNode as Node};
    let promises = sqlx::query_as::<_,(String,String)>("SELECT uid,transfer_uid FROM promise WHERE transfer_uid IS NOT NULL AND state != 'withdrawn' ORDER BY uid")
        .fetch_all(&mut *connection).await?;
    let mut owned: HashMap<&str, Vec<&str>> = HashMap::new();
    for (promise, transfer) in &promises {
        owned.entry(transfer).or_default().push(promise);
    }
    let parents = sqlx::query_as::<_,(String,String)>("SELECT t.record_uid,t.parent_uid FROM transfer t LEFT JOIN transfer_child_requirement r ON r.parent_uid = t.parent_uid AND r.child_uid = t.record_uid WHERE t.parent_uid IS NOT NULL AND coalesce(r.required,1) = 1")
        .fetch_all(connection).await?;
    for (child, parent) in parents {
        edges.push(Edge {
            upstream: Node::Transfer(child),
            downstream: Node::Transfer(parent),
        });
    }
    let mut extra = Vec::new();
    for edge in edges.iter() {
        if let Node::Transfer(transfer) = &edge.downstream {
            for promise in owned.get(transfer.as_str()).into_iter().flatten() {
                extra.push(Edge {
                    upstream: edge.upstream.clone(),
                    downstream: Node::Promise((*promise).into()),
                });
            }
        }
    }
    edges.extend(extra);
    for (promise, transfer) in promises {
        edges.push(Edge {
            upstream: Node::Promise(promise),
            downstream: Node::Transfer(transfer),
        });
    }
    nucleus::transfer::transfer_dependency_order(edges)
        .map_err(|_| StoreError::Protocol("Transfer parent or dependency cycle".into()))?;
    Ok(())
}
