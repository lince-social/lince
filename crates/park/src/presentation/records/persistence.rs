use super::*;
use std::sync::mpsc;

fn world() -> World {
    let mut world = World::new();
    world.init_resource::<Messages<native::cell_bridge::CellMessage>>();
    world.insert_resource(WakeSignal::new(|| {}));
    world
}

fn profile(records: &mut Records, creating: bool) -> (PlacementId, String) {
    let layout = native::layout::LayoutBox::new(Vec2::splat(300.0));
    let identity = PlacementId(layout.id);
    let uid = nucleus::new_uid("r");
    records.saved.insert(
        identity,
        Saved {
            uid: (!creating).then(|| uid.clone()),
            creation: creating.then(|| uid.clone()),
            layout,
            workspace: 1,
            position: [0.0; 2],
            size: [300.0; 2],
            tokens: Default::default(),
            launch: None,
        },
    );
    if creating {
        records.creations.insert(identity);
    }
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

fn created(message: cell::ClientMessage) -> String {
    let cell::ClientMessage::Act {
        action: engine::actions::Action::CreateRecordDraft { draft },
        ..
    } = message
    else {
        panic!("Expected a window Record creation");
    };
    draft.uid
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
fn creation_and_failed_saves_wake_without_further_input() {
    for failing in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("profiles.json");
        let mut records = Records::load(path.clone()).unwrap();
        profile(&mut records, !failing);
        let target = path.clone();
        records.writer = Some(
            storage::Writer::with_writer(move |profiles| {
                if failing {
                    return Err(io::Error::other("Controlled write failure"));
                }
                storage::save(&target, profiles)
            })
            .unwrap(),
        );
        let (awake, woken) = mpsc::sync_channel(1);
        records
            .start_saving(Some(WakeSignal::new(move || {
                let _ = awake.try_send(());
            })))
            .unwrap();
        woken.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(records.finish_saving().is_err(), failing);
        assert_eq!(path.exists(), !failing);
    }
}

#[test]
fn creation_uses_only_durable_current_uids_and_keeps_progress_when_new_profiles_arrive() {
    use native::full_record::window::CreationState;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("profiles.json");
    let mut records = Records::load(path.clone()).unwrap();
    let (first, first_uid) = profile(&mut records, true);
    let (sender, mut receiver) = tokio::sync::mpsc::channel(8);
    records.sender = Some(sender.clone());
    let (writer, started, release) = gated(path.clone());
    records.writer = Some(writer);
    let world = world();
    assert!(!records.create_pending(&world, Some(&sender)));
    started.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(receiver.try_recv().is_err());
    assert!(matches!(
        records.creation_state(first.0, &first_uid),
        Some(CreationState::Pending)
    ));
    let (second, old_uid) = profile(&mut records, true);
    assert!(!records.create_pending(&world, Some(&sender)));
    let version = records.saving.as_ref().unwrap().version;
    assert!(
        records
            .saved(storage::Finished {
                version: version - 1,
                result: Ok(())
            })
            .is_err()
    );
    assert_eq!(records.saving.as_ref().unwrap().version, version);
    assert!(records.durable_intents.is_empty());
    release.send(()).unwrap();
    records.finish_saving().unwrap();
    assert!(!records.create_pending(&world, Some(&sender)));
    started.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(created(receiver.try_recv().unwrap()), first_uid);
    assert!(receiver.try_recv().is_err());
    let replacement = nucleus::new_uid("r");
    records.saved.get_mut(&second).unwrap().creation = Some(replacement.clone());
    records.dirty = true;
    release.send(()).unwrap();
    records.finish_saving().unwrap();
    assert!(!records.durable_intents.contains_key(&second));
    assert!(!records.create_pending(&world, Some(&sender)));
    started.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(receiver.try_recv().is_err());
    assert_ne!(old_uid, replacement);
    release.send(()).unwrap();
    records.finish_saving().unwrap();
    assert!(!records.create_pending(&world, Some(&sender)));
    assert_eq!(created(receiver.try_recv().unwrap()), replacement);
    assert!(receiver.try_recv().is_err());
    assert_eq!(records.requests.len(), 2);
    let restored = Records::load(path).unwrap();
    assert_eq!(
        restored.profile(first).unwrap().creation.as_ref(),
        Some(&first_uid)
    );
    assert_eq!(
        restored.profile(second).unwrap().creation.as_ref(),
        Some(&replacement)
    );
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
