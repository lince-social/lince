use super::*;
use std::time::Instant;

#[test]
#[ignore = "CPU-only warmed source-resolution comparison; no concurrent compilation"]
fn source_snapshots_compare_sparse_dense_and_repeated_contributors() {
    let mut world = World::new();
    let root = world.spawn(Workspaces::default()).id();
    let mut requested = Vec::new();
    for _ in 0..100 {
        let owner = world.spawn((LayoutBox::new(Vec2::ONE), ChildOf(root))).id();
        requested.push(owner);
        for _ in 0..4 {
            let mut parent = owner;
            for _ in 0..6 {
                parent = world.spawn(ChildOf(parent)).id();
                requested.push(parent);
            }
        }
    }
    let mut state = SystemState::<Sources>::new(&mut world);
    let mut samples = Vec::new();
    for coverage in [1, 10, 100, 200] {
        let count = requested.len() * coverage / 100;
        for snapshot in [false, true] {
            let mut elapsed = Vec::new();
            for round in 0..24 {
                let sources = state.get(&world).unwrap();
                let started = Instant::now();
                let mut total = 0;
                if snapshot {
                    let captured = sources.snapshot(crate::host::capture::MAX_CONTRIBUTORS + 1);
                    for index in 0..count {
                        total += captured
                            .resolve(requested[index % requested.len()])
                            .unwrap()
                            .iter()
                            .count();
                    }
                } else {
                    let captured: BTreeMap<_, _> = sources
                        .entities()
                        .map(|entity| (entity, sources.compact(entity)))
                        .collect();
                    for index in 0..count {
                        total += captured[&requested[index % requested.len()]].iter().count();
                    }
                }
                let ms = started.elapsed().as_secs_f64() * 1000.0;
                assert_eq!(total, count);
                if round >= 4 {
                    elapsed.push(ms);
                }
            }
            elapsed.sort_by(f64::total_cmp);
            samples.push(serde_json::json!({"contributors_percent":coverage,"requested":count,"snapshot":snapshot,"median_ms":elapsed[elapsed.len()/2],"p95_ms":elapsed[elapsed.len()*95/100]}));
        }
    }
    let result = serde_json::json!({"scope":"CPU-only 100 owner groups with 2400 descendants; four warmups and 20 samples; dense means every item contributes; repeated means two uses of every item; no physical presentation claim","samples":samples});
    if let Some(path) = std::env::var_os("LINCE_SOURCE_TEST_ARTIFACTS") {
        let path = std::path::PathBuf::from(path);
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(
            path.join("source-density.json"),
            serde_json::to_vec_pretty(&result).unwrap(),
        )
        .unwrap();
    }
    eprintln!("source-density: {result}");
}
