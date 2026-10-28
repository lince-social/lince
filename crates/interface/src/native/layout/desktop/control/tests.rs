use super::*;

fn invocation(world: &World, root: Entity, shortcut: Shortcut) -> Invocation {
    Invocation { command: shortcut.id().into(), context: context(world, root).unwrap(), arguments: Arguments::None }
}

#[test]
fn captured_selection_cannot_retarget_after_focus_or_canvas_changes() {
    let (mut world, root, first, second) = super::super::tests::fixture();
    crate::native::layout::engine::resolve(&mut world);
    focus_item(&mut world, first);
    let call = invocation(&world, root, Shortcut::MoveRight);
    let grants = [Capability::DesktopControl, Capability::WindowControl];
    assert_eq!(prepare(&world, root, &call, &grants), Ok(Prepared::Action(DesktopAction::Move(crate::host::Direction::Right))));
    focus_item(&mut world, second);
    assert_eq!(prepare(&world, root, &call, &grants).unwrap_err().code, Error::StaleTarget);
    let fresh = invocation(&world, root, Shortcut::MoveRight);
    world.get_mut::<Workspaces>(root).unwrap().canvas_id = "replacement-canvas".into();
    assert_eq!(prepare(&world, root, &fresh, &grants).unwrap_err().code, Error::StaleTarget);
}

#[test]
fn dispatcher_access_cannot_grant_window_output_launch_or_capture_permissions() {
    let (mut world, root, first, _) = super::super::tests::fixture();
    crate::native::layout::engine::resolve(&mut world);
    focus_item(&mut world, first);
    for shortcut in [Shortcut::Close, Shortcut::MoveRight, Shortcut::Terminal, Shortcut::FocusMonitorRight, Shortcut::External(crate::host::external::Action::ScreenshotScreen)] {
        let call = invocation(&world, root, shortcut);
        assert_eq!(prepare(&world, root, &call, &[Capability::DesktopControl]).unwrap_err().code, Error::Denied);
        assert_eq!(prepare(&world, root, &call, &[Capability::WindowList]).unwrap_err().code, Error::Denied);
    }
    assert!(authority(&[Capability::DesktopControl, Capability::Launch], DesktopAction::External(crate::host::external::Action::ScreenshotScreen)).is_err());
}

#[test]
fn qualified_workspace_selection_is_idempotent_and_reordering_invalidates_the_destination() {
    let (mut world, root, _, _) = super::super::tests::fixture();
    let mut second = world.get::<Workspaces>(root).unwrap().entries[0].clone();
    second.id = 73;
    world.get_mut::<Workspaces>(root).unwrap().entries.push(second);
    let mut call = invocation(&world, root, Shortcut::Workspace1);
    call.arguments = Arguments::Workspace { id: 1 };
    assert_eq!(prepare(&world, root, &call, &[Capability::DesktopControl]), Ok(Prepared::Workspace { owner: root.to_bits(), id: 1 }));
    call.arguments = Arguments::None;
    assert_eq!(prepare(&world, root, &call, &[Capability::DesktopControl]).unwrap_err().code, Error::InvalidArguments);
    call.command = Shortcut::Workspace2.id().into();
    call.arguments = Arguments::Workspace { id: 73 };
    world.get_mut::<Workspaces>(root).unwrap().entries.swap(0, 1);
    assert_eq!(prepare(&world, root, &call, &[Capability::DesktopControl]).unwrap_err().code, Error::StaleTarget);
}
