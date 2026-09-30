use crate::StoreError;
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sqlx::{SqliteConnection, SqlitePool};
use std::{
    collections::{HashMap, HashSet},
    future::Future,
    pin::Pin,
};

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
    cache: &mut HashMap<String, Evaluated>,
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
                    let (upstream, _) =
                        evaluate(connection, &dependency.upstream_uid, visiting, cache, now)
                            .await?;
                    crate::transfer_outcomes::Outcome {
                        agreed: upstream.ready,
                        settled: upstream.settled,
                        evidence: upstream.evidence,
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
            "dependency" => {
                if !input.dependencies.iter().any(|dependency| {
                    dependency.scope == nucleus::transfer::TransferDependencyScope::Transfer
                        || dependency.promise_uid.as_deref() == Some(promise.uid.as_str())
                }) {
                    readiness
                        .blockers
                        .push(json!({"code":"upstream_requirement_missing"}));
                }
            }
            "individual" => {
                let matches = input
                    .promises
                    .iter()
                    .filter(|counterpart| agreement_path_matches(promise, counterpart))
                    .collect::<Vec<_>>();
                let relevant_people = if let Some(exchange) = &promise.exchange {
                    let other = if exchange.giver == person_uid {
                        &exchange.receiver
                    } else {
                        &exchange.giver
                    };
                    vec![(
                        Some(other.as_str()),
                        levels_by_person.get(other.as_str()).copied(),
                        matches.first().map(|promise| promise.uid.as_str()),
                    )]
                } else if matches.is_empty() {
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
pub async fn read(pool: &SqlitePool, transfer: &str) -> Result<Evaluated, StoreError> {
    read_at(pool, transfer, nucleus::execution::now()).await
}

pub async fn read_at(
    pool: &SqlitePool,
    transfer: &str,
    now: DateTime<Utc>,
) -> Result<Evaluated, StoreError> {
    let mut tx = pool.begin().await?;
    let result = evaluate(
        &mut tx,
        transfer,
        &mut HashSet::new(),
        &mut HashMap::new(),
        now,
    )
    .await?;
    tx.commit().await?;
    Ok(result)
}

pub async fn read_on(
    connection: &mut SqliteConnection,
    transfer: &str,
) -> Result<Evaluated, StoreError> {
    evaluate(
        connection,
        transfer,
        &mut HashSet::new(),
        &mut HashMap::new(),
        nucleus::execution::now(),
    )
    .await
}

type Evaluated = (AgreementReadinessProjection, Vec<Value>);

fn evaluate<'a>(
    connection: &'a mut SqliteConnection,
    transfer: &'a str,
    visiting: &'a mut HashSet<String>,
    cache: &'a mut HashMap<String, Evaluated>,
    now: DateTime<Utc>,
) -> Pin<Box<dyn Future<Output = Result<Evaluated, StoreError>> + Send + 'a>> {
    Box::pin(async move {
        if let Some(value) = cache.get(transfer) {
            return Ok(value.clone());
        }
        if visiting.len() >= 128 || cache.len() >= 4096 || !visiting.insert(transfer.into()) {
            return Ok((
                AgreementReadinessProjection {
                    blockers: vec![json!({"code": "agreement_cycle_or_limit"})],
                    ..Default::default()
                },
                vec![],
            ));
        }
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
            let (state, _) = evaluate(connection, &child.uid, visiting, cache, now).await?;
            result.children.push(ChildReadiness {
                uid: child.uid,
                required: child.required,
                revision: state.revision,
                ready: state.ready,
                settled: state.settled,
            });
        }
        let required = result
            .children
            .iter()
            .filter(|child| child.required)
            .collect::<Vec<_>>();
        let own = result
            .promises
            .values()
            .filter(|promise| promise.eligible)
            .collect::<Vec<_>>();
        if own.is_empty() && !required.is_empty() {
            result
                .blockers
                .retain(|blocker| blocker["code"] != "no_agreeable_promises");
            result.ready = true;
        }
        result.settled = (!own.is_empty() || !required.is_empty())
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
            && required.iter().all(|child| child.settled);
        let mut blockers = required
            .iter()
            .filter(|child| !child.ready)
            .map(|child| json!({"code": "required_child_not_ready", "child": child.uid}))
            .collect::<Vec<_>>();
        if own.is_empty() {
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
        cache.insert(transfer.into(), (result.clone(), dependencies.clone()));
        Ok((result, dependencies))
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
