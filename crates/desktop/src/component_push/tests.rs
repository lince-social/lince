use super::*;

fn setup() -> (App, Entity) {
    let mut app = App::new();
    crate::laboratory::isolate(app.world_mut());
    app.add_plugins(MinimalPlugins)
        .init_resource::<Assets<Font>>()
        .init_resource::<Assets<Image>>()
        .init_resource::<crate::theme::Typography>()
        .init_resource::<bevy::input_focus::InputFocus>()
        .add_plugins(ComponentPushPlugin);
    let root = app.world_mut().spawn((BoxRoot, Workspaces::default())).id();
    (app, root)
}

#[test]
fn generated_compositions_are_isolated_saved_and_reopened_as_independent_balloons() {
    let (mut app, root) = setup();
    let composition = nucleus::component::Composition {
        name: "Planning".into(),
        origin: None,
        parts: vec![
            nucleus::component::Part {
                id: "note".into(),
                events: Vec::new(),
                position: [0, 0],
                size: [480, 320],
                component: ComponentState::Text {
                    text: "A reusable note".into(),
                },
            },
            nucleus::component::Part {
                id: "area".into(),
                events: Vec::new(),
                position: [200, 0],
                size: [200, 200],
                component: ComponentState::Area {
                    immunity: Default::default(),
                    strength: 19,
                },
            },
        ],
    };
    let state = Presentation {
        slot: "planning".into(),
        component: ComponentState::Composition {
            composition: composition.clone(),
        },
    };
    let first = present(app.world_mut(), state.clone()).unwrap();
    assert!(app.world().get::<Node>(first).is_some());
    assert_eq!(present(app.world_mut(), state).unwrap(), first);
    assert_eq!(
        app.world().get::<InfluenceArea>(first).unwrap().immunity,
        crate::area_effects::Immunity::Isolation
    );
    let inner = app
        .world_mut()
        .query_filtered::<Entity, With<composition::GeneratedCanvas>>()
        .single(app.world())
        .unwrap();
    assert_ne!(inner, root);
    assert!(
        app.world()
            .get::<crate::scoped_events::IsolatedEvents>(inner)
            .is_some()
    );
    let note = app
        .world_mut()
        .query::<(Entity, &crate::sand_store::StoredSand, &ChildOf)>()
        .iter(app.world())
        .find(|(_, _, parent)| parent.parent() == inner)
        .map(|(entity, _, _)| entity)
        .unwrap();
    assert_eq!(
        crate::sand_text::snapshot(app.world(), note)[0].text,
        "A reusable note"
    );
    app.world_mut()
        .entity_mut(note)
        .insert(crate::area::RecordProperties(
            serde_json::json!({"quantity":-1}),
        ));
    let area = app
        .world_mut()
        .query::<(Entity, &InfluenceArea, &ChildOf)>()
        .iter(app.world())
        .find(|(_, _, parent)| parent.parent() == inner)
        .map(|(entity, _, _)| entity)
        .unwrap();
    app.world_mut()
        .get_mut::<InfluenceArea>(area)
        .unwrap()
        .reach
        .mode = crate::area::ReachMode::Unlimited;
    app.world_mut()
        .get_mut::<InfluenceArea>(area)
        .unwrap()
        .rules
        .push(crate::area::PropertyRule {
            property: crate::area::Property::Quantity,
            value: "-1".into(),
        });
    let mut outside =
        InfluenceArea::new(rectangle(), DVec2::new(-1000.0, 0.0), DVec2::splat(10000.0));
    outside.reach.mode = crate::area::ReachMode::Unlimited;
    outside.strength = 1000.0;
    outside.rules.push(crate::area::PropertyRule {
        property: crate::area::Property::Quantity,
        value: "-1".into(),
    });
    let outside = crate::area::spawn_area(app.world_mut(), root, 1, outside).unwrap();
    let outside_sand = app
        .world_mut()
        .spawn((
            CanvasItem {
                position: DVec2::ZERO,
                size: Vec2::splat(20.0),
            },
            crate::area::RecordProperties(serde_json::json!({"quantity":-1})),
            ChildOf(root),
            WorkspaceMember(1),
        ))
        .id();
    crate::area_effects::update(app.world_mut());
    let forces = app.world().get::<crate::area::AreaForces>(note).unwrap();
    assert!(!forces.0.iter().any(|force| force.area == outside));
    assert!(forces.0.iter().any(|force| force.area == area));
    assert!(
        app.world()
            .get::<crate::area::AreaForces>(outside_sand)
            .unwrap()
            .0
            .iter()
            .any(|force| force.area == outside)
    );
    let record = app
        .world()
        .get::<crate::area::RecordProperties>(outside_sand)
        .unwrap()
        .clone();
    assert!(!crate::area_effects::blocked(
        app.world_mut(),
        root,
        1,
        outside,
        DVec2::ZERO,
        Some(&record),
        None
    ));
    assert!(!crate::topology::influence::blocked(
        app.world_mut(),
        root,
        1,
        outside,
        bevy::math::DVec3::ZERO,
        Some(&record),
        None,
        false
    ));
    {
        let mut area = app.world_mut().get_mut::<InfluenceArea>(area).unwrap();
        area.strength = 23.0;
        area.immunity = crate::area_effects::Immunity::Isolation;
    }
    let captured = composition::capture(app.world(), first).unwrap();
    assert_eq!(
        captured.parts[1].component,
        ComponentState::Area {
            immunity: nucleus::component::Immunity::Isolation,
            strength: 23
        }
    );
    let document = nucleus::component::Document::encode(composition.clone()).unwrap();
    assert_eq!(
        nucleus::component::Document::decode(&document)
            .unwrap()
            .composition,
        composition
    );
    let placement = crate::sand_placement::Placement::capture(app.world(), first);
    let area = app.world().get::<InfluenceArea>(first).unwrap().clone();
    app.world_mut().despawn(first);
    assert!(app.world().get_entity(inner).is_err());
    let reopened = crate::area::spawn_area(app.world_mut(), root, 1, area).unwrap();
    placement.restore(app.world_mut(), reopened);
    assert!(
        app.world()
            .get::<composition::Generated>(reopened)
            .is_some()
    );
    let copy = present(
        app.world_mut(),
        Presentation {
            slot: "copy".into(),
            component: ComponentState::Composition { composition },
        },
    )
    .unwrap();
    assert_ne!(copy, reopened);
    assert_eq!(
        app.world_mut()
            .query_filtered::<Entity, With<composition::GeneratedCanvas>>()
            .iter(app.world())
            .count(),
        2
    );
}

fn request(component: ComponentState) -> Presentation {
    Presentation {
        slot: "reminder:record".into(),
        component,
    }
}

#[tokio::test]
async fn generated_buttons_apply_actions_save_and_close_through_the_native_bridge() {
    let engine = std::sync::Arc::new(engine::Engine::open_memory().await.unwrap());
    let target = engine
        .act(
            engine::actions::Action::CreateRecord {
                slug: None,
                kind: nucleus::RecordKind::Plain,
                head: "Interaction target".into(),
                body: String::new(),
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
        lanes: std::sync::Arc::new(cell::LaneHub::new()),
        wire: Default::default(),
        fiote: None,
        speech: None,
        information: None,
    };
    let bridge = crate::cell_bridge::connect(runtime, crate::wake::WakeSignal::new(|| {}));
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<Assets<Font>>()
        .init_resource::<Assets<Image>>()
        .init_resource::<crate::theme::Typography>()
        .init_resource::<bevy::input_focus::InputFocus>()
        .add_plugins(ComponentPushPlugin);
    app.insert_non_send(bridge);
    app.world_mut().spawn((BoxRoot, Workspaces::default()));
    let composition = nucleus::component::Composition {
        name: "Interaction".into(),
        origin: None,
        parts: vec![nucleus::component::Part {
            id: "apply".into(),
            position: [0, 0],
            size: [300, 100],
            events: vec![nucleus::component::composition::EventBinding {
                event: "commit".into(),
                action: serde_json::to_value(engine::actions::Action::SetQuantityExact {
                    target: target.clone(),
                    amount: "9".into(),
                })
                .unwrap(),
            }],
            component: ComponentState::Button {
                label: "Apply quantity".into(),
                action: serde_json::to_value(engine::actions::Action::SetQuantityExact {
                    target: target.clone(),
                    amount: "7".into(),
                })
                .unwrap(),
            },
        }],
    };
    let host = present(
        app.world_mut(),
        Presentation {
            slot: "interaction".into(),
            component: ComponentState::Composition { composition },
        },
    )
    .unwrap();
    fn click(world: &mut World, caption: &str) {
        let button = world
            .query::<(
                &crate::actions::ActionButton,
                &bevy::a11y::AccessibilityNode,
            )>()
            .iter(world)
            .find(|(_, node)| node.label() == Some(caption))
            .map(|(button, _)| button.clone())
            .unwrap();
        button.actions.run(world, button.target);
    }
    async fn outcome(app: &mut App) -> ServerMessage {
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
                if matches!(
                    message,
                    ServerMessage::ActionOk { .. } | ServerMessage::Error { .. }
                ) {
                    return message;
                }
            }
        })
        .await
        .unwrap()
    }
    click(app.world_mut(), "Apply quantity");
    let result = outcome(&mut app).await;
    assert!(
        matches!(result, ServerMessage::ActionOk { .. }),
        "{result:?}"
    );
    composition::receive(app.world_mut(), &result);
    assert_eq!(
        store::records::get(&engine.store.pool, &target)
            .await
            .unwrap()
            .unwrap()
            .quantity
            .to_string(),
        "7"
    );
    let source = app
        .world_mut()
        .query_filtered::<Entity, With<crate::scoped_events::EventListener>>()
        .single(app.world())
        .unwrap();
    crate::scoped_events::emit(app.world_mut(), source, "commit", serde_json::Value::Null);
    app.world_mut().flush();
    let result = outcome(&mut app).await;
    assert!(
        matches!(result, ServerMessage::ActionOk { .. }),
        "{result:?}"
    );
    composition::receive(app.world_mut(), &result);
    assert_eq!(
        store::records::get(&engine.store.pool, &target)
            .await
            .unwrap()
            .unwrap()
            .quantity
            .to_string(),
        "9"
    );
    click(app.world_mut(), "Save component");
    let result = outcome(&mut app).await;
    let ServerMessage::ActionOk {
        created: Some(saved),
        ..
    } = &result
    else {
        panic!("{result:?}")
    };
    let saved = store::records::get(&engine.store.pool, saved)
        .await
        .unwrap()
        .unwrap();
    let document = nucleus::component::Document::decode(&saved.body).unwrap();
    composition::receive(app.world_mut(), &result);
    click(app.world_mut(), "Close");
    assert!(app.world().get_entity(host).is_err());
    let copy = present(
        app.world_mut(),
        Presentation {
            slot: "saved".into(),
            component: ComponentState::Composition {
                composition: document.composition,
            },
        },
    )
    .unwrap();
    assert!(app.world().get::<composition::Generated>(copy).is_some());
    assert_eq!(
        store::records::get(&engine.store.pool, &target)
            .await
            .unwrap()
            .unwrap()
            .quantity
            .to_string(),
        "9"
    );
}

#[test]
fn repeated_pushes_reuse_one_instance_and_changed_state_replaces_it() {
    let (mut app, _) = setup();
    let state = request(ComponentState::Record {
        record: "room".into(),
        mode: RecordMode::Full,
        start_call: None,
    });
    app.world_mut()
        .write_message(CellMessage(ServerMessage::PresentComponent {
            presentation: state.clone(),
        }));
    app.update();
    let first = app
        .world_mut()
        .query_filtered::<Entity, With<Placed>>()
        .single(app.world())
        .unwrap();
    assert_eq!(present(app.world_mut(), state).unwrap(), first);
    let changed = present(
        app.world_mut(),
        request(ComponentState::Record {
            record: "other".into(),
            mode: RecordMode::Call,
            start_call: None,
        }),
    )
    .unwrap();
    assert_ne!(changed, first);
    assert!(app.world().get_entity(first).is_err());
    assert_eq!(
        app.world_mut().query::<&Placed>().iter(app.world()).count(),
        1
    );
    let config = app
        .world()
        .get::<InfluenceArea>(changed)
        .unwrap()
        .protein
        .as_ref()
        .unwrap();
    assert!(
        config
            .bindings
            .iter()
            .any(|binding| binding.property == "threads")
    );
    assert!(
        !config
            .bindings
            .iter()
            .any(|binding| binding.property == "body")
    );
    assert!(config.query().is_ok());
    let area_id = app.world().get::<Placed>(changed).unwrap().area.clone();
    let area = app
        .world_mut()
        .query::<&InfluenceArea>()
        .iter(app.world())
        .find(|area| area.id == area_id)
        .unwrap();
    assert!(area.contains(app.world().get::<CanvasItem>(changed).unwrap().position));
}

#[test]
fn saved_placement_restores_deduplication_and_closed_components_can_be_shown_again() {
    let (mut app, root) = setup();
    let state = request(ComponentState::Record {
        record: "room".into(),
        mode: RecordMode::Description,
        start_call: None,
    });
    let first = present(app.world_mut(), state.clone()).unwrap();
    let placement = crate::sand_placement::Placement::capture(app.world(), first);
    let serialized = serde_json::to_string(&placement).unwrap();
    let saved: crate::sand_placement::Placement = serde_json::from_str(&serialized).unwrap();
    assert!(saved.valid());
    let area = app.world().get::<InfluenceArea>(first).unwrap().clone();
    app.world_mut().despawn(first);
    let restored = crate::area::spawn_area(app.world_mut(), root, 1, area).unwrap();
    saved.restore(app.world_mut(), restored);
    assert_eq!(present(app.world_mut(), state.clone()).unwrap(), restored);
    app.world_mut().despawn(restored);
    assert_ne!(present(app.world_mut(), state).unwrap(), restored);
}

#[test]
fn each_component_receives_its_own_supported_state() {
    let (mut app, _) = setup();
    let karma = present(
        app.world_mut(),
        Presentation {
            slot: "karma".into(),
            component: ComponentState::Karma {
                search: "habit".into(),
            },
        },
    )
    .unwrap();
    assert_eq!(
        app.world()
            .get::<crate::karma_castle::KarmaCastle>(karma)
            .unwrap()
            .search,
        "habit"
    );
    let frequency = present(
        app.world_mut(),
        Presentation {
            slot: "frequency".into(),
            component: ComponentState::Frequency {
                search: "weekly".into(),
            },
        },
    )
    .unwrap();
    assert_eq!(
        app.world()
            .get::<crate::frequency_castle::FrequencyCastle>(frequency)
            .unwrap()
            .search,
        "weekly"
    );
    let text = present(
        app.world_mut(),
        Presentation {
            slot: "text".into(),
            component: ComponentState::Text {
                text: "Call family".into(),
            },
        },
    )
    .unwrap();
    assert_eq!(
        crate::sand_text::snapshot(app.world(), text)[0].text,
        "Call family"
    );
}

#[tokio::test]
async fn backend_push_reaches_the_native_component_area_through_the_cell_bridge() {
    let engine = std::sync::Arc::new(engine::Engine::open_memory().await.unwrap());
    store::organs::ensure_local(&engine.store.pool, "http://components.test")
        .await
        .unwrap();
    let target = engine
        .act(
            engine::actions::Action::CreateRecord {
                slug: Some("room".into()),
                kind: nucleus::RecordKind::Plain,
                head: "Room".into(),
                body: String::new(),
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
        lanes: std::sync::Arc::new(cell::LaneHub::new()),
        wire: Default::default(),
        fiote: None,
        speech: None,
        information: None,
    };
    let mut bridge = crate::cell_bridge::connect(runtime, crate::wake::WakeSignal::new(|| {}));
    engine
        .act(
            engine::actions::Action::PresentComponent {
                target: target.clone(),
                component: ComponentState::Record {
                    record: target,
                    mode: RecordMode::Call,
                    start_call: None,
                },
            },
            None,
        )
        .await
        .unwrap();
    let message = tokio::time::timeout(std::time::Duration::from_secs(5), bridge.incoming.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(message, ServerMessage::PresentComponent { .. }));
    let (mut app, _) = setup();
    app.world_mut().write_message(CellMessage(message));
    app.update();
    let placed = app
        .world_mut()
        .query::<&Placed>()
        .single(app.world())
        .unwrap();
    assert!(matches!(
        placed.component,
        ComponentState::Record {
            mode: RecordMode::Call,
            ..
        }
    ));
}

#[cfg(not(feature = "native-media"))]
#[test]
fn automatic_call_reports_unavailable_media_without_duplicating_components() {
    let (mut app, _) = setup();
    let state = request(ComponentState::Record {
        record: "room".into(),
        mode: RecordMode::Call,
        start_call: Some(nucleus::component::CallStart {
            thread: "thread".into(),
            person: "person".into(),
            media: Default::default(),
        }),
    });
    assert!(
        present(app.world_mut(), state.clone())
            .unwrap_err()
            .contains("native media")
    );
    let entity = app
        .world_mut()
        .query_filtered::<Entity, With<Placed>>()
        .single(app.world())
        .unwrap();
    assert!(
        present(app.world_mut(), state)
            .unwrap_err()
            .contains("native media")
    );
    assert_eq!(
        app.world_mut()
            .query_filtered::<Entity, With<Placed>>()
            .single(app.world())
            .unwrap(),
        entity
    );
    assert_eq!(
        app.world_mut().query::<&Placed>().iter(app.world()).count(),
        1
    );
}
