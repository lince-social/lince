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

    fn practice_intent(&self) -> PracticeIntent {
        PracticeIntent::Target
    }

    fn tutorial_operations(&self) -> &'static [lince_interface::practice::Operation] {
        &[]
    }

    fn tutorial_supports(&self) -> &'static [lince_interface::practice::Operation] {
        &[]
    }

    fn teaches_tutorial(&self, operation: lince_interface::practice::Operation) -> bool {
        self.tutorial_operations().contains(&operation)
            || self.practice_intent() == PracticeIntent::Feature(operation)
    }

    fn supports_tutorial(&self, operation: lince_interface::practice::Operation) -> bool {
        self.tutorial_supports().contains(&operation)
    }

    fn connections(&self, _: &World, _: Entity) -> Vec<crate::native::inspection::Connection> {
        Vec::new()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PracticeIntent {
    Target,
    Navigation,
    Recovery,
    Feature(lince_interface::practice::Operation),
    Object(Entity),
    SelectedArea,
    SelectedObjects,
    EditedSand,
}

#[derive(Component)]
pub(crate) struct ControlOwner(pub Entity);

#[derive(Component)]
pub(crate) struct TutorialControl {
    pub owner: Entity,
    pub operation: lince_interface::practice::Operation,
}

#[derive(Component)]
pub(crate) struct TutorialField {
    pub owner: Entity,
    pub operation: lince_interface::practice::Operation,
}

#[derive(Clone, Default)]
pub struct ActionSequence(Vec<Arc<dyn Action>>);

impl ActionSequence {
    pub fn then(mut self, action: impl Action) -> Self {
        self.0.push(Arc::new(action));
        self
    }

    pub fn run(&self, world: &mut World, target: Entity) {
        if crate::native::laboratory::suspended(world, target) {
            return;
        }
        for action in &self.0 {
            if world.get_entity(target).is_err() {
                break;
            }
            if !crate::native::instinct::practice::permits_control(world, target, action.as_ref()) {
                continue;
            }
            action.apply(world, target);
        }
        wake(world);
    }

    pub(crate) fn permitted(&self, world: &World, target: Entity) -> bool {
        self.0.iter().all(|action| {
            crate::native::instinct::practice::permits_control(world, target, action.as_ref())
        })
    }

    pub(crate) fn teaches(&self, operation: lince_interface::practice::Operation) -> bool {
        self.0
            .iter()
            .any(|action| action.teaches_tutorial(operation))
    }
}

impl Action for ActionSequence {
    fn teaches_tutorial(&self, operation: lince_interface::practice::Operation) -> bool {
        self.teaches(operation)
    }

    fn supports_tutorial(&self, operation: lince_interface::practice::Operation) -> bool {
        self.0
            .iter()
            .any(|action| action.supports_tutorial(operation))
    }

    fn apply(&self, world: &mut World, target: Entity) {
        self.run(world, target);
    }

    fn connections(
        &self,
        world: &World,
        target: Entity,
    ) -> Vec<crate::native::inspection::Connection> {
        self.0
            .iter()
            .flat_map(|action| action.connections(world, target))
            .collect()
    }
}

#[macro_export]
macro_rules! actions {
    ($($action:expr),* $(,)?) => {
        $crate::native::actions::ActionSequence::default()$(.then($action))*
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Modifiers {
    pub alt: bool,
    pub control: bool,
    pub shift: bool,
    pub super_key: bool,
}

impl Modifiers {
    pub(crate) fn contains(self, other: Self) -> bool {
        (!other.control || self.control)
            && (!other.alt || self.alt)
            && (!other.shift || self.shift)
            && (!other.super_key || self.super_key)
    }

    pub(crate) fn intersection(self, other: Self) -> Self {
        Self {
            control: self.control && other.control,
            alt: self.alt && other.alt,
            shift: self.shift && other.shift,
            super_key: self.super_key && other.super_key,
        }
    }

    pub(crate) fn difference(self, other: Self) -> Self {
        Self {
            control: self.control && !other.control,
            alt: self.alt && !other.alt,
            shift: self.shift && !other.shift,
            super_key: self.super_key && !other.super_key,
        }
    }

    pub(crate) fn union(self, other: Self) -> Self {
        Self {
            control: self.control || other.control,
            alt: self.alt || other.alt,
            shift: self.shift || other.shift,
            super_key: self.super_key || other.super_key,
        }
    }

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

    pub(crate) fn pressed(keys: &ButtonInput<KeyCode>) -> Self {
        Self {
            alt: keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]),
            control: keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]),
            shift: keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]),
            super_key: keys.any_pressed([KeyCode::SuperLeft, KeyCode::SuperRight]),
        }
    }

    pub(crate) fn pressed_for(
        keys: &ButtonInput<KeyCode>,
        keyboard: Option<&crate::host::input::KeyboardSnapshot>,
    ) -> Self {
        let Some(keyboard) = keyboard.filter(|keyboard| !keyboard.labels.is_empty()) else {
            return Self::pressed(keys);
        };
        let mut result = Self::NONE;
        for key in keys.get_pressed() {
            let Some(label) = keyboard.labels.get(&format!("{key:?}")) else {
                continue;
            };
            result.control |= label.modifiers.control;
            result.alt |= label.modifiers.alt;
            result.shift |= label.modifiers.shift;
            result.super_key |= label.modifiers.super_key;
        }
        result
    }
}

pub(crate) fn native_key_matches(
    physical: KeyCode,
    logical: &bevy::input::keyboard::Key,
    expected: KeyCode,
) -> bool {
    use bevy::input::keyboard::Key;
    let named = match expected {
        KeyCode::Escape => Key::Escape,
        KeyCode::Enter | KeyCode::NumpadEnter => Key::Enter,
        KeyCode::Tab => Key::Tab,
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Delete => Key::Delete,
        KeyCode::Insert => Key::Insert,
        KeyCode::ArrowLeft => Key::ArrowLeft,
        KeyCode::ArrowRight => Key::ArrowRight,
        KeyCode::ArrowUp => Key::ArrowUp,
        KeyCode::ArrowDown => Key::ArrowDown,
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,
        KeyCode::Space => Key::Space,
        _ => return physical == expected,
    };
    if matches!(logical, Key::Unidentified(_)) {
        physical == expected
    } else if expected == KeyCode::Space
        && matches!(logical, Key::Character(value) if value.as_str() == " ")
    {
        true
    } else {
        logical == &named
    }
}

#[derive(Clone)]
pub struct KeyBinding {
    pub key: KeyCode,
    pub modifiers: Modifiers,
    pub actions: ActionSequence,
    pub repeat: bool,
    pub outside_text: bool,
}

impl KeyBinding {
    pub fn new(key: KeyCode, modifiers: Modifiers, actions: ActionSequence) -> Self {
        Self {
            key,
            modifiers,
            actions,
            repeat: false,
            outside_text: false,
        }
    }

    pub fn outside_text(mut self) -> Self {
        self.outside_text = true;
        self
    }
}

#[derive(Component, Clone, Default)]
pub struct KeyBindings(pub Vec<KeyBinding>);

#[derive(Component)]
pub struct WindowActionTarget(pub Entity);

struct Request {
    target: Entity,
    actions: ActionSequence,
    activity: Option<u64>,
}

#[derive(Resource, Default)]
struct PendingActions(VecDeque<Request>);

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ApplyActions;

pub struct ActionsPlugin;

impl Plugin for ActionsPlugin {
    fn build(&self, app: &mut App) {
        crate::native::keyboard::install(app);
        app.init_resource::<PendingActions>()
            .init_resource::<InputFocus>()
            .add_observer(activate)
            .add_observer(keyboard)
            .add_systems(
                PreUpdate,
                crate::native::shortcuts::desktop_input
                    .after(crate::native::keyboard::Prepare)
                    .after(crate::native::shortcuts::CaptureInput)
                    .before(bevy::input_focus::InputFocusSystems::Dispatch),
            )
            .add_systems(
                PostUpdate,
                apply_pending
                    .in_set(ApplyActions)
                    .after(bevy::text::EditableTextSystems),
            );
    }
}

pub fn dispatch(world: &mut World, target: Entity, actions: ActionSequence) {
    let activity = world.get_resource::<crate::host::input::InputActivity>().map(|activity| activity.frame_revision());
    world
        .resource_mut::<PendingActions>()
        .0
        .push_back(Request { target, actions, activity });
    wake(world);
}

fn wake(world: &World) {
    if let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>() {
        wake.ring();
    }
}

fn activate(
    event: On<Activate>,
    buttons: Query<&ActionButton, Without<InteractionDisabled>>,
    mut pending: ResMut<PendingActions>,
    activity: Option<Res<crate::host::input::InputActivity>>,
) {
    if let Ok(button) = buttons.get(event.entity) {
        pending.0.push_back(Request {
            target: button.target,
            actions: button.actions.clone(),
            activity: activity.map(|activity| activity.frame_revision()),
        });
    }
}

fn keyboard(
    mut event: On<FocusedInput<KeyboardInput>>,
    keys: Option<Res<ButtonInput<KeyCode>>>,
    scopes: Query<
        (
            Option<&KeyBindings>,
            Option<&crate::native::shortcuts::ConfiguredBindings>,
        ),
        Without<InteractionDisabled>,
    >,
    parents: Query<&ChildOf>,
    editors: Query<(), With<bevy::text::EditableText>>,
    windows: Query<(&Window, Option<&WindowActionTarget>)>,
    keyboard_modal: Option<Res<crate::native::shortcuts::KeyboardModalOwner>>,
    keyboard: Option<Res<crate::host::input::KeyboardSnapshot>>,
    context: Option<Res<crate::native::keyboard::Dispatch>>,
    mut pending: ResMut<PendingActions>,
    activity: Option<Res<crate::host::input::InputActivity>>,
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
    let Ok((bindings, configured)) = scopes.get(target) else {
        return;
    };
    let modifiers = context
        .as_deref()
        .filter(|context| context.window == event.input.window)
        .map(|context| context.modifiers)
        .unwrap_or_else(|| {
            keys.as_deref()
                .map(|keys| Modifiers::pressed_for(keys, keyboard.as_deref()))
                .unwrap_or_default()
        });
    let modal = keyboard_modal
        .as_deref()
        .and_then(|owner| owner.0)
        .filter(|owner| scopes.contains(*owner));
    let configured = configured.filter(|_| modal.is_none());
    let bindings = bindings.filter(|_| {
        let Some(owner) = modal else {
            return true;
        };
        let mut cursor = Some(target);
        for _ in 0..crate::native::layout::MAX_DEPTH {
            let Some(entity) = cursor else {
                break;
            };
            if entity == owner {
                return true;
            }
            cursor = parents.get(entity).ok().map(ChildOf::parent);
        }
        false
    });
    if let Some(binding) = bindings
        .into_iter()
        .flat_map(|bindings| bindings.0.iter().rev())
        .map(|binding| (binding, true))
        .chain(
            configured
                .into_iter()
                .flat_map(|bindings| bindings.0.iter())
                .map(|binding| (binding, false)),
        )
        .find_map(|(binding, native)| {
            let matches = if native {
                native_key_matches(event.input.key_code, &event.input.logical_key, binding.key)
            } else {
                binding.key == event.input.key_code
            };
            (matches && binding.modifiers == modifiers).then_some(binding)
        })
    {
        if binding.outside_text {
            let mut cursor = Some(event.original_event_target());
            while let Some(entity) = cursor {
                if editors.contains(entity) {
                    return;
                }
                cursor = parents.get(entity).ok().map(ChildOf::parent);
            }
        }
        if !event.input.repeat || binding.repeat {
            pending.0.push_back(Request {
                target,
                actions: binding.actions.clone(),
                activity: activity.as_ref().map(|activity| activity.frame_revision()),
            });
        }
        event.propagate(false);
    }
}

fn apply_pending(world: &mut World) {
    let requests = std::mem::take(&mut world.resource_mut::<PendingActions>().0);
    for request in requests {
        if world.get_resource::<crate::host::input::InputActivity>().is_some_and(|activity| !activity.active() || request.activity != Some(activity.revision())) { continue; }
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
        crate::native::laboratory::isolate(app.world_mut());
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
            crate::native::actions![Append(value)],
        )])
    }

    #[cfg_attr(test, test)]
    fn buttons_and_dispatch_share_ordered_parameterized_actions() {
        let (mut app, root, _) = fixture();
        let sequence = crate::native::actions![Append(4), crate::native::actions![Append(7), Sum]];
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
                crate::native::actions![Append(3)],
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
                dispatch(world, target, crate::native::actions![Append(9)]);
            }
        }
        let (mut app, root, _) = fixture();
        let gone = app.world_mut().spawn(Values::default()).id();
        dispatch(app.world_mut(), gone, crate::native::actions![Append(1)]);
        app.world_mut().despawn(gone);
        dispatch(
            app.world_mut(),
            root,
            crate::native::actions![Followup, Append(2)],
        );
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

    #[test]
    fn queued_ui_actions_refuse_lost_and_returned_activity_while_fresh_and_windowed_actions_still_run() {
        for returned in [false, true] {
            let (mut app, root, _) = fixture();
            let activity = crate::host::input::InputActivity::default();
            app.insert_resource(activity.clone());
            app.update();
            dispatch(app.world_mut(), root, ActionSequence::default().then(Append(1)));
            activity.set_active(false);
            if returned { activity.set_active(true); }
            app.update();
            assert!(app.world().get::<Values>(root).unwrap().0.is_empty());
            activity.set_active(true);
            app.update();
            dispatch(app.world_mut(), root, ActionSequence::default().then(Append(2)));
            app.update();
            assert_eq!(app.world().get::<Values>(root).unwrap().0, vec![2]);
        }
        let (mut sandbox, root, _) = fixture();
        sandbox.update();
        dispatch(sandbox.world_mut(), root, ActionSequence::default().then(Append(3)));
        sandbox.update();
        assert_eq!(sandbox.world().get::<Values>(root).unwrap().0, vec![3]);
    }

    crate::native::laboratory_cases! {
        buttons_and_dispatch_share_ordered_parameterized_actions,
        shortcuts_match_exact_modifiers_and_ignore_release_repeat_and_unfocused_windows,
        nearest_scope_wins_and_later_bindings_override_defaults,
        despawned_targets_are_skipped_and_queued_followups_wait_for_next_frame,
    }
}
