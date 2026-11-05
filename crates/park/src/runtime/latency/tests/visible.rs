use super::*;
use crate::runtime::{CanvasWorker, Completed, Configuration, Input};
use bevy::{
    ecs::schedule::Schedules,
    input::keyboard::{Key, KeyCode},
    input_focus::{FocusCause, InputFocus},
    prelude::*,
};
use lince_interface::{
    host::{OutputViewport, Rectangle, raster::RasterMapping},
    native,
};
use std::sync::mpsc;

fn wait(worker: &CanvasWorker, serial: u64, length: Option<usize>) -> Completed {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        assert!(worker.failure().is_none());
        assert!(worker.is_running());
        if let Some(frame) = worker.frame()
            && frame.input_serial >= serial
            && length.is_none_or(|length| {
                frame
                    .frame
                    .input
                    .editor
                    .as_ref()
                    .is_some_and(|editor| editor.surrounding.len() == length)
            })
        {
            assert!(frame.frame.ready().unwrap());
            return frame;
        }
        assert!(
            Instant::now() < deadline,
            "The controlled editor response did not arrive"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
#[ignore = "private Vulkan/EGL input-visible trace; run with tracing on and off without concurrent compilation"]
fn controlled_typing_changes_pixels_after_editor_application_and_keeps_its_input_origin() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let mut worker = CanvasWorker::spawn(Configuration {
        showoff: false,
        size: [640, 480],
        directory: directory.path().into(),
        runtime: runtime.handle().clone(),
        cell: None,
    })
    .unwrap();
    wait(&worker, 0, None);
    let setup = worker
        .input(Input::Test(Box::new(|canvas| {
            let bounds = Rectangle::new([0.0; 2], [640.0, 480.0]).unwrap();
            canvas
                .viewport(OutputViewport {
                    rotation: Default::default(),
                    id: OutputId(1),
                    bounds,
                    usable: Some(bounds),
                    physical_size: [640, 480],
                    raster: RasterMapping {
                        origin: [0.0; 2],
                        scale: [1.0; 2],
                    },
                    workspace: 1,
                })
                .unwrap();
            let world = canvas.world_mut();
            let root = world
                .query_filtered::<Entity, With<native::workspace::Workspaces>>()
                .single(world)
                .unwrap();
            native::workspace_config::set_physics(world, root, 1, false);
            let editor = world
                .spawn((
                    native::sand::single_line_editor(
                        "",
                        world.resource::<native::theme::Typography>(),
                        0,
                        32,
                    ),
                    ChildOf(root),
                ))
                .id();
            let marker = world
                .spawn((
                    native::canvas::CanvasItem {
                        position: bevy::math::DVec2::ZERO,
                        size: Vec2::splat(96.0),
                    },
                    native::layout::LayoutBox::new(Vec2::splat(96.0)),
                    native::workspace::WorkspaceMember(1),
                    Node::default(),
                    BackgroundColor(Color::srgb(1.0, 0.0, 0.0)),
                    ChildOf(root),
                ))
                .id();
            world
                .resource_mut::<InputFocus>()
                .set(editor, FocusCause::Navigated);
            world
                .resource_mut::<Schedules>()
                .get_mut(PostUpdate)
                .unwrap()
                .add_systems(
                    (move |texts: Query<&bevy::text::EditableText>,
                           mut colors: Query<&mut BackgroundColor>| {
                        let length = texts.get(editor).unwrap().value().chars().count();
                        let color = if length % 2 == 0 {
                            Color::srgb(1.0, 0.0, 0.0)
                        } else {
                            Color::srgb(0.0, 1.0, 0.0)
                        };
                        colors
                            .get_mut(marker)
                            .unwrap()
                            .set_if_neq(BackgroundColor(color));
                    })
                    .after(bevy::text::EditableTextSystems)
                    .before(bevy::ui::UiSystems::Layout),
                );
        })))
        .unwrap();
    wait(&worker, setup, Some(0));
    let mut samples = Vec::new();
    for round in 0..12 {
        let (entered, started) = mpsc::sync_channel(1);
        let (release, resumed) = mpsc::sync_channel(1);
        if round == 0 {
            worker
                .input(Input::Test(Box::new(move |_| {
                    entered.send(()).unwrap();
                    resumed.recv_timeout(Duration::from_secs(10)).unwrap();
                })))
                .unwrap();
            started.recv_timeout(Duration::from_secs(10)).unwrap();
        }
        let begin = Instant::now();
        let (serial, origin) = {
            let scope = Dispatch::new();
            let origin = Stamp::current();
            let serial = worker
                .input(Input::Key {
                    physical: KeyCode::KeyQ,
                    logical: Key::Character("q".into()),
                    text: Some("q".into()),
                    pressed: true,
                    repeat: false,
                })
                .unwrap();
            drop(scope);
            (serial, origin)
        };
        if round == 0 {
            std::thread::sleep(Duration::from_millis(20));
            release.send(()).unwrap();
        }
        let frame = wait(&worker, serial, Some(round + 1));
        let pixels = crate::presentation::probe_pixels(frame.frame.dmabuf().unwrap()).unwrap();
        let expected = if round % 2 == 0 {
            [0_u8, 255, 0]
        } else {
            [255_u8, 0, 0]
        };
        let matching = pixels
            .chunks_exact(4)
            .filter(|pixel| {
                pixel[..3]
                    .iter()
                    .zip(expected)
                    .all(|(value, expected)| value.abs_diff(expected) < 20)
            })
            .count();
        assert!(
            matching > 1000,
            "The editor changed but its controlled visible response was missing: {matching}"
        );
        if let Some(origin) = origin {
            let timing = frame.latency.unwrap();
            let input = timing.input.unwrap();
            assert_eq!(input.stamp.id, origin.id);
            assert_eq!(input.input_serial, serial);
            timing.presented(OutputId(1), None, monotonic(), false);
        } else {
            assert!(frame.latency.is_none());
        }
        samples.push(serde_json::json!({"round":round,"held_queue":round==0,"origin":origin.map(|origin|origin.id),"input_serial":serial,"frame":frame.sequence,"matching_pixels":matching,"input_to_verified_readback_ms":begin.elapsed().as_secs_f64()*1000.0}));
        let serial = worker
            .input(Input::Key {
                physical: KeyCode::KeyQ,
                logical: Key::Character("q".into()),
                text: None,
                pressed: false,
                repeat: false,
            })
            .unwrap();
        wait(&worker, serial, Some(round + 1));
    }
    worker.stop().unwrap();
    let report = serde_json::json!({"scope":"Private controlled editor plus semantic-driven color oracle and GPU readback; observed completion, not physical scanout or normal daily timing","tracing":performance::enabled(),"samples":samples});
    if let Some(path) = std::env::var_os("LINCE_PARK_TEST_ARTIFACTS") {
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(
            std::path::Path::new(&path).join("native-input-visible.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
    }
    performance::flush();
    std::thread::sleep(Duration::from_millis(50));
}
