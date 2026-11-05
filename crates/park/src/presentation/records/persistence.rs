use super::*;
use std::sync::mpsc;

fn world() -> World {
    let mut world = World::new();
    world.init_resource::<Messages<native::cell_bridge::CellMessage>>();
    world.insert_resource(WakeSignal::new(|| {}));
    world
}

fn profile(records: &mut Records, _: bool) -> (PlacementId, String) {
    let layout = native::layout::LayoutBox::new(Vec2::splat(300.0));
    let identity = PlacementId(layout.id);
    let uid = String::new();
    records.saved.insert(
        identity,
        Saved {
            layout,
            workspace: 1,
            position: [0.0; 2],
            size: [300.0; 2],
            tokens: Default::default(),
            launch: None,
        },
    );
    records.dirty = true;
    (identity, uid)
}

fn gated(path: PathBuf) -> (storage::Writer, mpsc::Receiver<()>, mpsc::SyncSender<()>) {
    let (entered, started) = mpsc::sync_channel(1);
    let (release, resumed) = mpsc::sync_channel(1);
    let writer = storage::Writer::with_writer(move |profiles| {
        entered.send(()).map_err(io::Error::other)?;
        resumed
            .recv_timeout(Duration::from_secs(5))
            .map_err(io::Error::other)?;
        storage::save(&path, profiles)
    })
    .unwrap();
    (writer, started, release)
}

#[test]
fn blocked_saves_keep_ticks_available_and_coalesce_mutations_into_the_next_snapshot() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("profiles.json");
    let mut records = Records::load(path.clone()).unwrap();
    let (identity, _) = profile(&mut records, false);
    let (writer, started, release) = gated(path.clone());
    records.writer = Some(writer);
    let (awake, woken) = mpsc::sync_channel(1);
    records
        .start_saving(Some(WakeSignal::new(move || {
            let _ = awake.try_send(());
        })))
        .unwrap();
    started.recv_timeout(Duration::from_secs(2)).unwrap();
    let mut world = world();
    for position in 1..=100 {
        records.saved.get_mut(&identity).unwrap().position[0] = f64::from(position);
        records.dirty = true;
        records.tick(&mut world, &HashMap::new(), &HashMap::new());
        assert_eq!(records.save_version, 1);
        assert!(records.saving.is_some());
    }
    assert!(!path.exists());
    release.send(()).unwrap();
    woken.recv_timeout(Duration::from_secs(2)).unwrap();
    records.finish_saving().unwrap();
    assert!(records.dirty);
    assert_eq!(
        Records::load(path.clone())
            .unwrap()
            .profile(identity)
            .unwrap()
            .position[0],
        0.0
    );
    records.start_saving(None).unwrap();
    started.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(records.save_version, 2);
    release.send(()).unwrap();
    records.flush().unwrap();
    assert!(!records.dirty);
    assert_eq!(
        Records::load(path)
            .unwrap()
            .profile(identity)
            .unwrap()
            .position[0],
        100.0
    );
    assert!(started.try_recv().is_err());
}

#[test]
fn failed_saves_wake_without_further_input() {
    let directory = tempfile::tempdir().unwrap();
    let mut records = Records::load(directory.path().join("profiles.json")).unwrap();
    profile(&mut records, false);
    records.writer = Some(storage::Writer::with_writer(|_| Err(io::Error::other("Controlled write failure"))).unwrap());
    let (awake, woken) = mpsc::sync_channel(1);
    records.start_saving(Some(WakeSignal::new(move || { let _ = awake.try_send(()); }))).unwrap();
    woken.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(records.finish_saving().is_err());
}

#[test]
fn failed_background_save_preserves_the_old_file_and_retries_the_current_state() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("profiles.json");
    let mut records = Records::load(path.clone()).unwrap();
    let (identity, _) = profile(&mut records, false);
    records.flush().unwrap();
    let previous = std::fs::read(&path).unwrap();
    let target = path.clone();
    let mut fail = true;
    records.writer = Some(
        storage::Writer::with_writer(move |profiles| {
            if std::mem::take(&mut fail) {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "Controlled private write failure",
                ));
            }
            storage::save(&target, profiles)
        })
        .unwrap(),
    );
    records.saved.get_mut(&identity).unwrap().position[0] = 200.0;
    records.dirty = true;
    records.start_saving(None).unwrap();
    assert!(records.finish_saving().is_err());
    assert!(records.dirty && records.saving.is_none());
    assert_eq!(std::fs::read(&path).unwrap(), previous);
    records.flush().unwrap();
    assert_eq!(
        Records::load(path)
            .unwrap()
            .profile(identity)
            .unwrap()
            .position[0],
        200.0
    );
    assert!(!records.dirty);
}

#[test]
fn privacy_acknowledgement_waits_for_older_and_current_durable_writes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("profiles.json");
    let mut records = Records::load(path.clone()).unwrap();
    let (identity, uid) = profile(&mut records, false);
    records.flush().unwrap();
    let (writer, started, release) = gated(path.clone());
    records.writer = Some(writer);
    records.saved.get_mut(&identity).unwrap().position[0] = 200.0;
    records.dirty = true;
    records.start_saving(None).unwrap();
    started.recv_timeout(Duration::from_secs(2)).unwrap();
    let (entered, waiting) = mpsc::sync_channel(1);
    let (done, received) = mpsc::sync_channel(1);
    let task = std::thread::spawn(move || {
        entered.send(()).unwrap();
        let result = records.capture_exclusion(identity, &uid, true);
        done.send((result, records)).ok().unwrap();
    });
    waiting.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(received.recv_timeout(Duration::from_millis(20)).is_err());
    release.send(()).unwrap();
    started.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(
        !Records::load(path.clone())
            .unwrap()
            .profile(identity)
            .unwrap()
            .layout
            .capture_excluded
    );
    assert!(received.try_recv().is_err());
    release.send(()).unwrap();
    let (result, records) = received.recv_timeout(Duration::from_secs(2)).unwrap();
    result.unwrap();
    task.join().unwrap();
    assert!(records.profile(identity).unwrap().layout.capture_excluded);
    assert!(
        Records::load(path)
            .unwrap()
            .profile(identity)
            .unwrap()
            .layout
            .capture_excluded
    );
}
