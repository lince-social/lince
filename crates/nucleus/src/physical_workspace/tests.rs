use glam::DVec2;
use serde_json::{from_slice, from_str, to_string, to_value, to_vec};

use super::{
    Configuration, Device, DeviceKind, Evidence, Interaction, MAX_AGE_MS, MAX_OBSERVATIONS,
    MAX_REQUESTS, MAX_RUNNING, Phase, Quad, Request, Runtime, Status, Surface, Target,
    replay::{Event, Step, Trace},
};
use crate::canvas::room;

fn configuration() -> Configuration {
    Configuration {
        devices: vec![
            Device {
                id: "camera".into(),
                name: "Desk camera".into(),
                kind: DeviceKind::Camera,
                resolution: [1280, 720],
            },
            Device {
                id: "projector".into(),
                name: "Wall projector".into(),
                kind: DeviceKind::Projector,
                resolution: [1280, 720],
            },
        ],
        surfaces: vec![Surface {
            id: "desk".into(),
            name: "Desk".into(),
            workspace: 1,
            projector: "projector".into(),
            camera: Some("camera".into()),
            physical_pose_meters: room::identity_pose(),
            size_meters: [1.2, 0.8],
            calibration_revision: 1,
            projector_quad: Quad::default(),
            camera_quad: Quad::default(),
        }],
    }
}

fn surface_mut<'a>(configuration: &'a mut Configuration, id: &str) -> &'a mut Surface {
    configuration
        .surfaces
        .iter_mut()
        .find(|surface| surface.id == id)
        .expect("the fixture contains the requested surface")
}

fn widen(surface: &mut Surface) {
    let [width, _] = &mut surface.size_meters;
    *width = 2.0;
}

fn runtime() -> Runtime {
    let mut runtime = Runtime::new(configuration()).unwrap();
    runtime.connect("camera", "capture-1").unwrap();
    runtime.connect("projector", "output-1").unwrap();
    runtime.capture("camera", true).unwrap();
    runtime.blank(false);
    runtime
}

fn evidence(sequence: u64, time: u64) -> Evidence {
    Evidence {
        id: format!("observation-{sequence}"),
        device: "camera".into(),
        session: "capture-1".into(),
        sequence,
        captured_ms: time,
        surface: "desk".into(),
        calibration_revision: 1,
        confidence: 0.95,
        media: Some(format!("frame-{sequence}")),
        projected_content: false,
    }
}

fn interaction(sequence: u64, time: u64, phase: Phase) -> Interaction {
    Interaction {
        evidence: evidence(sequence, time),
        pointer: "hand-1".into(),
        phase,
        camera_normalized: [0.4, 0.6],
    }
}

fn request(id: &str, record: &str, time: u64) -> Request {
    Request {
        id: id.into(),
        surface: "desk".into(),
        user: "owner".into(),
        target: Target {
            organ: "local".into(),
            record: record.into(),
            revision: "revision-1".into(),
        },
        observations: vec![],
        prompt: "Explain the selected object".into(),
        status: Status::Queued,
        calibration_revision: 1,
        result: None,
        created_ms: time,
        updated_ms: time,
    }
}

fn request_status(runtime: &Runtime, id: &str) -> Option<Status> {
    runtime.requests.get(id).map(|request| request.status)
}

#[test]
fn projective_mapping_round_trips_corners_and_dense_interiors() {
    let quad = Quad([[0.08, 0.12], [0.92, 0.03], [0.78, 0.95], [0.2, 0.82]]);
    for y in 0..=20 {
        for x in 0..=20 {
            let point = [f64::from(x) / 20.0, f64::from(y) / 20.0];
            let output = quad.map(point, false).unwrap();
            let restored = quad.map(output, true).unwrap();
            assert!((DVec2::from_array(restored) - DVec2::from_array(point)).length() < 1e-10);
        }
    }
    assert!(quad.map([0.0, 0.0], true).is_err());
    assert!(quad.map([f64::NAN, 0.0], false).is_err());
    assert!(quad.map([1.1, 0.0], false).is_err());
    for quad in [
        Quad([[0.0, 0.0]; 4]),
        Quad([[0.0, 0.0], [1.0, 1.0], [1.0, 0.0], [0.0, 1.0]]),
    ] {
        assert!(quad.matrix().is_err());
    }
}

#[test]
fn physical_coordinates_are_explicit_meters_and_independent_of_canvas_poses() {
    let mut config = configuration();
    let surface = surface_mut(&mut config, "desk");
    surface.physical_pose_meters.position = [2.0, 3.0, 4.0];
    assert_eq!(surface.physical_point([0.5, 0.5]).unwrap(), [2.6, 3.4, 4.0]);
    assert_eq!(
        from_str::<Configuration>(&to_string(&config).unwrap()).unwrap(),
        config
    );
    assert!(config.validate().is_ok());
    config
        .devices
        .iter_mut()
        .find(|device| device.id == "camera")
        .expect("the fixture contains a camera")
        .kind = DeviceKind::Projector;
    assert!(config.validate().is_err());
}

#[test]
fn evidence_requires_consent_current_session_revision_freshness_and_sequence() {
    let mut runtime = runtime();
    runtime.capture("camera", false).unwrap();
    assert!(runtime.observe(1, evidence(1, 100), 100).is_err());
    runtime.capture("camera", true).unwrap();
    assert!(runtime.observe(2, evidence(1, 100), 100).is_err());
    let mut old = evidence(1, 100);
    old.calibration_revision = 2;
    assert!(runtime.observe(1, old, 100).is_err());
    runtime.observe(1, evidence(1, 100), 100).unwrap();
    assert!(runtime.observe(1, evidence(1, 100), 100).is_err());
    assert!(runtime.observe(1, evidence(2, 101), 100).is_err());
    runtime.disconnect("camera");
    assert!(runtime.observations.is_empty());
    runtime.connect("camera", "capture-2").unwrap();
    runtime.capture("camera", true).unwrap();
    assert!(runtime.observe(1, evidence(2, 100), 100).is_err());
    let mut fresh = evidence(2, 100);
    fresh.session = "capture-2".into();
    runtime.observe(1, fresh, 100).unwrap();
    runtime.advance(5101).unwrap();
    assert!(runtime.observations.is_empty());
    assert!(runtime.advance(100).is_err());
}

#[test]
fn explicit_selection_commit_and_loss_cancellation_are_separate_from_observation() {
    let mut runtime = runtime();
    assert!(
        runtime
            .interact(1, interaction(1, 0, Phase::End), 0)
            .is_err()
    );
    assert!(
        runtime
            .interact(1, interaction(1, 0, Phase::Begin), 0)
            .unwrap()
            .is_none()
    );
    let mut lost = interaction(2, 20, Phase::Update);
    lost.evidence.confidence = 0.2;
    assert!(runtime.interact(1, lost, 20).is_err());
    assert!(runtime.pointers.is_empty());
    assert!(
        runtime
            .interact(1, interaction(3, 30, Phase::End), 30)
            .is_err()
    );
    runtime
        .interact(1, interaction(3, 30, Phase::Begin), 30)
        .unwrap();
    let commit = runtime
        .interact(1, interaction(4, 40, Phase::End), 40)
        .unwrap()
        .unwrap();
    assert_eq!(commit.local, [0.4, 0.6]);
    assert!(runtime.pointers.is_empty());
    runtime
        .interact(1, interaction(5, 50, Phase::Begin), 50)
        .unwrap();
    runtime.advance(551).unwrap();
    assert!(runtime.pointers.is_empty());
    assert!(
        runtime
            .interact(1, interaction(6, 551, Phase::End), 551)
            .is_err()
    );
}

#[test]
fn blank_disconnect_recalibration_and_projected_evidence_cannot_commit() {
    let mut runtime = runtime();
    runtime
        .interact(1, interaction(1, 0, Phase::Begin), 0)
        .unwrap();
    runtime.blank(true);
    assert!(runtime.pointers.is_empty());
    runtime.blank(false);
    let mut projected = interaction(2, 0, Phase::Begin);
    projected.evidence.projected_content = true;
    assert!(runtime.interact(1, projected, 0).is_err());
    runtime
        .interact(1, interaction(2, 0, Phase::Begin), 0)
        .unwrap();
    runtime.disconnect("projector");
    assert!(runtime.pointers.is_empty());
    let mut config = runtime.configuration.clone();
    widen(surface_mut(&mut config, "desk"));
    assert!(runtime.configure(config.clone()).is_err());
    surface_mut(&mut config, "desk").calibration_revision += 1;
    runtime.configure(config).unwrap();
    assert!(runtime.blanked);
    assert!(runtime.connections.is_empty());
}

#[test]
fn requests_run_in_parallel_but_serialize_writes_and_reject_stale_results() {
    let mut runtime = runtime();
    let first = request("a", "record-1", 0);
    let second = request("b", "record-2", 0);
    let target = first.target.clone();
    let stale = Target {
        revision: "revision-2".into(),
        ..second.target.clone()
    };
    runtime.enqueue(first, 1, 0).unwrap();
    runtime.enqueue(second, 1, 0).unwrap();
    runtime.enqueue(request("c", "record-1", 0), 1, 0).unwrap();
    assert_eq!(runtime.dispatch(1, 0).unwrap().len(), 2);
    assert_eq!(request_status(&runtime, "c"), Some(Status::Queued));
    runtime.cancel("a").unwrap();
    assert!(
        runtime
            .finish("a", &target, "owner", "Late answer".into(), 10)
            .is_err()
    );
    assert_eq!(request_status(&runtime, "b"), Some(Status::Running));
    assert_eq!(runtime.dispatch(1, 10).unwrap().len(), 1);
    assert!(
        runtime
            .finish("b", &stale, "owner", "Stale answer".into(), 20)
            .is_err()
    );
    assert_eq!(request_status(&runtime, "b"), Some(Status::Failed));
    runtime
        .finish("c", &target, "owner", "Current answer".into(), 20)
        .unwrap();
    assert_eq!(
        runtime
            .requests
            .get("c")
            .and_then(|request| request.result.as_deref()),
        Some("Current answer")
    );
    assert!(runtime.dismiss("b").is_ok());
}

#[test]
fn bounded_observations_requests_and_expiry_do_not_accumulate_unlimited_work() {
    let mut runtime = runtime();
    let capacity = u64::try_from(MAX_OBSERVATIONS).expect("the observation limit fits a sequence");
    for sequence in 1..=capacity {
        runtime.observe(1, evidence(sequence, 0), 0).unwrap();
    }
    assert!(runtime.observe(1, evidence(1000, 0), 0).is_err());
    for i in 0..MAX_REQUESTS {
        runtime
            .enqueue(request(&format!("r-{i}"), &format!("record-{i}"), 0), 1, 0)
            .unwrap();
    }
    assert!(
        runtime
            .enqueue(request("overflow", "record-x", 0), 1, 0)
            .is_err()
    );
    assert_eq!(runtime.dispatch(1, 0).unwrap().len(), MAX_RUNNING);
    runtime.advance(120_001).unwrap();
    assert!(runtime.requests.values().all(Request::terminal));
    assert!(runtime.observations.is_empty());
}

#[test]
fn observed_content_is_not_a_command_and_expired_sources_do_not_start_work() {
    let mut runtime = runtime();
    let mut projected = evidence(1, 0);
    projected.projected_content = true;
    runtime.observe(1, projected, 0).unwrap();
    let mut scoped = request("a", "record-1", 0);
    scoped.observations.push("observation-1".into());
    assert!(runtime.enqueue(scoped.clone(), 1, 0).is_err());
    runtime.observe(1, evidence(2, 0), 0).unwrap();
    scoped.observations = vec!["observation-2".into()];
    runtime.enqueue(scoped, 1, 0).unwrap();
    assert!(runtime.dispatch(1, MAX_AGE_MS + 1).unwrap().is_empty());
    assert_eq!(request_status(&runtime, "a"), Some(Status::Canceled));
}

#[test]
fn observation_replacement_cannot_retarget_or_revive_queued_requests() {
    let mut runtime = runtime();
    runtime.observe(1, evidence(1, 0), 0).unwrap();
    let mut scoped = request("scoped", "record-1", 0);
    scoped.observations.push("observation-1".into());
    runtime.enqueue(scoped, 1, 0).unwrap();
    let mut replacement = evidence(2, 0);
    replacement.id = "observation-1".into();
    assert!(runtime.observe(1, replacement.clone(), 0).is_err());
    let later = MAX_AGE_MS + 1;
    replacement.captured_ms = later;
    assert!(runtime.observe(1, replacement.clone(), later).is_err());
    assert_eq!(request_status(&runtime, "scoped"), Some(Status::Canceled));
    assert!(runtime.dispatch(1, later).unwrap().is_empty());
    runtime.dismiss("scoped").unwrap();
    runtime.observe(1, replacement, later).unwrap();
}

#[test]
fn multiple_devices_and_recalibration_preserve_unrelated_surface_work() {
    let mut config = configuration();
    let [camera, projector] = config.devices.as_slice() else {
        panic!("the fixture contains a camera and a projector");
    };
    let [mut camera, mut projector] = [camera.clone(), projector.clone()];
    camera.id = "camera-2".into();
    projector.id = "projector-2".into();
    config.devices.extend([camera, projector]);
    let mut surface = surface_mut(&mut config, "desk").clone();
    surface.id = "wall".into();
    surface.camera = Some("camera-2".into());
    surface.projector = "projector-2".into();
    config.surfaces.push(surface);
    let mut runtime = Runtime::new(config.clone()).unwrap();
    for (device, session) in [
        ("camera", "capture-1"),
        ("projector", "output-1"),
        ("camera-2", "capture-2"),
        ("projector-2", "output-2"),
    ] {
        runtime.connect(device, session).unwrap();
    }
    runtime.capture("camera", true).unwrap();
    runtime.capture("camera-2", true).unwrap();
    runtime.blank(false);
    runtime
        .interact(1, interaction(1, 0, Phase::Begin), 0)
        .unwrap();
    let wall = surface_mut(&mut config, "wall");
    widen(wall);
    wall.calibration_revision += 1;
    runtime.configure(config).unwrap();
    assert!(runtime.connections.contains_key("camera"));
    assert!(!runtime.connections.contains_key("camera-2"));
    assert!(runtime.pointers.contains_key("hand-1"));
    assert!(!runtime.blanked);
    assert!(
        runtime
            .interact(1, interaction(2, 20, Phase::End), 20)
            .unwrap()
            .is_some()
    );
}

#[test]
fn serialized_replay_has_deterministic_results_and_bounded_admission()
-> Result<(), Box<dyn std::error::Error>> {
    let trace = Trace {
        workspace: 1,
        configuration: configuration(),
        steps: vec![
            Step {
                now_ms: 0,
                event: Event::Connect {
                    device: "camera".into(),
                    session: "capture-1".into(),
                },
            },
            Step {
                now_ms: 0,
                event: Event::Connect {
                    device: "projector".into(),
                    session: "output-1".into(),
                },
            },
            Step {
                now_ms: 0,
                event: Event::Capture {
                    device: "camera".into(),
                    enabled: true,
                },
            },
            Step {
                now_ms: 0,
                event: Event::Blank { enabled: false },
            },
            Step {
                now_ms: 0,
                event: Event::Interact {
                    interaction: interaction(1, 0, Phase::Begin),
                },
            },
            Step {
                now_ms: 20,
                event: Event::Interact {
                    interaction: interaction(2, 20, Phase::End),
                },
            },
            Step {
                now_ms: 19,
                event: Event::Blank { enabled: true },
            },
            Step {
                now_ms: 30,
                event: Event::Interact {
                    interaction: interaction(3, 30, Phase::End),
                },
            },
        ],
    };
    let decoded: Trace = from_slice(&to_vec(&trace)?)?;
    let (runtime, first) = trace.run()?;
    let (_, second) = decoded.run()?;
    assert_eq!(to_value(&first)?, to_value(&second)?);
    assert_eq!(first.commits.len(), 1);
    assert_eq!(first.accepted, 6);
    assert_eq!(
        first.errors,
        [
            (6, String::from("The adapter clock moved backwards.")),
            (7, String::from("Begin a deliberate selection first.")),
        ]
    );
    assert!(!runtime.blanked);
    assert_eq!(first.remaining_pointers, 0);
    let mut too_many = trace;
    too_many.steps = vec![
        Step {
            now_ms: 0,
            event: Event::Dispatch
        };
        513
    ];
    assert!(too_many.run().is_err());
    Ok(())
}
