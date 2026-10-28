use super::*;

fn start(fingers: u32) -> GestureRecognizer {
    let mut recognizer = GestureRecognizer::default();
    assert_eq!(
        recognizer.begin(1, OutputId(2), fingers, GestureSettings::default()),
        Ok(true)
    );
    recognizer
}

#[test]
fn one_completed_swipe_commits_once_to_its_original_output() {
    let mut recognizer = start(3);
    for _ in 0..4 {
        assert_eq!(recognizer.update(1, [0.0, 25.0]), Ok(None));
    }
    assert_eq!(
        recognizer.end(1, false),
        Some(GestureCompletion {
            sequence: 1,
            output: OutputId(2),
            result: GestureResult::Workspace(1),
        })
    );
    assert_eq!(recognizer.end(1, false), None);
    assert_eq!(recognizer.update(1, [0.0, 100.0]), Ok(None));
    assert_eq!(
        recognizer.begin(1, OutputId(3), 3, GestureSettings::default()),
        Err(GestureError::InvalidSequence)
    );
}

#[test]
fn small_diagonal_and_four_finger_horizontal_gestures_have_no_action() {
    for (fingers, delta) in [(3, [0.0, 40.0]), (3, [80.0, 70.0]), (4, [100.0, 0.0])] {
        let mut recognizer = start(fingers);
        assert_eq!(recognizer.update(1, delta), Ok(None));
        assert_eq!(
            recognizer.end(1, false).unwrap().result,
            GestureResult::Noop
        );
    }
}

#[test]
fn horizontal_pan_keeps_axis_and_reports_total_travel_for_coalescing() {
    let mut recognizer = start(3);
    let first = recognizer.update(1, [20.0, 1.0]).unwrap().unwrap();
    assert_eq!(first.horizontal_travel, 20.0);
    let last = recognizer.update(1, [-50.0, 100.0]).unwrap().unwrap();
    assert_eq!(last.sequence, first.sequence);
    assert_eq!(last.output, first.output);
    assert_eq!(last.horizontal_travel, -30.0);
    assert_eq!(
        recognizer.end(1, false).unwrap().result,
        GestureResult::Pan(-30.0)
    );
}

#[test]
fn cancel_and_invalid_travel_never_commit_a_partial_gesture() {
    let mut recognizer = start(3);
    recognizer.update(1, [100.0, 0.0]).unwrap();
    assert_eq!(
        recognizer.cancel().unwrap().result,
        GestureResult::Cancelled
    );
    assert_eq!(recognizer.cancel(), None);
    for delta in [[f64::NAN, 0.0], [0.0, f64::INFINITY], [2_000_000.0, 0.0]] {
        let mut recognizer = start(3);
        recognizer.update(1, [20.0, 0.0]).unwrap();
        assert_eq!(recognizer.update(1, delta), Err(GestureError::InvalidDelta));
        assert_eq!(
            recognizer.update(1, [10.0, 0.0]),
            Err(GestureError::InvalidDelta)
        );
        assert_eq!(
            recognizer.end(1, false).unwrap().result,
            GestureResult::Cancelled
        );
    }
}

#[test]
fn sensitivity_is_captured_at_begin_and_overview_is_directional() {
    let mut recognizer = GestureRecognizer::default();
    let mut settings = GestureSettings {
        sensitivity_percent: 200,
        ..Default::default()
    };
    assert!(recognizer.begin(1, OutputId(1), 4, settings).unwrap());
    settings.sensitivity_percent = 10;
    recognizer.update(1, [0.0, 40.0]).unwrap();
    assert_eq!(
        recognizer.end(1, false).unwrap().result,
        GestureResult::Overview(true)
    );
    assert!(recognizer.begin(2, OutputId(1), 4, settings).unwrap());
    recognizer.update(2, [0.0, -800.0]).unwrap();
    assert_eq!(
        recognizer.end(2, false).unwrap().result,
        GestureResult::Overview(false)
    );
}

#[test]
fn invalid_begin_does_not_claim_an_output_or_consume_a_valid_sequence() {
    let mut recognizer = GestureRecognizer::default();
    assert_eq!(
        recognizer.begin(0, OutputId(1), 3, GestureSettings::default()),
        Err(GestureError::InvalidSequence)
    );
    assert_eq!(
        recognizer.begin(
            1,
            OutputId(1),
            3,
            GestureSettings {
                commit_threshold: 0,
                ..Default::default()
            }
        ),
        Err(GestureError::InvalidSettings)
    );
    assert_eq!(recognizer.active(), None);
    assert!(
        recognizer
            .begin(1, OutputId(1), 3, GestureSettings::default())
            .unwrap()
    );
}

#[test]
fn zero_is_a_valid_output_identity() {
    let mut recognizer = GestureRecognizer::default();
    assert!(
        recognizer
            .begin(1, OutputId(0), 3, GestureSettings::default())
            .unwrap()
    );
    recognizer.update(1, [0.0, 80.0]).unwrap();
    assert_eq!(
        recognizer.end(1, false),
        Some(GestureCompletion {
            sequence: 1,
            output: OutputId(0),
            result: GestureResult::Workspace(1),
        })
    );
}

#[test]
fn unmapped_sequences_stay_unclaimed_and_cannot_be_reused_after_settings_change() {
    let mut recognizer = GestureRecognizer::default();
    assert!(
        !recognizer
            .begin(1, OutputId(1), 2, GestureSettings::default())
            .unwrap()
    );
    assert_eq!(recognizer.active(), None);
    assert_eq!(recognizer.update(1, [100.0, 0.0]), Ok(None));
    assert_eq!(recognizer.end(1, false), None);
    assert!(
        !recognizer
            .begin(
                2,
                OutputId(1),
                3,
                GestureSettings {
                    enabled: false,
                    ..Default::default()
                }
            )
            .unwrap()
    );
    assert_eq!(
        recognizer.begin(2, OutputId(1), 3, GestureSettings::default()),
        Err(GestureError::InvalidSequence)
    );
}

#[test]
fn overlapping_and_foreign_sequences_cannot_retarget_the_active_swipe() {
    let mut recognizer = start(3);
    assert_eq!(
        recognizer.begin(2, OutputId(3), 4, GestureSettings::default()),
        Err(GestureError::AlreadyActive)
    );
    assert_eq!(recognizer.update(2, [0.0, 200.0]), Ok(None));
    assert_eq!(recognizer.end(2, true), None);
    assert_eq!(recognizer.active(), Some((1, OutputId(2))));
    recognizer.update(1, [0.0, -80.0]).unwrap();
    assert_eq!(
        recognizer.end(1, false).unwrap().result,
        GestureResult::Workspace(-1)
    );
}

#[test]
fn settings_reject_unbounded_values_and_round_trip_as_portable_data() {
    let settings = GestureSettings::default();
    assert_eq!(
        serde_json::from_value::<GestureSettings>(serde_json::to_value(settings).unwrap()).unwrap(),
        settings
    );
    for settings in [
        GestureSettings {
            sensitivity_percent: 0,
            ..settings
        },
        GestureSettings {
            sensitivity_percent: 801,
            ..settings
        },
        GestureSettings {
            commit_threshold: 15,
            ..settings
        },
        GestureSettings {
            commit_threshold: 401,
            ..settings
        },
    ] {
        assert!(settings.validate().is_err());
    }
}
