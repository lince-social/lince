use super::*;
use crate::native::actions::ActionButton;

fn fixture() -> (
    World,
    Entity,
    Entity,
    std::sync::mpsc::Receiver<crate::host::HostRequest>,
) {
    let (mut world, root, native, _) = super::super::tests::fixture();
    world.init_resource::<Assets<Font>>();
    world.init_resource::<crate::native::theme::Typography>();
    world.init_resource::<InputFocus>();
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    world.insert_resource(HostRequests(sender));
    let field = world
        .spawn((
            crate::native::sand::editable("unsaved draft"),
            ChildOf(native),
        ))
        .id();
    world
        .resource_mut::<InputFocus>()
        .set(field, FocusCause::Pressed);
    (world, root, field, receiver)
}

#[test]
fn request_requires_a_host_and_cancel_restores_the_same_draft_focus_camera_and_workspace() {
    let (mut world, root, field, receiver) = fixture();
    let host = world.remove_resource::<HostRequests>().unwrap();
    DesktopAction::Logout.apply(&mut world, root);
    assert!(world.get::<Confirmation>(root).is_none());
    world.insert_resource(host);
    let before = serde_json::to_value(&world.get::<Workspaces>(root).unwrap().entries).unwrap();
    let view = *world.get::<CanvasView>(root).unwrap();
    DesktopAction::Logout.apply(&mut world, root);
    let modal = world.get::<Confirmation>(root).unwrap();
    let overlay = modal.overlay;
    assert!(receiver.try_recv().is_err());
    assert!(crate::native::shortcuts::keyboard_owned(&world));
    let cancel = world.resource::<InputFocus>().get().unwrap();
    assert_eq!(
        world
            .get::<bevy::a11y::AccessibilityNode>(cancel)
            .unwrap()
            .label(),
        Some(Token::Cancel.text())
    );
    assert_eq!(world.get::<ActionButton>(cancel).unwrap().target, overlay);
    DesktopAction::Logout.apply(&mut world, root);
    assert_eq!(world.get::<Confirmation>(root).unwrap().overlay, overlay);
    Decision(false).apply(&mut world, overlay);
    assert!(world.get::<Confirmation>(root).is_none());
    assert!(world.get_entity(overlay).is_err());
    assert!(!crate::native::shortcuts::keyboard_owned(&world));
    assert_eq!(world.resource::<InputFocus>().get(), Some(field));
    assert_eq!(
        world
            .get::<bevy::text::EditableText>(field)
            .unwrap()
            .value(),
        "unsaved draft"
    );
    assert_eq!(world.get::<CanvasView>(root).unwrap().center, view.center);
    assert_eq!(world.get::<CanvasView>(root).unwrap().zoom, view.zoom);
    assert_eq!(
        serde_json::to_value(&world.get::<Workspaces>(root).unwrap().entries).unwrap(),
        before
    );
    assert!(receiver.try_recv().is_err());
}

#[test]
fn queue_failure_keeps_the_modal_and_retry_queues_one_confirmed_logout() {
    let (mut world, root, field, receiver) = fixture();
    world
        .resource::<HostRequests>()
        .0
        .try_send(crate::host::HostRequest::Close(SurfaceId(77)))
        .unwrap();
    DesktopAction::Logout.apply(&mut world, root);
    let modal = world.get::<Confirmation>(root).unwrap();
    let (overlay, status) = (modal.overlay, modal.status);
    Decision(true).apply(&mut world, overlay);
    assert_eq!(world.get::<Confirmation>(root).unwrap().overlay, overlay);
    assert!(crate::native::shortcuts::keyboard_owned(&world));
    assert_eq!(
        world.get::<Text>(status).unwrap().0,
        Token::ParkLogoutBusy.text()
    );
    assert!(matches!(
        receiver.try_recv().unwrap(),
        crate::host::HostRequest::Close(SurfaceId(77))
    ));
    Decision(true).apply(&mut world, overlay);
    assert!(matches!(
        receiver.try_recv().unwrap(),
        crate::host::HostRequest::Action(DesktopAction::Logout)
    ));
    assert!(receiver.try_recv().is_err());
    assert!(world.get::<Confirmation>(root).is_none());
    assert_eq!(world.resource::<InputFocus>().get(), Some(field));
    Decision(true).apply(&mut world, overlay);
    assert!(receiver.try_recv().is_err());
}

#[test]
fn stale_confirmation_cannot_redirect_to_a_new_dialog_or_restore_a_departed_workspace_field() {
    let (mut world, root, _, receiver) = fixture();
    DesktopAction::Logout.apply(&mut world, root);
    let first = world.get::<Confirmation>(root).unwrap().overlay;
    Decision(false).apply(&mut world, first);
    DesktopAction::Logout.apply(&mut world, root);
    let second = world.get::<Confirmation>(root).unwrap().overlay;
    Decision(true).apply(&mut world, first);
    assert!(receiver.try_recv().is_err());
    assert_eq!(world.get::<Confirmation>(root).unwrap().overlay, second);
    world.get_mut::<Workspaces>(root).unwrap().active = 9;
    Decision(false).apply(&mut world, second);
    assert_eq!(world.resource::<InputFocus>().get(), None);
    assert!(receiver.try_recv().is_err());
    let other = world.spawn_empty().id();
    crate::native::shortcuts::own_keyboard(&mut world, other);
    DesktopAction::Logout.apply(&mut world, root);
    assert!(world.get::<Confirmation>(root).is_none());
    assert_eq!(
        world
            .resource::<crate::native::shortcuts::KeyboardModalOwner>()
            .0,
        Some(other)
    );
}
