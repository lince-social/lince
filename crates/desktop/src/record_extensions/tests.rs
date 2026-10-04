use super::*;
use std::sync::Arc;

async fn fixture() -> (App, Arc<engine::Engine>, String, Entity) {
    let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
    let organ = store::organs::ensure_local(&engine.store.pool, "http://extension-ui")
        .await
        .unwrap()
        .uid;
    engine
        .set_signer(engine::trust::Signer::generate(&organ, "key"))
        .await
        .unwrap();
    let record = engine
        .act(
            engine::actions::Action::CreateRecord {
                slug: None,
                kind: nucleus::RecordKind::Plain,
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
    let runtime = cell::CellRuntime {
        commands: Default::default(),
        store: engine.store.clone(),
        engine: engine.clone(),
        lanes: Arc::new(cell::LaneHub::new()),
        wire: Default::default(),
        fiote: None,
        speech: None,
        information: None,
    };
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<Assets<Font>>()
        .init_resource::<crate::theme::Typography>()
        .init_resource::<crate::tokens::ThemeSettings>()
        .add_plugins(ExtensionPlugin);
    app.world_mut().insert_non_send(crate::cell_bridge::connect(
        runtime,
        crate::wake::WakeSignal::new(|| {}),
    ));
    let root = app
        .world_mut()
        .spawn((
            Node::default(),
            crate::workspace::Workspaces::default(),
            crate::canvas::CanvasView::default(),
        ))
        .id();
    (app, engine, record, root)
}

async fn pump(app: &mut App, ready: impl Fn(&World) -> bool) {
    for _ in 0..1000 {
        let messages = {
            let mut bridge = app
                .world_mut()
                .non_send_mut::<crate::cell_bridge::CellBridge>();
            let mut messages = Vec::new();
            while let Ok(message) = bridge.incoming.try_recv() {
                messages.push(message);
            }
            messages
        };
        for message in messages {
            app.world_mut()
                .write_message(crate::cell_bridge::CellMessage(message));
        }
        app.update();
        if ready(app.world()) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(3)).await;
    }
    panic!("Extension UI did not receive its result");
}

fn text(world: &mut World, owner: Entity, key: impl Fn(&InputKey) -> bool, value: &str) {
    let entity = world
        .query::<(Entity, &Input)>()
        .iter(world)
        .find(|(_, input)| input.owner == owner && key(&input.key))
        .unwrap()
        .0;
    world
        .get_mut::<EditableText>(entity)
        .unwrap()
        .editor
        .set_text(value);
}

#[tokio::test]
async fn forms_create_a_schema_and_dropdown_apply_cancel_undo_and_restore_work() {
    let (mut app, engine, record, root) = fixture().await;
    store::concepts::ensure(&engine.store.pool, "ready")
        .await
        .unwrap();
    let owner = spawn(
        app.world_mut(),
        root,
        1,
        DVec2::new(40.0, 60.0),
        Settings {
            record: Some(record.clone()),
            ..default()
        },
    );
    pump(&mut app, |world| inspection(world, owner).is_some()).await;
    Command::New.apply(app.world_mut(), owner);
    app.update();
    text(
        app.world_mut(),
        owner,
        |key| matches!(key, InputKey::SchemaName),
        "Workflow",
    );
    text(
        app.world_mut(),
        owner,
        |key| matches!(key, InputKey::FieldName),
        "State",
    );
    Command::Choice(0).apply(app.world_mut(), owner);
    app.update();
    text(
        app.world_mut(),
        owner,
        |key| matches!(key, InputKey::ChoiceName),
        "Ready",
    );
    Command::AddPreset.apply(app.world_mut(), owner);
    app.update();
    Command::Pick(InputKey::Preset(0, 0)).apply(app.world_mut(), owner);
    pump(&mut app, |world| {
        world
            .get::<View>(owner)
            .unwrap()
            .picker
            .as_ref()
            .is_some_and(|picker| picker.rows.iter().any(|row| row["name"] == "ready"))
    })
    .await;
    let concept = app
        .world()
        .get::<View>(owner)
        .unwrap()
        .picker
        .as_ref()
        .unwrap()
        .rows
        .iter()
        .find(|row| row["name"] == "ready")
        .unwrap()["uid"]
        .as_str()
        .unwrap()
        .to_owned();
    Command::Picked(concept).apply(app.world_mut(), owner);
    app.update();
    let field = app
        .world()
        .get::<View>(owner)
        .unwrap()
        .schema
        .as_ref()
        .unwrap()
        .schema
        .fields[0]
        .id
        .clone();
    let choice = app
        .world()
        .get::<View>(owner)
        .unwrap()
        .schema
        .as_ref()
        .unwrap()
        .schema
        .fields[0]
        .choices[0]
        .id
        .clone();
    Command::SaveSchema.apply(app.world_mut(), owner);
    pump(&mut app, |world| {
        world.get::<View>(owner).unwrap().pending.is_none()
            && inspection(world, owner).is_some_and(|data| !data.schemas.is_empty())
    })
    .await;
    let schema = app
        .world()
        .get::<Settings>(owner)
        .unwrap()
        .schema
        .clone()
        .unwrap();
    assert!(
        app.world()
            .get::<View>(owner)
            .unwrap()
            .message
            .contains("Schema saved")
    );
    let dropdown = spawn(
        app.world_mut(),
        root,
        1,
        DVec2::ZERO,
        Settings {
            mode: Mode::Dropdown,
            record: Some(record.clone()),
            schema: Some(schema.clone()),
            field: Some(field.clone()),
            source: Source::Local,
        },
    );
    pump(&mut app, |world| inspection(world, dropdown).is_some()).await;
    Command::Select(field.clone(), choice.clone()).apply(app.world_mut(), dropdown);
    app.update();
    assert!(
        app.world_mut()
            .query::<&Text>()
            .iter(app.world())
            .any(|text| text.0.contains("Preset assertion preview"))
    );
    assert!(
        store::assertions::for_subjects(&engine.store.pool, &[record.clone()])
            .await
            .unwrap()
            .is_empty()
    );
    Command::Discard.apply(app.world_mut(), dropdown);
    app.update();
    assert!(
        app.world()
            .get::<View>(dropdown)
            .unwrap()
            .values
            .as_ref()
            .unwrap()
            .fields
            .is_empty()
    );
    Command::Select(field.clone(), choice.clone()).apply(app.world_mut(), dropdown);
    Command::Apply.apply(app.world_mut(), dropdown);
    pump(&mut app, |world| {
        world.get::<View>(dropdown).unwrap().pending.is_none()
            && inspection(world, dropdown).is_some_and(|data| {
                data.values
                    .get(&schema)
                    .is_some_and(|values| values.revision == 1)
            })
    })
    .await;
    assert_eq!(
        store::assertions::for_subjects(&engine.store.pool, &[record.clone()])
            .await
            .unwrap()
            .len(),
        1
    );
    Command::Undo.apply(app.world_mut(), dropdown);
    pump(&mut app, |world| {
        world.get::<View>(dropdown).unwrap().pending.is_none()
            && inspection(world, dropdown).is_some_and(|data| {
                data.values
                    .get(&schema)
                    .is_some_and(|values| values.revision == 2)
            })
    })
    .await;
    assert!(
        store::assertions::for_subjects(&engine.store.pool, &[record])
            .await
            .unwrap()
            .is_empty()
    );
    let saved = snapshot(app.world_mut(), root);
    assert_eq!(saved.len(), 2);
    assert!(saved.iter().all(SavedExtensions::valid));
    let saved: Vec<SavedExtensions> =
        serde_json::from_slice(&serde_json::to_vec(&saved).unwrap()).unwrap();
    let mut world = World::new();
    world.init_resource::<Assets<Font>>();
    world.init_resource::<crate::theme::Typography>();
    let root = world
        .spawn((Node::default(), crate::workspace::Workspaces::default()))
        .id();
    for saved in saved {
        saved.restore(&mut world, root);
    }
    assert!(
        world
            .query::<&Settings>()
            .iter(&world)
            .any(|settings| settings.mode == Mode::Dropdown
                && settings.schema.as_deref() == Some(&schema)
                && settings.field.as_deref() == Some(&field))
    );
}

#[tokio::test]
async fn multiselect_labels_refresh_without_changing_ids_and_conflicts_keep_the_draft() {
    let (mut app, engine, record, root) = fixture().await;
    let mut schema = Schema {
        name: "Labels".into(),
        fields: vec![new_field(FieldKind::MultiSelect)],
    };
    schema.fields[0].choices = vec![new_choice(), new_choice()];
    schema.fields[0].choices[0].name = "First".into();
    schema.fields[0].choices[1].name = "Second".into();
    let uid = engine
        .act(
            engine::actions::Action::RecordExtensions {
                target: None,
                request: Request::Create {
                    id: nucleus::new_uid("op"),
                    schema: schema.clone(),
                },
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let field = schema.fields[0].id.clone();
    let first = schema.fields[0].choices[0].id.clone();
    let second = schema.fields[0].choices[1].id.clone();
    let owner = spawn(
        app.world_mut(),
        root,
        1,
        DVec2::ZERO,
        Settings {
            mode: Mode::Dropdown,
            record: Some(record.clone()),
            schema: Some(uid.clone()),
            field: Some(field.clone()),
            source: Source::Local,
        },
    );
    pump(&mut app, |world| inspection(world, owner).is_some()).await;
    Command::Select(field.clone(), first.clone()).apply(app.world_mut(), owner);
    Command::Select(field.clone(), second.clone()).apply(app.world_mut(), owner);
    Command::Apply.apply(app.world_mut(), owner);
    pump(&mut app, |world| {
        inspection(world, owner).is_some_and(|data| {
            data.values
                .get(&uid)
                .is_some_and(|values| values.revision == 1)
        }) && world.get::<View>(owner).unwrap().pending.is_none()
    })
    .await;
    schema.fields[0].choices[0].name = "Renamed".into();
    schema.fields[0].choices.reverse();
    engine
        .act(
            engine::actions::Action::RecordExtensions {
                target: Some(uid.clone()),
                request: Request::Save {
                    id: nucleus::new_uid("op"),
                    expected_revision: 1,
                    schema,
                },
            },
            None,
        )
        .await
        .unwrap();
    Command::Reload.apply(app.world_mut(), owner);
    pump(&mut app, |world| {
        inspection(world, owner).is_some_and(|data| data.schemas[0].revision == 2)
    })
    .await;
    assert!(
        app.world_mut()
            .query::<&Text>()
            .iter(app.world())
            .any(|text| text.0.contains("Renamed"))
    );
    assert_eq!(
        inspection(app.world(), owner).unwrap().values[&uid].fields[&field],
        json!([first.clone(), second.clone()])
    );
    Command::Clear(field.clone()).apply(app.world_mut(), owner);
    engine
        .act(
            engine::actions::Action::RecordExtensions {
                target: Some(record),
                request: Request::Apply {
                    id: nucleus::new_uid("op"),
                    schema: uid.clone(),
                    expected_revision: 1,
                    expected_schema_revision: 2,
                    values: BTreeMap::from([(field.clone(), json!([first]))]),
                    remove: false,
                },
            },
            None,
        )
        .await
        .unwrap();
    Command::Apply.apply(app.world_mut(), owner);
    pump(&mut app, |world| {
        world.get::<View>(owner).unwrap().pending.is_none()
    })
    .await;
    let view = app.world().get::<View>(owner).unwrap();
    assert!(view.message.contains("changed"));
    assert!(!view.values.as_ref().unwrap().fields.contains_key(&field));
    assert!(view.dirty());
}

#[test]
fn custom_columns_compile_without_querying_unknown_properties_and_permissions_remove_controls() {
    let schema = nucleus::new_uid("r");
    let record = nucleus::new_uid("r");
    let property = nucleus::record_extension::column(&schema, "state");
    let mut config = crate::protein_area::Config::default();
    let mut binding = crate::protein_area::Binding::new(&property);
    binding.editable = true;
    config.bindings.push(binding);
    assert!(config.valid());
    assert!(!config.query().unwrap().fields.unwrap().contains(&property));
    let mut world = World::new();
    world.init_resource::<Assets<Font>>();
    world.init_resource::<crate::theme::Typography>();
    world.init_resource::<Runtime>();
    let owner = world.spawn(Node::default()).id();
    field(
        &mut world,
        owner,
        RecordBinding {
            area: owner,
            uid: record.clone(),
            source: Source::Local,
        },
        &schema,
        "state",
        true,
    );
    let settings = world.get::<Settings>(owner).unwrap().clone();
    let key = key(&settings);
    world.get_mut::<View>(owner).unwrap().feed = key.clone();
    let definition = Schema {
        name: "State".into(),
        fields: vec![Field {
            id: "state".into(),
            name: "Status".into(),
            kind: FieldKind::Select,
            archived: false,
            choices: vec![Choice {
                id: "ready".into(),
                name: "Ready".into(),
                archived: false,
                assertions: vec![],
            }],
        }],
    };
    world.resource_mut::<Runtime>().feeds.insert(
        key,
        Feed {
            schemas: vec![],
            source: Source::Local,
            target: Some(record),
            catalog: false,
            data: Some(Inspection {
                schemas: vec![nucleus::record_extension::Definition {
                    uid: schema.clone(),
                    revision: 1,
                    schema: definition,
                    editable: false,
                }],
                values: BTreeMap::from([(
                    schema.clone(),
                    nucleus::record_extension::Values {
                        revision: 1,
                        attached: true,
                        fields: BTreeMap::from([("state".into(), json!("ready"))]),
                    },
                )]),
                ..default()
            }),
            error: String::new(),
            version: 1,
            next: Instant::now(),
            pending: None,
        },
    );
    ui::render(&mut world, owner);
    assert!(
        world
            .query::<&Text>()
            .iter(&world)
            .any(|text| text.0.contains("Ready"))
    );
    assert!(
        world
            .query::<&crate::actions::ActionButton>()
            .iter(&world)
            .next()
            .is_none()
    );
    receive(
        &mut world,
        &Source::Local,
        &ServerMessage::Error {
            id: "connection".into(),
            message: "expired".into(),
            code: Some("session_expired".into()),
        },
    );
    ui::render(&mut world, owner);
    assert!(
        !world
            .query::<&Text>()
            .iter(&world)
            .any(|text| text.0.contains("Ready"))
    );
}

#[test]
fn many_choices_are_paged_and_searchable_and_multiselect_preserves_keyboard_focus() {
    let mut world = World::new();
    world.init_resource::<Assets<Font>>();
    world.init_resource::<crate::theme::Typography>();
    world.init_resource::<Runtime>();
    world.init_resource::<bevy::input_focus::InputFocus>();
    world.init_resource::<ButtonInput<KeyCode>>();
    let root = world
        .spawn((Node::default(), crate::workspace::Workspaces::default()))
        .id();
    let record = nucleus::new_uid("r");
    let schema = nucleus::new_uid("r");
    let owner = spawn(
        &mut world,
        root,
        1,
        DVec2::ZERO,
        Settings {
            mode: Mode::Dropdown,
            record: Some(record.clone()),
            source: Source::Local,
            schema: Some(schema.clone()),
            field: Some("state".into()),
        },
    );
    let key = key(world.get::<Settings>(owner).unwrap());
    world.get_mut::<View>(owner).unwrap().feed = key.clone();
    let field = Field {
        id: "state".into(),
        name: "State".into(),
        kind: FieldKind::MultiSelect,
        archived: false,
        choices: (0..512)
            .map(|index| Choice {
                id: format!("choice-{index}"),
                name: format!("Choice {index:03}"),
                archived: false,
                assertions: vec![],
            })
            .collect(),
    };
    world.resource_mut::<Runtime>().feeds.insert(
        key,
        Feed {
            schemas: vec![schema.clone()],
            source: Source::Local,
            target: Some(record),
            catalog: true,
            data: Some(Inspection {
                schemas: vec![nucleus::record_extension::Definition {
                    uid: schema.clone(),
                    revision: 1,
                    schema: Schema {
                        name: "Labels".into(),
                        fields: vec![field],
                    },
                    editable: true,
                }],
                writable: vec![schema],
                ..default()
            }),
            error: String::new(),
            version: 1,
            next: Instant::now(),
            pending: None,
        },
    );
    Command::Open("state".into()).apply(&mut world, owner);
    ui::render(&mut world, owner);
    let options = world
        .query::<&Text>()
        .iter(&world)
        .filter(|text| text.0.starts_with("○ Choice"))
        .count();
    assert_eq!(options, 20);
    Command::Select("state".into(), "choice-0".into()).apply(&mut world, owner);
    ui::render(&mut world, owner);
    let focused = world
        .resource::<bevy::input_focus::InputFocus>()
        .get()
        .unwrap();
    assert!(world.get::<Children>(focused).unwrap().iter().any(|child| {
        world
            .get::<Text>(child)
            .is_some_and(|text| text.0.contains("Choice 000"))
    }));
    world
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::Escape);
    ui::keyboard(&mut world);
    let focused = world
        .resource::<bevy::input_focus::InputFocus>()
        .get()
        .unwrap();
    assert!(world.get::<Children>(focused).unwrap().iter().any(|child| {
        world
            .get::<Text>(child)
            .is_some_and(|text| text.0.contains("State:"))
    }));
    assert!(world.get::<View>(owner).unwrap().open.is_none());
    Command::Open("state".into()).apply(&mut world, owner);
    ui::render(&mut world, owner);
    text(
        &mut world,
        owner,
        |key| matches!(key, InputKey::Search),
        "Choice 511",
    );
    Command::Search.apply(&mut world, owner);
    ui::render(&mut world, owner);
    assert_eq!(
        world
            .query::<&Text>()
            .iter(&world)
            .filter(|text| text.0.starts_with("○ Choice"))
            .count(),
        1
    );
    assert!(
        world
            .query::<&Text>()
            .iter(&world)
            .any(|text| text.0 == "○ Choice 511")
    );
}
