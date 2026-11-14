use super::*;

fn node(entity: Entity, blocks: bool) -> Geometry {
    Geometry { entity, output: OutputId(1), bounds: Some(Rectangle::new([0.0; 2], [800.0, 600.0]).unwrap()), image: Handle::<Image>::default().id(),
        lineage: vec![entity], actions: Vec::new(), owner: None, modal: None, eligible: true, blocks,
        node: ComputedNode { size: Vec2::splat(100.0), inverse_scale_factor: 1.0, ..default() },
        transform: UiGlobalTransform::from(bevy::math::Affine2::from_translation(Vec2::splat(50.0))), clip: None,
        projection: Projection::Final(RasterMapping::default()) }
}

#[test]
fn stacked_hits_capture_every_nonblocking_recipient_and_refuse_disabled_or_overflowed_targets() {
    let mut world = World::new();
    let first = world.spawn_empty().id();
    let second = world.spawn_empty().id();
    let mut snapshot = Snapshot { targets: vec![node(first, false), node(second, true)], ..default() };
    let captured = snapshot.target([50.0; 2]).unwrap();
    assert_eq!(captured.geometry.len(), 2);
    snapshot.targets[1].actions.push((second, first, 7));
    assert!(snapshot.target([50.0; 2]).as_ref() != Some(&captured));
    snapshot.targets[1].eligible = false;
    assert!(snapshot.target([50.0; 2]).is_none());
    snapshot.targets[0].blocks = true;
    assert_eq!(snapshot.target([50.0; 2]).unwrap().geometry.len(), 1);
    snapshot.overflowed = true;
    assert!(snapshot.target([50.0; 2]).is_none());
}

#[test]
fn held_guards_allow_geometry_motion_but_retire_replaced_actions_and_terminate_cancelled_buttons_once() {
    use bevy::input::mouse::MouseButton;
    let mut world = World::new();
    let entity = world.spawn_empty().id();
    let original = node(entity, true);
    let mut moved = original.clone();
    moved.transform = UiGlobalTransform::from(bevy::math::Affine2::from_translation(Vec2::splat(400.0)));
    moved.clip = Some(Rect::from_corners(Vec2::ZERO, Vec2::splat(800.0)));
    assert!(original.same_owner(&moved));
    moved.actions.push((entity, entity, 1));
    assert!(!original.same_owner(&moved));
    let target = Snapshot { targets: vec![original], ..default() }.target([50.0; 2]).unwrap();
    let mut held = Held::default();
    assert!(held.press(MouseButton::Left, target.clone()));
    assert!(!held.press(MouseButton::Left, target.clone()));
    assert!(held.press(MouseButton::Right, target.clone()));
    assert!(held.release(MouseButton::Left).unwrap().is_some());
    held.cancel();
    assert!(!held.press(MouseButton::Left, target.clone()));
    assert!(held.release(MouseButton::Right).is_err());
    assert!(held.release(MouseButton::Right).unwrap().is_none());
    assert!(held.press(MouseButton::Left, target));
}

#[test]
fn queued_hits_survive_clip_changes_only_while_the_original_recipient_and_mapping_stay_current() {
    let mut world = World::new();
    let entity = world.spawn_empty().id();
    let behind = world.spawn_empty().id();
    let mut snapshot = Snapshot { targets: vec![node(entity, true), node(behind, true)], ..default() };
    let original = snapshot.target([50.0; 2]).unwrap();
    snapshot.targets[0].clip = Some(Rect::from_corners(Vec2::splat(40.0), Vec2::splat(60.0)));
    assert!(original.same_hit(&snapshot.target([50.0; 2]).unwrap()));
    snapshot.targets[0].clip = Some(Rect::from_corners(Vec2::ZERO, Vec2::splat(40.0)));
    assert!(!original.same_hit(&snapshot.target([50.0; 2]).unwrap()));
    snapshot.targets[0].clip = Some(Rect::from_corners(Vec2::splat(40.0), Vec2::splat(60.0)));
    snapshot.targets[0].actions.push((entity, entity, 1));
    assert!(!original.same_hit(&snapshot.target([50.0; 2]).unwrap()));
    snapshot.targets[0].actions.clear();
    snapshot.targets[0].transform = UiGlobalTransform::from(bevy::math::Affine2::from_translation(Vec2::splat(51.0)));
    assert!(!original.same_hit(&snapshot.target([50.0; 2]).unwrap()));
    snapshot.targets[0].transform = original.geometry[0].transform;
    snapshot.targets[0].eligible = false;
    assert!(snapshot.target([50.0; 2]).is_none());
    snapshot.targets[0].eligible = true;
    snapshot.targets[0].blocks = false;
    assert!(!original.same_hit(&snapshot.target([50.0; 2]).unwrap()));
}

#[test]
fn rounded_clipped_rotated_scaled_and_projected_node_hits_keep_their_real_source_geometry() {
    let entity = World::new().spawn_empty().id();
    let mut target = node(entity, true);
    target.projection = Projection::Final(RasterMapping { origin: [1000.0, 50.0], scale: [2.0; 2] });
    target.bounds = Some(Rectangle::new([1000.0, 50.0], [400.0, 300.0]).unwrap());
    assert!(target.hit([1025.0, 75.0]));
    assert!(!target.hit([25.0, 25.0]));
    target.clip = Some(Rect::from_corners(Vec2::new(40.0, 0.0), Vec2::splat(100.0)));
    assert!(!target.hit([1005.0, 75.0]));
    target.transform = UiGlobalTransform::from(bevy::math::Affine2::from_scale_angle_translation(Vec2::ONE, std::f32::consts::FRAC_PI_4, Vec2::splat(50.0)));
    assert!(target.hit([1025.0, 75.0]));
    assert!(!target.hit([1001.0, 51.0]));
    target.transform = UiGlobalTransform::from(bevy::math::Affine2::from_translation(Vec2::splat(50.0)));
    target.clip = None;
    target.projection = Projection::Sand { owner: entity, pixels: UVec2::splat(200), item: ItemGeometry {
        id: ItemId::Native([1; 16]), output: OutputId(1), floating: true, clip: None,
        mapping: lince_interface::host::ContentMapping { canvas: Rectangle::new([1000.0, 50.0], [400.0, 300.0]).unwrap(),
            surface: Rectangle::new([0.0; 2], [200.0; 2]).unwrap(), corners: None },
    } };
    assert!(target.hit([1100.0, 125.0]));
    assert!(!target.hit([1300.0, 275.0]));
    assert!(!target.hit([f64::NAN, 0.0]));
}

#[test]
fn clipped_snapshot_rejection_preserves_possible_hits_under_transform_and_clip_changes() {
    let clip = Rect::from_corners(Vec2::ZERO, Vec2::splat(100.0));
    let shape = ComputedNode { size: Vec2::splat(60.0), inverse_scale_factor: 1.0, ..default() };
    for scale in [Vec2::ONE, Vec2::splat(0.01), Vec2::new(-2.0, 0.5)] {
        for angle in [0.0, std::f32::consts::FRAC_PI_4] {
            for center in [Vec2::splat(50.0), Vec2::new(129.99999, 50.0), Vec2::new(500.0, 50.0)] {
                let transform = UiGlobalTransform::from(bevy::math::Affine2::from_scale_angle_translation(scale, angle, center));
                let rejected = clipped_out(&shape, transform, clip);
                if angle == 0.0 && center.x == 500.0 { assert!(rejected); }
                for x in 0..=40 {
                    for y in 0..=40 {
                        let point = Vec2::new(x as f32, y as f32) * 2.5;
                        if shape.contains_point(transform, point) && clip.contains(point) {
                            assert!(!rejected, "a clipped snapshot lost a possible recipient");
                        }
                    }
                }
            }
        }
    }
    let transform = UiGlobalTransform::from_translation(Vec2::splat(500.0));
    assert!(clipped_out(&shape, transform, clip));
    assert!(!clipped_out(&shape, transform, Rect::from_corners(Vec2::splat(450.0), Vec2::splat(550.0))));
    assert!(!clipped_out(&shape, UiGlobalTransform::from_translation(Vec2::splat(f32::NAN)), clip));
    assert!(!clipped_out(&shape, transform, Rect { min: Vec2::splat(f32::NEG_INFINITY), max: Vec2::splat(f32::INFINITY) }));
}
