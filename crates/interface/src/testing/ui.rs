use super::{Backend, Dispatch, Route, Selector};
use crate::native::accessibility;
use bevy::{
    a11y::AccessibilityNode,
    camera::NormalizedRenderTarget,
    input::{
        ButtonState,
        keyboard::{Key, KeyboardInput},
        mouse::MouseButtonInput,
    },
    input_focus::InputFocus,
    picking::{
        input::{PointerInputPlugin, PointerInputSettings},
        pointer::{Location, PointerAction, PointerButton, PointerId, PointerInput},
    },
    prelude::*,
    text::EditableText,
    time::TimeUpdateStrategy,
    window::{CursorMoved, PrimaryWindow, WindowEvent, WindowRef},
};
use std::{collections::VecDeque, time::Duration};

#[derive(Clone)]
pub enum UiAction {
    Click,
    Text(String),
    Key(KeyCode, Key),
}

pub enum UiCondition {
    Visible(Selector),
    Absent(Selector),
    Value(Selector, String),
    Focused(Selector),
    Ready(Selector, fn(&World, Entity) -> bool),
    Check(fn(&World) -> bool),
}

#[derive(Clone)]
struct Target {
    entity: Entity,
    name: String,
    role: String,
    enabled: bool,
    activates_on_press: bool,
    context: accessibility::Context,
}

pub struct UiSnapshot {
    targets: Vec<Target>,
}

enum Input {
    Pointer(Location, PointerAction),
    Keyboard(KeyCode, Key, Option<String>, ButtonState),
}

pub struct UiBackend<'a> {
    app: &'a mut App,
    root: Entity,
    window: Option<Entity>,
    pointer_target: Option<NormalizedRenderTarget>,
    target: Option<Target>,
    pending: VecDeque<Input>,
    held: Vec<(KeyCode, Key)>,
    pointer: Option<Location>,
    mouse_held: bool,
    needs_frame: bool,
    previous_time: Option<TimeUpdateStrategy>,
    diagnostics: String,
}

impl<'a> UiBackend<'a> {
    pub fn new(app: &'a mut App, root: Entity) -> Self {
        let window = app
            .world_mut()
            .query_filtered::<Entity, With<PrimaryWindow>>()
            .iter(app.world())
            .next();
        let previous_time = app.world_mut().remove_resource::<TimeUpdateStrategy>();
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_nanos(
            16_666_667,
        )));
        Self {
            app,
            root,
            window,
            pointer_target: None,
            target: None,
            pending: VecDeque::new(),
            held: Vec::new(),
            pointer: None,
            mouse_held: false,
            needs_frame: false,
            previous_time,
            diagnostics: "snapshots=0".into(),
        }
    }

    pub fn with_window(mut self, window: Entity) -> Self {
        self.window = Some(window);
        self
    }

    pub fn with_pointer_target(mut self, target: NormalizedRenderTarget) -> Self {
        self.pointer_target = Some(target);
        self
    }

    fn scoped(&self, entity: Entity) -> bool {
        let mut current = Some(entity);
        for _ in 0..crate::native::layout::MAX_DEPTH {
            let Some(entity) = current else { return false };
            if entity == self.root {
                return true;
            }
            current = self.app.world().get::<ChildOf>(entity).map(ChildOf::parent);
        }
        false
    }

    fn matching<'s>(
        snapshot: &'s UiSnapshot,
        selector: &Selector,
    ) -> Result<Option<&'s Target>, String> {
        let mut matching = snapshot.targets.iter().filter(|target| {
            target.enabled
                && target.name == selector.name
                && selector
                    .role
                    .as_ref()
                    .is_none_or(|role| *role == target.role)
        });
        let target = matching.next();
        if matching.next().is_some() {
            return Err("The target is ambiguous within this root".into());
        }
        Ok(target)
    }

    fn selected<'s>(snapshot: &'s UiSnapshot, selector: &Selector) -> Result<&'s Target, String> {
        Self::matching(snapshot, selector)?
            .ok_or_else(|| "No visible target matches the exact label and role".into())
    }

    fn target_is_current(&self, target: &Target, ignore_selection_revision: bool) -> bool {
        self.scoped(target.entity)
            && accessibility::context(self.app.world(), target.entity, true).is_some_and(
                |current| {
                    if ignore_selection_revision {
                        target.context.same_input_target(current)
                    } else {
                        current == target.context
                    }
                },
            )
            && self
                .app
                .world()
                .get::<AccessibilityNode>(target.entity)
                .is_some_and(|node| {
                    node.label().unwrap_or_default() == target.name
                        && format!("{:?}", node.role()) == target.role
                })
    }

    fn focused(&self) -> Option<Entity> {
        self.app
            .world()
            .get_resource::<InputFocus>()
            .and_then(InputFocus::get)
    }

    fn pointer_position(world: &World, entity: Entity) -> Option<Vec2> {
        let node = world.get::<ComputedNode>(entity)?;
        let transform = world.get::<UiGlobalTransform>(entity)?;
        let center = transform.to_scale_angle_translation().2;
        let position = center * node.inverse_scale_factor;
        (node.size().is_finite()
            && node.size().min_element() > 0.0
            && position.is_finite()
            && world
                .get::<bevy::ui::CalculatedClip>(entity)
                .is_none_or(|clip| clip.contains_point(center)))
        .then_some(position)
    }

    fn assist(
        &mut self,
        target: &Target,
        action: accesskit::Action,
        data: Option<accesskit::ActionData>,
    ) -> Dispatch {
        let request = accesskit::ActionRequest {
            action,
            target_tree: accesskit::TreeId::ROOT,
            target_node: accesskit::NodeId(target.entity.to_bits()),
            data,
        };
        match accessibility::perform(
            self.app.world_mut(),
            target.entity,
            &target.context,
            &request,
        ) {
            Ok(()) => {
                self.needs_frame = true;
                Dispatch::Accepted
            }
            Err(error) => Dispatch::Rejected(error.into()),
        }
    }

    fn keyboard(&mut self, code: KeyCode, key: Key, text: Option<String>) {
        self.pending.push_back(Input::Keyboard(
            code,
            key.clone(),
            text,
            ButtonState::Pressed,
        ));
        self.pending
            .push_back(Input::Keyboard(code, key, None, ButtonState::Released));
    }

    fn write_keyboard(&mut self, event: KeyboardInput) {
        if self
            .app
            .world()
            .contains_resource::<Messages<WindowEvent>>()
        {
            self.app
                .world_mut()
                .write_message(WindowEvent::KeyboardInput(event.clone()));
        }
        if self
            .app
            .world()
            .contains_resource::<Messages<KeyboardInput>>()
        {
            self.app.world_mut().write_message(event);
        }
    }

    fn write_mouse(&mut self, event: MouseButtonInput) {
        if self
            .app
            .world()
            .contains_resource::<Messages<WindowEvent>>()
        {
            self.app
                .world_mut()
                .write_message(WindowEvent::MouseButtonInput(event.clone()));
        }
        if self
            .app
            .world()
            .contains_resource::<Messages<MouseButtonInput>>()
        {
            self.app.world_mut().write_message(event);
        }
    }

    fn release(&mut self) {
        self.pending.clear();
        self.target = None;
        self.needs_frame = false;
        if let Some(window) = self.window {
            for (key_code, logical_key) in std::mem::take(&mut self.held) {
                self.write_keyboard(KeyboardInput {
                    key_code,
                    logical_key,
                    state: ButtonState::Released,
                    text: None,
                    repeat: false,
                    window,
                });
            }
            if self.mouse_held {
                self.write_mouse(MouseButtonInput {
                    button: MouseButton::Left,
                    state: ButtonState::Released,
                    window,
                });
            }
        }
        self.mouse_held = false;
        if let Some(location) = self.pointer.take() {
            self.app.world_mut().write_message(PointerInput::new(
                PointerId::Mouse,
                location,
                PointerAction::Cancel,
            ));
        }
    }
}

impl Backend for UiBackend<'_> {
    type Snapshot = UiSnapshot;
    type Action = UiAction;
    type Condition = UiCondition;

    fn snapshot(&mut self, _: Duration) -> Result<UiSnapshot, String> {
        if self.app.world().get_entity(self.root).is_err() {
            return Err("The UI root was removed".into());
        }
        let entities = self
            .app
            .world_mut()
            .query::<(Entity, &AccessibilityNode)>()
            .iter(self.app.world())
            .map(|(entity, node)| {
                (
                    entity,
                    node.label().unwrap_or_default().to_string(),
                    format!("{:?}", node.role()),
                )
            })
            .collect::<Vec<_>>();
        let targets = entities
            .into_iter()
            .filter(|(entity, _, _)| self.scoped(*entity))
            .filter_map(|(entity, name, role)| {
                accessibility::context(self.app.world(), entity, false).map(|context| Target {
                    entity,
                    name,
                    role,
                    enabled: accessibility::context(self.app.world(), entity, true).is_some(),
                    activates_on_press: self
                        .app
                        .world()
                        .get::<bevy::ui_widgets::ActivateOnPress>(entity)
                        .is_some(),
                    context,
                })
            })
            .collect::<Vec<_>>();
        self.diagnostics = format!(
            "pending={}; controls={}; enabled={}",
            self.pending.len(),
            targets.len(),
            targets.iter().filter(|target| target.enabled).count()
        );
        Ok(UiSnapshot { targets })
    }

    fn dispatch(
        &mut self,
        snapshot: &UiSnapshot,
        selector: &Selector,
        action: &UiAction,
        route: Route,
        _: Duration,
    ) -> Dispatch {
        if !self.pending.is_empty() {
            return Dispatch::Rejected("Finish the accepted input sequence first".into());
        }
        let target = match Self::selected(snapshot, selector) {
            Ok(target) => target,
            Err(error) => return Dispatch::Rejected(error),
        };
        if !self.target_is_current(target, false) {
            return Dispatch::Rejected(
                "The selected control became stale, hidden or disabled".into(),
            );
        }
        if route == Route::Fallback {
            return match action {
                UiAction::Click => self.assist(target, accesskit::Action::Click, None),
                UiAction::Text(value) => self.assist(
                    target,
                    accesskit::Action::SetValue,
                    Some(accesskit::ActionData::Value(value.clone().into())),
                ),
                UiAction::Key(_, _) => {
                    Dispatch::Unavailable("Keyboard input has no accessibility replay route".into())
                }
            };
        }
        let Some(window) = self.window.filter(|window| {
            self.app
                .world()
                .get::<Window>(*window)
                .is_some_and(|window| window.focused)
        }) else {
            return Dispatch::Unavailable("No focused test window is available".into());
        };
        if self
            .app
            .world()
            .get::<crate::native::actions::WindowActionTarget>(window)
            .is_some_and(|window| window.0 != target.context.root)
        {
            return Dispatch::Rejected("The test window belongs to a different root".into());
        }
        match action {
            UiAction::Click => {
                if self.pointer_target.is_some() && self.app.is_plugin_added::<PointerInputPlugin>()
                {
                    return Dispatch::Unavailable(
                        "An explicit pointer target requires direct virtual pointer input".into(),
                    );
                }
                if self.app.is_plugin_added::<PointerInputPlugin>()
                    && self
                        .app
                        .world()
                        .get_resource::<PointerInputSettings>()
                        .is_some_and(|settings| !settings.is_mouse_enabled)
                {
                    return Dispatch::Unavailable("Mouse input is disabled".into());
                }
                if !self
                    .app
                    .world()
                    .contains_resource::<Messages<PointerInput>>()
                {
                    return Dispatch::Unavailable(
                        "The pointer input pipeline is unavailable".into(),
                    );
                }
                let Some(position) = Self::pointer_position(self.app.world(), target.entity) else {
                    return Dispatch::Unavailable(
                        "The target has no visible laid-out pointer bounds".into(),
                    );
                };
                let location = Location {
                    target: self.pointer_target.clone().unwrap_or_else(|| {
                        NormalizedRenderTarget::Window(
                            WindowRef::Entity(window).normalize(None).unwrap(),
                        )
                    }),
                    position,
                };
                self.pending.push_back(Input::Pointer(
                    location.clone(),
                    PointerAction::Move { delta: Vec2::ZERO },
                ));
                self.pending.push_back(Input::Pointer(
                    location.clone(),
                    PointerAction::Press(PointerButton::Primary),
                ));
                self.pending.push_back(Input::Pointer(
                    location,
                    PointerAction::Release(PointerButton::Primary),
                ));
            }
            UiAction::Text(value) => {
                if !self
                    .app
                    .world()
                    .contains_resource::<Messages<KeyboardInput>>()
                {
                    return Dispatch::Unavailable(
                        "The keyboard input pipeline is unavailable".into(),
                    );
                }
                if self
                    .app
                    .world()
                    .get::<EditableText>(target.entity)
                    .is_none()
                {
                    return Dispatch::Rejected("The target is not an editable text control".into());
                }
                if self.focused() != Some(target.entity) {
                    return Dispatch::Rejected(
                        "Use ordinary input to focus the field first".into(),
                    );
                }
                self.pending.push_back(Input::Keyboard(
                    KeyCode::ControlLeft,
                    Key::Control,
                    None,
                    ButtonState::Pressed,
                ));
                self.keyboard(KeyCode::KeyA, Key::Character("a".into()), None);
                self.pending.push_back(Input::Keyboard(
                    KeyCode::ControlLeft,
                    Key::Control,
                    None,
                    ButtonState::Released,
                ));
                self.keyboard(
                    KeyCode::Unidentified(bevy::input::keyboard::NativeKeyCode::Unidentified),
                    Key::Character(value.clone().into()),
                    Some(value.clone()),
                );
            }
            UiAction::Key(code, key) => {
                if !self
                    .app
                    .world()
                    .contains_resource::<Messages<KeyboardInput>>()
                {
                    return Dispatch::Unavailable(
                        "The keyboard input pipeline is unavailable".into(),
                    );
                }
                if *code == KeyCode::Tab {
                    if self
                        .focused()
                        .is_some_and(|focused| focused != window && !self.scoped(focused))
                    {
                        return Dispatch::Rejected(
                            "Keyboard focus belongs outside the test root".into(),
                        );
                    }
                } else if self.focused() != Some(target.entity) {
                    return Dispatch::Rejected(
                        "Use ordinary input to focus the control first".into(),
                    );
                }
                self.keyboard(*code, key.clone(), None);
            }
        }
        self.target = Some(target.clone());
        Dispatch::Accepted
    }

    fn condition(&self, snapshot: &UiSnapshot, condition: &UiCondition) -> Result<bool, String> {
        if !self.pending.is_empty() || self.needs_frame {
            return Ok(false);
        }
        match condition {
            UiCondition::Check(check) => Ok(check(self.app.world())),
            UiCondition::Visible(selector) => Ok(Self::matching(snapshot, selector)?.is_some()),
            UiCondition::Absent(selector) => Ok(!snapshot.targets.iter().any(|target| {
                target.name == selector.name
                    && selector
                        .role
                        .as_ref()
                        .is_none_or(|role| *role == target.role)
            })),
            UiCondition::Focused(selector) => Ok(Self::matching(snapshot, selector)?
                .is_some_and(|target| self.focused() == Some(target.entity))),
            UiCondition::Ready(selector, check) => Ok(Self::matching(snapshot, selector)?
                .is_some_and(|target| {
                    self.target_is_current(target, false)
                        && Self::pointer_position(self.app.world(), target.entity).is_some()
                        && check(self.app.world(), target.entity)
                })),
            UiCondition::Value(selector, value) => {
                let target = Self::selected(snapshot, selector)?;
                if accessibility::protected_text_input(self.app.world(), target.entity) {
                    return Err(
                        "Protected field values are excluded from driver observations".into(),
                    );
                }
                Ok(self
                    .app
                    .world()
                    .get::<EditableText>(target.entity)
                    .is_some_and(|field| field.value() == value))
            }
        }
    }

    fn pump(&mut self, _: Duration) -> Result<(), String> {
        if let Some(input) = self.pending.pop_front() {
            let window = self.window.ok_or("The test window was removed")?;
            let release_only = match &input {
                Input::Keyboard(_, _, _, ButtonState::Released) => true,
                Input::Pointer(_, PointerAction::Release(_)) => self
                    .target
                    .as_ref()
                    .is_some_and(|target| target.activates_on_press),
                _ => false,
            };
            let keyboard =
                matches!(&input, Input::Keyboard(code, _, _, _) if *code != KeyCode::Tab);
            let current = self
                .app
                .world()
                .get::<Window>(window)
                .is_some_and(|window| window.focused)
                && self.target.as_ref().is_some_and(|target| {
                    let focused = self.focused() == Some(target.entity);
                    self.target_is_current(target, keyboard || self.mouse_held && focused)
                        && (!keyboard || focused)
                        && match &input {
                            Input::Pointer(location, _) => {
                                Self::pointer_position(self.app.world(), target.entity)
                                    == Some(location.position)
                            }
                            Input::Keyboard(KeyCode::Tab, _, _, _) => self
                                .focused()
                                .is_none_or(|focused| focused == window || self.scoped(focused)),
                            Input::Keyboard(..) => true,
                        }
                        && self
                            .app
                            .world()
                            .get::<crate::native::actions::WindowActionTarget>(window)
                            .is_none_or(|owner| owner.0 == target.context.root)
                });
            if !release_only && !current {
                self.release();
                return Err(
                    "The input window, target or keyboard focus changed before dispatch".into(),
                );
            }
            match input {
                Input::Pointer(location, action) => {
                    let forwarded = self.app.is_plugin_added::<PointerInputPlugin>();
                    if let Some(mut window) = self.app.world_mut().get_mut::<Window>(window) {
                        window.set_cursor_position(Some(location.position));
                    }
                    if let PointerAction::Move { delta } = action {
                        let event = CursorMoved {
                            window,
                            position: location.position,
                            delta: Some(delta),
                        };
                        if self
                            .app
                            .world()
                            .contains_resource::<Messages<WindowEvent>>()
                        {
                            self.app
                                .world_mut()
                                .write_message(WindowEvent::CursorMoved(event.clone()));
                        }
                        if self
                            .app
                            .world()
                            .contains_resource::<Messages<CursorMoved>>()
                        {
                            self.app.world_mut().write_message(event);
                        }
                    }
                    if let PointerAction::Press(_) | PointerAction::Release(_) = action {
                        self.mouse_held = matches!(action, PointerAction::Press(_));
                        self.write_mouse(MouseButtonInput {
                            button: MouseButton::Left,
                            state: if self.mouse_held {
                                ButtonState::Pressed
                            } else {
                                ButtonState::Released
                            },
                            window,
                        });
                    }
                    self.pointer = if matches!(action, PointerAction::Release(_)) {
                        None
                    } else {
                        Some(location.clone())
                    };
                    if !forwarded {
                        self.app.world_mut().write_message(PointerInput::new(
                            PointerId::Mouse,
                            location,
                            action,
                        ));
                    }
                }
                Input::Keyboard(key_code, logical_key, text, state) => {
                    if state.is_pressed() {
                        self.held.push((key_code, logical_key.clone()));
                    } else {
                        self.held.retain(|(code, _)| *code != key_code);
                    }
                    self.write_keyboard(KeyboardInput {
                        key_code,
                        logical_key,
                        state,
                        text: text.map(Into::into),
                        repeat: false,
                        window,
                    });
                }
            }
        }
        self.app.update();
        self.needs_frame = false;
        Ok(())
    }

    fn diagnostics(&self) -> String {
        self.diagnostics.clone()
    }

    fn finish(&mut self) {
        self.release();
    }
}

impl Drop for UiBackend<'_> {
    fn drop(&mut self) {
        self.release();
        if let Some(previous) = self.previous_time.take() {
            self.app.insert_resource(previous);
        } else {
            self.app.world_mut().remove_resource::<TimeUpdateStrategy>();
        }
    }
}
