use super::*;
use crate::{
    host::Manipulation,
    native::{layout::desktop::manipulation, topology},
};

fn fixture(spatial: bool, scaled: bool) -> (App, Entity, Entity) {
    let (mut app, root, area, target) = tests::desktop_fixture(DVec2::ZERO, true);
    if scaled {
        let mut influence = app.world_mut().get_mut::<InfluenceArea>(area).unwrap();
        influence.rules.clear();
        influence.strength = 0.0;
        influence.scale = 1.5;
        app.world_mut().entity_mut(area).insert(topology::Spatial {
            elevation: 1000.0,
            depth: Some(2000.0),
            ..default()
        });
    } else {
        app.world_mut().despawn(area);
    }
    if spatial {
        app.init_resource::<topology::physics::Runtime>();
        app.world_mut()
            .entity_mut(target)
            .insert(topology::Spatial {
                elevation: 100.0,
                depth: Some(20.0),
                rotation: if scaled {
                    (bevy::math::DQuat::from_rotation_y(0.4)
                        * bevy::math::DQuat::from_rotation_x(0.25))
                    .to_array()
                } else {
                    bevy::math::DQuat::IDENTITY.to_array()
                },
                ..default()
            });
    }
    crate::native::workspace_config::set_physics(app.world_mut(), root, 1, true);
    tests::advance(&mut app, 2);
    if scaled {
        assert_eq!(
            app.world()
                .get::<crate::native::area_effects::AreaScale>(target)
                .unwrap()
                .0,
            1.5
        );
    }
    (app, root, target)
}

fn body(world: &mut World, target: Entity, spatial: bool) -> Entity {
    if spatial {
        world
            .query::<(Entity, &topology::physics::Body)>()
            .iter(world)
            .find(|(_, body)| body.members.contains(&target))
            .unwrap()
            .0
    } else {
        world
            .query::<(Entity, &BodyLink)>()
            .iter(world)
            .find(|(_, body)| body.sand == target)
            .unwrap()
            .0
    }
}

fn shape_entities(world: &mut World, body: Entity) -> HashSet<Entity> {
    world
        .query::<(Entity, &ColliderOf)>()
        .iter(world)
        .filter(|(_, owner)| owner.body == body)
        .map(|(entity, _)| entity)
        .collect()
}

fn matches_visible_shape(world: &mut World, target: Entity, spatial: bool) {
    let item = crate::native::canvas::presented(world, target).unwrap();
    let scale = f64::from(
        world
            .get::<crate::native::area_effects::AreaScale>(target)
            .map_or(1.0, |scale| scale.0),
    );
    let entity = body(world, target, spatial);
    if spatial {
        let pose = crate::native::canvas::presented_spatial(world, target);
        let size = DVec3::new(
            f64::from(item.size.x),
            pose.depth(item.size),
            f64::from(item.size.y),
        ) * scale;
        let center =
            pose.position(item.position) + pose.rotation() * DVec3::new(0.0, -size.y * 0.5, 0.0);
        let found = shape_entities(world, entity).iter().any(|part| {
            let position = world.get::<Position>(*part).unwrap().0;
            let rotation = world.get::<Rotation>(*part).unwrap().0;
            let shape = world
                .get::<Collider>(*part)
                .unwrap()
                .shape_scaled()
                .as_cuboid()
                .unwrap();
            position.distance(center) < 0.00001
                && rotation.dot(pose.rotation()).abs() > 1.0 - 1e-9
                && shape.half_extents.distance(size * 0.5) < 0.00001
        });
        assert!(
            found,
            "No collider matches the visible pose of {target:?} at {center:?}"
        );
    } else {
        let link = world.get::<BodyLink>(entity).unwrap();
        assert!(
            (world.get::<Position>(entity).unwrap().0.truncate() + link.origin - item.position)
                .length()
                < 0.00001
        );
        let half = world
            .get::<Collider>(entity)
            .unwrap()
            .shape_scaled()
            .as_cuboid()
            .unwrap()
            .half_extents;
        assert!((half.x - f64::from(item.size.x) * scale * 0.5).abs() < 0.00001);
        assert!((half.y - f64::from(item.size.y) * scale * 0.5).abs() < 0.00001);
    }
}

#[test]
fn moving_and_resizing_previews_drive_scaled_colliders_without_authoring_until_commit() {
    for spatial in [false, true] {
        for operation in [Manipulation::Move, Manipulation::Resize] {
            for commit in [false, true] {
                let (mut app, root, target) = fixture(spatial, true);
                let original = *app.world().get::<CanvasItem>(target).unwrap();
                let pose = topology::spatial(app.world(), target);
                let rules = app
                    .world()
                    .get::<crate::native::layout::LayoutBox>(target)
                    .unwrap()
                    .rules;
                let model = app.world().get::<Workspaces>(root).unwrap().entries[0]
                    .desktop
                    .clone();
                let entity = body(app.world_mut(), target, spatial);
                app.world_mut().entity_mut(entity).insert((
                    LinearVelocity(DVec3::splat(20.0)),
                    AngularVelocity(DVec3::splat(2.0)),
                ));
                assert!(manipulation::begin(
                    app.world_mut(),
                    root,
                    target,
                    operation,
                    [1.0; 2]
                ));
                assert!(manipulation::propose(app.world_mut(), [50.0, 25.0], None));
                tests::advance(&mut app, 3);
                matches_visible_shape(app.world_mut(), target, spatial);
                let preview = crate::native::canvas::presented(app.world(), target).unwrap();
                let preview_pose = crate::native::canvas::presented_spatial(app.world(), target);
                assert_eq!(
                    app.world().get::<CanvasItem>(target).unwrap().position,
                    original.position
                );
                assert_eq!(
                    app.world().get::<CanvasItem>(target).unwrap().size,
                    original.size
                );
                assert_eq!(topology::spatial(app.world(), target), pose);
                assert_eq!(
                    app.world()
                        .get::<crate::native::layout::LayoutBox>(target)
                        .unwrap()
                        .rules,
                    rules
                );
                assert_eq!(
                    app.world().get::<Workspaces>(root).unwrap().entries[0].desktop,
                    model
                );
                let owned = shape_entities(app.world_mut(), entity);
                tests::advance(&mut app, 8);
                assert_eq!(shape_entities(app.world_mut(), entity), owned);
                assert_eq!(
                    app.world().get::<LinearVelocity>(entity).unwrap().0,
                    DVec3::ZERO
                );
                assert_eq!(
                    app.world().get::<AngularVelocity>(entity).unwrap().0,
                    DVec3::ZERO
                );
                if commit {
                    assert!(manipulation::commit(app.world_mut()));
                } else {
                    manipulation::cancel(app.world_mut());
                }
                tests::advance(&mut app, 3);
                let expected = if commit { preview } else { original };
                assert_eq!(
                    app.world().get::<CanvasItem>(target).unwrap().position,
                    expected.position
                );
                assert_eq!(
                    app.world().get::<CanvasItem>(target).unwrap().size,
                    expected.size
                );
                assert_eq!(
                    topology::spatial(app.world(), target),
                    if commit { preview_pose } else { pose }
                );
                matches_visible_shape(app.world_mut(), target, spatial);
                assert_eq!(
                    app.world().get::<LinearVelocity>(entity).unwrap().0,
                    DVec3::ZERO
                );
                assert_eq!(
                    app.world().get::<AngularVelocity>(entity).unwrap().0,
                    DVec3::ZERO
                );
            }
        }
    }
}

#[test]
fn preview_motion_pushes_sleeping_neighbors_at_the_visible_position_in_both_physics_paths() {
    for spatial in [false, true] {
        let (mut app, root, target) = fixture(spatial, false);
        let original = *app.world().get::<CanvasItem>(target).unwrap();
        let other = app
            .world_mut()
            .spawn((
                CanvasItem {
                    position: original.position + DVec2::new(100.0, 0.0),
                    size: original.size,
                },
                WorkspaceMember(1),
                ChildOf(root),
                topology::Spatial {
                    elevation: if spatial { 100.0 } else { 0.0 },
                    depth: Some(20.0),
                    ..default()
                },
            ))
            .id();
        tests::advance(&mut app, 360);
        let other_body = body(app.world_mut(), other, spatial);
        assert!(app.world().get::<Sleeping>(other_body).is_some());
        assert!(manipulation::begin(
            app.world_mut(),
            root,
            target,
            Manipulation::Move,
            [0.5; 2]
        ));
        for step in 1..=70 {
            assert!(manipulation::propose(
                app.world_mut(),
                [f64::from(step) * 2.0, 0.0],
                None
            ));
            tests::advance(&mut app, 2);
        }
        let pushed = app.world().get::<CanvasItem>(other).unwrap().position;
        assert!(
            pushed.x >= original.position.x + 178.0,
            "{spatial}: peer did not follow the visible collider: {pushed:?}"
        );
        assert_eq!(
            app.world().get::<CanvasItem>(target).unwrap().position,
            original.position
        );
        matches_visible_shape(app.world_mut(), target, spatial);
        manipulation::cancel(app.world_mut());
        tests::advance(&mut app, 2);
        assert_eq!(
            app.world().get::<CanvasItem>(target).unwrap().position,
            original.position
        );
        assert!(app.world().get::<CanvasItem>(other).unwrap().position.x >= pushed.x - 0.01);
        matches_visible_shape(app.world_mut(), target, spatial);
    }
}

#[test]
fn compound_preview_changes_only_its_visible_part_and_committed_offsets_survive_physics() {
    for operation in [Manipulation::Move, Manipulation::Resize] {
        for commit in [false, true] {
            let (mut app, root, target) = fixture(true, true);
            let initial = *app.world().get::<CanvasItem>(target).unwrap();
            let peer = app
                .world_mut()
                .spawn((
                    CanvasItem {
                        position: initial.position + DVec2::new(160.0, 0.0),
                        size: initial.size,
                    },
                    WorkspaceMember(1),
                    ChildOf(root),
                    topology::Spatial {
                        elevation: 100.0,
                        depth: Some(20.0),
                        ..default()
                    },
                ))
                .id();
            for entity in [target, peer] {
                app.world_mut()
                    .entity_mut(entity)
                    .insert(crate::native::canvas_selection::SandGroup([11; 16]));
            }
            topology::groups::attach(app.world_mut(), &[target, peer]);
            topology::groups::transform(
                app.world_mut(),
                target,
                DVec3::ZERO,
                bevy::math::DQuat::from_rotation_y(0.3),
            );
            tests::advance(&mut app, 3);
            let original = *app.world().get::<CanvasItem>(target).unwrap();
            let original_peer = *app.world().get::<CanvasItem>(peer).unwrap();
            let pose = *app
                .world()
                .get::<topology::groups::GroupPose>(target)
                .unwrap();
            let attachment = *app.world().get::<topology::Attachment>(target).unwrap();
            let peer_attachment = *app.world().get::<topology::Attachment>(peer).unwrap();
            let entity = body(app.world_mut(), target, true);
            let original_parts = shape_entities(app.world_mut(), entity);
            assert!(manipulation::begin(
                app.world_mut(),
                root,
                target,
                operation,
                [0.5; 2]
            ));
            assert!(manipulation::propose(app.world_mut(), [50.0, 20.0], None));
            tests::advance(&mut app, 3);
            matches_visible_shape(app.world_mut(), target, true);
            matches_visible_shape(app.world_mut(), peer, true);
            assert_eq!(
                app.world().get::<CanvasItem>(target).unwrap().position,
                original.position
            );
            assert_eq!(
                app.world().get::<CanvasItem>(peer).unwrap().position,
                original_peer.position
            );
            assert_eq!(
                *app.world()
                    .get::<topology::groups::GroupPose>(target)
                    .unwrap(),
                pose
            );
            assert_eq!(
                *app.world().get::<topology::Attachment>(target).unwrap(),
                attachment
            );
            assert_eq!(
                *app.world().get::<topology::Attachment>(peer).unwrap(),
                peer_attachment
            );
            let parts = shape_entities(app.world_mut(), entity);
            assert_eq!(parts.len(), 2);
            if operation == Manipulation::Move {
                assert_eq!(parts, original_parts);
            } else {
                assert_ne!(parts, original_parts);
            }
            tests::advance(&mut app, 8);
            assert_eq!(shape_entities(app.world_mut(), entity), parts);
            if commit {
                assert!(manipulation::commit(app.world_mut()));
            } else {
                manipulation::cancel(app.world_mut());
            }
            tests::advance(&mut app, 3);
            matches_visible_shape(app.world_mut(), target, true);
            matches_visible_shape(app.world_mut(), peer, true);
            assert_eq!(
                *app.world()
                    .get::<topology::groups::GroupPose>(target)
                    .unwrap(),
                pose
            );
            assert_eq!(
                *app.world().get::<topology::Attachment>(peer).unwrap(),
                peer_attachment
            );
            assert_eq!(
                app.world()
                    .get::<topology::Attachment>(target)
                    .unwrap()
                    .rotation,
                attachment.rotation
            );
            if commit {
                assert_ne!(
                    *app.world().get::<topology::Attachment>(target).unwrap(),
                    attachment
                );
                let offset = topology::position(app.world(), target).unwrap()
                    - topology::position(app.world(), peer).unwrap();
                let position = app.world().get::<Position>(entity).unwrap().0;
                let before_move = topology::position(app.world(), target).unwrap();
                app.world_mut()
                    .entity_mut(entity)
                    .insert(Position(position + DVec3::X * 20.0))
                    .remove::<Sleeping>();
                tests::advance(&mut app, 2);
                let current = topology::position(app.world(), target).unwrap()
                    - topology::position(app.world(), peer).unwrap();
                assert!(
                    current.distance(offset) < 0.00001,
                    "Committed relative pose snapped back: {offset:?} -> {current:?}"
                );
                assert!(
                    topology::position(app.world(), target)
                        .unwrap()
                        .distance(before_move + DVec3::X * 20.0)
                        < 0.00001
                );
            } else {
                assert_eq!(
                    *app.world().get::<topology::Attachment>(target).unwrap(),
                    attachment
                );
                assert_eq!(
                    app.world().get::<CanvasItem>(target).unwrap().position,
                    original.position
                );
            }
            assert_eq!(shape_entities(app.world_mut(), entity).len(), 2);
        }
    }
}

fn group_fixture() -> (App, Entity, [Entity; 2]) {
    let (mut app, root, target) = fixture(true, false);
    let original = *app.world().get::<CanvasItem>(target).unwrap();
    let peer = app
        .world_mut()
        .spawn((
            CanvasItem {
                position: original.position + DVec2::new(160.0, 0.0),
                size: original.size,
            },
            WorkspaceMember(1),
            ChildOf(root),
            topology::Spatial {
                elevation: 100.0,
                depth: Some(20.0),
                ..default()
            },
        ))
        .id();
    for entity in [target, peer] {
        app.world_mut()
            .entity_mut(entity)
            .insert(crate::native::canvas_selection::SandGroup([12; 16]));
    }
    topology::groups::attach(app.world_mut(), &[target, peer]);
    tests::advance(&mut app, 3);
    (app, root, [target, peer])
}

#[test]
fn tiled_group_resize_preserves_free_attachments_and_restores_its_compound_on_untile() {
    use crate::{host::DesktopAction, native::actions::Action};

    let (mut app, root, members) = group_fixture();
    let saved = members.map(|entity| {
        (
            *app.world().get::<CanvasItem>(entity).unwrap(),
            topology::spatial(app.world(), entity),
            *app.world()
                .get::<topology::groups::GroupPose>(entity)
                .unwrap(),
            *app.world().get::<topology::Attachment>(entity).unwrap(),
        )
    });
    DesktopAction::ToggleTiling.apply(app.world_mut(), root);
    tests::advance(&mut app, 4);
    assert_eq!(
        app.world_mut()
            .query::<&topology::physics::Body>()
            .iter(app.world())
            .count(),
        0
    );
    assert!(manipulation::begin(
        app.world_mut(),
        root,
        members[0],
        Manipulation::Resize,
        [1.0; 2]
    ));
    assert!(manipulation::propose(app.world_mut(), [100.0, 40.0], None));
    assert!(manipulation::commit(app.world_mut()));
    tests::advance(&mut app, 3);
    for (entity, (_, _, pose, attachment)) in members.into_iter().zip(&saved) {
        assert_eq!(
            app.world().get::<topology::groups::GroupPose>(entity),
            Some(pose)
        );
        assert_eq!(
            app.world().get::<topology::Attachment>(entity),
            Some(attachment)
        );
    }
    DesktopAction::ToggleTiling.apply(app.world_mut(), root);
    tests::advance(&mut app, 4);
    for (entity, (item, spatial, _, attachment)) in members.into_iter().zip(&saved) {
        let current = app.world().get::<CanvasItem>(entity).unwrap();
        assert!(current.position.distance(item.position) < 0.00001);
        assert_eq!(current.size, item.size);
        assert_eq!(topology::spatial(app.world(), entity), *spatial);
        assert_eq!(
            app.world().get::<topology::Attachment>(entity),
            Some(attachment)
        );
        matches_visible_shape(app.world_mut(), entity, true);
    }
    let entity = body(app.world_mut(), members[0], true);
    assert_eq!(
        app.world()
            .get::<topology::physics::Body>(entity)
            .unwrap()
            .members
            .len(),
        2
    );
    assert!(
        !app.world()
            .get::<topology::physics::Body>(entity)
            .unwrap()
            .held
    );
}

#[test]
fn partially_tiled_or_suppressed_groups_exclude_hidden_colliders_without_losing_group_homes() {
    use crate::{
        host::DesktopAction,
        native::{actions::Action, layout::desktop},
    };

    for tiled in [false, true] {
        let (mut app, root, [target, peer]) = group_fixture();
        let saved = [target, peer].map(|entity| {
            (
                topology::position(app.world(), entity).unwrap(),
                *app.world()
                    .get::<topology::groups::GroupPose>(entity)
                    .unwrap(),
                *app.world().get::<topology::Attachment>(entity).unwrap(),
            )
        });
        if tiled {
            DesktopAction::ToggleTiling.apply(app.world_mut(), root);
            tests::advance(&mut app, 3);
            desktop::focus_item(app.world_mut(), target);
            DesktopAction::ToggleFloating.apply(app.world_mut(), root);
        } else {
            let id = app.world().get::<desktop::Managed>(peer).unwrap().id;
            assert!(
                app.world_mut().get_mut::<Workspaces>(root).unwrap().entries[0]
                    .desktop
                    .request_state(
                        id,
                        crate::host::WindowRequest {
                            minimized: Some(true),
                            ..default()
                        }
                    )
            );
        }
        tests::advance(&mut app, 3);
        assert!(if tiled {
            desktop::tiled(app.world(), peer)
        } else {
            app.world().get::<desktop::Suppressed>(peer).is_some()
        });
        let entity = body(app.world_mut(), target, true);
        let link = app.world().get::<topology::physics::Body>(entity).unwrap();
        assert_eq!(link.members, vec![target], "tiled={tiled}");
        assert!(link.held);
        assert_eq!(shape_entities(app.world_mut(), entity).len(), 1);
        matches_visible_shape(app.world_mut(), target, true);
        tests::advance(&mut app, 120);
        for (entity, (_, pose, attachment)) in [target, peer].into_iter().zip(&saved) {
            assert_eq!(
                app.world().get::<topology::groups::GroupPose>(entity),
                Some(pose)
            );
            assert_eq!(
                app.world().get::<topology::Attachment>(entity),
                Some(attachment)
            );
        }
        if tiled {
            desktop::focus_item(app.world_mut(), target);
            DesktopAction::ToggleFloating.apply(app.world_mut(), root);
            tests::advance(&mut app, 3);
            assert!(desktop::tiled(app.world(), target));
            assert_eq!(
                app.world_mut()
                    .query::<&topology::physics::Body>()
                    .iter(app.world())
                    .count(),
                0
            );
            DesktopAction::ToggleTiling.apply(app.world_mut(), root);
        } else {
            let id = app.world().get::<desktop::Managed>(peer).unwrap().id;
            assert!(
                app.world_mut().get_mut::<Workspaces>(root).unwrap().entries[0]
                    .desktop
                    .request_state(
                        id,
                        crate::host::WindowRequest {
                            minimized: Some(false),
                            ..default()
                        }
                    )
            );
        }
        tests::advance(&mut app, 4);
        let entity = body(app.world_mut(), target, true);
        let link = app.world().get::<topology::physics::Body>(entity).unwrap();
        assert_eq!(link.members.len(), 2);
        assert!(!link.held);
        for (entity, (position, _, attachment)) in [target, peer].into_iter().zip(&saved) {
            let current = topology::position(app.world(), entity).unwrap();
            assert!(
                current.distance(*position) < 0.00001,
                "tiled={tiled}, {entity:?}: {position:?} -> {current:?}"
            );
            assert_eq!(
                app.world().get::<topology::Attachment>(entity),
                Some(attachment)
            );
            matches_visible_shape(app.world_mut(), entity, true);
        }
    }
}
