mod support;

use engine::{Engine, actions::Action};
use nucleus::visibility::{
    Bound, Condition, Data,
    plan::{Command, Definition, Review, Saved, Stage, Status},
};
use support::{DraftOptions, Person, TransferFixture};

const START: i64 = 1_893_456_000_000;

struct Fixture {
    engine: Engine,
    owner: Person,
    other: Person,
    transfer: TransferFixture,
    definition: Definition,
    role: i64,
    item: String,
    viewers: Vec<String>,
}

async fn fixture(engine: Engine) -> Fixture {
    support::karma::authorize(&engine).await;
    let owner = support::person(&engine, "audience-owner").await;
    let other = support::person(&engine, "audience-receiver").await;
    let role = store::auth::ensure_role(&engine.store.pool, "audience-controller")
        .await
        .unwrap();
    for (resource, actions) in [
        ("record", vec!["read", "create", "update"]),
        ("transfer", vec!["read", "create", "update"]),
        ("permission", vec!["assign"]),
        ("frequency", vec!["create", "update"]),
    ] {
        for action in actions {
            let permission = store::auth::ensure_permission(&engine.store.pool, resource, action)
                .await
                .unwrap();
            store::auth::grant(&engine.store.pool, role, permission)
                .await
                .unwrap();
        }
    }
    store::auth::create_credential(
        &engine.store.pool,
        &owner.uid,
        "audience-owner",
        "hash",
        role,
    )
    .await
    .unwrap();
    let item = support::plain(&engine, "private-stock", 10.0).await;
    let transfer = support::create_transfer(
        &engine,
        &owner,
        &[other.clone()],
        vec![
            support::promise("audience-giver", &item, &owner, -1.0),
            support::promise("audience-receiver", &item, &other, 1.0),
        ],
        DraftOptions {
            slug: "audience-request",
            agreement: nucleus::transfer::AgreementType::Full,
            reserve_default: engine::actions::TransferReservePoint::Active,
            require_confirmation: true,
        },
    )
    .await;
    engine.set_signer(owner.signer.clone()).await.unwrap();
    let mut viewers = Vec::new();
    for (index, proximity) in [1, 2, 5, 1].into_iter().enumerate() {
        let uid = nucleus::new_uid("r");
        store::organs::add_contact(
            &engine.store.pool,
            &uid,
            None,
            &format!("Viewer {index}"),
            "",
            proximity,
        )
        .await
        .unwrap();
        store::organs::set_trust(&engine.store.pool, &uid, "known")
            .await
            .unwrap();
        viewers.push(uid);
    }
    let definition = Definition {
        uid: nucleus::new_uid("r"),
        projection_uid: nucleus::new_uid("r"),
        controller_uid: owner.uid.clone(),
        source_uid: transfer.transfer.clone(),
        source_revision: transfer.revision,
        projection: nucleus::social::PostDraft {
            title: "Public request".into(),
            text: "One performance".into(),
            quantity: Some("1".into()),
            unit: Some("performance".into()),
            ..Default::default()
        },
        stages: vec![
            Stage {
                after_ms: 0,
                include: vec![Condition {
                    organs: vec![viewers[0].clone()],
                    ..Default::default()
                }],
            },
            Stage {
                after_ms: 30_000,
                include: vec![Condition {
                    upper: Some(Bound {
                        value: 2,
                        inclusive: true,
                    }),
                    ..Default::default()
                }],
            },
            Stage {
                after_ms: 30_000,
                include: vec![Condition::default()],
            },
        ],
        exclude: vec![Condition {
            organs: vec![viewers[3].clone()],
            ..Default::default()
        }],
        expires_at_ms: START + 1_800_000,
    };
    Fixture {
        engine,
        owner,
        other,
        transfer,
        definition,
        role,
        item,
        viewers,
    }
}

async fn command(f: &Fixture, request: Command) -> serde_json::Value {
    f.engine
        .act(Action::Audience { request }, Some(f.owner.uid.clone()))
        .await
        .unwrap()
        .data
        .unwrap()
}

async fn review(f: &Fixture) -> (Review, String) {
    let value = command(
        f,
        Command::Review {
            definition: f.definition.clone(),
        },
    )
    .await;
    assert!(value["coverage"].as_str().unwrap().contains("incomplete"));
    (
        serde_json::from_value(value["review"].clone()).unwrap(),
        value["commitment"].as_str().unwrap().into(),
    )
}

async fn enable(f: &Fixture, reviewed: &(Review, String)) -> Saved {
    serde_json::from_value(
        command(
            f,
            Command::Enable {
                review: Box::new(reviewed.0.clone()),
                commitment: reviewed.1.clone(),
            },
        )
        .await["plan"]
            .clone(),
    )
    .unwrap()
}

async fn saved(f: &Fixture) -> Saved {
    let value = command(
        f,
        Command::Inspect {
            source_uid: f.transfer.transfer.clone(),
            controller_uid: f.owner.uid.clone(),
        },
    )
    .await;
    serde_json::from_value(value["plans"][0].clone()).unwrap()
}

async fn tick(f: &Fixture, clock: &nucleus::execution::Execution, offset: i64) {
    clock.set_time(START + offset).unwrap();
    let state = saved(f).await;
    f.engine
        .act(
            Action::ApplyRecurrenceOccurrence {
                recurrence: state.rule_uid.unwrap(),
                due_at: clock.now().to_rfc3339(),
                amount: None,
                note: None,
            },
            Some(f.owner.uid.clone()),
        )
        .await
        .unwrap();
    let effects = f.engine.run_database_effects().await.unwrap();
    assert!(
        effects.outcomes.iter().all(|outcome| outcome.ok),
        "{:?}",
        effects.outcomes
    );
}

async fn visibility(f: &Fixture, expected: [bool; 4]) {
    for (viewer, expected) in f.viewers.iter().zip(expected) {
        let result = f
            .engine
            .visibility_read(viewer, &f.definition.projection_uid, Data::Record)
            .await;
        assert_eq!(result.is_ok(), expected);
        if let Ok(value) = result {
            let bytes = serde_json::to_string(&value).unwrap();
            for secret in [&f.item, &f.transfer.transfer, &f.owner.uid, &f.other.uid] {
                assert!(!bytes.contains(secret));
            }
        }
    }
    assert_eq!(
        store::records::quantity(&f.engine.store.pool, &f.item)
            .await
            .unwrap()
            .unwrap()
            .to_string(),
        "10"
    );
    assert!(
        store::transfers::occurrences_of(&f.engine.store.pool, &f.transfer.transfer)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(f.engine.location_sources().await.is_empty());
}

#[test]
fn reviewed_audience_stages_use_one_normal_rule_and_survive_replay_and_real_reopen() {
    support::run_async_test("audience-stages", || async {
        let clock = nucleus::execution::Execution::new([71; 32], START).unwrap();
        clock
            .scope(async {
                let directory = tempfile::tempdir().unwrap();
                let url = format!(
                    "sqlite://{}",
                    directory.path().join("audience.db").display()
                );
                let mut f = fixture(Engine::open(&url).await.unwrap()).await;
                let reviewed = review(&f).await;
                assert!(
                    store::records::get(&f.engine.store.pool, &f.definition.uid)
                        .await
                        .unwrap()
                        .is_none()
                );
                let first = enable(&f, &reviewed).await;
                assert_eq!(first.stage, Some(0));
                assert!(
                    store::records::mark_deleted(&f.engine.store.pool, &f.definition.uid)
                        .await
                        .is_err()
                );
                assert!(
                    store::records::set_extension(
                        &f.engine.store.pool,
                        &f.definition.uid,
                        nucleus::visibility::plan::PRIVATE_NAMESPACE,
                        &serde_json::json!({"status":"stopped"})
                    )
                    .await
                    .is_err()
                );
                visibility(&f, [true, false, false, false]).await;
                let replay = enable(&f, &reviewed).await;
                assert_eq!(replay.revision, first.revision);
                tick(&f, &clock, 30_000).await;
                visibility(&f, [true, true, false, false]).await;
                f.engine.store.pool.close().await;
                f.engine = Engine::open(&url).await.unwrap();
                f.engine.set_signer(f.owner.signer.clone()).await.unwrap();
                assert_eq!(saved(&f).await.stage, Some(1));
                tick(&f, &clock, 60_000).await;
                visibility(&f, [true, true, true, false]).await;
                tick(&f, &clock, 90_000).await;
                assert_eq!(saved(&f).await.stage, Some(2));
                let count: i64 =
                    store::sqlx::query_scalar("SELECT COUNT(*) FROM recurrence WHERE record_uid=?")
                        .bind(&f.definition.uid)
                        .fetch_one(&f.engine.store.pool)
                        .await
                        .unwrap();
                assert_eq!(count, 1);
            })
            .await;
    });
}

#[test]
fn a_real_transfer_reply_pauses_and_only_fresh_review_resumes_the_remaining_stages() {
    support::run_async_test("audience-reply", || async {
        let clock = nucleus::execution::Execution::new([72; 32], START).unwrap();
        clock
            .scope(async {
                let f = fixture(Engine::open_memory().await.unwrap()).await;
                let reviewed = review(&f).await;
                enable(&f, &reviewed).await;
                f.engine.set_signer(f.other.signer.clone()).await.unwrap();
                let thread = f
                    .engine
                    .act(
                        Action::CreateTransferThread {
                            transfer: f.transfer.transfer.clone(),
                            head: "Reply".into(),
                            request_id: "audience-thread".into(),
                            person: f.other.uid.clone(),
                        },
                        None,
                    )
                    .await
                    .unwrap()
                    .created
                    .unwrap();
                f.engine
                    .act(
                        Action::CreateTransferMessage {
                            transfer: f.transfer.transfer.clone(),
                            thread,
                            body: "I can help".into(),
                            parent: None,
                            references: vec![],
                            request_id: "audience-reply".into(),
                            person: f.other.uid.clone(),
                        },
                        None,
                    )
                    .await
                    .unwrap();
                f.engine.set_signer(f.owner.signer.clone()).await.unwrap();
                tick(&f, &clock, 30_000).await;
                assert_eq!(saved(&f).await.status, Status::Paused);
                visibility(&f, [true, false, false, false]).await;
                enable(&f, &reviewed).await;
                assert_eq!(saved(&f).await.status, Status::Paused);
                let fresh = review(&f).await;
                assert_ne!(fresh.1, reviewed.1);
                enable(&f, &fresh).await;
                assert_eq!(saved(&f).await.stage, Some(0));
                tick(&f, &clock, 60_000).await;
                visibility(&f, [true, true, false, false]).await;
                let count: i64 =
                    store::sqlx::query_scalar("SELECT COUNT(*) FROM recurrence WHERE record_uid=?")
                        .bind(&f.definition.uid)
                        .fetch_one(&f.engine.store.pool)
                        .await
                        .unwrap();
                assert_eq!(count, 1);
            })
            .await;
    });
}

#[test]
fn changed_reviews_permissions_and_terminal_states_never_widen_or_disclose_private_fields() {
    support::run_async_test("audience-guards", || async {
        for variant in [
            "review-fields",
            "source-revision",
            "source-deleted",
            "permission",
            "expiry",
            "paused-expiry",
            "stop",
            "agreement",
            "cancel",
            "projection-fields",
            "audience-policy",
        ] {
            let clock = nucleus::execution::Execution::new([73; 32], START).unwrap();
            clock
                .scope(async {
                    let f = fixture(Engine::open_memory().await.unwrap()).await;
                    let mut reviewed = review(&f).await;
                    if variant == "review-fields" {
                        reviewed.0.definition.projection.text = "Changed".into();
                        let before = f.engine.store.state_hash().await.unwrap();
                        assert!(
                            f.engine
                                .act(
                                    Action::Audience {
                                        request: Command::Enable {
                                            review: Box::new(reviewed.0),
                                            commitment: reviewed.1
                                        }
                                    },
                                    Some(f.owner.uid.clone())
                                )
                                .await
                                .is_err()
                        );
                        assert_eq!(before, f.engine.store.state_hash().await.unwrap());
                        return;
                    }
                    let state = enable(&f, &reviewed).await;
                    match variant {
                        "source-revision" => {
                            f.engine
                                .act(
                                    Action::PublishTransfer {
                                        transfer: f.transfer.transfer.clone(),
                                        expected_revision: f.transfer.revision,
                                        request_id: "audience-source-publication".into(),
                                        person: Some(f.owner.uid.clone()),
                                        expected: None,
                                        expected_state: None,
                                    },
                                    None,
                                )
                                .await
                                .unwrap();
                        }
                        "permission" => {
                            let permission = store::auth::ensure_permission(
                                &f.engine.store.pool,
                                "permission",
                                "assign",
                            )
                            .await
                            .unwrap();
                            store::auth::revoke(&f.engine.store.pool, f.role, permission)
                                .await
                                .unwrap();
                        }
                        "source-deleted" => {
                            store::records::mark_deleted(
                                &f.engine.store.pool,
                                &f.transfer.transfer,
                            )
                            .await
                            .unwrap();
                        }
                        "expiry" | "paused-expiry" => {
                            if variant == "paused-expiry" {
                                command(
                                    &f,
                                    Command::Pause {
                                        plan_uid: f.definition.uid.clone(),
                                        controller_uid: f.owner.uid.clone(),
                                        expected_revision: state.revision,
                                    },
                                )
                                .await;
                            }
                            clock.set_time(f.definition.expires_at_ms).unwrap();
                        }
                        "stop" => {
                            command(
                                &f,
                                Command::Stop {
                                    plan_uid: f.definition.uid.clone(),
                                    controller_uid: f.owner.uid.clone(),
                                    expected_revision: state.revision,
                                },
                            )
                            .await;
                        }
                        "agreement" => {
                            support::agree(&f.engine, &f.transfer, &f.owner, "owner-agree").await;
                            support::agree(&f.engine, &f.transfer, &f.other, "receiver-agree")
                                .await;
                            f.engine.set_signer(f.owner.signer.clone()).await.unwrap();
                        }
                        "cancel" => {
                            store::sqlx::query(
                                "UPDATE promise SET state='withdrawn' WHERE transfer_uid=?",
                            )
                            .bind(&f.transfer.transfer)
                            .execute(&f.engine.store.pool)
                            .await
                            .unwrap();
                        }
                        "projection-fields" => {
                            f.engine
                                .act(
                                    Action::EditRecordText {
                                        target: f.definition.projection_uid.clone(),
                                        head: None,
                                        body: Some("unreviewed".into()),
                                    },
                                    None,
                                )
                                .await
                                .unwrap();
                        }
                        "audience-policy" => {
                            let mut tx = store::write_tx(&f.engine.store.pool).await.unwrap();
                            store::data_visibility::save_on(
                                &mut tx,
                                &f.definition.projection_uid,
                                Data::Record,
                                None,
                                &Default::default(),
                                state.policy_revision,
                            )
                            .await
                            .unwrap();
                            tx.commit().await.unwrap();
                        }
                        _ => unreachable!(),
                    }
                    let offset = if matches!(variant, "expiry" | "paused-expiry") {
                        f.definition.expires_at_ms - START
                    } else {
                        30_000
                    };
                    tick(&f, &clock, offset).await;
                    assert_ne!(saved(&f).await.status, Status::Active, "{variant}");
                    visibility(&f, [false, false, false, false]).await;
                })
                .await;
        }
    });
}
