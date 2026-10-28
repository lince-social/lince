use super::*;

#[test]
fn recent_window_pair_validates_a_shared_hold_modifier_and_can_be_unbound() {
    let mut settings = Settings::default();
    settings.validate().unwrap();
    assert_eq!(settings.text(Shortcut::RecentNext), "Alt+Tab");
    assert_eq!(settings.text(Shortcut::RecentPrevious), "Alt+Shift+Tab");
    assert!(!Shortcut::RecentNext.repeats());
    assert_eq!(
        Shortcut::from_operation("/focus/recent"),
        Some(Shortcut::RecentNext)
    );
    settings
        .stage(Shortcut::RecentNext, Some("Ctrl+Tab"))
        .unwrap();
    assert!(settings.validate().unwrap_err().contains("shared"));
    settings
        .stage(Shortcut::RecentPrevious, Some("Ctrl+Shift+Tab"))
        .unwrap();
    settings.validate().unwrap();
    settings
        .stage(Shortcut::RecentNext, Some("Shift+Tab"))
        .unwrap_err();
    settings
        .stage(Shortcut::RecentNext, Some("Unbound"))
        .unwrap();
    settings.validate().unwrap();
    settings
        .stage(Shortcut::RecentPrevious, Some("Unbound"))
        .unwrap();
    settings.validate().unwrap();
    assert_eq!(settings.recent_hold().unwrap(), None);
    settings
        .stage(Shortcut::RecentNext, Some("Alt+Escape"))
        .unwrap();
    assert!(settings.validate().unwrap_err().contains("Escape"));
}

#[test]
fn dedicated_hardware_keys_capture_round_trip_and_match_the_linux_input_converter() {
    use crate::host::external::Action;
    use bevy::input::keyboard::NativeKeyCode;
    use model::Chord;
    let defaults = Settings::default();
    for action in Action::ALL {
        let shortcut = Shortcut::External(action);
        assert_eq!(
            shortcut.desktop_action(),
            Some(crate::host::DesktopAction::External(action))
        );
        assert_eq!(Shortcut::from_id(action.id()), Some(shortcut));
        assert_eq!(shortcut.repeats(), action.repeats());
        assert_eq!(shortcut.scope(), Scope::Park);
        if let Some(chord) = defaults.chord(shortcut) {
            let captured = Chord::from_input(chord.key, chord.modifiers)
                .unwrap()
                .text();
            assert_eq!(Chord::parse_desktop(&captured).unwrap(), chord);
        }
    }
    assert!(Chord::parse_desktop("B").is_err());
    assert!(Chord::parse_desktop("Shift+B").is_err());
    assert!(Chord::parse_desktop("Print").is_ok());
    #[cfg(target_os = "linux")]
    {
        use winit::{keyboard::PhysicalKey, platform::scancode::PhysicalKeyExtScancode};
        for (action, code) in [
            (Action::BrightnessDown, 224),
            (Action::BrightnessUp, 225),
            (Action::MicrophoneMute, 248),
        ] {
            let physical = bevy::winit::converters::convert_physical_key_code(
                PhysicalKey::from_scancode(code),
            );
            assert_eq!(physical, KeyCode::Unidentified(NativeKeyCode::Xkb(code)));
            assert_eq!(
                defaults.chord(Shortcut::External(action)).unwrap().key,
                physical
            );
        }
    }
}

#[test]
fn hardware_binding_conflicts_do_not_make_other_commands_lock_eligible() {
    use crate::host::external::Action;
    let mut settings = Settings::default();
    assert!(
        settings
            .set(Shortcut::DeleteSand, "XF86AudioRaiseVolume")
            .is_err()
    );
    settings
        .set(Shortcut::External(Action::VolumeUp), "Unbound")
        .unwrap();
    settings
        .set(Shortcut::DeleteSand, "XF86AudioRaiseVolume")
        .unwrap();
    let mut world = World::new();
    world.insert_resource(settings);
    let (sender, _receiver) = std::sync::mpsc::sync_channel(2);
    world.insert_resource(crate::native::layout::desktop::HostRequests(sender));
    assert!(binding_snapshot(&world).1.iter().all(|binding| {
        matches!(binding.action, crate::host::DesktopAction::External(action) if action.lockable())
    }));
    assert!(
        !binding_snapshot(&world)
            .1
            .iter()
            .any(|binding| binding.key == KeyCode::AudioVolumeUp)
    );
}
#[test]
fn park_only_commands_do_not_intercept_sandbox_input_without_a_host() {
    let mut world = World::new();
    world.insert_resource(Settings::default());
    let (active, hardware) = binding_snapshot(&world);
    assert!(!active.is_empty());
    assert!(active.iter().all(|binding| !binding.action.requires_host()));
    assert!(hardware.is_empty());
    let (sender, _receiver) = std::sync::mpsc::sync_channel(2);
    world.insert_resource(crate::native::layout::desktop::HostRequests(sender));
    let (_, hardware) = binding_snapshot(&world);
    assert_eq!(hardware.len(), 9);
}

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
    if cfg!(target_os = "linux") {
        assert!(
            settings
                .set(Shortcut::FocusLeftArrow, "Ctrl+Alt+F3")
                .unwrap_err()
                .contains("virtual terminals")
        );
    }
    assert!(settings.set(Shortcut::DesktopOperation, "Super+H").is_err());
    settings
        .set(Shortcut::DesktopOperation, "Alt+Space")
        .unwrap();
    assert_eq!(settings.text(Shortcut::DesktopOperation), "Alt+Space");
}

#[cfg_attr(test, test)]
fn tiling_and_individual_floating_have_distinct_non_repeating_defaults() {
    let mut world = World::new();
    world.init_resource::<Settings>();
    world.resource::<Settings>().validate().unwrap();
    let bindings = desktop_bindings(&world);
    for (action, shift) in [
        (crate::host::DesktopAction::ToggleTiling, false),
        (crate::host::DesktopAction::ToggleFloating, true),
    ] {
        let binding = bindings
            .iter()
            .find(|binding| binding.action == action)
            .unwrap();
        assert_eq!(binding.key, KeyCode::KeyT);
        assert!(binding.modifiers.super_key);
        assert_eq!(binding.modifiers.shift, shift);
        assert!(!binding.repeat);
    }
    let width = bindings
        .iter()
        .find(|binding| binding.action == crate::host::DesktopAction::CycleItemWidth)
        .unwrap();
    assert_eq!(width.key, KeyCode::KeyR);
    assert!(width.modifiers.super_key);
    assert!(!width.modifiers.shift);
    assert!(!width.repeat);
}

#[test]
fn monitor_focus_defaults_use_yuio_without_an_inspection_collision_and_stay_rebindable() {
    use crate::native::actions::Modifiers;

    let mut settings = Settings::default();
    settings.validate().unwrap();
    assert!(settings.chord(Shortcut::WindowRecord).is_none());
    for (shortcut, key) in [
        (Shortcut::FocusMonitorLeft, KeyCode::KeyY),
        (Shortcut::FocusMonitorDown, KeyCode::KeyU),
        (Shortcut::FocusMonitorUp, KeyCode::KeyI),
        (Shortcut::FocusMonitorRight, KeyCode::KeyO),
    ] {
        let chord = settings.chord(shortcut).unwrap();
        assert_eq!(chord.key, key);
        assert_eq!(
            chord.modifiers,
            Modifiers {
                super_key: true,
                ..Modifiers::NONE
            }
        );
        assert!(settings.set(Shortcut::WindowRecord, &chord.text()).is_err());
    }
    settings.set(Shortcut::FocusMonitorUp, "Mod+F8").unwrap();
    settings.set(Shortcut::WindowRecord, "Mod+I").unwrap();
    settings.validate().unwrap();
    assert_eq!(
        settings.chord(Shortcut::WindowRecord).unwrap().key,
        KeyCode::KeyI
    );
}

#[test]
fn item_workspace_and_monitor_row_defaults_remain_distinct_and_round_trip() {
    use crate::host::{DesktopAction, Direction};
    use crate::native::actions::Modifiers;

    let mut settings = Settings::default();
    settings.validate().unwrap();
    for (index, shortcut) in [
        Shortcut::MoveWorkspace1,
        Shortcut::MoveWorkspace2,
        Shortcut::MoveWorkspace3,
        Shortcut::MoveWorkspace4,
        Shortcut::MoveWorkspace5,
        Shortcut::MoveWorkspace6,
        Shortcut::MoveWorkspace7,
        Shortcut::MoveWorkspace8,
        Shortcut::MoveWorkspace9,
    ]
    .into_iter()
    .enumerate()
    {
        let number = index as u32 + 1;
        assert_eq!(
            shortcut.desktop_action(),
            Some(DesktopAction::MoveToWorkspace(number))
        );
        let chord = settings.chord(shortcut).unwrap();
        assert_eq!(
            chord.key,
            [
                KeyCode::Digit1,
                KeyCode::Digit2,
                KeyCode::Digit3,
                KeyCode::Digit4,
                KeyCode::Digit5,
                KeyCode::Digit6,
                KeyCode::Digit7,
                KeyCode::Digit8,
                KeyCode::Digit9
            ][index]
        );
        assert_eq!(
            chord.modifiers,
            Modifiers {
                super_key: true,
                shift: true,
                ..Modifiers::NONE
            }
        );
        assert!(!shortcut.repeats());
        assert_eq!(Shortcut::from_id(shortcut.id()), Some(shortcut));
        assert_eq!(
            Shortcut::from_operation(&format!("/item/workspace/{number}")),
            Some(shortcut)
        );
    }
    for (item, row, whole, key, direction) in [
        (
            Shortcut::MoveMonitorLeft,
            Shortcut::RowMonitorLeft,
            Shortcut::WorkspaceMonitorLeft,
            KeyCode::KeyY,
            Direction::Left,
        ),
        (
            Shortcut::MoveMonitorDown,
            Shortcut::RowMonitorDown,
            Shortcut::WorkspaceMonitorDown,
            KeyCode::KeyU,
            Direction::Down,
        ),
        (
            Shortcut::MoveMonitorUp,
            Shortcut::RowMonitorUp,
            Shortcut::WorkspaceMonitorUp,
            KeyCode::KeyI,
            Direction::Up,
        ),
        (
            Shortcut::MoveMonitorRight,
            Shortcut::RowMonitorRight,
            Shortcut::WorkspaceMonitorRight,
            KeyCode::KeyO,
            Direction::Right,
        ),
    ] {
        assert_eq!(
            item.desktop_action(),
            Some(DesktopAction::MoveToOutput(direction))
        );
        assert_eq!(
            row.desktop_action(),
            Some(DesktopAction::MoveRowToOutput(direction))
        );
        assert_eq!(
            whole.desktop_action(),
            Some(DesktopAction::MoveWorkspaceToOutput(direction))
        );
        assert!(settings.chord(whole).is_none());
        for (shortcut, shift, control) in [(item, false, true), (row, true, false)] {
            let chord = settings.chord(shortcut).unwrap();
            assert_eq!(chord.key, key);
            assert_eq!(
                chord.modifiers,
                Modifiers {
                    super_key: true,
                    shift,
                    control,
                    ..Modifiers::NONE
                }
            );
            assert!(!shortcut.repeats());
            assert!(settings.set(Shortcut::WindowRecord, &chord.text()).is_err());
        }
    }
    settings.set(Shortcut::MoveWorkspace9, "Mod+F9").unwrap();
    settings
        .set(Shortcut::RowMonitorUp, "Mod+Shift+F10")
        .unwrap();
    let saved = serde_json::to_string(&settings).unwrap();
    let restored: Settings = serde_json::from_str(&saved).unwrap();
    assert_eq!(restored, settings);
    restored.validate().unwrap();
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
fn settings_panel_opens_the_shared_capture_editor() {
    let (mut app, root) = crate::native::edit_mode::tests::fixture();
    crate::native::edit_mode::EditAction::Open.apply(app.world_mut(), root);
    crate::native::edit_mode::EditAction::Shortcuts.apply(app.world_mut(), root);
    app.update();
    let button = app
        .world_mut()
        .query::<(Entity, &Children)>()
        .iter(app.world())
        .find(|(entity, children)| {
            app.world()
                .get::<crate::native::actions::ActionButton>(*entity)
                .is_some()
                && children.iter().any(|child| {
                    app.world()
                        .get::<Text>(child)
                        .is_some_and(|text| text.0.starts_with(Shortcut::DeleteSand.label()))
                })
        })
        .map(|(entity, _)| entity)
        .unwrap();
    app.world_mut()
        .trigger(bevy::ui_widgets::Activate { entity: button });
    app.update();
    assert!(keyboard_owned(app.world()));
    assert!(app.world().get::<KeyboardModalTrigger>(button).is_some());
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
    use crate::host::{DesktopAction, Direction, HostRequest, SurfaceId, WindowInspection};
    let (mut app, root, sand, window) = fixture();
    app.world_mut().entity_mut(root).insert((
        crate::native::canvas::CanvasView::default(),
        ComputedNode {
            size: Vec2::new(1000.0, 700.0),
            inverse_scale_factor: 1.0,
            ..default()
        },
    ));
    let layout = crate::native::layout::LayoutBox::new(Vec2::splat(100.0));
    app.world_mut().entity_mut(sand).insert((
        layout,
        crate::native::layout::desktop::ApplicationWindow {
            surface: SurfaceId(7),
            content_size: [100; 2],
        },
    ));
    crate::native::layout::desktop::resolve(app.world_mut());
    crate::native::layout::desktop::focus_item(app.world_mut(), sand);
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
        (
            Shortcut::WorkspacePrevious,
            DesktopAction::PreviousWorkspace,
        ),
        (Shortcut::WorkspaceNext, DesktopAction::NextWorkspace),
        (
            Shortcut::WorkspaceEarlier,
            DesktopAction::MoveWorkspaceEarlier,
        ),
        (Shortcut::WorkspaceLater, DesktopAction::MoveWorkspaceLater),
        (
            Shortcut::RowMonitorRight,
            DesktopAction::MoveRowToOutput(Direction::Right),
        ),
        (Shortcut::MoveWorkspace1, DesktopAction::MoveToWorkspace(1)),
        (Shortcut::MoveWorkspace2, DesktopAction::MoveToWorkspace(2)),
        (Shortcut::MoveWorkspace3, DesktopAction::MoveToWorkspace(3)),
        (Shortcut::MoveWorkspace4, DesktopAction::MoveToWorkspace(4)),
        (Shortcut::MoveWorkspace5, DesktopAction::MoveToWorkspace(5)),
        (Shortcut::MoveWorkspace6, DesktopAction::MoveToWorkspace(6)),
        (Shortcut::MoveWorkspace7, DesktopAction::MoveToWorkspace(7)),
        (Shortcut::MoveWorkspace8, DesktopAction::MoveToWorkspace(8)),
        (Shortcut::MoveWorkspace9, DesktopAction::MoveToWorkspace(9)),
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
        if action == DesktopAction::Logout {
            assert!(receiver.try_recv().is_err());
            assert!(keyboard_owned(app.world()));
            let confirm = app
                .world_mut()
                .query::<(
                    Entity,
                    &crate::native::actions::ActionButton,
                    &bevy::a11y::AccessibilityNode,
                )>()
                .iter(app.world())
                .find(|(_, _, node)| {
                    node.label() == Some(crate::i18n::Token::ShortcutLogout.text())
                })
                .map(|(entity, _, _)| entity)
                .unwrap();
            app.world_mut()
                .trigger(bevy::ui_widgets::Activate { entity: confirm });
            app.update();
            assert!(!keyboard_owned(app.world()));
        }
        let expected = if action == DesktopAction::InspectWindowRecord {
            HostRequest::Inspect(WindowInspection {
                owner: root.to_bits(),
                workspace: 1,
                position: app
                    .world()
                    .get::<crate::native::canvas::CanvasView>(root)
                    .unwrap()
                    .center
                    .to_array(),
                surface: SurfaceId(7),
                placement: layout.id,
            })
        } else {
            HostRequest::Action(action)
        };
        assert_eq!(receiver.try_recv().unwrap(), expected);
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
    tiling_and_individual_floating_have_distinct_non_repeating_defaults,
    desktop_chords_allow_super_space_and_arrows_without_relaxing_text_shortcuts,
    settings_reject_conflicts_and_reserved_keys_without_partial_changes,
    default_delete_uses_confirmation_and_leaves_text_fields_alone,
    remapped_delete_works_outside_edit_mode_and_ignores_text_focus,
    scoped_actions_override_configured_bindings_and_unbinding_removes_them,
    settings_panel_opens_the_shared_capture_editor,
    movement_bindings_match_modifiers_and_ignore_old_keys,
    remapped_monitor_workspace_and_record_shortcuts_reach_the_host,
}
