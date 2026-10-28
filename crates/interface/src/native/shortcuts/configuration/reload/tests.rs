use super::*;
use std::io::Write;

fn fixture(directory: &Path) -> App {
    let mut app = App::new();
    app.insert_resource(crate::native::workspace::WorkspaceFile::new(directory.join("interface.json")))
        .insert_resource(crate::host::WakeSignal::new(|| {}))
        .add_systems(Startup, super::super::load);
    install(&mut app);
    app
}

async fn until(app: &mut App, predicate: impl Fn(&World) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        app.update();
        if predicate(app.world()) { return; }
        assert!(Instant::now() < deadline, "Configuration watcher did not reach the expected state: {}", app.world().resource::<Reload>().status);
        tokio::time::sleep(Duration::from_millis(3)).await;
    }
}

fn replace(path: &Path, bytes: &[u8]) {
    let mut temporary = tempfile::NamedTempFile::new_in(path.parent().unwrap()).unwrap();
    temporary.write_all(bytes).unwrap();
    temporary.as_file().sync_all().unwrap();
    temporary.persist(path).unwrap();
}

#[test]
fn debounce_does_not_allow_a_continuous_burst_to_postpone_reading_forever() {
    let began = Instant::now();
    let mut state = Reload::default();
    state.first = Some(began);
    for milliseconds in (0..2000).step_by(10) {
        state.last = Some(began + Duration::from_millis(milliseconds));
        let expected = (milliseconds + 150).min(500);
        assert_eq!(deadline(&state), Some(began + Duration::from_millis(expected)));
    }
    state.first = None;
    assert_eq!(deadline(&state), None);
}

#[tokio::test]
async fn native_atomic_replacement_invalid_file_and_self_save_share_one_accepted_configuration() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("interface.lingua");
    replace(&path, &encode(&Settings::default(), &LayoutSettings::default()).unwrap());
    let mut app = fixture(directory.path());
    until(&mut app, |world| world.resource::<Reload>().watch.is_some() && world.resource::<Reload>().job.is_none() && world.resource::<Reload>().status.contains("External edits are watched")).await;
    let initial = app.world().resource::<Storage>().revision;
    let mut external = Settings::default();
    external.set(Shortcut::FocusLeft, "Mod+F2").unwrap();
    let layout = LayoutSettings { gaps: 17, ..Default::default() };
    replace(&path, &encode(&external, &layout).unwrap());
    until(&mut app, |world| world.resource::<Settings>() == &external && world.resource::<LayoutSettings>() == &layout).await;
    assert_eq!(app.world().resource::<Storage>().revision, initial + 1);
    replace(&path, b"This is not an interface configuration");
    until(&mut app, |world| world.resource::<Storage>().error.is_some()).await;
    assert_eq!(app.world().resource::<Settings>(), &external);
    assert_eq!(app.world().resource::<LayoutSettings>(), &layout);
    assert_eq!(app.world().resource::<Storage>().revision, initial + 1);
    replace(&path, &encode(&external, &layout).unwrap());
    until(&mut app, |world| world.resource::<Storage>().error.is_none()).await;
    assert_eq!(app.world().resource::<Storage>().revision, initial + 1);
    external.set(Shortcut::FocusRight, "Mod+F3").unwrap();
    super::super::commit(app.world_mut(), external.clone(), layout.clone()).unwrap();
    let own = app.world().resource::<Storage>().revision;
    let deadline = Instant::now() + Duration::from_millis(800);
    while Instant::now() < deadline { app.update(); tokio::time::sleep(Duration::from_millis(3)).await; }
    assert_eq!(app.world().resource::<Storage>().revision, own);
    assert_eq!(app.world().resource::<Settings>(), &external);
    assert_eq!(app.world().resource::<LayoutSettings>(), &layout);
    assert!(app.world().resource::<Reload>().job.is_none());
    assert!(app.world().resource::<Reload>().clock.is_none());
}

#[cfg(unix)]
#[tokio::test]
async fn a_dangling_selected_symlink_observes_creation_and_later_atomic_target_replacement() {
    let directory = tempfile::tempdir().unwrap();
    let targets = tempfile::tempdir().unwrap();
    let path = directory.path().join("interface.lingua");
    let target = targets.path().join("selected-config");
    std::os::unix::fs::symlink(&target, &path).unwrap();
    let mut app = fixture(directory.path());
    until(&mut app, |world| world.resource::<Reload>().watch.is_some() && world.resource::<Reload>().job.is_none() && world.resource::<Reload>().targets.contains(&target)).await;
    assert!(app.world().resource::<Storage>().error.is_none());
    let mut external = Settings::default();
    external.set(Shortcut::FocusLeft, "Mod+F2").unwrap();
    replace(&target, &encode(&external, &LayoutSettings::default()).unwrap());
    until(&mut app, |world| world.resource::<Settings>() == &external).await;
    external.set(Shortcut::FocusRight, "Mod+F3").unwrap();
    replace(&target, &encode(&external, &LayoutSettings::default()).unwrap());
    until(&mut app, |world| world.resource::<Settings>() == &external).await;
    assert!(std::fs::symlink_metadata(&path).unwrap().file_type().is_symlink());
}

#[tokio::test]
async fn a_read_started_before_a_local_save_cannot_restore_the_old_configuration() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("interface.lingua");
    let previous = encode(&Settings::default(), &LayoutSettings::default()).unwrap();
    replace(&path, &previous);
    let mut app = fixture(directory.path());
    until(&mut app, |world| world.resource::<Reload>().watch.is_some() && world.resource::<Reload>().job.is_none()).await;
    let mut state = app.world_mut().remove_resource::<Reload>().unwrap();
    state.origin = Some(Some(previous.clone()));
    let mut local = Settings::default();
    local.set(Shortcut::FocusLeft, "Mod+F2").unwrap();
    super::super::commit(app.world_mut(), local.clone(), LayoutSettings::default()).unwrap();
    accepted(app.world_mut(), &mut state, Loaded { path, targets: Default::default(), event_driven: true, missing: false, result: Ok((previous, Settings::default(), LayoutSettings::default())) });
    assert_eq!(app.world().resource::<Settings>(), &local);
    assert!(state.force);
    app.world_mut().insert_resource(state);
}

#[tokio::test]
async fn continued_atomic_writes_do_not_starve_publication_and_settle_on_the_final_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("interface.lingua");
    replace(&path, &encode(&Settings::default(), &LayoutSettings::default()).unwrap());
    let mut app = fixture(directory.path());
    until(&mut app, |world| world.resource::<Reload>().job.is_none() && world.resource::<Reload>().status.contains("External edits are watched")).await;
    let initial = app.world().resource::<Storage>().revision;
    let mut published_during_burst = false;
    for index in 0..90 {
        let layout = LayoutSettings { gaps: 16 + index % 8, ..Default::default() };
        replace(&path, &encode(&Settings::default(), &layout).unwrap());
        app.update();
        published_during_burst |= app.world().resource::<Storage>().revision > initial;
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(published_during_burst, "A continuing write burst starved the bounded read attempt");
    let final_layout = LayoutSettings { gaps: 23, ..Default::default() };
    replace(&path, &encode(&Settings::default(), &final_layout).unwrap());
    until(&mut app, |world| world.resource::<LayoutSettings>() == &final_layout && world.resource::<Reload>().job.is_none() && world.resource::<Reload>().clock.is_none()).await;
    assert!(app.world().resource::<Storage>().error.is_none());
}

#[tokio::test]
async fn disposal_aborts_the_deadline_and_discards_a_late_reader_result_without_a_wake() {
    use std::sync::atomic::AtomicUsize;
    let wakes = Arc::new(AtomicUsize::new(0));
    let counted = wakes.clone();
    let wake = crate::host::WakeSignal::new(move || { counted.fetch_add(1, Ordering::AcqRel); });
    let mut state = Reload::default();
    let retired = state.retired.clone();
    let received = state.received.clone();
    let timer_wake = wake.clone();
    state.clock = Some((Instant::now() + Duration::from_millis(20), tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(20)).await;
        timer_wake.ring();
    })));
    drop(state);
    assert!(retired.load(Ordering::Acquire));
    delivered(&received, &retired, Some(&wake), Loaded {
        path: PathBuf::from("retired-interface"), targets: Default::default(), event_driven: true, missing: false,
        result: Ok((Vec::new(), Settings::default(), LayoutSettings::default())),
    });
    tokio::time::sleep(Duration::from_millis(40)).await;
    assert!(received.lock().unwrap().is_none());
    assert_eq!(wakes.load(Ordering::Acquire), 0);
}
