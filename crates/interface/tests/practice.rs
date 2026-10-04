use lince_interface::practice::*;

fn runner() -> Runner { Runner::start(1, &LESSONS[0], Mode::Assisted, LearningProgress::default()) }

fn ticket(effect: Effect) -> Ticket {
    match effect { Effect::Execute { ticket, .. } => ticket, other => panic!("Expected execution: {other:?}") }
}

#[test]
fn next_checks_completion_and_never_duplicates_pending_actions() {
    let mut runner = runner();
    let request = ticket(runner.next(Observation::Waiting, 10, 30));
    assert_eq!(runner.next(Observation::Waiting, 11, 30), Effect::None);
    assert!(!runner.tick(39));
    assert!(runner.tick(40));
    assert!(!runner.restricted());
    assert_eq!(ticket(runner.next(Observation::Waiting, 41, 30)), request);
    assert_eq!(runner.response(request, Observation::Complete), Effect::Advanced);
    assert_eq!(runner.response(request, Observation::Complete), Effect::None);
    assert_eq!(runner.progress.0["step-interface-open-edit"], Progress::Practiced);
    assert_eq!(runner.next(Observation::Complete, 42, 30), Effect::Advanced);
    assert_eq!(runner.step, 2);
}

#[test]
fn skip_close_and_chapter_changes_invalidate_late_responses() {
    for exit in 0..3 {
        let mut runner = runner();
        let request = ticket(runner.next(Observation::Waiting, 0, 30));
        match exit { 0 => { runner.skip(); }, 1 => runner.close(), _ => { assert!(runner.select(&LESSONS[3], 2)); } }
        let phase = runner.phase.clone();
        let step = runner.step;
        assert_eq!(runner.response(request, Observation::Complete), Effect::None);
        assert_eq!(runner.phase, phase);
        assert_eq!(runner.step, step);
        assert!(!runner.restricted());
    }
}

#[test]
fn modes_and_missing_targets_release_restrictions_and_recovery_is_independent() {
    let mut runner = runner();
    let target = Target { window: 1, workspace: 2, owner: "sample".into(), role: "edit.open".into() };
    let mut other = target.clone();
    other.role = "delete".into();
    runner.set_target(Ok(()));
    assert!(runner.restricted());
    assert!(!runner.permits(&other, &target, false, false));
    assert!(runner.permits(&other, &target, true, false));
    assert!(runner.permits(&other, &target, false, true));
    runner.set_target(Err(Resolution::Ambiguous));
    assert!(!runner.restricted());
    runner.set_target(Ok(()));
    runner.switch_mode(Mode::Free);
    assert!(!runner.restricted());
    runner.switch_mode(Mode::Assisted);
    runner.next(Observation::Unavailable("No capability".into()), 0, 30);
    assert!(!runner.restricted());
    runner.skip();
    assert_eq!(runner.progress.0["step-interface-open-edit"], Progress::Unavailable);
}

#[test]
fn every_lesson_starts_directly_and_skip_is_never_verified_practice() {
    for lesson in LESSONS {
        for mode in [Mode::Free, Mode::Assisted] {
            let mut runner = Runner::start(1, lesson, mode, LearningProgress::default());
            assert_eq!(runner.current(), lesson.steps.first());
            for step in lesson.steps { runner.skip(); assert_eq!(runner.progress.0[step.slug], Progress::Skipped); }
            assert_eq!(runner.phase, Phase::Complete);
        }
    }
}

#[test]
fn semantic_resolution_requires_the_exact_instance_and_accepts_recreation() {
    let target = Target { window: 1, workspace: 2, owner: "castle-a".into(), role: "protein.run".into() };
    let mut other = target.clone();
    other.owner = "castle-b".into();
    assert_eq!(resolve(&target, [(other, 11), (target.clone(), 12)]), Ok(12));
    assert_eq!(resolve(&target, [(target.clone(), 13)]), Ok(13));
    assert_eq!(resolve(&target, [(target.clone(), 12), (target.clone(), 13)]), Err(Resolution::Ambiguous));
    assert_eq!(resolve::<i32>(&target, []), Err(Resolution::Missing));
}

#[test]
fn discard_only_returns_owned_resources_and_keeps_pending_work_visible() {
    let mut ownership = Ownership::default();
    ownership.created("sample-a");
    ownership.created("sample-a");
    ownership.pending("request-b");
    assert_eq!(ownership.discard(), ["sample-a"]);
    assert_eq!(ownership.outstanding().collect::<Vec<_>>(), ["request-b"]);
    ownership.confirmed("request-b", "sample-b");
    assert_eq!(ownership.discard(), ["sample-b"]);
    assert_eq!(ownership.outstanding().count(), 0);
}
