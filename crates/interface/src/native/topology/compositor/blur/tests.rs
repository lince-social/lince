use super::*;

#[test]
fn backdrop_readiness_is_bound_to_the_current_target_viewport_and_request_lifetime() {
    let mut world = World::new();
    let camera = world.spawn_empty().id();
    let mut images = Assets::<Image>::default();
    let first = images.add(Image::default());
    let second = images.add(Image::default());
    let stamp = Stamp {
        size: UVec2::new(800, 600), viewport: UVec4::new(0, 0, 800, 600),
        target: Some(NormalizedRenderTarget::Image(first.into())), radius: 8.0_f32.to_bits(), bytes: 8_000_000,
    };
    let runtime = Runtime::default();
    runtime.record(camera, stamp.clone(), Outcome::Ready, None);
    assert!(runtime.0.lock().unwrap().outcomes.is_empty());
    runtime.0.lock().unwrap().requested.insert(camera, stamp.clone());
    runtime.record(camera, stamp.clone(), Outcome::Ready, None);
    assert!(runtime.0.lock().unwrap().outcomes.get(&camera).is_some_and(|(current, outcome)| current == &stamp && *outcome == Outcome::Ready));
    let mut replacement = stamp.clone();
    replacement.target = Some(NormalizedRenderTarget::Image(second.into()));
    replacement.viewport.x = 20;
    replacement.viewport.z = 780;
    runtime.0.lock().unwrap().requested.insert(camera, replacement.clone());
    runtime.record(camera, replacement.clone(), Outcome::Waiting, None);
    runtime.record(camera, stamp, Outcome::Ready, None);
    assert!(runtime.0.lock().unwrap().outcomes.get(&camera).is_some_and(|(current, outcome)| current == &replacement && *outcome == Outcome::Waiting));
    runtime.0.lock().unwrap().requested.clear();
    runtime.0.lock().unwrap().outcomes.clear();
    runtime.record(camera, replacement, Outcome::Ready, None);
    assert!(runtime.0.lock().unwrap().outcomes.is_empty());
}
