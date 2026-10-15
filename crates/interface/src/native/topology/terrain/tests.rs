use super::*;
use crate::native::{
    canvas::{CanvasItem, CanvasView},
    workspace::{WorkspaceMember, Workspaces},
};
use nucleus::terrain::Preset;

fn fixture() -> (World, Entity) {
    let mut world = World::new();
    world.init_resource::<Terrain>();
    let root = world
        .spawn((
            Workspaces::default(),
            CanvasView::default(),
            super::super::view::View::default(),
        ))
        .id();
    (world, root)
}

#[test]
fn empty_floor_does_not_evaluate_formulas() {
    let (mut world, root) = fixture();
    prepare(&mut world);
    support(&mut world);
    assert_eq!(height(&world, root, 1, DVec2::new(1e9, -1e9)), 0.0);
    let terrain = world.resource::<Terrain>();
    assert_eq!(terrain.evaluations, 0);
    assert_eq!(terrain.rebuilds, 0);
    assert!(terrain.tiles.is_empty());
}

#[test]
fn tiles_are_cached_and_moving_an_effect_restores_its_old_location() {
    let (mut world, root) = fixture();
    let entity = spawn(&mut world, root, 1, DVec2::ZERO, TopologyEffect::default()).unwrap();
    prepare(&mut world);
    let mut terrain = world.remove_resource::<Terrain>().unwrap();
    let key = Key(root, 1, 0, 0);
    tile(&mut terrain, &mut world, key, false);
    let evaluations = terrain.evaluations;
    let rebuilds = terrain.rebuilds;
    tile(&mut terrain, &mut world, key, false);
    assert_eq!(terrain.evaluations, evaluations);
    assert_eq!(terrain.rebuilds, rebuilds);
    world.insert_resource(terrain);
    assert_eq!(height(&world, root, 1, DVec2::ZERO), 100.0);
    world.get_mut::<CanvasItem>(entity).unwrap().position = DVec2::new(2000.0, 0.0);
    prepare(&mut world);
    let mut terrain = world.remove_resource::<Terrain>().unwrap();
    tile(&mut terrain, &mut world, key, false);
    world.insert_resource(terrain);
    assert_eq!(height(&world, root, 1, DVec2::ZERO), 0.0);
    assert_eq!(height(&world, root, 1, DVec2::new(2000.0, 0.0)), 100.0);
}

#[test]
fn opposite_fields_cancel_and_a_fault_remains_suspended_until_edited() {
    let (mut world, root) = fixture();
    spawn(&mut world, root, 1, DVec2::ZERO, TopologyEffect::default()).unwrap();
    spawn(
        &mut world,
        root,
        1,
        DVec2::ZERO,
        TopologyEffect {
            preset: Preset::Valley,
            ..Default::default()
        },
    )
    .unwrap();
    prepare(&mut world);
    assert_eq!(height(&world, root, 1, DVec2::new(100.0, 30.0)), 0.0);
    let faulty = spawn(
        &mut world,
        root,
        1,
        DVec2::ZERO,
        TopologyEffect {
            preset: Preset::Formula,
            formula: "1/0".into(),
            ..Default::default()
        },
    )
    .unwrap();
    prepare(&mut world);
    let mut terrain = world.remove_resource::<Terrain>().unwrap();
    tile(&mut terrain, &mut world, Key(root, 1, 0, 0), false);
    world.insert_resource(terrain);
    assert!(world.get::<Fault>(faulty).is_some());
    for _ in 0..3 {
        prepare(&mut world);
    }
    assert!(
        world.resource::<Terrain>().fields[&(root, 1)]
            .iter()
            .all(|f| f.input.entity != faulty)
    );
    world.get_mut::<Effect>(faulty).unwrap().0.formula = "1".into();
    prepare(&mut world);
    assert!(world.get::<Fault>(faulty).is_none());
    assert_eq!(height(&world, root, 1, DVec2::ZERO), 100.0);
}

#[test]
fn generic_area_anchor_chains_and_deletion_preserve_the_last_position() {
    let (mut world, root) = fixture();
    let id = nucleus::new_uid("placement");
    let area = crate::native::area::spawn_area(
        &mut world,
        root,
        1,
        crate::native::area::InfluenceArea::new(
            crate::native::area::AreaShape::Circle,
            DVec2::new(100.0, 200.0),
            DVec2::splat(500.0),
        ),
    )
    .unwrap();
    world
        .entity_mut(area)
        .insert(crate::native::canvas_host::Identity(id.clone()));
    let effect = spawn(&mut world, root, 1, DVec2::ZERO, TopologyEffect::default()).unwrap();
    world.entity_mut(effect).insert(Bound(SpatialBinding {
        anchor: Anchor::Item(id),
        offset: [50.0, 0.0, -70.0],
        ..Default::default()
    }));
    prepare(&mut world);
    assert_eq!(
        world.get::<CanvasItem>(effect).unwrap().position,
        DVec2::new(150.0, 130.0)
    );
    world
        .get_mut::<crate::native::area::InfluenceArea>(area)
        .unwrap()
        .enabled = false;
    super::super::set_position(&mut world, area, DVec3::new(1000.0, 0.0, 2000.0));
    prepare(&mut world);
    assert_eq!(
        world.get::<CanvasItem>(effect).unwrap().position,
        DVec2::new(1050.0, 1930.0)
    );
    world.despawn(area);
    prepare(&mut world);
    assert_eq!(world.get::<Bound>(effect).unwrap().0.anchor, Anchor::Fixed);
    assert_eq!(
        world.get::<CanvasItem>(effect).unwrap().position,
        DVec2::new(1050.0, 1930.0)
    );
}

#[test]
fn camera_floor_position_orientation_and_item_chains_resolve_before_effects() {
    let (mut world, root) = fixture();
    let camera = super::super::view::View {
        spatial: true,
        position: [120.0, 800.0, 240.0],
        yaw: std::f32::consts::FRAC_PI_2,
        ..Default::default()
    };
    world.entity_mut(root).insert(camera);
    let parent = spawn(
        &mut world,
        root,
        1,
        DVec2::ZERO,
        TopologyEffect {
            enabled: false,
            ..Default::default()
        },
    )
    .unwrap();
    let parent_id = world
        .get::<crate::native::canvas_host::Identity>(parent)
        .unwrap()
        .0
        .clone();
    world.entity_mut(parent).insert(Bound(SpatialBinding {
        anchor: Anchor::Camera,
        offset: [40.0, 0.0, 0.0],
        camera_floor: false,
        follow_orientation: false,
        ..Default::default()
    }));
    let child = spawn(&mut world, root, 1, DVec2::ZERO, TopologyEffect::default()).unwrap();
    world.entity_mut(child).insert(Bound(SpatialBinding {
        anchor: Anchor::Item(parent_id),
        offset: [0.0, 0.0, 30.0],
        ..Default::default()
    }));
    prepare(&mut world);
    assert_eq!(
        super::super::position(&world, child).unwrap(),
        DVec3::new(160.0, 800.0, 270.0)
    );
    world.get_mut::<Bound>(parent).unwrap().0.follow_orientation = true;
    world.get_mut::<Bound>(parent).unwrap().0.camera_floor = true;
    let floor = height(&world, root, 1, DVec2::new(120.0, 240.0));
    prepare(&mut world);
    let position = super::super::position(&world, parent).unwrap();
    assert!((position.x - 120.0).abs() < 0.001);
    assert!((position.z - 200.0).abs() < 0.001);
    assert!((position.y - floor).abs() < 0.001);
}

#[test]
fn effects_save_and_reload_with_stable_anchor_references() {
    let (mut world, root) = fixture();
    let a = spawn(
        &mut world,
        root,
        1,
        DVec2::new(100.0, 200.0),
        TopologyEffect::default(),
    )
    .unwrap();
    let id = world
        .get::<crate::native::canvas_host::Identity>(a)
        .unwrap()
        .0
        .clone();
    let b = spawn(
        &mut world,
        root,
        1,
        DVec2::ZERO,
        TopologyEffect {
            preset: Preset::Valley,
            ..Default::default()
        },
    )
    .unwrap();
    world.entity_mut(b).insert(Bound(SpatialBinding {
        anchor: Anchor::Item(id),
        offset: [30.0, 0.0, 40.0],
        ..Default::default()
    }));
    prepare(&mut world);
    let saved: Vec<Saved> =
        serde_json::from_str(&serde_json::to_string(&snapshot(&mut world, root)).unwrap()).unwrap();
    assert!(saved.iter().all(Saved::valid));
    let (mut restored, restored_root) = fixture();
    for saved in saved {
        saved.restore(&mut restored, restored_root);
    }
    prepare(&mut restored);
    let valley = restored
        .query::<(&Effect, &CanvasItem)>()
        .iter(&restored)
        .find(|(e, _)| e.0.preset == Preset::Valley)
        .unwrap();
    assert_eq!(valley.1.position, DVec2::new(130.0, 240.0));
}

fn physics_fixture() -> (App, Entity, Entity) {
    let mut app = App::new();
    crate::native::laboratory::isolate(app.world_mut());
    app.add_plugins((
        MinimalPlugins,
        crate::native::physics::WorkspacePhysicsPlugin,
    ))
    .init_resource::<super::super::physics::Runtime>()
    .init_resource::<Terrain>()
    .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
        std::time::Duration::from_secs_f64(1.0 / 120.0),
    ));
    app.finish();
    app.cleanup();
    let root = app
        .world_mut()
        .spawn((Workspaces::default(), CanvasView::default()))
        .id();
    crate::native::workspace_config::set_physics(app.world_mut(), root, 1, true);
    let sand = app
        .world_mut()
        .spawn((
            CanvasItem {
                position: DVec2::new(150.0, 0.0),
                size: Vec2::splat(20.0),
            },
            super::super::Spatial {
                elevation: 300.0,
                ..Default::default()
            },
            WorkspaceMember(1),
            ChildOf(root),
        ))
        .id();
    (app, root, sand)
}

#[test]
fn gravity_settles_on_flat_ground_without_terrain_work_and_then_sleeps() {
    let (mut app, _, sand) = physics_fixture();
    for _ in 0..1200 {
        app.update();
    }
    let position = super::super::position(app.world(), sand).unwrap();
    assert!((position.y - 20.0).abs() < 1.0, "{position:?}");
    assert_eq!(position.x, 150.0);
    assert!(!super::super::physics::awake(app.world_mut()));
    let terrain = app.world().resource::<Terrain>();
    assert_eq!(terrain.evaluations, 0);
    assert_eq!(terrain.rebuilds, 0);
}

#[test]
fn gravity_slides_into_a_valley_and_removing_terrain_wakes_support() {
    let (mut app, root, sand) = physics_fixture();
    let effect = spawn(
        app.world_mut(),
        root,
        1,
        DVec2::ZERO,
        TopologyEffect {
            amplitude: 200.0,
            preset: Preset::Valley,
            ..Default::default()
        },
    )
    .unwrap();
    for _ in 0..1200 {
        app.update();
    }
    let position = super::super::position(app.world(), sand).unwrap();
    assert!(position.x < 100.0, "{position:?}");
    assert!(position.y < 0.0, "{position:?}");
    app.world_mut().get_mut::<Effect>(effect).unwrap().0.enabled = false;
    for _ in 0..600 {
        app.update();
    }
    let position = super::super::position(app.world(), sand).unwrap();
    assert!((position.y - 20.0).abs() < 1.0, "{position:?}");
}

#[test]
fn cursor_offsets_use_canvas_units_and_cursor_exit_holds_the_last_position() {
    let (mut world, root) = fixture();
    world.entity_mut(root).insert((
        ComputedNode {
            size: Vec2::splat(1000.0),
            ..Default::default()
        },
        UiGlobalTransform::from(bevy::math::Affine2::from_translation(Vec2::splat(500.0))),
    ));
    let mut window = Window::default();
    window.set_cursor_position(Some(Vec2::new(300.0, 200.0)));
    let window = world.spawn((window, bevy::window::PrimaryWindow)).id();
    let effect = spawn(&mut world, root, 1, DVec2::ZERO, TopologyEffect::default()).unwrap();
    world.entity_mut(effect).insert(Bound(SpatialBinding {
        anchor: Anchor::Cursor,
        offset: [50.0, 0.0, -20.0],
        ..Default::default()
    }));
    prepare(&mut world);
    assert_eq!(
        world.get::<CanvasItem>(effect).unwrap().position,
        DVec2::new(-150.0, -320.0)
    );
    world.get_mut::<CanvasView>(root).unwrap().zoom = 2.0;
    prepare(&mut world);
    assert_eq!(
        world.get::<CanvasItem>(effect).unwrap().position,
        DVec2::new(-50.0, -170.0)
    );
    world
        .get_mut::<Window>(window)
        .unwrap()
        .set_cursor_position(None);
    prepare(&mut world);
    assert_eq!(
        world.get::<CanvasItem>(effect).unwrap().position,
        DVec2::new(-50.0, -170.0)
    );
}

#[test]
fn visual_preview_does_not_change_the_applied_physical_floor() {
    let (mut world, root) = fixture();
    let effect = spawn(&mut world, root, 1, DVec2::ZERO, TopologyEffect::default()).unwrap();
    world.entity_mut(effect).insert(Preview(TopologyEffect {
        preset: Preset::Valley,
        ..Default::default()
    }));
    prepare(&mut world);
    assert_eq!(height(&world, root, 1, DVec2::ZERO), 100.0);
    draw(&mut world);
    assert_eq!(height(&world, root, 1, DVec2::ZERO), 100.0);
    assert!(world.resource::<Terrain>().render_tiles[&Key(root, 1, 0, 0)].heights[0] < 0.0);
    assert!(world.resource::<Terrain>().tiles.is_empty());
}

#[test]
fn floor_areas_affect_gravity_supported_sands_through_their_support_point() {
    let (mut world, root) = fixture();
    world.init_resource::<super::super::physics::Runtime>();
    let mut area = crate::native::area::InfluenceArea::new(
        crate::native::area::AreaShape::Circle,
        DVec2::ZERO,
        DVec2::splat(500.0),
    );
    area.strength = 500.0;
    area.rules = vec![crate::native::area::PropertyRule {
        property: crate::native::area::Property::Title,
        value: "Card".into(),
    }];
    crate::native::area::spawn_area(&mut world, root, 1, area).unwrap();
    let sand = world
        .spawn((
            CanvasItem {
                position: DVec2::new(150.0, 0.0),
                size: Vec2::splat(20.0),
            },
            super::super::Spatial {
                elevation: 20.1,
                ..Default::default()
            },
            crate::native::area::RecordProperties(serde_json::json!({"head":"Card"})),
            WorkspaceMember(1),
            ChildOf(root),
        ))
        .id();
    crate::native::area_effects::update(&mut world);
    let force = world.resource::<super::super::influence::Forces>().totals[&sand];
    assert!(force.x < 0.0);
    assert!(force.y.abs() < 1e-6);
}

#[test]
fn excessive_footprints_pause_safely_and_resume_on_flat_ground() {
    let (mut app, root, sand) = physics_fixture();
    spawn(
        app.world_mut(),
        root,
        1,
        DVec2::ZERO,
        TopologyEffect::default(),
    )
    .unwrap();
    app.world_mut().get_mut::<CanvasItem>(sand).unwrap().size = Vec2::splat(100_000.0);
    for _ in 0..4 {
        app.update();
    }
    assert!(app.world().get::<Unsupported>(sand).is_some());
    assert!(!super::super::physics::awake(app.world_mut()));
    assert!(app.world().resource::<Terrain>().tiles.len() <= MAX_TILES);
    app.world_mut().get_mut::<CanvasItem>(sand).unwrap().size = Vec2::splat(20.0);
    for _ in 0..4 {
        app.update();
    }
    assert!(app.world().get::<Unsupported>(sand).is_none());
}

#[test]
fn moving_bodies_evict_collision_tiles_outside_their_current_support() {
    let (mut app, root, sand) = physics_fixture();
    spawn(
        app.world_mut(),
        root,
        1,
        DVec2::ZERO,
        TopologyEffect::default(),
    )
    .unwrap();
    for i in 0..20 {
        app.world_mut()
            .get_mut::<CanvasItem>(sand)
            .unwrap()
            .position = DVec2::new(i as f64 * 2000.0, 0.0);
        app.update();
        assert!(app.world().resource::<Terrain>().tiles.len() < 16);
    }
}
