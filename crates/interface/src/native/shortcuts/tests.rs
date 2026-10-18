use super::*;
use crate::native::actions::Action;
use bevy::{
    input::{
        ButtonState, InputPlugin, InputSystems,
        keyboard::{Key, KeyboardInput},
    },
    input_focus::{FocusCause, InputFocus, InputFocusSystems, dispatch_focused_input},
    window::PrimaryWindow,
};

#[cfg_attr(test, test)]
fn desktop_chords_allow_super_space_and_arrows_without_relaxing_text_shortcuts() {
    let mut settings = Settings::default();
    settings.validate().unwrap();
    assert!(model::Chord::parse("Super+Space").is_err());
    assert!(model::Chord::parse("Super+Left").is_err());
    assert!(settings.chord(Shortcut::DesktopOperation).is_some());
    assert!(settings.chord(Shortcut::FocusLeftArrow).is_some());
    assert!(settings.set(Shortcut::FocusLeftArrow, "Left").is_err());
    assert!(settings.set(Shortcut::DesktopOperation, "Super+H").is_err());
    settings
        .set(Shortcut::DesktopOperation, "Alt+Space")
        .unwrap();
    assert_eq!(settings.text(Shortcut::DesktopOperation), "Alt+Space");
}

#[cfg_attr(test, test)]
fn settings_reject_conflicts_and_reserved_keys_without_partial_changes() {
    let mut settings = Settings::default();
    settings.validate().unwrap();
    for value in [
        "W",
        "Escape",
        "Shift+Tab",
        "Ctrl+Enter",
        "Alt+Space",
        "Ctrl+C",
        "Super+C",
        "Command+Left",
        "Alt+Home",
        "Ctrl+Delete",
        "Ctrl+Ctrl+D",
        "unknown",
    ] {
        assert!(
            settings.set(Shortcut::DeleteSand, value).is_err(),
            "{value}"
        );
        assert_eq!(settings, Settings::default());
    }
    settings
        .set(Shortcut::DeleteSand, "shift + control + d")
        .unwrap();
    assert_eq!(settings.text(Shortcut::DeleteSand), "Ctrl+Shift+D");
    settings.set(Shortcut::Forward, "Unbound").unwrap();
    settings.set(Shortcut::DeleteSand, "W").unwrap();
    assert!(settings.reset(Shortcut::Forward).is_err());
    assert!(settings.chord(Shortcut::Forward).is_none());
    settings.reset(Shortcut::DeleteSand).unwrap();
    settings.reset(Shortcut::Forward).unwrap();
    assert_eq!(settings, Settings::default());
}

fn fixture() -> (App, Entity, Entity, Entity) {
    let mut app = App::new();
    crate::native::laboratory::isolate(app.world_mut());
    app.add_plugins((
        InputPlugin,
        crate::native::actions::ActionsPlugin,
        crate::native::deletion::DeletionPlugin,
    ))
    .init_resource::<Assets<Font>>()
    .init_resource::<crate::native::theme::Typography>()
    .init_resource::<Settings>()
    .add_systems(
        PreUpdate,
        dispatch_focused_input::<KeyboardInput>
            .in_set(InputFocusSystems::Dispatch)
            .after(InputSystems),
    );
    let root = app
        .world_mut()
        .spawn(crate::native::workspace::Workspaces::default())
        .id();
    let sand = app
        .world_mut()
        .spawn((
            crate::native::canvas::CanvasItem {
                position: bevy::math::DVec2::ZERO,
                size: Vec2::splat(100.0),
            },
            crate::native::workspace::WorkspaceMember(1),
            ChildOf(root),
        ))
        .id();
    app.world_mut()
        .entity_mut(root)
        .insert(crate::native::canvas_selection::SandSelection(vec![sand]));
    let window = app
        .world_mut()
        .spawn((
            Window::default(),
            PrimaryWindow,
            crate::native::actions::WindowActionTarget(root),
        ))
        .id();
    install(app.world_mut(), root);
    (app, root, sand, window)
}

fn key(app: &mut App, window: Entity, key_code: KeyCode, state: ButtonState) {
    app.world_mut().write_message(KeyboardInput {
        key_code,
        logical_key: Key::Unidentified(bevy::input::keyboard::NativeKey::Unidentified),
        state,
        text: None,
        repeat: false,
        window,
    });
    app.update();
}

#[cfg_attr(test, test)]
fn default_delete_uses_confirmation_and_leaves_text_fields_alone() {
    let (mut app, root, sand, window) = fixture();
    let editor = app
        .world_mut()
        .spawn((crate::native::sand::editable("draft"), ChildOf(sand)))
        .id();
    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(editor, FocusCause::Pressed);
    key(&mut app, window, KeyCode::Delete, ButtonState::Pressed);
    assert_eq!(app.world().resource::<InputFocus>().get(), Some(editor));
    assert_eq!(
        app.world()
            .get::<bevy::text::EditableText>(editor)
            .unwrap()
            .value(),
        "draft"
    );
    key(&mut app, window, KeyCode::Delete, ButtonState::Released);
    app.world_mut().resource_mut::<InputFocus>().clear();
    key(&mut app, window, KeyCode::Delete, ButtonState::Pressed);
    assert!(app.world().resource::<InputFocus>().get().is_some());
    assert!(app.world().get_entity(sand).is_ok());
    crate::native::deletion::Decision(false).apply(app.world_mut(), root);
    assert!(app.world().get_entity(sand).is_ok());
    key(&mut app, window, KeyCode::Delete, ButtonState::Released);
    key(&mut app, window, KeyCode::Delete, ButtonState::Pressed);
    crate::native::deletion::Decision(true).apply(app.world_mut(), root);
    assert!(app.world().get_entity(sand).is_err());
}

#[cfg_attr(test, test)]
fn remapped_delete_works_outside_edit_mode_and_ignores_text_focus() {
    let (mut app, root, sand, window) = fixture();
    app.world_mut()
        .resource_mut::<Settings>()
        .set(Shortcut::DeleteSand, "Ctrl+Shift+D")
        .unwrap();
    install(app.world_mut(), root);
    key(&mut app, window, KeyCode::Delete, ButtonState::Pressed);
    assert!(app.world().resource::<InputFocus>().get().is_none());
    key(&mut app, window, KeyCode::Delete, ButtonState::Released);
    let editor = app
        .world_mut()
        .spawn((crate::native::sand::editable("keep this"), ChildOf(sand)))
        .id();
    let caret = app.world_mut().spawn(ChildOf(editor)).id();
    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(caret, FocusCause::Pressed);
    key(&mut app, window, KeyCode::ControlLeft, ButtonState::Pressed);
    key(&mut app, window, KeyCode::ShiftLeft, ButtonState::Pressed);
    key(&mut app, window, KeyCode::KeyD, ButtonState::Pressed);
    assert_eq!(app.world().resource::<InputFocus>().get(), Some(caret));
    assert!(app.world().get_entity(sand).is_ok());
    key(&mut app, window, KeyCode::KeyD, ButtonState::Released);
    app.world_mut().resource_mut::<InputFocus>().clear();
    key(&mut app, window, KeyCode::KeyD, ButtonState::Pressed);
    assert!(app.world().resource::<InputFocus>().get().is_some());
    assert!(app.world().get_entity(sand).is_ok());
    crate::native::deletion::Decision(true).apply(app.world_mut(), root);
    assert!(app.world().get_entity(sand).is_err());
}

#[cfg_attr(test, test)]
fn scoped_actions_override_configured_bindings_and_unbinding_removes_them() {
    #[derive(Component, Default)]
    struct Count(u32);
    struct Increment;
    impl Action for Increment {
        fn apply(&self, world: &mut World, target: Entity) {
            world.get_mut::<Count>(target).unwrap().0 += 1;
        }
    }
    let (mut app, root, sand, window) = fixture();
    let scope = app
        .world_mut()
        .spawn((
            Count::default(),
            ChildOf(root),
            crate::native::actions::KeyBindings(vec![crate::native::actions::KeyBinding::new(
                KeyCode::Delete,
                crate::native::actions::Modifiers::NONE,
                crate::native::actions![Increment],
            )]),
        ))
        .id();
    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(scope, FocusCause::Pressed);
    key(&mut app, window, KeyCode::Delete, ButtonState::Pressed);
    assert_eq!(app.world().get::<Count>(scope).unwrap().0, 1);
    assert!(app.world().get_entity(sand).is_ok());
    key(&mut app, window, KeyCode::Delete, ButtonState::Released);
    app.world_mut()
        .resource_mut::<Settings>()
        .set(Shortcut::DeleteSand, "Unbound")
        .unwrap();
    install(app.world_mut(), root);
    app.world_mut().resource_mut::<InputFocus>().clear();
    key(&mut app, window, KeyCode::Delete, ButtonState::Pressed);
    assert!(app.world().resource::<InputFocus>().get().is_none());
    assert!(app.world().get_entity(sand).is_ok());
}

#[cfg_attr(test, test)]
fn settings_panel_saves_reports_conflicts_and_resets_active_bindings() {
    let (mut app, root) = crate::native::edit_mode::tests::fixture();
    crate::native::edit_mode::EditAction::Open.apply(app.world_mut(), root);
    crate::native::edit_mode::EditAction::Shortcuts.apply(app.world_mut(), root);
    app.update();
    let row = app
        .world_mut()
        .query::<(Entity, &BindingRow)>()
        .iter(app.world())
        .find(|(_, row)| row.shortcut == Shortcut::DeleteSand)
        .map(|(e, _)| e)
        .unwrap();
    let editor = app.world().get::<BindingRow>(row).unwrap().editor;
    let status = app.world().get::<BindingRow>(row).unwrap().status;
    let active = app.world().get::<BindingRow>(row).unwrap().active;
    let save = app
        .world_mut()
        .query::<(Entity, &crate::native::actions::ActionButton)>()
        .iter(app.world())
        .find(|(entity, button)| {
            button.target == row
                && app
                    .world()
                    .get::<Children>(*entity)
                    .is_some_and(|children| {
                        children.iter().any(|child| {
                            app.world()
                                .get::<Text>(child)
                                .is_some_and(|text| text.0 == "Save")
                        })
                    })
        })
        .map(|(entity, _)| entity)
        .unwrap();
    app.world_mut()
        .get_mut::<bevy::text::EditableText>(editor)
        .unwrap()
        .editor
        .set_text("W");
    app.world_mut()
        .trigger(bevy::ui_widgets::Activate { entity: save });
    app.update();
    assert!(
        app.world()
            .get::<Text>(status)
            .unwrap()
            .0
            .contains("conflicting")
    );
    assert_eq!(
        app.world()
            .resource::<Settings>()
            .text(Shortcut::DeleteSand),
        "Delete"
    );
    app.world_mut()
        .get_mut::<bevy::text::EditableText>(editor)
        .unwrap()
        .editor
        .set_text("F2");
    app.world_mut()
        .trigger(bevy::ui_widgets::Activate { entity: save });
    app.update();
    assert_eq!(app.world().get::<Text>(active).unwrap().0, "Active: F2");
    assert_eq!(
        app.world().get::<ConfiguredBindings>(root).unwrap().0[0].key,
        KeyCode::F2
    );
    Change::Unbind.apply(app.world_mut(), row);
    assert!(
        app.world()
            .resource::<Settings>()
            .chord(Shortcut::DeleteSand)
            .is_none()
    );
    Change::Reset.apply(app.world_mut(), row);
    assert_eq!(app.world().get::<Text>(active).unwrap().0, "Active: Delete");
    app.world_mut()
        .resource_mut::<Settings>()
        .set(Shortcut::Forward, "F3")
        .unwrap();
    Change::ResetAll.apply(app.world_mut(), root);
    assert_eq!(app.world().resource::<Settings>(), &Settings::default());
}

#[cfg_attr(test, test)]
fn movement_bindings_match_modifiers_and_ignore_old_keys() {
    let mut world = World::new();
    world.init_resource::<Settings>();
    world
        .resource_mut::<Settings>()
        .set(Shortcut::Forward, "Ctrl+F2")
        .unwrap();
    let mut keys = ButtonInput::default();
    keys.press(KeyCode::KeyW);
    assert!(!pressed(&world, Shortcut::Forward, &keys));
    keys.press(KeyCode::F2);
    assert!(!pressed(&world, Shortcut::Forward, &keys));
    keys.press(KeyCode::ControlLeft);
    assert!(pressed(&world, Shortcut::Forward, &keys));
    keys.press(KeyCode::ShiftLeft);
    assert!(!pressed(&world, Shortcut::Forward, &keys));
}

#[cfg_attr(test, test)]
fn remapped_monitor_workspace_and_record_shortcuts_reach_the_host() {
    use crate::host::{DesktopAction, Direction, HostRequest};
    let (mut app, root, _, window) = fixture();
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    app.insert_resource(crate::native::layout::desktop::HostRequests(sender));
    for (shortcut, action) in [
        (
            Shortcut::FocusMonitorRight,
            DesktopAction::FocusOutput(Direction::Right),
        ),
        (
            Shortcut::MoveMonitorRight,
            DesktopAction::MoveToOutput(Direction::Right),
        ),
        (
            Shortcut::WorkspaceMonitorRight,
            DesktopAction::MoveWorkspaceToOutput(Direction::Right),
        ),
        (Shortcut::WindowRecord, DesktopAction::InspectWindowRecord),
        (Shortcut::Terminal, DesktopAction::OpenTerminal),
        (Shortcut::Launcher, DesktopAction::OpenLauncher),
        (Shortcut::Lock, DesktopAction::Lock),
        (Shortcut::Logout, DesktopAction::Logout),
    ] {
        app.world_mut()
            .resource_mut::<Settings>()
            .set(shortcut, "Alt+Shift+R")
            .unwrap();
        install(app.world_mut(), root);
        key(&mut app, window, KeyCode::AltLeft, ButtonState::Pressed);
        key(&mut app, window, KeyCode::ShiftLeft, ButtonState::Pressed);
        key(&mut app, window, KeyCode::KeyR, ButtonState::Pressed);
        assert_eq!(receiver.try_recv().unwrap(), HostRequest::Action(action));
        key(&mut app, window, KeyCode::KeyR, ButtonState::Released);
        key(&mut app, window, KeyCode::ShiftLeft, ButtonState::Released);
        key(&mut app, window, KeyCode::AltLeft, ButtonState::Released);
        app.world_mut()
            .resource_mut::<Settings>()
            .reset(shortcut)
            .unwrap();
    }
}

crate::native::laboratory_cases! {
    desktop_chords_allow_super_space_and_arrows_without_relaxing_text_shortcuts,
    settings_reject_conflicts_and_reserved_keys_without_partial_changes,
    default_delete_uses_confirmation_and_leaves_text_fields_alone,
    remapped_delete_works_outside_edit_mode_and_ignores_text_focus,
    scoped_actions_override_configured_bindings_and_unbinding_removes_them,
    settings_panel_saves_reports_conflicts_and_resets_active_bindings,
    movement_bindings_match_modifiers_and_ignore_old_keys,
    remapped_monitor_workspace_and_record_shortcuts_reach_the_host,
}
