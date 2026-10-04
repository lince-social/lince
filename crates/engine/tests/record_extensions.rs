use engine::{Engine, actions::Action};
use nucleus::{
    RecordKind,
    record_extension::{
        Choice, Field, FieldKind, Inspection, Preset, Request, SCHEMA_NAMESPACE, Schema,
        VALUES_NAMESPACE,
    },
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

async fn setup() -> (Engine, String, String) {
    let engine = Engine::open_memory().await.unwrap();
    let organ = store::organs::ensure_local(&engine.store.pool, "http://extension-test")
        .await
        .unwrap()
        .uid;
    engine
        .set_signer(engine::trust::Signer::generate(&organ, "key"))
        .await
        .unwrap();
    let uid = engine
        .act(
            Action::CreateRecord {
                slug: None,
                kind: RecordKind::Plain,
                head: "Task".into(),
                body: "".into(),
                quantity: 0.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let concept = store::concepts::ensure(&engine.store.pool, "ready")
        .await
        .unwrap();
    (engine, uid, concept)
}

fn schema(concept: &str, kind: FieldKind) -> Schema {
    Schema {
        name: "Workflow".into(),
        fields: vec![Field {
            id: "state".into(),
            name: "State".into(),
            kind,
            archived: false,
            choices: vec![
                Choice {
                    id: "todo".into(),
                    name: "Todo".into(),
                    archived: false,
                    assertions: vec![],
                },
                Choice {
                    id: "done".into(),
                    name: "Done".into(),
                    archived: false,
                    assertions: vec![Preset {
                        predicate: concept.into(),
                        object: None,
                        quantity: None,
                        unit: None,
                    }],
                },
            ],
        }],
    }
}

async fn create(engine: &Engine, schema: Schema) -> String {
    engine
        .act(
            Action::RecordExtensions {
                target: None,
                request: Request::Create {
                    id: nucleus::new_uid("op"),
                    schema,
                },
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap()
}

fn apply(uid: &str, schema: &str, revision: u64, value: Value) -> Action {
    apply_schema(uid, schema, revision, value, 1)
}

fn apply_schema(
    uid: &str,
    schema: &str,
    revision: u64,
    value: Value,
    schema_revision: u64,
) -> Action {
    Action::RecordExtensions {
        target: Some(uid.into()),
        request: Request::Apply {
            id: nucleus::new_uid("op"),
            schema: schema.into(),
            expected_revision: revision,
            expected_schema_revision: schema_revision,
            values: BTreeMap::from([("state".into(), value)]),
            remove: false,
        },
    }
}

async fn inspect(engine: &Engine, uid: &str) -> Inspection {
    serde_json::from_value(
        engine
            .act(
                Action::RecordExtensions {
                    target: Some(uid.into()),
                    request: Request::Inspect {
                        catalog: true,
                        schemas: vec![],
                    },
                },
                None,
            )
            .await
            .unwrap()
            .data
            .unwrap(),
    )
    .unwrap()
}

#[tokio::test]
async fn ordinary_actions_create_edit_select_and_retry_without_losing_labels_or_manual_assertions()
{
    let (engine, uid, concept) = setup().await;
    let definition = schema(&concept, FieldKind::Select);
    let action = Action::RecordExtensions {
        target: None,
        request: Request::Create {
            id: nucleus::new_uid("op"),
            schema: definition.clone(),
        },
    };
    let schema_uid = engine
        .act(action.clone(), None)
        .await
        .unwrap()
        .created
        .unwrap();
    assert_eq!(
        engine.act(action, None).await.unwrap().created.as_deref(),
        Some(schema_uid.as_str())
    );
    let catalog = inspect(&engine, &uid).await;
    assert_eq!(catalog.schemas.len(), 1);
    assert!(catalog.writable.contains(&schema_uid));
    let column: Inspection = serde_json::from_value(
        engine
            .act(
                Action::RecordExtensions {
                    target: Some(uid.clone()),
                    request: Request::Inspect {
                        catalog: false,
                        schemas: vec![schema_uid.clone()],
                    },
                },
                None,
            )
            .await
            .unwrap()
            .data
            .unwrap(),
    )
    .unwrap();
    assert_eq!(column.schemas.len(), 1);
    assert!(column.values.is_empty());
    engine
        .act(
            Action::AssertRecord {
                subject: uid.clone(),
                predicate: concept.clone(),
                object: None,
                quantity: None,
                unit: None,
            },
            None,
        )
        .await
        .unwrap();
    let manual = store::assertions::for_subjects(&engine.store.pool, &[uid.clone()])
        .await
        .unwrap()[0]
        .uid
        .clone();
    let selection = apply(&uid, &schema_uid, 0, json!("done"));
    let result = engine.act(selection.clone(), None).await.unwrap().data;
    assert_eq!(
        engine.act(selection.clone(), None).await.unwrap().data,
        result
    );
    let mut renamed = definition;
    renamed.fields[0].choices.reverse();
    renamed.fields[0].choices[0].name = "Finished".into();
    engine
        .act(
            Action::RecordExtensions {
                target: Some(schema_uid.clone()),
                request: Request::Save {
                    id: nucleus::new_uid("op"),
                    expected_revision: 1,
                    schema: renamed,
                },
            },
            None,
        )
        .await
        .unwrap();
    let data = inspect(&engine, &uid).await;
    assert_eq!(data.values[&schema_uid].fields["state"], json!("done"));
    assert_eq!(data.schemas[0].schema.fields[0].choices[0].name, "Finished");
    assert_eq!(engine.act(selection, None).await.unwrap().data, result);
    engine
        .act(apply_schema(&uid, &schema_uid, 1, json!("todo"), 2), None)
        .await
        .unwrap();
    let assertions = store::assertions::for_subjects(&engine.store.pool, &[uid])
        .await
        .unwrap();
    assert_eq!(assertions.len(), 1);
    assert_eq!(assertions[0].uid, manual);
}

#[tokio::test]
async fn multiple_schemas_share_owned_assertions_and_detachment_keeps_values_and_monotonic_revisions()
 {
    let (engine, uid, concept) = setup().await;
    let first = create(&engine, schema(&concept, FieldKind::MultiSelect)).await;
    let second = create(&engine, schema(&concept, FieldKind::Select)).await;
    engine
        .act(apply(&uid, &first, 0, json!(["todo", "done"])), None)
        .await
        .unwrap();
    engine
        .act(apply(&uid, &second, 0, json!("done")), None)
        .await
        .unwrap();
    assert_eq!(
        store::assertions::for_subjects(&engine.store.pool, &[uid.clone()])
            .await
            .unwrap()
            .len(),
        1
    );
    engine
        .act(apply(&uid, &first, 1, json!(["todo"])), None)
        .await
        .unwrap();
    assert_eq!(
        store::assertions::for_subjects(&engine.store.pool, &[uid.clone()])
            .await
            .unwrap()
            .len(),
        1
    );
    let detach = Action::RecordExtensions {
        target: Some(uid.clone()),
        request: Request::Apply {
            id: nucleus::new_uid("op"),
            schema: second.clone(),
            expected_revision: 1,
            expected_schema_revision: 1,
            values: BTreeMap::new(),
            remove: true,
        },
    };
    engine.act(detach.clone(), None).await.unwrap();
    engine.act(detach, None).await.unwrap();
    assert!(
        store::assertions::for_subjects(&engine.store.pool, &[uid.clone()])
            .await
            .unwrap()
            .is_empty()
    );
    let data = inspect(&engine, &uid).await;
    assert!(!data.values[&second].attached);
    assert_eq!(data.values[&second].fields["state"], json!("done"));
    assert_eq!(data.values[&second].revision, 2);
    assert!(
        engine
            .act(apply(&uid, &second, 1, json!("todo")), None)
            .await
            .is_err()
    );
    engine
        .act(apply(&uid, &second, 2, json!("done")), None)
        .await
        .unwrap();
    assert_eq!(inspect(&engine, &uid).await.values[&second].revision, 3);
}

#[tokio::test]
async fn conflicts_and_invalid_choices_leave_values_and_assertions_unchanged() {
    let (engine, uid, concept) = setup().await;
    let mut definition = schema(&concept, FieldKind::MultiSelect);
    let mut duplicate = definition.fields[0].choices[1].clone();
    duplicate.id = "different".into();
    duplicate.assertions[0].quantity = Some("2".into());
    definition.fields[0].choices.push(duplicate);
    let schema_uid = create(&engine, definition.clone()).await;
    assert!(
        engine
            .act(
                apply(&uid, &schema_uid, 0, json!(["done", "different"])),
                None
            )
            .await
            .is_err()
    );
    assert!(inspect(&engine, &uid).await.values.is_empty());
    assert!(
        store::assertions::for_subjects(&engine.store.pool, &[uid.clone()])
            .await
            .unwrap()
            .is_empty()
    );
    engine
        .act(apply(&uid, &schema_uid, 0, json!(["done"])), None)
        .await
        .unwrap();
    let original = store::records::get_extension(&engine.store.pool, &uid, VALUES_NAMESPACE)
        .await
        .unwrap();
    for value in [json!(["missing"]), json!(["done", "done"]), json!("done")] {
        assert!(
            engine
                .act(apply(&uid, &schema_uid, 1, value), None)
                .await
                .is_err()
        );
        assert_eq!(
            store::records::get_extension(&engine.store.pool, &uid, VALUES_NAMESPACE)
                .await
                .unwrap(),
            original
        );
    }
    assert!(
        engine
            .act(apply(&uid, &schema_uid, 0, json!(["todo"])), None)
            .await
            .is_err()
    );
    definition.fields[0].choices[1].archived = true;
    engine
        .act(
            Action::RecordExtensions {
                target: Some(schema_uid.clone()),
                request: Request::Save {
                    id: nucleus::new_uid("op"),
                    expected_revision: 1,
                    schema: definition.clone(),
                },
            },
            None,
        )
        .await
        .unwrap();
    assert!(
        engine
            .act(apply(&uid, &schema_uid, 1, json!(["todo"])), None)
            .await
            .is_err()
    );
    assert_eq!(
        store::records::get_extension(&engine.store.pool, &uid, VALUES_NAMESPACE)
            .await
            .unwrap(),
        original
    );
    engine
        .act(apply_schema(&uid, &schema_uid, 1, json!(["done"]), 2), None)
        .await
        .unwrap();
    engine
        .act(apply_schema(&uid, &schema_uid, 2, json!(["todo"]), 2), None)
        .await
        .unwrap();
    assert!(
        engine
            .act(apply_schema(&uid, &schema_uid, 3, json!(["done"]), 2), None)
            .await
            .is_err()
    );
    definition.fields[0].choices.remove(1);
    assert!(
        engine
            .act(
                Action::RecordExtensions {
                    target: Some(schema_uid),
                    request: Request::Save {
                        id: nucleus::new_uid("op"),
                        expected_revision: 2,
                        schema: definition
                    }
                },
                None
            )
            .await
            .is_err()
    );
}

#[tokio::test]
async fn extension_and_preset_permissions_are_both_required_and_raw_replacement_is_blocked() {
    use protein::authority::{
        AssertionGrant, AssertionRole, AssertionTarget, ExtensionProperty, MutationGrant,
        Operation, Property, RolePolicy,
    };
    let (engine, uid, concept) = setup().await;
    let schema_uid = create(&engine, schema(&concept, FieldKind::Select)).await;
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
    let role = store::auth::ensure_role(&engine.store.pool, "extension-editor")
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
    let mut policy = RolePolicy {
        read: protein::Predicate::All(vec![]),
        grants: vec![MutationGrant {
            operation: Operation::Update,
            selector: protein::Predicate::All(vec![]),
            properties: BTreeSet::from([Property::Extension(ExtensionProperty {
                namespace: VALUES_NAMESPACE.into(),
                field: schema_uid.clone(),
            })]),
            assertions_add: vec![],
            assertions_remove: vec![],
        }],
    };
    store::role_policies::set(
        &engine.store.pool,
        role,
        &serde_json::to_value(&policy).unwrap(),
        0,
    )
    .await
    .unwrap();
    assert!(
        engine
            .act(
                apply(&uid, &schema_uid, 0, json!("done")),
                Some(person.clone())
            )
            .await
            .is_err()
    );
    assert!(
        store::assertions::for_subjects(&engine.store.pool, &[uid.clone()])
            .await
            .unwrap()
            .is_empty()
    );
    engine
        .act(
            apply(&uid, &schema_uid, 0, json!("todo")),
            Some(person.clone()),
        )
        .await
        .unwrap();
    let assertion = AssertionGrant {
        predicate_uid: concept,
        target: AssertionTarget::Unary,
        role: AssertionRole::Ordinary,
        properties: BTreeSet::new(),
    };
    policy.grants[0].assertions_add.push(assertion.clone());
    policy.grants[0].assertions_remove.push(assertion);
    store::role_policies::set(
        &engine.store.pool,
        role,
        &serde_json::to_value(&policy).unwrap(),
        1,
    )
    .await
    .unwrap();
    engine
        .act(
            apply(&uid, &schema_uid, 1, json!("done")),
            Some(person.clone()),
        )
        .await
        .unwrap();
    for namespace in [SCHEMA_NAMESPACE, VALUES_NAMESPACE] {
        assert!(
            engine
                .act(
                    Action::SetExtension {
                        target: uid.clone(),
                        namespace: namespace.into(),
                        fds: json!({})
                    },
                    None
                )
                .await
                .is_err()
        );
    }
    assert!(
        engine
            .act(
                Action::RecordExtensions {
                    target: Some(schema_uid.clone()),
                    request: Request::Save {
                        id: nucleus::new_uid("op"),
                        expected_revision: 1,
                        schema: schema("ready", FieldKind::Select)
                    }
                },
                Some(person.clone())
            )
            .await
            .is_err()
    );
    let inspected: Inspection = serde_json::from_value(
        engine
            .act(
                Action::RecordExtensions {
                    target: Some(uid),
                    request: Request::Inspect {
                        catalog: true,
                        schemas: vec![],
                    },
                },
                Some(person.clone()),
            )
            .await
            .unwrap()
            .data
            .unwrap(),
    )
    .unwrap();
    assert_eq!(inspected.schemas.len(), 1);
    assert!(!inspected.schemas[0].editable);
    let permission = store::auth::ensure_permission(&engine.store.pool, "record", "create")
        .await
        .unwrap();
    store::auth::grant(&engine.store.pool, role, permission)
        .await
        .unwrap();
    assert!(
        engine
            .act(
                Action::RecordExtensions {
                    target: None,
                    request: Request::Create {
                        id: nucleus::new_uid("op"),
                        schema: schema("ready", FieldKind::Select)
                    }
                },
                Some(person)
            )
            .await
            .is_err()
    );
}
