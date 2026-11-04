use super::*;
use lince_interface::host::{DesktopAction, OutputId, WorkspaceLayout};
use std::hash::{Hash, Hasher};

#[test]
#[ignore = "hardware Vulkan/EGL full-frame visibility diagnostic; no concurrent compilation"]
fn invisible_native_items_record_preparation_cost_without_changing_visible_pixels() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let mut reports = Vec::new();
    for (mode, hidden) in [
        ("visible-only", 0),
        ("offscreen", 32),
        ("inactive-workspace", 32),
    ] {
        let size = [640, 480];
        std::fs::create_dir_all(directory.path().join(mode)).unwrap();
        let mut canvas = super::super::appearance::launch(
            &directory.path().join(format!("{mode}/workspace.json")),
            size,
        );
        let root = canvas.canvas_root().unwrap();
        for _ in 0..2 {
            native::workspace::create(canvas.world_mut(), root);
        }
        assert!(
            canvas
                .app
                .world()
                .get::<native::workspace::Workspaces>(root)
                .unwrap()
                .entries
                .iter()
                .any(|workspace| workspace.id == 3)
        );
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
        let presented: Vec<_> = canvas
            .app
            .world()
            .resource::<native::output_views::OutputViews>()
            .outputs
            .iter()
            .map(|output| output.workspace)
            .collect();
        let inactive = canvas
            .app
            .world()
            .get::<native::workspace::Workspaces>(root)
            .unwrap()
            .entries
            .iter()
            .find(|workspace| !presented.contains(&workspace.id))
            .unwrap()
            .id;
        for workspace in [1, 2, 3] {
            native::workspace_config::set_physics(canvas.world_mut(), root, workspace, false);
        }
        let mut owners = Vec::new();
        for id in 1..=2 {
            canvas.focus_output(OutputId(id)).unwrap();
            let workspace = canvas
                .app
                .world()
                .get::<native::workspace::Workspaces>(root)
                .unwrap()
                .active;
            canvas
                .desktop_action(DesktopAction::SetLayout(WorkspaceLayout::Free))
                .unwrap();
            *canvas
                .world_mut()
                .get_mut::<native::canvas::CanvasView>(root)
                .unwrap() = Default::default();
            for index in 0..(8 + hidden) {
                let visible = index < 8;
                let owner = super::super::appearance::item(
                    &mut canvas,
                    root,
                    Vec2::new(68.0, 80.0),
                    if visible {
                        [index as f64 * 72.0 - 252.0, 0.0]
                    } else {
                        [10000.0 + index as f64 * 100.0, 0.0]
                    },
                    Color::srgb(0.0, 1.0, 0.0),
                    index,
                    false,
                );
                canvas
                    .world_mut()
                    .entity_mut(owner)
                    .insert(native::workspace::WorkspaceMember(
                        if !visible && mode == "inactive-workspace" {
                            inactive
                        } else {
                            workspace
                        },
                    ));
                for node in 0..32 {
                    canvas.world_mut().spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            left: px((node % 8) as f32 * 6.0),
                            top: px((node / 8) as f32 * 6.0),
                            width: px(3),
                            height: px(3),
                            ..default()
                        },
                        ChildOf(owner),
                    ));
                }
                let font = canvas
                    .app
                    .world()
                    .resource::<native::theme::Typography>()
                    .text(12.0);
                canvas.world_mut().spawn((
                    Text::new(format!("Item {index}")),
                    TextColor(Color::BLACK),
                    font,
                    TextLayout::no_wrap(),
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(4),
                        top: px(24),
                        width: px(60),
                        height: px(20),
                        flex_shrink: 0.0,
                        ..default()
                    },
                    ChildOf(owner),
                ));
                owners.push((owner, visible));
            }
        }
        canvas.focus_output(OutputId(1)).unwrap();
        canvas.desktop_action(DesktopAction::Unfocus).unwrap();
        for _ in 0..12 {
            drop(canvas.probe_frame().unwrap());
        }
        for (owner, visible) in &owners {
            assert_eq!(
                canvas
                    .app
                    .world()
                    .get::<native::topology::presentation::Surface>(*owner)
                    .unwrap()
                    .visible,
                *visible,
                "Unexpected visibility for {mode}, owner {owner:?}; outputs {:?}",
                canvas
                    .app
                    .world()
                    .resource::<native::output_views::OutputViews>()
            );
        }
        let mut elapsed = Vec::new();
        let mut cpu = Vec::new();
        for _ in 0..15 {
            let started = Instant::now();
            let before = rustix::time::clock_gettime(rustix::time::ClockId::ProcessCPUTime);
            drop(canvas.probe_frame().unwrap());
            let after = rustix::time::clock_gettime(rustix::time::ClockId::ProcessCPUTime);
            elapsed.push(started.elapsed().as_secs_f64() * 1000.0);
            cpu.push(
                (after.tv_sec - before.tv_sec) as f64 * 1000.0
                    + (after.tv_nsec - before.tv_nsec) as f64 / 1_000_000.0,
            );
            lince_interface::host::performance::flush_if_due();
        }
        let frame = canvas.probe_frame().unwrap();
        let mut pixels = Vec::new();
        for id in 1..=2 {
            let image = if frame.output == Some(OutputId(id)) {
                frame.dmabuf().unwrap()
            } else {
                frame
                    .outputs
                    .iter()
                    .find(|image| image.id == OutputId(id))
                    .unwrap()
                    .dmabuf()
            };
            let bytes = probe_pixels(image).unwrap();
            let green = bytes
                .chunks_exact(4)
                .filter(|pixel| pixel[0] < 30 && pixel[1] > 230 && pixel[2] < 30)
                .count();
            assert!(
                green > 20000,
                "The visible reference disappeared: {mode}, output{id}, {green} green pixels"
            );
            let mut glyphs = 0;
            for index in 0..8 {
                for y in 200..280 {
                    for x in (34 + index * 72)..(102 + index * 72) {
                        let pixel = &bytes[(y * 640 + x) * 4..][..4];
                        glyphs += usize::from(pixel[..3].iter().all(|channel| *channel < 180));
                    }
                }
            }
            if glyphs <= 200 {
                super::super::shortcuts::capture(
                    &format!("missing-offscreen-labels-{mode}"),
                    size,
                    &frame,
                );
                let world = canvas.app.world();
                for (owner, visible) in &owners {
                    if !visible {
                        continue;
                    }
                    let surface = world
                        .get::<native::topology::presentation::Surface>(*owner)
                        .unwrap();
                    eprintln!(
                        "Label owner {owner:?}: pixels {:?}, density {}",
                        surface.pixels, surface.density
                    );
                    for child in world.get::<Children>(*owner).unwrap().iter() {
                        if world.get::<Text>(child).is_some() {
                            eprintln!(
                                "Label {child:?}: size {:?}, transform {:?}, shaped {:?}, camera {:?}, visibility {:?}",
                                world.get::<ComputedNode>(child).map(ComputedNode::size),
                                world.get::<UiGlobalTransform>(child),
                                world
                                    .get::<bevy::text::TextLayoutInfo>(child)
                                    .map(|info| info.glyphs.len()),
                                world.get::<bevy::ui::ComputedUiTargetCamera>(child),
                                world.get::<InheritedVisibility>(child)
                            );
                        }
                    }
                }
            }
            assert!(
                glyphs > 200,
                "The visible labels disappeared: {mode}, output{id}, {glyphs} dark glyph pixels"
            );
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            bytes.hash(&mut hash);
            pixels
                .push(serde_json::json!({"output":id, "green_pixels":green,"glyph_pixels":glyphs,"hash":hash.finish()}));
        }
        elapsed.sort_by(f64::total_cmp);
        cpu.sort_by(f64::total_cmp);
        reports.push(serde_json::json!({"mode":mode,"outputs":2,"visible_per_output":8,"hidden_per_output":hidden,"nodes_per_owner":34,
            "warmup_frames":12,"rounds":15,"median_ms":elapsed[7],"p95_upper_ms":elapsed[14],"cpu_median_ms":cpu[7],"cpu_total_ms":cpu.iter().sum::<f64>(),"pixels":pixels}));
        eprintln!("offscreen-case: {}", reports.last().unwrap());
        canvas.finish().unwrap();
    }
    let result = serde_json::json!({"scope":"Private diagnostic full native update/render/export/completion; two outputs, equal visible content and additional offscreen or inactive-workspace native trees. Readback outside timing. No physical FPS or input-visible latency acceptance.","reports":reports});
    if let Some(path) = std::env::var_os("LINCE_PARK_TEST_ARTIFACTS") {
        let path = std::path::PathBuf::from(path);
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(
            path.join("offscreen-frames.json"),
            serde_json::to_vec_pretty(&result).unwrap(),
        )
        .unwrap();
    }
    eprintln!("offscreen-frames: {result}");
}
