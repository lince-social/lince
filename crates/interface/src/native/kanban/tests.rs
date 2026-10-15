use super::*;

mod room_mount;

fn fixture() -> (App, Entity, Entity) {
    let mut app = App::new();
    crate::native::laboratory::isolate(app.world_mut());
    app.add_plugins(MinimalPlugins)
        .init_resource::<Assets<Font>>()
        .init_resource::<crate::native::theme::Typography>()
        .init_resource::<bevy::input_focus::InputFocus>()
        .add_plugins((
            crate::native::protein_area::ProteinAreaPlugin,
            crate::native::area_mutation::AreaMutationPlugin,
            crate::native::layout::LayoutPlugin,
            KanbanPlugin,
        ));
    let root = app
        .world_mut()
        .spawn((
            crate::native::container::BoxRoot,
            crate::native::workspace::Workspaces::default(),
        ))
        .id();
    let board = spawn(app.world_mut(), root, 1, DVec2::ZERO).unwrap();
    (app, root, board)
}

async fn until(app: &mut App, predicate: impl Fn(&World) -> bool) {
    let result = tokio::time::timeout(std::time::Duration::from_secs(15), async {
        loop {
            app.update();
            if predicate(app.world()) {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await;
    if result.is_err() {
        let status: Vec<_> = app
            .world_mut()
            .query::<&View>()
            .iter(app.world())
            .map(|view| (view.status.clone(), view.setup))
            .collect();
        let areas: Vec<_> = app
            .world_mut()
            .query::<(Entity, &InfluenceArea)>()
            .iter(app.world())
            .map(|(entity, area)| {
                (
                    area.name.clone(),
                    crate::native::protein_area::calendar_status(app.world(), entity).to_string(),
                    app.world()
                        .get::<crate::native::area_mutation::MutationStatus>(entity)
                        .map(|s| s.0.clone()),
                )
            })
            .collect();
        panic!("Kanban did not settle: {status:?}; {areas:?}");
    }
}

#[test]
fn preset_uses_valid_connected_areas_and_editable_task_fields() {
    let (mut app, root, owner) = fixture();
    let board = app.world().get::<Kanban>(owner).unwrap().clone();
    assert!(board.valid());
    let areas: Vec<_> = app
        .world_mut()
        .query::<&InfluenceArea>()
        .iter(app.world())
        .cloned()
        .collect();
    assert_eq!(areas.len(), 8);
    for area in &areas {
        assert!(area.validate(), "{}", area.name);
        if let Some(config) = area.protein.as_ref().or(area.filter.as_ref()) {
            config.query().unwrap();
        }
    }
    assert_eq!(
        config().bindings,
        crate::native::full_record::config("record", crate::native::protein_area::Source::Local)
            .bindings
    );
    app.update();
    assert_eq!(
        app.world_mut().query::<&Part>().iter(app.world()).count(),
        7
    );
    assert_eq!(
        app.world_mut()
            .query_filtered::<&crate::native::castle::Castle, With<CanvasItem>>()
            .iter(app.world())
            .count(),
        7
    );
    assert!(
        !app.world_mut()
            .query::<&Text>()
            .iter(app.world())
            .any(|text| text.0 == "Kanban")
    );
    let source = area(app.world(), owner, &board.source).unwrap();
    assert_source_covers_columns(app.world(), owner);
    assert_eq!(
        app.world().get::<InfluenceArea>(source).unwrap().immunity,
        crate::native::area_effects::Immunity::Containment
    );
    let saved_source =
        serde_json::to_string(app.world().get::<InfluenceArea>(source).unwrap()).unwrap();
    assert_eq!(
        serde_json::from_str::<InfluenceArea>(&saved_source)
            .unwrap()
            .immunity,
        crate::native::area_effects::Immunity::Containment
    );
    assert_eq!(
        crate::native::topology::groups::members(app.world(), source).len(),
        15
    );
    let saved = snapshot(app.world_mut(), root);
    assert_eq!(saved.len(), 1);
    assert!(saved[0].valid());
}

#[tokio::test]
async fn setup_creation_transfer_and_exit_preserve_card_identity() {
    let (mut app, _, owner) = fixture();
    let engine = std::sync::Arc::new(engine::Engine::open_memory().await.unwrap());
    app.insert_resource(crate::native::app::CellHandle(cell::CellRuntime {
        commands: Default::default(),
        engine: engine.clone(),
        store: engine.store.clone(),
        lanes: std::sync::Arc::new(cell::LaneHub::new()),
        wire: Default::default(),
        fiote: None,
        speech: None,
        information: None,
    }))
    .insert_resource(crate::native::wake::WakeSignal::new(|| {}))
    .add_plugins(crate::native::cell_bridge::CellBridgePlugin);
    app.update();
    let board = app.world().get::<Kanban>(owner).unwrap().clone();
    let backlog = area(app.world(), owner, &board.columns[0].area).unwrap();
    let todo = area(app.world(), owner, &board.columns[1].area).unwrap();
    until(&mut app, |world| {
        crate::native::area_mutation::armed(world, backlog)
            && crate::native::area_mutation::armed(world, todo)
    })
    .await;
    Command::Add(0).apply(app.world_mut(), owner);
    let source = area(app.world(), owner, &board.source).unwrap();
    until(&mut app, |world| {
        crate::native::protein_area::calendar_feed(world, source)
            .is_some_and(|(rows, _)| rows.iter().any(|r| r["head"] == ""))
    })
    .await;
    let uid = crate::native::protein_area::calendar_feed(app.world(), source)
        .unwrap()
        .0
        .iter()
        .find(|r| r["head"] == "")
        .unwrap()["uid"]
        .as_str()
        .unwrap()
        .to_string();
    until(&mut app, |world| {
        world
            .get::<Children>(world.get::<ChildOf>(owner).unwrap().parent())
            .unwrap()
            .iter()
            .any(|e| world.get::<RecordBinding>(e).is_some_and(|r| r.uid == uid))
    })
    .await;
    let card = app
        .world_mut()
        .query::<(Entity, &RecordBinding)>()
        .iter(app.world())
        .find(|(e, b)| b.uid == uid && app.world().get::<CanvasItem>(*e).is_some())
        .unwrap()
        .0;
    for _ in 0..4 {
        app.update();
    }
    assert_eq!(app.world().get::<Card>(card).unwrap().column, Some(backlog));
    let earlier = engine
        .act(
            engine::actions::Action::CreateRecordWithTags {
                head: "Earlier task".into(),
                body: String::new(),
                quantity: 0.0,
                tags: vec!["task".into(), "backlog".into()],
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let today = chrono::Utc::now().date_naive();
    for (target, due) in [
        (&uid, (today + chrono::TimeDelta::days(10)).to_string()),
        (&earlier, (today + chrono::TimeDelta::days(1)).to_string()),
    ] {
        engine
            .act(
                engine::actions::Action::SetExtension {
                    target: target.clone(),
                    namespace: "work".into(),
                    fds: json!({"due":due}),
                },
                None,
            )
            .await
            .unwrap();
    }
    until(&mut app, |world| {
        world
            .get::<LayoutBox>(card)
            .is_some_and(|layout| layout.order == 1)
    })
    .await;
    let destination = crate::native::topology::position(app.world(), todo).unwrap();
    app.world_mut()
        .init_resource::<crate::native::topology::input::PointerState>();
    let width = app.world().get::<CanvasItem>(card).unwrap().size.x;
    assert!(app.world().get::<RecordCard>(card).is_some());
    begin_drag(app.world_mut(), card, destination);
    crate::native::topology::set_position(app.world_mut(), card, destination);
    app.update();
    assert_eq!(app.world().get::<CanvasItem>(card).unwrap().size.x, width);
    assert_eq!(app.world().get::<Card>(card).unwrap().column, Some(backlog));
    assert_eq!(
        app.world().get::<RecordProperties>(card).unwrap().0["quantity"],
        "0"
    );
    app.world_mut()
        .resource_mut::<crate::native::topology::input::PointerState>()
        .drag = None;
    until(&mut app, |world| {
        world
            .get::<Card>(card)
            .is_some_and(|c| c.column == Some(todo))
    })
    .await;
    assert!(crate::native::area_mutation::armed(app.world(), todo));
    let properties = &app.world().get::<RecordProperties>(card).unwrap().0;
    assert_eq!(properties["quantity"], "-1");
    assert_eq!(column_index(properties), Some(1));
    assert!(
        !properties["assertions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["predicate"] == "backlog")
    );
    crate::native::layout::detach(app.world_mut(), card);
    crate::native::topology::set_position(app.world_mut(), card, DVec3::new(5000.0, 0.0, 0.0));
    until(&mut app, |world| {
        world
            .get::<RecordProperties>(card)
            .is_some_and(|r| r.0["quantity"] == "0" && column_index(&r.0).is_none())
    })
    .await;
    assert!(app.world().get::<RecordBinding>(card).is_some());
}

#[test]
fn columns_touch_after_resize_and_source_unlock_preserves_its_connection() {
    let (mut app, _, owner) = fixture();
    app.update();
    let board = app.world().get::<Kanban>(owner).unwrap().clone();
    let columns: Vec<_> = board
        .columns
        .iter()
        .map(|c| area(app.world(), owner, &c.area).unwrap())
        .collect();
    let source = area(app.world(), owner, &board.source).unwrap();
    let mut rules = app
        .world()
        .get::<LayoutBox>(columns[1])
        .unwrap()
        .rules
        .clone();
    rules.axes[0].size = 420.0;
    rules.axes[1].size = 900.0;
    crate::native::layout::configure(app.world_mut(), columns[1], rules).unwrap();
    app.update();
    assert_source_covers_columns(app.world(), owner);
    for pair in columns.windows(2) {
        let first = app.world().get::<CanvasItem>(pair[0]).unwrap();
        let second = app.world().get::<CanvasItem>(pair[1]).unwrap();
        let edge = crate::native::topology::position(app.world(), pair[0])
            .unwrap()
            .x
            + f64::from(first.size.x) * 0.5;
        let next = crate::native::topology::position(app.world(), pair[1])
            .unwrap()
            .x
            - f64::from(second.size.x) * 0.5;
        assert!((edge - next).abs() < 1e-8);
        let boundary = DVec2::new(edge, 0.0);
        assert!(
            !app.world()
                .get::<InfluenceArea>(pair[0])
                .unwrap()
                .contains(boundary)
        );
        assert!(
            app.world()
                .get::<InfluenceArea>(pair[1])
                .unwrap()
                .contains(boundary)
        );
    }
    let before = crate::native::topology::position(app.world(), source).unwrap();
    let translation = DVec3::new(30.0, 0.0, 50.0);
    crate::native::topology::groups::transform(
        app.world_mut(),
        owner,
        translation,
        bevy::math::DQuat::IDENTITY,
    );
    app.update();
    assert_eq!(
        crate::native::topology::position(app.world(), source),
        Some(before + translation)
    );
    let source_position = crate::native::topology::position(app.world(), source).unwrap();
    assert_source_covers_columns(app.world(), owner);
    crate::native::canvas_selection::detach(app.world_mut(), source);
    assert!(
        app.world()
            .get::<crate::native::canvas_selection::SandGroup>(columns[0])
            .is_some()
    );
    assert!(
        crate::native::sand_placement::Placement::capture(app.world(), source)
            .group
            .is_none()
    );
    let detached = source_position + DVec3::new(400.0, 0.0, 200.0);
    crate::native::topology::set_position(app.world_mut(), source, detached);
    app.update();
    assert_eq!(
        crate::native::topology::position(app.world(), source),
        Some(detached)
    );
    let config = app
        .world()
        .get::<InfluenceArea>(source)
        .unwrap()
        .protein
        .as_ref()
        .unwrap();
    assert_eq!(
        config.spawn_targets,
        board
            .columns
            .iter()
            .map(|c| c.area.clone())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        config.placement,
        crate::native::protein_area::SpawnPlacement::MatchingAreas
    );
    let original_root = app.world().get::<ChildOf>(owner).unwrap().parent();
    let saved_board = snapshot(app.world_mut(), original_root).pop().unwrap();
    let saved_areas: Vec<_> = board
        .ids()
        .map(|id| {
            let entity = area(app.world(), owner, id).unwrap();
            (
                app.world().get::<InfluenceArea>(entity).unwrap().clone(),
                crate::native::sand_placement::Placement::capture(app.world(), entity),
            )
        })
        .collect();
    let restored_root = app
        .world_mut()
        .spawn((
            crate::native::container::BoxRoot,
            crate::native::workspace::Workspaces::default(),
        ))
        .id();
    let mut restored_source = None;
    for (saved, placement) in saved_areas {
        let is_source = saved.id == board.source;
        let entity =
            crate::native::area::spawn_area(app.world_mut(), restored_root, 1, saved).unwrap();
        placement.restore(app.world_mut(), entity);
        if is_source {
            restored_source = Some(entity);
        }
    }
    saved_board.restore(app.world_mut(), restored_root);
    app.update();
    let restored_source = restored_source.unwrap();
    assert_eq!(
        crate::native::topology::position(app.world(), restored_source),
        Some(detached)
    );
    assert!(
        app.world()
            .get::<crate::native::canvas_selection::SandGroup>(restored_source)
            .is_none()
    );
}

fn assert_source_covers_columns(world: &World, owner: Entity) {
    let board = world.get::<Kanban>(owner).unwrap();
    let source = area(world, owner, &board.source).unwrap();
    let shield = world.get::<InfluenceArea>(source).unwrap();
    let placement = crate::native::topology::spatial(world, source);
    for column in &board.columns {
        let column = area(world, owner, &column.area).unwrap();
        let item = world.get::<CanvasItem>(column).unwrap();
        let column_placement = crate::native::topology::spatial(world, column);
        assert!(world.get::<ZIndex>(source).unwrap().0 < world.get::<ZIndex>(column).unwrap().0);
        for x in [-0.5, 0.5] {
            for z in [-0.5, 0.5] {
                let corner = column_placement.position(item.position)
                    + column_placement.rotation()
                        * DVec3::new(f64::from(item.size.x) * x, 0.0, f64::from(item.size.y) * z);
                assert!(crate::native::topology::influence::contains(
                    shield, placement, corner
                ));
            }
        }
    }
}

#[test]
fn containment_keeps_lone_records_outside_until_immunity_is_removed() {
    for spatial in [false, true] {
        let (mut app, root, owner) = fixture();
        app.update();
        if spatial {
            app.world_mut()
                .init_resource::<crate::native::topology::physics::Runtime>();
        }
        let board = app.world().get::<Kanban>(owner).unwrap().clone();
        let source = area(app.world(), owner, &board.source).unwrap();
        let source_item = app.world().get::<CanvasItem>(source).unwrap();
        let outside =
            source_item.position + DVec2::new(f64::from(source_item.size.x) * 0.5 + 500.0, 0.0);
        let first = area(app.world(), owner, &board.columns[0].area).unwrap();
        let first_item = app.world().get::<CanvasItem>(first).unwrap();
        let inside = first_item.position + DVec2::new(f64::from(first_item.size.x) * 0.25, 0.0);
        for (index, column) in board.columns.iter().enumerate() {
            let column = area(app.world(), owner, &column.area).unwrap();
            app.world_mut().entity_mut(column).insert(
                crate::native::protein_area::filter::Matches {
                    source: crate::native::protein_area::Source::Local,
                    uids: if index == 0 {
                        ["r_lone".into()].into()
                    } else {
                        Default::default()
                    },
                    current: true,
                },
            );
        }
        let sand = app
            .world_mut()
            .spawn((
                CanvasItem {
                    position: outside,
                    size: Vec2::splat(40.0),
                },
                RecordProperties(json!({"uid":"r_lone", "quantity":"0"})),
                ChildOf(root),
                WorkspaceMember(1),
            ))
            .id();
        for (point, attracted) in [(outside, false), (inside, true), (outside, false)] {
            crate::native::topology::set_position(
                app.world_mut(),
                sand,
                DVec3::new(point.x, 0.0, point.y),
            );
            crate::native::area_effects::update(app.world_mut());
            assert_eq!(
                app.world()
                    .get::<crate::native::area::AreaForces>(sand)
                    .unwrap()
                    .total()
                    .length()
                    > 0.0,
                attracted
            );
        }
        app.world_mut()
            .get_mut::<InfluenceArea>(source)
            .unwrap()
            .immunity = crate::native::area_effects::Immunity::None;
        maintain(app.world_mut(), owner, &board);
        crate::native::area_effects::update(app.world_mut());
        assert!(
            app.world()
                .get::<crate::native::area::AreaForces>(sand)
                .unwrap()
                .total()
                .x
                < 0.0
        );
        assert_eq!(
            app.world().get::<InfluenceArea>(source).unwrap().immunity,
            crate::native::area_effects::Immunity::None
        );
        let saved =
            serde_json::to_string(app.world().get::<InfluenceArea>(source).unwrap()).unwrap();
        assert_eq!(
            serde_json::from_str::<InfluenceArea>(&saved)
                .unwrap()
                .immunity,
            crate::native::area_effects::Immunity::None
        );
    }
}

#[test]
fn source_fits_columns_and_headers_without_padding() {
    let (mut app, _, owner) = fixture();
    app.update();
    let world = app.world();
    let board = world.get::<Kanban>(owner).unwrap();
    let source = area(world, owner, &board.source).unwrap();
    let source_item = world.get::<CanvasItem>(source).unwrap();
    let first = area(world, owner, &board.columns[0].area).unwrap();
    let first_item = world.get::<CanvasItem>(first).unwrap();
    assert_eq!(
        source_item.size,
        Vec2::new(COLUMNS.len() as f32 * 680.0, 720.0)
    );
    assert_eq!(
        source_item.position.y - f64::from(source_item.size.y) * 0.5,
        first_item.position.y - f64::from(first_item.size.y) * 0.5 - 80.0
    );
    assert_eq!(
        source_item.position.y + f64::from(source_item.size.y) * 0.5,
        first_item.position.y + f64::from(first_item.size.y) * 0.5
    );
}

#[test]
fn source_covers_columns_after_group_rotation() {
    let (mut app, _, owner) = fixture();
    app.update();
    crate::native::topology::groups::transform(
        app.world_mut(),
        owner,
        DVec3::new(80.0, 120.0, 45.0),
        bevy::math::DQuat::from_rotation_x(0.4) * bevy::math::DQuat::from_rotation_y(0.7),
    );
    app.update();
    assert_source_covers_columns(app.world(), owner);
}

#[test]
fn room_kanban_group_keeps_column_layout_after_room_rotation() {
    use crate::native::{
        canvas_host::Identity,
        topology::{
            self,
            rooms::{self, Member, Room},
        },
    };
    use nucleus::canvas::{
        Pose,
        room::{self as model, compose, relative},
    };
    fn same_group_pose(world: &World, entity: Entity, owner: Entity) {
        let actual = world
            .get::<crate::native::topology::groups::GroupPose>(entity)
            .unwrap();
        let expected = world
            .get::<crate::native::topology::groups::GroupPose>(owner)
            .unwrap();
        assert!(
            (DVec3::from_array(actual.position) - DVec3::from_array(expected.position)).length()
                < 1e-6
        );
        assert!(
            bevy::math::DQuat::from_array(actual.rotation)
                .dot(bevy::math::DQuat::from_array(expected.rotation))
                .abs()
                > 1.0 - 1e-10
        );
    }
    let (mut app, root, owner) = fixture();
    app.update();
    let world = app.world_mut();
    let room_id = nucleus::new_uid("placement");
    let room = world
        .spawn((
            CanvasItem {
                position: DVec2::ZERO,
                size: Vec2::splat(2000.0),
            },
            Room(default()),
            Identity(room_id.clone()),
            ChildOf(root),
            WorkspaceMember(1),
        ))
        .id();
    let members: Vec<_> = topology::groups::members(world, owner)
        .into_iter()
        .filter(|entity| *entity == owner || world.get::<Part>(*entity).is_none())
        .collect();
    assert_eq!(members.len(), 9);
    let headers: Vec<_> = world
        .query::<(Entity, &Part)>()
        .iter(world)
        .filter(|(entity, part)| *entity != owner && part.owner == owner)
        .map(|(entity, _)| {
            (
                entity,
                Pose {
                    position: topology::position(world, entity).unwrap().to_array(),
                    rotation: topology::spatial(world, entity).rotation,
                },
            )
        })
        .collect();
    assert_eq!(headers.len(), 6);
    let original: Vec<_> = members
        .iter()
        .map(|entity| Pose {
            position: topology::position(world, *entity).unwrap().to_array(),
            rotation: topology::spatial(world, *entity).rotation,
        })
        .collect();
    for (entity, local) in members.iter().zip(&original) {
        world.entity_mut(*entity).insert(Member(model::Member {
            room: room_id.clone(),
            parent: None,
            local: local.clone(),
            role: model::Role::Content,
        }));
    }
    rooms::synchronize(world);
    let moved = Pose {
        position: [1e9, 150.0, -1e9],
        rotation: bevy::math::DQuat::from_euler(EulerRot::XYZ, 0.3, 0.7, 0.1).to_array(),
    };
    topology::set_position(world, room, DVec3::from_array(moved.position));
    world.entity_mut(room).insert(topology::Spatial {
        rotation: moved.rotation,
        elevation: moved.position[1],
        ..default()
    });
    rooms::synchronize(world);
    for (entity, local) in &headers {
        let expected = compose(&moved, local);
        assert!(
            (topology::position(world, *entity).unwrap() - DVec3::from_array(expected.position))
                .length()
                < 1e-6
        );
        same_group_pose(world, *entity, owner);
    }
    for (entity, local) in members.iter().zip(&original) {
        let expected = compose(&moved, local);
        assert!(
            (topology::position(world, *entity).unwrap() - DVec3::from_array(expected.position))
                .length()
                < 1e-6
        );
        same_group_pose(world, *entity, owner);
    }
    app.update();
    rooms::synchronize(app.world_mut());
    assert_source_covers_columns(app.world(), owner);
    for (entity, local) in members.iter().zip(&original) {
        let world = app.world();
        let actual = Pose {
            position: topology::position(world, *entity).unwrap().to_array(),
            rotation: topology::spatial(world, *entity).rotation,
        };
        let projected = relative(&moved, &actual);
        assert!(
            (DVec3::from_array(projected.position) - DVec3::from_array(local.position)).length()
                < 1e-5
        );
    }
}

#[tokio::test]
async fn quantity_changes_move_cards_from_the_source_and_between_columns() {
    quantity_changes_in_board(false, false).await;
}

#[tokio::test]
async fn room_quantity_changes_keep_real_record_cards_in_moving_wall_boards() {
    quantity_changes_in_board(true, false).await;
}

#[tokio::test]
async fn room_wall_mount_keeps_backend_task_changes_in_columns_while_the_room_and_anchor_move() {
    quantity_changes_in_board(true, true).await;
}

async fn quantity_changes_in_board(in_room: bool, mounted: bool) {
    let (mut app, root, owner) = fixture();
    if mounted {
        app.add_plugins(crate::native::edit_mode::EditModePlugin)
            .init_resource::<Assets<Image>>();
    }
    let engine = std::sync::Arc::new(engine::Engine::open_memory().await.unwrap());
    let board = app.world().get::<Kanban>(owner).unwrap().clone();
    let columns: Vec<_> = board
        .columns
        .iter()
        .map(|c| area(app.world(), owner, &c.area).unwrap())
        .collect();
    app.insert_resource(crate::native::app::CellHandle(cell::CellRuntime {
        commands: Default::default(),
        engine: engine.clone(),
        store: engine.store.clone(),
        lanes: std::sync::Arc::new(cell::LaneHub::new()),
        wire: Default::default(),
        fiote: None,
        speech: None,
        information: None,
    }))
    .insert_resource(crate::native::wake::WakeSignal::new(|| {}))
    .add_plugins((
        crate::native::cell_bridge::CellBridgePlugin,
        crate::native::physics::WorkspacePhysicsPlugin,
    ))
    .init_resource::<crate::native::topology::physics::Runtime>();
    crate::native::workspace_config::set_physics(app.world_mut(), root, 1, true);
    app.finish();
    until(&mut app, |world| {
        world
            .get::<View>(owner)
            .is_some_and(|view| view.status == "Ready")
    })
    .await;
    let mut wall_anchor = None;
    let room = if in_room {
        use crate::native::{
            canvas_host::Identity,
            topology::{
                self,
                rooms::{self, Member, Room},
            },
        };
        use nucleus::canvas::{Pose, room as model};
        let world = app.world_mut();
        let uid = nucleus::new_uid("placement");
        let room = if mounted {
            world
                .get_mut::<crate::native::edit_mode::EditMode>(root)
                .unwrap()
                .enabled = true;
            let entity = crate::native::sand_store::spawn_sand(
                world,
                root,
                1,
                crate::native::sand_store::SandKind::Square,
                "Room",
                DVec2::ZERO,
            );
            world.get_mut::<CanvasItem>(entity).unwrap().size = Vec2::splat(4000.0);
            world
                .entity_mut(entity)
                .insert((Room(default()), Identity(uid.clone())));
            entity
        } else {
            world
                .spawn((
                    CanvasItem {
                        position: DVec2::ZERO,
                        size: Vec2::splat(4000.0),
                    },
                    Room(default()),
                    Identity(uid.clone()),
                    ChildOf(root),
                    WorkspaceMember(1),
                ))
                .id()
        };
        let members: Vec<_> = topology::groups::members(world, owner)
            .into_iter()
            .filter(|entity| *entity == owner || world.get::<Part>(*entity).is_none())
            .collect();
        if mounted {
            let wall = rooms::create_proxy(world, room, model::Role::Wall).unwrap();
            rooms::mount(world, owner, wall).unwrap();
            wall_anchor = Some(wall);
        } else {
            for entity in members {
                let local = Pose {
                    position: topology::position(world, entity).unwrap().to_array(),
                    rotation: topology::spatial(world, entity).rotation,
                };
                world.entity_mut(entity).insert(Member(model::Member {
                    room: uid.clone(),
                    parent: None,
                    local,
                    role: model::Role::Content,
                }));
            }
        }
        rooms::synchronize(world);
        topology::set_position(world, room, DVec3::new(1000.0, 250.0, 500.0));
        let mut spatial = topology::spatial(world, room);
        spatial.rotation = bevy::math::DQuat::from_euler(EulerRot::XYZ, 0.2, 0.4, 0.1).to_array();
        world.entity_mut(room).insert(spatial);
        rooms::synchronize(world);
        app.add_systems(
            Update,
            rooms::synchronize.after(crate::native::physics::SimulateWorkspaces),
        );
        Some(room)
    } else {
        None
    };
    let uid = engine
        .act(
            engine::actions::Action::CreateRecordWithTags {
                head: "Moving task".into(),
                body: String::new(),
                quantity: 99.0,
                tags: vec!["task".into()],
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let source = area(app.world(), owner, &board.source).unwrap();
    until(&mut app, |world| {
        world.get::<Children>(root).unwrap().iter().any(|entity| {
            world
                .get::<RecordBinding>(entity)
                .is_some_and(|binding| binding.area == source && binding.uid == uid)
                && world
                    .get::<Card>(entity)
                    .is_some_and(|card| card.column.is_none())
        })
    })
    .await;
    let card = app
        .world_mut()
        .query::<(Entity, &RecordBinding)>()
        .iter(app.world())
        .find(|(entity, binding)| binding.uid == uid && app.world().get::<Card>(*entity).is_some())
        .unwrap()
        .0;
    for mode in 0..3 {
        if let Some(room) = room {
            crate::native::topology::set_position(
                app.world_mut(),
                room,
                DVec3::new(
                    1000.0 + mode as f64 * 200.0,
                    250.0 + mode as f64 * 50.0,
                    500.0,
                ),
            );
            crate::native::topology::rooms::synchronize(app.world_mut());
        }
        if mode == 1 {
            if let Some(wall) = wall_anchor {
                let world = app.world_mut();
                let mut anchor = world
                    .get_mut::<crate::native::topology::rooms::Member>(wall)
                    .unwrap();
                anchor.0.local.position[0] += 120.0;
                anchor.0.local.rotation = (bevy::math::DQuat::from_rotation_y(0.15)
                    * bevy::math::DQuat::from_array(anchor.0.local.rotation))
                .to_array();
                crate::native::topology::rooms::synchronize(world);
            }
            for (index, column) in columns.iter().enumerate() {
                let mut area = app.world_mut().get_mut::<InfluenceArea>(*column).unwrap();
                area.filter.as_mut().unwrap().draft.query["where"] =
                    json!([{"quantity_eq": COLUMNS[index].2.to_string()}]);
            }
        } else if mode == 2 {
            for (index, column) in columns.iter().enumerate() {
                let mut area = app.world_mut().get_mut::<InfluenceArea>(*column).unwrap();
                area.filter = None;
                area.rules = vec![crate::native::area::PropertyRule {
                    property: crate::native::area::Property::Quantity,
                    value: COLUMNS[index].2.to_string(),
                }];
            }
        }
        for index in [2, 1, 2, 1] {
            engine
                .act(
                    engine::actions::Action::SetQuantity {
                        target: uid.clone(),
                        value: f64::from(COLUMNS[index].2),
                    },
                    None,
                )
                .await
                .unwrap();
            until(&mut app, |world| {
                world
                    .get::<Card>(card)
                    .is_some_and(|card| card.column == Some(columns[index]))
                    && world
                        .get::<LayoutRuntime>(card)
                        .is_some_and(|layout| layout.parent == Some(columns[index]))
                    && world
                        .get::<RecordProperties>(card)
                        .is_some_and(|record| record.0["quantity"] == COLUMNS[index].2.to_string())
            })
            .await;
            assert_eq!(
                app.world().get::<RecordProperties>(card).unwrap().0["quantity"],
                COLUMNS[index].2.to_string()
            );
            assert_eq!(app.world().get::<RecordBinding>(card).unwrap().uid, uid);
            if mounted {
                let card_rotation = crate::native::topology::spatial(app.world(), card).rotation();
                let column_rotation =
                    crate::native::topology::spatial(app.world(), columns[index]).rotation();
                assert!(card_rotation.dot(column_rotation).abs() > 1.0 - 1e-8);
            }
        }
    }
}

#[test]
fn restoring_a_preset_removes_its_status_requirement_and_keeps_custom_filters() {
    let (mut app, root, owner) = fixture();
    let board = app.world().get::<Kanban>(owner).unwrap().clone();
    let first = area(app.world(), owner, &board.columns[0].area).unwrap();
    let custom = area(app.world(), owner, &board.columns[1].area).unwrap();
    app.world_mut()
        .get_mut::<InfluenceArea>(first)
        .unwrap()
        .filter
        .as_mut()
        .unwrap()
        .draft
        .query["where"][0]["all"]
        .as_array_mut()
        .unwrap()
        .push(json!({"concept_in":"backlog"}));
    let conditions = json!([{"concept_in":"special"}, {"quantity_eq":"-1"}]);
    app.world_mut()
        .get_mut::<InfluenceArea>(custom)
        .unwrap()
        .filter
        .as_mut()
        .unwrap()
        .draft
        .query["where"] = conditions.clone();
    app.world_mut().despawn(owner);
    restore(app.world_mut(), root, 1, DVec2::ZERO, board);
    assert_eq!(
        app.world()
            .get::<InfluenceArea>(first)
            .unwrap()
            .filter
            .as_ref()
            .unwrap()
            .draft
            .query["where"][0]["all"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert_eq!(
        app.world()
            .get::<InfluenceArea>(custom)
            .unwrap()
            .filter
            .as_ref()
            .unwrap()
            .draft
            .query["where"],
        conditions
    );
}
