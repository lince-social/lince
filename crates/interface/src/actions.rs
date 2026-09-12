use bevy::{
    input::keyboard::KeyboardInput,
    input_focus::{FocusedInput, InputFocus},
    prelude::*,
    ui::InteractionDisabled,
    ui_widgets::Activate,
};
use std::{collections::VecDeque, sync::Arc};

pub trait Action: Send + Sync + 'static {
    fn apply(&self, world: &mut World, target: Entity);

    fn connections(&self, _: &World, _: Entity) -> Vec<crate::inspection::Connection> {
        Vec::new()
    }
}

#[derive(Clone, Default)]
pub struct ActionSequence(Vec<Arc<dyn Action>>);

impl ActionSequence {
    pub fn then(mut self, action: impl Action) -> Self {
        self.0.push(Arc::new(action));
        self
    }

    pub fn run(&self, world: &mut World, target: Entity) {
        if crate::laboratory::suspended(world, target) {
            return;
        }
        for action in &self.0 {
            if world.get_entity(target).is_err() {
                break;
            }
            action.apply(world, target);
        }
        wake(world);
    }
}

impl Action for ActionSequence {
    fn apply(&self, world: &mut World, target: Entity) {
        self.run(world, target);
    }

    fn connections(&self, world: &World, target: Entity) -> Vec<crate::inspection::Connection> {
        self.0
            .iter()
            .flat_map(|action| action.connections(world, target))
            .collect()
    }
}

#[macro_export]
macro_rules! actions {
    ($($action:expr),* $(,)?) => {
        $crate::actions::ActionSequence::default()$(.then($action))*
    };
}

#[derive(Component, Clone)]
pub struct ActionButton {
    pub target: Entity,
    pub actions: ActionSequence,
}

impl ActionButton {
    pub fn new(target: Entity, actions: ActionSequence) -> Self {
        Self { target, actions }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub alt: bool,
    pub control: bool,
    pub shift: bool,
    pub super_key: bool,
}

impl Modifiers {
    pub const NONE: Self = Self {
        alt: false,
        control: false,
        shift: false,
        super_key: false,
    };
    pub const ALT: Self = Self {
        alt: true,
        ..Self::NONE
    };
    pub const CONTROL: Self = Self {
        control: true,
        ..Self::NONE
    };

    fn pressed(keys: &ButtonInput<KeyCode>) -> Self {
        Self {
            alt: keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]),
            control: keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]),
            shift: keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]),
            super_key: keys.any_pressed([KeyCode::SuperLeft, KeyCode::SuperRight]),
        }
    }
}

#[derive(Clone)]
pub struct KeyBinding {
    pub key: KeyCode,
    pub modifiers: Modifiers,
    pub actions: ActionSequence,
    pub repeat: bool,
}

impl KeyBinding {
    pub fn new(key: KeyCode, modifiers: Modifiers, actions: ActionSequence) -> Self {
        Self {
            key,
            modifiers,
            actions,
            repeat: false,
        }
    }
}

#[derive(Component, Clone, Default)]
pub struct KeyBindings(pub Vec<KeyBinding>);

#[derive(Component)]
pub struct WindowActionTarget(pub Entity);

struct Request {
    target: Entity,
    actions: ActionSequence,
}

#[derive(Resource, Default)]
struct PendingActions(VecDeque<Request>);

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ApplyActions;

pub struct ActionsPlugin;

impl Plugin for ActionsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PendingActions>()
            .init_resource::<InputFocus>()
            .add_observer(activate)
            .add_observer(keyboard)
            .add_systems(
                PostUpdate,
                apply_pending
                    .in_set(ApplyActions)
                    .after(bevy::text::EditableTextSystems),
            );
    }
}

pub fn dispatch(world: &mut World, target: Entity, actions: ActionSequence) {
    world
        .resource_mut::<PendingActions>()
        .0
        .push_back(Request { target, actions });
    wake(world);
}

fn wake(world: &World) {
    if let Some(wake) = world.get_resource::<crate::wake::WakeSignal>() {
        wake.ring();
    }
}

fn activate(
    event: On<Activate>,
    buttons: Query<&ActionButton, Without<InteractionDisabled>>,
    mut pending: ResMut<PendingActions>,
) {
    if let Ok(button) = buttons.get(event.entity) {
        pending.0.push_back(Request {
            target: button.target,
            actions: button.actions.clone(),
        });
    }
}

fn keyboard(
    mut event: On<FocusedInput<KeyboardInput>>,
    keys: Option<Res<ButtonInput<KeyCode>>>,
    scopes: Query<&KeyBindings, Without<InteractionDisabled>>,
    windows: Query<(&Window, Option<&WindowActionTarget>)>,
    mut pending: ResMut<PendingActions>,
) {
    if !event.input.state.is_pressed() {
        return;
    }
    if windows
        .get(event.input.window)
        .is_ok_and(|(window, _)| !window.focused)
    {
        return;
    }
    let mut target = event.focused_entity;
    if let Ok((_, Some(default_target))) = windows.get(target) {
        if event.original_event_target() != target {
            return;
        }
        target = default_target.0;
    }
    let Ok(bindings) = scopes.get(target) else {
        return;
    };
    let modifiers = keys.as_deref().map(Modifiers::pressed).unwrap_or_default();
    if let Some(binding) = bindings
        .0
        .iter()
        .rev()
        .find(|binding| binding.key == event.input.key_code && binding.modifiers == modifiers)
    {
        if !event.input.repeat || binding.repeat {
            pending.0.push_back(Request {
                target,
                actions: binding.actions.clone(),
            });
        }
        event.propagate(false);
    }
}

fn apply_pending(world: &mut World) {
    let requests = std::mem::take(&mut world.resource_mut::<PendingActions>().0);
    for request in requests {
        request.actions.run(world, request.target);
    }
}

pub(crate) mod tests {
    use super::*;
    use bevy::{
        input::{ButtonState, InputPlugin, InputSystems, keyboard::Key},
        input_focus::{InputFocusSystems, dispatch_focused_input},
        window::PrimaryWindow,
    };

    #[derive(Component, Default)]
    struct Values(Vec<i32>);

    struct Append(i32);

    impl Action for Append {
        fn apply(&self, world: &mut World, target: Entity) {
            world.get_mut::<Values>(target).unwrap().0.push(self.0);
        }
    }

    struct Sum;

    impl Action for Sum {
        fn apply(&self, world: &mut World, target: Entity) {
            let mut values = world.get_mut::<Values>(target).unwrap();
            let sum = values.0.iter().sum();
            values.0.push(sum);
        }
    }

    fn fixture() -> (App, Entity, Entity) {
        let mut app = App::new();
        crate::laboratory::isolate(app.world_mut());
        app.add_plugins((InputPlugin, ActionsPlugin)).add_systems(
            PreUpdate,
            dispatch_focused_input::<KeyboardInput>
                .in_set(InputFocusSystems::Dispatch)
                .after(InputSystems),
        );
        let root = app.world_mut().spawn(Values::default()).id();
        let window = app
            .world_mut()
            .spawn((Window::default(), PrimaryWindow, WindowActionTarget(root)))
            .id();
        (app, root, window)
    }

    fn key(app: &mut App, window: Entity, key_code: KeyCode, state: ButtonState, repeat: bool) {
        app.world_mut().write_message(KeyboardInput {
            key_code,
            logical_key: Key::Unidentified(bevy::input::keyboard::NativeKey::Unidentified),
            state,
            text: None,
            repeat,
            window,
        });
        app.update();
    }

    fn bindings(value: i32) -> KeyBindings {
        KeyBindings(vec![KeyBinding::new(
            KeyCode::KeyE,
            Modifiers::ALT,
            crate::actions![Append(value)],
        )])
    }

    #[cfg_attr(test, test)]
    fn buttons_and_dispatch_share_ordered_parameterized_actions() {
        let (mut app, root, _) = fixture();
        let sequence = crate::actions![Append(4), crate::actions![Append(7), Sum]];
        let button = app
            .world_mut()
            .spawn(ActionButton::new(root, sequence.clone()))
            .id();
        app.world_mut().trigger(Activate { entity: button });
        app.world_mut().trigger(Activate { entity: button });
        app.update();
        assert_eq!(
            app.world().get::<Values>(root).unwrap().0,
            [4, 7, 11, 4, 7, 33]
        );
        dispatch(app.world_mut(), root, sequence);
        app.update();
        assert_eq!(
            app.world().get::<Values>(root).unwrap().0,
            [4, 7, 11, 4, 7, 33, 4, 7, 77]
        );
        app.world_mut()
            .entity_mut(button)
            .insert(InteractionDisabled);
        app.world_mut().trigger(Activate { entity: button });
        app.update();
        assert_eq!(app.world().get::<Values>(root).unwrap().0.len(), 9);
    }

    #[cfg_attr(test, test)]
    fn shortcuts_match_exact_modifiers_and_ignore_release_repeat_and_unfocused_windows() {
        let (mut app, root, window) = fixture();
        app.world_mut().entity_mut(root).insert(bindings(1));
        key(&mut app, window, KeyCode::KeyE, ButtonState::Pressed, false);
        assert!(app.world().get::<Values>(root).unwrap().0.is_empty());
        key(
            &mut app,
            window,
            KeyCode::AltRight,
            ButtonState::Pressed,
            false,
        );
        key(&mut app, window, KeyCode::KeyE, ButtonState::Pressed, false);
        key(&mut app, window, KeyCode::KeyE, ButtonState::Pressed, true);
        key(
            &mut app,
            window,
            KeyCode::KeyE,
            ButtonState::Released,
            false,
        );
        key(
            &mut app,
            window,
            KeyCode::ControlLeft,
            ButtonState::Pressed,
            false,
        );
        key(&mut app, window, KeyCode::KeyE, ButtonState::Pressed, false);
        key(
            &mut app,
            window,
            KeyCode::ControlLeft,
            ButtonState::Released,
            false,
        );
        app.world_mut().get_mut::<Window>(window).unwrap().focused = false;
        key(&mut app, window, KeyCode::KeyE, ButtonState::Pressed, false);
        assert_eq!(app.world().get::<Values>(root).unwrap().0, [1]);
    }

    #[cfg_attr(test, test)]
    fn nearest_scope_wins_and_later_bindings_override_defaults() {
        let (mut app, root, window) = fixture();
        app.world_mut().entity_mut(root).insert(bindings(1));
        let child = app
            .world_mut()
            .spawn((Values::default(), bindings(2), ChildOf(root)))
            .id();
        app.world_mut()
            .get_mut::<KeyBindings>(child)
            .unwrap()
            .0
            .push(KeyBinding::new(
                KeyCode::KeyE,
                Modifiers::ALT,
                crate::actions![Append(3)],
            ));
        let focused = app.world_mut().spawn(ChildOf(child)).id();
        app.insert_resource(InputFocus::from_entity(focused));
        key(
            &mut app,
            window,
            KeyCode::AltLeft,
            ButtonState::Pressed,
            false,
        );
        key(&mut app, window, KeyCode::KeyE, ButtonState::Pressed, false);
        assert_eq!(app.world().get::<Values>(child).unwrap().0, [3]);
        assert!(app.world().get::<Values>(root).unwrap().0.is_empty());
        app.world_mut().entity_mut(child).remove::<KeyBindings>();
        key(&mut app, window, KeyCode::KeyE, ButtonState::Pressed, false);
        assert_eq!(app.world().get::<Values>(root).unwrap().0, [1]);
        let unrelated = app.world_mut().spawn_empty().id();
        app.insert_resource(InputFocus::from_entity(unrelated));
        key(&mut app, window, KeyCode::KeyE, ButtonState::Pressed, false);
        assert_eq!(app.world().get::<Values>(root).unwrap().0, [1]);
    }

    #[cfg_attr(test, test)]
    fn despawned_targets_are_skipped_and_queued_followups_wait_for_next_frame() {
        struct Followup;
        impl Action for Followup {
            fn apply(&self, world: &mut World, target: Entity) {
                dispatch(world, target, crate::actions![Append(9)]);
            }
        }
        let (mut app, root, _) = fixture();
        let gone = app.world_mut().spawn(Values::default()).id();
        dispatch(app.world_mut(), gone, crate::actions![Append(1)]);
        app.world_mut().despawn(gone);
        dispatch(app.world_mut(), root, crate::actions![Followup, Append(2)]);
        app.update();
        assert_eq!(app.world().get::<Values>(root).unwrap().0, [2]);
        app.update();
        assert_eq!(app.world().get::<Values>(root).unwrap().0, [2, 9]);
        app.update();
        assert!(
            !app.world()
                .entity(root)
                .get_ref::<Values>()
                .unwrap()
                .is_changed()
        );
    }

    crate::laboratory_cases! {
        buttons_and_dispatch_share_ordered_parameterized_actions,
        shortcuts_match_exact_modifiers_and_ignore_release_repeat_and_unfocused_windows,
        nearest_scope_wins_and_later_bindings_override_defaults,
        despawned_targets_are_skipped_and_queued_followups_wait_for_next_frame,
    }
}
