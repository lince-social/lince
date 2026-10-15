use super::*;
use crate::native::{
    canvas_host::Identity,
    topology::{self, rooms},
};
use nucleus::canvas::{Pose, room as model};

mod admission;

fn pose(world: &World, entity: Entity) -> Pose {
    Pose {
        position: topology::position(world, entity).unwrap().to_array(),
        rotation: topology::spatial(world, entity).rotation,
    }
}

#[track_caller]
fn close(actual: &Pose, expected: &Pose) {
    assert!(
        (DVec3::from_array(actual.position) - DVec3::from_array(expected.position)).length() < 1e-5,
        "{actual:?} != {expected:?}"
    );
    assert!(
        bevy::math::DQuat::from_array(actual.rotation)
            .dot(bevy::math::DQuat::from_array(expected.rotation))
            .abs()
            > 1.0 - 1e-9
    );
}

#[test]
fn room_wall_mount_preserves_all_kanban_parts_headers_undo_and_serialized_placement() {
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
    let members: Vec<_> = topology::groups::members(world, owner)
        .into_iter()
        .filter(|entity| *entity == owner || world.get::<Part>(*entity).is_none())
        .collect();
    assert_eq!(members.len(), 9);
    let original: Vec<_> = members
        .iter()
        .map(|entity| (*entity, pose(world, *entity)))
        .collect();
    let original_owner = pose(world, owner);
    let headers: Vec<_> = world
        .query::<(Entity, &Part)>()
        .iter(world)
        .filter(|(entity, part)| *entity != owner && part.owner == owner)
        .map(|(entity, _)| {
            (
                entity,
                model::relative(&original_owner, &pose(world, entity)),
            )
        })
        .collect();
    assert_eq!(headers.len(), 6);
    let component = crate::native::canvas_host::capture(world, root)
        .unwrap()
        .placements
        .into_iter()
        .find(|item| item.id == world.get::<Identity>(owner).unwrap().0)
        .unwrap()
        .component;
    rooms::mount(world, owner, wall).unwrap();
    assert_eq!(
        crate::native::canvas_host::capture(world, root)
            .unwrap()
            .placements
            .into_iter()
            .find(|item| item.id == world.get::<Identity>(owner).unwrap().0)
            .unwrap()
            .component,
        component
    );
    let mounted = pose(world, owner);
    for (entity, old) in &original {
        close(
            &pose(world, *entity),
            &model::compose(&mounted, &model::relative(&original_owner, old)),
        );
        assert_eq!(
            world
                .get::<rooms::Member>(*entity)
                .unwrap()
                .0
                .parent
                .as_deref(),
            Some(world.get::<Identity>(wall).unwrap().0.as_str())
        );
    }
    for (entity, local) in &headers {
        close(&pose(world, *entity), &model::compose(&mounted, local));
    }
    topology::set_position(world, room, DVec3::new(1e9, 200.0, -1e9));
    let mut spatial = topology::spatial(world, room);
    spatial.rotation = bevy::math::DQuat::from_rotation_y(0.7).to_array();
    world.entity_mut(room).insert(spatial);
    rooms::synchronize(world);
    let panel = world.spawn((Node::default(), ChildOf(root))).id();
    rooms::controls(world, panel, owner);
    let undo = world
        .query::<(&ActionButton, &crate::native::icons::Tooltip)>()
        .iter(world)
        .find(|(action, tooltip)| action.target == owner && tooltip.0 == "Undo room edit")
        .unwrap()
        .0
        .clone();
    undo.actions.run(world, owner);
    for (entity, old) in &original {
        assert!(world.get::<rooms::Member>(*entity).is_none());
        close(&pose(world, *entity), old);
    }
    for (entity, local) in &headers {
        close(
            &pose(world, *entity),
            &model::compose(&original_owner, local),
        );
    }
    rooms::mount(world, owner, wall).unwrap();
    let expected = pose(world, owner);
    let board = persistence::snapshot(world, root)
        .into_iter()
        .next()
        .unwrap();
    assert!(board.valid());
    let board: persistence::SavedKanban =
        serde_json::from_slice(&serde_json::to_vec(&board).unwrap()).unwrap();
    let areas: Vec<_> = members
        .iter()
        .filter_map(|entity| {
            Some(crate::native::area::SavedArea {
                workspace: 1,
                area: crate::native::record_presentation::capture(
                    world,
                    *entity,
                    world.get::<InfluenceArea>(*entity)?.clone(),
                ),
                placement: crate::native::sand_placement::Placement::capture(world, *entity),
            })
        })
        .collect();
    let areas: Vec<crate::native::area::SavedArea> =
        serde_json::from_slice(&serde_json::to_vec(&areas).unwrap()).unwrap();
    for entity in members {
        world.despawn(entity);
    }
    app.update();
    let world = app.world_mut();
    for saved in areas {
        let entity =
            crate::native::area::spawn_area(world, root, saved.workspace, saved.area).unwrap();
        saved.placement.restore(world, entity);
    }
    board.restore(world, root);
    rooms::synchronize(world);
    app.update();
    let world = app.world_mut();
    rooms::synchronize(world);
    let restored = world
        .query_filtered::<Entity, With<Kanban>>()
        .single(world)
        .unwrap();
    close(&pose(world, restored), &expected);
    assert_eq!(topology::groups::members(world, restored).len(), 15);
    assert_source_covers_columns(world, restored);
    let snapshot = crate::native::canvas_host::capture(world, root).unwrap();
    snapshot.validate().unwrap();
    assert_eq!(
        snapshot
            .placements
            .iter()
            .filter(|item| item
                .metadata
                .room_member
                .as_ref()
                .is_some_and(|member| member.parent.as_deref()
                    == Some(world.get::<Identity>(wall).unwrap().0.as_str())))
            .count(),
        1
    );
    let placed: Vec<_> = topology::groups::members(world, restored)
        .into_iter()
        .map(|entity| (entity, pose(world, entity)))
        .collect();
    rooms::controls(world, panel, restored);
    let detach = world
        .query::<(&ActionButton, &crate::native::icons::Tooltip)>()
        .iter(world)
        .find(|(action, tooltip)| action.target == restored && tooltip.0 == "Detach from room")
        .unwrap()
        .0
        .clone();
    detach.actions.run(world, restored);
    for (entity, previous) in &placed {
        assert!(world.get::<rooms::Member>(*entity).is_none());
        close(&pose(world, *entity), previous);
    }
    rooms::controls(world, panel, restored);
    let attach = world
        .query::<(&ActionButton, &crate::native::icons::Tooltip)>()
        .iter(world)
        .find(|(action, tooltip)| {
            action.target == restored
                && tooltip.0.starts_with("Parent to ")
                && tooltip.0.contains("Wall")
        })
        .unwrap()
        .0
        .clone();
    attach.actions.run(world, restored);
    for (entity, previous) in &placed {
        close(&pose(world, *entity), previous);
        if *entity == restored || world.get::<Part>(*entity).is_none() {
            assert_eq!(
                world.get::<rooms::Member>(*entity).unwrap().0.parent,
                Some(world.get::<Identity>(wall).unwrap().0.clone())
            );
        }
    }
}
