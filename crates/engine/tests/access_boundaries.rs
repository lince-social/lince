use engine::{Engine, actions::Action};
use nucleus::{RecordKind, component::ComponentState};
use serde_json::{Value, json};

mod support;

async fn actor(engine: &Engine, name: &str, keys: &[&str]) -> (String, i64) {
    let person = support::person(engine, name).await.uid;
    let role = store::auth::ensure_role(&engine.store.pool, name)
        .await
        .unwrap();
    for key in keys {
        let (resource, operation) = key.split_once(':').unwrap();
        let permission = store::auth::ensure_permission(&engine.store.pool, resource, operation)
            .await
            .unwrap();
        store::auth::grant(&engine.store.pool, role, permission)
            .await
            .unwrap();
    }
    engine
        .act(
            Action::AssignRoles {
                person: person.clone(),
                roles: vec![name.into()],
                expected_revision: 0,
            },
            None,
        )
        .await
        .unwrap();
    (person, role)
}

async fn policy(engine: &Engine, name: &str, revision: i64, read: Value, grants: Vec<Value>) {
    engine
        .act(
            Action::SetRolePolicy {
                role: name.into(),
                expected_revision: revision,
                policy: json!({"read":read,"grants":grants}),
            },
            None,
        )
        .await
        .unwrap();
}

fn update(selector: Value, properties: Value, add: Vec<Value>, remove: Vec<Value>) -> Value {
    json!({"operation":"update","selector":selector,"properties":properties,"assertions_add":add,"assertions_remove":remove})
}

fn assertion(predicate: &str, target: Value, identity: bool) -> Value {
    json!({"predicate_uid":predicate,"target":target,"role":if identity { "identity" } else { "ordinary" },"properties":[]})
}

async fn count(engine: &Engine, table: &str) -> i64 {
    let query = match table {
        "place" => "SELECT COUNT(*) FROM place",
        "assertion" => "SELECT COUNT(*) FROM record_assertion",
        "fact" => "SELECT COUNT(*) FROM fact",
        "sync" => "SELECT COUNT(*) FROM sync_op",
        "visibility" => "SELECT COUNT(*) FROM visibility_rule",
        "effect" => "SELECT COUNT(*) FROM effect_queue",
        "application" => "SELECT COUNT(*) FROM karma_rule_application",
        _ => panic!("Unknown count"),
    };
    store::sqlx::query_scalar(query)
        .fetch_one(&engine.store.pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn visibility_and_ontology_require_access_management_authority() {
    let engine = Engine::open_memory().await.unwrap();
    let record = support::plain(&engine, "visible", 0.0).await;
    let (person, role) = actor(
        &engine,
        "editor",
        &["record:read", "record:update", "record:create"],
    )
    .await;
    let before = count(&engine, "visibility").await;
    let grant = Action::GrantVisibility {
        subject_kind: "public".into(),
        subject: None,
        target: record.clone(),
    };
    assert!(
        engine
            .act(grant.clone(), Some(person.clone()))
            .await
            .is_err()
    );
    assert_eq!(count(&engine, "visibility").await, before);
    let concept = Action::CreateConcept {
        name: "access-sensitive".into(),
        parents: vec![],
        lingua: store::linguas::LOCAL_UID.into(),
    };
    assert!(
        engine
            .act(concept.clone(), Some(person.clone()))
            .await
            .is_err()
    );
    assert!(
        store::concepts::resolve(&engine.store.pool, "access-sensitive")
            .await
            .unwrap()
            .is_none()
    );
    let permission = store::auth::ensure_permission(&engine.store.pool, "permission", "assign")
        .await
        .unwrap();
    store::auth::grant(&engine.store.pool, role, permission)
        .await
        .unwrap();
    engine.act(grant, Some(person.clone())).await.unwrap();
    engine.act(concept, Some(person)).await.unwrap();
    assert_eq!(count(&engine, "visibility").await, before + 1);
}

#[tokio::test]
async fn native_presentations_cannot_borrow_the_local_interfaces_authority() {
    let engine = Engine::open_memory().await.unwrap();
    let record = support::plain(&engine, "presented", 0.0).await;
    let (person, _) = actor(&engine, "presenter", &["record:read", "record:update"]).await;
    let mut receiver = engine.subscribe_components();
    engine
        .act(
            Action::PresentComponent {
                target: record.clone(),
                component: ComponentState::Text {
                    text: "A permitted reminder".into(),
                },
            },
            Some(person.clone()),
        )
        .await
        .unwrap();
    assert!(receiver.try_recv().is_ok());
    let action = Action::PresentComponent {
        target: record.clone(),
        component: ComponentState::Button {
            label: "Apply".into(),
            action: serde_json::to_value(Action::SetQuantityExact {
                target: record,
                amount: "42".into(),
            })
            .unwrap(),
        },
    };
    assert!(
        engine
            .act(action.clone(), Some(person))
            .await
            .unwrap_err()
            .to_string()
            .contains("local interface")
    );
    assert!(receiver.try_recv().is_err());
    engine.act(action, None).await.unwrap();
    assert!(receiver.try_recv().is_ok());
}

#[tokio::test]
async fn saved_components_check_their_declared_button_and_event_authority() {
    let engine = Engine::open_memory().await.unwrap();
    let record = support::plain(&engine, "component-target", 0.0).await;
    let (person, _) = actor(&engine, "designer", &["record:read", "record:create"]).await;
    let action = serde_json::to_value(Action::SetQuantityExact {
        target: record,
        amount: "42".into(),
    })
    .unwrap();
    for events in [false, true] {
        let composition = nucleus::component::Composition {
            name: "Control".into(),
            origin: None,
            parts: vec![nucleus::component::Part {
                settings: Default::default(),
                id: "control".into(),
                position: [0, 0],
                size: [100, 50],
                component: if events {
                    ComponentState::Text {
                        text: "Control".into(),
                    }
                } else {
                    ComponentState::Button {
                        label: "Apply".into(),
                        action: action.clone(),
                    }
                },
                events: if events {
                    vec![nucleus::component::EventBinding {
                        event: "activate".into(),
                        action: action.clone(),
                    }]
                } else {
                    vec![]
                },
            }],
        };
        let command = Action::CreateCustomComponent {
            head: composition.name.clone(),
            body: nucleus::component::Document::encode(composition).unwrap(),
        };
        let before = count(&engine, "fact").await;
        assert!(engine.act(command, Some(person.clone())).await.is_err());
        assert_eq!(count(&engine, "fact").await, before);
    }
}

#[tokio::test]
async fn identity_refinement_and_tuple_retraction_obey_separate_assertion_grants() {
    let engine = Engine::open_memory().await.unwrap();
    let record = support::plain(&engine, "subject", 0.0).await;
    let object = support::plain(&engine, "object", 0.0).await;
    let (person, _) = actor(&engine, "relations", &["record:read", "record:update"]).await;
    let predicate = store::concepts::ensure(&engine.store.pool, "refined")
        .await
        .unwrap();
    let identity = store::concepts::ensure(&engine.store.pool, "identity")
        .await
        .unwrap();
    let unary = assertion(&predicate, json!("unary"), false);
    let binary = assertion(&predicate, json!({"record":object}), false);
    let identity_rule = assertion(&identity, json!("unary"), true);
    let asserted = engine
        .act(
            Action::AssertRecord {
                subject: record.clone(),
                predicate: predicate.clone(),
                object: None,
                quantity: None,
                unit: None,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    policy(
        &engine,
        "relations",
        0,
        json!({"all":[]}),
        vec![update(
            json!({"uid_eq":record}),
            json!([]),
            vec![binary.clone(), identity_rule.clone()],
            vec![],
        )],
    )
    .await;
    let refine = Action::RefineAssertion {
        subject: record.clone(),
        predicate: predicate.clone(),
        object: object.clone(),
    };
    let before = count(&engine, "sync").await;
    assert!(
        engine
            .act(refine.clone(), Some(person.clone()))
            .await
            .is_err()
    );
    assert_eq!(count(&engine, "sync").await, before);
    assert!(
        store::assertions::get(&engine.store.pool, &asserted)
            .await
            .unwrap()
            .unwrap()
            .retracted_at
            .is_none()
    );
    policy(
        &engine,
        "relations",
        1,
        json!({"all":[]}),
        vec![update(
            json!({"uid_eq":record}),
            json!([]),
            vec![binary.clone(), identity_rule],
            vec![unary, binary],
        )],
    )
    .await;
    engine.act(refine, Some(person.clone())).await.unwrap();
    engine
        .act(
            Action::SetIdentity {
                subject: record.clone(),
                predicate: Some(identity),
            },
            Some(person.clone()),
        )
        .await
        .unwrap();
    engine
        .act(
            Action::RetractRecord {
                subject: record,
                predicate,
                object: Some(object.clone()),
            },
            Some(person.clone()),
        )
        .await
        .unwrap();
    let object_facts: i64 =
        store::sqlx::query_scalar("SELECT COUNT(*) FROM fact WHERE record_uid=? AND actor_uid=?")
            .bind(object)
            .bind(person)
            .fetch_one(&engine.store.pool)
            .await
            .unwrap();
    assert_eq!(object_facts, 0);
}

#[tokio::test]
async fn identity_change_rolls_back_when_it_would_remove_read_access() {
    let engine = Engine::open_memory().await.unwrap();
    let record = support::plain(&engine, "identified", 0.0).await;
    let (person, _) = actor(&engine, "identities", &["record:read", "record:update"]).await;
    let predicate = store::concepts::ensure(&engine.store.pool, "visible-identity")
        .await
        .unwrap();
    engine
        .act(
            Action::SetIdentity {
                subject: record.clone(),
                predicate: Some(predicate.clone()),
            },
            None,
        )
        .await
        .unwrap();
    policy(
        &engine,
        "identities",
        0,
        json!({"concept_in":predicate}),
        vec![update(
            json!({"all":[]}),
            json!([]),
            vec![],
            vec![assertion(&predicate, json!("unary"), true)],
        )],
    )
    .await;
    let before = (count(&engine, "fact").await, count(&engine, "sync").await);
    assert!(
        engine
            .act(
                Action::SetIdentity {
                    subject: record.clone(),
                    predicate: None
                },
                Some(person)
            )
            .await
            .is_err()
    );
    assert_eq!(
        (count(&engine, "fact").await, count(&engine, "sync").await),
        before
    );
    assert_eq!(
        store::records::get(&engine.store.pool, &record)
            .await
            .unwrap()
            .unwrap()
            .identity_predicate_uid,
        Some(predicate)
    );
}

#[tokio::test]
async fn ordering_refuses_a_partial_write_and_rolls_back_all_assertions() {
    let engine = Engine::open_memory().await.unwrap();
    let a = support::plain(&engine, "a", 0.0).await;
    let b = support::plain(&engine, "b", 0.0).await;
    let (person, _) = actor(&engine, "order-editor", &["record:read", "record:update"]).await;
    let predicate = store::concepts::ensure(&engine.store.pool, "ordered-before")
        .await
        .unwrap();
    engine
        .act(
            Action::SetAssertionOrder {
                predicate: predicate.clone(),
                ordered: vec![a.clone(), b.clone()],
                reverse: false,
            },
            None,
        )
        .await
        .unwrap();
    let rule = assertion(&predicate, json!("any_readable_record"), false);
    policy(
        &engine,
        "order-editor",
        0,
        json!({"all":[]}),
        vec![update(
            json!({"uid_eq":a}),
            json!([]),
            vec![rule.clone()],
            vec![rule.clone()],
        )],
    )
    .await;
    let before = (
        count(&engine, "assertion").await,
        count(&engine, "sync").await,
        count(&engine, "fact").await,
    );
    let action = Action::SetAssertionOrder {
        predicate: predicate.clone(),
        ordered: vec![a.clone(), b.clone()],
        reverse: true,
    };
    assert!(
        engine
            .act(action.clone(), Some(person.clone()))
            .await
            .is_err()
    );
    assert_eq!(
        (
            count(&engine, "assertion").await,
            count(&engine, "sync").await,
            count(&engine, "fact").await
        ),
        before
    );
    policy(
        &engine,
        "order-editor",
        1,
        json!({"all":[]}),
        vec![update(
            json!({"any":[{"uid_eq":a},{"uid_eq":b}]}),
            json!([]),
            vec![rule.clone()],
            vec![rule],
        )],
    )
    .await;
    engine.act(action, Some(person)).await.unwrap();
}

#[tokio::test]
async fn place_edits_require_the_property_and_do_not_leave_orphans_on_refusal() {
    let engine = Engine::open_memory().await.unwrap();
    let record = support::plain(&engine, "located", 0.0).await;
    let (person, _) = actor(&engine, "place-editor", &["record:read", "record:update"]).await;
    policy(
        &engine,
        "place-editor",
        0,
        json!({"all":[]}),
        vec![update(
            json!({"uid_eq":record}),
            json!(["head"]),
            vec![],
            vec![],
        )],
    )
    .await;
    let action = Action::SetPlace {
        target: record.clone(),
        lat: 1.0,
        lon: 2.0,
        address: Some("Here".into()),
    };
    let before = count(&engine, "place").await;
    assert!(
        engine
            .act(action.clone(), Some(person.clone()))
            .await
            .is_err()
    );
    assert_eq!(count(&engine, "place").await, before);
    policy(
        &engine,
        "place-editor",
        1,
        json!({"all":[]}),
        vec![update(
            json!({"uid_eq":record}),
            json!(["place"]),
            vec![],
            vec![],
        )],
    )
    .await;
    engine.act(action, Some(person.clone())).await.unwrap();
    assert_eq!(count(&engine, "place").await, before + 1);
    assert!(
        engine
            .act(
                Action::SetPlace {
                    target: record,
                    lat: 100.0,
                    lon: 0.0,
                    address: None
                },
                Some(person)
            )
            .await
            .is_err()
    );
    assert_eq!(count(&engine, "place").await, before + 1);
}

async fn recurrence(
    engine: &Engine,
    record: &str,
    consequence: nucleus::karma::Consequence,
) -> String {
    engine
        .act(
            Action::CreateRecurrence {
                target: record.into(),
                consequences: vec![consequence],
                condition: None,
                gate: None,
                carry: None,
                note: None,
                cadence: nucleus::karma::Cadence::every_months(1),
                anchor_at: Some("2026-01-01T00:00:00Z".into()),
                request_id: None,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap()
}

fn occurrence(rule: &str) -> Action {
    Action::ApplyRecurrenceOccurrence {
        recurrence: rule.into(),
        due_at: "2026-02-01T00:00:00Z".into(),
        amount: None,
        note: None,
    }
}

#[tokio::test]
async fn manual_occurrences_do_not_execute_with_the_rule_authors_record_authority() {
    let engine = support::karma::engine().await;
    let record = support::plain(&engine, "rule-target", 0.0).await;
    let rule = recurrence(
        &engine,
        &record,
        nucleus::karma::Consequence::SetQuantity {
            value: Some(store::exact::one()),
        },
    )
    .await;
    let (person, _) = actor(&engine, "scheduler", &["record:read", "frequency:update"]).await;
    assert!(engine.act(occurrence(&rule), Some(person)).await.is_err());
    assert!(
        store::records::get(&engine.store.pool, &record)
            .await
            .unwrap()
            .unwrap()
            .quantity
            .is_zero()
    );
    assert_eq!(count(&engine, "application").await, 0);
}

#[tokio::test]
async fn automatic_quantity_changes_check_the_proposed_state_in_their_transaction() {
    let engine = support::karma::engine().await;
    let record = support::plain(&engine, "bounded-target", 0.0).await;
    let rule = recurrence(
        &engine,
        &record,
        nucleus::karma::Consequence::SetQuantity {
            value: Some(nucleus::DecimalValue::parse_inferred("2").unwrap()),
        },
    )
    .await;
    let (person, _) = actor(
        &engine,
        "bounded-scheduler",
        &["record:read", "record:update", "frequency:update"],
    )
    .await;
    policy(
        &engine,
        "bounded-scheduler",
        0,
        json!({"all":[]}),
        vec![update(
            json!({"all":[{"uid_eq":record},{"quantity_lte":"1"}]}),
            json!(["quantity"]),
            vec![],
            vec![],
        )],
    )
    .await;
    let before = count(&engine, "fact").await;
    assert!(engine.act(occurrence(&rule), Some(person)).await.is_err());
    assert!(
        store::records::get(&engine.store.pool, &record)
            .await
            .unwrap()
            .unwrap()
            .quantity
            .is_zero()
    );
    assert_eq!(count(&engine, "fact").await, before);
    assert_eq!(count(&engine, "application").await, 0);
    assert_eq!(count(&engine, "effect").await, 0);
}

#[tokio::test]
async fn deferred_identity_changes_keep_the_manual_invoker_and_recheck_revocation() {
    let engine = support::karma::engine().await;
    let record = support::plain(&engine, "deferred-target", 0.0).await;
    let concept = store::concepts::ensure(&engine.store.pool, "deferred-identity")
        .await
        .unwrap();
    let rule = recurrence(
        &engine,
        &record,
        nucleus::karma::Consequence::SetConcept {
            concept: concept.clone(),
        },
    )
    .await;
    let (person, role) = actor(
        &engine,
        "invoker",
        &["record:read", "record:update", "frequency:update"],
    )
    .await;
    engine
        .act(occurrence(&rule), Some(person.clone()))
        .await
        .unwrap();
    let permission = store::auth::ensure_permission(&engine.store.pool, "record", "update")
        .await
        .unwrap();
    store::auth::revoke(&engine.store.pool, role, permission)
        .await
        .unwrap();
    let outcomes = engine.run_due_effects().await.unwrap();
    assert!(!outcomes.is_empty());
    assert!(outcomes.iter().all(|outcome| !outcome.ok));
    assert!(
        store::records::get(&engine.store.pool, &record)
            .await
            .unwrap()
            .unwrap()
            .identity_predicate_uid
            .is_none()
    );
    let authors: Vec<Option<String>> =
        store::sqlx::query_scalar("SELECT json_extract(payload,'$.actor') FROM effect_queue")
            .fetch_all(&engine.store.pool)
            .await
            .unwrap();
    assert!(
        authors
            .iter()
            .all(|actor| actor.as_deref() == Some(&person))
    );
}

#[tokio::test]
async fn resuming_automation_requires_record_authority_and_records_the_new_executor() {
    let engine = support::karma::engine().await;
    let record = support::plain(&engine, "resumed-target", 0.0).await;
    let rule = recurrence(
        &engine,
        &record,
        nucleus::karma::Consequence::SetQuantity {
            value: Some(store::exact::one()),
        },
    )
    .await;
    engine
        .act(
            Action::SetRecurrencePaused {
                recurrence: rule.clone(),
                expected_revision: 1,
                request_id: "pause".into(),
                paused: true,
            },
            None,
        )
        .await
        .unwrap();
    let (person, role) = actor(&engine, "resumer", &["record:read", "frequency:update"]).await;
    let resume = Action::SetRecurrencePaused {
        recurrence: rule.clone(),
        expected_revision: 2,
        request_id: "resume".into(),
        paused: false,
    };
    assert!(
        engine
            .act(resume.clone(), Some(person.clone()))
            .await
            .is_err()
    );
    assert!(
        store::recurrence::get(&engine.store.pool, &rule)
            .await
            .unwrap()
            .unwrap()
            .is_paused()
    );
    let permission = store::auth::ensure_permission(&engine.store.pool, "record", "update")
        .await
        .unwrap();
    store::auth::grant(&engine.store.pool, role, permission)
        .await
        .unwrap();
    engine.act(resume, Some(person.clone())).await.unwrap();
    assert_eq!(
        store::recurrence::get(&engine.store.pool, &rule)
            .await
            .unwrap()
            .unwrap()
            .actor_uid,
        Some(person)
    );
}

#[tokio::test]
async fn deferred_queries_filter_results_using_the_manual_invokers_read_policy() {
    let engine = support::karma::engine().await;
    let secret = support::plain(&engine, "private-query-result", 0.0).await;
    let record = support::plain(&engine, "query-rule-target", 0.0).await;
    let query = engine
        .act(
            Action::SaveProtein {
                slug: "deferred-private-query".into(),
                head: "Private query".into(),
                ast: json!({"source":"record","where":[{"all":[{"uid_eq":secret}]}]}),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let rule = recurrence(
        &engine,
        &record,
        nucleus::karma::Consequence::RunQuery {
            query: query.clone(),
            params: None,
        },
    )
    .await;
    let (person, _) = actor(
        &engine,
        "query-invoker",
        &["record:read", "record:update", "frequency:update"],
    )
    .await;
    policy(
        &engine,
        "query-invoker",
        0,
        json!({"any":[{"uid_eq":record},{"uid_eq":query}]}),
        vec![update(
            json!({"uid_eq":record}),
            json!(["quantity"]),
            vec![],
            vec![],
        )],
    )
    .await;
    assert_eq!(
        protein::execute_saved(&engine.store, &query, None)
            .await
            .unwrap()
            .len(),
        1
    );
    engine.act(occurrence(&rule), Some(person)).await.unwrap();
    let mut outcomes = engine.run_due_effects().await.unwrap();
    assert!(outcomes.iter().all(|outcome| outcome.ok), "{outcomes:?}");
    outcomes.extend(engine.run_due_effects().await.unwrap());
    let outcome = outcomes
        .iter()
        .find(|outcome| outcome.kind == "query")
        .unwrap();
    assert!(outcome.ok, "{}", outcome.result);
    assert_eq!(outcome.result, "0 rows");
}

#[tokio::test]
async fn managed_record_kinds_require_their_dedicated_creation_service() {
    let engine = Engine::open_memory().await.unwrap();
    let (person, _) = actor(&engine, "plain-editor", &["record:read", "record:create"]).await;
    let before = count(&engine, "fact").await;
    let result = engine
        .act(
            Action::CreateRecord {
                slug: None,
                kind: RecordKind::Organ,
                head: "Unauthorized Organ".into(),
                body: "".into(),
                quantity: 0.0,
            },
            Some(person),
        )
        .await;
    assert!(result.is_err());
    assert_eq!(count(&engine, "fact").await, before);
}

#[tokio::test]
async fn generic_extensions_cannot_change_person_standing() {
    let engine = Engine::open_memory().await.unwrap();
    let (person, _) = actor(
        &engine,
        "profile-editor",
        &["record:read", "record:update", "user:update_self"],
    )
    .await;
    let before = count(&engine, "fact").await;
    assert!(
        engine
            .act(
                Action::SetExtension {
                    target: person.clone(),
                    namespace: store::people::NAMESPACE.into(),
                    fds: json!({"standing":{"active":false}})
                },
                Some(person.clone())
            )
            .await
            .is_err()
    );
    assert!(
        store::people::is_active(&engine.store.pool, &person)
            .await
            .unwrap()
    );
    assert_eq!(count(&engine, "fact").await, before);
}

#[tokio::test]
async fn ordinary_relationship_edits_cannot_change_another_records_workspace_admission() {
    let engine = Engine::open_memory().await.unwrap();
    let subject = support::plain(&engine, "admission-source", 0.0).await;
    let other = support::plain(&engine, "admission-dependent", 0.0).await;
    let concept = store::concepts::ensure(&engine.store.pool, "admission-link")
        .await
        .unwrap();
    let (person, _) = actor(
        &engine,
        "admission-editor",
        &["record:read", "record:update"],
    )
    .await;
    policy(
        &engine,
        "admission-editor",
        0,
        json!({"all":[]}),
        vec![update(
            json!({"uid_eq":subject}),
            json!([]),
            vec![assertion(&concept, json!({"record":other}), false)],
            vec![],
        )],
    )
    .await;
    engine.act(Action::Workspace { request: engine::workspace_sync::Request { client: Default::default(), command: engine::workspace_sync::Command::Create { name: "Bounded admission".into(), policy: json!({"required_capabilities":[],"ceiling":{"read":{"relation":{"kind":concept,"direction":"in","other":subject}},"grants":[]}}) } } }, None).await.unwrap();
    let before = count(&engine, "assertion").await;
    assert!(
        engine
            .act(
                Action::AssertRecord {
                    subject,
                    predicate: concept,
                    object: Some(other),
                    quantity: None,
                    unit: None
                },
                Some(person)
            )
            .await
            .is_err()
    );
    assert_eq!(count(&engine, "assertion").await, before);
}

#[tokio::test]
async fn hidden_and_missing_records_have_the_same_action_refusal() {
    let engine = Engine::open_memory().await.unwrap();
    let hidden = support::plain(&engine, "hidden-target", 0.0).await;
    let (person, _) = actor(
        &engine,
        "restricted-reader",
        &["record:read", "record:update"],
    )
    .await;
    policy(
        &engine,
        "restricted-reader",
        0,
        json!({"uid_eq":person}),
        vec![update(json!({"all":[]}), json!(["head"]), vec![], vec![])],
    )
    .await;
    let mut errors = Vec::new();
    for target in [hidden, nucleus::new_uid("r")] {
        errors.push(
            engine
                .act(
                    Action::EditRecordText {
                        target,
                        head: Some("Changed".into()),
                        body: None,
                    },
                    Some(person.clone()),
                )
                .await
                .unwrap_err()
                .to_string(),
        );
    }
    assert_eq!(errors[0], errors[1]);
}

#[tokio::test]
async fn call_context_does_not_list_contacts_outside_the_callers_read_policy() {
    let engine = Engine::open_memory().await.unwrap();
    let record = support::plain(&engine, "call-target", 0.0).await;
    let thread = engine
        .act(
            Action::CreateThread {
                target: record,
                head: "Call".into(),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let hidden = nucleus::new_uid("r");
    store::organs::add_contact(
        &engine.store.pool,
        &hidden,
        None,
        "Private contact",
        "http://private.test",
        1,
    )
    .await
    .unwrap();
    store::organs::set_node_id(&engine.store.pool, &hidden, Some("private-node"))
        .await
        .unwrap();
    let (person, _) = actor(&engine, "caller", &["record:read", "record:create"]).await;
    policy(&engine, "caller", 0, json!({"uid_eq":thread}), vec![]).await;
    let context = engine.call_context(&thread, Some(&person)).await.unwrap();
    assert!(context.organs.is_empty());
    assert!(
        engine
            .call_context(&thread, None)
            .await
            .unwrap()
            .organs
            .iter()
            .any(|(uid, _)| uid == &hidden)
    );
}

#[tokio::test]
async fn record_grants_do_not_delegate_the_hosts_fiote_execution_authority() {
    let engine = Engine::open_memory().await.unwrap();
    let record = support::plain(&engine, "fiote-target", 0.0).await;
    let (person, _) = actor(
        &engine,
        "automation-editor",
        &["record:read", "record:update"],
    )
    .await;
    assert!(
        engine
            .act(
                Action::ActivateFiote {
                    target: record.clone(),
                    value: "1".into(),
                    request_id: "untrusted-trigger".into()
                },
                Some(person.clone())
            )
            .await
            .unwrap_err()
            .to_string()
            .contains("local interface")
    );
    engine
        .act(
            Action::ConfigureFiote {
                target: record.clone(),
                prompt_parent: None,
                run_assigned: true,
            },
            None,
        )
        .await
        .unwrap();
    assert!(
        engine
            .act(
                Action::EditRecordText {
                    target: record.clone(),
                    head: None,
                    body: Some("Execute a privileged change".into())
                },
                Some(person.clone())
            )
            .await
            .is_err()
    );
    assert!(
        engine
            .record_text_permissions(Some(&person), &record)
            .await
            .unwrap()
            .is_empty()
    );
    let task = support::plain(&engine, "untrusted-task", 0.0).await;
    assert!(
        engine
            .act(
                Action::AssertRecord {
                    subject: task.clone(),
                    predicate: "assigned-to".into(),
                    object: Some(record.clone()),
                    quantity: None,
                    unit: None
                },
                Some(person.clone())
            )
            .await
            .is_err()
    );
    engine
        .act(
            Action::AssertRecord {
                subject: task.clone(),
                predicate: "assigned-to".into(),
                object: Some(record),
                quantity: None,
                unit: None,
            },
            None,
        )
        .await
        .unwrap();
    assert!(
        engine
            .act(
                Action::EditRecordText {
                    target: task.clone(),
                    head: None,
                    body: Some("Replace the queued local task".into()),
                },
                Some(person.clone())
            )
            .await
            .is_err()
    );
    assert!(
        engine
            .record_text_permissions(Some(&person), &task)
            .await
            .unwrap()
            .is_empty()
    );
    let pending: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM fiote_activation")
        .fetch_one(&engine.store.pool)
        .await
        .unwrap();
    assert_eq!(pending, 0);
}
