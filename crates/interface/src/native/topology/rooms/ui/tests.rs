use super::*;
use crate::native::{
    canvas::CanvasItem,
    sand_store::{SandKind, spawn_sand},
    topology,
};
use bevy::{
    input_focus::{FocusCause, InputFocus},
    math::DVec2,
};

fn fixture() -> (App, Entity) {
    let (mut app, root) = crate::native::edit_mode::tests::fixture();
    app.init_resource::<Assets<Image>>();
    app.world_mut()
        .get_mut::<crate::native::edit_mode::EditMode>(root)
        .unwrap()
        .enabled = true;
    (app, root)
}

fn new_room(world: &mut World, root: Entity) -> Entity {
    perform(world, root, Control::Create).unwrap();
    world
        .query_filtered::<Entity, With<Room>>()
        .single(world)
        .unwrap()
}

#[test]
fn room_canvas_receipts_preserve_the_spatial_canvas_ghost_root() {
    let (mut app, root) = fixture();
    let world = app.world_mut();
    world.entity_mut(root).remove::<Node>().insert((
        bevy::ui::experimental::GhostNode,
        topology::presentation::SpatialRoot,
    ));
    let room = new_room(world, root);
    create_proxy(world, room, model::Role::Floor).unwrap();
    perform(world, room, Control::Ambient).unwrap();
    assert!(world.get::<Node>(root).is_none());
    assert!(
        world
            .get::<bevy::ui::experimental::GhostNode>(root)
            .is_some()
    );
    assert!(
        world
            .get::<crate::native::canvas::CanvasView>(root)
            .is_some()
    );
}

#[test]
fn room_proxy_controls_create_calibrated_surfaces_and_undo_without_losing_the_room() {
    let (mut app, root) = fixture();
    let world = app.world_mut();
    let room = new_room(world, root);
    let roles = [
        model::Role::Floor,
        model::Role::Wall,
        model::Role::Desk,
        model::Role::Computer,
    ];
    let proxies = roles.map(|role| create_proxy(world, room, role).unwrap());
    world
        .entity_mut(root)
        .insert(crate::native::inspection::Inspection::default());
    perform(world, proxies[0], Control::Inspect(room)).unwrap();
    assert_eq!(
        world
            .get::<crate::native::inspection::Inspection>(root)
            .unwrap()
            .selected,
        Some(room)
    );
    let floor_text = world
        .get::<Children>(proxies[0])
        .unwrap()
        .iter()
        .find(|child| {
            world
                .get::<crate::native::sand_text::SandText>(*child)
                .is_some()
        })
        .unwrap();
    world
        .get_mut::<crate::native::sand_text::SandText>(floor_text)
        .unwrap()
        .settings
        .0
        .insert(
            "height".into(),
            crate::native::sand_settings::Value::Number(24.0),
        );
    assert_eq!(
        world.get::<CanvasItem>(proxies[0]).unwrap().size,
        Vec2::new(1200.0, 900.0)
    );
    assert_eq!(
        world.get::<Member>(proxies[1]).unwrap().0.local.position,
        [0.0, 150.0, -450.0]
    );
    assert!(
        (DQuat::from_array(world.get::<Member>(proxies[1]).unwrap().0.local.rotation)
            * bevy::math::DVec3::Y
            - bevy::math::DVec3::Z)
            .length()
            < 1e-10
    );
    let locals = proxies.map(|entity| world.get::<Member>(entity).unwrap().0.local.clone());
    for (entity, role) in proxies.iter().zip(roles) {
        assert_eq!(world.get::<Member>(*entity).unwrap().0.role, role);
        assert_eq!(topology::spatial(world, *entity).depth, Some(4.0));
        assert!(
            world
                .get::<topology::appearance::Material>(*entity)
                .unwrap()
                .0
                .lit
        );
    }
    let (id, mut settings) = metadata(world, root, room).unwrap();
    settings.spatial.rotation = DQuat::from_rotation_y(0.6).to_array();
    settings.spatial.elevation = 40.0;
    transact(
        world,
        root,
        api::Mutation::SetMetadata {
            placement: id,
            metadata: settings,
        },
    )
    .unwrap();
    let parent = runtime::world_pose(world, room).unwrap();
    for (entity, local) in proxies.iter().zip(&locals) {
        let expected = runtime::compose(&parent, local);
        assert!(
            (topology::position(world, *entity).unwrap()
                - bevy::math::DVec3::from_array(expected.position))
            .length()
                < 1e-8
        );
    }
    let snapshot = canvas_host::capture(world, root).unwrap();
    assert_eq!(
        snapshot
            .placements
            .iter()
            .filter(|placement| placement.metadata.room_member.is_some())
            .count(),
        4
    );
    perform(world, room, Control::Undo).unwrap();
    assert!(world.get_entity(proxies[3]).is_err());
    assert!(world.get::<Room>(room).is_some());
    assert!(
        proxies[..3]
            .iter()
            .all(|entity| world.get::<Member>(*entity).is_some())
    );
    world
        .entity_mut(room)
        .insert(crate::native::workspace_sync::HostedReplica);
    assert!(create_proxy(world, room, model::Role::Floor).is_err());
}

fn field(world: &mut World, owner: Entity, key: u8, value: &str) -> Entity {
    let entity = world
        .query::<(Entity, &Field)>()
        .iter(world)
        .find(|(_, field)| field.owner == owner && field.key == key)
        .unwrap()
        .0;
    world
        .get_mut::<bevy::text::EditableText>(entity)
        .unwrap()
        .editor
        .set_text(value);
    entity
}

fn press(world: &mut World, owner: Entity, control: Control) -> Entity {
    let entity = world
        .query::<(Entity, &Button, &crate::native::actions::ActionButton)>()
        .iter(world)
        .find(|(_, button, action)| button.0 == control && action.target == owner)
        .unwrap()
        .0;
    let action = world
        .get::<crate::native::actions::ActionButton>(entity)
        .unwrap()
        .clone();
    action.actions.run(world, action.target);
    entity
}

#[test]
fn room_controls_create_parent_move_reset_detach_and_undo_through_canvas_receipts() {
    let (mut app, root) = fixture();
    let world = app.world_mut();
    let room = new_room(world, root);
    let child = spawn_sand(
        world,
        root,
        1,
        SandKind::Square,
        "Desk",
        DVec2::new(120.0, 80.0),
    );
    let before = runtime::world_pose(world, child).unwrap();
    perform(world, child, Control::Attach(room, None)).unwrap();
    assert_eq!(runtime::world_pose(world, child).unwrap(), before);
    let member = world.get::<Member>(child).unwrap().0.clone();
    let room_id = world.get::<Identity>(room).unwrap().0.clone();
    transact(
        world,
        root,
        api::Mutation::Move {
            placement: room_id.clone(),
            workspace: 1,
            position: [100.0, 200.0],
        },
    )
    .unwrap();
    assert_eq!(
        topology::position(world, child).unwrap(),
        bevy::math::DVec3::new(220.0, 0.0, 280.0)
    );
    assert_eq!(world.get::<Member>(child).unwrap().0, member);
    perform(world, child, Control::Detach).unwrap();
    let detached = runtime::world_pose(world, child).unwrap();
    perform(world, child, Control::Undo).unwrap();
    assert_eq!(world.get::<Member>(child).unwrap().0, member);
    assert_eq!(runtime::world_pose(world, child).unwrap(), detached);
    perform(world, room, Control::ResetRoom).unwrap();
    assert_eq!(runtime::world_pose(world, child).unwrap(), before);
    perform(world, room, Control::Undo).unwrap();
    assert_eq!(runtime::world_pose(world, child).unwrap(), detached);
    assert_eq!(world.get::<Member>(child).unwrap().0, member);
    perform(world, room, Control::Ambient).unwrap();
    assert!(!world.get::<Room>(room).unwrap().0.ambient_enabled);
    assert!(world.get::<CanvasItem>(child).is_some());
    perform(world, room, Control::Undo).unwrap();
    assert!(world.get::<Room>(room).unwrap().0.ambient_enabled);
}

#[test]
fn room_controls_build_accessible_local_fields_reject_invalid_drafts_and_keep_keyboard_focus() {
    let (mut app, root) = fixture();
    let world = app.world_mut();
    let room = new_room(world, root);
    let child = spawn_sand(
        world,
        root,
        1,
        SandKind::Square,
        "Child",
        DVec2::new(20.0, 30.0),
    );
    perform(world, child, Control::Attach(room, None)).unwrap();
    let panel = world.spawn((Node::default(), ChildOf(root))).id();
    controls(world, panel, child);
    let input = field(world, child, 0, "40, 50, 60");
    assert_eq!(
        world
            .get::<bevy::a11y::AccessibilityNode>(input)
            .unwrap()
            .label(),
        Some("Local position XYZ")
    );
    field(world, child, 1, "0, 90, 0");
    press(world, child, Control::Apply);
    assert_eq!(
        topology::position(world, child).unwrap(),
        bevy::math::DVec3::new(40.0, 50.0, 60.0)
    );
    update_controls(world);
    let before = world.get::<Member>(child).unwrap().0.clone();
    field(world, child, 0, "NaN, 0, 0");
    assert!(perform(world, child, Control::Apply).is_err());
    assert_eq!(world.get::<Member>(child).unwrap().0, before);
    let role = world
        .query::<(Entity, &Button)>()
        .iter(world)
        .find(|(_, button)| button.0 == Control::Role)
        .unwrap()
        .0;
    world
        .resource_mut::<InputFocus>()
        .set(role, FocusCause::Navigated);
    press(world, child, Control::Role);
    update_controls(world);
    let focused = world.resource::<InputFocus>().get().unwrap();
    assert_ne!(focused, role);
    assert!(
        world
            .get::<Button>(focused)
            .is_some_and(|button| button.0 == Control::Role)
    );
    assert_eq!(
        world.get::<Member>(child).unwrap().0.role,
        model::Role::Floor
    );
}

#[test]
fn room_controls_reject_closed_edit_mode_hosted_edits_and_anchor_cycles_without_changing_working_placement()
 {
    let (mut app, root) = fixture();
    let world = app.world_mut();
    world
        .get_mut::<crate::native::edit_mode::EditMode>(root)
        .unwrap()
        .enabled = false;
    assert!(perform(world, root, Control::Create).is_err());
    assert_eq!(world.query::<&Room>().iter(world).count(), 0);
    world
        .get_mut::<crate::native::edit_mode::EditMode>(root)
        .unwrap()
        .enabled = true;
    let room = new_room(world, root);
    let desk = spawn_sand(
        world,
        root,
        1,
        SandKind::Square,
        "Desk",
        DVec2::new(20.0, 30.0),
    );
    let computer = spawn_sand(
        world,
        root,
        1,
        SandKind::Square,
        "Computer",
        DVec2::new(40.0, 50.0),
    );
    perform(world, desk, Control::Attach(room, None)).unwrap();
    perform(world, desk, Control::Role).unwrap();
    perform(world, computer, Control::Attach(room, Some(desk))).unwrap();
    perform(world, computer, Control::Role).unwrap();
    let before = canvas_host::capture(world, root).unwrap();
    assert!(
        perform(world, desk, Control::Attach(room, Some(computer)))
            .unwrap_err()
            .contains("acyclic")
    );
    assert_eq!(canvas_host::capture(world, root).unwrap(), before);
    world
        .entity_mut(desk)
        .insert(crate::native::workspace_sync::HostedReplica);
    assert!(
        perform(world, desk, Control::Detach)
            .unwrap_err()
            .contains("Workspace Sync")
    );
    assert_eq!(world.get::<Member>(desk).unwrap().0.parent, None);
}

#[test]
fn room_creation_undo_checks_all_receipts_before_changing_later_material_work() {
    let (mut app, root) = fixture();
    let world = app.world_mut();
    let room = new_room(world, root);
    let (id, mut value) = metadata(world, root, room).unwrap();
    value.material = Some(nucleus::canvas::appearance::Material {
        lit: true,
        ..default()
    });
    transact(
        world,
        root,
        api::Mutation::SetMetadata {
            placement: id,
            metadata: value,
        },
    )
    .unwrap();
    let before = canvas_host::capture(world, root).unwrap();
    assert!(
        perform(world, room, Control::Undo)
            .unwrap_err()
            .contains("later authored")
    );
    assert_eq!(canvas_host::capture(world, root).unwrap(), before);
    assert!(world.get::<Room>(room).is_some());
}

#[test]
fn room_controls_attach_move_and_detach_complete_groups_atomically() {
    let (mut app, root) = fixture();
    let world = app.world_mut();
    let room = new_room(world, root);
    let children = [DVec2::new(40.0, 60.0), DVec2::new(140.0, 60.0)]
        .map(|point| spawn_sand(world, root, 1, SandKind::Square, "Board", point));
    for child in children {
        world
            .entity_mut(child)
            .insert(crate::native::canvas_selection::SandGroup([9; 16]));
    }
    topology::groups::attach(world, &children);
    perform(world, children[0], Control::Attach(room, None)).unwrap();
    assert!(
        children
            .iter()
            .all(|child| world.get::<Member>(*child).is_some())
    );
    let panel = world.spawn((Node::default(), ChildOf(root))).id();
    controls(world, panel, children[0]);
    field(world, children[0], 0, "20, 50, 80");
    field(world, children[0], 1, "0, 90, 0");
    perform(world, children[0], Control::Apply).unwrap();
    let first = topology::position(world, children[0]).unwrap();
    let second = topology::position(world, children[1]).unwrap();
    assert!((first - bevy::math::DVec3::new(20.0, 50.0, 80.0)).length() < 1e-8);
    assert!((second - first - bevy::math::DVec3::new(0.0, 0.0, -100.0)).length() < 1e-8);
    assert_eq!(
        world.get::<topology::groups::GroupPose>(children[0]),
        world.get::<topology::groups::GroupPose>(children[1])
    );
    perform(world, children[0], Control::Detach).unwrap();
    assert!(
        children
            .iter()
            .all(|child| world.get::<Member>(*child).is_none())
    );
    perform(world, children[0], Control::Undo).unwrap();
    assert!(
        children
            .iter()
            .all(|child| world.get::<Member>(*child).is_some())
    );
    assert_eq!(topology::position(world, children[0]), Some(first));
    assert_eq!(topology::position(world, children[1]), Some(second));
}

#[test]
fn room_anchor_alignment_moves_complete_groups_with_one_receipt_and_restores_them_on_undo() {
    let (mut app, root) = fixture();
    let world = app.world_mut();
    let room = new_room(world, root);
    let wall = create_proxy(world, room, model::Role::Wall).unwrap();
    let children = [DVec2::new(40.0, 60.0), DVec2::new(140.0, 90.0)]
        .map(|point| spawn_sand(world, root, 1, SandKind::Square, "Board", point));
    for child in children {
        world
            .entity_mut(child)
            .insert(crate::native::canvas_selection::SandGroup([27; 16]));
    }
    topology::groups::attach(world, &children);
    let original = children.map(|entity| runtime::world_pose(world, entity).unwrap());
    let panel = world.spawn((Node::default(), ChildOf(root))).id();
    controls(world, panel, children[0]);
    assert!(
        choices(world, children[0])
            .iter()
            .any(|(_, _, anchor)| *anchor == Some(wall))
    );
    let history = world.get::<History>(root).unwrap().0.len();
    press(world, children[0], Control::Mount(room, Some(wall)));
    assert_eq!(world.get::<History>(root).unwrap().0.len(), history + 1);
    assert_eq!(
        world.get::<History>(root).unwrap().0.last().unwrap().len(),
        1
    );
    let wall_pose = runtime::world_pose(world, wall).unwrap();
    let aligned = runtime::compose(&wall_pose, &mount_pose(world, wall).unwrap());
    for (entity, old) in children.iter().zip(&original) {
        let expected = runtime::compose(&aligned, &runtime::relative(&original[0], old));
        let actual = runtime::world_pose(world, *entity).unwrap();
        assert!(
            (bevy::math::DVec3::from_array(actual.position)
                - bevy::math::DVec3::from_array(expected.position))
            .length()
                < 1e-7
        );
        assert!(
            DQuat::from_array(actual.rotation)
                .dot(DQuat::from_array(expected.rotation))
                .abs()
                > 1.0 - 1e-10
        );
        assert_eq!(
            world.get::<Member>(*entity).unwrap().0.parent.as_ref(),
            Some(&world.get::<Identity>(wall).unwrap().0)
        );
    }
    let snapshot = canvas_host::capture(world, root).unwrap();
    let decoded: api::Snapshot =
        serde_json::from_slice(&serde_json::to_vec(&snapshot).unwrap()).unwrap();
    api::State::new(decoded, canvas_host::registry())
        .unwrap()
        .validate()
        .unwrap();
    topology::set_position(world, room, bevy::math::DVec3::new(1e9, 200.0, -1e9));
    let mut spatial = topology::spatial(world, room);
    spatial.rotation = DQuat::from_rotation_y(0.7).to_array();
    world.entity_mut(room).insert(spatial);
    super::super::synchronize(world);
    perform(world, children[0], Control::Undo).unwrap();
    for (entity, old) in children.iter().zip(&original) {
        assert!(world.get::<Member>(*entity).is_none());
        assert_eq!(runtime::world_pose(world, *entity).unwrap(), *old);
    }
    world
        .get_mut::<crate::native::edit_mode::EditMode>(root)
        .unwrap()
        .enabled = false;
    assert!(
        mount(world, children[0], wall)
            .unwrap_err()
            .contains("Edit mode")
    );
    assert!(world.get::<Member>(children[0]).is_none());
}

#[test]
fn room_record_controls_override_detach_restore_defaults_and_reject_hosted_sources() {
    let (mut app, root, source) = crate::native::protein_area::tests::fixture();
    app.add_plugins((
        crate::native::edit_mode::EditModePlugin,
        crate::native::layout::LayoutPlugin,
    ));
    app.update();
    let world = app.world_mut();
    world
        .get_mut::<crate::native::edit_mode::EditMode>(root)
        .unwrap()
        .enabled = true;
    crate::native::protein_area::tests::authoring_record(
        world,
        source,
        serde_json::json!({"uid":"room-record", "head":"Room task", "quantity":"7"}),
    );
    app.update();
    let world = app.world_mut();
    let row = world
        .query::<(
            Entity,
            &crate::native::protein_area::RecordBinding,
            &CanvasItem,
        )>()
        .iter(world)
        .find(|(_, binding, _)| binding.uid == "room-record")
        .unwrap()
        .0;
    let room_id = nucleus::new_uid("placement");
    let room = world
        .spawn((
            CanvasItem {
                position: DVec2::ZERO,
                size: Vec2::splat(1000.0),
            },
            Room(default()),
            Identity(room_id),
            ChildOf(root),
            WorkspaceMember(1),
        ))
        .id();
    let before = runtime::world_pose(world, row).unwrap();
    crate::native::layout::configure(
        world,
        source,
        crate::native::layout::Rules::fixed(Vec2::splat(800.0)),
    )
    .unwrap();
    crate::native::layout::attach(world, row, source).unwrap();
    perform(world, row, Control::Attach(room, None)).unwrap();
    assert_eq!(runtime::world_pose(world, row).unwrap(), before);
    let panel = world.spawn((Node::default(), ChildOf(root))).id();
    controls(world, panel, row);
    field(world, row, 0, "40, 50, 60");
    field(world, row, 1, "0, 90, 0");
    perform(world, row, Control::Apply).unwrap();
    let local = world.get::<Member>(row).unwrap().0.local.clone();
    assert_eq!(local.position, [40.0, 50.0, 60.0]);
    assert!(
        crate::native::layout::attach(world, row, source)
            .unwrap_err()
            .contains("room")
    );
    app.update();
    let world = app.world_mut();
    super::super::synchronize(world);
    assert_eq!(world.get::<Member>(row).unwrap().0.local, local);
    assert_eq!(
        topology::position(world, row),
        Some(bevy::math::DVec3::new(40.0, 50.0, 60.0))
    );
    perform(world, row, Control::Detach).unwrap();
    let detached = runtime::world_pose(world, row).unwrap();
    let encoded = serde_json::to_string(
        world
            .get::<crate::native::area::InfluenceArea>(source)
            .unwrap(),
    )
    .unwrap();
    let restored: crate::native::area::InfluenceArea = serde_json::from_str(&encoded).unwrap();
    assert_eq!(
        restored.records["room-record"].room_world,
        Some(detached.clone())
    );
    assert!(restored.records["room-record"].valid());
    topology::set_position(world, room, bevy::math::DVec3::new(100.0, 20.0, 200.0));
    super::super::synchronize(world);
    assert_eq!(runtime::world_pose(world, row).unwrap(), detached);
    perform(world, row, Control::RecordUndo).unwrap();
    assert_eq!(world.get::<Member>(row).unwrap().0.local, local);
    let anchor_id = nucleus::new_uid("placement");
    let anchor = world
        .spawn((
            CanvasItem {
                position: DVec2::ZERO,
                size: Vec2::splat(100.0),
            },
            Identity(anchor_id.clone()),
            ChildOf(root),
            WorkspaceMember(1),
            Member(model::Member {
                room: world.get::<Identity>(room).unwrap().0.clone(),
                parent: None,
                local: api::Pose {
                    position: [200.0, 100.0, 0.0],
                    rotation: DQuat::from_rotation_x(std::f64::consts::FRAC_PI_2).to_array(),
                },
                role: model::Role::Computer,
            }),
        ))
        .id();
    let effect = crate::native::shader_effects::FloatingSettings {
        effect_id: Some("amber".into()),
        sphere: true,
        ..default()
    };
    world
        .get_mut::<crate::native::area::InfluenceArea>(source)
        .unwrap()
        .records
        .get_mut("room-record")
        .unwrap()
        .shader_placement = Some(effect.clone());
    mount(world, row, anchor).unwrap();
    assert_eq!(
        world.get::<Member>(row).unwrap().0.parent.as_deref(),
        Some(anchor_id.as_str())
    );
    assert_eq!(
        world.get::<Member>(row).unwrap().0.local.position,
        [0.0, 80.0, 0.0]
    );
    let saved: crate::native::area::InfluenceArea = serde_json::from_str(
        &serde_json::to_string(
            world
                .get::<crate::native::area::InfluenceArea>(source)
                .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        saved.records["room-record"].shader_placement.as_ref(),
        Some(&effect)
    );
    assert_eq!(
        saved.records["room-record"]
            .room_member
            .as_ref()
            .unwrap()
            .parent
            .as_deref(),
        Some(anchor_id.as_str())
    );
    let bound = world
        .get::<crate::native::protein_area::RecordBinding>(row)
        .unwrap();
    assert_eq!(bound.uid, "room-record");
    assert_eq!(bound.area, source);
    perform(world, row, Control::RecordUndo).unwrap();
    assert_eq!(world.get::<Member>(row).unwrap().0.local, local);
    assert_eq!(
        topology::position(world, row),
        Some(bevy::math::DVec3::new(140.0, 70.0, 260.0))
    );
    perform(world, row, Control::RecordDefault).unwrap();
    assert!(world.get::<Member>(row).is_none());
    assert!(world.get::<super::super::WorldPlacement>(row).is_none());
    assert_eq!(runtime::world_pose(world, row).unwrap(), before);
    perform(world, row, Control::RecordUndo).unwrap();
    assert_eq!(world.get::<Member>(row).unwrap().0.local, local);
    world
        .entity_mut(source)
        .insert(crate::native::workspace_sync::HostedReplica);
    assert!(
        perform(world, row, Control::RecordDefault)
            .unwrap_err()
            .contains("Workspace Sync")
    );
    assert_eq!(world.get::<Member>(row).unwrap().0.local, local);
}
