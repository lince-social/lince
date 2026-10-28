use super::*;
use bevy::input::{
    ButtonState, InputPlugin,
    keyboard::{Key, KeyboardInput},
};

fn fixture() -> (App, Entity, Entity, [Entity; 3]) {
    let mut app = App::new();
    crate::native::laboratory::isolate(app.world_mut());
    app.add_plugins((
        InputPlugin,
        crate::native::actions::ActionsPlugin,
        crate::native::shortcuts::EditorPlugin,
    ))
    .init_resource::<Assets<Font>>()
    .init_resource::<crate::native::theme::Typography>()
    .init_resource::<Settings>();
    let root = app
        .world_mut()
        .spawn((
            Workspaces::default(),
            CanvasView::default(),
            ComputedNode {
                size: Vec2::new(1000.0, 700.0),
                inverse_scale_factor: 1.0,
                ..default()
            },
        ))
        .id();
    let window = app
        .world_mut()
        .spawn((
            Window::default(),
            bevy::window::PrimaryWindow,
            crate::native::actions::WindowActionTarget(root),
        ))
        .id();
    let items = std::array::from_fn(|index| {
        app.world_mut()
            .spawn((
                CanvasItem {
                    position: bevy::math::DVec2::new(index as f64 * 400.0, 0.0),
                    size: Vec2::new(300.0, 200.0),
                },
                LayoutBox::new(Vec2::new(300.0, 200.0)),
                WorkspaceMember(1),
                Name::new(format!("Window {index}")),
                ChildOf(root),
            ))
            .id()
    });
    crate::native::layout::engine::resolve(app.world_mut());
    for item in items {
        focus_item(app.world_mut(), item);
    }
    crate::native::shortcuts::install(app.world_mut(), root);
    (app, root, window, items)
}

fn event(app: &mut App, window: Entity, physical: KeyCode, logical: Key, pressed: bool) {
    app.world_mut().write_message(KeyboardInput {
        window,
        key_code: physical,
        logical_key: logical,
        state: if pressed {
            ButtonState::Pressed
        } else {
            ButtonState::Released
        },
        repeat: false,
        text: None,
    });
}

fn gesture(app: &mut App, window: Entity, reverse: bool) {
    event(app, window, KeyCode::AltLeft, Key::Alt, true);
    if reverse {
        event(app, window, KeyCode::ShiftLeft, Key::Shift, true);
    }
    event(app, window, KeyCode::Tab, Key::Tab, true);
    event(app, window, KeyCode::Tab, Key::Tab, false);
    if reverse {
        event(app, window, KeyCode::ShiftLeft, Key::Shift, false);
    }
    event(app, window, KeyCode::AltLeft, Key::Alt, false);
}

fn focused(app: &App, root: Entity) -> Option<Entity> {
    catalogue::current(app.world(), root).map(|target| target.entity)
}

#[test]
fn quick_batched_switch_reverse_and_two_gestures_follow_committed_history() {
    let (mut app, root, window, items) = fixture();
    gesture(&mut app, window, false);
    app.update();
    assert_eq!(focused(&app, root), Some(items[1]));
    assert!(!active(app.world()));
    gesture(&mut app, window, false);
    gesture(&mut app, window, false);
    app.update();
    assert_eq!(focused(&app, root), Some(items[1]));
    gesture(&mut app, window, true);
    app.update();
    assert_eq!(focused(&app, root), Some(items[0]));
}

#[test]
fn restoring_the_same_held_modifier_does_not_commit_the_recent_window_preview() {
    let (mut app, root, window, items) = fixture();
    event(&mut app, window, KeyCode::AltLeft, Key::Alt, true);
    event(&mut app, window, KeyCode::Tab, Key::Tab, true);
    event(&mut app, window, KeyCode::Tab, Key::Tab, false);
    app.update();
    assert!(active(app.world()));
    crate::native::synchronize_keyboard_modifiers(
        app.world_mut(),
        window,
        &[(KeyCode::AltLeft, Key::Alt)],
    );
    app.update();
    assert!(active(app.world()));
    assert_eq!(focused(&app, root), Some(items[2]));
    event(&mut app, window, KeyCode::AltLeft, Key::Alt, false);
    app.update();
    assert!(!active(app.world()));
    assert_eq!(focused(&app, root), Some(items[1]));
}

#[test]
fn previews_cancel_without_changing_camera_layout_or_native_draft() {
    let (mut app, root, window, items) = fixture();
    let editor = app
        .world_mut()
        .spawn((
            crate::native::sand::editable("retained draft"),
            ChildOf(items[2]),
        ))
        .id();
    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(editor, FocusCause::Pressed);
    let view = *app.world().get::<CanvasView>(root).unwrap();
    let layout =
        serde_json::to_value(&app.world().get::<Workspaces>(root).unwrap().entries).unwrap();
    event(&mut app, window, KeyCode::AltLeft, Key::Alt, true);
    event(&mut app, window, KeyCode::Tab, Key::Tab, true);
    event(&mut app, window, KeyCode::Tab, Key::Tab, false);
    app.update();
    assert!(active(app.world()));
    assert_eq!(focused(&app, root), Some(items[2]));
    assert_eq!(
        serde_json::to_value(&app.world().get::<Workspaces>(root).unwrap().entries).unwrap(),
        layout
    );
    event(&mut app, window, KeyCode::Escape, Key::Escape, true);
    event(&mut app, window, KeyCode::Escape, Key::Escape, false);
    event(&mut app, window, KeyCode::AltLeft, Key::Alt, false);
    app.update();
    assert!(!active(app.world()));
    assert_eq!(focused(&app, root), Some(items[2]));
    assert_eq!(app.world().resource::<InputFocus>().get(), Some(editor));
    assert_eq!(
        app.world()
            .get::<bevy::text::EditableText>(editor)
            .unwrap()
            .value(),
        "retained draft"
    );
    assert_eq!(
        (
            app.world().get::<CanvasView>(root).unwrap().center,
            app.world().get::<CanvasView>(root).unwrap().zoom
        ),
        (view.center, view.zoom)
    );
}

#[test]
fn reverse_shift_and_one_of_two_remapped_hold_contributors_cannot_commit() {
    let (mut app, root, window, items) = fixture();
    event(&mut app, window, KeyCode::CapsLock, Key::Alt, true);
    event(&mut app, window, KeyCode::ControlRight, Key::Alt, true);
    event(&mut app, window, KeyCode::ShiftLeft, Key::Shift, true);
    event(&mut app, window, KeyCode::Tab, Key::Tab, true);
    event(&mut app, window, KeyCode::Tab, Key::Tab, false);
    event(&mut app, window, KeyCode::ShiftLeft, Key::Shift, false);
    event(&mut app, window, KeyCode::CapsLock, Key::Alt, false);
    app.update();
    assert!(active(app.world()));
    assert_eq!(focused(&app, root), Some(items[2]));
    event(&mut app, window, KeyCode::ControlRight, Key::Alt, false);
    app.update();
    assert!(!active(app.world()));
    assert_eq!(focused(&app, root), Some(items[0]));
}

#[test]
fn queued_gestures_project_history_until_the_host_acknowledges_and_reject_stale_identity() {
    let (mut app, root, window, items) = fixture();
    let (sender, receiver) = std::sync::mpsc::sync_channel(4);
    app.world_mut().insert_resource(HostRequests(sender));
    gesture(&mut app, window, false);
    gesture(&mut app, window, false);
    app.update();
    let requests: Vec<_> = receiver
        .try_iter()
        .map(|request| {
            let crate::host::HostRequest::Action(DesktopAction::CommitRecent { request }) = request
            else {
                panic!("unexpected host request");
            };
            request
        })
        .collect();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        selection(app.world(), root, requests[0]).unwrap().1,
        app.world().get::<LayoutBox>(items[1]).unwrap().id
    );
    assert_eq!(
        selection(app.world(), root, requests[1]).unwrap().1,
        app.world().get::<LayoutBox>(items[2]).unwrap().id
    );
    acknowledge(app.world_mut(), Entity::PLACEHOLDER, requests[0], false);
    assert!(selection(app.world(), root, requests[0]).is_some());
    assert!(activate(app.world_mut(), root, requests[0]));
    assert!(activate(app.world_mut(), root, requests[1]));
    assert_eq!(focused(&app, root), Some(items[2]));
    gesture(&mut app, window, false);
    app.update();
    let crate::host::HostRequest::Action(DesktopAction::CommitRecent { request }) =
        receiver.try_recv().unwrap()
    else {
        panic!("unexpected host request");
    };
    let identity = app.world().get::<LayoutBox>(items[1]).unwrap().id;
    app.world_mut().despawn(items[1]);
    let mut replacement = LayoutBox::new(Vec2::new(300.0, 200.0));
    replacement.id = identity;
    app.world_mut().spawn((
        CanvasItem {
            position: bevy::math::DVec2::ZERO,
            size: Vec2::splat(100.0),
        },
        replacement,
        WorkspaceMember(1),
        ChildOf(root),
    ));
    assert!(selection(app.world(), root, request).is_none());
    assert!(!activate(app.world_mut(), root, request));
    assert_eq!(focused(&app, root), Some(items[2]));
}

#[test]
fn disappeared_preview_prunes_in_place_and_minimized_parent_and_transients_are_excluded() {
    let (mut app, root, window, items) = fixture();
    event(&mut app, window, KeyCode::AltLeft, Key::Alt, true);
    event(&mut app, window, KeyCode::Tab, Key::Tab, true);
    event(&mut app, window, KeyCode::Tab, Key::Tab, false);
    app.update();
    app.world_mut().despawn(items[1]);
    update(app.world_mut());
    let session = app.world().resource::<Recent>().session.as_ref().unwrap();
    assert_eq!(session.entries.len(), 2);
    assert_eq!(session.entries[session.selected].entity, items[0]);
    event(&mut app, window, KeyCode::AltLeft, Key::Alt, false);
    app.update();
    assert_eq!(focused(&app, root), Some(items[0]));
    app.world_mut()
        .entity_mut(items[2])
        .insert(TransientParent(items[0]));
    assert_eq!(catalogue::entries(app.world(), root).len(), 1);
    let id = PlacementId(app.world().get::<LayoutBox>(items[0]).unwrap().id);
    app.world_mut().get_mut::<Workspaces>(root).unwrap().entries[0]
        .desktop
        .set_minimized(id, true);
    assert!(catalogue::entries(app.world(), root).is_empty());
}

#[test]
fn a_newer_chooser_survives_an_older_host_commit_and_keeps_its_snapshot() {
    let (mut app, root, window, items) = fixture();
    let (sender, receiver) = std::sync::mpsc::sync_channel(4);
    app.world_mut().insert_resource(HostRequests(sender));
    gesture(&mut app, window, false);
    event(&mut app, window, KeyCode::AltLeft, Key::Alt, true);
    event(&mut app, window, KeyCode::Tab, Key::Tab, true);
    event(&mut app, window, KeyCode::Tab, Key::Tab, false);
    app.update();
    let crate::host::HostRequest::Action(DesktopAction::CommitRecent { request }) =
        receiver.try_recv().unwrap()
    else {
        panic!("unexpected request");
    };
    assert!(active(app.world()));
    let selected = app
        .world()
        .resource::<Recent>()
        .session
        .as_ref()
        .unwrap()
        .entries[1];
    assert_eq!(selected.entity, items[2]);
    assert!(activate(app.world_mut(), root, request));
    assert!(active(app.world()));
    let session = app.world().resource::<Recent>().session.as_ref().unwrap();
    assert_eq!(session.entries[session.selected].entity, items[2]);
    assert!(focus::contains_input(
        app.world(),
        app.world().resource::<InputFocus>().get().unwrap(),
        session.overlay
    ));
    event(&mut app, window, KeyCode::AltLeft, Key::Alt, false);
    app.update();
    let crate::host::HostRequest::Action(DesktopAction::CommitRecent { request }) =
        receiver.try_recv().unwrap()
    else {
        panic!("unexpected request");
    };
    assert!(activate(app.world_mut(), root, request));
    assert_eq!(focused(&app, root), Some(items[2]));
}

#[test]
fn rebindings_use_the_common_hold_modifier_and_repeat_does_not_advance() {
    use crate::native::shortcuts::Shortcut;
    let (mut app, root, window, items) = fixture();
    let mut settings = app.world().resource::<Settings>().clone();
    settings
        .stage(Shortcut::RecentNext, Some("Ctrl+Tab"))
        .unwrap();
    settings
        .stage(Shortcut::RecentPrevious, Some("Ctrl+Shift+Tab"))
        .unwrap();
    settings.validate().unwrap();
    app.world_mut().insert_resource(settings);
    event(&mut app, window, KeyCode::CapsLock, Key::Control, true);
    event(&mut app, window, KeyCode::Tab, Key::Tab, true);
    app.update();
    assert!(active(app.world()));
    app.world_mut().write_message(KeyboardInput {
        window,
        key_code: KeyCode::Tab,
        logical_key: Key::Tab,
        state: ButtonState::Pressed,
        repeat: true,
        text: None,
    });
    app.update();
    event(&mut app, window, KeyCode::Tab, Key::Tab, false);
    event(&mut app, window, KeyCode::CapsLock, Key::Control, false);
    app.update();
    assert_eq!(focused(&app, root), Some(items[1]));
}

#[test]
fn input_focus_loss_cancels_and_an_explicitly_unfocused_single_item_can_be_selected() {
    let (mut app, root, window, items) = fixture();
    event(&mut app, window, KeyCode::AltLeft, Key::Alt, true);
    event(&mut app, window, KeyCode::Tab, Key::Tab, true);
    app.update();
    app.world_mut().get_mut::<Window>(window).unwrap().focused = false;
    update(app.world_mut());
    assert!(!active(app.world()));
    assert_eq!(focused(&app, root), Some(items[2]));
    app.world_mut().get_mut::<Window>(window).unwrap().focused = true;
    app.world_mut().despawn(items[0]);
    app.world_mut().despawn(items[1]);
    app.world_mut().get_mut::<Workspaces>(root).unwrap().entries[0]
        .desktop
        .clear_focus();
    cycle(app.world_mut(), root, false);
    assert_eq!(focused(&app, root), Some(items[2]));
    let snapshot =
        serde_json::to_value(&app.world().get::<Workspaces>(root).unwrap().entries).unwrap();
    cycle(app.world_mut(), root, false);
    assert_eq!(
        serde_json::to_value(&app.world().get::<Workspaces>(root).unwrap().entries).unwrap(),
        snapshot
    );
}

#[test]
fn a_full_host_queue_restores_focus_without_promoting_an_uncommitted_target() {
    let (mut app, root, window, items) = fixture();
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    sender
        .try_send(crate::host::HostRequest::Close(SurfaceId(900)))
        .unwrap();
    app.world_mut().insert_resource(HostRequests(sender));
    gesture(&mut app, window, false);
    app.update();
    assert_eq!(focused(&app, root), Some(items[2]));
    assert!(app.world().resource::<Recent>().pending.is_empty());
    assert!(!active(app.world()));
    assert_eq!(
        receiver.try_recv(),
        Ok(crate::host::HostRequest::Close(SurfaceId(900)))
    );
    gesture(&mut app, window, false);
    app.update();
    let crate::host::HostRequest::Action(DesktopAction::CommitRecent { request }) =
        receiver.try_recv().unwrap()
    else {
        panic!("unexpected request");
    };
    assert_eq!(
        selection(app.world(), root, request).unwrap().1,
        app.world().get::<LayoutBox>(items[1]).unwrap().id
    );
}

#[test]
fn normal_mru_selection_leaves_another_items_fullscreen_and_stale_modal_decisions_do_nothing() {
    let (mut app, root, window, items) = fixture();
    DesktopAction::ToggleFullscreen.apply(app.world_mut(), root);
    gesture(&mut app, window, false);
    app.update();
    assert_eq!(focused(&app, root), Some(items[1]));
    assert_eq!(
        app.world().get::<Workspaces>(root).unwrap().entries[0]
            .desktop
            .fullscreen(),
        None
    );
    event(&mut app, window, KeyCode::AltLeft, Key::Alt, true);
    event(&mut app, window, KeyCode::Tab, Key::Tab, true);
    event(&mut app, window, KeyCode::Tab, Key::Tab, false);
    app.update();
    let old = app
        .world()
        .resource::<Recent>()
        .session
        .as_ref()
        .unwrap()
        .overlay;
    let modal = app.world_mut().spawn_empty().id();
    crate::native::shortcuts::own_keyboard(app.world_mut(), modal);
    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(modal, FocusCause::Navigated);
    update(app.world_mut());
    assert!(app.world().get_entity(old).is_err());
    assert_eq!(app.world().resource::<InputFocus>().get(), Some(modal));
    Change::Choose(Target {
        entity: items[2],
        placement: PlacementId(app.world().get::<LayoutBox>(items[2]).unwrap().id),
    })
    .apply(app.world_mut(), old);
    assert_eq!(focused(&app, root), Some(items[1]));
    assert_eq!(
        app.world()
            .resource::<crate::native::shortcuts::KeyboardModalOwner>()
            .0,
        Some(modal)
    );
}

#[test]
fn park_keymap_roles_drive_hold_release_even_when_logical_symbols_are_not_modifiers() {
    use crate::host::input::{KeyLabel, KeyModifiers, KeyboardSnapshot};
    let (mut app, root, window, items) = fixture();
    app.insert_resource(KeyboardSnapshot {
        labels: std::sync::Arc::new(BTreeMap::from([
            (
                "CapsLock".into(),
                KeyLabel {
                    is_modifier: true,
                    modifiers: KeyModifiers {
                        alt: true,
                        ..default()
                    },
                    ..default()
                },
            ),
            (
                "KeyQ".into(),
                KeyLabel {
                    is_modifier: true,
                    modifiers: KeyModifiers {
                        alt: true,
                        ..default()
                    },
                    ..default()
                },
            ),
        ])),
        ..default()
    });
    event(
        &mut app,
        window,
        KeyCode::CapsLock,
        Key::Character("a".into()),
        true,
    );
    event(
        &mut app,
        window,
        KeyCode::KeyQ,
        Key::Character("q".into()),
        true,
    );
    event(&mut app, window, KeyCode::Tab, Key::Tab, true);
    event(&mut app, window, KeyCode::Tab, Key::Tab, false);
    event(
        &mut app,
        window,
        KeyCode::CapsLock,
        Key::Character("a".into()),
        false,
    );
    app.update();
    assert!(active(app.world()));
    event(
        &mut app,
        window,
        KeyCode::KeyQ,
        Key::Character("q".into()),
        false,
    );
    app.update();
    assert!(!active(app.world()));
    assert_eq!(focused(&app, root), Some(items[1]));
}

#[test]
fn a_configured_directional_chord_keeps_its_command_instead_of_using_chooser_direction() {
    use crate::native::shortcuts::Shortcut;
    let (mut app, root, window, items) = fixture();
    let mut settings = Settings::default();
    settings
        .stage(Shortcut::RecentNext, Some("Ctrl+Up"))
        .unwrap();
    settings
        .stage(Shortcut::RecentPrevious, Some("Ctrl+Shift+Up"))
        .unwrap();
    settings.validate().unwrap();
    app.insert_resource(settings);
    event(&mut app, window, KeyCode::ControlLeft, Key::Control, true);
    for _ in 0..2 {
        event(&mut app, window, KeyCode::ArrowUp, Key::ArrowUp, true);
        event(&mut app, window, KeyCode::ArrowUp, Key::ArrowUp, false);
    }
    event(&mut app, window, KeyCode::ControlLeft, Key::Control, false);
    app.update();
    assert_eq!(focused(&app, root), Some(items[0]));
}

#[test]
fn cancelling_a_newer_chooser_after_an_older_cross_workspace_commit_preserves_normal_editor_return()
{
    let (mut app, root, window, items) = fixture();
    let mut other = app.world().get::<Workspaces>(root).unwrap().entries[0].clone();
    other.id = 2;
    other.desktop = crate::layout::desktop::Workspace::new(WorkspaceLayout::Free);
    app.world_mut()
        .get_mut::<Workspaces>(root)
        .unwrap()
        .entries
        .push(other);
    app.world_mut()
        .entity_mut(items[2])
        .insert(WorkspaceMember(2));
    crate::native::layout::engine::resolve(app.world_mut());
    let editors = [items[1], items[2]].map(|item| {
        app.world_mut()
            .spawn((
                crate::native::sand::editable("retained workspace editor"),
                ChildOf(item),
            ))
            .id()
    });
    focus_item(app.world_mut(), items[1]);
    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(editors[0], FocusCause::Pressed);
    observe(app.world_mut(), root);
    crate::native::workspace::switch_output(app.world_mut(), root, 2);
    focus_item(app.world_mut(), items[2]);
    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(editors[1], FocusCause::Pressed);
    observe(app.world_mut(), root);
    let (sender, receiver) = std::sync::mpsc::sync_channel(4);
    app.world_mut().insert_resource(HostRequests(sender));
    gesture(&mut app, window, false);
    event(&mut app, window, KeyCode::AltLeft, Key::Alt, true);
    event(&mut app, window, KeyCode::Tab, Key::Tab, true);
    event(&mut app, window, KeyCode::Tab, Key::Tab, false);
    app.update();
    let crate::host::HostRequest::Action(DesktopAction::CommitRecent { request }) =
        receiver.try_recv().unwrap()
    else {
        panic!("unexpected request");
    };
    assert_eq!(selection(app.world(), root, request).unwrap().0, 1);
    assert!(activate(app.world_mut(), root, request));
    assert!(active(app.world()));
    let overlay = app
        .world()
        .resource::<Recent>()
        .session
        .as_ref()
        .unwrap()
        .overlay;
    Change::Cancel.apply(app.world_mut(), overlay);
    event(&mut app, window, KeyCode::AltLeft, Key::Alt, false);
    app.update();
    assert_eq!(app.world().resource::<InputFocus>().get(), Some(editors[0]));
    assert!(receiver.try_recv().is_err());
    crate::native::workspace::switch_output(app.world_mut(), root, 2);
    assert_eq!(app.world().resource::<InputFocus>().get(), Some(editors[1]));
    assert_eq!(
        app.world()
            .get::<bevy::text::EditableText>(editors[1])
            .unwrap()
            .value(),
        "retained workspace editor"
    );
}
