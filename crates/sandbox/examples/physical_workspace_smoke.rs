use bevy::{
    diagnostic::FrameCount,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    winit::WinitSettings,
};
use lince_sandbox::{
    canvas::{CanvasItem, CanvasView},
    container::BoxRoot,
    physical_workspace as physical,
};
use nucleus::physical_workspace as model;
use std::{path::PathBuf, time::Instant};

#[derive(Resource)]
struct Replay {
    directory: PathBuf,
    started: Instant,
    root: Option<Entity>,
    owner: Option<Entity>,
    window: Option<Entity>,
    phase: usize,
    next: u32,
    waiting: bool,
    images: Vec<Vec<u8>>,
    receipt: Option<String>,
    after_move: Option<nucleus::canvas::Snapshot>,
}

fn input(
    world: &World,
    root: Entity,
    sequence: u64,
    phase: model::Phase,
    point: [f64; 2],
) -> model::Interaction {
    let session = &world.get::<physical::Installation>(root).unwrap().0[&1];
    model::Interaction {
        evidence: model::Evidence {
            id: format!("replay-{sequence}"),
            device: "replay-camera".into(),
            session: "replay-session".into(),
            sequence,
            captured_ms: session.now_ms(),
            surface: "desk".into(),
            calibration_revision: 1,
            confidence: 0.95,
            media: None,
            projected_content: false,
        },
        pointer: "deliberate-replay-pointer".into(),
        phase,
        camera_normalized: point,
    }
}

fn exercise(world: &mut World) {
    let frame = world.resource::<FrameCount>().0;
    assert!(
        world.resource::<Replay>().started.elapsed().as_secs() < 600,
        "Physical workspace replay timed out"
    );
    if world.resource::<Replay>().waiting || frame < world.resource::<Replay>().next {
        return;
    }
    let root = if let Some(root) = world.resource::<Replay>().root {
        root
    } else {
        let Some(root) = world
            .query_filtered::<Entity, With<BoxRoot>>()
            .iter(world)
            .next()
        else {
            return;
        };
        let Some(mut editing) = world.get_mut::<lince_sandbox::edit_mode::EditMode>(root) else {
            return;
        };
        editing.enabled = true;
        assert!(lince_sandbox::workspace_config::set_physics(
            world, root, 1, false
        ));
        let owner = lince_sandbox::sand_store::spawn_sand(
            world,
            root,
            1,
            lince_sandbox::sand_store::SandKind::Square,
            "Public task",
            bevy::math::DVec2::new(100.0, 100.0),
        );
        world.entity_mut(owner).insert((Name::new("Assigned task"), lince_sandbox::area::RecordProperties(serde_json::json!({"uid":nucleus::new_uid("r"),"head":"Projected task","quantity":-3})), lince_sandbox::shader_effects::Surface { effect: nucleus::shader_effect::Effect::default(), quantity: "-3".into(), motion: false, glow: 0.0, ..default() }));
        lince_sandbox::sand_store::spawn_sand(
            world,
            root,
            1,
            lince_sandbox::sand_store::SandKind::Square,
            "UNASSIGNED PRIVATE SENTINEL",
            bevy::math::DVec2::new(-300.0, -100.0),
        );
        world
            .entity_mut(root)
            .insert(lince_sandbox::canvas_selection::SandSelection(vec![owner]));
        let mut saved = physical::replay_configuration(1);
        saved.configuration.surfaces[0].projector_quad =
            model::Quad([[0.08, 0.10], [0.92, 0.04], [0.82, 0.94], [0.15, 0.86]]);
        physical::configure(world, root, saved).unwrap();
        physical::connect(world, root, 1, "preview-projector", "output-session").unwrap();
        physical::connect(world, root, 1, "replay-camera", "replay-session").unwrap();
        physical::capture(world, root, 1, "replay-camera", true).unwrap();
        physical::bind_selection(world, root, 1, "desk").unwrap();
        physical::open_output(world, root, 1, "desk").unwrap();
        let window = world
            .query::<(Entity, &Window)>()
            .iter(world)
            .find(|(_, w)| w.title.starts_with("Lince surface"))
            .unwrap()
            .0;
        let directory = world.resource::<Replay>().directory.clone();
        std::fs::write(
            directory.join("physical-workspace.json"),
            serde_json::to_vec_pretty(&physical::snapshot(world, root)).unwrap(),
        )
        .unwrap();
        let mut replay = world.resource_mut::<Replay>();
        replay.root = Some(root);
        replay.owner = Some(owner);
        replay.window = Some(window);
        replay.next = frame + 60;
        return;
    };
    let phase = world.resource::<Replay>().phase;
    let owner = world.resource::<Replay>().owner.unwrap();
    if phase < 3
        && !lince_sandbox::shader_effects::surface_reading(world, owner)
            .is_some_and(|r| r.ready && r.image_available)
    {
        return;
    }
    if phase == 4 {
        assert!(
            world
                .query::<&Window>()
                .iter(world)
                .all(|w| !w.title.starts_with("Lince surface"))
        );
        let replay = world.resource::<Replay>();
        let [first, second, third, blank] = replay.images.as_slice() else {
            panic!("Expected four output captures")
        };
        assert!(
            blank
                .chunks_exact(4)
                .all(|pixel| pixel[..3].iter().all(|value| *value < 8)),
            "Blanking must retain a black output window"
        );
        let difference = first
            .chunks_exact(4)
            .zip(second.chunks_exact(4))
            .filter(|(a, b)| a[..3].iter().zip(&b[..3]).any(|(a, b)| a.abs_diff(*b) > 12))
            .count();
        assert!(
            difference > 100,
            "Quantity-driven shader pixels did not change: {difference}"
        );
        let movement = second
            .chunks_exact(4)
            .zip(third.chunks_exact(4))
            .filter(|(a, b)| a[..3].iter().zip(&b[..3]).any(|(a, b)| a.abs_diff(*b) > 12))
            .count();
        assert!(
            movement > 500,
            "Projected placement did not move: {movement}"
        );
        println!(
            "Physical software replay passed: calibrated surface output, assigned content, live WGSL quantity, receipt movement, normal Undo and window/resource cleanup; changed pixels {difference}/{movement}"
        );
        world.write_message(AppExit::Success);
        return;
    }
    let window = world.resource::<Replay>().window.unwrap();
    let path = world
        .resource::<Replay>()
        .directory
        .join(format!("physical-surface-{phase}.png"));
    world.resource_mut::<Replay>().waiting = true;
    world
        .spawn(Screenshot::window(window))
        .observe(save_to_disk(path))
        .observe(
            move |event: On<ScreenshotCaptured>,
                  mut replay: ResMut<Replay>,
                  mut commands: Commands| {
                use bevy::render::render_resource::TextureFormat;
                let mut pixels = event.image.data.as_ref().unwrap().clone();
                if matches!(
                    event.image.texture_descriptor.format,
                    TextureFormat::Bgra8Unorm | TextureFormat::Bgra8UnormSrgb
                ) {
                    for p in pixels.chunks_exact_mut(4) {
                        p.swap(0, 2);
                    }
                }
                assert!(
                    phase == 3
                        || pixels
                            .chunks_exact(4)
                            .filter(|p| p[0] > 40 || p[1] > 40 || p[2] > 40)
                            .count()
                            > 1500,
                    "Output is blank"
                );
                assert!(
                    pixels[..3].iter().all(|v| *v < 8),
                    "Pixels outside calibration should be black"
                );
                replay.images.push(pixels);
                commands.queue(move |world: &mut World| {
                    match phase {
                        0 => {
                            world
                                .get_mut::<lince_sandbox::shader_effects::Surface>(owner)
                                .unwrap()
                                .quantity = "0".into();
                            world
                                .get_mut::<lince_sandbox::area::RecordProperties>(owner)
                                .unwrap()
                                .0["quantity"] = 0.into();
                        }
                        1 => {
                            let begin = input(world, root, 1, model::Phase::Begin, [0.4, 0.4]);
                            physical::interact(world, root, 1, begin).unwrap();
                            let end = input(world, root, 2, model::Phase::End, [0.5, 0.5]);
                            let receipt = physical::interact(world, root, 1, end)
                                .unwrap()
                                .unwrap()
                                .receipt()
                                .unwrap();
                            world.resource_mut::<Replay>().receipt = Some(receipt);
                            let after = lince_sandbox::canvas_host::capture(world, root).unwrap();
                            world.resource_mut::<Replay>().after_move = Some(after);
                            assert_ne!(
                                world.get::<CanvasItem>(owner).unwrap().position,
                                bevy::math::DVec2::new(100.0, 100.0)
                            );
                        }
                        2 => {
                            let current = lince_sandbox::canvas_host::capture(world, root).unwrap();
                            let replay = world.resource::<Replay>();
                            std::fs::write(
                                replay.directory.join("after-move.json"),
                                serde_json::to_vec_pretty(&replay.after_move).unwrap(),
                            )
                            .unwrap();
                            std::fs::write(
                                replay.directory.join("before-undo.json"),
                                serde_json::to_vec_pretty(&current).unwrap(),
                            )
                            .unwrap();
                            assert!(world.resource_mut::<Replay>().receipt.take().is_some());
                            physical::undo(world, root, 1).unwrap();
                            assert_eq!(
                                world.get::<CanvasItem>(owner).unwrap().position,
                                bevy::math::DVec2::new(100.0, 100.0)
                            );
                            physical::blank(world, root);
                        }
                        3 => physical::close_outputs(world, root),
                        _ => unreachable!(),
                    }
                    let frame = world.resource::<FrameCount>().0;
                    let mut replay = world.resource_mut::<Replay>();
                    replay.phase += 1;
                    replay.waiting = false;
                    replay.next = frame + 60;
                });
            },
        );
}

fn main() -> AppExit {
    let directory = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp/lince-physical-workspace"));
    std::fs::create_dir_all(&directory).unwrap();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let _guard = runtime.enter();
    let mut app = lince_sandbox::app::interface_app();
    app.insert_resource(lince_sandbox::notifications::Notifications::new(
        cell::Diagnostics::default(),
    ));
    app.add_plugins(bevy::log::LogPlugin::default())
        .insert_resource(WinitSettings::continuous())
        .insert_resource(Replay {
            directory,
            started: Instant::now(),
            root: None,
            owner: None,
            window: None,
            phase: 0,
            next: 10,
            waiting: false,
            images: Vec::new(),
            receipt: None,
            after_move: None,
        });
    app.add_systems(Startup, |mut commands: Commands| {
        commands.spawn((BoxRoot, CanvasView::default()));
    });
    app.add_systems(
        Update,
        exercise.after(lince_sandbox::workspace::PrepareWorkspaces),
    );
    app.run()
}
