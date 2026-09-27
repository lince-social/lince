use super::*;
use crate::{actions::Action, area::AreaShape, canvas::CanvasView, workspace::WorkspaceMember};
use bevy::{math::DQuat, mesh::VertexAttributeValues};

fn positions(mesh: &Mesh) -> &[[f32; 3]] {
    let Some(VertexAttributeValues::Float32x3(points)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        panic!("missing positions")
    };
    points
}

#[test]
fn finite_preview_matches_force_reach_for_regular_and_concave_shapes() {
    let drawn = InfluenceArea::drawn(&[
        DVec2::ZERO,
        DVec2::new(200.0, 0.0),
        DVec2::new(200.0, 60.0),
        DVec2::new(60.0, 60.0),
        DVec2::new(60.0, 200.0),
        DVec2::new(0.0, 200.0),
    ])
    .unwrap();
    for mut area in [
        InfluenceArea::new(AreaShape::Square, DVec2::ZERO, DVec2::splat(200.0)),
        InfluenceArea::new(AreaShape::Circle, DVec2::ZERO, DVec2::splat(200.0)),
        drawn,
    ] {
        area.center = [0.0; 2];
        for shape in [ReachShape::FollowShape, ReachShape::Square] {
            area.reach.shape = shape;
            for radius in [0.0, 25.0, 300.0] {
                area.reach.radius = radius;
                let (fill, border) = meshes(&area);
                assert!(!positions(&fill).is_empty());
                assert!(!positions(&border).is_empty());
                assert!(positions(&fill).len() < 100_000);
                for triangle in positions(&fill).chunks_exact(3) {
                    let middle = triangle.iter().map(|p| Vec3::from_array(*p)).sum::<Vec3>() / 3.0;
                    assert!(area.reaches(DVec2::new(f64::from(middle.x), f64::from(middle.z))));
                }
                for point in positions(&border) {
                    let point = DVec2::new(f64::from(point[0]), f64::from(point[2]));
                    let distance = if shape == ReachShape::Square {
                        point.abs().max_element() - area.size[0].max(area.size[1]) * 0.5
                    } else {
                        area.signed_distance(point)
                    };
                    assert!((distance - radius).abs() < 1.0, "{distance} != {radius}");
                }
            }
        }
    }
}

fn fixture() -> (App, Entity, Entity) {
    let mut app = App::new();
    crate::laboratory::isolate(app.world_mut());
    app.init_resource::<Assets<Font>>()
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<StandardMaterial>>()
        .init_resource::<crate::theme::Typography>()
        .init_resource::<bevy::input_focus::InputFocus>()
        .add_plugins((
            crate::workspace::WorkspacePlugin,
            crate::edit_mode::EditModePlugin,
        ));
    let root = app.world_mut().spawn(crate::container::BoxRoot).id();
    app.update();
    crate::edit_mode::EditAction::Open.apply(app.world_mut(), root);
    let area = InfluenceArea::new(
        AreaShape::Square,
        DVec2::new(40.0, 80.0),
        DVec2::splat(200.0),
    );
    let entity = crate::area::spawn_area(app.world_mut(), root, 1, area).unwrap();
    crate::edit_mode::EditAction::Area(crate::area_panel::AreaAction::Select(entity))
        .apply(app.world_mut(), root);
    (app, root, entity)
}

#[test]
fn selection_shows_cached_nonblocking_veil_and_tracks_world_placement() {
    let (mut app, root, entity) = fixture();
    let world = app.world_mut();
    update(world);
    let visuals = world.get::<Preview>(root).unwrap().visuals.clone();
    assert_eq!(visuals.len(), 2);
    for visual in &visuals {
        assert_eq!(world.get::<Pickable>(*visual), Some(&Pickable::IGNORE));
        assert_eq!(
            world
                .get::<super::super::presentation::VisualOwner>(*visual)
                .unwrap()
                .0,
            entity
        );
    }
    let material = &world
        .get::<MeshMaterial3d<StandardMaterial>>(visuals[0])
        .unwrap()
        .0;
    let material = world
        .resource::<Assets<StandardMaterial>>()
        .get(material)
        .unwrap();
    assert_eq!(material.alpha_mode, AlphaMode::Blend);
    assert_eq!(material.base_color.alpha(), 0.10);
    let placement = super::super::Spatial {
        elevation: 70.0,
        rotation: DQuat::from_rotation_x(0.7).to_array(),
        ..default()
    };
    world.entity_mut(entity).insert(placement);
    world.get_mut::<InfluenceArea>(entity).unwrap().center = [400.0, -80.0];
    world.get_mut::<CanvasView>(root).unwrap().center = DVec2::new(20.0, 30.0);
    update(world);
    assert_eq!(world.get::<Preview>(root).unwrap().visuals, visuals);
    let transform = world.get::<Transform>(visuals[0]).unwrap();
    assert_eq!(transform.translation, Vec3::new(380.0, 70.0, -110.0));
    assert_eq!(transform.rotation, placement.rotation().as_quat());
    world.get_mut::<InfluenceArea>(entity).unwrap().target =
        crate::area::AttractionTarget::Point([200.0, 300.0]);
    update(world);
    assert_eq!(world.get::<Preview>(root).unwrap().visuals, visuals);
    world.get_mut::<InfluenceArea>(entity).unwrap().reach.radius = 25.0;
    update(world);
    assert!(
        visuals
            .iter()
            .all(|entity| world.get_entity(*entity).is_err())
    );
    let visuals = world.get::<Preview>(root).unwrap().visuals.clone();
    crate::inspection::Deselect.apply(world, root);
    update(world);
    assert!(world.get::<Preview>(root).is_none());
    assert!(
        visuals
            .iter()
            .all(|entity| world.get_entity(*entity).is_err())
    );
}

#[test]
fn unlimited_replaces_veil_with_label_and_preview_clears_on_mode_workspace_and_removal() {
    let (mut app, root, entity) = fixture();
    let world = app.world_mut();
    update(world);
    let finite = world.get::<Preview>(root).unwrap().visuals.clone();
    world.get_mut::<InfluenceArea>(entity).unwrap().reach.mode = ReachMode::Unlimited;
    update(world);
    assert!(
        finite
            .iter()
            .all(|entity| world.get_entity(*entity).is_err())
    );
    let unlimited = world.get::<Preview>(root).unwrap().visuals[0];
    assert_eq!(
        world.get::<Text>(unlimited).unwrap().0,
        "Force range · Unlimited"
    );
    assert!(world.get::<Mesh3d>(unlimited).is_none());
    world.get_mut::<EditMode>(root).unwrap().enabled = false;
    update(world);
    assert!(world.get_entity(unlimited).is_err());
    world.get_mut::<EditMode>(root).unwrap().enabled = true;
    world.get_mut::<InfluenceArea>(entity).unwrap().reach.mode = ReachMode::Limited;
    update(world);
    assert_eq!(world.get::<Preview>(root).unwrap().visuals.len(), 2);
    world.get_mut::<WorkspaceMember>(entity).unwrap().0 = 2;
    update(world);
    assert!(world.get::<Preview>(root).is_none());
    world.get_mut::<WorkspaceMember>(entity).unwrap().0 = 1;
    update(world);
    let visuals = world.get::<Preview>(root).unwrap().visuals.clone();
    world.despawn(entity);
    update(world);
    assert!(world.get::<Preview>(root).is_none());
    assert!(
        visuals
            .iter()
            .all(|entity| world.get_entity(*entity).is_err())
    );
}
