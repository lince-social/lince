use super::shortcuts::{capture, click, named_button, settle};
use super::*;
use lince_interface::host::{DesktopAction, HostRequest, OutputId, OutputViewport, Rectangle};
use native::actions::Action;

pub(super) fn output(id: u64, x: f64, size: [u32; 2]) -> Output {
    let bounds = Rectangle::new([x, 0.0], size.map(f64::from)).unwrap();
    Output {
        name: format!("Transfer-{id}"),
        viewport: OutputViewport { rotation: Default::default(),
            id: OutputId(id),
            bounds,
            usable: Some(bounds),
            physical_size: size,
            raster: lince_interface::host::raster::RasterMapping {
                origin: bounds.origin(),
                scale: [1.0; 2],
            },
            workspace: 1,
        },
    }
}

fn active(canvas: &Presentation, root: Entity) -> u64 {
    canvas
        .app
        .world()
        .get::<native::workspace::Workspaces>(root)
        .unwrap()
        .active
}

fn control(canvas: &mut Presentation, action: native::edit_mode::EditAction) -> Entity {
    let world = canvas.world_mut();
    world
        .query::<(Entity, &native::edit_mode::EditControl)>()
        .iter(world)
        .find(|(_, control)| control.action == action)
        .map(|(entity, _)| entity)
        .unwrap()
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn workspace_buttons_create_and_select_stable_destinations_through_the_host_on_multiple_outputs() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    for size in [[1000, 700], [360, 700]] {
        workspace_controls(size, directory.path().join(format!("assets-{}", size[0])));
    }
}

fn workspace_controls(size: [u32; 2], assets: PathBuf) {
    let mut canvas = Presentation::new(size, assets, WakeSignal::new(|| {})).unwrap();
    settle(&mut canvas);
    canvas
        .outputs(vec![output(1, 0.0, size), output(2, 1000.0, [640, 480])])
        .unwrap();
    let root = canvas.canvas_root().unwrap();
    let ids = native::workspace::numbers(canvas.app.world(), root);
    assert_eq!(ids.len(), 2);
    canvas
        .world_mut()
        .get_mut::<native::workspace::Workspaces>(root)
        .unwrap()
        .entries[1]
        .name = "Applications and research with tools, notes and longer workspace names".into();
    let names: Vec<_> = canvas
        .app
        .world()
        .get::<native::workspace::Workspaces>(root)
        .unwrap()
        .entries
        .iter()
        .map(|entry| entry.name.clone())
        .collect();
    let (sender, receiver) = std::sync::mpsc::sync_channel(16);
    canvas.host_requests(sender);
    native::edit_mode::EditAction::Open.apply(canvas.world_mut(), root);
    native::edit_mode::EditAction::Workspaces.apply(canvas.world_mut(), root);
    settle(&mut canvas);
    let new = control(&mut canvas, native::edit_mode::EditAction::CreateWorkspace);
    click(&mut canvas, new, size);
    assert_eq!(
        receiver.try_recv(),
        Ok(HostRequest::CreateWorkspace {
            owner: root.to_bits()
        })
    );
    assert_eq!(native::workspace::numbers(canvas.app.world(), root), ids);
    canvas.create_workspace(root.to_bits()).unwrap();
    let frame = settle(&mut canvas);
    let created = active(&canvas, root);
    let spaces = canvas
        .app
        .world()
        .get::<native::workspace::Workspaces>(root)
        .unwrap();
    assert_eq!(spaces.entries[0].name, names[0]);
    assert_eq!(spaces.entries[1].name, names[1]);
    assert_eq!(
        spaces
            .entries
            .iter()
            .find(|entry| entry.id == created)
            .unwrap()
            .name,
        format!("Workspace {created}")
    );
    assert!(!ids.contains(&created));
    assert_eq!(frame.output, Some(OutputId(1)));
    assert_eq!(
        canvas
            .output_state
            .assignments
            .outputs()
            .iter()
            .find(|output| output.id == OutputId(1))
            .unwrap()
            .active,
        created
    );
    assert_eq!(
        canvas
            .output_state
            .assignments
            .outputs()
            .iter()
            .find(|output| output.id == OutputId(2))
            .unwrap()
            .active,
        ids[1]
    );
    capture(&format!("workspace-created-{}", size[0]), size, &frame);
    let other_name = canvas
        .app
        .world()
        .get::<native::workspace::Workspaces>(root)
        .unwrap()
        .entries
        .iter()
        .find(|entry| entry.id == ids[1])
        .unwrap()
        .name
        .clone();
    let other = named_button(&mut canvas, &other_name);
    let world = canvas.app.world();
    let text = world
        .get::<Children>(other)
        .unwrap()
        .iter()
        .find(|child| world.get::<Text>(*child).is_some())
        .unwrap();
    let layout = world.get::<bevy::text::TextLayoutInfo>(text).unwrap();
    assert!(
        layout.size.x <= world.get::<ComputedNode>(other).unwrap().size().x,
        "The workspace name overflows its button: {:?}",
        layout.size
    );
    click(&mut canvas, other, size);
    assert_eq!(
        receiver.try_recv(),
        Ok(HostRequest::Workspace {
            owner: root.to_bits(),
            id: ids[1]
        })
    );
    assert_eq!(active(&canvas, root), created);
    canvas.activate_workspace(root.to_bits(), ids[1]).unwrap();
    let frame = settle(&mut canvas);
    assert_eq!(frame.output, Some(OutputId(2)));
    assert_eq!(active(&canvas, root), ids[1]);
    assert_eq!(
        canvas
            .app
            .world()
            .get::<native::workspace::Workspaces>(root)
            .unwrap()
            .entries[1]
            .name,
        names[1]
    );
    assert_eq!(
        canvas
            .output_state
            .assignments
            .outputs()
            .iter()
            .find(|output| output.id == OutputId(1))
            .unwrap()
            .active,
        created
    );
    canvas
        .create_workspace(root.to_bits().wrapping_add(1))
        .unwrap();
    canvas
        .activate_workspace(root.to_bits().wrapping_add(1), ids[0])
        .unwrap();
    canvas.activate_workspace(root.to_bits(), u64::MAX).unwrap();
    assert_eq!(active(&canvas, root), ids[1]);
    assert_eq!(
        native::workspace::numbers(canvas.app.world(), root).len(),
        3
    );
    canvas.desktop_action(DesktopAction::Workspace(9)).unwrap();
    settle(&mut canvas);
    let ids = native::workspace::numbers(canvas.app.world(), root);
    assert_eq!(ids.len(), 9);
    assert_eq!(active(&canvas, root), ids[8]);
    for (index, id) in ids.iter().enumerate() {
        canvas
            .desktop_action(DesktopAction::Workspace(index as u32 + 1))
            .unwrap();
        assert_eq!(active(&canvas, root), *id);
    }
    native::edit_mode::EditAction::Close.apply(canvas.world_mut(), root);
    canvas.focus_output(OutputId(1)).unwrap();
    native::laboratory::LaboratoryAction::Open.apply(canvas.world_mut(), root);
    settle(&mut canvas);
    let temporary = native::laboratory::scope(canvas.app.world()).unwrap().2;
    assert!(!native::workspace::numbers(canvas.app.world(), root).contains(&temporary));
    canvas
        .world_mut()
        .get_mut::<native::workspace::Workspaces>(root)
        .unwrap()
        .entries
        .rotate_right(1);
    assert_eq!(native::workspace::numbers(canvas.app.world(), root), ids);
    canvas.activate_workspace(root.to_bits(), ids[1]).unwrap();
    let frame = settle(&mut canvas);
    assert!(native::laboratory::scope(canvas.app.world()).is_none());
    assert_eq!(active(&canvas, root), ids[1]);
    assert_eq!(native::workspace::numbers(canvas.app.world(), root), ids);
    capture(
        &format!("workspace-after-laboratory-{}", size[0]),
        [640, 480],
        &frame,
    );
    native::laboratory::LaboratoryAction::Open.apply(canvas.world_mut(), root);
    settle(&mut canvas);
    let temporary = native::laboratory::scope(canvas.app.world()).unwrap().2;
    canvas.create_workspace(root.to_bits()).unwrap();
    settle(&mut canvas);
    let numbers = native::workspace::numbers(canvas.app.world(), root);
    assert_eq!(numbers.len(), ids.len() + 1);
    assert_eq!(&numbers[..ids.len()], &ids);
    assert!(!numbers.contains(&temporary));
    assert_eq!(active(&canvas, root), *numbers.last().unwrap());
    assert!(native::laboratory::scope(canvas.app.world()).is_none());
    canvas.finish().unwrap();
}
