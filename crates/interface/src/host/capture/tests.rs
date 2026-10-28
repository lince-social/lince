use super::*;

#[test]
fn unresolved_retained_images_require_filtering_without_stopping_public_output_sessions() {
    let gate = PolicyGate::default();
    gate.publish(Policy::default());
    let public_revision = gate.read().unwrap().revision();
    gate.publish(Policy { unresolved_images: true, ..Default::default() });
    let unresolved = gate.read().unwrap();
    assert!(unresolved.requires_filter());
    assert!(unresolved.permits(Target::Output(OutputId(1))));
    assert!(!unresolved.matches(Target::Output(OutputId(1)), public_revision));
    let revision = unresolved.revision();
    drop(unresolved);
    gate.publish(Policy::default());
    let restored = gate.read().unwrap();
    assert!(!restored.requires_filter());
    assert!(!restored.matches(Target::Output(OutputId(1)), revision));
}

#[test]
fn publishing_unchanged_content_keeps_in_flight_privacy_revisions_valid() {
    let gate = PolicyGate::default();
    gate.invalidate();
    assert!(!gate.read().unwrap().permits(Target::Output(OutputId(1))));
    let policy = Policy { windows: BTreeMap::from([(SurfaceId(7), false)]), excluded: BTreeSet::new(), ..Default::default() };
    gate.publish(policy.clone());
    let revision = gate.read().unwrap().revision();
    for _ in 0..120 { gate.publish(policy.clone()); }
    assert!(gate.read().unwrap().matches(Target::Window(SurfaceId(7)), revision));
    assert!(gate.read().unwrap().matches(Target::Output(OutputId(1)), revision));
    assert!(!gate.read().unwrap().permits(Target::Window(SurfaceId(8))));
    assert!(!gate.read().unwrap().knows(SurfaceId(8)));
}

#[test]
fn policy_changes_retire_old_jobs_and_unknown_or_private_sources_fail_closed() {
    let gate = PolicyGate::default();
    gate.publish(Policy { windows: BTreeMap::from([(SurfaceId(7), false), (SurfaceId(8), false)]), excluded: BTreeSet::new(), ..Default::default() });
    let revision = gate.read().unwrap().revision();
    gate.publish(Policy { windows: BTreeMap::from([(SurfaceId(7), true), (SurfaceId(8), false)]), excluded: BTreeSet::from([[5; 16]]), ..Default::default() });
    let policy = gate.read().unwrap();
    assert!(!policy.matches(Target::Window(SurfaceId(8)), revision));
    assert!(!policy.permits(Target::Window(SurfaceId(7))));
    assert!(policy.permits(Target::Window(SurfaceId(8))));
    assert!(policy.permits(Target::Output(OutputId(1))));
    assert!(policy.requires_filter());
    drop(policy);
    gate.invalidate();
    assert!(!gate.read().unwrap().permits(Target::Window(SurfaceId(8))));
}

#[test]
fn final_capture_copy_orders_security_publication_after_its_guard() {
    let gate = PolicyGate::default();
    gate.publish(Policy::default());
    let read = gate.read().unwrap();
    let revision = read.revision();
    let writer = gate.clone();
    let (started, ready) = std::sync::mpsc::sync_channel(1);
    let (finished, done) = std::sync::mpsc::sync_channel(1);
    let task = std::thread::spawn(move || {
        started.send(()).unwrap();
        writer.invalidate();
        finished.send(()).unwrap();
    });
    ready.recv_timeout(std::time::Duration::from_secs(3)).unwrap();
    assert_eq!(done.try_recv(), Err(std::sync::mpsc::TryRecvError::Empty));
    assert!(read.matches(Target::Output(OutputId(1)), revision));
    drop(read);
    done.recv_timeout(std::time::Duration::from_secs(3)).unwrap();
    task.join().unwrap();
    assert!(!gate.read().unwrap().matches(Target::Output(OutputId(1)), revision));
}

#[test]
fn poisoned_policy_publication_cannot_reopen_capture() {
    let gate = PolicyGate::default();
    let writer = gate.clone();
    assert!(std::thread::spawn(move || {
        let _guard = writer.0.write().unwrap();
        panic!("interrupted policy publication");
    }).join().is_err());
    gate.publish(Policy::default());
    assert!(gate.read().is_none());
}

#[test]
fn private_contributors_without_saved_layouts_still_require_filtering_and_retire_old_images() {
    let gate = PolicyGate::default();
    gate.publish(Policy { private_entities: BTreeSet::from([41]), ..Default::default() });
    let revision = gate.read().unwrap().revision();
    assert!(gate.read().unwrap().requires_filter());
    gate.publish(Policy { private_entities: BTreeSet::from([79]), ..Default::default() });
    assert!(!gate.read().unwrap().matches(Target::Output(OutputId(1)), revision));
}
