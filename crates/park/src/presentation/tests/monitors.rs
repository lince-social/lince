use super::shortcuts::{key, settle};
use super::*;
use lince_interface::host::{
    DesktopAction, Direction, HostRequest, OutputId, OutputViewport, Rectangle, WorkspaceLayout,
};

fn monitor(id: u64, origin: [f64; 2]) -> Output {
    Output {
        name: format!("Display-{id}"),
        viewport: OutputViewport { rotation: Default::default(),
            id: OutputId(id),
            bounds: Rectangle::new(origin, [320.0, 240.0]).unwrap(),
            usable: Rectangle::new([origin[0], origin[1] + 20.0], [320.0, 220.0]),
            physical_size: [320, 240],
            raster: lince_interface::host::raster::RasterMapping { origin: (Rectangle::new(origin, [320.0, 240.0]).unwrap()).origin(), scale: [1.0; 2] },
            workspace: 1,
        },
    }
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn monitor_chords_preserve_workspace_editor_follow_and_geometry_through_bar_changes_and_reconnect()
{
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
    settle(&mut canvas);
    let outputs = vec![
        monitor(20, [320.0, 240.0]),
        monitor(40, [0.0, 0.0]),
        monitor(10, [320.0, 0.0]),
        monitor(30, [0.0, 240.0]),
    ];
    canvas.outputs(outputs.clone()).unwrap();
    let root = canvas.canvas_root().unwrap();
    let mut targets = std::collections::BTreeMap::new();
    for output in &outputs {
        canvas.focus_output(output.viewport.id).unwrap();
        let workspace = canvas
            .app
            .world()
            .get::<native::workspace::Workspaces>(root)
            .unwrap()
            .active;
        canvas
            .world_mut()
            .get_mut::<native::workspace::Workspaces>(root)
            .unwrap()
            .entries
            .iter_mut()
            .find(|space| space.id == workspace)
            .unwrap()
            .desktop
            .set_mode(WorkspaceLayout::Free);
        assert!(native::workspace_config::set_physics(
            canvas.world_mut(),
            root,
            workspace,
            false
        ));
        let size = Vec2::new(120.0, 80.0);
        let layout = native::layout::LayoutBox::new(size);
        let target = canvas
            .world_mut()
            .spawn((
                native::castle::Castle,
                native::canvas::CanvasItem {
                    position: bevy::math::DVec2::new(output.viewport.id.0 as f64 * 100.0, -500.0),
                    size,
                },
                layout,
                native::workspace::WorkspaceMember(workspace),
                BackgroundColor(Color::srgb(0.1, 0.5, 0.7)),
                ChildOf(root),
            ))
            .id();
        let editor = canvas
            .world_mut()
            .spawn((native::sand::editable("Retained draft"), ChildOf(target)))
            .id();
        canvas.desktop_action(DesktopAction::ToggleFocus).unwrap();
        settle(&mut canvas);
        canvas
            .world_mut()
            .resource_mut::<bevy::input_focus::InputFocus>()
            .set(editor, bevy::input_focus::FocusCause::Pressed);
        targets.insert(output.viewport.id, (target, editor, workspace, layout.id));
    }
    let (sender, receiver) = std::sync::mpsc::sync_channel(16);
    canvas.host_requests(sender);
    canvas.focus_output(OutputId(40)).unwrap();
    settle(&mut canvas);
    assert_eq!(
        canvas
            .app
            .world()
            .resource::<bevy::input_focus::InputFocus>()
            .get(),
        Some(targets[&OutputId(40)].1)
    );
    for (code, direction, expected) in [
        (KeyCode::KeyO, Direction::Right, 10),
        (KeyCode::KeyU, Direction::Down, 20),
        (KeyCode::KeyY, Direction::Left, 30),
        (KeyCode::KeyI, Direction::Up, 40),
        (KeyCode::KeyY, Direction::Left, 40),
    ] {
        key(&mut canvas, KeyCode::SuperLeft, true);
        key(&mut canvas, code, true);
        key(&mut canvas, code, false);
        key(&mut canvas, KeyCode::SuperLeft, false);
        let action = DesktopAction::FocusOutput(direction);
        assert_eq!(
            receiver.try_recv(),
            Ok(HostRequest::Action(action)),
            "Chord {code:?} did not reach Park"
        );
        assert!(receiver.try_recv().is_err());
        canvas.desktop_action(action).unwrap();
        let output = OutputId(expected);
        let (target, editor, workspace, identity) = targets[&output];
        let frame = settle(&mut canvas);
        assert_eq!(frame.output, Some(output));
        assert_eq!(
            canvas
                .app
                .world()
                .get::<native::workspace::Workspaces>(root)
                .unwrap()
                .active,
            workspace
        );
        assert_eq!(
            canvas
                .app
                .world()
                .resource::<bevy::input_focus::InputFocus>()
                .get(),
            Some(editor)
        );
        assert!(native::layout::desktop::focus::following(
            canvas.app.world(),
            root
        ));
        canvas
            .world_mut()
            .get_mut::<native::canvas::CanvasItem>(target)
            .unwrap()
            .position += bevy::math::DVec2::new(17.0, -13.0);
        let frame = settle(&mut canvas);
        let geometry = frame
            .geometry
            .as_ref()
            .unwrap()
            .items
            .iter()
            .find(|item| item.id == lince_interface::host::ItemId::Native(identity))
            .unwrap();
        let center = std::array::from_fn::<_, 2, _>(|axis| {
            geometry.mapping.canvas.origin()[axis] + geometry.mapping.canvas.size()[axis] * 0.5
        });
        let bounds = outputs
            .iter()
            .find(|candidate| candidate.viewport.id == output)
            .unwrap()
            .viewport
            .usable
            .unwrap();
        for axis in 0..2 {
            assert!(
                (center[axis] - bounds.origin()[axis] - bounds.size()[axis] * 0.5).abs() < 0.05
            );
        }
    }
    let preferences = canvas.output_state.assignments.preferences().to_vec();
    let mut reserved = outputs.clone();
    reserved
        .iter_mut()
        .find(|output| output.viewport.id == OutputId(40))
        .unwrap()
        .viewport
        .usable = Rectangle::new([0.0, 40.0], [320.0, 200.0]);
    canvas.outputs(reserved.clone()).unwrap();
    settle(&mut canvas);
    assert_eq!(
        canvas
            .app
            .world()
            .resource::<bevy::input_focus::InputFocus>()
            .get(),
        Some(targets[&OutputId(40)].1)
    );
    canvas
        .outputs(
            reserved
                .iter()
                .filter(|output| output.viewport.id != OutputId(40))
                .cloned()
                .collect(),
        )
        .unwrap();
    settle(&mut canvas);
    assert_eq!(
        canvas
            .app
            .world()
            .get::<native::workspace::Workspaces>(root)
            .unwrap()
            .active,
        targets[&OutputId(40)].2
    );
    canvas.outputs(reserved).unwrap();
    canvas.focus_output(OutputId(40)).unwrap();
    settle(&mut canvas);
    assert!(native::layout::desktop::focus::following(
        canvas.app.world(),
        root
    ));
    assert_eq!(
        canvas
            .app
            .world()
            .resource::<bevy::input_focus::InputFocus>()
            .get(),
        Some(targets[&OutputId(40)].1)
    );
    assert_eq!(canvas.output_state.assignments.preferences(), preferences);
    for (_, (target, editor, workspace, identity)) in targets {
        assert_eq!(
            canvas
                .app
                .world()
                .get::<native::layout::LayoutBox>(target)
                .unwrap()
                .id,
            identity
        );
        assert_eq!(
            canvas
                .app
                .world()
                .get::<native::workspace::WorkspaceMember>(target)
                .unwrap()
                .0,
            workspace
        );
        assert_eq!(
            canvas.app.world().get::<ChildOf>(editor).unwrap().parent(),
            target
        );
        assert_eq!(
            canvas
                .app
                .world()
                .get::<bevy::text::EditableText>(editor)
                .unwrap()
                .value()
                .to_string(),
            "Retained draft"
        );
    }
}
