use super::*;
use crate::{
    area::{AreaShape, RecordProperties, spawn_area},
    canvas::CanvasView,
    topology::presentation::Surface,
};

fn fixture(count: usize) -> (World, Entity) {
    let mut world = World::new();
    let mut spaces = Workspaces::default();
    let template = spaces.entries[0].clone();
    for index in 1..count {
        let mut space = template.clone();
        space.id = (index + 1) as u64;
        space.name = format!("Workspace {}", space.id);
        spaces.entries.push(space);
    }
    spaces.active = count as u64;
    let root = world
        .spawn((
            spaces,
            CanvasView::default(),
            super::super::view::View::default(),
        ))
        .id();
    (world, root)
}

fn hole(
    world: &mut World,
    root: Entity,
    workspace: u64,
    shape: AreaShape,
    center: DVec2,
    size: f64,
) -> Entity {
    let mut area = InfluenceArea::new(shape, center, DVec2::splat(size));
    area.hole = true;
    spawn_area(world, root, workspace, area).unwrap()
}

fn sand(world: &mut World, root: Entity, workspace: u64, center: DVec2, size: f32) -> Entity {
    world
        .spawn((
            CanvasItem {
                position: center,
                size: Vec2::splat(size),
            },
            WorkspaceMember(workspace),
            ChildOf(root),
        ))
        .id()
}

fn rectangle(center: DVec2, size: f64) -> Polygon {
    vec![
        center + DVec2::new(-size, -size) * 0.5,
        center + DVec2::new(size, -size) * 0.5,
        center + DVec2::new(size, size) * 0.5,
        center + DVec2::new(-size, size) * 0.5,
    ]
}

fn area(pieces: &[Polygon]) -> f64 {
    pieces
        .iter()
        .map(|polygon| signed_area(polygon).abs())
        .sum()
}

#[test]
fn overlapping_holes_clip_both_layers_without_double_drawing() {
    let (mut world, root) = fixture(2);
    hole(
        &mut world,
        root,
        2,
        AreaShape::Square,
        DVec2::new(-25.0, 0.0),
        100.0,
    );
    hole(
        &mut world,
        root,
        2,
        AreaShape::Square,
        DVec2::new(25.0, 0.0),
        100.0,
    );
    refresh(&mut world);
    let lower = pieces(&world, root, 1, rectangle(DVec2::ZERO, 300.0));
    let upper = pieces(&world, root, 2, rectangle(DVec2::ZERO, 300.0));
    assert!((area(&lower) - 15_000.0).abs() < 1e-7);
    assert!((area(&upper) - 75_000.0).abs() < 1e-7);
    assert!(contains(world.get::<Layers>(root), 1, DVec2::ZERO));
    assert!(!contains(world.get::<Layers>(root), 2, DVec2::ZERO));
    assert!(!contains(world.get::<Layers>(root), 1, DVec2::splat(100.0)));
}

#[test]
fn round_and_concave_holes_preserve_the_shape_and_large_coordinates() {
    for center in [DVec2::ZERO, DVec2::new(1e12, -1e12)] {
        let (mut world, root) = fixture(2);
        let circle = hole(&mut world, root, 2, AreaShape::Circle, center, 100.0);
        refresh(&mut world);
        let lower = pieces(&world, root, 1, rectangle(center, 200.0));
        assert!((area(&lower) - std::f64::consts::PI * 2500.0).abs() < 15.0);
        assert!(contains(world.get::<Layers>(root), 1, center));
        assert!(!contains(
            world.get::<Layers>(root),
            1,
            center + DVec2::splat(45.0)
        ));
        world.despawn(circle);
        let shape = AreaShape::Polygon(vec![
            [-0.5, -0.5],
            [0.5, -0.5],
            [0.5, -0.1],
            [-0.1, -0.1],
            [-0.1, 0.5],
            [-0.5, 0.5],
            [-0.5, -0.5],
        ]);
        hole(&mut world, root, 2, shape, center, 100.0);
        refresh(&mut world);
        let lower = pieces(&world, root, 1, rectangle(center, 200.0));
        assert!((area(&lower) - 6400.0).abs() < 0.01);
        assert!(contains(
            world.get::<Layers>(root),
            1,
            center + DVec2::new(-30.0, 30.0)
        ));
        assert!(!contains(
            world.get::<Layers>(root),
            1,
            center + DVec2::new(30.0, 30.0)
        ));
    }
}

#[test]
fn nested_holes_activate_only_the_reachable_workspaces() {
    let (mut world, root) = fixture(3);
    hole(&mut world, root, 3, AreaShape::Square, DVec2::ZERO, 100.0);
    let middle = hole(
        &mut world,
        root,
        2,
        AreaShape::Square,
        DVec2::new(50.0, 0.0),
        100.0,
    );
    refresh(&mut world);
    assert!(live(&world, root, 1));
    assert!((area(&pieces(&world, root, 1, rectangle(DVec2::ZERO, 300.0))) - 5000.0).abs() < 1e-7);
    assert!(contains(
        world.get::<Layers>(root),
        1,
        DVec2::new(25.0, 0.0)
    ));
    assert!(contains(
        world.get::<Layers>(root),
        2,
        DVec2::new(-25.0, 0.0)
    ));
    world.get_mut::<InfluenceArea>(middle).unwrap().center = [200.0, 0.0];
    refresh(&mut world);
    assert!(!live(&world, root, 1));
    assert!(live(&world, root, 2));
}

#[test]
fn disabled_paused_removed_and_isolated_holes_stop_the_lower_workspace() {
    let (mut world, root) = fixture(2);
    let entity = hole(&mut world, root, 2, AreaShape::Square, DVec2::ZERO, 100.0);
    refresh(&mut world);
    assert!(live(&world, root, 1));
    for state in 0..3 {
        {
            let mut area = world.get_mut::<InfluenceArea>(entity).unwrap();
            area.enabled = state != 0;
            area.paused = state == 1;
            area.backend_components = state == 2;
        }
        refresh(&mut world);
        assert!(!live(&world, root, 1));
    }
    world
        .get_mut::<InfluenceArea>(entity)
        .unwrap()
        .backend_components = false;
    refresh(&mut world);
    assert!(live(&world, root, 1));
    world.despawn(entity);
    refresh(&mut world);
    assert!(!live(&world, root, 1));
    hole(&mut world, root, 1, AreaShape::Square, DVec2::ZERO, 100.0);
    world.get_mut::<Workspaces>(root).unwrap().active = 1;
    refresh(&mut world);
    assert!(world.get::<Layers>(root).unwrap().0[0].cuts.is_empty());
    assert!(!live(&world, root, 2));
}

fn render_assets(world: &mut World) {
    world.init_resource::<Assets<Image>>();
    world.init_resource::<Assets<Mesh>>();
    world.init_resource::<Assets<StandardMaterial>>();
    world.spawn(Window {
        resolution: (800, 600).into(),
        ..default()
    });
}

#[test]
fn hidden_sands_do_not_capture_and_visible_faces_are_clipped_with_their_uvs() {
    let (mut world, root) = fixture(2);
    render_assets(&mut world);
    let opening = hole(&mut world, root, 2, AreaShape::Square, DVec2::ZERO, 100.0);
    let lower = sand(&mut world, root, 1, DVec2::ZERO, 300.0);
    let upper = sand(&mut world, root, 2, DVec2::ZERO, 300.0);
    let hidden: Vec<_> = (0..64)
        .map(|index| {
            sand(
                &mut world,
                root,
                1,
                DVec2::new(200.0 + index as f64, 0.0),
                20.0,
            )
        })
        .collect();
    super::super::presentation::synchronize(&mut world);
    for entity in hidden {
        let surface = world.get::<Surface>(entity).unwrap();
        assert!(!surface.visible);
        assert_eq!(surface.pixels, UVec2::ONE);
        assert!(!world.get::<Camera>(surface.camera).unwrap().is_active);
        assert!(!crate::canvas_item::eligible(&world, root, entity));
    }
    assert!(crate::canvas_item::eligible(&world, root, lower));
    for (entity, expected) in [(lower, 10_000.0), (upper, 80_000.0)] {
        let surface = world.get::<Surface>(entity).unwrap();
        assert!(surface.visible);
        let mesh = world
            .resource::<Assets<Mesh>>()
            .get(&world.get::<Mesh3d>(surface.face).unwrap().0)
            .unwrap();
        let bevy::mesh::VertexAttributeValues::Float32x3(points) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap()
        else {
            panic!("face positions")
        };
        let total: f64 = points
            .chunks_exact(3)
            .map(|points| {
                let [a, b, c] = [points[0], points[1], points[2]].map(Vec3::from_array);
                f64::from((b - a).cross(c - a).length()) * 0.5 * 300.0 * 300.0
            })
            .sum();
        assert!((total - expected).abs() < 0.02, "{total} != {expected}");
        let bevy::mesh::VertexAttributeValues::Float32x2(uvs) =
            mesh.attribute(Mesh::ATTRIBUTE_UV_0).unwrap()
        else {
            panic!("face UVs")
        };
        for (point, uv) in points.iter().zip(uvs) {
            assert!((point[0] + 0.5 - uv[0]).abs() < 1e-6);
            assert!((0.5 - point[1] - uv[1]).abs() < 1e-6);
        }
    }
    let mesh_tick = world
        .get_resource_ref::<Assets<Mesh>>()
        .unwrap()
        .last_changed();
    world.increment_change_tick();
    super::super::presentation::synchronize(&mut world);
    assert_eq!(
        world
            .get_resource_ref::<Assets<Mesh>>()
            .unwrap()
            .last_changed(),
        mesh_tick
    );
    world.get_mut::<InfluenceArea>(opening).unwrap().enabled = false;
    super::super::presentation::synchronize(&mut world);
    assert!(!world.get::<Surface>(lower).unwrap().visible);
    assert_eq!(world.get::<Surface>(lower).unwrap().pixels, UVec2::ONE);
    assert_eq!(world.query::<&Background>().iter(&world).count(), 0);
    world.init_resource::<bevy::input_focus::InputFocus>();
    let focused = world.spawn((Text::new("Below"), ChildOf(lower))).id();
    world
        .resource_mut::<bevy::input_focus::InputFocus>()
        .set(focused, bevy::input_focus::FocusCause::Navigated);
    world
        .run_system_cached(crate::canvas::project_canvas)
        .unwrap();
    assert_eq!(
        world.resource::<bevy::input_focus::InputFocus>().get(),
        None
    );
}

#[test]
fn off_hole_sands_keep_their_workspace_forces_while_visible_through_a_hole() {
    let (mut world, root) = fixture(2);
    let opening = hole(&mut world, root, 2, AreaShape::Square, DVec2::ZERO, 100.0);
    let mut force = InfluenceArea::new(AreaShape::Square, DVec2::ZERO, DVec2::splat(1000.0));
    force.strength = 100.0;
    force.reach.mode = crate::area::ReachMode::Unlimited;
    force.rules.push(crate::area::PropertyRule {
        property: crate::area::Property::Kind,
        value: "plain".into(),
    });
    spawn_area(&mut world, root, 1, force).unwrap();
    let lower = sand(&mut world, root, 1, DVec2::new(200.0, 0.0), 20.0);
    world
        .entity_mut(lower)
        .insert(RecordProperties(serde_json::json!({"kind":"plain"})));
    crate::area_effects::update(&mut world);
    assert!(
        world
            .get::<crate::area::AreaForces>(lower)
            .unwrap()
            .total()
            .x
            < 0.0
    );
    world.get_mut::<InfluenceArea>(opening).unwrap().paused = true;
    crate::area_effects::update(&mut world);
    assert_eq!(
        world.get::<crate::area::AreaForces>(lower).unwrap().total(),
        DVec2::ZERO
    );
}
