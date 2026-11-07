use super::*;
use bevy::{
    input::keyboard::Key,
    input_focus::{FocusCause, InputFocus},
    math::DVec2,
};
use lince_interface::{
    host::{
        DesktopAction, OutputId, WorkspaceLayout,
        performance::{self, Phase},
    },
    native::{
        actions::Action,
        sand_store::{SandKind, spawn_sand},
    },
};

fn advance(canvas: &mut Presentation, frames: usize) {
    for _ in 0..frames {
        let frame = canvas.probe_frame().unwrap();
        assert!(frame.ready().unwrap());
        assert!(frame.geometry.is_some());
    }
}

#[test]
#[ignore = "precompiled private mixed native performance route; requires Vulkan/EGL and no competing renderer"]
fn mixed_daily_phases_publish_measured_work_and_complete_pixels() {
    let _observer = crate::presentation::observe_systems();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let _entered = runtime.enter();
    performance::set_phase(Phase::Warmup);
    let directory = tempfile::tempdir().unwrap();
    let size = [960, 640];
    let mut canvas =
        super::super::appearance::launch(&directory.path().join("workspace.json"), size);
    let root = canvas.canvas_root().unwrap();
    for _ in 0..2 {
        native::workspace::create(canvas.world_mut(), root);
    }
    canvas
        .outputs(
            (1..=2)
                .map(|id| {
                    let mut output = super::super::transfers::output(
                        id,
                        (id - 1) as f64 * f64::from(size[0]),
                        size,
                    );
                    output.viewport.workspace = id;
                    output
                })
                .collect(),
        )
        .unwrap();
    for workspace in 1..=3 {
        native::workspace_config::set_physics(canvas.world_mut(), root, workspace, false);
    }
    let kinds = [
        SandKind::Square,
        SandKind::Text,
        SandKind::EditableText,
        SandKind::Operation,
        SandKind::WorkTimer,
        SandKind::Text,
    ];
    let mut items = Vec::new();
    for index in 0..24 {
        let workspace = if index < 8 {
            1
        } else if index < 16 {
            2
        } else if index < 20 {
            1
        } else {
            3
        };
        let position = if index < 16 {
            DVec2::new(
                (index % 4) as f64 * 220.0 - 330.0,
                (index % 8 / 4) as f64 * 240.0 - 120.0,
            )
        } else {
            DVec2::new(10000.0 + index as f64 * 240.0, 0.0)
        };
        let kind = kinds[index % kinds.len()];
        let text = if matches!(kind, SandKind::EditableText | SandKind::WorkTimer) {
            ""
        } else {
            "Park measured native text"
        };
        let item = spawn_sand(canvas.world_mut(), root, workspace, kind, text, position);
        canvas
            .world_mut()
            .get_mut::<native::canvas::CanvasItem>(item)
            .unwrap()
            .size = Vec2::new(200.0, 160.0);
        if let Some(mut layout) = canvas
            .world_mut()
            .get_mut::<native::layout::LayoutBox>(item)
        {
            layout.rules = lince_interface::layout::Rules::fixed([200.0, 160.0]);
        } else {
            let mut layout = native::layout::LayoutBox::new(Vec2::new(200.0, 160.0));
            layout.rules = lince_interface::layout::Rules::fixed([200.0, 160.0]);
            canvas.world_mut().entity_mut(item).insert(layout);
        }
        items.push(item);
    }
    canvas.focus_output(OutputId(1)).unwrap();
    canvas
        .desktop_action(DesktopAction::SetLayout(WorkspaceLayout::Free))
        .unwrap();
    canvas.desktop_action(DesktopAction::Unfocus).unwrap();
    let viewport = canvas
        .world_mut()
        .spawn((
            Node {
                width: px(180.0),
                height: px(140.0),
                flex_direction: FlexDirection::Column,
                overflow: Overflow::scroll_y(),
                ..default()
            },
            ScrollPosition::default(),
            ChildOf(items[0]),
        ))
        .id();
    native::scroll_sand::attach(canvas.world_mut(), viewport);
    for row in 0..48 {
        let font = canvas
            .app
            .world()
            .resource::<native::theme::Typography>()
            .text(12.0);
        canvas.world_mut().spawn((
            Text::new(format!("Measured row {row}")),
            font,
            Node {
                height: px(24.0),
                min_height: px(24.0),
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(viewport),
        ));
    }
    advance(&mut canvas, 16);
    let first = canvas.probe_frame().unwrap();
    assert_eq!(first.outputs.len() + usize::from(first.output.is_some()), 2);
    for output in frame_pixels(&first) {
        assert!(
            output
                .chunks_exact(4)
                .any(|pixel| pixel[0] != pixel[1] || pixel[1] != pixel[2]),
            "Missing complete colored canvas content"
        );
    }
    drop(first);
    performance::set_phase(Phase::Quiet);
    std::thread::sleep(std::time::Duration::from_millis(250));
    performance::set_phase(Phase::Typing);
    let editor = {
        let world = canvas.world_mut();
        let mut query = world.query::<(Entity, &bevy::text::EditableText)>();
        query
            .iter(world)
            .find_map(|(entity, text)| {
                if !text.value().is_empty() {
                    return None;
                }
                let mut current = entity;
                loop {
                    if current == items[2] {
                        return Some(entity);
                    }
                    current = world.get::<ChildOf>(current)?.parent();
                }
            })
            .unwrap()
    };
    canvas
        .world_mut()
        .resource_mut::<InputFocus>()
        .set(editor, FocusCause::Navigated);
    let mut expected = String::new();
    for _ in 0..24 {
        canvas
            .key(
                KeyCode::KeyE,
                Key::Character("é".into()),
                Some("é".into()),
                true,
                false,
            )
            .unwrap();
        canvas
            .key(
                KeyCode::KeyE,
                Key::Character("é".into()),
                None,
                false,
                false,
            )
            .unwrap();
        expected.push('é');
        advance(&mut canvas, 1);
        assert_eq!(
            canvas
                .app
                .world()
                .get::<bevy::text::EditableText>(editor)
                .unwrap()
                .value(),
            expected
        );
    }
    performance::set_phase(Phase::Scroll);
    let (output, points) = canvas.native_test_points(viewport).unwrap();
    canvas.focus_output(output).unwrap();
    let center = std::array::from_fn(|axis| {
        (points.iter().map(|point| point[axis]).sum::<f64>() / 4.0) as f32
    });
    canvas.pointer(center).unwrap();
    for _ in 0..24 {
        canvas
            .scroll(
                [0.0, -16.0],
                bevy::input::mouse::MouseScrollUnit::Pixel,
                bevy::input::touch::TouchPhase::Moved,
            )
            .unwrap();
        advance(&mut canvas, 1);
    }
    assert!(
        canvas
            .app
            .world()
            .get::<ScrollPosition>(viewport)
            .unwrap()
            .0
            .y
            > 0.0
    );
    performance::set_phase(Phase::Pan);
    let original = canvas
        .app
        .world()
        .get::<native::canvas::CanvasView>(root)
        .unwrap()
        .clone();
    for offset in 0..24 {
        canvas
            .world_mut()
            .get_mut::<native::canvas::CanvasView>(root)
            .unwrap()
            .center = original.center + DVec2::new(offset as f64 * 2.0, 0.0);
        advance(&mut canvas, 1);
    }
    *canvas
        .world_mut()
        .get_mut::<native::canvas::CanvasView>(root)
        .unwrap() = original;
    performance::set_phase(Phase::OpenReveal);
    for _ in 0..3 {
        native::edit_mode::EditAction::Open.apply(canvas.world_mut(), root);
        advance(&mut canvas, 2);
        native::edit_mode::EditAction::Close.apply(canvas.world_mut(), root);
        advance(&mut canvas, 2);
    }
    performance::set_phase(Phase::Active);
    advance(&mut canvas, 48);
    let final_pixels = frame_pixels(&canvas.probe_frame().unwrap());
    assert!(final_pixels.iter().all(|pixels| !pixels.is_empty()));
    assert_eq!(
        canvas
            .app
            .world()
            .get::<bevy::text::EditableText>(editor)
            .unwrap()
            .value(),
        expected
    );
    performance::set_phase(Phase::ReturnQuiet);
    std::thread::sleep(std::time::Duration::from_millis(250));
    eprintln!(
        "Park workload {}",
        serde_json::json!({"identity":"park-a0-combined-v1", "top_level_items":24,"outputs":2,"physical":false,"external_clients":false,"families":["square-scroll-list","plain-text","editable-text","operation","work-timer"],"missing":["file-backed-record-save","external-client-cohort","physical-presentation","3d-in-this-scene"],"pan_scope":"production-transform-derivation; not-device-to-visible-latency","quiet_scope":"no-forced-private-frame; not-installed-worker-idle-acceptance"})
    );
    assert!(
        performance::finish(std::time::Duration::from_secs(2)),
        "The final diagnostic report was not delivered"
    );
}
