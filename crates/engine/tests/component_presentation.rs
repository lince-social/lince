use engine::{Engine, actions::Action};
use nucleus::component::{ComponentState, RecordMode};
use nucleus::karma::rule_field::RuleFieldInput;

mod support;

#[tokio::test]
async fn fiote_presentations_are_wrapped_and_typed_saved_components_cannot_drop_protection() {
    let engine = support::engine().await;
    let target = support::plain(&engine, "target", 0.0).await;
    let agent = support::person(&engine, "agent").await;
    let thread = engine
        .act(
            Action::CreateThread {
                target: target.clone(),
                head: "Interaction".into(),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let mut pushes = engine.subscribe_components();
    engine::operation_origin::fiote(
        &agent.uid,
        &thread,
        engine.act(
            Action::PresentComponent {
                target,
                component: ComponentState::Text {
                    text: "Review".into(),
                },
            },
            None,
        ),
    )
    .await
    .unwrap();
    let ComponentState::Composition { composition } = pushes.try_recv().unwrap().component else {
        panic!("Fiote presentation needs its mandatory host");
    };
    assert_eq!(composition.origin.as_ref().unwrap().agent, agent.uid);
    assert_eq!(composition.origin.as_ref().unwrap().thread, thread);
    let saved = engine
        .act(
            Action::CreateCustomComponent {
                head: composition.name.clone(),
                body: nucleus::component::Document::encode(composition).unwrap(),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let original = store::records::get(&engine.store.pool, &saved)
        .await
        .unwrap()
        .unwrap()
        .body;
    assert!(engine.act(Action::EditRecordText { target: saved.clone(), head: None, body: Some(serde_json::json!({"format":"lince.custom_component","castle":{"name":"Fiote interaction","parts":[{}]}}).to_string()) }, None).await.is_err());
    assert_eq!(
        store::records::get(&engine.store.pool, &saved)
            .await
            .unwrap()
            .unwrap()
            .body,
        original
    );
    let mut stripped = nucleus::component::Document::decode(&original).unwrap();
    stripped.composition.origin = None;
    assert!(
        engine
            .act(
                Action::EditRecordText {
                    target: saved,
                    head: None,
                    body: Some(nucleus::component::Document::encode(stripped.composition).unwrap())
                },
                None
            )
            .await
            .is_err()
    );
    let invalid = nucleus::component::Composition {
        name: "Invalid".into(),
        origin: None,
        parts: vec![nucleus::component::Part {
            id: "action".into(),
            events: Vec::new(),
            position: [0, 0],
            size: [100, 50],
            component: ComponentState::Button {
                label: "Apply".into(),
                action: serde_json::json!({"action":"not-an-action"}),
            },
        }],
    };
    assert!(
        engine
            .act(
                Action::CreateCustomComponent {
                    head: "Invalid".into(),
                    body: nucleus::component::Document::encode(invalid).unwrap()
                },
                None
            )
            .await
            .is_err()
    );
}

fn field(source: &str) -> RuleFieldInput {
    RuleFieldInput::Text {
        source: source.into(),
    }
}

async fn save(engine: &Engine, rule: Option<String>, revision: Option<i64>) -> String {
    let existing = rule.clone();
    engine.act(Action::SaveKarmaRule {
        identity: None, rule, expected_revision: revision,
        fields: [field("@source"), field(">0"), field("@target: show({\"kind\":\"record\",\"record\":\"shown\",\"mode\":\"call\"})")],
        request_id: nucleus::new_uid("request"),
    }, None).await.unwrap().created.or(existing).unwrap()
}

async fn set(engine: &Engine, value: &str) {
    engine
        .act(
            Action::SetQuantityExact {
                target: "source".into(),
                amount: value.into(),
            },
            None,
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn rule_effects_push_typed_state_without_changing_record_quantities() {
    let engine = support::karma::engine().await;
    support::plain(&engine, "source", 0.0).await;
    let target = support::plain(&engine, "target", 7.0).await;
    let shown = support::plain(&engine, "shown", 3.0).await;
    save(&engine, None, None).await;
    let mut pushes = engine.subscribe_components();
    set(&engine, "1").await;
    assert!(pushes.try_recv().is_err());
    let outcomes = engine.run_due_effects().await.unwrap();
    assert!(outcomes[0].ok, "{}", outcomes[0].result);
    let first = pushes.try_recv().unwrap();
    assert_eq!(first.slot, format!("{target}:record"));
    assert_eq!(
        first.component,
        ComponentState::Record {
            record: shown.clone(),
            mode: RecordMode::Call,
            start_call: None,
        }
    );
    assert!(engine.run_due_effects().await.unwrap().is_empty());
    set(&engine, "2").await;
    assert!(engine.run_due_effects().await.unwrap()[0].ok);
    assert_eq!(pushes.try_recv().unwrap(), first);
    assert_eq!(
        store::facts::level(&engine.store.pool, &target)
            .await
            .unwrap(),
        store::exact::integer(7)
    );
    assert_eq!(
        store::facts::level(&engine.store.pool, &shown)
            .await
            .unwrap(),
        store::exact::integer(3)
    );
}

#[tokio::test]
async fn paused_rules_and_database_only_simulation_never_push_components() {
    let engine = support::karma::engine().await;
    support::plain(&engine, "source", 0.0).await;
    support::plain(&engine, "target", 0.0).await;
    support::plain(&engine, "shown", 0.0).await;
    let uid = save(&engine, None, None).await;
    let mut pushes = engine.subscribe_components();
    set(&engine, "1").await;
    let result = engine.run_database_effects().await.unwrap();
    assert!(result.unsupported && result.pending);
    assert!(pushes.try_recv().is_err());
    let rule = store::recurrence::get(&engine.store.pool, &uid)
        .await
        .unwrap()
        .unwrap();
    engine
        .act(
            Action::SetRecurrencePaused {
                recurrence: uid,
                expected_revision: rule.revision,
                request_id: "pause".into(),
                paused: true,
            },
            None,
        )
        .await
        .unwrap();
    let result = engine.run_due_effects().await.unwrap();
    assert!(result.iter().all(|outcome| !outcome.ok));
    assert!(pushes.try_recv().is_err());
}

#[tokio::test]
async fn editing_after_a_record_rename_keeps_the_original_component_binding() {
    let engine = support::karma::engine().await;
    support::plain(&engine, "source", 0.0).await;
    support::plain(&engine, "target", 0.0).await;
    let shown = support::plain(&engine, "shown", 0.0).await;
    let uid = save(&engine, None, None).await;
    engine
        .act(
            Action::SetSlug {
                target: shown.clone(),
                slug: Some("renamed".into()),
            },
            None,
        )
        .await
        .unwrap();
    support::plain(&engine, "shown", 0.0).await;
    let revision = store::recurrence::get(&engine.store.pool, &uid)
        .await
        .unwrap()
        .unwrap()
        .revision;
    save(&engine, Some(uid), Some(revision)).await;
    let mut pushes = engine.subscribe_components();
    set(&engine, "1").await;
    assert!(engine.run_due_effects().await.unwrap()[0].ok);
    assert_eq!(
        pushes.try_recv().unwrap().component.record(),
        Some(shown.as_str())
    );
}

#[tokio::test]
async fn deleted_components_and_unavailable_interfaces_are_explicit_failures() {
    let engine = support::engine().await;
    let target = support::plain(&engine, "target", 0.0).await;
    let action = Action::PresentComponent {
        target: target.clone(),
        component: ComponentState::Text {
            text: "Hello".into(),
        },
    };
    assert!(
        engine
            .act(action, None)
            .await
            .unwrap_err()
            .to_string()
            .contains("No native interface")
    );
    let mut pushes = engine.subscribe_components();
    engine
        .act(
            Action::DeleteRecord {
                target: target.clone(),
            },
            None,
        )
        .await
        .unwrap();
    assert!(
        engine
            .act(
                Action::PresentComponent {
                    target: target.clone(),
                    component: ComponentState::Record {
                        record: target,
                        mode: RecordMode::Full,
                        start_call: None,
                    }
                },
                None
            )
            .await
            .is_err()
    );
    assert!(pushes.try_recv().is_err());
}

#[tokio::test]
async fn hidden_bound_records_cannot_be_pushed_by_an_actor() {
    let engine = support::engine().await;
    let target = support::plain(&engine, "target", 0.0).await;
    let secret = support::plain(&engine, "secret", 0.0).await;
    let person = support::person(&engine, "viewer").await;
    let role = store::auth::ensure_role(&engine.store.pool, "component-viewer")
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
    store::auth::create_credential(
        &engine.store.pool,
        &person.uid,
        "component-viewer",
        "hash",
        role,
    )
    .await
    .unwrap();
    store::visibility::grant(&engine.store.pool, "actor", Some(&person.uid), &target)
        .await
        .unwrap();
    let mut pushes = engine.subscribe_components();
    engine
        .act(
            Action::PresentComponent {
                target: target.clone(),
                component: ComponentState::Text {
                    text: "Visible reminder".into(),
                },
            },
            Some(person.uid.clone()),
        )
        .await
        .unwrap();
    assert!(pushes.try_recv().is_ok());
    assert!(
        engine
            .act(
                Action::PresentComponent {
                    target,
                    component: ComponentState::Composition {
                        composition: nucleus::component::Composition {
                            name: "Nested restricted view".into(),
                            origin: None,
                            parts: vec![nucleus::component::Part {
                                id: "secret".into(),
                                position: [0, 0],
                                size: [480, 320],
                                events: Vec::new(),
                                component: ComponentState::Record {
                                    record: secret,
                                    mode: RecordMode::Full,
                                    start_call: None,
                                },
                            }],
                        },
                    }
                },
                Some(person.uid)
            )
            .await
            .is_err()
    );
    assert!(pushes.try_recv().is_err());
}

async fn automatic_call_fixture() -> (Engine, String, String, String) {
    let engine = support::karma::engine().await;
    support::plain(&engine, "source", 0.0).await;
    let shown = support::plain(&engine, "shown", 3.0).await;
    let person = support::person(&engine, "caller").await.uid;
    let thread = engine
        .act(
            Action::CreateThread {
                target: shown.clone(),
                head: "Call".into(),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    engine
        .act(
            Action::SetSlug {
                target: thread.clone(),
                slug: Some("call-thread".into()),
            },
            None,
        )
        .await
        .unwrap();
    let role = store::auth::ensure_role(&engine.store.pool, "automatic-call-person")
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
    store::auth::create_credential(
        &engine.store.pool,
        &person,
        "automatic-call-person",
        "hash",
        role,
    )
    .await
    .unwrap();
    for uid in [&shown, &thread, &person] {
        store::visibility::grant(&engine.store.pool, "actor", Some(&person), uid)
            .await
            .unwrap();
    }
    (engine, shown, thread, person)
}

async fn save_automatic_call(
    engine: &Engine,
    rule: Option<String>,
    revision: Option<i64>,
) -> String {
    let existing = rule.clone();
    engine.act(Action::SaveKarmaRule {
        identity: None, rule, expected_revision: revision,
        fields: [field("@source"), field(">0"), field("@shown: show({\"kind\":\"record\",\"mode\":\"call\",\"start_call\":{\"thread\":\"call-thread\",\"person\":\"caller\"}})")],
        request_id: nucleus::new_uid("request"),
    }, None).await.unwrap().created.or(existing).unwrap()
}

#[tokio::test]
async fn automatic_call_rule_binds_thread_and_person_and_preserves_them_after_renames() {
    let (engine, shown, thread, person) = automatic_call_fixture().await;
    let rule = save_automatic_call(&engine, None, None).await;
    for (uid, slug) in [(&thread, "renamed-thread"), (&person, "renamed-caller")] {
        engine
            .act(
                Action::SetSlug {
                    target: uid.clone(),
                    slug: Some(slug.into()),
                },
                None,
            )
            .await
            .unwrap();
    }
    support::plain(&engine, "call-thread", 0.0).await;
    support::person(&engine, "caller").await;
    let revision = store::recurrence::get(&engine.store.pool, &rule)
        .await
        .unwrap()
        .unwrap()
        .revision;
    save_automatic_call(&engine, Some(rule), Some(revision)).await;
    let mut pushes = engine.subscribe_components();
    set(&engine, "1").await;
    let result = engine.run_database_effects().await.unwrap();
    assert!(result.unsupported && result.pending);
    assert!(pushes.try_recv().is_err());
    assert!(engine.run_due_effects().await.unwrap()[0].ok);
    let ComponentState::Record {
        record,
        start_call: Some(start),
        ..
    } = pushes.try_recv().unwrap().component
    else {
        panic!()
    };
    assert_eq!(record, shown);
    assert_eq!(start.thread, thread);
    assert_eq!(start.person, person);
    assert_eq!(start.media, nucleus::component::CallMedia::Audio);
}

#[tokio::test]
async fn automatic_calls_refuse_unrelated_threads_invalid_people_and_denied_access() {
    let (engine, shown, thread, person) = automatic_call_fixture().await;
    let other = support::plain(&engine, "other", 0.0).await;
    let mut pushes = engine.subscribe_components();
    let component = |record: &str, person: &str| ComponentState::Record {
        record: record.into(),
        mode: RecordMode::Call,
        start_call: Some(nucleus::component::CallStart {
            thread: thread.clone(),
            person: person.into(),
            media: Default::default(),
        }),
    };
    for state in [component(&other, &person), component(&shown, &other)] {
        assert!(
            engine
                .act(
                    Action::PresentComponent {
                        target: shown.clone(),
                        component: state
                    },
                    None
                )
                .await
                .is_err()
        );
    }
    engine
        .set_read_filter(&person, Some(&protein::Predicate::UidEq(shown.clone())))
        .await
        .unwrap();
    assert!(
        engine
            .act(
                Action::PresentComponent {
                    target: shown.clone(),
                    component: component(&shown, &person)
                },
                Some(person.clone())
            )
            .await
            .is_err()
    );
    assert!(pushes.try_recv().is_err());
    engine.set_read_filter(&person, None).await.unwrap();
    let other_person = support::person(&engine, "other-caller").await.uid;
    store::visibility::grant(&engine.store.pool, "actor", Some(&person), &other_person)
        .await
        .unwrap();
    let refusal = engine
        .act(
            Action::PresentComponent {
                target: shown.clone(),
                component: component(&shown, &other_person),
            },
            Some(person),
        )
        .await
        .unwrap_err();
    assert!(refusal.to_string().contains("another person's identity"));
    assert!(pushes.try_recv().is_err());
}
