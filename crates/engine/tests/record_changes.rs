use engine::{
    Engine,
    actions::Action,
    record_change::{Mutation, Request, WorkField},
    sync::{Delivery, OpBatch},
    trust::Signer,
};
use nucleus::RecordKind;
use serde_json::json;

#[tokio::test]
async fn concurrent_assertion_additions_converge_and_observed_removals_do_not_return() {
    let (a, ao) = cell().await;
    let (b, bo) = cell().await;
    pair(&a, &ao, &b, &bo).await;
    let uid = record(&a, "membership").await;
    let predicate = store::concepts::ensure(&a.store.pool, "selected")
        .await
        .unwrap();
    sync(&a, &b, &bo).await;
    for engine in [&a, &b] {
        engine
            .change_record(
                request(
                    &uid,
                    Mutation::Assertion {
                        predicate: predicate.clone(),
                        object: None,
                        quantity: None,
                        unit: None,
                    },
                ),
                None,
            )
            .await
            .unwrap();
    }
    sync(&a, &b, &bo).await;
    sync(&b, &a, &ao).await;
    let first = store::assertions::for_subjects(&a.store.pool, &[uid.clone()])
        .await
        .unwrap();
    let second = store::assertions::for_subjects(&b.store.pool, &[uid.clone()])
        .await
        .unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(second.len(), 1);
    assert_eq!(first[0].uid, second[0].uid);
    a.change_record(
        request(
            &uid,
            Mutation::RetractAssertion {
                assertion: first[0].uid.clone(),
            },
        ),
        None,
    )
    .await
    .unwrap();
    sync(&a, &b, &bo).await;
    sync(&b, &a, &ao).await;
    for engine in [&a, &b] {
        assert!(
            store::assertions::for_subjects(&engine.store.pool, &[uid.clone()])
                .await
                .unwrap()
                .is_empty()
        );
    }
    b.change_record(
        request(
            &uid,
            Mutation::Assertion {
                predicate,
                object: None,
                quantity: None,
                unit: None,
            },
        ),
        None,
    )
    .await
    .unwrap();
    sync(&b, &a, &ao).await;
    assert_eq!(
        store::assertions::for_subjects(&a.store.pool, &[uid])
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn assigned_levels_survive_checkpoints_and_later_additions() {
    let (engine, _) = cell().await;
    let uid = record(&engine, "levels").await;
    let from = chrono::Utc::now() - chrono::Duration::seconds(1);
    engine
        .change_record(
            request(
                &uid,
                Mutation::Quantity {
                    value: "12.50".into(),
                },
            ),
            None,
        )
        .await
        .unwrap();
    engine.checkpoint_all(chrono::Utc::now()).await.unwrap();
    engine.append_user(&uid, 2.0).await.unwrap();
    engine
        .change_record(
            request(
                &uid,
                Mutation::Quantity {
                    value: "9.75".into(),
                },
            ),
            None,
        )
        .await
        .unwrap();
    let to = chrono::Utc::now() + chrono::Duration::seconds(1);
    assert_eq!(
        store::facts::level(&engine.store.pool, &uid)
            .await
            .unwrap()
            .to_string(),
        "9.75"
    );
    assert_eq!(
        store::ledger::level_at(&engine.store.pool, &uid, to)
            .await
            .unwrap()
            .to_string(),
        "9.75"
    );
    let series = store::ledger::level_series(&engine.store.pool, &uid, from, to)
        .await
        .unwrap();
    assert_eq!(series.last().unwrap().1.to_string(), "9.75");
    engine.rebuild_read_model().await.unwrap();
    assert_eq!(quantity(&engine, &uid).await, "9.75");
}

#[tokio::test]
async fn assertion_requests_are_saved_once_and_retractions_stay_on_their_record() {
    let (engine, _) = cell().await;
    let uid = record(&engine, "assertion-request").await;
    let other = record(&engine, "other-assertion-request").await;
    store::concepts::ensure(&engine.store.pool, "selected")
        .await
        .unwrap();
    let add = request(
        &uid,
        Mutation::Assertion {
            predicate: "selected".into(),
            object: None,
            quantity: None,
            unit: None,
        },
    );
    let first = engine.change_record(add.clone(), None).await.unwrap();
    let again = engine.change_record(add, None).await.unwrap();
    assert_eq!(first.created, again.created);
    assert!(again.facts.is_empty());
    let assertion = first.created.unwrap();
    assert!(
        engine
            .change_record(
                request(
                    &other,
                    Mutation::RetractAssertion {
                        assertion: assertion.clone()
                    }
                ),
                None
            )
            .await
            .is_err()
    );
    let remove = request(
        &uid,
        Mutation::RetractAssertion {
            assertion: assertion.clone(),
        },
    );
    engine.change_record(remove.clone(), None).await.unwrap();
    let newer = engine
        .change_record(
            request(
                &uid,
                Mutation::Assertion {
                    predicate: "selected".into(),
                    object: None,
                    quantity: None,
                    unit: None,
                },
            ),
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    engine.change_record(remove, None).await.unwrap();
    assert!(
        store::assertions::get(&engine.store.pool, &newer)
            .await
            .unwrap()
            .unwrap()
            .retracted_at
            .is_none()
    );
    assert!(
        store::assertions::get(&engine.store.pool, &assertion)
            .await
            .unwrap()
            .unwrap()
            .retracted_at
            .is_some()
    );
}

#[tokio::test]
async fn rebuilding_preserves_register_values_and_work_log_identity() {
    let (engine, _) = cell().await;
    let uid = record(&engine, "rebuild").await;
    engine
        .change_record(
            request(
                &uid,
                Mutation::Quantity {
                    value: "7.25".into(),
                },
            ),
            None,
        )
        .await
        .unwrap();
    engine
        .change_record(
            request(
                &uid,
                Mutation::Slug {
                    value: Some("rebuilt".into()),
                },
            ),
            None,
        )
        .await
        .unwrap();
    engine
        .change_record(
            request(
                &uid,
                Mutation::Work {
                    field: WorkField::Due,
                    value: json!("2026-09-17"),
                },
            ),
            None,
        )
        .await
        .unwrap();
    engine
        .change_record(request(&uid, Mutation::Timer { running: true }), None)
        .await
        .unwrap();
    engine
        .change_record(request(&uid, Mutation::Timer { running: false }), None)
        .await
        .unwrap();
    let before = store::records::get_extension(&engine.store.pool, &uid, "work")
        .await
        .unwrap();
    engine.rebuild_read_model().await.unwrap();
    assert_eq!(quantity(&engine, &uid).await, "7.25");
    assert_eq!(
        store::records::get(&engine.store.pool, &uid)
            .await
            .unwrap()
            .unwrap()
            .slug
            .as_deref(),
        Some("rebuilt")
    );
    assert_eq!(
        store::records::get_extension(&engine.store.pool, &uid, "work")
            .await
            .unwrap(),
        before
    );
    assert!(engine.audit_read_model().await.unwrap().is_clean());
}

#[tokio::test]
async fn offline_text_crosses_multiple_checkpoints_and_a_lost_ack() {
    let (a, ao) = cell().await;
    let (b, bo) = cell().await;
    pair(&a, &ao, &b, &bo).await;
    let uid = record(&a, "checkpoint").await;
    sync(&a, &b, &bo).await;
    for index in 0..300 {
        a.write_record_text(&uid, None, Some(&format!("Olá 👩‍💻 {index}")))
            .await
            .unwrap();
    }
    a.close_record_doc(&uid);
    sync(&a, &b, &bo).await;
    assert_eq!(b.doc_text(&uid).await.unwrap().1, "Olá 👩‍💻 299");
    a.write_record_text(&uid, None, Some("After reconnect"))
        .await
        .unwrap();
    let peer = &b;
    a.drain_outbox(|_, _, batch| async move {
        peer.import_op_batch(&batch).await.unwrap();
        Delivery::Failed("acknowledgement lost".into())
    })
    .await
    .unwrap();
    store::sqlx::query("UPDATE sync_outbox SET attempts = 0")
        .execute(&a.store.pool)
        .await
        .unwrap();
    sync(&a, &b, &bo).await;
    assert_eq!(b.doc_text(&uid).await.unwrap().1, "After reconnect");
}

#[tokio::test]
async fn pause_closes_concurrent_observed_work_sessions() {
    let (a, ao) = cell().await;
    let (b, bo) = cell().await;
    pair(&a, &ao, &b, &bo).await;
    let uid = record(&a, "concurrent timers").await;
    sync(&a, &b, &bo).await;
    a.change_record(request(&uid, Mutation::Timer { running: true }), None)
        .await
        .unwrap();
    b.change_record(request(&uid, Mutation::Timer { running: true }), None)
        .await
        .unwrap();
    sync(&b, &a, &ao).await;
    a.change_record(request(&uid, Mutation::Timer { running: false }), None)
        .await
        .unwrap();
    sync(&a, &b, &bo).await;
    for engine in [&a, &b] {
        let work = store::records::get_extension(&engine.store.pool, &uid, "work")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(work["logs"].as_array().unwrap().len(), 2);
        assert!(
            work["logs"]
                .as_array()
                .unwrap()
                .iter()
                .all(|log| log["end"].is_string())
        );
    }
}

async fn cell() -> (Engine, String) {
    let engine = Engine::open_memory().await.unwrap();
    let organ = store::organs::ensure_local(&engine.store.pool, "http://cell")
        .await
        .unwrap()
        .uid;
    engine
        .set_signer(Signer::generate(&organ, "key"))
        .await
        .unwrap();
    (engine, organ)
}

async fn record(engine: &Engine, head: &str) -> String {
    engine
        .act(
            Action::CreateRecord {
                slug: None,
                kind: RecordKind::Plain,
                head: head.into(),
                body: "original".into(),
                quantity: 5.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap()
}

async fn pair(a: &Engine, ao: &str, b: &Engine, bo: &str) {
    b.adopt_introduction(&a.introduction().await.unwrap(), 1)
        .await
        .unwrap();
    a.adopt_introduction(&b.introduction().await.unwrap(), 1)
        .await
        .unwrap();
    store::organs::set_sync_policy(&a.store.pool, bo, true, false)
        .await
        .unwrap();
    store::organs::set_sync_policy(&b.store.pool, ao, true, false)
        .await
        .unwrap();
}

async fn sync(a: &Engine, b: &Engine, bo: &str) {
    a.drain_outbox(|contact, root, batch| async move {
        if contact.record_uid != bo {
            return Delivery::Failed("other contact".into());
        }
        let result = match root {
            Some(root) => b.import_grant_batch(&root, &batch).await,
            None => b.import_op_batch(&batch).await,
        };
        match result {
            Ok(_) if b.batch_is_saved(&batch).await.unwrap() => Delivery::Sent,
            Ok(_) => Delivery::Failed("change was not accepted".into()),
            Err(error) => Delivery::Failed(error.to_string()),
        }
    })
    .await
    .unwrap();
}

fn request(uid: &str, mutation: Mutation) -> Request {
    Request {
        id: nucleus::new_uid("op"),
        record_uid: uid.into(),
        mutation,
    }
}

#[tokio::test]
async fn numbered_assertions_update_in_place_and_sync_without_changing_other_values() {
    let (a, ao) = cell().await;
    let (b, bo) = cell().await;
    pair(&a, &ao, &b, &bo).await;
    let uid = record(&a, "Numbered").await;
    let predicate = store::concepts::ensure(&a.store.pool, "my-number")
        .await
        .unwrap();
    let other = store::concepts::ensure(&a.store.pool, "keep-this")
        .await
        .unwrap();
    let mut assertion_uid = String::new();
    for predicate in [&predicate, &other] {
        let result = a
            .change_record(
                request(
                    &uid,
                    Mutation::Assertion {
                        predicate: predicate.clone(),
                        object: None,
                        quantity: Some("10".into()),
                        unit: None,
                    },
                ),
                None,
            )
            .await
            .unwrap();
        if assertion_uid.is_empty() {
            assertion_uid = result.created.unwrap();
        }
    }
    sync(&a, &b, &bo).await;
    let change = request(
        &uid,
        Mutation::NumberAssertion {
            predicate: predicate.clone(),
            position: 1,
        },
    );
    let result = a.change_record(change.clone(), None).await.unwrap();
    assert_eq!(result.created.as_deref(), Some(assertion_uid.as_str()));
    assert!(!result.facts.is_empty());
    assert!(
        a.change_record(change, None)
            .await
            .unwrap()
            .facts
            .is_empty()
    );
    sync(&a, &b, &bo).await;
    for engine in [&a, &b] {
        let assertions = store::assertions::for_subjects(&engine.store.pool, &[uid.clone()])
            .await
            .unwrap();
        assert_eq!(assertions.len(), 2);
        let numbered = assertions
            .iter()
            .find(|assertion| assertion.predicate_uid == predicate)
            .unwrap();
        assert_eq!(numbered.uid, assertion_uid);
        assert_eq!(numbered.quantity.unwrap().to_string(), "1");
        assert_eq!(
            assertions
                .iter()
                .find(|assertion| assertion.predicate_uid == other)
                .unwrap()
                .quantity
                .unwrap()
                .to_string(),
            "10"
        );
        assert_eq!(quantity(engine, &uid).await, "5");
    }
    let missing = record(&a, "Missing assertion").await;
    a.change_record(
        request(
            &missing,
            Mutation::NumberAssertion {
                predicate: predicate.clone(),
                position: 2,
            },
        ),
        None,
    )
    .await
    .unwrap();
    assert_eq!(
        store::assertions::for_subjects(&a.store.pool, &[missing])
            .await
            .unwrap()[0]
            .quantity
            .unwrap()
            .to_string(),
        "2"
    );
    assert!(
        a.change_record(
            request(
                &uid,
                Mutation::NumberAssertion {
                    predicate,
                    position: 0
                }
            ),
            None
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn numbering_preserves_units_and_identity_assertions() {
    let (engine, _) = cell().await;
    let uid = record(&engine, "Units").await;
    let predicate = store::concepts::ensure(&engine.store.pool, "rank-with-unit")
        .await
        .unwrap();
    let unit = store::concepts::ensure(&engine.store.pool, "hours")
        .await
        .unwrap();
    engine
        .change_record(
            request(
                &uid,
                Mutation::Assertion {
                    predicate: predicate.clone(),
                    object: None,
                    quantity: Some("10".into()),
                    unit: Some(unit.clone()),
                },
            ),
            None,
        )
        .await
        .unwrap();
    assert!(
        engine
            .change_record(
                request(
                    &uid,
                    Mutation::NumberAssertion {
                        predicate: predicate.clone(),
                        position: 1
                    }
                ),
                None
            )
            .await
            .is_err()
    );
    let rows = store::assertions::for_subjects(&engine.store.pool, &[uid.clone()])
        .await
        .unwrap();
    assert_eq!(rows[0].quantity.unwrap().to_string(), "10");
    assert_eq!(rows[0].unit_uid.as_deref(), Some(unit.as_str()));
    let identity = store::concepts::ensure(&engine.store.pool, "identity-rank")
        .await
        .unwrap();
    engine
        .act(
            Action::SetIdentity {
                subject: uid.clone(),
                predicate: Some(identity.clone()),
            },
            None,
        )
        .await
        .unwrap();
    assert!(
        engine
            .change_record(
                request(
                    &uid,
                    Mutation::NumberAssertion {
                        predicate: identity.clone(),
                        position: 1
                    }
                ),
                None
            )
            .await
            .is_err()
    );
    let assertions = store::assertions::list_active(&engine.store.pool)
        .await
        .unwrap();
    assert!(assertions.iter().any(|row| row.subject_uid == uid
        && row.predicate_uid == identity
        && row.role == "identity"
        && row.quantity.is_none()));
    assert!(!assertions.iter().any(|row| row.subject_uid == uid
        && row.predicate_uid == identity
        && row.role == "ordinary"));
}

#[tokio::test]
async fn numbering_requires_permission_for_the_selected_assertion_and_quantity() {
    use protein::authority::{
        AssertionGrant, AssertionProperty, AssertionRole, AssertionTarget, MutationGrant,
        Operation, Property, RolePolicy,
    };
    use std::collections::BTreeSet;
    let (engine, _) = cell().await;
    let uid = record(&engine, "Protected rank").await;
    let person = store::records::create(
        &engine.store.pool,
        store::records::NewRecord {
            slug: None,
            kind: RecordKind::Person,
            head: "Editor",
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .unwrap()
    .uid;
    let role = store::auth::ensure_role(&engine.store.pool, "rank-editor")
        .await
        .unwrap();
    store::auth::compare_and_set_role(&engine.store.pool, &person, Some(role), 0)
        .await
        .unwrap();
    for action in ["read", "update"] {
        let permission = store::auth::ensure_permission(&engine.store.pool, "record", action)
            .await
            .unwrap();
        store::auth::grant(&engine.store.pool, role, permission)
            .await
            .unwrap();
    }
    let predicate = store::concepts::ensure(&engine.store.pool, "protected-rank")
        .await
        .unwrap();
    let assertion = AssertionGrant {
        predicate_uid: predicate.clone(),
        target: AssertionTarget::Unary,
        role: AssertionRole::Ordinary,
        properties: BTreeSet::from([AssertionProperty::Quantity]),
    };
    let mut policy = RolePolicy {
        read: protein::Predicate::All(vec![]),
        grants: vec![MutationGrant {
            operation: Operation::Update,
            selector: protein::Predicate::All(vec![]),
            properties: BTreeSet::from([Property::Head, Property::Body]),
            assertions_add: vec![assertion.clone()],
            assertions_remove: vec![assertion],
        }],
    };
    for (revision, allowed) in [false, true, false].into_iter().enumerate() {
        if allowed {
            policy.grants[0].assertions_add[0]
                .properties
                .insert(AssertionProperty::Quantity);
        } else {
            policy.grants[0].assertions_add[0].properties.clear();
        }
        store::role_policies::set(
            &engine.store.pool,
            role,
            &serde_json::to_value(&policy).unwrap(),
            revision as i64,
        )
        .await
        .unwrap();
        let result = engine
            .change_record(
                request(
                    &uid,
                    Mutation::NumberAssertion {
                        predicate: predicate.clone(),
                        position: (revision + 1) as u32,
                    },
                ),
                Some(&person),
            )
            .await;
        assert_eq!(result.is_ok(), allowed, "{result:?}");
    }
    let rows = store::assertions::for_subjects(&engine.store.pool, &[uid])
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].quantity.unwrap().to_string(), "2");
}

async fn quantity(engine: &Engine, uid: &str) -> String {
    store::records::quantity(&engine.store.pool, uid)
        .await
        .unwrap()
        .unwrap()
        .to_string()
}

#[tokio::test]
async fn simultaneous_assignments_merge_as_one_register_and_preserve_additions() {
    let (a, ao) = cell().await;
    let (b, bo) = cell().await;
    pair(&a, &ao, &b, &bo).await;
    let uid = record(&a, "quantity").await;
    sync(&a, &b, &bo).await;
    for engine in [&a, &b] {
        engine
            .change_record(
                request(
                    &uid,
                    Mutation::Quantity {
                        value: "7.125".into(),
                    },
                ),
                None,
            )
            .await
            .unwrap();
    }
    sync(&a, &b, &bo).await;
    sync(&b, &a, &ao).await;
    assert_eq!(quantity(&a, &uid).await, "7.125");
    assert_eq!(quantity(&b, &uid).await, "7.125");
    a.append_user(&uid, 2.0).await.unwrap();
    b.change_record(
        request(
            &uid,
            Mutation::Quantity {
                value: "9.125".into(),
            },
        ),
        None,
    )
    .await
    .unwrap();
    sync(&a, &b, &bo).await;
    sync(&b, &a, &ao).await;
    assert_eq!(quantity(&a, &uid).await, "11.125");
    assert_eq!(quantity(&b, &uid).await, "11.125");
}

#[tokio::test]
async fn durable_change_identity_does_not_repeat_after_later_changes() {
    let (engine, _) = cell().await;
    let uid = record(&engine, "retries").await;
    let first = request(
        &uid,
        Mutation::Quantity {
            value: "9007199254740993.123456".into(),
        },
    );
    engine.change_record(first.clone(), None).await.unwrap();
    assert_eq!(quantity(&engine, &uid).await, "9007199254740993.123456");
    engine
        .change_record(
            request(&uid, Mutation::Quantity { value: "4".into() }),
            None,
        )
        .await
        .unwrap();
    engine.change_record(first.clone(), None).await.unwrap();
    assert_eq!(quantity(&engine, &uid).await, "4.000000");
    let mut changed = first;
    changed.mutation = Mutation::Quantity { value: "12".into() };
    assert!(engine.change_record(changed, None).await.is_err());
}

#[tokio::test]
async fn independent_work_fields_survive_exchange_in_both_directions() {
    let (a, ao) = cell().await;
    let (b, bo) = cell().await;
    pair(&a, &ao, &b, &bo).await;
    let uid = record(&a, "dates").await;
    sync(&a, &b, &bo).await;
    a.change_record(
        request(
            &uid,
            Mutation::Work {
                field: WorkField::Start,
                value: json!("2026-09-13"),
            },
        ),
        None,
    )
    .await
    .unwrap();
    b.change_record(
        request(
            &uid,
            Mutation::Work {
                field: WorkField::Due,
                value: json!("2026-09-17"),
            },
        ),
        None,
    )
    .await
    .unwrap();
    sync(&a, &b, &bo).await;
    sync(&b, &a, &ao).await;
    for engine in [&a, &b] {
        let work = store::records::get_extension(&engine.store.pool, &uid, "work")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(work["start"], "2026-09-13");
        assert_eq!(work["due"], "2026-09-17");
    }
}

#[tokio::test]
async fn timer_retries_do_not_reopen_a_paused_session() {
    let (engine, _) = cell().await;
    let uid = record(&engine, "timer").await;
    let pause_empty = request(&uid, Mutation::Timer { running: false });
    engine
        .change_record(pause_empty.clone(), None)
        .await
        .unwrap();
    let start = request(&uid, Mutation::Timer { running: true });
    engine.change_record(start.clone(), None).await.unwrap();
    engine.change_record(pause_empty, None).await.unwrap();
    let work = store::records::get_extension(&engine.store.pool, &uid, "work")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(work["logs"].as_array().unwrap().len(), 1);
    assert!(work["logs"][0]["end"].is_null());
    engine
        .change_record(request(&uid, Mutation::Timer { running: false }), None)
        .await
        .unwrap();
    engine.change_record(start, None).await.unwrap();
    let work = store::records::get_extension(&engine.store.pool, &uid, "work")
        .await
        .unwrap()
        .unwrap();
    assert!(work["logs"][0]["end"].is_string());
}

#[tokio::test]
async fn owner_relays_an_accepted_property_change_to_another_participant() {
    let (a, ao) = cell().await;
    let (b, bo) = cell().await;
    let (c, co) = cell().await;
    pair(&a, &ao, &b, &bo).await;
    pair(&a, &ao, &c, &co).await;
    let uid = record(&a, "relay").await;
    sync(&a, &b, &bo).await;
    sync(&a, &c, &co).await;
    b.change_record(
        request(&uid, Mutation::Quantity { value: "8".into() }),
        None,
    )
    .await
    .unwrap();
    sync(&b, &a, &ao).await;
    sync(&a, &c, &co).await;
    assert_eq!(quantity(&a, &uid).await, "8");
    assert_eq!(quantity(&c, &uid).await, "8");
}

#[tokio::test]
async fn a_rejected_operation_cannot_be_acknowledged_as_saved() {
    let (a, ao) = cell().await;
    let (b, bo) = cell().await;
    pair(&a, &ao, &b, &bo).await;
    let uid = record(&a, "admission").await;
    sync(&a, &b, &bo).await;
    a.change_record(
        request(&uid, Mutation::Quantity { value: "8".into() }),
        None,
    )
    .await
    .unwrap();
    let rows = store::sync_ops::for_field(&a.store.pool, "record", &uid, "property:quantity")
        .await
        .unwrap();
    let mut ops = a.hydrate_ops(rows).await.unwrap();
    ops[0].value = Some("not a register".into());
    let batch = OpBatch {
        from_organ: ao,
        ops,
    };
    b.import_op_batch(&batch).await.unwrap();
    assert!(!b.batch_is_saved(&batch).await.unwrap());
    assert_eq!(quantity(&b, &uid).await, "5");
}

#[tokio::test]
async fn slug_claims_converge_and_release_the_next_claimant() {
    let (a, ao) = cell().await;
    let (b, bo) = cell().await;
    pair(&a, &ao, &b, &bo).await;
    let first = record(&a, "first").await;
    let second = record(&a, "second").await;
    sync(&a, &b, &bo).await;
    a.change_record(
        request(
            &first,
            Mutation::Slug {
                value: Some("same-slug".into()),
            },
        ),
        None,
    )
    .await
    .unwrap();
    b.change_record(
        request(
            &second,
            Mutation::Slug {
                value: Some("same-slug".into()),
            },
        ),
        None,
    )
    .await
    .unwrap();
    sync(&a, &b, &bo).await;
    sync(&b, &a, &ao).await;
    for engine in [&a, &b] {
        assert_eq!(
            store::records::get(&engine.store.pool, &second)
                .await
                .unwrap()
                .unwrap()
                .slug
                .as_deref(),
            Some("same-slug")
        );
        assert!(
            store::records::get(&engine.store.pool, &first)
                .await
                .unwrap()
                .unwrap()
                .slug
                .is_none()
        );
    }
    b.change_record(request(&second, Mutation::Slug { value: None }), None)
        .await
        .unwrap();
    sync(&b, &a, &ao).await;
    for engine in [&a, &b] {
        assert_eq!(
            store::records::get(&engine.store.pool, &first)
                .await
                .unwrap()
                .unwrap()
                .slug
                .as_deref(),
            Some("same-slug")
        );
    }
}
