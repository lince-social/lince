use super::*;
use bevy::{
    input::{ButtonState, InputPlugin, keyboard::Key},
    input_focus::dispatch_focused_input,
    window::PrimaryWindow,
};

#[test]
fn reviewed_rebase_keeps_independent_binding_changes_in_draft_until_normal_save() {
    let (mut app, root, window) = fixture();
    record(&mut app, root, window, Shortcut::Forward, KeyCode::F2);
    let mut external = app.world().resource::<Settings>().clone();
    external.set(Shortcut::Backward, "F3").unwrap();
    app.world_mut().insert_resource(external.clone());
    app.update();
    let save = app.world().resource::<Editor>().modal.as_ref().unwrap().save;
    assert!(app.world().get::<InteractionDisabled>(save).is_some());
    let nodes: Vec<_> = app.world_mut().query_filtered::<Entity, With<Node>>().iter(app.world()).collect();
    for node in nodes { app.world_mut().entity_mut(node).insert(InheritedVisibility::VISIBLE); }
    click(&mut app, "Review draft against file");
    click(&mut app, "Continue with reviewed draft");
    let modal = app.world().resource::<Editor>().modal.as_ref().unwrap();
    assert_eq!(modal.draft.text(Shortcut::Forward), "F2");
    assert_eq!(modal.draft.text(Shortcut::Backward), "F3");
    assert_eq!(app.world().resource::<Settings>(), &external);
    click(&mut app, "Save all changes");
    assert!(app.world().resource::<Editor>().modal.is_none());
    assert_eq!(app.world().resource::<Settings>().text(Shortcut::Forward), "F2");
    assert_eq!(app.world().resource::<Settings>().text(Shortcut::Backward), "F3");
}

#[test]
fn cancelling_binding_review_preserves_the_original_draft_and_can_resume_capture() {
    let (mut app, root, window) = fixture();
    record(&mut app, root, window, Shortcut::Forward, KeyCode::F2);
    let before = app.world().resource::<Editor>().modal.as_ref().unwrap().draft.clone();
    let mut external = app.world().resource::<Settings>().clone();
    external.set(Shortcut::Forward, "F3").unwrap();
    app.world_mut().insert_resource(external.clone());
    app.update();
    let nodes: Vec<_> = app.world_mut().query_filtered::<Entity, With<Node>>().iter(app.world()).collect();
    for node in nodes { app.world_mut().entity_mut(node).insert(InheritedVisibility::VISIBLE); }
    click(&mut app, "Review draft against file");
    click(&mut app, "Use file value");
    click(&mut app, "Cancel review and keep draft");
    assert_eq!(app.world().resource::<Editor>().modal.as_ref().unwrap().draft, before);
    assert_eq!(app.world().resource::<Settings>(), &external);
    record(&mut app, root, window, Shortcut::Backward, KeyCode::F4);
    assert_eq!(app.world().resource::<Editor>().modal.as_ref().unwrap().draft.text(Shortcut::Backward), "F4");
}

#[derive(Resource, Default)]
struct Delivered(Vec<KeyCode>);

#[test]
fn an_external_binding_change_during_a_held_capture_preserves_its_release_and_local_candidate() {
    let (mut app, root, window) = fixture();
    begin(&mut app, root, Shortcut::Forward);
    key(&mut app, window, KeyCode::F2, true, false);
    let mut external = app.world().resource::<Settings>().clone();
    external.set(Shortcut::Backward, "F3").unwrap();
    app.world_mut().insert_resource(external.clone());
    app.update();
    key(&mut app, window, KeyCode::F2, true, true);
    key(&mut app, window, KeyCode::F2, false, false);
    click(&mut app, "Validate combination");
    assert_eq!(app.world().resource::<Editor>().modal.as_ref().unwrap().draft.text(Shortcut::Forward), "F2");
    assert_eq!(app.world().resource::<Settings>(), &external);
    assert!(app.world().resource::<Delivered>().0.is_empty());
    let nodes: Vec<_> = app.world_mut().query_filtered::<Entity, With<Node>>().iter(app.world()).collect();
    for node in nodes { app.world_mut().entity_mut(node).insert(InheritedVisibility::VISIBLE); }
    click(&mut app, "Review draft against file");
    click(&mut app, "Continue with reviewed draft");
    click(&mut app, "Save all changes");
    assert_eq!(app.world().resource::<Settings>().text(Shortcut::Forward), "F2");
    assert_eq!(app.world().resource::<Settings>().text(Shortcut::Backward), "F3");
    assert!(!app.world().resource::<Editor>().held.contains(&KeyCode::F2));
}

fn fixture() -> (App, Entity, Entity) {
    let mut app = App::new();
    crate::native::laboratory::isolate(app.world_mut());
    app.add_plugins((
        InputPlugin,
        crate::native::actions::ActionsPlugin,
        EditorPlugin,
    ))
    .init_resource::<Settings>()
    .init_resource::<Assets<Font>>()
    .init_resource::<crate::native::theme::Typography>()
    .init_resource::<Delivered>()
    .add_systems(
        PreUpdate,
        dispatch_focused_input::<KeyboardInput>
            .in_set(InputFocusSystems::Dispatch)
            .after(InputSystems),
    )
    .add_observer(
        |event: On<bevy::input_focus::FocusedInput<KeyboardInput>>,
         mut delivered: ResMut<Delivered>| {
            if event.focused_entity == event.original_event_target()
                && event.input.state.is_pressed()
            {
                delivered.0.push(event.input.key_code);
            }
        },
    );
    let root = app
        .world_mut()
        .spawn(crate::native::workspace::Workspaces::default())
        .id();
    let window = app
        .world_mut()
        .spawn((
            Window::default(),
            PrimaryWindow,
            crate::native::actions::WindowActionTarget(root),
        ))
        .id();
    super::super::install(app.world_mut(), root);
    panel(app.world_mut(), root);
    app.update();
    (app, root, window)
}

fn key(app: &mut App, window: Entity, code: KeyCode, pressed: bool, repeat: bool) {
    queue_key(app, window, code, pressed, repeat);
    app.update();
}

fn queue_key(app: &mut App, window: Entity, code: KeyCode, pressed: bool, repeat: bool) {
    app.world_mut().write_message(KeyboardInput {
        key_code: code,
        logical_key: Key::Unidentified(bevy::input::keyboard::NativeKey::Unidentified),
        state: if pressed {
            ButtonState::Pressed
        } else {
            ButtonState::Released
        },
        text: None,
        repeat,
        window,
    });
}

fn mouse(app: &mut App, window: Entity, button: MouseButton, pressed: bool) {
    queue_mouse(app, window, button, pressed);
    app.update();
}

fn queue_mouse(app: &mut App, window: Entity, button: MouseButton, pressed: bool) {
    if !app
        .world()
        .contains_resource::<Messages<bevy::window::WindowEvent>>()
    {
        app.add_message::<bevy::window::WindowEvent>();
    }
    let event = bevy::input::mouse::MouseButtonInput {
        window,
        button,
        state: if pressed {
            ButtonState::Pressed
        } else {
            ButtonState::Released
        },
    };
    app.world_mut()
        .write_message(bevy::window::WindowEvent::MouseButtonInput(event.clone()));
    app.world_mut().write_message(event);
}

#[test]
fn capturing_a_mouse_press_preserves_other_held_button_releases_once() {
    use bevy::input::mouse::MouseButtonInput;
    let (mut app, root, window) = fixture();
    mouse(&mut app, window, MouseButton::Right, true);
    mouse(&mut app, window, MouseButton::Middle, true);
    begin(&mut app, root, Shortcut::PointerMove);
    key(&mut app, window, KeyCode::ControlLeft, true, false);
    let mut cursor = MessageCursor::<MouseButtonInput>::default();
    cursor.clear(app.world().resource::<Messages<MouseButtonInput>>());
    queue_mouse(&mut app, window, MouseButton::Left, true);
    queue_mouse(&mut app, window, MouseButton::Right, false);
    queue_mouse(&mut app, window, MouseButton::Middle, false);
    app.update();
    let events: Vec<_> = cursor
        .read(app.world().resource::<Messages<MouseButtonInput>>())
        .map(|event| (event.button, event.state))
        .collect();
    assert_eq!(
        events,
        [
            (MouseButton::Right, ButtonState::Released),
            (MouseButton::Middle, ButtonState::Released)
        ]
    );
    assert_eq!(
        app.world()
            .resource::<Editor>()
            .modal
            .as_ref()
            .unwrap()
            .capture
            .as_ref()
            .unwrap()
            .value
            .as_deref(),
        Some("Ctrl+MouseLeft")
    );
    app.update();
    assert_eq!(
        cursor
            .read(app.world().resource::<Messages<MouseButtonInput>>())
            .count(),
        0
    );
    mouse(&mut app, window, MouseButton::Left, false);
    assert_eq!(
        cursor
            .read(app.world().resource::<Messages<MouseButtonInput>>())
            .count(),
        0
    );
}

#[test]
fn pointer_capture_is_typed_conflicts_in_the_shared_draft_and_is_swallowed_through_release() {
    let (mut app, root, window) = fixture();
    begin(&mut app, root, Shortcut::PointerMove);
    key(&mut app, window, KeyCode::SuperLeft, true, false);
    mouse(&mut app, window, MouseButton::Right, true);
    let modal = app.world().resource::<Editor>().modal.as_ref().unwrap();
    assert_eq!(
        modal.capture.as_ref().unwrap().value.as_deref(),
        Some("Mod+MouseRight")
    );
    assert_eq!(modal.draft.text(Shortcut::PointerMove), "Mod+MouseLeft");
    assert!(
        !app.world()
            .resource::<ButtonInput<MouseButton>>()
            .pressed(MouseButton::Right)
    );
    key(&mut app, window, KeyCode::SuperLeft, false, false);
    click(&mut app, "Validate combination");
    let modal = app.world().resource::<Editor>().modal.as_ref().unwrap();
    assert_eq!(
        modal.draft.conflicts()[0].1,
        [Shortcut::PointerMove, Shortcut::PointerResize]
    );
    assert!(app.world().get::<InteractionDisabled>(modal.save).is_some());
    mouse(&mut app, window, MouseButton::Right, false);
    assert!(
        !app.world()
            .resource::<ButtonInput<MouseButton>>()
            .just_released(MouseButton::Right)
    );
    begin(&mut app, root, Shortcut::PointerResize);
    key(&mut app, window, KeyCode::ControlLeft, true, false);
    mouse(&mut app, window, MouseButton::Middle, true);
    mouse(&mut app, window, MouseButton::Middle, false);
    key(&mut app, window, KeyCode::ControlLeft, false, false);
    click(&mut app, "Validate combination");
    assert!(
        app.world()
            .resource::<Editor>()
            .modal
            .as_ref()
            .unwrap()
            .draft
            .conflicts()
            .is_empty()
    );
    click(&mut app, "Save all changes");
    let settings = app.world().resource::<Settings>();
    assert_eq!(settings.text(Shortcut::PointerMove), "Mod+MouseRight");
    assert_eq!(settings.text(Shortcut::PointerResize), "Ctrl+MouseMiddle");
    assert!(app.world().resource::<Delivered>().0.is_empty());
}

#[test]
fn cancelling_pointer_capture_keeps_the_original_draft_and_plain_buttons_available() {
    let (mut app, root, window) = fixture();
    begin(&mut app, root, Shortcut::PointerMove);
    key(&mut app, window, KeyCode::KeyA, true, false);
    assert!(
        app.world()
            .resource::<Editor>()
            .modal
            .as_ref()
            .unwrap()
            .capture
            .as_ref()
            .unwrap()
            .value
            .is_none()
    );
    mouse(&mut app, window, MouseButton::Left, true);
    assert!(
        app.world()
            .resource::<ButtonInput<MouseButton>>()
            .pressed(MouseButton::Left)
    );
    mouse(&mut app, window, MouseButton::Left, false);
    click(&mut app, "Cancel capture");
    assert_eq!(
        app.world()
            .resource::<Editor>()
            .modal
            .as_ref()
            .unwrap()
            .draft
            .text(Shortcut::PointerMove),
        "Mod+MouseLeft"
    );
}

#[test]
fn mouse_capture_removes_only_fresh_owned_buttons_without_replaying_previous_clicks() {
    use bevy::picking::pointer::{Location, PointerAction, PointerButton, PointerId, PointerInput};
    let (mut app, root, window) = fixture();
    app.add_message::<PointerInput>();
    let location = Location {
        target: bevy::camera::RenderTarget::Window(bevy::window::WindowRef::Entity(window))
            .normalize(Some(window))
            .unwrap(),
        position: Vec2::new(20.0, 30.0),
    };
    let mut cursor = MessageCursor::<PointerInput>::default();
    app.world_mut().write_message(PointerInput::new(
        PointerId::Mouse,
        location.clone(),
        PointerAction::Press(PointerButton::Primary),
    ));
    app.update();
    assert_eq!(
        cursor
            .read(app.world().resource::<Messages<PointerInput>>())
            .count(),
        1
    );
    begin(&mut app, root, Shortcut::PointerMove);
    key(&mut app, window, KeyCode::ControlLeft, true, false);
    app.world_mut().write_message(PointerInput::new(
        PointerId::Mouse,
        location.clone(),
        PointerAction::Press(PointerButton::Secondary),
    ));
    app.world_mut().write_message(PointerInput::new(
        PointerId::Mouse,
        location,
        PointerAction::Move { delta: Vec2::ONE },
    ));
    mouse(&mut app, window, MouseButton::Right, true);
    let events: Vec<_> = cursor
        .read(app.world().resource::<Messages<PointerInput>>())
        .collect();
    assert_eq!(events.len(), 1);
    assert!(matches!(events[0].action, PointerAction::Move { .. }));
    mouse(&mut app, window, MouseButton::Right, false);
    assert_eq!(
        cursor
            .read(app.world().resource::<Messages<PointerInput>>())
            .count(),
        0
    );
}

fn click(app: &mut App, label: &str) {
    let entity = app
        .world_mut()
        .query::<(Entity, &ActionButton, &Children)>()
        .iter(app.world())
        .find(|(_, _, children)| {
            children.iter().any(|child| {
                app.world()
                    .get::<Text>(child)
                    .is_some_and(|text| text.0 == label)
            })
        })
        .map(|(entity, _, _)| entity)
        .unwrap();
    app.world_mut()
        .trigger(bevy::ui_widgets::Activate { entity });
    app.update();
}

fn binding(app: &mut App, shortcut: Shortcut) -> Entity {
    app.world_mut()
        .query::<&Row>()
        .iter(app.world())
        .find(|row| row.shortcut == shortcut)
        .unwrap()
        .binding
}

fn begin(app: &mut App, root: Entity, shortcut: Shortcut) {
    if !keyboard_owned(app.world()) {
        Change::Open(None).apply(app.world_mut(), root);
    }
    let button = binding(app, shortcut);
    app.world_mut()
        .trigger(bevy::ui_widgets::Activate { entity: button });
    app.update();
}

fn record(app: &mut App, root: Entity, window: Entity, shortcut: Shortcut, key_code: KeyCode) {
    begin(app, root, shortcut);
    key(app, window, key_code, true, false);
    key(app, window, key_code, false, false);
    click(app, "Validate combination");
}

#[test]
fn capture_isolated_from_commands_and_supports_a_batch_swap() {
    let (mut app, root, window) = fixture();
    record(&mut app, root, window, Shortcut::DeleteSand, KeyCode::KeyW);
    let modal = app.world().resource::<Editor>().modal.as_ref().unwrap();
    assert_eq!(modal.draft.text(Shortcut::DeleteSand), "W");
    assert_eq!(
        modal.draft.conflicts()[0].1,
        [Shortcut::DeleteSand, Shortcut::Forward]
    );
    assert!(app.world().get::<InteractionDisabled>(modal.save).is_some());
    assert_eq!(
        app.world()
            .resource::<Settings>()
            .text(Shortcut::DeleteSand),
        "Delete"
    );
    record(&mut app, root, window, Shortcut::Forward, KeyCode::Delete);
    assert!(
        app.world()
            .resource::<Editor>()
            .modal
            .as_ref()
            .unwrap()
            .draft
            .conflicts()
            .is_empty()
    );
    click(&mut app, "Save all changes");
    assert!(!keyboard_owned(app.world()));
    assert_eq!(
        app.world()
            .resource::<Settings>()
            .text(Shortcut::DeleteSand),
        "W"
    );
    assert_eq!(
        app.world().resource::<Settings>().text(Shortcut::Forward),
        "Delete"
    );
    assert!(app.world().resource::<Delivered>().0.is_empty());
    let configured = app
        .world()
        .get::<super::super::ConfiguredBindings>(root)
        .unwrap();
    assert_eq!(configured.0[0].key, KeyCode::KeyW);
}

#[test]
fn capture_retains_a_complete_chords_modifiers_when_every_key_is_released_in_one_update() {
    let (mut app, root, window) = fixture();
    begin(&mut app, root, Shortcut::CycleWidth);
    for (code, logical, pressed, repeat) in [
        (KeyCode::ControlLeft, Key::Control, true, false),
        (KeyCode::ShiftLeft, Key::Shift, true, false),
        (KeyCode::F2, Key::F2, true, false),
        (KeyCode::F2, Key::F2, true, true),
        (KeyCode::F2, Key::F2, false, false),
        (KeyCode::ShiftLeft, Key::Shift, false, false),
        (KeyCode::ControlLeft, Key::Control, false, false),
    ] {
        app.world_mut().write_message(KeyboardInput {
            window,
            key_code: code,
            logical_key: logical,
            state: if pressed {
                ButtonState::Pressed
            } else {
                ButtonState::Released
            },
            text: None,
            repeat,
        });
    }
    app.update();
    let modal = app.world().resource::<Editor>().modal.as_ref().unwrap();
    assert_eq!(
        modal.capture.as_ref().unwrap().value.as_deref(),
        Some("Ctrl+Shift+F2")
    );
    assert_eq!(
        app.world()
            .resource::<Settings>()
            .text(Shortcut::CycleWidth),
        "Mod+R"
    );
    assert!(app.world().resource::<Delivered>().0.is_empty());
    click(&mut app, "Validate combination");
    assert_eq!(
        app.world()
            .resource::<Editor>()
            .modal
            .as_ref()
            .unwrap()
            .draft
            .text(Shortcut::CycleWidth),
        "Ctrl+Shift+F2"
    );
    assert!(app.world().resource::<Delivered>().0.is_empty());
}

#[test]
fn closing_the_editor_retires_quarantine_before_a_later_fresh_key_in_the_same_batch() {
    for close in ["Save all changes", "Cancel draft"] {
        let (mut app, root, window) = fixture();
        begin(&mut app, root, Shortcut::DeleteSand);
        key(&mut app, window, KeyCode::F2, true, false);
        click(&mut app, "Validate combination");
        click(&mut app, close);
        assert!(!keyboard_owned(app.world()));
        app.world_mut().resource_mut::<Delivered>().0.clear();
        queue_key(&mut app, window, KeyCode::F2, false, false);
        queue_key(&mut app, window, KeyCode::KeyB, true, false);
        app.update();
        assert_eq!(app.world().resource::<Delivered>().0, [KeyCode::KeyB]);
        let editor = app.world().resource::<Editor>();
        assert!(editor.swallowed.is_empty());
        assert_eq!(editor.held, HashSet::from([KeyCode::KeyB]));
        assert!(
            app.world()
                .resource::<ButtonInput<KeyCode>>()
                .pressed(KeyCode::KeyB)
        );
        queue_key(&mut app, window, KeyCode::KeyB, false, false);
        app.update();
        assert_eq!(app.world().resource::<Delivered>().0, [KeyCode::KeyB]);
        assert!(app.world().resource::<Editor>().held.is_empty());
    }
}

#[test]
fn enter_validates_the_current_candidate_before_later_keys_in_the_same_batch() {
    let (mut app, root, window) = fixture();
    begin(&mut app, root, Shortcut::DeleteSand);
    for (code, pressed) in [
        (KeyCode::F2, true),
        (KeyCode::F2, false),
        (KeyCode::Enter, true),
        (KeyCode::Enter, false),
        (KeyCode::KeyB, true),
        (KeyCode::KeyB, false),
    ] {
        queue_key(&mut app, window, code, pressed, false);
    }
    app.update();
    let modal = app.world().resource::<Editor>().modal.as_ref().unwrap();
    assert!(modal.capture.is_none());
    assert_eq!(modal.draft.text(Shortcut::DeleteSand), "F2");
    assert_eq!(
        app.world()
            .resource::<Settings>()
            .text(Shortcut::DeleteSand),
        "Delete"
    );
    assert_eq!(app.world().resource::<Delivered>().0, [KeyCode::KeyB]);
}

#[test]
fn escape_closes_the_modal_before_fresh_input_without_leaking_its_discarded_prefix() {
    let (mut app, root, window) = fixture();
    Change::Open(None).apply(app.world_mut(), root);
    for (code, pressed) in [
        (KeyCode::KeyA, true),
        (KeyCode::KeyA, false),
        (KeyCode::Escape, true),
        (KeyCode::Escape, false),
        (KeyCode::KeyB, true),
        (KeyCode::KeyB, false),
    ] {
        queue_key(&mut app, window, code, pressed, false);
    }
    app.update();
    assert!(!keyboard_owned(app.world()));
    assert!(app.world().resource::<Editor>().modal.is_none());
    assert_eq!(app.world().resource::<Delivered>().0, [KeyCode::KeyB]);
    app.update();
    assert_eq!(app.world().resource::<Delivered>().0, [KeyCode::KeyB]);
}

#[test]
fn every_conflicting_command_is_visible_and_can_be_unbound_without_leaving_the_modal() {
    let (mut app, root, _) = fixture();
    Change::Open(None).apply(app.world_mut(), root);
    {
        let mut editor = app.world_mut().resource_mut::<Editor>();
        let modal = editor.modal.as_mut().unwrap();
        modal
            .draft
            .stage(Shortcut::FocusDown, Some("Mod+H"))
            .unwrap();
        modal
            .draft
            .stage(Shortcut::FocusUp, Some("Super+H"))
            .unwrap();
        modal.conflicts_only = true;
        modal.dirty = true;
    }
    app.update();
    let modal = app.world().resource::<Editor>().modal.as_ref().unwrap();
    assert_eq!(modal.draft.conflicts()[0].1.len(), 3);
    let overlay = modal.overlay;
    let rows: Vec<_> = app
        .world_mut()
        .query::<(Entity, &Row)>()
        .iter(app.world())
        .filter(|(_, row)| {
            [Shortcut::FocusLeft, Shortcut::FocusDown, Shortcut::FocusUp].contains(&row.shortcut)
        })
        .map(|(entity, row)| (entity, row.status))
        .collect();
    for (entity, status) in rows {
        assert_eq!(
            app.world().get::<Node>(entity).unwrap().display,
            Display::Flex
        );
        let text = &app.world().get::<Text>(status).unwrap().0;
        assert!(text.contains("Conflicts with:"));
        assert_eq!(text.matches(", ").count(), 1);
    }
    Change::Unbind(Shortcut::FocusDown).apply(app.world_mut(), root);
    Change::Unbind(Shortcut::FocusUp).apply(app.world_mut(), root);
    let modal = app.world().resource::<Editor>().modal.as_ref().unwrap();
    assert_eq!(modal.overlay, overlay);
    assert!(modal.draft.conflicts().is_empty());
    assert!(app.world().get::<InteractionDisabled>(modal.save).is_none());
    click(&mut app, "Cancel draft");
    assert_eq!(app.world().resource::<Settings>(), &Settings::default());
}

#[test]
fn capture_ignores_repeat_and_modifiers_and_quarantines_keys_until_release() {
    let (mut app, root, window) = fixture();
    begin(&mut app, root, Shortcut::FocusLeft);
    key(&mut app, window, KeyCode::SuperLeft, true, false);
    assert!(
        app.world()
            .resource::<Editor>()
            .modal
            .as_ref()
            .unwrap()
            .capture
            .as_ref()
            .unwrap()
            .value
            .is_none()
    );
    key(&mut app, window, KeyCode::KeyP, true, true);
    assert!(
        app.world()
            .resource::<Editor>()
            .modal
            .as_ref()
            .unwrap()
            .capture
            .as_ref()
            .unwrap()
            .value
            .is_none()
    );
    key(&mut app, window, KeyCode::KeyP, true, false);
    assert_eq!(
        app.world()
            .resource::<Editor>()
            .modal
            .as_ref()
            .unwrap()
            .capture
            .as_ref()
            .unwrap()
            .value
            .as_deref(),
        Some("Mod+P")
    );
    click(&mut app, "Cancel capture");
    click(&mut app, "Cancel draft");
    key(&mut app, window, KeyCode::KeyP, true, true);
    key(&mut app, window, KeyCode::KeyP, false, false);
    key(&mut app, window, KeyCode::SuperLeft, false, false);
    assert!(app.world().resource::<Delivered>().0.is_empty());
    key(&mut app, window, KeyCode::KeyA, true, false);
    app.update();
    assert_eq!(app.world().resource::<Delivered>().0, [KeyCode::KeyA]);
}

#[test]
fn escape_cancels_capture_but_modified_escape_can_be_recorded() {
    let (mut app, root, window) = fixture();
    begin(&mut app, root, Shortcut::FocusLeft);
    let source = binding(&mut app, Shortcut::FocusLeft);
    key(&mut app, window, KeyCode::Escape, true, false);
    assert!(
        app.world()
            .resource::<Editor>()
            .modal
            .as_ref()
            .unwrap()
            .capture
            .is_none()
    );
    assert_eq!(app.world().resource::<InputFocus>().get(), Some(source));
    key(&mut app, window, KeyCode::Escape, false, false);
    begin(&mut app, root, Shortcut::FocusLeft);
    key(&mut app, window, KeyCode::SuperRight, true, false);
    key(&mut app, window, KeyCode::Escape, true, false);
    assert_eq!(
        app.world()
            .resource::<Editor>()
            .modal
            .as_ref()
            .unwrap()
            .capture
            .as_ref()
            .unwrap()
            .value
            .as_deref(),
        Some("Mod+Escape")
    );
    assert!(app.world().resource::<Delivered>().0.is_empty());
}

#[test]
fn invalid_keys_focus_loss_and_reset_do_not_publish_partial_edits() {
    let (mut app, root, window) = fixture();
    begin(&mut app, root, Shortcut::DeleteSand);
    key(&mut app, window, KeyCode::Tab, true, false);
    let modal = app.world().resource::<Editor>().modal.as_ref().unwrap();
    assert!(modal.capture.as_ref().unwrap().value.is_none());
    assert!(
        app.world()
            .get::<Text>(modal.status)
            .unwrap()
            .0
            .contains("reserved")
    );
    key(&mut app, window, KeyCode::IntlRo, true, false);
    assert!(
        app.world()
            .resource::<Editor>()
            .modal
            .as_ref()
            .unwrap()
            .capture
            .as_ref()
            .unwrap()
            .value
            .is_none()
    );
    app.world_mut().write_message(KeyboardFocusLost);
    app.update();
    assert!(
        app.world()
            .resource::<Editor>()
            .modal
            .as_ref()
            .unwrap()
            .capture
            .is_none()
    );
    Change::Unbind(Shortcut::Forward).apply(app.world_mut(), root);
    Change::Reset(Shortcut::Forward).apply(app.world_mut(), root);
    assert_eq!(app.world().resource::<Settings>(), &Settings::default());
    Change::Unbind(Shortcut::Forward).apply(app.world_mut(), root);
    Change::ResetAll.apply(app.world_mut(), root);
    assert_eq!(
        app.world()
            .resource::<Editor>()
            .modal
            .as_ref()
            .unwrap()
            .draft,
        Settings::default()
    );
    assert_eq!(app.world().resource::<Settings>(), &Settings::default());
}

#[test]
fn focus_loss_after_validation_preserves_review_feedback_and_the_unsaved_binding() {
    let (mut app, root, window) = fixture();
    record(&mut app, root, window, Shortcut::DeleteSand, KeyCode::KeyW);
    let modal = app.world().resource::<Editor>().modal.as_ref().unwrap();
    assert!(modal.capture.is_none());
    let draft = modal.draft.clone();
    let message = modal.message.clone();
    assert!(message.contains("Combination added"));
    app.world_mut().write_message(KeyboardFocusLost);
    app.update();
    let modal = app.world().resource::<Editor>().modal.as_ref().unwrap();
    assert_eq!(modal.draft, draft);
    assert_eq!(modal.message, message);
    assert_eq!(app.world().resource::<Settings>(), &Settings::default());
    assert_eq!(modal.draft.conflicts().len(), 1);
    Change::Unbind(Shortcut::Forward).apply(app.world_mut(), root);
    app.update();
    assert!(
        app.world()
            .resource::<Editor>()
            .modal
            .as_ref()
            .unwrap()
            .draft
            .conflicts()
            .is_empty()
    );
    click(&mut app, "Save all changes");
    assert_eq!(
        app.world()
            .resource::<Settings>()
            .text(Shortcut::DeleteSand),
        "W"
    );
}

#[test]
fn dedicated_hardware_capture_records_the_binding_without_delivering_or_executing_it() {
    use crate::host::external::Action;
    use bevy::input::keyboard::NativeKeyCode;
    for (action, code) in [
        (
            Action::BrightnessUp,
            KeyCode::Unidentified(NativeKeyCode::Xkb(225)),
        ),
        (Action::VolumeUp, KeyCode::AudioVolumeUp),
        (
            Action::MicrophoneMute,
            KeyCode::Unidentified(NativeKeyCode::Xkb(248)),
        ),
        (Action::ScreenshotScreen, KeyCode::PrintScreen),
    ] {
        let (mut app, root, window) = fixture();
        let shortcut = Shortcut::External(action);
        record(&mut app, root, window, shortcut, code);
        assert_eq!(
            app.world()
                .resource::<Editor>()
                .modal
                .as_ref()
                .unwrap()
                .draft
                .text(shortcut),
            action.binding()
        );
        assert!(app.world().resource::<Delivered>().0.is_empty());
    }
}

#[test]
fn enter_validates_a_captured_chord_and_keeps_the_original_row_focused() {
    let (mut app, root, window) = fixture();
    begin(&mut app, root, Shortcut::DeleteSand);
    let source = binding(&mut app, Shortcut::DeleteSand);
    key(&mut app, window, KeyCode::F2, true, false);
    key(&mut app, window, KeyCode::F2, false, false);
    key(&mut app, window, KeyCode::Enter, true, false);
    let modal = app.world().resource::<Editor>().modal.as_ref().unwrap();
    assert!(modal.capture.is_none());
    assert_eq!(modal.draft.text(Shortcut::DeleteSand), "F2");
    assert_eq!(app.world().resource::<InputFocus>().get(), Some(source));
    key(&mut app, window, KeyCode::Enter, false, false);
    assert!(app.world().resource::<Delivered>().0.is_empty());
}

#[test]
fn the_batch_editor_defers_root_commands_while_searching_and_restores_them_on_close() {
    #[derive(Resource, Default)]
    struct Count(usize);
    struct Increment;
    impl Action for Increment {
        fn apply(&self, world: &mut World, _: Entity) {
            world.resource_mut::<Count>().0 += 1;
        }
    }
    let (mut app, root, window) = fixture();
    app.init_resource::<Count>();
    app.world_mut()
        .entity_mut(root)
        .insert(super::super::ConfiguredBindings(vec![KeyBinding::new(
            KeyCode::F8,
            Modifiers::NONE,
            crate::native::actions![Increment],
        )]));
    Change::Open(None).apply(app.world_mut(), root);
    key(&mut app, window, KeyCode::F8, true, false);
    assert_eq!(app.world().resource::<Count>().0, 0);
    key(&mut app, window, KeyCode::F8, false, false);
    click(&mut app, "Cancel draft");
    key(&mut app, window, KeyCode::F8, true, false);
    assert_eq!(app.world().resource::<Count>().0, 1);
}

#[test]
fn changing_mod_is_reviewed_in_the_draft_and_updates_the_shared_help_after_save() {
    let (mut app, root, _) = fixture();
    Change::Open(None).apply(app.world_mut(), root);
    Change::Modifier.apply(app.world_mut(), root);
    assert_eq!(app.world().resource::<Settings>().modifier(), ModKey::Super);
    let modal = app.world().resource::<Editor>().modal.as_ref().unwrap();
    assert_eq!(modal.draft.modifier(), ModKey::Alt);
    assert!(
        !app.world()
            .get::<Text>(modal.status)
            .unwrap()
            .0
            .contains("0 changed commands")
    );
    click(&mut app, "Save all changes");
    assert_eq!(app.world().resource::<Settings>().modifier(), ModKey::Alt);
    let text = app
        .world_mut()
        .query_filtered::<&Text, With<ModifierSummary>>()
        .iter(app.world())
        .next()
        .unwrap();
    assert!(text.0.contains("Mod is Alt"));
    assert!(
        super::super::desktop_bindings(app.world())
            .into_iter()
            .filter(|binding| binding.action
                == crate::host::DesktopAction::Focus(crate::host::Direction::Left))
            .any(|binding| binding.modifiers.alt && !binding.modifiers.super_key)
    );
}

#[test]
fn command_help_updates_availability_and_bindings_without_replacing_focused_controls() {
    let (mut app, root, _) = fixture();
    let (button, label) = app
        .world_mut()
        .query::<(Entity, &Summary)>()
        .iter(app.world())
        .find(|(_, summary)| summary.shortcut == Shortcut::Terminal)
        .map(|(entity, summary)| (entity, summary.label))
        .unwrap();
    assert!(
        app.world()
            .get::<crate::native::icons::Tooltip>(button)
            .unwrap()
            .0
            .contains("requires a Park session")
    );
    let input = app
        .world_mut()
        .spawn((
            crate::native::sand::editable("Retained draft"),
            ChildOf(root),
        ))
        .id();
    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(input, FocusCause::Pressed);
    let (sender, _receiver) = std::sync::mpsc::sync_channel(2);
    app.world_mut()
        .insert_resource(crate::native::layout::desktop::HostRequests(sender));
    app.world_mut()
        .resource_mut::<Settings>()
        .set(Shortcut::Terminal, "Mod+F2")
        .unwrap();
    app.update();
    assert_eq!(app.world().get::<Summary>(button).unwrap().label, label);
    assert!(app.world().get::<Text>(label).unwrap().0.contains("Mod+F2"));
    let help = &app
        .world()
        .get::<crate::native::icons::Tooltip>(button)
        .unwrap()
        .0;
    assert!(help.contains("Mod+F2") && help.contains("Park session"));
    assert!(!help.contains("requires a Park session"));
    assert_eq!(app.world().resource::<InputFocus>().get(), Some(input));
    assert_eq!(
        app.world().get::<EditableText>(input).unwrap().value(),
        "Retained draft"
    );
}

#[test]
fn failed_save_and_search_preserve_the_draft_entities_focus_and_scroll() {
    let (mut app, root, window) = fixture();
    let directory = tempfile::tempdir().unwrap();
    app.insert_resource(crate::native::workspace::WorkspaceFile::new(
        directory.path().join("interface.json"),
    ));
    configuration::load(app.world_mut());
    record(&mut app, root, window, Shortcut::DeleteSand, KeyCode::F2);
    let modal = app.world().resource::<Editor>().modal.as_ref().unwrap();
    let (overlay, filter) = (modal.overlay, modal.filter);
    let binding = binding(&mut app, Shortcut::DeleteSand);
    let entry = app.world().get::<ChildOf>(binding).unwrap().parent();
    let row = app.world().get::<ChildOf>(entry).unwrap().parent();
    let list = app.world().get::<ChildOf>(row).unwrap().parent();
    app.world_mut().get_mut::<ScrollPosition>(list).unwrap().0.y = 350.0;
    app.world_mut()
        .get_mut::<EditableText>(filter)
        .unwrap()
        .editor
        .set_text("F2");
    restore_focus(app.world_mut(), Some(filter));
    app.update();
    assert_eq!(app.world().get::<Node>(row).unwrap().display, Display::Flex);
    std::fs::write(directory.path().join("interface.lingua"), "external edit").unwrap();
    click(&mut app, "Save all changes");
    let modal = app.world().resource::<Editor>().modal.as_ref().unwrap();
    assert_eq!(modal.overlay, overlay);
    assert_eq!(modal.filter, filter);
    assert_eq!(modal.draft.text(Shortcut::DeleteSand), "F2");
    assert_eq!(app.world().get::<ScrollPosition>(list).unwrap().0.y, 350.0);
    assert_eq!(app.world().resource::<InputFocus>().get(), Some(filter));
    assert!(
        app.world()
            .get::<Text>(modal.status)
            .unwrap()
            .0
            .contains("changed on disk")
    );
    assert_eq!(app.world().resource::<Settings>(), &Settings::default());
}
