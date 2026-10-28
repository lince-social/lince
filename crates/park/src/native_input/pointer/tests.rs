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
