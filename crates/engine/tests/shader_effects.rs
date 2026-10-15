use engine::{Engine, actions::Action};
use nucleus::{
    RecordKind,
    shader_effect::{Effect, Extension, NAMESPACE},
};

async fn setup() -> (Engine, String) {
    let engine = Engine::open_memory().await.unwrap();
    let organ = store::organs::ensure_local(&engine.store.pool, "http://shader-effects")
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
                head: "Shader task".into(),
                body: String::new(),
                quantity: -1.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    (engine, uid)
}

#[tokio::test]
async fn spatial_contracts_validate_all_stages_before_replacing_a_catalog() {
    use nucleus::shader_effect::spatial::{
        DEFORM_SPHERE, Deformation, MESH_SHADE, MeshSettings, Mode, VOLUME_CLOUD, VolumeSettings,
    };
    let (engine, uid) = setup().await;
    let mut effect = Effect::default();
    effect.mode = Mode::Mesh(MeshSettings {
        deformation: Some(Deformation {
            source: DEFORM_SPHERE.into(),
            max_displacement: 0.25,
        }),
        ..Default::default()
    });
    effect.programs = [("mesh".into(), MESH_SHADE.into())].into();
    effect.rules.clear();
    effect.fallback = "mesh".into();
    engine
        .act(save(&uid, 1, effect.clone()), None)
        .await
        .unwrap();
    let stored = store::records::get_extension(&engine.store.pool, &uid, NAMESPACE)
        .await
        .unwrap()
        .unwrap();
    let restored: Extension = serde_json::from_value(stored.clone()).unwrap();
    assert_eq!(restored.selected(None).unwrap().mode, effect.mode);
    if let Mode::Mesh(settings) = &mut effect.mode {
        settings.deformation.as_mut().unwrap().max_displacement = 100.0;
    }
    assert!(
        engine
            .act(save(&uid, 2, effect.clone()), None)
            .await
            .is_err()
    );
    effect.mode = Mode::Volume(VolumeSettings::default());
    assert!(
        engine
            .act(save(&uid, 2, effect.clone()), None)
            .await
            .is_err()
    );
    assert_eq!(
        store::records::get_extension(&engine.store.pool, &uid, NAMESPACE)
            .await
            .unwrap()
            .unwrap(),
        stored
    );
    effect.programs = [("cloud".into(), VOLUME_CLOUD.into())].into();
    effect.fallback = "cloud".into();
    engine
        .act(save(&uid, 2, effect.clone()), None)
        .await
        .unwrap();
    let restored: Extension = serde_json::from_value(
        store::records::get_extension(&engine.store.pool, &uid, NAMESPACE)
            .await
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(restored.selected(None), Some(&effect));
}

#[tokio::test]
async fn catalog_updates_preserve_siblings_and_reject_stale_or_invalid_replacements() {
    use nucleus::shader_effect::{
        NamedEffect,
        spatial::{Mode, Quality, VOLUME_CLOUD, VolumeSettings},
    };
    let (engine, uid) = setup().await;
    let mut catalog = Extension::single(1, Effect::default());
    let mut calm = Effect::default();
    calm.mode = Mode::Volume(VolumeSettings {
        quality: Quality::High,
        density: 8.0,
    });
    calm.programs = [("calm".into(), VOLUME_CLOUD.into())].into();
    calm.rules.clear();
    calm.fallback = "calm".into();
    catalog.effects.insert(
        "calm".into(),
        NamedEffect {
            label: "Calm".into(),
            effect: calm,
        },
    );
    let action = |extension: &Extension| Action::SetExtension {
        target: uid.clone(),
        namespace: NAMESPACE.into(),
        fds: serde_json::to_value(extension).unwrap(),
    };
    engine.act(action(&catalog), None).await.unwrap();
    let sibling = catalog.effects["calm"].clone();
    catalog.revision = 2;
    catalog.effects.get_mut("main").unwrap().label = "Task energy".into();
    engine.act(action(&catalog), None).await.unwrap();
    let stored = store::records::get_extension(&engine.store.pool, &uid, NAMESPACE)
        .await
        .unwrap()
        .unwrap();
    let restored: Extension = serde_json::from_value(stored.clone()).unwrap();
    assert_eq!(restored, catalog);
    assert_eq!(restored.effects["calm"], sibling);
    assert!(engine.act(action(&catalog), None).await.is_err());
    catalog.revision = 3;
    catalog.default_effect = "missing".into();
    assert!(engine.act(action(&catalog), None).await.is_err());
    assert_eq!(
        store::records::get_extension(&engine.store.pool, &uid, NAMESPACE)
            .await
            .unwrap()
            .unwrap(),
        stored
    );
}

#[tokio::test]
async fn numeric_binding_declarations_round_trip_and_invalid_edits_preserve_the_extension() {
    use nucleus::shader_effect::bindings::{NumericBinding, NumericField, NumericSource};
    let (engine, uid) = setup().await;
    let mut effect = Effect::default();
    effect.bindings.push(NumericBinding {
        slot: 3,
        label: "Protein".into(),
        source: NumericSource {
            organ: None,
            record: Some(uid.clone()),
            field: NumericField::Extension {
                namespace: "nutrition".into(),
                path: vec!["protein".into()],
            },
        },
    });
    engine
        .act(save(&uid, 1, effect.clone()), None)
        .await
        .unwrap();
    let stored = store::records::get_extension(&engine.store.pool, &uid, NAMESPACE)
        .await
        .unwrap()
        .unwrap();
    let restored: Extension = serde_json::from_value(stored.clone()).unwrap();
    assert_eq!(restored.selected(None).unwrap().bindings, effect.bindings);
    effect.bindings[0].source.record = Some("an-unresolved-slug".into());
    assert!(engine.act(save(&uid, 2, effect), None).await.is_err());
    assert_eq!(
        store::records::get_extension(&engine.store.pool, &uid, NAMESPACE)
            .await
            .unwrap()
            .unwrap(),
        stored
    );
}

#[tokio::test]
async fn numeric_field_projection_is_exact_scoped_and_updates_without_effect_changes() {
    use nucleus::shader_effect::bindings::NumericField;
    use protein::shader_inputs::{Request, key};
    let (engine, uid) = setup().await;
    engine.act(Action::SetExtension {
        target: uid.clone(), namespace: "nutrition".into(),
        fds: serde_json::json!({"daily":{"protein":9007199254740993_i64,"note":"unrequested metadata"}}),
    }, None).await.unwrap();
    let field = NumericField::Extension {
        namespace: "nutrition".into(),
        path: vec!["daily".into(), "protein".into()],
    };
    let missing = NumericField::Extension {
        namespace: "nutrition".into(),
        path: vec!["missing".into()],
    };
    let mut query = protein::Protein {
        source: protein::Source::Record,
        filter: vec![protein::Predicate::UidEq(uid.clone())],
        fields: Some(["uid", "numeric_values"].map(str::to_string).into()),
        include: protein::Include {
            numeric_fields: vec![
                Request {
                    record: uid.clone(),
                    field: field.clone(),
                },
                Request {
                    record: uid.clone(),
                    field: missing.clone(),
                },
                Request {
                    record: uid.clone(),
                    field: NumericField::Quantity,
                },
                Request {
                    record: nucleus::new_uid("r"),
                    field: field.clone(),
                },
            ],
            ..Default::default()
        },
        aggregate: None,
        order: vec![],
        limit: Some(1),
    };
    let rows = protein::execute_for(&engine.store, &query, None)
        .await
        .unwrap();
    assert_eq!(rows[0]["numeric_values"][key(&field)], "9007199254740993");
    assert_eq!(
        rows[0]["numeric_values"][key(&NumericField::Quantity)],
        "-1"
    );
    assert!(rows[0]["numeric_values"][key(&missing)].is_null());
    assert_eq!(rows[0]["numeric_values"].as_object().unwrap().len(), 3);
    assert!(
        !serde_json::to_string(&rows)
            .unwrap()
            .contains("unrequested metadata")
    );
    let stranger = nucleus::new_uid("r");
    assert!(
        protein::execute_for(&engine.store, &query, Some(&stranger))
            .await
            .unwrap()
            .is_empty()
    );
    engine
        .act(
            Action::SetQuantity {
                target: uid.clone(),
                value: 7.0,
            },
            None,
        )
        .await
        .unwrap();
    let rows = protein::execute_for(&engine.store, &query, None)
        .await
        .unwrap();
    assert_eq!(rows[0]["numeric_values"][key(&NumericField::Quantity)], "7");
    query.include.numeric_fields.push(Request {
        record: uid,
        field: NumericField::Extension {
            namespace: "lince.social.private".into(),
            path: vec!["value".into()],
        },
    });
    assert!(
        protein::execute_for(&engine.store, &query, None)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn mapped_task_state_round_trips_and_rejects_invalid_ranges_without_overwriting() {
    use nucleus::shader_effect::{InputMappings, QuantityRange, TaskState};
    let (engine, uid) = setup().await;
    let mut effect = Effect::default();
    effect.inputs = InputMappings {
        task: Some(QuantityRange {
            start: "-3".into(),
            end: "0".into(),
        }),
        progress: Some(QuantityRange {
            start: "-3".into(),
            end: "0".into(),
        }),
    };
    engine
        .act(save(&uid, 1, effect.clone()), None)
        .await
        .unwrap();
    let query = protein::Protein {
        source: protein::Source::Record,
        filter: vec![protein::Predicate::UidEq(uid.clone())],
        fields: Some(
            ["uid", "quantity", "shader_effect"]
                .map(str::to_string)
                .into(),
        ),
        include: protein::Include {
            shader_effect: true,
            ..Default::default()
        },
        aggregate: None,
        order: vec![],
        limit: Some(1),
    };
    let rows = protein::execute_for(&engine.store, &query, None)
        .await
        .unwrap();
    let projected: Extension = serde_json::from_value(rows[0]["shader_effect"].clone()).unwrap();
    assert_eq!(projected.selected(None).unwrap(), &effect);
    assert_eq!(
        projected
            .selected(None)
            .unwrap()
            .inputs
            .resolve(rows[0]["quantity"].as_str())
            .task,
        TaskState::Active
    );
    engine
        .act(
            Action::SetQuantity {
                target: uid.clone(),
                value: 0.0,
            },
            None,
        )
        .await
        .unwrap();
    let rows = protein::execute_for(&engine.store, &query, None)
        .await
        .unwrap();
    let mapped = projected
        .selected(None)
        .unwrap()
        .inputs
        .resolve(rows[0]["quantity"].as_str());
    assert_eq!(mapped.task, TaskState::Completed);
    assert_eq!(mapped.progress, Some(1.0));
    effect.inputs.progress.as_mut().unwrap().end = "-3".into();
    assert!(engine.act(save(&uid, 2, effect), None).await.is_err());
    let current = store::records::get_extension(&engine.store.pool, &uid, NAMESPACE)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(current["revision"], 1);
    assert_eq!(
        serde_json::from_value::<Extension>(current).unwrap(),
        projected
    );
}

fn save(uid: &str, revision: u64, effect: Effect) -> Action {
    Action::SetExtension {
        target: uid.into(),
        namespace: NAMESPACE.into(),
        fds: serde_json::to_value(Extension::single(revision, effect)).unwrap(),
    }
}

#[tokio::test]
async fn validates_bundles_and_rejects_stale_revisions_without_overwriting() {
    let (engine, uid) = setup().await;
    let effect = Effect::default();
    engine
        .act(save(&uid, 1, effect.clone()), None)
        .await
        .unwrap();
    let before = store::records::get_extension(&engine.store.pool, &uid, NAMESPACE)
        .await
        .unwrap()
        .unwrap();
    assert!(
        engine
            .act(save(&uid, 1, effect.clone()), None)
            .await
            .is_err()
    );
    let mut invalid = effect.clone();
    invalid
        .programs
        .insert("negative".into(), "fn shade(".into());
    assert!(engine.act(save(&uid, 2, invalid), None).await.is_err());
    assert_eq!(
        store::records::get_extension(&engine.store.pool, &uid, NAMESPACE)
            .await
            .unwrap()
            .unwrap(),
        before
    );
    engine.act(save(&uid, 2, effect), None).await.unwrap();
    assert_eq!(
        store::records::get_extension(&engine.store.pool, &uid, NAMESPACE)
            .await
            .unwrap()
            .unwrap()["revision"],
        2
    );
}

#[tokio::test]
async fn record_query_projects_effect_and_quantity_together_and_updates_at_zero() {
    let (engine, uid) = setup().await;
    engine
        .act(save(&uid, 1, Effect::default()), None)
        .await
        .unwrap();
    let query = protein::Protein {
        source: protein::Source::Record,
        filter: vec![protein::Predicate::UidEq(uid.clone())],
        fields: Some(
            ["uid", "quantity", "shader_effect"]
                .map(str::to_string)
                .into(),
        ),
        include: protein::Include {
            shader_effect: true,
            ..Default::default()
        },
        aggregate: None,
        order: vec![],
        limit: Some(1),
    };
    let before = protein::execute_for(&engine.store, &query, None)
        .await
        .unwrap();
    let extension: Extension = serde_json::from_value(before[0]["shader_effect"].clone()).unwrap();
    assert_eq!(
        extension
            .selected(None)
            .unwrap()
            .selected(before[0]["quantity"].as_str()),
        "negative"
    );
    engine
        .act(
            Action::SetQuantity {
                target: uid,
                value: 0.0,
            },
            None,
        )
        .await
        .unwrap();
    let after = protein::execute_for(&engine.store, &query, None)
        .await
        .unwrap();
    assert_eq!(
        extension
            .selected(None)
            .unwrap()
            .selected(after[0]["quantity"].as_str()),
        "nonnegative"
    );
    assert_eq!(after[0]["shader_effect"], before[0]["shader_effect"]);
    let mut unbounded = query;
    unbounded.limit = None;
    assert_eq!(
        protein::execute_for(&engine.store, &unbounded, None)
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn unreadable_record_does_not_expose_effect_source() {
    let (engine, uid) = setup().await;
    engine
        .act(save(&uid, 1, Effect::default()), None)
        .await
        .unwrap();
    let query = protein::Protein {
        source: protein::Source::Record,
        filter: vec![protein::Predicate::UidEq(uid.clone())],
        fields: Some(["uid", "shader_effect"].map(str::to_string).into()),
        include: protein::Include {
            shader_effect: true,
            ..Default::default()
        },
        aggregate: None,
        order: vec![],
        limit: Some(1),
    };
    let stranger = nucleus::new_uid("r");
    assert!(
        protein::execute_for(&engine.store, &query, Some(&stranger))
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        engine
            .act(save(&uid, 2, Effect::default()), Some(stranger))
            .await
            .is_err()
    );
}
