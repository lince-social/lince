use super::*;

#[test]
fn face_outputs_require_valid_shapes_and_select_highest_confidence() {
    let mut outputs = Vec::new();
    for channels in [1, 1, 4, 10] {
        for stride in [8, 16, 32] {
            outputs.push(vec![0.0; (320 / stride) * (320 / stride) * channels]);
        }
    }
    outputs[0][41] = 0.95;
    outputs[3][41] = 0.95;
    assert_eq!(decode_head(&outputs).unwrap(), Some([0.025, 0.025]));
    outputs[3].clear();
    assert!(decode_head(&outputs).is_err());
}

#[test]
fn head_relative_pan_prioritizes_camera_right_and_obeys_neutral_zone() {
    let mut pan = HeadRelativePan::default();
    let head = Some([0.5, 0.5]);
    assert_eq!(pan.update(head, &[[0.5, 0.5]], 0.0), [0.0; 2]);
    assert_eq!(pan.update(head, &[[0.5, 0.5]], 0.03), [0.0; 2]);
    let delta = pan.update(head, &[[0.1, 0.1], [0.9, 0.9]], 0.06);
    assert!(delta[0] > 0.0 && delta[1] > 0.0);
    for (hand, axis, sign) in [
        ([0.2, 0.5], 0, -1.0),
        ([0.8, 0.5], 0, 1.0),
        ([0.5, 0.2], 1, -1.0),
        ([0.5, 0.8], 1, 1.0),
    ] {
        pan.reset();
        pan.update(head, &[hand], 1.0);
        let movement = pan.update(head, &[hand], 1.03);
        assert!(movement[axis] * sign > 0.0);
        assert_eq!(movement[1 - axis], 0.0);
    }
}

#[test]
fn head_relative_pan_stops_and_reanchors_after_loss_or_stale_frames() {
    let mut pan = HeadRelativePan::default();
    let head = Some([0.5, 0.5]);
    let hands = [[0.2, 0.2]];
    pan.update(head, &hands, 0.0);
    assert!(pan.update(head, &hands, 0.03)[0] < 0.0);
    assert_eq!(pan.update(None, &hands, 0.06), [0.0; 2]);
    assert_eq!(pan.update(head, &hands, 0.09), [0.0; 2]);
    assert_eq!(pan.update(head, &hands, 1.0), [0.0; 2]);
    assert_eq!(pan.update(head, &[], 1.03), [0.0; 2]);
}

fn open_landmarks() -> [[f32; 3]; 21] {
    let mut points = [[0.0; 3]; 21];
    points[0] = [100.0, 200.0, 0.0];
    for (finger, base) in [5, 9, 13, 17].into_iter().enumerate() {
        let x = 70.0 + finger as f32 * 20.0;
        for joint in 0..4 {
            points[base + joint] = [x, 160.0 - joint as f32 * 25.0, 0.0];
        }
    }
    points
}

#[test]
fn palm_orientation_rejects_back_edge_and_uncertain_handedness() {
    let mut hand = open_landmarks();
    assert!(palm_facing_camera(&hand, 0.95));
    assert!(!palm_facing_camera(&hand, 0.05));
    assert!(!palm_facing_camera(&hand, 0.5));
    for point in &mut hand { point[0] = -point[0]; }
    assert!(palm_facing_camera(&hand, 0.05));
    hand[17] = hand[5];
    assert!(!palm_facing_camera(&hand, 0.05));
}

#[test]
fn pinch_requires_close_thumb_and_index_at_a_valid_hand_scale() {
    let mut hand = open_landmarks();
    hand[4] = [70.0, 85.0, 0.0];
    assert!(is_pinched(&hand));
    hand[4] = [0.0, 0.0, 0.0];
    assert!(!is_pinched(&hand));
    hand[4][0] = f32::NAN;
    assert!(!is_pinched(&hand));
    assert!(!is_pinched(&[[0.0; 3]; 21]));
    let mut tracker = PointerTracker::default();
    let open = HandObservation { center: [0.5, 0.5], pointer: [0.5, 0.4], pinched: false };
    let closed = HandObservation { pinched: true, ..open };
    assert_eq!(tracker.update(&[open]), Some(PointerGesture::Hover(open.pointer)));
    assert_eq!(tracker.update(&[closed]), Some(PointerGesture::Hover(closed.pointer)));
    assert_eq!(tracker.update(&[closed]), Some(PointerGesture::Begin(closed.pointer)));
    assert_eq!(tracker.update(&[closed]), Some(PointerGesture::Update(closed.pointer)));
    assert_eq!(tracker.update(&[]), Some(PointerGesture::Cancel));
    assert_eq!(tracker.update(&[closed]), None);
    assert_eq!(tracker.update(&[open]), Some(PointerGesture::Hover(open.pointer)));
    assert_eq!(tracker.update(&[closed]), Some(PointerGesture::Hover(closed.pointer)));
    assert_eq!(tracker.update(&[closed]), Some(PointerGesture::Begin(closed.pointer)));
    assert_eq!(tracker.update(&[open]), Some(PointerGesture::Update(open.pointer)));
    assert_eq!(tracker.update(&[open]), Some(PointerGesture::End(open.pointer)));
    tracker.disarm();
    assert_eq!(tracker.update(&[closed]), None);
    assert_eq!(tracker.update(&[open]), Some(PointerGesture::Hover(open.pointer)));
    assert_eq!(tracker.update(&[closed]), Some(PointerGesture::Hover(closed.pointer)));
    assert_eq!(tracker.update(&[]), None);
    assert_eq!(tracker.update(&[closed]), None);
    assert_eq!(tracker.update(&[closed]), None);
    assert_eq!(tracker.update(&[open]), Some(PointerGesture::Hover(open.pointer)));
}

#[test]
fn open_hand_requires_all_four_fingers_extended() {
    let mut points = open_landmarks();
    assert!(is_open(&points));
    points[8] = [70.0, 175.0, 0.0];
    assert!(!is_open(&points));
    assert!(!is_open(&[[0.0; 3]; 21]));
    points[8][0] = f32::NAN;
    assert!(!is_open(&points));
}

#[test]
fn hand_pose_test_is_rotation_invariant() {
    let mut points = open_landmarks();
    for point in &mut points {
        *point = [-point[1], point[0], point[2]];
    }
    assert!(is_open(&points));
}

#[test]
fn pan_reanchors_after_closed_hand_tracking_loss_stale_frames_and_jumps() {
    let mut pan = PanTracker::default();
    assert_eq!(pan.update(Some([0.5, 0.5]), 0.0), [0.0; 2]);
    let delta = pan.update(Some([0.55, 0.45]), 0.03);
    assert!(delta[0] > 0.0 && delta[1] < 0.0);
    assert_eq!(pan.update(None, 0.06), [0.0; 2]);
    assert_eq!(pan.update(Some([0.7, 0.7]), 0.09), [0.0; 2]);
    assert_eq!(pan.update(Some([0.8, 0.8]), 0.8), [0.0; 2]);
    assert_eq!(pan.update(Some([0.1, 0.1]), 0.83), [0.0; 2]);
    assert_eq!(pan.update(Some([f32::NAN, 0.0]), 0.86), [0.0; 2]);
    assert_eq!(pan.update(Some([0.5, 0.5]), 0.89), [0.0; 2]);
    pan.reset();
    assert_eq!(pan.update(Some([0.6, 0.6]), 0.92), [0.0; 2]);
}

#[test]
fn jitter_is_suppressed_but_slow_intentional_movement_accumulates() {
    let mut pan = PanTracker::default();
    pan.update(Some([0.5, 0.5]), 0.0);
    for frame in 1..20 {
        let x = if frame % 2 == 0 { 0.501 } else { 0.499 };
        assert_eq!(
            pan.update(Some([x, 0.5]), f64::from(frame) * 0.03),
            [0.0; 2]
        );
    }
    let mut movement = 0.0;
    for frame in 20..80 {
        movement += pan.update(
            Some([0.5 + (frame - 20) as f32 * 0.001, 0.5]),
            f64::from(frame) * 0.03,
        )[0];
    }
    assert!(movement > 0.05 && movement < 0.06);
}

#[test]
fn resampling_preserves_rgb_and_letterboxes_non_square_frames() {
    let image = RgbImage::from_pixel(4, 2, image::Rgb([255, 0, 128]));
    let pixels = crop(&image, [2.0, 1.0], [1.0, 0.0], 4.0, 4);
    assert_eq!(&pixels[0..3], &[0.0; 3]);
    assert_eq!(pixels[12], 1.0);
    assert_eq!(pixels[13], 0.0);
    assert!((pixels[14] - 128.0 / 255.0).abs() < 0.0001);
    assert_eq!(anchor(0), [0.5 / 24.0; 2]);
    assert_eq!(anchor(1152), [0.5 / 12.0; 2]);
    assert!(
        anchor(2015)
            .iter()
            .all(|value| (value - 11.5 / 12.0).abs() < 0.000001)
    );
}

#[test]
#[ignore = "requires cargo xtask hand-models"]
fn downloaded_models_run() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.cache-lince-hand");
    let tracker = HandTracker::load(&directory).unwrap();
    let image = RgbImage::new(640, 480);
    let started = std::time::Instant::now();
    assert_eq!(tracker.open_hand(&image).unwrap(), None);
    assert_eq!(tracker.head(&image).unwrap(), None);
    let outputs = run(&tracker.hand, vec![0.0; 224 * 224 * 3], 224).unwrap();
    assert_eq!(
        outputs.iter().map(Vec::len).collect::<Vec<_>>(),
        [63, 1, 1, 63]
    );
    assert!(outputs.iter().flatten().all(|value| value.is_finite()));
    println!("Hand and face models inferred in {:?}", started.elapsed());
    if let Some(path) = std::env::var_os("LINCE_HAND_TEST_IMAGE") {
        let image = image::open(path).unwrap().to_rgb8();
        assert!(
            tracker.open_hand(&image).unwrap().is_some(),
            "Expected an open hand in the supplied image"
        );
        assert!(
            tracker
                .open_hand(&image::imageops::rotate90(&image))
                .unwrap()
                .is_some()
        );
    }
    if let Some(path) = std::env::var_os("LINCE_HAND_CLOSED_TEST_IMAGE") {
        let image = image::open(path).unwrap().to_rgb8();
        assert_eq!(tracker.open_hand(&image).unwrap(), None);
    }
    if let Some(path) = std::env::var_os("LINCE_FACE_TEST_IMAGE") {
        let image = image::open(path).unwrap().to_rgb8();
        assert!(
            tracker.head(&image).unwrap().is_some(),
            "Expected a face in the supplied image"
        );
    }
    if let Some(path) = std::env::var_os("LINCE_TWO_PALMS_TEST_IMAGE") {
        let image = image::open(path).unwrap().to_rgb8();
        assert_eq!(
            tracker.open_hands(&image).unwrap().len(),
            2,
            "Expected two open hands in the supplied image"
        );
    }
}
