use super::*;
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

#[cfg_attr(test, test)]
fn empty_clock_cadence_bounds_hand_motion_and_preserves_fast_clocks() {
    for timezone in ["UTC", "America/Sao_Paulo", "America/New_York"] {
        for aperture_ms in [1_000, 60_000, 720_000, 3_600_000, 86_400_000] {
            let settings = Settings {
                timezone: timezone.into(),
                aperture_ms,
                horizon_ms: aperture_ms,
                ..Settings::default()
            };
            for pixels in [64.0, 128.0, 360.0, 1024.0, 4096.0, 16384.0] {
                let delay = render::hand_delay(aperture_ms, pixels);
                assert!((16..=1000).contains(&delay.as_millis()));
                if delay > Duration::from_millis(16) {
                    for now in [0, 1_742_630_399_997, 1_800_000_000_000] {
                        let before = Vec3::from_array(settings.transverse(now, now, 0.0));
                        let later = now + delay.as_millis() as i64;
                        let after = Vec3::from_array(settings.transverse(later, later, 0.0));
                        assert!(
                            f64::from(before.distance(after)) * pixels * 0.4 <= 0.026,
                            "{timezone} {aperture_ms} {pixels} {now} {delay:?}"
                        );
                    }
                }
            }
        }
    }
    assert_eq!(render::hand_delay(60_000, 360.0), Duration::from_millis(16));
    assert!(render::hand_delay(3_600_000, 360.0) >= Duration::from_millis(90));
    assert!(render::hand_delay(3_600_000, 1440.0) < render::hand_delay(3_600_000, 360.0));
    for pixels in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert_eq!(
            render::hand_delay(3_600_000, pixels),
            Duration::from_millis(16)
        );
    }
}

fn clock(world: &mut World, root: Entity) -> Entity {
    let owner = world
        .spawn((
            Node::default(),
            ChildOf(root),
            TimeSettings(Settings::default()),
        ))
        .id();
    populate(world, owner);
    owner
}

#[cfg_attr(test, test)]
fn clock_updates_coalesce_deadlines_and_retire_removed_views() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();
    runtime.block_on(async {
        let mut world = World::new();
        world.insert_resource(crate::native::theme::Typography(Handle::default()));
        world.init_resource::<bevy::input_focus::InputFocus>();
        let root = world.spawn(Node::default()).id();
        let owner = clock(&mut world, root);
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = calls.clone();
        world.insert_resource(crate::host::WakeSignal::new(move || {
            observed.fetch_add(1, Ordering::Relaxed);
        }));
        for _ in 0..32 {
            world.run_system_cached(render::update).unwrap();
        }
        let other_calls = Arc::new(AtomicUsize::new(0));
        let observed = other_calls.clone();
        let mut independent = crate::host::WakeDeadline::default();
        independent.after(
            &crate::host::WakeSignal::new(move || {
                observed.fetch_add(1, Ordering::Relaxed);
            }),
            Duration::from_millis(210),
        );
        tokio::time::sleep(Duration::from_millis(140)).await;
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        assert_eq!(other_calls.load(Ordering::Relaxed), 0);
        world.run_system_cached(render::update).unwrap();
        world.despawn(owner);
        world.run_system_cached(render::update).unwrap();
        tokio::time::sleep(Duration::from_millis(140)).await;
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        assert_eq!(other_calls.load(Ordering::Relaxed), 1);
        let owner = clock(&mut world, root);
        world.run_system_cached(render::update).unwrap();
        let previous_calls = calls.load(Ordering::Relaxed);
        let replaced = Arc::new(AtomicUsize::new(0));
        let observed = replaced.clone();
        world.insert_resource(crate::host::WakeSignal::new(move || {
            observed.fetch_add(1, Ordering::Relaxed);
        }));
        world.run_system_cached(render::update).unwrap();
        tokio::time::sleep(Duration::from_millis(140)).await;
        assert_eq!(calls.load(Ordering::Relaxed), previous_calls);
        assert_eq!(replaced.load(Ordering::Relaxed), 1);
        world
            .get_mut::<TimeSettings>(owner)
            .unwrap()
            .0
            .set_aperture(60_000);
        for _ in 0..32 {
            world.run_system_cached(render::update).unwrap();
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
        assert_eq!(replaced.load(Ordering::Relaxed), 2);
        world.get_mut::<View>(owner).unwrap().animation_at =
            std::time::Instant::now() - Duration::from_secs(1);
        world.get_mut::<TimeSettings>(owner).unwrap().0.mode = Mode::Straight;
        world.run_system_cached(render::update).unwrap();
        assert!(world.get::<View>(owner).unwrap().unwind < 0.001);
        world.get_mut::<View>(owner).unwrap().animation_at =
            std::time::Instant::now() - Duration::from_millis(16);
        world.run_system_cached(render::update).unwrap();
        assert!((0.04..0.1).contains(&world.get::<View>(owner).unwrap().unwind));
        world.run_system_cached(render::update).unwrap();
        let mut laboratory = crate::native::laboratory::Laboratory::default();
        laboratory.root = Some(root);
        world.insert_resource(laboratory);
        world.run_system_cached(render::update).unwrap();
        let paused_calls = replaced.load(Ordering::Relaxed);
        tokio::time::sleep(Duration::from_millis(40)).await;
        assert_eq!(replaced.load(Ordering::Relaxed), paused_calls);
    });
}

crate::native::laboratory_cases! {
    empty_clock_cadence_bounds_hand_motion_and_preserves_fast_clocks,
    clock_updates_coalesce_deadlines_and_retire_removed_views,
}
