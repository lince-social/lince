use super::*;
use nucleus::karma::DecimalValue;
use std::{process::Command, time::Instant};

fn screw() -> Scene {
    let mut scene = Scene::default();
    let first = scene.add_part("Screw").unwrap();
    let second = scene.place_again(&first).unwrap();
    scene
        .edit(
            &second,
            [DecimalValue::from_integer(0); 3],
            Some(nucleus::id::new_uid("r")),
        )
        .unwrap();
    scene
}

#[test]
fn stable_handles_reuse_one_definition_and_detect_conflicting_writes() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().join("sample.workspace");
    let mut scene = screw();
    let initial = save(&dir, &scene, None).unwrap();
    let stale = open(&dir).unwrap();
    let unchanged = scene.slots[0].clone();
    let moved = scene.handle(&scene.slots[1]);
    let coordinates = [
        DecimalValue::parse_inferred("0.000000042").unwrap(),
        DecimalValue::from_integer(-23),
        DecimalValue::from_integer(1),
    ];
    let record = scene.slots[1].record.clone();
    scene.edit(&moved, coordinates, record.clone()).unwrap();
    Arc::make_mut(&mut scene.slots).reverse();
    let start = Instant::now();
    let revision = save(&dir, &scene, Some(&initial)).unwrap();
    let commit = start.elapsed();
    let start = Instant::now();
    let opened = open(&dir).unwrap();
    let load = start.elapsed();
    assert_eq!(opened.scene, scene);
    assert_eq!(opened.scene.slots[1], unchanged);
    assert_eq!(opened.scene.slot(&moved).unwrap().metres, coordinates);
    assert_eq!(opened.scene.slot(&moved).unwrap().record, record);
    assert!(Arc::ptr_eq(&scene.parts, &scene.clone().parts));
    assert_ne!(initial, revision);
    assert_eq!(
        save(&dir, &stale.scene, Some(&stale.revision))
            .unwrap_err()
            .kind(),
        io::ErrorKind::AlreadyExists
    );
    assert_eq!(save(&dir, &scene, Some(&revision)).unwrap(), revision);
    assert_eq!(open(&dir).unwrap().scene, scene);
    let sizes: Vec<u64> = fs::read_dir(dir.join("pages"))
        .unwrap()
        .map(|p| p.unwrap().metadata().unwrap().len())
        .collect();
    assert_eq!(
        sizes.len(),
        5,
        "one part page, two slot pages and two manifests"
    );
    eprintln!(
        "two-slot save {commit:?}, open {load:?}, immutable page sizes {sizes:?}, snapshot descriptor {} bytes",
        std::mem::size_of::<Scene>()
    );
}

#[test]
fn interrupted_writer_preserves_the_last_complete_root() {
    const CHILD: &str = "LINCE_SCENE_TEST_ABORT_DIRECTORY";
    if let Some(dir) = std::env::var_os(CHILD) {
        let dir = PathBuf::from(dir);
        let mut old = open(&dir).unwrap();
        old.scene.add_part("Interrupted").unwrap();
        publish(&dir, &old.scene, Some(&old.revision), || {
            std::process::exit(74)
        })
        .unwrap();
        unreachable!();
    }
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    let mut scene = screw();
    scene.canvas = Some(CanvasBinding {
        handle: scene.handle(&scene.slots[1]),
        room: nucleus::new_uid("placement"),
        placement: nucleus::new_uid("placement"),
    });
    let before = save(dir, &scene, None).unwrap();
    let result = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "workspace_scene::tests::interrupted_writer_preserves_the_last_complete_root",
        ])
        .env(CHILD, dir)
        .status()
        .unwrap();
    assert_eq!(result.code(), Some(74));
    assert!(fs::read_dir(dir).unwrap().any(|p| {
        p.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".stage-")
    }));
    let recovered = open(dir).unwrap();
    assert_eq!(recovered.scene, scene);
    assert_eq!(recovered.revision, before);
    let mut next = scene;
    next.add_part("Recovered").unwrap();
    save(dir, &next, Some(&before)).unwrap();
    assert_eq!(open(dir).unwrap().scene, next);
}

#[test]
fn incomplete_corrupt_and_unsupported_scenes_fail_closed() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    let mut scene = screw();
    let rev = save(dir, &scene, None).unwrap();
    let mut manifest: Manifest = page(dir, ROOT, &rev).unwrap();
    let publish_manifest = |m: &Manifest| {
        let hash = write_page(dir, ROOT, m).unwrap();
        fs::write(dir.join("HEAD"), canonical_json_bytes(&hash).unwrap()).unwrap();
    };
    manifest.required_profiles.push("unknown/1".into());
    publish_manifest(&manifest);
    assert!(open(dir).unwrap_err().to_string().contains("Unsupported"));
    manifest.required_profiles.pop();
    Arc::make_mut(&mut scene.slots)[0].id = scene.slots[1].id.clone();
    assert!(scene.validate().is_err());
    Arc::make_mut(&mut scene.parts)[0].name = "Changed pinned definition".into();
    manifest.definitions = write_page(dir, PARTS, &*scene.parts).unwrap();
    publish_manifest(&manifest);
    assert!(
        open(dir)
            .unwrap_err()
            .to_string()
            .contains("pinned definition")
    );
    let file = page_path(dir, &manifest.placements);
    fs::write(&file, b"[]").unwrap();
    assert!(
        open(dir)
            .unwrap_err()
            .to_string()
            .contains("digest mismatch")
    );
    fs::write(&file, vec![b' '; LIMIT + 1]).unwrap();
    assert!(open(dir).unwrap_err().to_string().contains("bounded"));
    fs::remove_file(file).unwrap();
    assert_eq!(open(dir).unwrap_err().kind(), io::ErrorKind::NotFound);
}

#[test]
fn writers_from_one_base_publish_only_one_complete_successor() {
    let dir = tempfile::tempdir().unwrap();
    let scene = screw();
    let head = save(dir.path(), &scene, None).unwrap();
    let barrier = std::sync::Barrier::new(2);
    let results = std::thread::scope(|s| {
        let writers = ["A", "B"].map(|name| {
            let mut copy = scene.clone();
            copy.add_part(name).unwrap();
            let (gate, path, head) = (&barrier, dir.path(), &head);
            s.spawn(move || {
                gate.wait();
                save(path, &copy, Some(head)).map(|r| (r, copy))
            })
        });
        writers.map(|w| w.join().unwrap())
    });
    let winners: Vec<_> = results.into_iter().filter_map(Result::ok).collect();
    assert_eq!(winners.len(), 1);
    let accepted = open(dir.path()).unwrap();
    assert_eq!(accepted.revision, winners[0].0);
    assert_eq!(accepted.scene, winners[0].1);
}

#[test]
fn equivalent_decimals_round_trip_and_wrong_root_cannot_move_a_slot() {
    let mut scene = screw();
    let handle = scene.handle(&scene.slots[0]);
    let x = DecimalValue::parse_inferred("0.12500").unwrap();
    scene.edit(&handle, [x; 3], None).unwrap();
    assert_eq!(scene.slot(&handle).unwrap().metres[0].to_string(), "0.125");
    let other = Scene::default();
    let wrong = nucleus::canvas::scene::Handle {
        root: other.root,
        slot: handle.slot,
    };
    let before = scene.clone();
    assert!(
        scene
            .edit(&wrong, [DecimalValue::from_integer(9); 3], None)
            .is_err()
    );
    assert_eq!(before, scene);
}

#[test]
#[ignore = "run after compilation in an isolated local storage measurement window"]
fn measure_small_scene_commits_and_verified_opens() {
    let directory = tempfile::tempdir().unwrap();
    let mut scene = screw();
    let slot = scene.handle(&scene.slots[0]);
    let mut head = save(directory.path(), &scene, None).unwrap();
    let mut durations = Vec::new();
    for i in 1..=40 {
        scene
            .edit(&slot, [DecimalValue::from_integer(i); 3], None)
            .unwrap();
        let start = Instant::now();
        head = save(directory.path(), &scene, Some(&head)).unwrap();
        let save_us = start.elapsed().as_micros();
        let start = Instant::now();
        let loaded = open(directory.path()).unwrap();
        durations.push((save_us, start.elapsed().as_micros()));
        assert_eq!(loaded.scene, scene);
    }
    eprintln!(
        "paired save/open microseconds {}",
        serde_json::to_string(&durations).unwrap()
    );
}
