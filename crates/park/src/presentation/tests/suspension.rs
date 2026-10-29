use super::shortcuts::{capture_button, key, named_button, settle};
use super::*;
use lince_interface::host::{DesktopAction, OutputId, OutputViewport, Rectangle};
use native::actions::Action;

fn outputs() -> Vec<Output> {
    [0.0, 640.0]
        .into_iter()
        .enumerate()
        .map(|(index, x)| {
            let bounds = Rectangle::new([x, 0.0], [640.0, 480.0]).unwrap();
            Output {
                name: format!("Suspension-{}", index + 1),
                viewport: OutputViewport { rotation: Default::default(),
                    id: OutputId(index as u64 + 1),
                    bounds,
                    usable: Some(bounds),
                    physical_size: [640, 480],
                    raster: lince_interface::host::raster::RasterMapping {
                        origin: bounds.origin(),
                        scale: [1.0; 2],
                    },
                    workspace: index as u64 + 1,
                },
            }
        })
        .collect()
}

fn activate(canvas: &mut Presentation, entity: Entity) {
    canvas
        .world_mut()
        .trigger(bevy::ui_widgets::Activate { entity });
    settle(canvas);
}

fn snapshot(canvas: &Presentation, root: Entity) -> serde_json::Value {
    let spaces = canvas
        .app
        .world()
        .get::<native::workspace::Workspaces>(root)
        .unwrap();
    serde_json::to_value((spaces.active, &spaces.entries)).unwrap()
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn removing_every_output_retains_workspaces_and_the_settings_draft_and_allows_headless_save() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let mut canvas = Presentation::new(
        [640, 480],
        directory.path().join("assets"),
        WakeSignal::new(|| {}),
    )
    .unwrap();
    canvas
        .world_mut()
        .insert_resource(native::workspace::WorkspaceFile::new(
            directory.path().join("interface.json"),
        ));
    settle(&mut canvas);
    canvas.outputs(Vec::new()).unwrap();
    assert!(settle(&mut canvas).dmabuf().is_none());
    canvas.outputs(outputs()).unwrap();
    canvas.focus(true).unwrap();
    settle(&mut canvas);
    let root = canvas.canvas_root().unwrap();
    canvas
        .desktop_action(DesktopAction::SetLayout(
            lince_interface::host::WorkspaceLayout::Free,
        ))
        .unwrap();
    assert!(native::workspace_config::set_physics(
        canvas.world_mut(),
        root,
        1,
        false
    ));
    native::edit_mode::EditAction::AddSand(native::sand_store::SandKind::Square)
        .apply(canvas.world_mut(), root);
    settle(&mut canvas);
    canvas.outputs(outputs()).unwrap();
    settle(&mut canvas);
    native::edit_mode::EditAction::Open.apply(canvas.world_mut(), root);
    native::edit_mode::EditAction::Park.apply(canvas.world_mut(), root);
    settle(&mut canvas);
    let button = named_button(&mut canvas, "Edit all shortcuts");
    activate(&mut canvas, button);
    let button = capture_button(&mut canvas, "layout.toggle-tiling");
    activate(&mut canvas, button);
    key(&mut canvas, KeyCode::ControlLeft, true);
    key(&mut canvas, KeyCode::F3, true);
    key(&mut canvas, KeyCode::F3, false);
    key(&mut canvas, KeyCode::ControlLeft, false);
    let validate = named_button(&mut canvas, "Validate combination");
    activate(&mut canvas, validate);
    let button = capture_button(&mut canvas, "layout.toggle-tiling");
    let caption = canvas
        .app
        .world()
        .get::<bevy::a11y::AccessibilityNode>(button)
        .unwrap()
        .label()
        .unwrap()
        .to_owned();
    assert!(caption.ends_with("Ctrl+F3"));
    let before = snapshot(&canvas, root);
    let dimensions = canvas.render_size;
    let revision = settle(&mut canvas).geometry.as_ref().unwrap().revision;
    let target = canvas.target.clone();
    let entities: Vec<_> = canvas
        .app
        .world()
        .get::<Children>(root)
        .unwrap()
        .iter()
        .collect();

    canvas.outputs(Vec::new()).unwrap();
    let frame = settle(&mut canvas);
    assert!(canvas.app.world().get_resource::<native::output_views::OutputViews>().is_none());
    assert!(frame.dmabuf().is_none());
    assert!(frame.output.is_none());
    assert!(frame.outputs.is_empty());
    assert!(frame.ready().unwrap());
    let hidden_revision = frame.geometry.as_ref().unwrap().revision;
    assert!(hidden_revision > revision);
    assert!(frame.geometry.as_ref().unwrap().outputs.is_empty());
    assert!(frame.geometry.as_ref().unwrap().items.is_empty());
    assert!(frame.input.controls.is_empty());
    assert!(frame.input.pointer_targets.is_empty());
    assert!(frame.input.editor.is_none());
    assert!(frame.input.drop_target.is_none());
    assert!(!frame.input.keyboard_modal);
    assert!(
        frame
            .input
            .workspaces
            .entries
            .iter()
            .all(|workspace| !workspace.active)
    );
    assert_eq!(canvas.render_size, dimensions);
    assert_eq!(canvas.target, target);
    assert!(
        canvas
            .world_mut()
            .query::<&Camera>()
            .iter(canvas.app.world())
            .all(|camera| !camera.is_active)
    );
    assert_eq!(snapshot(&canvas, root), before);
    assert!(
        entities
            .iter()
            .all(|entity| canvas.app.world().get_entity(*entity).is_ok())
    );
    key(&mut canvas, KeyCode::Escape, true);
    key(&mut canvas, KeyCode::Escape, false);
    canvas.focus_output(OutputId(99)).unwrap();
    canvas.focus(true).unwrap();
    let window = canvas.input_window().unwrap();
    assert!(!canvas.app.world().get::<Window>(window).unwrap().focused);
    assert_eq!(
        canvas
            .app
            .world()
            .get::<bevy::a11y::AccessibilityNode>(button)
            .unwrap()
            .label(),
        Some(caption.as_str())
    );

    canvas.outputs(outputs()).unwrap();
    canvas.focus(true).unwrap();
    let frame = settle(&mut canvas);
    assert!(frame.dmabuf().is_some());
    assert!(frame.geometry.as_ref().unwrap().revision > hidden_revision);
    assert!(
        !canvas
            .world_mut()
            .query::<&Text>()
            .iter(canvas.app.world())
            .any(|text| text
                .0
                .starts_with("Capture cancelled because keyboard focus left this window."))
    );
    super::shortcuts::capture("output-reconnected-settings", [640, 480], &frame);
    assert_eq!(frame.geometry.as_ref().unwrap().outputs.len(), 2);
    assert!(frame.input.keyboard_modal);
    assert_eq!(
        canvas
            .app
            .world()
            .get::<bevy::a11y::AccessibilityNode>(button)
            .unwrap()
            .label(),
        Some(caption.as_str())
    );
    assert_eq!(snapshot(&canvas, root), before);
    let cancel = named_button(&mut canvas, "Cancel draft");
    activate(&mut canvas, cancel);
    canvas.outputs(Vec::new()).unwrap();
    canvas.finish().unwrap();
    assert!(directory.path().join("interface.snapshots").is_dir());
}
