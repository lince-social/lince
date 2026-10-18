use lince_map::{
    Camera, Settings, TileStore,
    scene::{Palette, Scene},
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

#[tokio::test]
async fn one_and_four_scenes_release_their_own_workers_across_twenty_close_cycles() {
    let temporary = tempfile::tempdir().unwrap();
    let store = TileStore::new(Settings::default(), temporary.path()).unwrap();
    let palette = Palette {
        land: [40, 30, 20, 255],
        water: [10, 20, 40, 255],
        roads: [200; 4],
        nature: [20, 40, 20, 255],
        ink: [240; 4],
    };
    let survivor =
        Scene::new(store.clone(), Camera::default(), [1280, 720], palette, None).unwrap();
    for views in [1, 4] {
        for _ in 0..20 {
            let mut counters = Vec::new();
            let mut scenes = Vec::new();
            for _ in 0..views {
                let scene =
                    Scene::new(store.clone(), Camera::default(), [1280, 720], palette, None)
                        .unwrap();
                counters.push(Arc::downgrade(&scene.counters));
                scene.load();
                scenes.push(scene);
            }
            tokio::task::yield_now().await;
            for scene in &scenes {
                let state = scene.counters.snapshot();
                assert!(state.alive);
                assert!(state.loads <= 4);
                assert!(state.processors <= 4);
            }
            drop(scenes);
            let started = Instant::now();
            loop {
                let idle = counters.iter().all(|weak| {
                    weak.upgrade().is_none_or(|counters| {
                        let state = counters.snapshot();
                        !state.alive && state.loads == 0 && state.processors == 0
                    })
                });
                if idle {
                    break;
                }
                assert!(started.elapsed() < Duration::from_secs(5));
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            assert!(survivor.counters.snapshot().alive);
            survivor.load();
        }
    }
    let counters = Arc::downgrade(&survivor.counters);
    drop(survivor);
    tokio::task::yield_now().await;
    assert!(
        counters
            .upgrade()
            .is_none_or(|counter| !counter.snapshot().alive)
    );
}
