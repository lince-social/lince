//! Part VII completion: the fact/concept/transfer sources and the
//! extension/projection/link-depth includes.

use chrono::{DateTime, Utc};
use engine::Engine;
use engine::actions::{
    Action, TransferOccurrenceClaimRole, TransferPromiseInput, TransferReservePoint,
    TransferSatiation, TransferVisibility,
};
use engine::trust::Signer;
use nucleus::transfer::{AgreementType, OpenPromiseReusePolicy, TransferRemainderPolicy};
use nucleus::{Cause, NewFact, PromiseState, RecordKind};
use protein::{
    Aggregate, AggregateOp, ExtensionInclude, GroupBy, Include, LinksInclude, Order, Predicate,
    ProjectionInclude, Protein, Source,
};
use store::records::NewRecord;

async fn engine() -> Engine {
    let engine = Engine::open_memory().await.expect("engine opens");
    store::organs::ensure_local(&engine.store.pool, "http://protein-sources.test")
        .await
        .expect("local Organ");
    engine
}

async fn plain(e: &Engine, slug: &str, quantity: f64) -> String {
    store::records::create(
        &e.store.pool,
        NewRecord {
            slug: Some(slug),
            kind: RecordKind::Plain,
            head: slug,
            body: "",
            quantity: store::exact::from_f64(quantity),
        },
    )
    .await
    .expect("record")
    .uid
}

async fn person(e: &Engine, slug: &str) -> String {
    store::records::create(
        &e.store.pool,
        NewRecord {
            slug: Some(slug),
            kind: RecordKind::Person,
            head: slug,
            body: "",
            quantity: store::exact::one(),
        },
    )
    .await
    .expect("person")
    .uid
}

fn at(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}

fn protein(source: Source) -> Protein {
    Protein {
        source,
        filter: vec![],
        include: Include::default(),
        aggregate: None,
        order: vec![],
        limit: None,
    }
}

async fn bump(e: &Engine, uid: &str, delta: f64, cause: Cause, now: DateTime<Utc>) {
    e.append(
        NewFact {
            actor_uid: None,
            ..NewFact::quantity_f64(uid.to_string(), delta, cause)
        },
        now,
    )
    .await
    .expect("append");
}

#[tokio::test]
async fn fact_source_filters_and_aggregates_the_ledger() {
    let e = engine().await;
    let food = store::concepts::create(&e.store.pool, "food", &[])
        .await
        .unwrap();
    let apples = plain(&e, "apples.stock", 0.0).await;
    let hammer = plain(&e, "hammer", 0.0).await;
    store::records::set_concept(&e.store.pool, &apples, Some(&food))
        .await
        .unwrap();

    bump(
        &e,
        &apples,
        10.0,
        Cause::user_edit(),
        at("2026-07-01T08:00:00Z"),
    )
    .await;
    bump(
        &e,
        &apples,
        -3.0,
        Cause::settlement("t_X"),
        at("2026-07-02T08:00:00Z"),
    )
    .await;
    bump(
        &e,
        &hammer,
        1.0,
        Cause::user_edit(),
        at("2026-07-02T09:00:00Z"),
    )
    .await;

    // W-provenance: the facts of one record
    let mut q = protein(Source::Fact);
    q.filter = vec![Predicate::RecordEq("apples.stock".into())];
    let rows = protein::execute(&e.store, &q).await.unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|r| r["record"] == apples.as_str()));

    // at_since (absolute) narrows the window
    let mut q = protein(Source::Fact);
    q.filter = vec![Predicate::AtSince("2026-07-02T00:00:00Z".into())];
    assert_eq!(protein::execute(&e.store, &q).await.unwrap().len(), 2);

    // concept_in walks the Lingua DAG on the fact's record
    let mut q = protein(Source::Fact);
    q.filter = vec![Predicate::ConceptIn("food".into())];
    assert_eq!(protein::execute(&e.store, &q).await.unwrap().len(), 2);

    // W-finance: sum delta by cause_kind
    let mut q = protein(Source::Fact);
    q.aggregate = Some(Aggregate {
        op: AggregateOp::Sum,
        by: GroupBy::CauseKind,
    });
    let rows = protein::execute(&e.store, &q).await.unwrap();
    // Sums cross the wire as canonical decimal text, not IEEE doubles.
    let get = |group: &str| {
        rows.iter()
            .find(|r| r["group"] == group)
            .map(|r| r["net"].as_str().unwrap().to_string())
    };
    assert_eq!(get("user_edit").as_deref(), Some("11"));
    assert_eq!(get("settlement").as_deref(), Some("-3"));

    // ... and by calendar day
    let mut q = protein(Source::Fact);
    q.aggregate = Some(Aggregate {
        op: AggregateOp::Sum,
        by: GroupBy::Day,
    });
    let rows = protein::execute(&e.store, &q).await.unwrap();
    assert!(
        rows.iter()
            .any(|r| r["group"] == "2026-07-01" && r["net"] == "10")
    );
    assert!(
        rows.iter()
            .any(|r| r["group"] == "2026-07-02" && r["net"] == "-2")
    );
}

#[tokio::test]
async fn concept_source_reads_the_lingua_dag() {
    let e = engine().await;
    let food = store::concepts::create(&e.store.pool, "food", &[])
        .await
        .unwrap();
    let fruit = store::concepts::create(&e.store.pool, "fruit", &[&food])
        .await
        .unwrap();
    store::concepts::create(&e.store.pool, "apple", &[&fruit])
        .await
        .unwrap();
    store::concepts::create(&e.store.pool, "tool", &[])
        .await
        .unwrap();

    let rows = protein::execute(&e.store, &protein(Source::Concept))
        .await
        .unwrap();
    assert_eq!(rows.len(), 4);
    let apple = rows.iter().find(|r| r["name"] == "apple").unwrap();
    assert_eq!(apple["parents"][0], fruit.as_str());

    // concept_in narrows to the family
    let mut q = protein(Source::Concept);
    q.filter = vec![Predicate::ConceptIn("food".into())];
    let names: Vec<String> = protein::execute(&e.store, &q)
        .await
        .unwrap()
        .iter()
        .map(|r| r["name"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(names.len(), 3);
    assert!(!names.contains(&"tool".to_string()));
}

#[test]
fn transfer_source_derives_the_occurrence_status_ladder() {
    std::thread::Builder::new()
        .name("protein-transfer-status".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async {
                    let e = engine().await;
                    let apples = plain(&e, "status.apples", 10.0).await;
                    let ana = person(&e, "status.ana").await;
                    let bia = person(&e, "status.bia").await;
                    let ana_signer = Signer::generate(&ana, "test:status:ana");
                    let bia_signer = Signer::generate(&bia, "test:status:bia");

                    let promise = |uid: &str, person: &str, delta: f64| TransferPromiseInput {
                        uid: Some(uid.into()),
                        record: apples.clone(),
                        party: Some(person.into()),
                        open: false,
                        delta,
                        unit: None,
                        window_start: None,
                        window_end: None,
                        place: None,
                        condition: None,
                        reserve_from: Some(TransferReservePoint::Active),
                        reuse_policy: OpenPromiseReusePolicy::Duplicate,
                        withdrawn: false,
                    };
                    e.set_signer(ana_signer.clone()).await.unwrap();
                    let transfer = e
                        .act(
                            Action::CreateTransferDraft {
                                request_id: "status:create".into(),
                                creator: Some(ana.clone()),
                                slug: Some("status.transfer".into()),
                                head: "Status".into(),
                                agreement: AgreementType::Full,
                                agreement_pct: None,
                                satiation: TransferSatiation::None,
                                parent: None,
                                source: None,
                                visibility: TransferVisibility::Hidden,
                                max_proximity: None,
                                reserve_default: TransferReservePoint::Active,
                                require_confirmation: true,
                                default_place: None,
                                invitees: vec![bia.clone()],
                                promises: vec![
                                    promise("status-give", &ana, -5.0),
                                    promise("status-receive", &bia, 5.0),
                                ],
                                dependencies: Vec::new(),
                            },
                            None,
                        )
                        .await
                        .unwrap()
                        .created
                        .unwrap();
                    let invitation =
                        store::transfers::invitations_for_transfer(&e.store.pool, &transfer)
                            .await
                            .unwrap()
                            .into_iter()
                            .find(|invitation| invitation.addressed_person_uid == bia)
                            .unwrap();
                    let revision = store::transfers::get(&e.store.pool, &transfer)
                        .await
                        .unwrap()
                        .unwrap()
                        .revision as u64;
                    e.set_signer(bia_signer.clone()).await.unwrap();
                    e.act(
                        Action::AcceptTransferInvitation {
                            invitation: invitation.uid,
                            expected_revision: revision,
                            request_id: "status:accept".into(),
                            transfer: Some(transfer.clone()),
                            person: Some(bia.clone()),
                        },
                        None,
                    )
                    .await
                    .unwrap();
                    let revision = store::transfers::get(&e.store.pool, &transfer)
                        .await
                        .unwrap()
                        .unwrap()
                        .revision as u64;
                    assert_eq!(transfer_status(&e, &transfer).await, "proposed");

                    for (person, signer, prefix) in [
                        (&ana, &ana_signer, "status:ana"),
                        (&bia, &bia_signer, "status:bia"),
                    ] {
                        for (level, suffix) in [(1, "checked"), (2, "agreed")] {
                            e.set_signer(signer.clone()).await.unwrap();
                            e.act(
                                Action::SetTransferAgreementLevel {
                                    transfer: transfer.clone(),
                                    expected_revision: revision,
                                    request_id: format!("{prefix}:{suffix}"),
                                    person: Some(person.clone()),
                                    level,
                                },
                                None,
                            )
                            .await
                            .unwrap();
                        }
                    }
                    assert_eq!(transfer_status(&e, &transfer).await, "agreed");

                    let mut occurrences = Vec::new();
                    for (person, signer, promise_uid, request_id) in [
                        (&ana, &ana_signer, "status-give", "status:activate:give"),
                        (
                            &bia,
                            &bia_signer,
                            "status-receive",
                            "status:activate:receive",
                        ),
                    ] {
                        e.set_signer(signer.clone()).await.unwrap();
                        let occurrence = e
                            .act(
                                Action::ActivateTransferOccurrence {
                                    transfer: transfer.clone(),
                                    promise: promise_uid.into(),
                                    expected_revision: revision,
                                    request_id: request_id.into(),
                                    person: Some(person.clone()),
                                },
                                None,
                            )
                            .await
                            .unwrap()
                            .created
                            .unwrap();
                        occurrences.push((person.clone(), occurrence));
                    }
                    assert_eq!(transfer_status(&e, &transfer).await, "in_transfer");

                    for (index, (owner, occurrence)) in occurrences.iter().enumerate() {
                        for (person, signer, role, suffix) in [
                            (
                                &ana,
                                &ana_signer,
                                TransferOccurrenceClaimRole::Delivery,
                                "delivery",
                            ),
                            (
                                &bia,
                                &bia_signer,
                                TransferOccurrenceClaimRole::Receipt,
                                "receipt",
                            ),
                        ] {
                            e.set_signer(signer.clone()).await.unwrap();
                            e.act(
                                Action::SetTransferOccurrenceClaim {
                                    occurrence: occurrence.clone(),
                                    request_id: format!("status:{index}:{suffix}"),
                                    person: Some(person.clone()),
                                    role,
                                    claimed: true,
                                },
                                None,
                            )
                            .await
                            .unwrap();
                        }
                        let signer = if owner == &ana {
                            &ana_signer
                        } else {
                            &bia_signer
                        };
                        e.set_signer(signer.clone()).await.unwrap();
                        let preview = settlement_preview(&e, occurrence, owner, 5.0).await;
                        e.act(
                            Action::SettleTransferOccurrence {
                                occurrence: occurrence.clone(),
                                request_id: format!("status:settle:{index}"),
                                person: Some(owner.clone()),
                                canonical_quantity: 5.0,
                                expected_remaining_quantity: preview["expected_remaining_quantity"]
                                    .as_f64()
                                    .unwrap(),
                                expected_local_delta: preview["expected_local_delta"]
                                    .as_f64()
                                    .unwrap(),
                                expected_application_formula_hash:
                                    preview["expected_application_formula_hash"]
                                        .as_str()
                                        .unwrap()
                                        .into(),
                                expected_application_formula_version:
                                    preview["expected_application_formula_version"]
                                        .as_u64()
                                        .unwrap(),
                                expected_remainder_policy: TransferRemainderPolicy::Visible,
                            },
                            None,
                        )
                        .await
                        .unwrap();
                        assert_eq!(
                            transfer_status(&e, &transfer).await,
                            if index == 0 {
                                "partially_settled"
                            } else {
                                "settled"
                            }
                        );
                    }
                });
        })
        .unwrap()
        .join()
        .unwrap();
}

async fn transfer_status(engine: &Engine, transfer: &str) -> String {
    let mut query = protein(Source::Transfer);
    query.filter = vec![Predicate::UidEq(transfer.into())];
    protein::execute(&engine.store, &query)
        .await
        .unwrap()
        .into_iter()
        .find(|row| row["uid"] == transfer)
        .unwrap()["status"]
        .as_str()
        .unwrap()
        .into()
}

async fn settlement_preview(
    engine: &Engine,
    occurrence: &str,
    person: &str,
    quantity: f64,
) -> serde_json::Value {
    let mut query = protein(Source::TransferSettlementPreview);
    query.filter = vec![
        Predicate::UidEq(occurrence.into()),
        Predicate::QuantityEq(quantity),
    ];
    protein::execute_for_with_signer(&engine.store, &query, None, Some(person))
        .await
        .unwrap()
        .into_iter()
        .next()
        .unwrap()
}

#[tokio::test]
async fn extension_and_projection_includes_attach() {
    let e = engine().await;
    let apples = plain(&e, "apples.stock", 5.0).await;
    e.act(
        Action::SetExtension {
            target: apples.clone(),
            namespace: "task.effort".into(),
            fds: serde_json::json!({ "points": 3 }),
        },
        None,
    )
    .await
    .unwrap();
    store::misc::insert_promise(
        &e.store.pool,
        store::misc::NewPromise {
            record_uid: Some(apples.clone()),
            delta: 3.0,
            window_end: Some("2026-08-01T00:00:00Z".into()),
            state: Some(PromiseState::Agreed),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let mut q = protein(Source::Record);
    q.filter = vec![Predicate::UidEq(apples.clone())];
    q.include = Include {
        extension: Some(ExtensionInclude {
            namespace: "task.effort".into(),
        }),
        projection: Some(ProjectionInclude {
            at: "2026-09-01T00:00:00Z".into(),
        }),
        ..Default::default()
    };
    let rows = protein::execute(&e.store, &q).await.unwrap();
    assert_eq!(rows[0]["extension"]["points"], 3);
    assert_eq!(
        rows[0]["projected"]["quantity"], 8.0,
        "the agreed +3 folds in by September"
    );

    // before the window closes, the promise does not count yet
    q.include.projection = Some(ProjectionInclude {
        at: "2026-07-15T00:00:00Z".into(),
    });
    let rows = protein::execute(&e.store, &q).await.unwrap();
    assert_eq!(rows[0]["projected"]["quantity"], 5.0);
}

#[tokio::test]
async fn link_depth_expands_the_tree() {
    let e = engine().await;
    plain(&e, "a", 0.0).await;
    plain(&e, "b", 0.0).await;
    plain(&e, "c", 0.0).await;
    store::concepts::create(&e.store.pool, "needs", &[])
        .await
        .unwrap();
    for (from, to) in [("a", "b"), ("b", "c")] {
        e.act(
            Action::AddLink {
                from: from.into(),
                kind: "needs".into(),
                to: to.into(),
                quantity: None,
            },
            None,
        )
        .await
        .unwrap();
    }

    let mut q = protein(Source::Record);
    q.filter = vec![Predicate::SlugEq("a".into())];
    q.include = Include {
        links: Some(LinksInclude {
            kind: None,
            kinds: vec!["needs".into()],
            direction: protein::LinkDirection::Out,
            depth: 0, // default: direct links only
        }),
        ..Default::default()
    };
    q.order = vec![Order::Asc("slug".into())];
    let rows = protein::execute(&e.store, &q).await.unwrap();
    assert_eq!(rows[0]["links"].as_array().unwrap().len(), 1);

    q.include.links.as_mut().unwrap().depth = 2;
    let rows = protein::execute(&e.store, &q).await.unwrap();
    let links = rows[0]["links"].as_array().unwrap();
    assert_eq!(links.len(), 2, "hop 2 reaches b -> c");
    assert!(links.iter().any(|l| l["hop"] == 1));
    assert!(links.iter().any(|l| l["hop"] == 2));
}
