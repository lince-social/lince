use super::*;
use nucleus::canvas::{self as api, Component as ContentKind};

fn setup() -> (App, Entity, Entity, Entity, Entity) {
    let (mut app, root, owner) = fixture();
    app.add_plugins(crate::native::edit_mode::EditModePlugin)
        .init_resource::<Assets<Image>>();
    app.update();
    let world = app.world_mut();
    world
        .get_mut::<crate::native::edit_mode::EditMode>(root)
        .unwrap()
        .enabled = true;
    let room = crate::native::sand_store::spawn_sand(
        world,
        root,
        1,
        crate::native::sand_store::SandKind::Square,
        "Room",
        DVec2::ZERO,
    );
    world.get_mut::<CanvasItem>(room).unwrap().size = Vec2::new(4000.0, 3000.0);
    world.entity_mut(room).insert((
        rooms::Room(default()),
        Identity(nucleus::new_uid("placement")),
    ));
    let wall = rooms::create_proxy(world, room, model::Role::Wall).unwrap();
    (app, root, owner, room, wall)
}

fn member(world: &World, room: Entity, anchor: Option<Entity>) -> model::Member {
    model::Member {
        room: world.get::<Identity>(room).unwrap().0.clone(),
        parent: anchor.map(|anchor| world.get::<Identity>(anchor).unwrap().0.clone()),
        local: Pose {
            position: [0.0, 12.0, 0.0],
            ..model::identity_pose()
        },
        role: model::Role::Content,
    }
}

fn mutate(
    world: &mut World,
    root: Entity,
    mutation: api::Mutation,
) -> Result<api::Response, String> {
    let response = crate::native::canvas_host::handle(
        world,
        root,
        &api::Request::Inspect {
            workspace: None,
            selected_only: false,
            offset: 0,
            limit: 1,
        },
    )?;
    let api::Response::Snapshot { snapshot } = response else {
        unreachable!()
    };
    crate::native::canvas_host::handle(
        world,
        root,
        &api::Request::Mutate {
            request_id: nucleus::new_uid("request"),
            expected_revision: snapshot.revision,
            mutation,
        },
    )
}

#[test]
fn room_mount_admission_counts_owned_regions_and_hidden_record_overrides_atomically() {
    let (mut app, root, owner, room, wall) = setup();
    let world = app.world_mut();
    for _ in 0..246 {
        let entity = crate::native::sand_store::spawn_sand(
            world,
            root,
            1,
            crate::native::sand_store::SandKind::Square,
            "Member",
            DVec2::ZERO,
        );
        let membership = member(world, room, None);
        world.entity_mut(entity).insert(rooms::Member(membership));
    }
    let mut source = InfluenceArea::new(
        crate::native::component_push::rectangle(),
        DVec2::ZERO,
        DVec2::splat(100.0),
    );
    let mut config = Config::records();
    config.record_cards = true;
    config.draft.query["limit"] = json!(1);
    source.protein = Some(config);
    let source = crate::native::area::spawn_area(world, root, 1, source).unwrap();
    let row = world
        .spawn((
            CanvasItem {
                position: DVec2::ZERO,
                size: Vec2::splat(24.0),
            },
            topology::Spatial::default(),
            rooms::Member(member(world, room, None)),
            RecordBinding {
                area: source,
                uid: nucleus::new_uid("r"),
                source: crate::native::protein_area::Source::Local,
            },
            ChildOf(root),
            WorkspaceMember(1),
        ))
        .id();
    rooms::synchronize(world);
    crate::native::canvas_host::handle(
        world,
        root,
        &api::Request::Inspect {
            workspace: None,
            selected_only: false,
            offset: 0,
            limit: 1,
        },
    )
    .unwrap();
    let before = crate::native::canvas_host::capture(world, root).unwrap();
    let old_pose = pose(world, owner);
    let error = rooms::mount(world, owner, wall).unwrap_err();
    assert!(error.contains("owned Kanban regions"), "{error}");
    assert_eq!(
        crate::native::canvas_host::capture(world, root).unwrap(),
        before
    );
    close(&pose(world, owner), &old_pose);
    let mutation = api::Mutation::SetRoomMembers {
        members: std::collections::BTreeMap::from([(
            world.get::<Identity>(owner).unwrap().0.clone(),
            Some(member(world, room, Some(wall))),
        )]),
    };
    let error = mutate(world, root, mutation).unwrap_err();
    assert!(error.contains("Record overrides"), "{error}");
    assert_eq!(
        crate::native::canvas_host::capture(world, root).unwrap(),
        before
    );
    world.despawn(row);
    rooms::mount(world, owner, wall).unwrap();
    rooms::synchronize(world);
    assert_eq!(
        world.query::<&rooms::Member>().iter(world).count(),
        model::MAX_MEMBERS
    );
    assert_eq!(
        topology::groups::members(world, owner)
            .iter()
            .filter(|entity| world.get::<rooms::Member>(**entity).is_some())
            .count(),
        9
    );
}

#[test]
fn room_canvas_creation_and_reuse_place_complete_kanbans_into_rotated_anchors() {
    let (mut app, root, owner, room, wall) = setup();
    let world = app.world_mut();
    topology::set_position(world, room, DVec3::new(1e9, 200.0, -1e9));
    let mut spatial = topology::spatial(world, room);
    spatial.rotation = bevy::math::DQuat::from_rotation_y(0.65).to_array();
    world.entity_mut(room).insert(spatial);
    rooms::synchronize(world);
    let mut component = ContentKind::Native {
        kind: "kanban".into(),
        settings: default(),
        bindings: default(),
    };
    for offset in [0.0, 700.0] {
        let id = nucleus::new_uid("placement");
        let mut membership = member(world, room, Some(wall));
        membership.local.position[0] = offset;
        let expected = model::compose(&pose(world, wall), &membership.local);
        let mut metadata = api::Metadata::default();
        metadata.room_member = Some(membership);
        mutate(
            world,
            root,
            api::Mutation::AddWithMetadata {
                placement: id.clone(),
                workspace: 1,
                component,
                geometry: api::Geometry {
                    position: [0.0, 0.0],
                    size: [340.0, 80.0],
                },
                metadata,
            },
        )
        .unwrap();
        let created = world
            .query::<(Entity, &Identity)>()
            .iter(world)
            .find(|(_, identity)| identity.0 == id)
            .unwrap()
            .0;
        assert_ne!(created, owner);
        close(&pose(world, created), &expected);
        let board = world.get::<Kanban>(created).unwrap().clone();
        for id in board.ids() {
            let region = area(world, created, id).unwrap();
            assert!(world.get::<rooms::Member>(region).is_some());
            assert!(
                bevy::math::DQuat::from_array(topology::spatial(world, region).rotation)
                    .dot(bevy::math::DQuat::from_array(expected.rotation))
                    .abs()
                    > 1.0 - 1e-9
            );
        }
        component = crate::native::canvas_host::capture(world, root)
            .unwrap()
            .placements
            .into_iter()
            .find(|item| item.id == id)
            .unwrap()
            .component;
    }
    app.update();
    let world = app.world_mut();
    rooms::synchronize(world);
    let boards: Vec<_> = world
        .query::<(Entity, &Kanban)>()
        .iter(world)
        .map(|(entity, _)| entity)
        .collect();
    assert_eq!(boards.len(), 3);
    for board in &boards {
        assert_source_covers_columns(world, *board);
    }
    let board = *boards.iter().find(|entity| **entity != owner).unwrap();
    let computer = rooms::create_proxy(world, room, model::Role::Computer).unwrap();
    let previous: Vec<_> = topology::groups::members(world, board)
        .into_iter()
        .map(|entity| (entity, pose(world, entity)))
        .collect();
    let panel = world.spawn((Node::default(), ChildOf(root))).id();
    rooms::controls(world, panel, board);
    let attach = world
        .query::<(&ActionButton, &crate::native::icons::Tooltip)>()
        .iter(world)
        .find(|(action, tooltip)| {
            action.target == board
                && tooltip.0.starts_with("Parent to ")
                && tooltip.0.contains("Computer")
        })
        .unwrap()
        .0
        .clone();
    attach.actions.run(world, board);
    for (entity, old) in &previous {
        close(&pose(world, *entity), old);
        if world.get::<InfluenceArea>(*entity).is_some() || *entity == board {
            assert_eq!(
                world.get::<rooms::Member>(*entity).unwrap().0.parent,
                Some(world.get::<Identity>(computer).unwrap().0.clone())
            );
        }
    }
    let old_parent = pose(world, computer);
    let mut anchor = world.get_mut::<rooms::Member>(computer).unwrap();
    anchor.0.local.position[0] += 120.0;
    anchor.0.local.rotation = bevy::math::DQuat::from_rotation_y(0.15).to_array();
    rooms::synchronize(world);
    let new_parent = pose(world, computer);
    for (entity, old) in &previous {
        close(
            &pose(world, *entity),
            &model::compose(&new_parent, &model::relative(&old_parent, old)),
        );
    }
    crate::native::canvas_host::capture(world, root)
        .unwrap()
        .validate()
        .unwrap();
}
