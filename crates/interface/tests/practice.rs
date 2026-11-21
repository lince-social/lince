use lince_interface::{
    handbook::{foundations, pages},
    practice::{
        Effect, LearningProgress, Lesson, Mode, Observation, Ownership, Phase, Progress,
        Resolution, Runner, Target, Ticket, lessons, resolve,
    },
};

const EDIT_STEP: &str = "step-interface-open-edit";

fn lesson(subject: &str) -> &'static Lesson {
    lessons()
        .iter()
        .find(|lesson| lesson.subject == subject)
        .unwrap_or_else(|| panic!("missing authored Practice lesson {subject}"))
}

fn runner() -> Runner {
    Runner::start(
        1,
        lesson("sands"),
        Mode::Assisted,
        LearningProgress::default(),
    )
}

fn ticket(effect: Effect) -> Ticket {
    match effect {
        Effect::Execute { ticket, .. } => ticket,
        other => panic!("Expected execution: {other:?}"),
    }
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
    assert_eq!(
        runner.response(request, Observation::Complete),
        Effect::Advanced
    );
    assert_eq!(
        runner.response(request, Observation::Complete),
        Effect::None
    );
    assert_eq!(runner.progress.0.get(EDIT_STEP), Some(&Progress::Practiced));
    assert_eq!(runner.next(Observation::Complete, 42, 30), Effect::Advanced);
    assert_eq!(runner.step, 2);
}

#[test]
fn skip_close_and_chapter_changes_invalidate_late_responses() {
    let exits: [fn(&mut Runner); 3] = [
        |runner| {
            runner.skip();
        },
        Runner::close,
        |runner| assert!(runner.select(lesson("areas-of-influence"), 2)),
    ];
    for exit in exits {
        let mut runner = runner();
        let request = ticket(runner.next(Observation::Waiting, 0, 30));
        exit(&mut runner);
        let phase = runner.phase.clone();
        let step = runner.step;
        assert_eq!(
            runner.response(request, Observation::Complete),
            Effect::None
        );
        assert_eq!(runner.phase, phase);
        assert_eq!(runner.step, step);
        assert!(!runner.restricted());
    }
}

#[test]
fn modes_and_missing_targets_release_restrictions_and_recovery_is_independent() {
    let mut runner = runner();
    let target = Target {
        window: 1,
        workspace: 2,
        owner: "sample".into(),
        role: "edit.open".into(),
    };
    let other = Target {
        owner: target.owner.clone(),
        role: "delete".into(),
        ..target
    };
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
    assert_eq!(
        runner.progress.0.get(EDIT_STEP),
        Some(&Progress::Unavailable)
    );
}

#[test]
fn every_lesson_starts_directly_and_skip_is_never_verified_practice() {
    let lessons = lessons();
    assert!(!lessons.is_empty(), "missing authored Practice lessons");
    for lesson in lessons {
        for mode in [Mode::Free, Mode::Assisted] {
            let mut runner = Runner::start(1, lesson, mode, LearningProgress::default());
            assert_eq!(runner.current(), lesson.steps.first());
            for step in lesson.steps {
                runner.skip();
                assert_eq!(runner.progress.0.get(step.slug), Some(&Progress::Skipped));
            }
            assert_eq!(runner.phase, Phase::Complete);
        }
    }
}

#[test]
fn semantic_resolution_requires_the_exact_instance_and_accepts_recreation() {
    let target = Target {
        window: 1,
        workspace: 2,
        owner: "castle-a".into(),
        role: "protein.run".into(),
    };
    let other = Target {
        owner: "castle-b".into(),
        role: target.role.clone(),
        ..target
    };
    assert_eq!(
        resolve(&target, [(other, 11), (target.clone(), 12)]),
        Ok(12)
    );
    assert_eq!(resolve(&target, [(target.clone(), 13)]), Ok(13));
    assert_eq!(
        resolve(&target, [(target.clone(), 12), (target.clone(), 13)]),
        Err(Resolution::Ambiguous)
    );
    assert_eq!(resolve::<i32>(&target, []), Err(Resolution::Missing));
}

#[test]
fn discard_only_returns_owned_resources_and_keeps_pending_work_visible() {
    let mut ownership = Ownership::default();
    ownership.created("sample-a");
    ownership.created("sample-a");
    ownership.pending("request-b");
    assert_eq!(ownership.discard(), ["sample-a"]);
    assert!(ownership.outstanding().eq(["request-b"]));
    ownership.confirmed("request-b", "sample-b");
    assert_eq!(ownership.discard(), ["sample-b"]);
    assert_eq!(ownership.outstanding().next(), None);
}

#[test]
fn unavailable_hardware_inspection_is_visited_and_never_practiced() {
    let mut runner = runner();
    let slug = runner.current().expect("introductory inspection step").slug;
    runner.next(Observation::Viewed, 0, 30);
    assert_eq!(runner.progress.0.get(slug), Some(&Progress::Visited));
    assert_eq!(runner.step, 1);
    let slug = runner.current().expect("first actionable step").slug;
    let request = ticket(runner.next(Observation::Waiting, 1, 30));
    runner.response(request, Observation::Viewed);
    assert_eq!(runner.progress.0.get(slug), Some(&Progress::Visited));
}

#[test]
fn foundations_refer_to_earlier_pages_without_creating_progress_locks() {
    let pages = pages();
    assert!(!pages.is_empty(), "missing authored handbook pages");
    for (index, page) in pages.iter().enumerate() {
        for &foundation in foundations(page.slug) {
            let precedes = pages
                .iter()
                .take(index)
                .any(|earlier| earlier.slug == foundation);
            assert!(precedes, "{foundation} must precede {}", page.slug);
        }
    }
}
