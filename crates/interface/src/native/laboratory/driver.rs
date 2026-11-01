pub use crate::testing::ui::{UiAction, UiBackend, UiCondition, UiSnapshot};
pub use crate::testing::{Limits, Runner, Selector, Step};

use crate::native::{
    actions::{Action, ActionButton, ActionsPlugin, WindowActionTarget},
    workspace::Workspaces,
};
use bevy::{
    a11y::AccessibilityNode,
    input::{
        InputPlugin, InputSystems,
        keyboard::{Key, KeyboardInput},
        mouse::MouseButtonInput,
    },
    input_focus::{InputFocusSystems, dispatch_focused_input},
    picking::{
        InteractionPlugin, PickingPlugin, PickingSystems,
        backend::{HitData, PointerHits},
        pointer::{PointerId, PointerInput, PointerLocation},
    },
    prelude::*,
    text::{EditableText, FontCx, LayoutCx},
    time::TimeUpdateStrategy,
    window::{PrimaryWindow, WindowEvent},
};
use std::time::Duration;

#[derive(Component, Default)]
struct Count(usize);

#[derive(Resource, Default)]
struct InputTrace {
    pointer: usize,
    mouse: usize,
    ordered_mouse: usize,
    keyboard: usize,
    ordered_keyboard: usize,
}

fn input_trace(
    mut pointer: MessageReader<PointerInput>,
    mut mouse: MessageReader<MouseButtonInput>,
    mut keyboard: MessageReader<KeyboardInput>,
    mut ordered: MessageReader<WindowEvent>,
    mut trace: ResMut<InputTrace>,
) {
    trace.pointer += pointer.read().count();
    trace.mouse += mouse.read().count();
    trace.keyboard += keyboard.read().count();
    for event in ordered.read() {
        match event {
            WindowEvent::MouseButtonInput(_) => trace.ordered_mouse += 1,
            WindowEvent::KeyboardInput(_) => trace.ordered_keyboard += 1,
            _ => {}
        }
    }
}

struct Increment;

impl Action for Increment {
    fn apply(&self, world: &mut World, target: Entity) {
        world.get_mut::<Count>(target).unwrap().0 += 1;
    }
}

#[derive(Resource)]
struct TestHit {
    button: Entity,
    camera: Entity,
}

fn hit_backend(
    pointers: Query<(&PointerId, &PointerLocation)>,
    hit: Res<TestHit>,
    mut hits: MessageWriter<PointerHits>,
) {
    if hit.button == Entity::PLACEHOLDER {
        return;
    }
    for (pointer, location) in &pointers {
        if location.location().is_some_and(|location| {
            Rect::from_center_size(Vec2::splat(50.0), Vec2::splat(30.0)).contains(location.position)
        }) {
            hits.write(PointerHits::new(
                *pointer,
                vec![(hit.button, HitData::new(hit.camera, 0.0, None, None))],
                1.0,
            ));
        }
    }
}

fn fixture() -> (App, Entity, Entity, Entity, Entity) {
    let mut app = App::new();
    super::isolate(app.world_mut());
    app.add_plugins((
        MinimalPlugins,
        InputPlugin,
        PickingPlugin,
        InteractionPlugin,
        bevy::ui_widgets::ButtonPlugin,
        ActionsPlugin,
    ))
    .init_resource::<FontCx>()
    .init_resource::<LayoutCx>()
    .init_resource::<bevy::clipboard::Clipboard>()
    .init_resource::<UiScale>()
    .init_resource::<InputTrace>()
    .add_message::<WindowEvent>()
    .add_systems(PreUpdate, input_trace.before(InputSystems))
    .add_message::<bevy::window::Ime>()
    .add_plugins(bevy::ui_widgets::EditableTextInputPlugin)
    .add_systems(
        PreUpdate,
        dispatch_focused_input::<KeyboardInput>
            .in_set(InputFocusSystems::Dispatch)
            .after(InputSystems),
    )
    .add_systems(
        PostUpdate,
        bevy::text::apply_text_edits.in_set(bevy::text::EditableTextSystems),
    )
    .add_systems(PreUpdate, hit_backend.in_set(PickingSystems::Backend));
    let font = Font::from_bytes(
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../institute/assets/fonts/Lato/Lato-Regular.ttf"
        ))
        .to_vec(),
    );
    app.world_mut()
        .resource_mut::<FontCx>()
        .collection
        .register_fonts(font.data, None);
    app.world_mut()
        .resource_mut::<FontCx>()
        .set_sans_serif_family("Lato")
        .unwrap();
    let root = app
        .world_mut()
        .spawn((Workspaces::default(), Count::default()))
        .id();
    let window = app
        .world_mut()
        .spawn((Window::default(), PrimaryWindow, WindowActionTarget(root)))
        .id();
    let camera = app.world_mut().spawn_empty().id();
    let mut label = accesskit::Node::new(accesskit::Role::Button);
    label.set_label("Submit draft");
    let button = app
        .world_mut()
        .spawn((
            bevy::ui_widgets::Button,
            AccessibilityNode(label),
            Node::default(),
            ComputedNode {
                size: Vec2::splat(30.0),
                ..default()
            },
            UiGlobalTransform::from_translation(Vec2::splat(50.0)),
            InheritedVisibility::VISIBLE,
            ChildOf(root),
            ActionButton::new(root, crate::native::actions![Increment]),
        ))
        .id();
    let mut label = accesskit::Node::new(accesskit::Role::TextInput);
    label.set_label("Draft title");
    let field = app
        .world_mut()
        .spawn((
            EditableText::new("old draft"),
            AccessibilityNode(label),
            Node::default(),
            InheritedVisibility::VISIBLE,
            ChildOf(root),
        ))
        .id();
    app.world_mut().spawn(PointerId::Mouse);
    app.insert_resource(TestHit { button, camera });
    app.update();
    (app, root, window, button, field)
}

fn runner() -> Runner {
    Runner::new(Limits {
        timeout: Duration::from_secs(2),
        max_polls: 16,
    })
}

fn clicked(world: &World) -> bool {
    world
        .iter_entities()
        .any(|entity| entity.get::<Count>().is_some_and(|count| count.0 == 1))
}

#[cfg_attr(test, test)]
fn virtual_pointer_and_keyboard_activate_the_normal_button_pipeline_once() {
    let (mut app, root, _, _, _) = fixture();
    let step = Step::action(
        "submit using pointer",
        Selector::new("Submit draft").with_role("Button"),
        UiAction::Click,
        UiCondition::Check(clicked),
    );
    let report = runner().run(&mut UiBackend::new(&mut app, root), &step);
    assert!(
        report.is_success(),
        "{:?}: {}",
        report.outcome,
        report.diagnostics
    );
    assert_eq!(report.metrics.dispatches, 1);
    assert!(matches!(
        app.world().resource::<TimeUpdateStrategy>(),
        TimeUpdateStrategy::Automatic
    ));
    assert_eq!(app.world().get::<Count>(root).unwrap().0, 1);
    let trace = app.world().resource::<InputTrace>();
    assert_eq!((trace.mouse, trace.ordered_mouse), (2, 2));
    app.world_mut().get_mut::<Count>(root).unwrap().0 = 0;
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
        37,
    )));
    let step = Step::action(
        "submit using keyboard",
        Selector::new("Submit draft").with_role("Button"),
        UiAction::Key(KeyCode::Enter, Key::Enter),
        UiCondition::Check(clicked),
    );
    let report = runner().run(&mut UiBackend::new(&mut app, root), &step);
    assert!(
        report.is_success(),
        "{:?}: {}",
        report.outcome,
        report.diagnostics
    );
    assert_eq!(app.world().get::<Count>(root).unwrap().0, 1);
    let trace = app.world().resource::<InputTrace>();
    assert_eq!((trace.keyboard, trace.ordered_keyboard), (2, 2));
    assert!(matches!(
        app.world().resource::<TimeUpdateStrategy>(),
        TimeUpdateStrategy::ManualDuration(duration) if *duration == Duration::from_millis(37)
    ));
    let (mut app, root, _, _, _) = fixture();
    app.add_plugins(bevy::picking::input::PointerInputPlugin);
    app.update();
    let step = Step::action(
        "submit using forwarded pointer",
        Selector::new("Submit draft").with_role("Button"),
        UiAction::Click,
        UiCondition::Check(clicked),
    );
    let report = runner().run(&mut UiBackend::new(&mut app, root), &step);
    assert!(
        report.is_success(),
        "{:?}: {}",
        report.outcome,
        report.diagnostics
    );
    assert_eq!(app.world().get::<Count>(root).unwrap().0, 1);
    let trace = app.world().resource::<InputTrace>();
    assert_eq!((trace.pointer, trace.mouse, trace.ordered_mouse), (3, 2, 2));
}

#[cfg_attr(test, test)]
fn keyboard_replaces_text_through_the_normal_editor_pipeline_without_leaking_payloads() {
    let (mut app, root, _, _, field) = fixture();
    let title = Selector::new("Draft title").with_role("TextInput");
    let step = Step::action(
        "replace draft title",
        title.clone(),
        UiAction::Text("Café 界".into()),
        UiCondition::Value(title, "Café 界".into()),
    );
    let report = runner().run(&mut UiBackend::new(&mut app, root), &step);
    assert!(
        report.is_success(),
        "{:?}: {}",
        report.outcome,
        report.diagnostics
    );
    assert_eq!(
        app.world().get::<EditableText>(field).unwrap().value(),
        "Café 界"
    );
    assert!(!report.diagnostics.contains("Café"));
}

#[cfg_attr(test, test)]
fn unavailable_pointer_uses_explicit_accessibility_fallback_and_never_replays_accepted_input() {
    let (mut app, root, window, _, _) = fixture();
    app.world_mut().despawn(window);
    let step = Step::action(
        "fallback submission",
        Selector::new("Submit draft").with_role("Button"),
        UiAction::Click,
        UiCondition::Check(clicked),
    )
    .with_fallback();
    let report = runner().run(&mut UiBackend::new(&mut app, root), &step);
    assert!(
        report.is_success(),
        "{:?}: {}",
        report.outcome,
        report.diagnostics
    );
    assert_eq!(report.metrics.dispatches, 2);
    assert_eq!(app.world().get::<Count>(root).unwrap().0, 1);
    let (mut app, root, _, _, _) = fixture();
    app.world_mut().remove_resource::<TestHit>();
    app.world_mut().insert_resource(TestHit {
        button: Entity::PLACEHOLDER,
        camera: Entity::PLACEHOLDER,
    });
    let report = runner().run(&mut UiBackend::new(&mut app, root), &step);
    assert_eq!(report.outcome, crate::testing::Outcome::PollLimit);
    assert_eq!(report.metrics.dispatches, 1);
    assert_eq!(app.world().get::<Count>(root).unwrap().0, 0);
}

#[cfg_attr(test, test)]
fn selectors_reject_hidden_disabled_ambiguous_and_stale_controls_with_bounded_diagnostics() {
    use crate::testing::{Backend, Dispatch, Route};
    let (mut app, root, _, button, _) = fixture();
    let selector = Selector::new("Submit draft").with_role("Button");
    let other_root = app.world_mut().spawn(Workspaces::default()).id();
    let outside_label = app
        .world()
        .get::<AccessibilityNode>(button)
        .unwrap()
        .clone();
    app.world_mut().spawn((
        outside_label,
        Node::default(),
        InheritedVisibility::VISIBLE,
        ChildOf(other_root),
    ));
    let snapshot = {
        let mut backend = UiBackend::new(&mut app, root);
        let snapshot = backend.snapshot(Duration::from_secs(1)).unwrap();
        assert!(
            backend
                .condition(&snapshot, &UiCondition::Visible(selector.clone()))
                .unwrap()
        );
        snapshot
    };
    app.world_mut()
        .entity_mut(button)
        .insert(bevy::ui::InteractionDisabled);
    {
        let mut backend = UiBackend::new(&mut app, root);
        assert!(matches!(
            backend.dispatch(
                &snapshot,
                &selector,
                &UiAction::Click,
                Route::Primary,
                Duration::from_secs(1)
            ),
            Dispatch::Rejected(_)
        ));
    }
    app.world_mut()
        .entity_mut(button)
        .remove::<bevy::ui::InteractionDisabled>();
    app.world_mut().get_mut::<Node>(button).unwrap().display = Display::None;
    let step = Step::action(
        "hidden submission",
        selector.clone(),
        UiAction::Click,
        UiCondition::Check(clicked),
    )
    .with_fallback();
    let report = runner().run(&mut UiBackend::new(&mut app, root), &step);
    assert_eq!(report.outcome, crate::testing::Outcome::Rejected);
    app.world_mut().get_mut::<Node>(button).unwrap().display = Display::Flex;
    let node = app
        .world()
        .get::<AccessibilityNode>(button)
        .unwrap()
        .clone();
    let duplicate = app
        .world_mut()
        .spawn((
            node,
            Node::default(),
            InheritedVisibility::VISIBLE,
            ChildOf(root),
        ))
        .id();
    let private_label = "Private point 12.345678,-98.765432 at 42 Confidential Lane";
    let mut private_node = accesskit::Node::new(accesskit::Role::Button);
    private_node.set_label(private_label);
    app.world_mut().spawn((
        AccessibilityNode(private_node),
        Node::default(),
        InheritedVisibility::VISIBLE,
        ChildOf(root),
    ));
    let report = runner().run(&mut UiBackend::new(&mut app, root), &step);
    assert_eq!(report.outcome, crate::testing::Outcome::Rejected);
    assert_eq!(report.selector, selector);
    assert!(!report.diagnostics.contains(private_label));
    assert!(!report.diagnostics.contains("12.345678"));
    assert!(!report.diagnostics.contains("Confidential Lane"));
    let mut limited = Runner::new(Limits {
        timeout: Duration::ZERO,
        max_polls: 16,
    });
    let report = limited.run(&mut UiBackend::new(&mut app, root), &step);
    assert_eq!(report.outcome, crate::testing::Outcome::Timeout);
    assert_eq!(report.metrics.dispatches, 0);
    app.world_mut().despawn(duplicate);
    let snapshot = {
        let mut backend = UiBackend::new(&mut app, root);
        backend.snapshot(Duration::from_secs(1)).unwrap()
    };
    app.world_mut().despawn(button);
    let mut backend = UiBackend::new(&mut app, root);
    assert!(matches!(
        backend.dispatch(
            &snapshot,
            &selector,
            &UiAction::Click,
            Route::Primary,
            Duration::from_secs(1)
        ),
        Dispatch::Rejected(_)
    ));
}

#[cfg_attr(test, test)]
fn failed_native_step_discards_remaining_text_and_releases_modifiers() {
    use crate::testing::{Backend, Dispatch, Route};
    fn released(world: &World) -> bool {
        !world
            .resource::<ButtonInput<KeyCode>>()
            .pressed(KeyCode::ControlLeft)
    }
    let (mut app, root, _, _, field) = fixture();
    let selector = Selector::new("Draft title").with_role("TextInput");
    let step = Step::action(
        "bounded text input",
        selector.clone(),
        UiAction::Text("must not arrive".into()),
        UiCondition::Value(selector.clone(), "must not arrive".into()),
    );
    {
        let mut backend = UiBackend::new(&mut app, root);
        let report = Runner::new(Limits {
            timeout: Duration::from_secs(2),
            max_polls: 2,
        })
        .run(&mut backend, &step);
        assert_eq!(report.outcome, crate::testing::Outcome::PollLimit);
        let report = runner().run(
            &mut backend,
            &Step::observe(
                "released input",
                selector.clone(),
                UiCondition::Check(released),
            ),
        );
        assert!(report.is_success());
    }
    assert_eq!(
        app.world().get::<EditableText>(field).unwrap().value(),
        "old draft"
    );
    {
        let mut backend = UiBackend::new(&mut app, root);
        let snapshot = backend.snapshot(Duration::from_secs(1)).unwrap();
        assert!(matches!(
            backend.dispatch(
                &snapshot,
                &selector,
                step.action.as_ref().unwrap(),
                Route::Primary,
                Duration::from_secs(1)
            ),
            Dispatch::Accepted
        ));
        backend.pump(Duration::from_secs(1)).unwrap();
    }
    app.update();
    assert!(released(app.world()));
    assert_eq!(
        app.world().get::<EditableText>(field).unwrap().value(),
        "old draft"
    );
    app.world_mut().remove_resource::<TimeUpdateStrategy>();
    drop(UiBackend::new(&mut app, root));
    assert!(!app.world().contains_resource::<TimeUpdateStrategy>());
}

crate::native::laboratory_cases! {
    virtual_pointer_and_keyboard_activate_the_normal_button_pipeline_once,
    keyboard_replaces_text_through_the_normal_editor_pipeline_without_leaking_payloads,
    unavailable_pointer_uses_explicit_accessibility_fallback_and_never_replays_accepted_input,
    selectors_reject_hidden_disabled_ambiguous_and_stale_controls_with_bounded_diagnostics,
    failed_native_step_discards_remaining_text_and_releases_modifiers,
}
