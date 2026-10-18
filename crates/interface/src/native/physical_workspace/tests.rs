use super::*;

fn setup() -> (App, Entity) {
    let (mut app, root) = crate::native::edit_mode::tests::fixture();
    app.world_mut()
        .get_mut::<crate::native::edit_mode::EditMode>(root)
        .unwrap()
        .enabled = true;
    (app, root)
}

#[test]
fn physical_configuration_restore_never_resumes_capture_or_projection() {
    let (mut app, root) = setup();
    let world = app.world_mut();
    configure(world, root, replay_configuration(1)).unwrap();
    connect(world, root, 1, "replay-camera", "session-1").unwrap();
    capture(world, root, 1, "replay-camera", true).unwrap();
    let saved = snapshot(&world, root);
    let encoded = serde_json::to_vec(&saved).unwrap();
    restore(world, root, serde_json::from_slice(&encoded).unwrap());
    let session = &world.get::<Installation>(root).unwrap().0[&1];
    assert!(session.runtime.blanked);
    assert!(session.runtime.connections.is_empty());
    assert!(session.runtime.observations.is_empty());
    assert_eq!(snapshot(&world, root), saved);
}

#[test]
fn physical_controls_reject_foreign_workspace_non_editing_and_invalid_configuration() {
    let (mut app, root) = setup();
    let world = app.world_mut();
    assert!(configure(world, root, replay_configuration(2)).is_err());
    world
        .get_mut::<crate::native::edit_mode::EditMode>(root)
        .unwrap()
        .enabled = false;
    assert!(configure(world, root, replay_configuration(1)).is_err());
    world
        .get_mut::<crate::native::edit_mode::EditMode>(root)
        .unwrap()
        .enabled = true;
    let mut saved = replay_configuration(1);
    saved.configuration.surfaces[0].projector_quad.0[1] = [f64::NAN, 0.0];
    assert!(configure(world, root, saved).is_err());
    assert!(world.get::<Installation>(root).is_none());
    configure(world, root, replay_configuration(1)).unwrap();
    assert!(open_output(world, root, 1, "desk").is_err());
}

#[test]
fn physical_workspace_switch_revokes_input_and_cancels_queued_work() {
    let (mut app, root) = setup();
    let world = app.world_mut();
    configure(world, root, replay_configuration(1)).unwrap();
    connect(world, root, 1, "replay-camera", "session-1").unwrap();
    capture(world, root, 1, "replay-camera", true).unwrap();
    world.get_mut::<Workspaces>(root).unwrap().active = 2;
    tick(world);
    let session = &world.get::<Installation>(root).unwrap().0[&1];
    assert!(session.runtime.blanked);
    assert!(session.runtime.connections.is_empty());
}

fn evidence(world: &World, root: Entity, sequence: u64) -> model::Evidence {
    let session = &world.get::<Installation>(root).unwrap().0[&1];
    let id = format!("sample-{sequence}");
    model::Evidence {
        id: id.clone(),
        device: "replay-camera".into(),
        session: "camera-session".into(),
        sequence,
        captured_ms: session.now_ms(),
        surface: "desk".into(),
        calibration_revision: 1,
        confidence: 0.95,
        media: Some(id),
        projected_content: false,
    }
}

fn connected(world: &mut World, root: Entity) {
    configure(world, root, replay_configuration(1)).unwrap();
    connect(world, root, 1, "replay-camera", "camera-session").unwrap();
    connect(world, root, 1, "preview-projector", "projector-session").unwrap();
    capture(world, root, 1, "replay-camera", true).unwrap();
    world
        .get_mut::<Installation>(root)
        .unwrap()
        .0
        .get_mut(&1)
        .unwrap()
        .runtime
        .blank(false);
}

#[test]
fn physical_surface_movement_uses_canvas_receipts_and_normal_undo() {
    let (mut app, root) = setup();
    let world = app.world_mut();
    world.init_resource::<Assets<Image>>();
    let owner = crate::native::sand_store::spawn_sand(
        world,
        root,
        1,
        crate::native::sand_store::SandKind::Square,
        "Assigned task",
        bevy::math::DVec2::new(120.0, 80.0),
    );
    world
        .entity_mut(root)
        .insert(crate::native::canvas_selection::SandSelection(vec![owner]));
    connected(world, root);
    bind_selection(world, root, 1, "desk").unwrap();
    let binding = world.get::<Installation>(root).unwrap().0[&1].bindings[0].clone();
    let begin = model::Interaction {
        evidence: evidence(world, root, 1),
        pointer: "hand".into(),
        phase: model::Phase::Begin,
        camera_normalized: [0.4, 0.4],
    };
    assert!(interact(world, root, 1, begin).unwrap().is_none());
    let end = model::Interaction {
        evidence: evidence(world, root, 2),
        pointer: "hand".into(),
        phase: model::Phase::End,
        camera_normalized: [0.5, 0.5],
    };
    let receipt = interact(world, root, 1, end)
        .unwrap()
        .unwrap()
        .receipt()
        .unwrap();
    let position = world
        .get::<crate::native::canvas::CanvasItem>(owner)
        .unwrap()
        .position;
    assert!((position.x - 120.0 - 0.1 * binding.canvas_span[0]).abs() < 1e-8);
    let revision = canvas_host::capture(world, root).unwrap().revision;
    canvas_host::handle(
        world,
        root,
        &canvas_api::Request::Mutate {
            request_id: nucleus::new_uid("request"),
            expected_revision: revision,
            mutation: canvas_api::Mutation::Undo { receipt },
        },
    )
    .unwrap();
    let restored = world
        .get::<crate::native::canvas::CanvasItem>(owner)
        .unwrap()
        .position;
    assert_eq!(restored, bevy::math::DVec2::new(120.0, 80.0));
    assert_eq!(binding.rectangle_at(restored.to_array()), binding.rectangle);
}

#[test]
fn physical_media_is_bounded_ephemeral_and_requires_camera_consent() {
    let (mut app, root) = setup();
    let world = app.world_mut();
    connected(world, root);
    let frame = || Frame {
        size: [8, 8],
        rgb: vec![80; 8 * 8 * 3],
    };
    let first = evidence(world, root, 1);
    submit_frame(world, root, 1, first, frame()).unwrap();
    assert_eq!(
        world.get::<Installation>(root).unwrap().0[&1].media.len(),
        1
    );
    let saved = serde_json::to_string(&snapshot(world, root)).unwrap();
    assert!(!saved.contains("image/jpeg"));
    assert!(!saved.contains("sample-1"));
    let malformed = Frame {
        size: [1920, 1920],
        rgb: Vec::new(),
    };
    let second = evidence(world, root, 2);
    assert!(submit_frame(world, root, 1, second, malformed).is_err());
    capture(world, root, 1, "replay-camera", false).unwrap();
    assert!(
        world.get::<Installation>(root).unwrap().0[&1]
            .media
            .is_empty()
    );
    let second = evidence(world, root, 2);
    assert!(submit_frame(world, root, 1, second, frame()).is_err());
}

#[test]
fn physical_inspector_exposes_explicit_capture_output_and_recovery_controls() {
    let (mut app, root) = setup();
    let world = app.world_mut();
    let parent = world.spawn(Node::default()).id();
    populate_controls(world, root, parent);
    Control::Open.apply(world, parent);
    let panel = world
        .query::<(Entity, &Panel)>()
        .iter(world)
        .find(|(_, p)| p.root == root)
        .unwrap()
        .0;
    Control::Replay.apply(world, panel);
    let labels: Vec<_> = world
        .query::<&Text>()
        .iter(world)
        .map(|text| text.0.clone())
        .collect();
    for label in [
        "Apply configuration",
        "Blank output",
        "Close output windows",
        "Stop all capture",
        "Request Fiote with latest frame",
        "Undo physical movement",
        "Local camera index",
    ] {
        assert!(labels.iter().any(|text| text == label), "Missing {label}");
    }
    Control::Stop.apply(world, panel);
    assert!(
        !world.get::<Installation>(root).unwrap().0[&1]
            .runtime
            .connections
            .values()
            .any(|c| c.capture_enabled)
    );
}

#[test]
fn physical_projection_ignores_virtual_room_relocation_and_preserves_local_gesture_axes() {
    use crate::native::{
        sand_store::{SandKind, spawn_sand},
        topology::{self, rooms},
    };
    let (mut app, root) = setup();
    let world = app.world_mut();
    world.init_resource::<Assets<Image>>();
    let room = spawn_sand(
        world,
        root,
        1,
        SandKind::Square,
        "Virtual room",
        bevy::math::DVec2::ZERO,
    );
    world
        .entity_mut(room)
        .insert(rooms::Room(Default::default()));
    canvas_host::capture(world, root).unwrap();
    let room_id = world.get::<canvas_host::Identity>(room).unwrap().0.clone();
    let owner = spawn_sand(
        world,
        root,
        1,
        SandKind::Square,
        "Task",
        bevy::math::DVec2::ZERO,
    );
    world
        .entity_mut(owner)
        .insert(rooms::Member(canvas_api::room::Member {
            room: room_id,
            parent: None,
            local: canvas_api::Pose {
                position: [100.0, 30.0, 80.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
            },
            role: canvas_api::room::Role::Content,
        }));
    rooms::synchronize(world);
    world
        .entity_mut(root)
        .insert(crate::native::canvas_selection::SandSelection(vec![owner]));
    connected(world, root);
    bind_selection(world, root, 1, "desk").unwrap();
    let binding = world.get::<Installation>(root).unwrap().0[&1].bindings[0].clone();
    let before = coordinates(world, root, 1, &binding.placement)
        .unwrap()
        .local;
    topology::set_position(world, room, bevy::math::DVec3::new(1e9, 400.0, -1e9));
    let mut spatial = topology::spatial(world, room);
    spatial.rotation = bevy::math::DQuat::from_euler(EulerRot::XYZ, 0.3, 0.6, 0.2).to_array();
    world.entity_mut(room).insert(spatial);
    rooms::synchronize(world);
    let after = coordinates(world, root, 1, &binding.placement)
        .unwrap()
        .local;
    assert!(
        (bevy::math::DVec3::from_array(before.position)
            - bevy::math::DVec3::from_array(after.position))
        .length()
            < 1e-6
    );
    let begin = model::Interaction {
        evidence: evidence(world, root, 1),
        pointer: "hand".into(),
        phase: model::Phase::Begin,
        camera_normalized: [0.4, 0.4],
    };
    interact(world, root, 1, begin).unwrap();
    let end = model::Interaction {
        evidence: evidence(world, root, 2),
        pointer: "hand".into(),
        phase: model::Phase::End,
        camera_normalized: [0.5, 0.5],
    };
    interact(world, root, 1, end).unwrap();
    let local = &world.get::<rooms::Member>(owner).unwrap().0.local.position;
    assert!((local[0] - before.position[0] - 0.1 * binding.canvas_span[0]).abs() < 1e-5);
    assert!((local[1] - before.position[1]).abs() < 1e-5);
    assert!((local[2] - before.position[2] - 0.1 * binding.canvas_span[1]).abs() < 1e-5);
}

#[tokio::test]
async fn physical_adapter_protocol_scopes_frames_and_movements_and_rejects_unpaired_input() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    async fn exchange(
        stream: &mut tokio::net::TcpStream,
        token: &str,
        sequence: u64,
        command: adapter::Command,
    ) -> adapter::Reply {
        let bytes = serde_json::to_vec(&adapter::Packet {
            version: adapter::PROTOCOL_VERSION,
            token: token.into(),
            sequence,
            command,
        })
        .unwrap();
        stream.write_u32(bytes.len() as u32).await.unwrap();
        stream.write_all(&bytes).await.unwrap();
        let size = stream.read_u32().await.unwrap();
        assert!(size <= 16_384);
        let mut bytes = vec![0; size as usize];
        stream.read_exact(&mut bytes).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }
    let (mut app, root) = setup();
    let world = app.world_mut();
    world.init_resource::<Assets<Image>>();
    world.init_resource::<adapter::Endpoints>();
    let owner = crate::native::sand_store::spawn_sand(
        world,
        root,
        1,
        crate::native::sand_store::SandKind::Square,
        "Task",
        bevy::math::DVec2::ZERO,
    );
    world
        .entity_mut(root)
        .insert(crate::native::canvas_selection::SandSelection(vec![owner]));
    connected(world, root);
    bind_selection(world, root, 1, "desk").unwrap();
    let descriptor = adapter::start(world, root, 1, "desk").unwrap();
    let address = descriptor.address.clone();
    let client = tokio::spawn(async move {
        use base64::{Engine, prelude::BASE64_STANDARD};
        let mut stream = tokio::net::TcpStream::connect(&descriptor.address)
            .await
            .unwrap();
        let clock = exchange(&mut stream, &descriptor.token, 1, adapter::Command::Clock).await;
        assert!(clock.error.is_none());
        let evidence = |sequence, now_ms| {
            let id = format!("adapter-frame-{sequence}");
            model::Evidence {
                id: id.clone(),
                device: descriptor.device.clone(),
                session: descriptor.session.clone(),
                sequence,
                captured_ms: now_ms,
                surface: "desk".into(),
                calibration_revision: descriptor.calibration_revision,
                confidence: 0.99,
                media: Some(id),
                projected_content: false,
            }
        };
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new(&mut jpeg)
            .encode(&vec![100; 8 * 8 * 3], 8, 8, image::ExtendedColorType::Rgb8)
            .unwrap();
        let frame = exchange(
            &mut stream,
            &descriptor.token,
            2,
            adapter::Command::Frame {
                evidence: evidence(1, clock.now_ms),
                jpeg: BASE64_STANDARD.encode(jpeg),
            },
        )
        .await;
        assert!(frame.error.is_none(), "{:?}", frame.error);
        let begin = exchange(
            &mut stream,
            &descriptor.token,
            3,
            adapter::Command::Interact {
                interaction: model::Interaction {
                    evidence: evidence(2, frame.now_ms),
                    pointer: "adapter-pointer".into(),
                    phase: model::Phase::Begin,
                    camera_normalized: [0.4, 0.4],
                },
            },
        )
        .await;
        assert!(begin.error.is_none(), "{:?}", begin.error);
        let end = exchange(
            &mut stream,
            &descriptor.token,
            4,
            adapter::Command::Interact {
                interaction: model::Interaction {
                    evidence: evidence(3, begin.now_ms),
                    pointer: "adapter-pointer".into(),
                    phase: model::Phase::End,
                    camera_normalized: [0.5, 0.5],
                },
            },
        )
        .await;
        assert!(end.error.is_none(), "{:?}", end.error);
        assert!(matches!(
            end.outcome,
            Some(InteractionOutcome::Placement { .. })
        ));
    });
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while !client.is_finished() {
            adapter::tick(app.world_mut());
            tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
    client.await.unwrap();
    assert_ne!(
        app.world()
            .get::<crate::native::canvas::CanvasItem>(owner)
            .unwrap()
            .position,
        bevy::math::DVec2::ZERO
    );
    assert_eq!(
        app.world().get::<Installation>(root).unwrap().0[&1]
            .media
            .len(),
        1
    );
    let mut unpaired = tokio::net::TcpStream::connect(&address).await.unwrap();
    let bytes = serde_json::to_vec(&adapter::Packet {
        version: adapter::PROTOCOL_VERSION,
        token: "wrong".into(),
        sequence: 1,
        command: adapter::Command::Clock,
    })
    .unwrap();
    unpaired.write_u32(bytes.len() as u32).await.unwrap();
    unpaired.write_all(&bytes).await.unwrap();
    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(2), unpaired.read_u8())
            .await
            .unwrap()
            .is_err()
    );
    let mut oversized = tokio::net::TcpStream::connect(&address).await.unwrap();
    oversized.write_u32(1_048_577).await.unwrap();
    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(2), oversized.read_u8())
            .await
            .unwrap()
            .is_err()
    );
    adapter::stop(app.world_mut(), root, 1);
    assert!(
        !app.world().get::<Installation>(root).unwrap().0[&1]
            .runtime
            .connections
            .contains_key("replay-camera")
    );
}
