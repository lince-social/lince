use super::*;
use crate::native::actions::{Action, ActionsPlugin, KeyBinding, KeyBindings, WindowActionTarget};
use bevy::{
    input::{ButtonState, InputPlugin},
    input_focus::{FocusCause, InputFocus},
};

#[derive(Component, Default)]
struct Count(usize);

struct Increment;

impl Action for Increment {
    fn apply(&self, world: &mut World, target: Entity) {
        world.get_mut::<Count>(target).unwrap().0 += 1;
    }
}

fn fixture(modifiers: Modifiers) -> (App, Entity, Entity) {
    let mut app = App::new();
    crate::native::laboratory::isolate(app.world_mut());
    app.add_plugins((InputPlugin, ActionsPlugin));
    let root = app
        .world_mut()
        .spawn((
            Count::default(),
            KeyBindings(vec![KeyBinding::new(
                KeyCode::KeyS,
                modifiers,
                crate::native::actions![Increment],
            )]),
        ))
        .id();
    let window = app
        .world_mut()
        .spawn((Window::default(), PrimaryWindow, WindowActionTarget(root)))
        .id();
    (app, root, window)
}

fn emit(
    app: &mut App,
    window: Entity,
    key_code: KeyCode,
    logical_key: Key,
    pressed: bool,
    repeat: bool,
) {
    let text = if pressed {
        if let Key::Character(text) = &logical_key {
            Some(text.clone())
        } else {
            None
        }
    } else {
        None
    };
    app.world_mut().write_message(KeyboardInput {
        window,
        key_code,
        logical_key,
        text,
        repeat,
        state: if pressed {
            ButtonState::Pressed
        } else {
            ButtonState::Released
        },
    });
}

fn trigger(app: &mut App, window: Entity) {
    emit(
        app,
        window,
        KeyCode::KeyS,
        Key::Character("s".into()),
        true,
        false,
    );
    emit(
        app,
        window,
        KeyCode::KeyS,
        Key::Character("s".into()),
        false,
        false,
    );
}

#[test]
fn complete_chords_use_event_modifiers_and_each_distinct_press_in_one_update() {
    for presses in [1, 2, 5] {
        let (mut app, root, window) = fixture(Modifiers::CONTROL);
        emit(
            &mut app,
            window,
            KeyCode::ControlLeft,
            Key::Control,
            true,
            false,
        );
        for _ in 0..presses {
            trigger(&mut app, window);
        }
        emit(
            &mut app,
            window,
            KeyCode::ControlLeft,
            Key::Control,
            false,
            false,
        );
        app.update();
        assert_eq!(app.world().get::<Count>(root).unwrap().0, presses);
        assert!(
            !app.world()
                .resource::<ButtonInput<Key>>()
                .pressed(Key::Control)
        );
        assert!(take(app.world_mut()).is_empty());
        app.update();
        assert_eq!(app.world().get::<Count>(root).unwrap().0, presses);
    }
}

#[test]
fn mapped_logical_modifiers_override_physical_names_without_a_park_snapshot() {
    let (mut app, root, window) = fixture(Modifiers::CONTROL);
    emit(
        &mut app,
        window,
        KeyCode::CapsLock,
        Key::Control,
        true,
        false,
    );
    trigger(&mut app, window);
    emit(
        &mut app,
        window,
        KeyCode::ControlLeft,
        Key::Escape,
        true,
        false,
    );
    trigger(&mut app, window);
    emit(
        &mut app,
        window,
        KeyCode::CapsLock,
        Key::Control,
        false,
        false,
    );
    trigger(&mut app, window);
    app.update();
    assert_eq!(app.world().get::<Count>(root).unwrap().0, 2);
    assert!(
        app.world()
            .resource::<ButtonInput<KeyCode>>()
            .pressed(KeyCode::ControlLeft)
    );
    assert!(
        !app.world()
            .resource::<ButtonInput<Key>>()
            .pressed(Key::Control)
    );
}

#[test]
fn releasing_one_physical_contributor_keeps_the_other_modifier_effective() {
    let (mut app, root, window) = fixture(Modifiers::CONTROL);
    emit(
        &mut app,
        window,
        KeyCode::ControlLeft,
        Key::Control,
        true,
        false,
    );
    emit(
        &mut app,
        window,
        KeyCode::ControlRight,
        Key::Control,
        true,
        false,
    );
    app.update();
    emit(
        &mut app,
        window,
        KeyCode::ControlLeft,
        Key::Control,
        false,
        false,
    );
    trigger(&mut app, window);
    app.update();
    assert_eq!(app.world().get::<Count>(root).unwrap().0, 1);
    assert!(
        app.world()
            .resource::<ButtonInput<Key>>()
            .pressed(Key::Control)
    );
    emit(
        &mut app,
        window,
        KeyCode::ControlRight,
        Key::Control,
        false,
        false,
    );
    trigger(&mut app, window);
    app.update();
    assert_eq!(app.world().get::<Count>(root).unwrap().0, 1);
    assert!(
        !app.world()
            .resource::<ButtonInput<Key>>()
            .pressed(Key::Control)
    );
}

#[test]
fn focus_loss_clears_logical_state_and_old_modifier_ownership_before_return() {
    let (mut app, root, window) = fixture(Modifiers::CONTROL);
    emit(
        &mut app,
        window,
        KeyCode::ControlLeft,
        Key::Control,
        true,
        false,
    );
    emit(
        &mut app,
        window,
        KeyCode::KeyZ,
        Key::Character("z".into()),
        true,
        false,
    );
    app.update();
    app.world_mut().write_message(KeyboardFocusLost);
    app.update();
    assert!(
        !app.world()
            .resource::<ButtonInput<Key>>()
            .pressed(Key::Control)
    );
    assert!(
        !app.world()
            .resource::<ButtonInput<Key>>()
            .pressed(Key::Character("z".into()))
    );
    trigger(&mut app, window);
    app.update();
    assert_eq!(app.world().get::<Count>(root).unwrap().0, 0);
}

#[test]
fn ordered_cancellation_discards_queued_keys_without_dropping_the_returning_chord() {
    let (mut app, root, window) = fixture(Modifiers::ALT);
    emit(
        &mut app,
        window,
        KeyCode::ControlLeft,
        Key::Control,
        true,
        false,
    );
    trigger(&mut app, window);
    cancel(app.world_mut());
    assert!(app.world().resource::<Messages<KeyboardInput>>().is_empty());
    emit(&mut app, window, KeyCode::AltLeft, Key::Alt, true, false);
    trigger(&mut app, window);
    app.update();
    assert_eq!(app.world().get::<Count>(root).unwrap().0, 1);
    let keys = app.world().resource::<ButtonInput<KeyCode>>();
    assert!(keys.pressed(KeyCode::AltLeft));
    assert!(!keys.just_released(KeyCode::AltLeft));
    assert!(!keys.pressed(KeyCode::ControlLeft));
    assert!(
        !app.world()
            .resource::<ButtonInput<Key>>()
            .pressed(Key::Control)
    );
    emit(&mut app, window, KeyCode::AltLeft, Key::Alt, false, false);
    app.update();
    assert!(
        !app.world()
            .resource::<ButtonInput<KeyCode>>()
            .pressed(KeyCode::AltLeft)
    );
    assert!(!app.world().resource::<ButtonInput<Key>>().pressed(Key::Alt));
}

#[test]
fn restoring_modifiers_releases_stale_keys_and_preserves_queued_actions_without_a_frame() {
    let (mut app, root, window) = fixture(Modifiers::ALT);
    app.world_mut()
        .get_mut::<KeyBindings>(root)
        .unwrap()
        .0
        .push(KeyBinding::new(
            KeyCode::KeyZ,
            Modifiers {
                super_key: true,
                ..Modifiers::NONE
            },
            crate::native::actions![Increment],
        ));
    emit(
        &mut app,
        window,
        KeyCode::SuperLeft,
        Key::Super,
        true,
        false,
    );
    app.update();
    for pressed in [true, false] {
        emit(
            &mut app,
            window,
            KeyCode::KeyZ,
            Key::Character("z".into()),
            pressed,
            false,
        );
    }
    synchronize_modifiers(app.world_mut(), window, &[(KeyCode::AltLeft, Key::Alt)]);
    trigger(&mut app, window);
    app.update();
    assert_eq!(app.world().get::<Count>(root).unwrap().0, 2);
    let keys = app.world().resource::<ButtonInput<KeyCode>>();
    assert!(!keys.pressed(KeyCode::SuperLeft));
    assert!(keys.pressed(KeyCode::AltLeft));
    assert!(
        !app.world()
            .resource::<ButtonInput<Key>>()
            .pressed(Key::Super)
    );
    synchronize_modifiers(app.world_mut(), window, &[]);
    app.update();
    assert_eq!(
        app.world()
            .resource::<ButtonInput<KeyCode>>()
            .get_pressed()
            .count(),
        0
    );
}

#[test]
fn alt_graph_is_not_ctrl_alt_and_real_ctrl_alt_keeps_its_own_role() {
    let (mut app, root, window) = fixture(Modifiers {
        control: true,
        alt: true,
        ..Modifiers::NONE
    });
    emit(
        &mut app,
        window,
        KeyCode::AltRight,
        Key::AltGraph,
        true,
        false,
    );
    trigger(&mut app, window);
    app.update();
    assert_eq!(app.world().get::<Count>(root).unwrap().0, 0);
    emit(
        &mut app,
        window,
        KeyCode::ControlLeft,
        Key::Control,
        true,
        false,
    );
    emit(&mut app, window, KeyCode::AltLeft, Key::Alt, true, false);
    trigger(&mut app, window);
    app.update();
    assert_eq!(app.world().get::<Count>(root).unwrap().0, 1);
}

#[test]
fn owned_keymap_roles_remain_authoritative_over_logical_symbols() {
    let (mut app, root, window) = fixture(Modifiers {
        super_key: true,
        ..Modifiers::NONE
    });
    app.insert_resource(KeyboardSnapshot {
        labels: std::sync::Arc::new(BTreeMap::from([(
            "KeyA".into(),
            crate::host::input::KeyLabel {
                plain: "a".into(),
                shifted: "A".into(),
                is_modifier: true,
                modifiers: crate::host::input::KeyModifiers {
                    super_key: true,
                    ..default()
                },
            },
        )])),
        ..default()
    });
    let host = HostWindow::new(&mut app, window);
    host.send(
        &mut app,
        winit::event::WindowEvent::ModifiersChanged(
            winit::keyboard::ModifiersState::CONTROL.into(),
        ),
    );
    emit(
        &mut app,
        window,
        KeyCode::KeyA,
        Key::Character("a".into()),
        true,
        false,
    );
    trigger(&mut app, window);
    emit(
        &mut app,
        window,
        KeyCode::KeyA,
        Key::Character("a".into()),
        false,
        false,
    );
    app.update();
    assert_eq!(app.world().get::<Count>(root).unwrap().0, 1);
}

#[test]
fn native_widget_selection_and_replacement_keep_each_events_modifier_context() {
    let (mut app, _, window) = fixture(Modifiers::CONTROL);
    app.init_resource::<bevy::text::FontCx>()
        .init_resource::<bevy::text::LayoutCx>()
        .init_resource::<bevy::clipboard::Clipboard>()
        .init_resource::<UiScale>()
        .add_message::<bevy::window::Ime>()
        .add_message::<bevy::picking::events::Pointer<bevy::picking::events::Release>>()
        .add_plugins(bevy::ui_widgets::EditableTextInputPlugin)
        .add_systems(
            PostUpdate,
            bevy::text::apply_text_edits.in_set(bevy::text::EditableTextSystems),
        );
    let font = Font::from_bytes(
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../institute/assets/fonts/Lato/Lato-Regular.ttf"
        ))
        .to_vec(),
    );
    app.world_mut()
        .resource_mut::<bevy::text::FontCx>()
        .collection
        .register_fonts(font.data, None);
    app.world_mut()
        .resource_mut::<bevy::text::FontCx>()
        .set_sans_serif_family("Lato")
        .unwrap();
    let field = app
        .world_mut()
        .spawn(bevy::text::EditableText::new("draft"))
        .id();
    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(field, FocusCause::Pressed);
    app.update();
    emit(
        &mut app,
        window,
        KeyCode::ControlLeft,
        Key::Control,
        true,
        false,
    );
    emit(
        &mut app,
        window,
        KeyCode::KeyA,
        Key::Character("a".into()),
        true,
        false,
    );
    emit(
        &mut app,
        window,
        KeyCode::KeyA,
        Key::Character("a".into()),
        false,
        false,
    );
    emit(
        &mut app,
        window,
        KeyCode::ControlLeft,
        Key::Control,
        false,
        false,
    );
    emit(
        &mut app,
        window,
        KeyCode::KeyB,
        Key::Character("b".into()),
        true,
        false,
    );
    emit(
        &mut app,
        window,
        KeyCode::KeyB,
        Key::Character("b".into()),
        false,
        false,
    );
    app.update();
    assert_eq!(
        app.world()
            .get::<bevy::text::EditableText>(field)
            .unwrap()
            .value(),
        "b"
    );
    assert!(
        !app.world()
            .resource::<ButtonInput<Key>>()
            .pressed(Key::Control)
    );
}

#[test]
fn desktop_widths_count_batched_rebound_chords_and_ignore_auto_repeat() {
    use crate::{
        host::WorkspaceLayout,
        layout::desktop::{PlacementId, Width},
        native::{
            canvas::{CanvasItem, CanvasView},
            layout::{LayoutBox, desktop},
            shortcuts::{Settings, Shortcut},
            workspace::{WorkspaceMember, Workspaces},
        },
    };
    for (key, presses) in [(KeyCode::KeyR, 1), (KeyCode::F2, 2)] {
        let (mut app, root, window) = fixture(Modifiers::CONTROL);
        let mut spaces = Workspaces::default();
        spaces.entries[0].desktop.set_mode(WorkspaceLayout::Tiled);
        app.world_mut().entity_mut(root).insert((
            spaces,
            CanvasView::default(),
            ComputedNode {
                size: Vec2::new(900.0, 700.0),
                inverse_scale_factor: 1.0,
                ..default()
            },
        ));
        let layout = LayoutBox::new(Vec2::new(300.0, 200.0));
        let id = PlacementId(layout.id);
        let item = app
            .world_mut()
            .spawn((
                layout,
                CanvasItem {
                    position: bevy::math::DVec2::ZERO,
                    size: Vec2::new(300.0, 200.0),
                },
                WorkspaceMember(1),
                ChildOf(root),
            ))
            .id();
        desktop::resolve(app.world_mut());
        desktop::focus_item(app.world_mut(), item);
        let mut settings = Settings::default();
        if key == KeyCode::F2 {
            settings.set(Shortcut::CycleWidth, "Mod+F2").unwrap();
        }
        app.insert_resource(settings);
        emit(
            &mut app,
            window,
            KeyCode::SuperLeft,
            Key::Super,
            true,
            false,
        );
        for _ in 0..presses {
            emit(
                &mut app,
                window,
                key,
                Key::Unidentified(bevy::input::keyboard::NativeKey::Unidentified),
                true,
                false,
            );
            emit(
                &mut app,
                window,
                key,
                Key::Unidentified(bevy::input::keyboard::NativeKey::Unidentified),
                true,
                true,
            );
            emit(
                &mut app,
                window,
                key,
                Key::Unidentified(bevy::input::keyboard::NativeKey::Unidentified),
                false,
                false,
            );
        }
        emit(
            &mut app,
            window,
            KeyCode::SuperLeft,
            Key::Super,
            false,
            false,
        );
        app.update();
        let model = &app.world().get::<Workspaces>(root).unwrap().entries[0].desktop;
        assert_eq!(
            model.item_width(id),
            Some(if presses == 1 {
                Width::Fraction(1, 3)
            } else {
                Width::Fraction(2, 3)
            })
        );
    }
}

struct HostWindow(winit::window::WindowId);

impl HostWindow {
    fn new(app: &mut App, window: Entity) -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(10000);
        let id = NEXT
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            .into();
        app.add_message::<bevy::winit::RawWinitWindowEvent>();
        bevy::winit::WINIT_WINDOWS.with_borrow_mut(|windows| {
            assert!(windows.winit_to_entity.insert(id, window).is_none());
        });
        Self(id)
    }

    fn send(&self, app: &mut App, event: winit::event::WindowEvent) {
        app.world_mut()
            .write_message(bevy::winit::RawWinitWindowEvent {
                window_id: self.0,
                event,
            });
    }
}

impl Drop for HostWindow {
    fn drop(&mut self) {
        bevy::winit::WINIT_WINDOWS.with_borrow_mut(|windows| {
            windows.winit_to_entity.remove(&self.0);
        });
    }
}

#[test]
fn sandbox_host_masks_handle_modifiers_held_before_focus_and_actual_remapped_masks() {
    use winit::{event::WindowEvent, keyboard::ModifiersState};
    let (mut app, root, window) = fixture(Modifiers::CONTROL);
    let host = HostWindow::new(&mut app, window);
    host.send(&mut app, WindowEvent::Focused(true));
    host.send(
        &mut app,
        WindowEvent::ModifiersChanged(ModifiersState::CONTROL.into()),
    );
    trigger(&mut app, window);
    app.update();
    assert_eq!(app.world().get::<Count>(root).unwrap().0, 1);
    assert_eq!(current(app.world()), Some(Modifiers::CONTROL));
    assert!(
        !app.world()
            .resource::<ButtonInput<KeyCode>>()
            .pressed(KeyCode::ControlLeft)
    );
    host.send(
        &mut app,
        WindowEvent::ModifiersChanged(ModifiersState::empty().into()),
    );
    emit(
        &mut app,
        window,
        KeyCode::ControlLeft,
        Key::Escape,
        true,
        false,
    );
    trigger(&mut app, window);
    app.update();
    assert_eq!(app.world().get::<Count>(root).unwrap().0, 1);
    assert_eq!(current(app.world()), Some(Modifiers::NONE));
    assert!(
        app.world()
            .resource::<ButtonInput<KeyCode>>()
            .pressed(KeyCode::ControlLeft)
    );
    assert!(
        !app.world()
            .resource::<ButtonInput<Key>>()
            .pressed(Key::Control)
    );
}

#[test]
fn host_mask_changes_update_native_widget_modifiers_without_a_keyboard_event() {
    use winit::{event::WindowEvent, keyboard::ModifiersState};
    let (mut app, _, window) = fixture(Modifiers::CONTROL);
    let host = HostWindow::new(&mut app, window);
    for mask in [
        ModifiersState::CONTROL,
        ModifiersState::ALT,
        ModifiersState::SHIFT,
        ModifiersState::SUPER,
        ModifiersState::CONTROL
            | ModifiersState::ALT
            | ModifiersState::SHIFT
            | ModifiersState::SUPER,
        ModifiersState::empty(),
    ] {
        host.send(&mut app, WindowEvent::ModifiersChanged(mask.into()));
        app.update();
        let logical = app.world().resource::<ButtonInput<Key>>();
        for (key, expected) in [
            (Key::Control, mask.control_key()),
            (Key::Alt, mask.alt_key()),
            (Key::Shift, mask.shift_key()),
            (Key::Super, mask.super_key()),
        ] {
            assert_eq!(logical.pressed(key), expected);
        }
        assert_eq!(
            app.world()
                .resource::<ButtonInput<KeyCode>>()
                .get_pressed()
                .count(),
            0
        );
    }
}

#[test]
fn host_focus_loss_and_destroyed_windows_cannot_leave_stale_modifiers() {
    use winit::{event::WindowEvent, keyboard::ModifiersState};
    let (mut app, root, window) = fixture(Modifiers::CONTROL);
    let host = HostWindow::new(&mut app, window);
    host.send(
        &mut app,
        WindowEvent::ModifiersChanged(ModifiersState::CONTROL.into()),
    );
    app.update();
    assert_eq!(current(app.world()), Some(Modifiers::CONTROL));
    app.world_mut().get_mut::<Window>(window).unwrap().focused = false;
    host.send(&mut app, WindowEvent::Focused(false));
    app.world_mut().write_message(KeyboardFocusLost);
    app.world_mut().write_message(KeyboardFocusLost);
    app.update();
    assert_eq!(current(app.world()), None);
    app.world_mut().get_mut::<Window>(window).unwrap().focused = true;
    host.send(&mut app, WindowEvent::Focused(true));
    trigger(&mut app, window);
    app.update();
    assert_eq!(app.world().get::<Count>(root).unwrap().0, 0);
    host.send(
        &mut app,
        WindowEvent::ModifiersChanged(ModifiersState::CONTROL.into()),
    );
    trigger(&mut app, window);
    app.update();
    assert_eq!(app.world().get::<Count>(root).unwrap().0, 1);
    app.world_mut().despawn(window);
    app.update();
    assert_eq!(current(app.world()), None);
}

#[test]
fn switching_owned_windows_uses_the_focused_hosts_mask_after_old_window_releases() {
    use winit::{event::WindowEvent, keyboard::ModifiersState};
    let (mut app, root, first) = fixture(Modifiers::CONTROL);
    let first_host = HostWindow::new(&mut app, first);
    let second = app
        .world_mut()
        .spawn((Window::default(), WindowActionTarget(root)))
        .id();
    let second_host = HostWindow::new(&mut app, second);
    first_host.send(
        &mut app,
        WindowEvent::ModifiersChanged(ModifiersState::SHIFT.into()),
    );
    app.update();
    app.world_mut().get_mut::<Window>(first).unwrap().focused = false;
    first_host.send(&mut app, WindowEvent::Focused(false));
    second_host.send(&mut app, WindowEvent::Focused(true));
    second_host.send(
        &mut app,
        WindowEvent::ModifiersChanged(ModifiersState::CONTROL.into()),
    );
    trigger(&mut app, second);
    emit(
        &mut app,
        first,
        KeyCode::ShiftLeft,
        Key::Shift,
        false,
        false,
    );
    app.update();
    assert_eq!(app.world().get::<Count>(root).unwrap().0, 1);
    assert_eq!(current(app.world()), Some(Modifiers::CONTROL));
}
