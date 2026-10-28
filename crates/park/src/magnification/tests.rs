use super::*;
use lince_interface::host::{Rectangle, OutputViewport, control::Context, display::{OutputHead, OutputMode}, magnification::Outcome, mapping::{Mapping, QuarterTurn}};

fn context() -> Context { Context { owner: 1, canvas: "fixture".into(), workspace: 1, output: Some(1), item: None } }

fn display() -> DisplaySnapshot {
    let bounds = Rectangle::new([0.0; 2], [800.0, 600.0]).unwrap();
    DisplaySnapshot { revision: 1,
        heads: vec![OutputHead { color: Default::default(), adaptive_sync: Default::default(), rotation: Default::default(), id: OutputId(1), name: "DP-1".into(), modes: vec![OutputMode { size: [800, 600], refresh_mhz: 60_000, preferred: true }], current: Some(0), position: [0; 2], scale: 1.0 }],
        outputs: vec![OutputViewport { rotation: Default::default(), id: OutputId(1), bounds, usable: Some(bounds), physical_size: [800, 600], raster: Default::default(), workspace: 1 }], ..Default::default() }
}

fn request(state: &State, id: u64, factor: f64) -> Request {
    Request { id, revision: state.snapshot.revision, output: OutputId(1), factor, context: context() }
}

#[test]
fn saving_changes_durable_intent_while_effective_state_waits_for_its_actual_output() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("park.json");
    std::fs::write(&path, br#"{"outputs":{"DP-1":{"scale":1.25},"DP-2":{"magnification":3}},"keyboard":{"layout":"br"}}"#).unwrap();
    let mut state = State::default();
    let display = display();
    state.reconcile(&display, true);
    let baseline = Mapping::new(&display.outputs[0], QuarterTurn::Normal, 1.0, [400.0, 300.0], 1).unwrap();
    state.presented(OutputId(1), &mapping::Snapshot::new(1, [baseline]).unwrap());
    let revision = state.snapshot.revision;
    assert!(state.request(request(&state, 1, 2.5), None, |name, factor, expected| crate::commands::Commands::save_magnification(&path, name, factor, expected)));
    assert_eq!(state.snapshot.head(OutputId(1)).unwrap().requested, 2.5);
    assert_eq!(state.snapshot.head(OutputId(1)).unwrap().effective, Some(1.0));
    assert!(state.snapshot.revision > revision);
    let saved = crate::commands::Commands::load(&path).unwrap();
    assert_eq!(saved.outputs["DP-1"].magnification, 2.5);
    assert_eq!(saved.outputs["DP-1"].scale, 1.25);
    assert_eq!(saved.outputs["DP-2"].magnification, 3.0);
    assert_eq!(saved.keyboard.layout, "br");
    crate::commands::Commands::save_outputs(&path, &display.heads).unwrap();
    let saved = crate::commands::Commands::load(&path).unwrap();
    assert_eq!(saved.outputs["DP-1"].magnification, 2.5);
    assert_eq!(saved.outputs["DP-2"].magnification, 3.0);
    let mapping = Mapping::new(&display.outputs[0], QuarterTurn::Normal, 2.5, [400.0, 300.0], 2).unwrap();
    let snapshot = mapping::Snapshot::new(2, [mapping]).unwrap();
    assert!(!state.presented(OutputId(2), &snapshot));
    assert!(state.presented(OutputId(1), &snapshot));
    assert_eq!(state.snapshot.head(OutputId(1)).unwrap().effective, Some(2.5));
    assert!(!state.presented(OutputId(1), &snapshot));
}

#[test]
fn stale_invalid_removed_and_denied_requests_never_reach_the_save_transaction() {
    let mut state = State::default();
    state.reconcile(&display(), true);
    for (index, factor) in [f64::NAN, f64::INFINITY, 0.99, 4.01].into_iter().enumerate() {
        let request = request(&state, index as u64 + 1, factor);
        assert!(!state.request(request, None, |_, _, _| panic!("invalid settings reached storage")));
    }
    let mut stale = request(&state, 5, 2.0);
    stale.revision -= 1;
    assert!(!state.request(stale, None, |_, _, _| panic!("stale settings reached storage")));
    let mut removed = request(&state, 6, 2.0);
    removed.output = OutputId(99);
    assert!(!state.request(removed, None, |_, _, _| panic!("removed output reached storage")));
    assert!(!state.request(request(&state, 7, 2.0), Some("The original view was retired."), |_, _, _| panic!("retired view reached storage")));
    assert_eq!(state.snapshot.head(OutputId(1)).unwrap().requested, 1.0);
    assert!(matches!(state.snapshot.response.as_ref().unwrap().outcome, Outcome::Failed(_)));
}

#[test]
fn outside_edits_and_invalid_reload_preserve_the_draft_source_and_last_good_effective_intent() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("park.json");
    std::fs::write(&path, br#"{"outputs":{"DP-1":{"magnification":3}}}"#).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let mut state = State::default();
    state.reconcile(&display(), true);
    assert!(!state.request(request(&state, 1, 2.0), None, |name, factor, expected| crate::commands::Commands::save_magnification(&path, name, factor, expected)));
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert_eq!(state.snapshot.head(OutputId(1)).unwrap().requested, 1.0);
    state.reload(Reload { id: 2, context: context() }, None, || {
        Ok(crate::commands::Commands::load(&path)?.outputs.into_iter().map(|(name, output)| (name, output.magnification)).collect())
    });
    assert_eq!(state.snapshot.head(OutputId(1)).unwrap().requested, 3.0);
    assert!(matches!(state.snapshot.response.as_ref().unwrap().outcome, Outcome::Loaded));
    std::fs::write(&path, br#"{"outputs":{"DP-1":{"magnification":5}}}"#).unwrap();
    state.reload(Reload { id: 3, context: context() }, None, || {
        Ok(crate::commands::Commands::load(&path)?.outputs.into_iter().map(|(name, output)| (name, output.magnification)).collect())
    });
    assert_eq!(state.snapshot.head(OutputId(1)).unwrap().requested, 3.0);
    assert!(matches!(state.snapshot.response.as_ref().unwrap().outcome, Outcome::Failed(_)));
    assert!(state.reconcile(&DisplaySnapshot::default(), true));
    assert!(state.snapshot.heads.is_empty());
    state.reconcile(&display(), true);
    assert_eq!(state.snapshot.head(OutputId(1)).unwrap().requested, 3.0);
}

#[test]
fn a_storage_failure_leaves_active_intent_unchanged_and_a_reused_request_cannot_apply_twice() {
    let mut state = State::default();
    state.reconcile(&display(), true);
    let candidate = request(&state, 1, 4.0);
    assert!(!state.request(candidate.clone(), None, |_, _, _| Err(io::Error::other("fixture write failure"))));
    assert_eq!(state.snapshot.head(OutputId(1)).unwrap().requested, 1.0);
    assert!(!state.request(candidate, None, |_, _, _| panic!("reused request reached storage")));
    let candidate = request(&state, 2, 4.0);
    assert!(state.request(candidate, None, |_, _, _| Ok(Some("Directory sync warning".into()))));
    assert_eq!(state.snapshot.head(OutputId(1)).unwrap().requested, 4.0);
    assert!(matches!(&state.snapshot.response.as_ref().unwrap().outcome, Outcome::Saved { warning: Some(warning) } if warning == "Directory sync warning"));
}
