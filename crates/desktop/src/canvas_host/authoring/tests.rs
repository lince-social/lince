use super::*;
use api::authoring::{Change, PartEdit, Style};

fn fixture() -> (App, Entity, Entity, String) {
    let (mut app, root) = crate::canvas_host::tests::fixture();
    let host = crate::sand_store::spawn_sand(
        app.world_mut(),
        root,
        1,
        SandKind::EditableText,
        "Initial text",
        DVec2::ZERO,
    );
    let snapshot = capture(app.world_mut(), root).unwrap();
    (app, root, host, snapshot.placements[0].id.clone())
}

fn inspect(world: &mut World, root: Entity, placement: &str) -> Element {
    let api::Response::Element { element } = crate::canvas_host::handle(
        world,
        root,
        &api::Request::InspectElement {
            placement: placement.into(),
            offset: 0,
            limit: 128,
        },
    )
    .unwrap() else {
        panic!("inspection");
    };
    let mut element = element;
    while let Some(offset) = element.next_offset {
        let api::Response::Element { element: page } = crate::canvas_host::handle(
            world,
            root,
            &api::Request::InspectElement {
                placement: placement.into(),
                offset,
                limit: 128,
            },
        )
        .unwrap() else {
            panic!("inspection page");
        };
        element.parts.extend(page.parts);
        element.next_offset = page.next_offset;
    }
    element
}

fn patch(
    world: &mut World,
    root: Entity,
    placement: &str,
    changes: Vec<Change>,
) -> Result<api::Receipt, String> {
    let revision = capture(world, root)?.revision;
    let response = crate::canvas_host::handle(
        world,
        root,
        &api::Request::Mutate {
            request_id: nucleus::new_uid("request"),
            expected_revision: revision,
            mutation: api::Mutation::Patch {
                placement: placement.into(),
                changes,
            },
        },
    )?;
    let api::Response::Receipt { receipt } = response else {
        return Err("receipt".into());
    };
    Ok(receipt)
}

#[test]
fn layout_and_tokens_preserve_the_editor_draft_and_focus() {
    let (mut app, root, host, placement) = fixture();
    let editor = app
        .world()
        .get::<StoredSand>(host)
        .unwrap()
        .content
        .unwrap();
    app.world_mut()
        .get_mut::<bevy::text::EditableText>(editor)
        .unwrap()
        .editor
        .set_text("An unsaved draft");
    app.world_mut()
        .resource_mut::<bevy::input_focus::InputFocus>()
        .set(editor, bevy::input_focus::FocusCause::Pressed);
    let element = inspect(app.world_mut(), root, &placement);
    let id = element
        .parts
        .iter()
        .find(|part| part.role == "editable-text")
        .unwrap()
        .id
        .clone();
    let receipt = patch(
        app.world_mut(),
        root,
        &placement,
        vec![Change::SetPart {
            part: id.clone(),
            edit: PartEdit {
                style: Style {
                    numbers: BTreeMap::from([("flex-grow".into(), 1.0)]),
                    ..default()
                },
                tokens: BTreeMap::from([(
                    "FontSize".into(),
                    api::authoring::TokenValue::Number(21.0),
                )]),
                ..default()
            },
        }],
    )
    .unwrap();
    assert_eq!(
        app.world().get::<StoredSand>(host).unwrap().content,
        Some(editor)
    );
    assert_eq!(text(app.world(), editor).unwrap(), "An unsaved draft");
    assert_eq!(
        app.world()
            .resource::<bevy::input_focus::InputFocus>()
            .get(),
        Some(editor)
    );
    assert_eq!(
        crate::token_style::resolve(app.world(), editor, Token::FontSize).0,
        TokenValue::Number(21.0)
    );
    let response = crate::canvas_host::handle(
        app.world_mut(),
        root,
        &api::Request::Receipt {
            request_id: receipt.request_id,
        },
    )
    .unwrap();
    assert!(matches!(response, api::Response::Receipt { .. }));
    patch(
        app.world_mut(),
        root,
        &placement,
        vec![Change::ResetPart { part: id }],
    )
    .unwrap();
    assert_eq!(text(app.world(), editor).unwrap(), "An unsaved draft");
    assert_ne!(
        crate::token_style::resolve(app.world(), editor, Token::FontSize).0,
        TokenValue::Number(21.0)
    );
}

#[test]
fn invalid_tokens_unknown_parts_and_cycles_are_atomic() {
    let (mut app, root, host, placement) = fixture();
    inspect(app.world_mut(), root, &placement);
    let before = capture(app.world_mut(), root).unwrap();
    for edit in [
        PartEdit {
            tokens: BTreeMap::from([(
                "FontSize".into(),
                api::authoring::TokenValue::Number(999.0),
            )]),
            ..default()
        },
        PartEdit {
            parent: Some("root".into()),
            ..default()
        },
        PartEdit {
            tokens: BTreeMap::from([(
                "DoesNotExist".into(),
                api::authoring::TokenValue::Number(12.0),
            )]),
            ..default()
        },
    ] {
        assert!(
            patch(
                app.world_mut(),
                root,
                &placement,
                vec![
                    Change::InsertPart {
                        part: "user/staged".into(),
                        definition: AddedPart {
                            parent: "root".into(),
                            component: None,
                            size: [200.0; 2]
                        }
                    },
                    Change::SetPart {
                        part: "root".into(),
                        edit
                    }
                ]
            )
            .is_err()
        );
        assert_eq!(capture(app.world_mut(), root).unwrap(), before);
        assert!(app.world().get_entity(host).is_ok());
    }
    assert!(
        patch(
            app.world_mut(),
            root,
            &placement,
            vec![Change::SetPart {
                part: "missing".into(),
                edit: default()
            }]
        )
        .is_err()
    );
    assert_eq!(capture(app.world_mut(), root).unwrap(), before);
}

#[test]
fn authored_parts_links_behaviors_export_reload_and_guarded_removal() {
    let (mut app, root, host, placement) = fixture();
    let initial = inspect(app.world_mut(), root, &placement);
    let change = Change::InsertPart {
        part: "user/actions".into(),
        definition: AddedPart {
            parent: "root".into(),
            component: None,
            size: [200.0, 40.0],
        },
    };
    patch(app.world_mut(), root, &placement, vec![change, Change::SetPart { part: "user/actions".into(), edit: PartEdit { style: Style { choices: BTreeMap::from([("direction".into(), "row".into()), ("wrap".into(), "wrap".into())]), ..default() }, ..default() } }, Change::SetLink { id: "route".into(), link: api::authoring::Link { from: "root".into(), to: "user/actions".into(), event: "activate".into(), receive: "go".into(), enabled: false } }, Change::SetBehavior { id: "action".into(), behavior: api::authoring::Behavior { part: "user/actions".into(), event: "go".into(), action: serde_json::json!({"action":"create-record","head":"Only on activation","body":"","quantity":0.0,"kind":"plain","slug":null}), enabled: false } }]).unwrap();
    let updated = inspect(app.world_mut(), root, &placement);
    assert_eq!(updated.total_parts, initial.total_parts + 1);
    let exported = crate::canvas_host::handle(
        app.world_mut(),
        root,
        &api::Request::Export {
            placement: placement.clone(),
            name: "Workflow".into(),
        },
    )
    .unwrap();
    let api::Response::Export { document, hash, .. } = exported else {
        panic!("export");
    };
    assert_eq!(api::authoring::hash(&document).unwrap(), hash);
    let decoded = api::Document::decode(&serde_json::to_string(&document).unwrap()).unwrap();
    let copy = super::super::spawn(
        app.world_mut(),
        root,
        1,
        DVec2::new(500.0, 0.0),
        &decoded.component,
    )
    .unwrap();
    maintain(app.world_mut());
    let copy_id = app.world().get::<Identity>(copy).unwrap().0.clone();
    assert_ne!(copy_id, placement);
    let copied = inspect(app.world_mut(), root, &copy_id);
    assert_eq!(
        api::authoring::edits(&copied.placement.component),
        api::authoring::edits(&document.component)
    );
    assert_eq!(
        app.world()
            .get::<Node>(app.world().get::<Editing>(copy).unwrap().added["user/actions"].0)
            .unwrap()
            .flex_direction,
        FlexDirection::Row
    );
    assert!(
        patch(
            app.world_mut(),
            root,
            &placement,
            vec![Change::RemovePart {
                part: "user/actions".into(),
                cascade: false
            }]
        )
        .is_err()
    );
    let removed = patch(
        app.world_mut(),
        root,
        &placement,
        vec![Change::RemovePart {
            part: "user/actions".into(),
            cascade: true,
        }],
    )
    .unwrap();
    assert!(
        api::authoring::edits(&capture_content(app.world(), host))
            .links
            .is_empty()
    );
    let revision = capture(app.world_mut(), root).unwrap().revision;
    crate::canvas_host::handle(
        app.world_mut(),
        root,
        &api::Request::Mutate {
            request_id: nucleus::new_uid("request"),
            expected_revision: revision,
            mutation: api::Mutation::Undo {
                receipt: removed.request_id,
            },
        },
    )
    .unwrap();
    assert!(
        api::authoring::edits(&capture_content(app.world(), host))
            .added
            .contains_key("user/actions")
    );
}

#[test]
fn deleting_record_parts_with_a_draft_fails_without_losing_state() {
    let (mut app, root, host, placement) = fixture();
    let editor = app
        .world()
        .get::<StoredSand>(host)
        .unwrap()
        .content
        .unwrap();
    let status = app.world_mut().spawn(Text::default()).id();
    app.world_mut()
        .entity_mut(editor)
        .insert(crate::record_view::RecordEditor {
            uid: nucleus::new_uid("r"),
            confirmed: "Initial text".into(),
            pending: None,
            status,
        });
    app.world_mut()
        .get_mut::<bevy::text::EditableText>(editor)
        .unwrap()
        .editor
        .set_text("Record draft");
    let element = inspect(app.world_mut(), root, &placement);
    let id = element
        .parts
        .iter()
        .find(|part| part.role == "editable-text")
        .unwrap()
        .id
        .clone();
    assert!(
        patch(
            app.world_mut(),
            root,
            &placement,
            vec![Change::RemovePart {
                part: id,
                cascade: false
            }]
        )
        .unwrap_err()
        .contains("draft")
    );
    assert_eq!(text(app.world(), editor).unwrap(), "Record draft");
}

#[test]
fn area_configuration_keeps_identity_and_requires_explicit_activation() {
    let (mut app, root) = crate::canvas_host::tests::fixture();
    let component = ContentKind::Native {
        kind: "area".into(),
        settings: BTreeMap::from([(
            "configuration".into(),
            serde_json::json!(
                "{\"name\":\"Workflow area\",\"strength\":42.0,\"immunity\":\"Containment\",\"hole\":true}"
            ),
        )]),
        bindings: Vec::new(),
    };
    let host = super::super::spawn(app.world_mut(), root, 1, DVec2::ZERO, &component).unwrap();
    let identity = app
        .world()
        .get::<crate::area::InfluenceArea>(host)
        .unwrap()
        .id
        .clone();
    assert!(
        app.world()
            .get::<crate::area::InfluenceArea>(host)
            .unwrap()
            .paused
    );
    let before = capture(app.world_mut(), root).unwrap();
    let mut after = before.clone();
    let ContentKind::Native { settings, .. } = &mut after.placements[0].component else {
        panic!("native area");
    };
    settings.insert("activate".into(), serde_json::json!(true));
    super::super::apply(app.world_mut(), root, &before, &after).unwrap();
    let area = app.world().get::<crate::area::InfluenceArea>(host).unwrap();
    assert_eq!(area.id, identity);
    assert_eq!(area.strength, 42.0);
    assert!(area.hole);
    assert!(!area.paused);
    assert!(!crate::area_mutation::armed(app.world(), host));
}

#[test]
fn repeated_maintenance_retains_parts_and_observers() {
    let (mut app, root, host, placement) = fixture();
    inspect(app.world_mut(), root, &placement);
    patch(
        app.world_mut(),
        root,
        &placement,
        vec![Change::SetPart {
            part: "root".into(),
            edit: PartEdit {
                tokens: BTreeMap::from([(
                    "Roundness".into(),
                    api::authoring::TokenValue::Number(6.0),
                )]),
                ..default()
            },
        }],
    )
    .unwrap();
    let count = app.world().entities().len();
    for _ in 0..100 {
        maintain(app.world_mut());
    }
    assert_eq!(app.world().entities().len(), count);
    assert!(app.world().get_entity(host).is_ok());
}

#[tokio::test]
async fn authored_activation_routes_execute_once_and_disabled_behaviors_stay_inactive() {
    let engine = std::sync::Arc::new(engine::Engine::open_memory().await.unwrap());
    let runtime = cell::CellRuntime {
        commands: default(),
        store: engine.store.clone(),
        engine: engine.clone(),
        lanes: std::sync::Arc::new(cell::LaneHub::new()),
        wire: default(),
        fiote: None,
        speech: None,
        information: None,
    };
    let (mut app, root, host, placement) = fixture();
    app.insert_non_send(crate::cell_bridge::connect(
        runtime,
        crate::wake::WakeSignal::new(|| {}),
    ));
    fn action(head: &str) -> engine::actions::Action {
        engine::actions::Action::CreateRecord {
            slug: None,
            kind: nucleus::RecordKind::Plain,
            head: head.into(),
            body: String::new(),
            quantity: 0.0,
        }
    }
    let mut behavior = api::authoring::Behavior {
        part: "user/action".into(),
        event: "go".into(),
        action: serde_json::to_value(action("Activated behavior")).unwrap(),
        enabled: true,
    };
    patch(
        app.world_mut(),
        root,
        &placement,
        vec![
            Change::InsertPart {
                part: "user/action".into(),
                definition: AddedPart {
                    parent: "root".into(),
                    component: None,
                    size: [100.0; 2],
                },
            },
            Change::SetBehavior {
                id: "create".into(),
                behavior: behavior.clone(),
            },
            Change::SetLink {
                id: "forward".into(),
                link: api::authoring::Link {
                    from: "root".into(),
                    to: "user/action".into(),
                    event: "activate".into(),
                    receive: "go".into(),
                    enabled: true,
                },
            },
            Change::SetLink {
                id: "feedback".into(),
                link: api::authoring::Link {
                    from: "user/action".into(),
                    to: "root".into(),
                    event: "go".into(),
                    receive: "activate".into(),
                    enabled: true,
                },
            },
        ],
    )
    .unwrap();
    assert!(
        store::records::list_all(&engine.store.pool)
            .await
            .unwrap()
            .iter()
            .all(|record| record.head != "Activated behavior")
    );
    app.world_mut()
        .trigger(bevy::ui_widgets::Activate { entity: host });
    app.world_mut().flush();
    async fn created(app: &mut App) -> String {
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                let message = app
                    .world_mut()
                    .get_non_send_mut::<crate::cell_bridge::CellBridge>()
                    .unwrap()
                    .incoming
                    .recv()
                    .await
                    .unwrap();
                match message {
                    cell::ServerMessage::ActionOk {
                        created: Some(uid), ..
                    } => break uid,
                    cell::ServerMessage::Error { message, .. } => panic!("{message}"),
                    _ => {}
                }
            }
        })
        .await
        .unwrap()
    }
    let record = created(&mut app).await;
    assert_eq!(
        store::records::get(&engine.store.pool, &record)
            .await
            .unwrap()
            .unwrap()
            .head,
        "Activated behavior"
    );
    behavior.enabled = false;
    patch(
        app.world_mut(),
        root,
        &placement,
        vec![Change::SetBehavior {
            id: "create".into(),
            behavior,
        }],
    )
    .unwrap();
    app.world_mut()
        .trigger(bevy::ui_widgets::Activate { entity: host });
    app.world_mut().flush();
    crate::sand_panel::send(
        app.world(),
        cell::ClientMessage::Act {
            id: nucleus::new_uid("barrier"),
            action: action("Following command"),
        },
    )
    .unwrap();
    let following = created(&mut app).await;
    assert_eq!(
        store::records::get(&engine.store.pool, &following)
            .await
            .unwrap()
            .unwrap()
            .head,
        "Following command"
    );
    assert_eq!(
        store::records::list_all(&engine.store.pool)
            .await
            .unwrap()
            .iter()
            .filter(|record| record.head == "Activated behavior")
            .count(),
        1
    );
}

#[test]
fn real_record_castle_property_blocks_reparent_without_replacing_inputs() {
    let (mut app, root) = crate::canvas_host::tests::fixture();
    app.add_plugins(crate::protein_area::ProteinAreaPlugin);
    let uid = nucleus::new_uid("r");
    let component = ContentKind::Builtin {
        state: nucleus::component::ComponentState::Record {
            record: uid.clone(),
            mode: Default::default(),
            start_call: None,
        },
    };
    let host = super::super::spawn(app.world_mut(), root, 1, DVec2::ZERO, &component).unwrap();
    crate::protein_area::tests::authoring_record(
        app.world_mut(),
        host,
        serde_json::json!({"uid":uid,"head":"Native Record","body":"Description","quantity":"0","start_date":"2026-10-11","due_date":"2026-10-12","threads":[]}),
    );
    let placement = capture(app.world_mut(), root).unwrap().placements[0]
        .id
        .clone();
    let element = inspect(app.world_mut(), root, &placement);
    let dates = element
        .parts
        .iter()
        .find(|part| part.name == "dates")
        .unwrap()
        .id
        .clone();
    let planning = element
        .parts
        .iter()
        .find(|part| part.name == "planning-section")
        .unwrap()
        .id
        .clone();
    let before = index(app.world_mut(), host).unwrap();
    let date_entity = before.original[&dates].entity;
    let editors: Vec<_> = subtree(app.world(), date_entity)
        .into_iter()
        .filter(|entity| {
            app.world()
                .get::<bevy::text::EditableText>(*entity)
                .is_some()
        })
        .collect();
    assert!(!editors.is_empty());
    patch(
        app.world_mut(),
        root,
        &placement,
        vec![Change::SetPart {
            part: dates.clone(),
            edit: PartEdit {
                parent: Some(planning.clone()),
                style: Style {
                    choices: BTreeMap::from([
                        ("direction".into(), "row".into()),
                        ("wrap".into(), "wrap".into()),
                    ]),
                    ..default()
                },
                ..default()
            },
        }],
    )
    .unwrap();
    maintain(app.world_mut());
    assert_eq!(
        app.world().get::<ChildOf>(date_entity).unwrap().parent(),
        before.original[&planning].entity
    );
    assert!(editors.iter().all(|entity| {
        app.world()
            .get::<bevy::text::EditableText>(*entity)
            .is_some()
    }));
    assert_eq!(
        app.world().get::<Node>(date_entity).unwrap().flex_wrap,
        FlexWrap::Wrap
    );
}

#[test]
fn workspace_reload_restores_reset_baselines_and_token_inheritance() {
    let (mut app, root, host, placement) = fixture();
    let element = inspect(app.world_mut(), root, &placement);
    let part = element
        .parts
        .iter()
        .find(|part| part.role == "editable-text")
        .unwrap()
        .id
        .clone();
    patch(
        app.world_mut(),
        root,
        &placement,
        vec![Change::SetPart {
            part: part.clone(),
            edit: PartEdit {
                tokens: BTreeMap::from([(
                    "FontSize".into(),
                    api::authoring::TokenValue::Number(22.0),
                )]),
                ..default()
            },
        }],
    )
    .unwrap();
    let saved = crate::sand_placement::Placement::capture(app.world(), host);
    let saved: crate::sand_placement::Placement =
        serde_json::from_value(serde_json::to_value(saved).unwrap()).unwrap();
    let texts = serde_json::to_string(&crate::sand_text::snapshot(app.world(), host)).unwrap();
    let (mut restored, new_root) = crate::canvas_host::tests::fixture();
    let new_host = super::super::spawn(
        restored.world_mut(),
        new_root,
        1,
        DVec2::ZERO,
        &ContentKind::Native {
            kind: "editabletext".into(),
            settings: BTreeMap::from([("texts".into(), serde_json::json!(texts))]),
            bindings: Vec::new(),
        },
    )
    .unwrap();
    saved.restore(restored.world_mut(), new_host);
    maintain(restored.world_mut());
    let editor = restored
        .world()
        .get::<StoredSand>(new_host)
        .unwrap()
        .content
        .unwrap();
    assert_eq!(
        crate::token_style::resolve(restored.world(), editor, Token::FontSize).0,
        TokenValue::Number(22.0)
    );
    patch(
        restored.world_mut(),
        new_root,
        &placement,
        vec![Change::ResetPart { part }],
    )
    .unwrap();
    assert_eq!(
        crate::token_style::resolve(restored.world(), editor, Token::FontSize).0,
        TokenValue::Number(16.0)
    );
}

#[test]
fn a_failed_native_child_factory_leaves_all_prior_widgets_and_geometry_intact() {
    let (mut app, root, host, placement) = fixture();
    inspect(app.world_mut(), root, &placement);
    let before = capture(app.world_mut(), root).unwrap();
    let count = app.world().entities().len();
    let result = patch(
        app.world_mut(),
        root,
        &placement,
        vec![
            Change::SetPart {
                part: "root".into(),
                edit: PartEdit {
                    tokens: BTreeMap::from([(
                        "Width".into(),
                        api::authoring::TokenValue::Number(920.0),
                    )]),
                    ..default()
                },
            },
            Change::InsertPart {
                part: "user/invalid".into(),
                definition: AddedPart {
                    parent: "root".into(),
                    component: Some(ContentKind::Native {
                        kind: "editabletext".into(),
                        settings: BTreeMap::from([(
                            "texts".into(),
                            serde_json::json!("malformed JSON"),
                        )]),
                        bindings: Vec::new(),
                    }),
                    size: [200.0; 2],
                },
            },
        ],
    );
    assert!(result.is_err());
    assert_eq!(capture(app.world_mut(), root).unwrap(), before);
    assert_eq!(app.world().entities().len(), count);
    assert_eq!(
        app.world().get::<CanvasItem>(host).unwrap().size.x,
        before.placements[0].geometry.size[0] as f32
    );
}

#[test]
fn two_saved_authored_sands_have_distinct_parts_inside_one_composition() {
    let (mut app, root) = crate::canvas_host::tests::fixture();
    let leaf = ContentKind::Authored {
        base: Box::new(ContentKind::Native {
            kind: "square".into(),
            settings: BTreeMap::new(),
            bindings: Vec::new(),
        }),
        edits: api::authoring::Edits {
            parts: BTreeMap::from([(
                "root".into(),
                PartEdit {
                    tokens: BTreeMap::from([(
                        "Roundness".into(),
                        api::authoring::TokenValue::Number(4.0),
                    )]),
                    ..default()
                },
            )]),
            ..default()
        },
    };
    let component = ContentKind::Composition {
        composition: api::Composition {
            name: "Two saved items".into(),
            origin: None,
            parts: (0..2)
                .map(|index| api::Part {
                    id: format!("part-{index}"),
                    geometry: api::Geometry {
                        position: [index as f64 * 300.0, 0.0],
                        size: [250.0, 150.0],
                    },
                    component: leaf.clone(),
                    events: Vec::new(),
                })
                .collect(),
        },
    };
    let host = super::super::spawn(app.world_mut(), root, 1, DVec2::ZERO, &component).unwrap();
    maintain(app.world_mut());
    let id = capture(app.world_mut(), root).unwrap().placements[0]
        .id
        .clone();
    let element = inspect(app.world_mut(), root, &id);
    let mut ids = BTreeSet::new();
    assert!(element.parts.iter().all(|part| ids.insert(&part.id)));
    patch(
        app.world_mut(),
        root,
        &id,
        vec![Change::SetPart {
            part: "root".into(),
            edit: PartEdit {
                tokens: BTreeMap::from([(
                    "Padding".into(),
                    api::authoring::TokenValue::Number(12.0),
                )]),
                ..default()
            },
        }],
    )
    .unwrap();
    maintain(app.world_mut());
    assert!(app.world().get_entity(host).is_ok());
}

#[test]
fn native_kanban_copies_remap_regions_and_removal_disposes_owned_regions() {
    let (mut app, root) = crate::canvas_host::tests::fixture();
    let host = super::super::spawn(
        app.world_mut(),
        root,
        1,
        DVec2::ZERO,
        &ContentKind::Native {
            kind: "kanban".into(),
            settings: BTreeMap::new(),
            bindings: Vec::new(),
        },
    )
    .unwrap();
    let before = capture(app.world_mut(), root).unwrap();
    assert_eq!(before.placements.len(), 1);
    let original: BTreeSet<_> = app
        .world()
        .get::<crate::kanban::Kanban>(host)
        .unwrap()
        .ids()
        .cloned()
        .collect();
    let copy = super::super::spawn(
        app.world_mut(),
        root,
        1,
        DVec2::new(5000.0, 0.0),
        &before.placements[0].component,
    )
    .unwrap();
    let cloned: BTreeSet<_> = app
        .world()
        .get::<crate::kanban::Kanban>(copy)
        .unwrap()
        .ids()
        .cloned()
        .collect();
    assert!(original.is_disjoint(&cloned));
    assert_eq!(original.len(), cloned.len());
    assert!(
        app.world_mut()
            .query::<&crate::area::InfluenceArea>()
            .iter(app.world())
            .all(|area| !area.enabled && area.paused)
    );
    let snapshot = capture(app.world_mut(), root).unwrap();
    let copy_id = snapshot
        .placements
        .iter()
        .find(|placement| placement.id != before.placements[0].id)
        .unwrap()
        .id
        .clone();
    crate::canvas_host::handle(
        app.world_mut(),
        root,
        &api::Request::Mutate {
            request_id: nucleus::new_uid("request"),
            expected_revision: snapshot.revision,
            mutation: api::Mutation::Remove { placement: copy_id },
        },
    )
    .unwrap();
    assert!(
        app.world_mut()
            .query::<&crate::area::InfluenceArea>()
            .iter(app.world())
            .all(|area| !cloned.contains(&area.id))
    );
    assert_eq!(capture(app.world_mut(), root).unwrap().placements.len(), 1);
}

#[test]
fn native_layout_factory_keeps_its_rules_in_the_export() {
    let (mut app, root) = crate::canvas_host::tests::fixture();
    let mut rules = crate::layout::Rules::fixed(Vec2::new(500.0, 300.0));
    rules.arrangement = crate::layout::Arrangement::Column;
    let host = super::super::spawn(
        app.world_mut(),
        root,
        1,
        DVec2::ZERO,
        &ContentKind::Native {
            kind: "layout".into(),
            settings: BTreeMap::from([(
                "rules".into(),
                serde_json::json!(serde_json::to_string(&rules).unwrap()),
            )]),
            bindings: Vec::new(),
        },
    )
    .unwrap();
    let ContentKind::Native { kind, settings, .. } = capture_content(app.world(), host) else {
        panic!("layout factory");
    };
    assert_eq!(kind, "layout");
    assert_eq!(
        serde_json::from_str::<crate::layout::Rules>(settings["rules"].as_str().unwrap()).unwrap(),
        rules
    );
}

#[test]
fn layout_membership_changes_and_cycles_use_native_layout_rules() {
    let (mut app, root) = crate::canvas_host::tests::fixture();
    let component = ContentKind::Composition {
        composition: api::Composition {
            name: "Layout test".into(),
            origin: None,
            parts: (0..2)
                .map(|index| api::Part {
                    id: format!("item-{index}"),
                    geometry: api::Geometry {
                        position: [index as f64 * 200.0, 0.0],
                        size: [180.0, 120.0],
                    },
                    component: ContentKind::Native {
                        kind: "square".into(),
                        settings: BTreeMap::new(),
                        bindings: Vec::new(),
                    },
                    events: Vec::new(),
                })
                .collect(),
        },
    };
    let host = super::super::spawn(app.world_mut(), root, 1, DVec2::ZERO, &component).unwrap();
    let placement = capture(app.world_mut(), root).unwrap().placements[0]
        .id
        .clone();
    let editing = index(app.world_mut(), host).unwrap();
    let parts: Vec<_> = editing
        .original
        .iter()
        .filter(|(_, original)| {
            original.entity != host && app.world().get::<CanvasItem>(original.entity).is_some()
        })
        .map(|(id, original)| (id.clone(), original.entity))
        .collect();
    assert_eq!(parts.len(), 2);
    patch(
        app.world_mut(),
        root,
        &placement,
        vec![Change::SetPart {
            part: parts[0].0.clone(),
            edit: PartEdit {
                layout_parent: Some(parts[1].0.clone()),
                ..default()
            },
        }],
    )
    .unwrap();
    assert_eq!(
        app.world()
            .get::<crate::layout::LayoutBox>(parts[0].1)
            .unwrap()
            .parent,
        Some(
            app.world()
                .get::<crate::layout::LayoutBox>(parts[1].1)
                .unwrap()
                .id
        )
    );
    assert!(
        patch(
            app.world_mut(),
            root,
            &placement,
            vec![Change::SetPart {
                part: parts[1].0.clone(),
                edit: PartEdit {
                    layout_parent: Some(parts[0].0.clone()),
                    ..default()
                }
            }]
        )
        .is_err()
    );
    patch(
        app.world_mut(),
        root,
        &placement,
        vec![Change::ResetPart {
            part: parts[0].0.clone(),
        }],
    )
    .unwrap();
    assert_eq!(
        app.world()
            .get::<crate::layout::LayoutBox>(parts[0].1)
            .unwrap()
            .parent,
        None
    );
}
