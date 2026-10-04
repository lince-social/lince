use super::*;
use engine::{Engine, actions::Action};
use std::sync::Arc;

fn fixture() -> (App, Entity) {
    let mut app = App::new();
    crate::laboratory::isolate(app.world_mut());
    app.add_plugins(MinimalPlugins)
        .init_resource::<Assets<Font>>()
        .init_resource::<Assets<Image>>()
        .init_resource::<crate::theme::Typography>()
        .init_resource::<bevy::input_focus::InputFocus>()
        .add_message::<crate::cell_bridge::CellMessage>();
    let root = app
        .world_mut()
        .spawn((
            crate::container::BoxRoot,
            Workspaces::default(),
            CanvasView::default(),
            Node::default(),
        ))
        .id();
    (app, root)
}

#[test]
fn malformed_native_groups_reject_before_changing_widgets() {
    let (mut app, root) = fixture();
    let entity = crate::sand_store::spawn_sand(
        app.world_mut(),
        root,
        1,
        SandKind::EditableText,
        "Keep me",
        DVec2::ZERO,
    );
    let before = capture(app.world_mut(), root).unwrap();
    let mut after = before.clone();
    after.placements[0].geometry.position = [150.0, 250.0];
    after.placements[0].group = Some("é".repeat(16));
    assert!(apply(app.world_mut(), root, &before, &after).is_err());
    assert_eq!(
        app.world().get::<CanvasItem>(entity).unwrap().position,
        DVec2::ZERO
    );
    assert_eq!(capture(app.world_mut(), root).unwrap(), before);
}

#[test]
fn oversized_rendered_compositions_reject_before_committing_native_widgets() {
    let (mut app, root) = fixture();
    let before = capture(app.world_mut(), root).unwrap();
    let mut after = before.clone();
    let component = ContentKind::Composition {
        composition: api::Composition {
            name: "Large composition".into(),
            origin: None,
            parts: (0..32)
                .map(|index| api::Part {
                    id: format!("part-{index}"),
                    geometry: api::Geometry {
                        position: [0.0, 0.0],
                        size: [250.0, 120.0],
                    },
                    component: ContentKind::Native {
                        kind: "text".into(),
                        settings: BTreeMap::from([(
                            "text".into(),
                            serde_json::json!("x".repeat(4096)),
                        )]),
                        bindings: Vec::new(),
                    },
                    events: Vec::new(),
                })
                .collect(),
        },
    };
    component.validate(&registry(), false).unwrap();
    after.placements.push(api::Placement {
        id: nucleus::new_uid("placement"),
        workspace: 1,
        component,
        geometry: api::Geometry {
            position: [0.0, 0.0],
            size: [520.0, 420.0],
        },
        selected: false,
        group: None,
        metadata: Default::default(),
    });
    assert!(apply(app.world_mut(), root, &before, &after).is_err());
    assert_eq!(capture(app.world_mut(), root).unwrap(), before);
}

#[test]
fn replacing_or_removing_a_record_placement_preserves_unsaved_drafts() {
    let (mut app, root) = fixture();
    let entity = crate::sand_store::spawn_sand(
        app.world_mut(),
        root,
        1,
        SandKind::EditableText,
        "Original",
        DVec2::ZERO,
    );
    let editor = app
        .world()
        .get::<StoredSand>(entity)
        .unwrap()
        .content
        .unwrap();
    let status = app.world_mut().spawn(Text::default()).id();
    app.world_mut()
        .entity_mut(editor)
        .insert(crate::record_view::RecordEditor {
            uid: nucleus::new_uid("r"),
            confirmed: "Original".into(),
            pending: None,
            status,
        });
    app.world_mut()
        .get_mut::<bevy::text::EditableText>(editor)
        .unwrap()
        .editor
        .set_text("Unsent Record edit");
    let before = capture(app.world_mut(), root).unwrap();
    let mut after = before.clone();
    after.placements.clear();
    assert!(
        apply(app.world_mut(), root, &before, &after)
            .unwrap_err()
            .contains("draft")
    );
    assert_eq!(
        app.world()
            .get::<bevy::text::EditableText>(editor)
            .unwrap()
            .value()
            .to_string(),
        "Unsent Record edit"
    );
    let mut after = before.clone();
    after.placements[0].component = ContentKind::Native {
        kind: "text".into(),
        settings: BTreeMap::new(),
        bindings: Vec::new(),
    };
    assert!(apply(app.world_mut(), root, &before, &after).is_err());
    assert_eq!(capture(app.world_mut(), root).unwrap(), before);
}

#[test]
fn builtin_inspection_tracks_manual_area_changes_after_native_creation() {
    let (mut app, root) = fixture();
    let entity = spawn(
        app.world_mut(),
        root,
        1,
        DVec2::ZERO,
        &ContentKind::Builtin {
            state: nucleus::component::ComponentState::Area {
                immunity: nucleus::component::Immunity::None,
                strength: 12,
            },
        },
    )
    .unwrap();
    let mut area = app
        .world_mut()
        .get_mut::<crate::area::InfluenceArea>(entity)
        .unwrap();
    area.immunity = crate::area_effects::Immunity::Isolation;
    area.strength = 23.0;
    assert_eq!(
        capture_content(app.world(), entity),
        ContentKind::Builtin {
            state: nucleus::component::ComponentState::Area {
                immunity: nucleus::component::Immunity::Isolation,
                strength: 23
            }
        }
    );
}

#[test]
fn every_registered_sand_can_spawn_capture_and_recreate_native_state() {
    let (mut app, root) = fixture();
    for kind in SandKind::ALL {
        let component = ContentKind::Native {
            kind: kind_id(kind),
            settings: BTreeMap::new(),
            bindings: Vec::new(),
        };
        let entity = spawn(app.world_mut(), root, 1, DVec2::ZERO, &component).unwrap();
        assert_eq!(app.world().get::<StoredSand>(entity).unwrap().kind, kind);
        let saved = capture_content(app.world(), entity);
        saved.validate(&registry(), false).unwrap();
        let copy = spawn(app.world_mut(), root, 1, DVec2::new(50.0, 50.0), &saved).unwrap();
        assert_eq!(app.world().get::<StoredSand>(copy).unwrap().kind, kind);
        assert_ne!(
            app.world().get::<Identity>(entity).unwrap().0,
            app.world().get::<Identity>(copy).unwrap().0
        );
        assert_eq!(capture_content(app.world(), copy), saved);
        app.world_mut().despawn(entity);
        app.world_mut().despawn(copy);
    }
}

#[test]
fn primary_text_survives_empty_default_text_areas_and_reusable_copies() {
    let (mut app, root) = fixture();
    for kind in [SandKind::Text, SandKind::EditableText] {
        let component = ContentKind::Native {
            kind: kind_id(kind),
            settings: BTreeMap::from([
                ("text".into(), serde_json::json!("Walk daily")),
                ("texts".into(), serde_json::json!("[]")),
            ]),
            bindings: Vec::new(),
        };
        let original = spawn(app.world_mut(), root, 1, DVec2::ZERO, &component).unwrap();
        let captured = capture_content(app.world(), original);
        let ContentKind::Native { settings, .. } = &captured else {
            panic!("The created native text was not captured");
        };
        assert_eq!(settings["text"], "Walk daily");
        let texts = crate::sand_text::snapshot(app.world(), original);
        assert_eq!(texts[0].text, "Walk daily");
        let copy = spawn(app.world_mut(), root, 1, DVec2::ONE, &captured).unwrap();
        assert_eq!(capture_content(app.world(), copy), captured);
        assert_ne!(
            app.world().get::<Identity>(original).unwrap().0,
            app.world().get::<Identity>(copy).unwrap().0
        );
    }
}

#[tokio::test]
async fn native_composition_save_and_close_controls_use_ordinary_actions() {
    let engine = Arc::new(Engine::open_memory().await.unwrap());
    let receiver = engine
        .register_canvas(
            nucleus::new_uid("canvas"),
            "Native fixture".into(),
            registry(),
        )
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
    let (mut app, root) = fixture();
    app.insert_non_send(crate::cell_bridge::connect(
        runtime,
        crate::wake::WakeSignal::new(|| {}),
    ));
    let host = composition::spawn(
        app.world_mut(),
        root,
        1,
        DVec2::ZERO,
        api::Composition {
            name: "Saved native Castle".into(),
            origin: None,
            parts: vec![api::Part {
                id: "note".into(),
                geometry: api::Geometry {
                    position: [0.0, 0.0],
                    size: [250.0, 120.0],
                },
                component: ContentKind::Native {
                    kind: "editabletext".into(),
                    settings: BTreeMap::from([(
                        "text".into(),
                        serde_json::json!("Keep this note"),
                    )]),
                    bindings: Vec::new(),
                },
                events: Vec::new(),
            }],
        },
    )
    .unwrap();
    fn click(world: &mut World, host: Entity, label: &str) {
        let button = world
            .query::<(
                &crate::actions::ActionButton,
                &bevy::a11y::AccessibilityNode,
            )>()
            .iter(world)
            .find(|(button, node)| button.target == host && node.label() == Some(label))
            .map(|(button, _)| button.clone())
            .unwrap();
        button.actions.run(world, button.target);
    }
    click(app.world_mut(), host, "Save component");
    let response = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let response = app
                .world_mut()
                .get_non_send_mut::<crate::cell_bridge::CellBridge>()
                .unwrap()
                .incoming
                .recv()
                .await
                .unwrap();
            if matches!(
                response,
                cell::ServerMessage::ActionOk { .. } | cell::ServerMessage::Error { .. }
            ) {
                break response;
            }
        }
    })
    .await
    .unwrap();
    let cell::ServerMessage::ActionOk {
        created: Some(saved),
        ..
    } = &response
    else {
        panic!("{response:?}")
    };
    let record = store::records::get(&engine.store.pool, saved)
        .await
        .unwrap()
        .unwrap();
    let document = api::Document::decode(&record.body).unwrap();
    assert_eq!(document.name, "Saved native Castle");
    let ContentKind::Composition {
        composition: restored,
    } = document.component
    else {
        panic!("composition")
    };
    assert_eq!(restored, composition::capture(app.world(), host).unwrap());
    composition::receive(app.world_mut(), &response);
    let status = app
        .world()
        .get::<composition::Generated>(host)
        .unwrap()
        .status;
    assert!(
        app.world()
            .get::<Text>(status)
            .unwrap()
            .0
            .starts_with("Saved component")
    );
    click(app.world_mut(), host, "Close");
    assert!(app.world().get_entity(host).is_err());
    let reopened = composition::spawn(app.world_mut(), root, 1, DVec2::ZERO, restored).unwrap();
    assert_eq!(
        app.world()
            .get::<crate::area::InfluenceArea>(reopened)
            .unwrap()
            .immunity,
        crate::area_effects::Immunity::Isolation
    );
    assert!(
        store::records::get(&engine.store.pool, saved)
            .await
            .unwrap()
            .is_some()
    );
    drop(receiver);
}

fn execute(
    world: &mut World,
    root: Entity,
    state: &mut api::State,
    mutation: api::Mutation,
) -> api::Receipt {
    state.synchronize(capture(world, root).unwrap()).unwrap();
    let before = state.snapshot.clone();
    let request = api::Request::Mutate {
        request_id: nucleus::new_uid("request"),
        expected_revision: state.snapshot.revision,
        mutation,
    };
    let api::Response::Receipt { receipt } = state.handle(&request).unwrap() else {
        panic!("receipt");
    };
    apply(world, root, &before, &state.snapshot).unwrap();
    state
        .normalize_applied_snapshot(capture(world, root).unwrap())
        .unwrap();
    world.get_mut::<Workspaces>(root).unwrap().canvas_state = Some(state.clone());
    receipt
}

#[test]
fn native_crud_uses_actual_placements_workspaces_and_guarded_undo() {
    let (mut app, root) = fixture();
    let existing = crate::sand_store::spawn_sand(
        app.world_mut(),
        root,
        1,
        SandKind::EditableText,
        "Human note",
        DVec2::ZERO,
    );
    let mut state = api::State::new(capture(app.world_mut(), root).unwrap(), registry()).unwrap();
    let id = app.world().get::<Identity>(existing).unwrap().0.clone();
    execute(
        app.world_mut(),
        root,
        &mut state,
        api::Mutation::CreateWorkspace {
            name: "Habits".into(),
        },
    );
    execute(
        app.world_mut(),
        root,
        &mut state,
        api::Mutation::RenameWorkspace {
            workspace: 1,
            name: "Home renamed".into(),
        },
    );
    execute(
        app.world_mut(),
        root,
        &mut state,
        api::Mutation::Move {
            placement: id.clone(),
            workspace: 2,
            position: [35.0, 45.0],
        },
    );
    execute(
        app.world_mut(),
        root,
        &mut state,
        api::Mutation::Resize {
            placement: id.clone(),
            size: [350.0, 250.0],
        },
    );
    assert_eq!(app.world().get::<WorkspaceMember>(existing).unwrap().0, 2);
    assert_eq!(
        app.world().get::<CanvasItem>(existing).unwrap().size,
        Vec2::new(350.0, 250.0)
    );
    let removed = execute(
        app.world_mut(),
        root,
        &mut state,
        api::Mutation::Remove {
            placement: id.clone(),
        },
    );
    assert!(app.world().get_entity(existing).is_err());
    execute(
        app.world_mut(),
        root,
        &mut state,
        api::Mutation::Undo {
            receipt: removed.request_id,
        },
    );
    let restored = app
        .world_mut()
        .query::<(Entity, &Identity)>()
        .iter(app.world())
        .find(|(_, identity)| identity.0 == id)
        .unwrap()
        .0;
    assert_eq!(
        crate::sand_text::snapshot(app.world(), restored)[0].text,
        "Human note"
    );
    execute(
        app.world_mut(),
        root,
        &mut state,
        api::Mutation::RemoveWorkspace { workspace: 2 },
    );
    execute(
        app.world_mut(),
        root,
        &mut state,
        api::Mutation::CreateWorkspace {
            name: "Next".into(),
        },
    );
    assert_eq!(state.snapshot.active_workspace, 3);
    let encoded = serde_json::to_vec(&state).unwrap();
    let restored: api::State = serde_json::from_slice(&encoded).unwrap();
    restored.validate().unwrap();
    assert_eq!(restored.snapshot, state.snapshot);
}

#[test]
fn placement_capture_restores_identity_and_complete_metadata_and_copy_gets_fresh_identity() {
    let (mut app, root) = fixture();
    let entity =
        crate::sand_store::spawn_sand(app.world_mut(), root, 1, SandKind::Square, "", DVec2::ZERO);
    app.world_mut().entity_mut(entity).insert((
        Identity(nucleus::new_uid("placement")),
        crate::sand_placement::Pinned {
            anchor: [0.2, 0.8],
            scale: 1.5,
        },
        crate::scoped_events::EventBoundary(vec!["changed".into()]),
        crate::canvas_selection::SandGroup([7; 16]),
        crate::topology::Spatial {
            elevation: 20.0,
            ..default()
        },
        ZIndex(9),
    ));
    let saved = crate::sand_placement::Placement::capture(app.world(), entity);
    let bytes = serde_json::to_vec(&saved).unwrap();
    let saved: crate::sand_placement::Placement = serde_json::from_slice(&bytes).unwrap();
    assert!(saved.valid());
    let copy =
        crate::sand_store::spawn_sand(app.world_mut(), root, 1, SandKind::Square, "", DVec2::ZERO);
    saved.restore(app.world_mut(), copy);
    assert_eq!(
        app.world().get::<Identity>(entity).unwrap().0,
        app.world().get::<Identity>(copy).unwrap().0
    );
    let snapshot = capture(app.world_mut(), root).unwrap();
    assert_ne!(snapshot.placements[0].id, snapshot.placements[1].id);
    assert!(
        snapshot
            .placements
            .iter()
            .all(|placement| placement.metadata.order == 9
                && placement.metadata.event_boundary == ["changed"]
                && placement.metadata.spatial.elevation == 20.0)
    );
}

#[test]
fn native_composition_has_known_balloon_internal_events_and_independent_copies() {
    let (mut app, root) = fixture();
    let composition = api::Composition {
        name: "Habits".into(),
        origin: Some(nucleus::component::composition::Origin {
            agent: nucleus::new_uid("r"),
            thread: nucleus::new_uid("r"),
        }),
        parts: vec![api::Part {
            id: "note".into(),
            geometry: api::Geometry {
                position: [0.0, 0.0],
                size: [300.0, 200.0],
            },
            component: ContentKind::Native {
                kind: "editabletext".into(),
                settings: BTreeMap::from([("text".into(), serde_json::json!("Habit plan"))]),
                bindings: vec![],
            },
            events: vec![],
        }],
    };
    let first = spawn(
        app.world_mut(),
        root,
        1,
        DVec2::ZERO,
        &ContentKind::Composition {
            composition: composition.clone(),
        },
    )
    .unwrap();
    let second = spawn(
        app.world_mut(),
        root,
        1,
        DVec2::ONE,
        &ContentKind::Composition {
            composition: composition.clone(),
        },
    )
    .unwrap();
    assert_eq!(
        app.world()
            .get::<crate::area::InfluenceArea>(first)
            .unwrap()
            .immunity,
        crate::area_effects::Immunity::Isolation
    );
    assert_ne!(
        app.world().get::<Identity>(first).unwrap().0,
        app.world().get::<Identity>(second).unwrap().0
    );
    let captured = composition::capture(app.world(), first).unwrap();
    assert_eq!(captured.origin, composition.origin);
    assert_eq!(captured.name, composition.name);
    assert_eq!(captured.parts[0].geometry, composition.parts[0].geometry);
    let inner = app
        .world_mut()
        .query_filtered::<Entity, With<crate::component_push::composition::GeneratedCanvas>>()
        .iter(app.world())
        .next()
        .unwrap();
    assert!(
        app.world()
            .get::<crate::scoped_events::IsolatedEvents>(inner)
            .is_some()
    );
    let source = app.world().get::<Children>(inner).unwrap()[0];
    let internal = app
        .world_mut()
        .spawn((
            ChildOf(inner),
            crate::scoped_events::EventListener(vec!["test".into()]),
        ))
        .id();
    let external = app
        .world_mut()
        .spawn((
            ChildOf(root),
            crate::scoped_events::EventListener(vec!["test".into()]),
        ))
        .id();
    #[derive(Resource, Default)]
    struct Events(Vec<Entity>);
    app.init_resource::<Events>().add_observer(
        |event: On<crate::scoped_events::SandEvent>, mut events: ResMut<Events>| {
            events.0.push(event.entity)
        },
    );
    crate::scoped_events::emit(app.world_mut(), source, "test", serde_json::Value::Null);
    app.world_mut().flush();
    assert_eq!(app.world().resource::<Events>().0, [internal]);
    app.world_mut().resource_mut::<Events>().0.clear();
    crate::scoped_events::emit(app.world_mut(), external, "test", serde_json::Value::Null);
    app.world_mut().flush();
    assert_eq!(app.world().resource::<Events>().0, [external]);
}

#[tokio::test]
async fn engine_request_is_acknowledged_after_native_application_without_cell_callback() {
    let engine = Arc::new(Engine::open_memory().await.unwrap());
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
    let (mut app, root) = fixture();
    app.insert_resource(crate::app::CellHandle(runtime))
        .add_plugins(Plugin);
    app.update();
    assert_eq!(engine.connected_canvases().len(), 1);
    let placement = nucleus::new_uid("placement");
    let id = placement.clone();
    let revision = app
        .world()
        .get::<Workspaces>(root)
        .unwrap()
        .canvas_state
        .as_ref()
        .unwrap()
        .snapshot
        .revision;
    let task = tokio::spawn(async move {
        engine
            .act(
                Action::Canvas {
                    canvas: None,
                    request: api::Request::Mutate {
                        request_id: nucleus::new_uid("request"),
                        expected_revision: revision,
                        mutation: api::Mutation::Add {
                            placement: id,
                            workspace: 1,
                            component: ContentKind::Native {
                                kind: "text".into(),
                                settings: BTreeMap::from([(
                                    "text".into(),
                                    serde_json::json!("Actual native result"),
                                )]),
                                bindings: vec![],
                            },
                            geometry: api::Geometry {
                                position: [20.0, 30.0],
                                size: [200.0, 100.0],
                            },
                        },
                    },
                },
                None,
            )
            .await
    });
    for _ in 0..100 {
        app.update();
        if task.is_finished() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    let outcome = task.await.unwrap().unwrap();
    assert!(outcome.data.is_some());
    let entity = app
        .world_mut()
        .query::<(Entity, &Identity)>()
        .iter(app.world())
        .find(|(_, id)| id.0 == placement)
        .unwrap()
        .0;
    assert_eq!(
        crate::sand_text::snapshot(app.world(), entity)[0].text,
        "Actual native result"
    );
    assert_eq!(
        app.world().get::<CanvasItem>(entity).unwrap().position,
        DVec2::new(20.0, 30.0)
    );
}

#[test]
fn disk_restart_keeps_canvas_identity_placements_protection_and_saved_receipts() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("interface.json");
    fn open(path: std::path::PathBuf) -> (App, Entity) {
        let mut app = App::new();
        crate::laboratory::isolate(app.world_mut());
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Font>>()
            .init_resource::<Assets<Image>>()
            .init_resource::<crate::theme::Typography>()
            .init_resource::<bevy::input_focus::InputFocus>()
            .insert_resource(crate::workspace::WorkspaceFile::new(path))
            .add_plugins((crate::workspace::WorkspacePlugin, Plugin));
        let root = app.world_mut().spawn(crate::container::BoxRoot).id();
        app.update();
        (app, root)
    }
    let (mut app, root) = open(path.clone());
    let canvas_id = app
        .world()
        .get::<Workspaces>(root)
        .unwrap()
        .canvas_id
        .clone();
    let mut state = app
        .world()
        .get::<Workspaces>(root)
        .unwrap()
        .canvas_state
        .clone()
        .unwrap();
    let placement = nucleus::new_uid("placement");
    let composition = api::Composition {
        name: "Saved habits".into(),
        origin: Some(nucleus::component::composition::Origin {
            agent: nucleus::new_uid("r"),
            thread: nucleus::new_uid("r"),
        }),
        parts: vec![api::Part {
            id: "text".into(),
            geometry: api::Geometry {
                position: [0.0, 0.0],
                size: [200.0, 100.0],
            },
            component: ContentKind::Native {
                kind: "text".into(),
                settings: BTreeMap::from([("text".into(), serde_json::json!("Saved note"))]),
                bindings: vec![],
            },
            events: vec![],
        }],
    };
    let receipt = execute(
        app.world_mut(),
        root,
        &mut state,
        api::Mutation::Add {
            placement: placement.clone(),
            workspace: 1,
            component: ContentKind::Composition {
                composition: composition.clone(),
            },
            geometry: api::Geometry {
                position: [10.0, 20.0],
                size: [520.0, 420.0],
            },
        },
    );
    app.world_mut().write_message(AppExit::Success);
    app.update();
    assert!(crate::workspace::saved_canvas_revision(app.world(), root) > 0);
    let other_canvas = app.world_mut().spawn(Workspaces::default()).id();
    assert_eq!(
        crate::workspace::saved_canvas_revision(app.world(), other_canvas),
        0
    );
    drop(app);
    let (mut app, root) = open(path);
    assert_eq!(
        app.world().get::<Workspaces>(root).unwrap().canvas_id,
        canvas_id
    );
    let mut state = app
        .world()
        .get::<Workspaces>(root)
        .unwrap()
        .canvas_state
        .clone()
        .unwrap();
    let restored = app
        .world_mut()
        .query::<(Entity, &Identity)>()
        .iter(app.world())
        .find(|(_, id)| id.0 == placement)
        .unwrap()
        .0;
    assert_eq!(
        composition::capture(app.world(), restored).unwrap().origin,
        composition.origin
    );
    assert_eq!(
        app.world()
            .get::<crate::area::InfluenceArea>(restored)
            .unwrap()
            .immunity,
        crate::area_effects::Immunity::Isolation
    );
    let api::Response::Receipt { receipt } = state
        .handle(&api::Request::Receipt {
            request_id: receipt.request_id,
        })
        .unwrap()
    else {
        panic!("receipt");
    };
    assert_eq!(receipt.persistence, api::Persistence::Saved);
}

#[test]
fn populated_native_canvas_pages_and_manual_workspace_allocation_keep_bounded_contracts() {
    let (mut app, root) = fixture();
    for index in 0..300 {
        crate::sand_store::spawn_sand(
            app.world_mut(),
            root,
            1,
            SandKind::Square,
            "",
            DVec2::new(f64::from(index), 0.0),
        );
    }
    let mut state = api::State::new(capture(app.world_mut(), root).unwrap(), registry()).unwrap();
    let api::Response::Snapshot { snapshot } = state
        .handle(&api::Request::Inspect {
            workspace: Some(1),
            selected_only: false,
            offset: 0,
            limit: 256,
        })
        .unwrap()
    else {
        panic!("snapshot");
    };
    assert_eq!(snapshot.placements.len(), 256);
    assert_eq!(snapshot.next_offset, Some(256));
    assert!(serde_json::to_vec(&snapshot).unwrap().len() <= api::MAX_BYTES);
    app.world_mut()
        .get_mut::<Workspaces>(root)
        .unwrap()
        .canvas_state = Some(state);
    crate::workspace::create(app.world_mut(), root);
    assert!(crate::workspace::remove(app.world_mut(), root, 2));
    crate::workspace::create(app.world_mut(), root);
    assert_eq!(app.world().get::<Workspaces>(root).unwrap().active, 3);
    let snapshot = capture(app.world_mut(), root).unwrap();
    assert_eq!(snapshot.placements.len(), 300);
}

#[test]
fn active_influence_stays_inside_balloon_and_outside_influence_stays_outside() {
    let (mut app, root) = fixture();
    let component = ContentKind::Composition {
        composition: api::Composition {
            name: "Isolated effects".into(),
            origin: None,
            parts: vec![api::Part {
                id: "inside".into(),
                geometry: api::Geometry {
                    position: [80.0, 0.0],
                    size: [40.0, 40.0],
                },
                component: ContentKind::Native {
                    kind: "square".into(),
                    settings: BTreeMap::new(),
                    bindings: vec![],
                },
                events: vec![],
            }],
        },
    };
    spawn(app.world_mut(), root, 1, DVec2::ZERO, &component).unwrap();
    let inner = app
        .world_mut()
        .query_filtered::<Entity, With<crate::component_push::composition::GeneratedCanvas>>()
        .single(app.world())
        .unwrap();
    let inside = app.world().get::<Children>(inner).unwrap()[0];
    let outside = crate::sand_store::spawn_sand(
        app.world_mut(),
        root,
        1,
        SandKind::Square,
        "",
        DVec2::new(80.0, 0.0),
    );
    for entity in [inside, outside] {
        app.world_mut()
            .entity_mut(entity)
            .insert(crate::area::RecordProperties(
                serde_json::json!({"uid":nucleus::new_uid("r"),"quantity":-3}),
            ));
    }
    fn emitter(world: &mut World, root: Entity, strength: f64) -> Entity {
        let mut area = crate::area::InfluenceArea::new(
            crate::area::AreaShape::Square,
            DVec2::ZERO,
            DVec2::splat(100.0),
        );
        area.strength = strength;
        area.reach.mode = crate::area::ReachMode::Unlimited;
        area.rules.push(crate::area::PropertyRule {
            property: crate::area::Property::Quantity,
            value: "-3".into(),
        });
        crate::area::spawn_area(world, root, 1, area).unwrap()
    }
    let internal = emitter(app.world_mut(), inner, 50.0);
    crate::area_effects::update(app.world_mut());
    assert!(
        app.world()
            .get::<crate::area::AreaForces>(inside)
            .unwrap()
            .total()
            .length()
            > 0.0
    );
    assert_eq!(
        app.world()
            .get::<crate::area::AreaForces>(outside)
            .unwrap()
            .total(),
        DVec2::ZERO
    );
    app.world_mut().despawn(internal);
    emitter(app.world_mut(), root, 200.0);
    crate::area_effects::update(app.world_mut());
    assert_eq!(
        app.world()
            .get::<crate::area::AreaForces>(inside)
            .unwrap()
            .total(),
        DVec2::ZERO
    );
    assert!(
        app.world()
            .get::<crate::area::AreaForces>(outside)
            .unwrap()
            .total()
            .length()
            > 0.0
    );
}

#[test]
fn canvas_automation_cannot_move_or_delete_hosted_replicas_using_the_panels_session() {
    let (mut app, root) = fixture();
    let entity = crate::sand_store::spawn_sand(
        app.world_mut(),
        root,
        1,
        SandKind::Text,
        "Replica",
        DVec2::ZERO,
    );
    app.world_mut()
        .entity_mut(entity)
        .insert(crate::workspace_sync::HostedReplica);
    let before = capture(app.world_mut(), root).unwrap();
    let mut moved = before.clone();
    moved.placements[0].geometry.position = [100.0, 0.0];
    assert!(apply(app.world_mut(), root, &before, &moved).is_err());
    let mut removed = before.clone();
    removed.placements.clear();
    assert!(apply(app.world_mut(), root, &before, &removed).is_err());
    assert_eq!(capture(app.world_mut(), root).unwrap(), before);
    let mut selected = before.clone();
    selected.placements[0].selected = true;
    apply(app.world_mut(), root, &before, &selected).unwrap();
    assert_eq!(
        app.world().get::<CanvasItem>(entity).unwrap().position,
        DVec2::ZERO
    );
}
