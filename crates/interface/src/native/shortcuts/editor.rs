use super::{
    Settings, Shortcut, configuration,
    model::{Chord, ModKey, Trigger},
};
use crate::native::{
    actions::{Action, ActionButton, KeyBinding, KeyBindings, Modifiers},
    edit_mode::label,
};
use bevy::{
    ecs::message::MessageCursor,
    input::{
        InputSystems,
        keyboard::{KeyboardFocusLost, KeyboardInput},
    },
    input_focus::{FocusCause, InputFocus, InputFocusSystems, tab_navigation::TabGroup},
    prelude::*,
    text::EditableText,
    ui::InteractionDisabled,
};
use std::collections::{BTreeSet, HashSet};

mod pointer;

#[derive(Component)]
pub struct KeyboardModalTrigger;

#[derive(Component)]
struct CaptureTrigger;

pub(crate) fn capture_entry(world: &mut World, entity: Entity) {
    world
        .entity_mut(entity)
        .insert((KeyboardModalTrigger, CaptureTrigger));
}

#[derive(Resource, Default)]
pub(crate) struct Editor {
    dispatch_quarantine: HashSet<KeyCode>,
    modal: Option<Modal>,
    held: HashSet<KeyCode>,
    swallowed: HashSet<KeyCode>,
    focus_lost: MessageCursor<KeyboardFocusLost>,
    pressed_trigger: Option<Entity>,
    buffered: Vec<crate::native::keyboard::PreparedKey>,
    pointer: pointer::CapturePointer,
}

struct Modal {
    overlay: Entity,
    previous: Option<Entity>,
    origin: Settings,
    draft: Settings,
    capture: Option<Capture>,
    filter: Entity,
    observed: String,
    observed_focus: Option<Entity>,
    conflicts_only: bool,
    review: BTreeSet<Shortcut>,
    capture_panel: Entity,
    capture_text: Entity,
    validate: Entity,
    status: Entity,
    modifier_text: Entity,
    save: Entity,
    dirty: bool,
    message: String,
}

struct Capture {
    shortcut: Shortcut,
    value: Option<String>,
    return_to: Entity,
}

#[derive(Component)]
struct Row {
    shortcut: Shortcut,
    binding: Entity,
    text: Entity,
    status: Entity,
}

#[derive(Component)]
struct Summary {
    shortcut: Shortcut,
    root: Entity,
    label: Entity,
    context: Option<crate::native::layout::desktop::CommandContext>,
}

#[derive(Component)]
struct ModifierSummary;

#[derive(Clone, Copy)]
enum Change {
    Open(Option<Shortcut>),
    Capture(Shortcut),
    Validate,
    CancelCapture,
    Unbind(Shortcut),
    Reset(Shortcut),
    ResetAll,
    Modifier,
    Conflicts,
    Save,
    Cancel,
    Reload,
    Rebase,
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct CaptureInput;

pub(crate) struct EditorPlugin;

impl Plugin for EditorPlugin {
    fn build(&self, app: &mut App) {
        configuration::reload::install(app);
        configuration::review::install(app);
        app.init_resource::<Editor>()
            .init_resource::<super::KeyboardModalOwner>()
            .init_resource::<super::widths::WidthEditor>()
            .add_systems(
                PreUpdate,
                capture_input
                    .in_set(CaptureInput)
                    .after(InputSystems)
                    .after(crate::native::keyboard::Prepare)
                    .before(bevy::picking::PickingSystems::ProcessInput)
                    .before(InputFocusSystems::Dispatch)
                    .before(crate::native::inspection::InspectInput),
            )
            .add_systems(
                PostUpdate,
                (refresh, super::widths::refresh)
                    .after(crate::native::actions::ApplyActions)
                    .before(bevy::ui::UiSystems::Layout),
            );
    }
}

pub fn keyboard_owned(world: &World) -> bool {
    world
        .get_resource::<super::KeyboardModalOwner>()
        .and_then(|owner| owner.0)
        .is_some_and(|entity| world.get_entity(entity).is_ok())
}

pub(super) fn quarantine(world: &mut World) {
    let current = world
        .get_resource::<crate::native::keyboard::Dispatch>()
        .map(|context| context.held.clone());
    if let Some(mut editor) = world.get_resource_mut::<Editor>() {
        let held = current
            .as_ref()
            .map(|held| held.iter().copied().collect())
            .unwrap_or_else(|| editor.held.clone());
        if let Some(current) = current {
            editor.dispatch_quarantine.extend(current);
        }
        editor.swallowed.extend(held);
    }
}

pub(super) fn dispatch_input(world: &mut World, event: &KeyboardInput) -> bool {
    let Some(mut editor) = world.get_resource_mut::<Editor>() else {
        return false;
    };
    if editor.dispatch_quarantine.is_empty() {
        return false;
    }
    if event.state.is_pressed() {
        editor.dispatch_quarantine.insert(event.key_code);
        editor.swallowed.insert(event.key_code);
    } else {
        editor.dispatch_quarantine.remove(&event.key_code);
        editor.swallowed.remove(&event.key_code);
    }
    true
}

pub fn keyboard_capturing(world: &World) -> bool {
    world.get_resource::<Editor>().is_some_and(|editor| {
        editor
            .modal
            .as_ref()
            .is_some_and(|modal| modal.capture.is_some())
    })
}

pub(super) fn edit(world: &mut World, root: Entity, shortcut: Shortcut) {
    Change::Open(Some(shortcut)).apply(world, root);
}

fn capture_trigger(world: &mut World) -> Option<Entity> {
    use bevy::window::PrimaryWindow;
    let (window, position) = world
        .query_filtered::<(Entity, &Window), With<PrimaryWindow>>()
        .iter(world)
        .find_map(|(entity, window)| Some((entity, window.cursor_position()?)))?;
    for entity in crate::native::pointer_hit::nodes(world, window, position) {
        let mut ancestor = Some(entity);
        for _ in 0..crate::native::layout::MAX_DEPTH {
            let Some(entity) = ancestor else {
                break;
            };
            if world.get::<InteractionDisabled>(entity).is_some() {
                return None;
            }
            if world.get::<CaptureTrigger>(entity).is_some() {
                return Some(entity);
            }
            ancestor = world.get::<ChildOf>(entity).map(ChildOf::parent);
        }
        if world
            .get::<Pickable>(entity)
            .is_none_or(|pickable| pickable.should_block_lower)
        {
            return None;
        }
    }
    None
}

fn capture_input(world: &mut World) {
    let mut editor = world.remove_resource::<Editor>().unwrap_or_default();
    editor.dispatch_quarantine.clear();
    if let Some(events) = world.get_resource::<Messages<KeyboardFocusLost>>()
        && editor.focus_lost.read(events).count() > 0
    {
        editor.held.clear();
        editor.swallowed.clear();
        editor.buffered.clear();
        editor.pointer.clear();
        editor.pressed_trigger = None;
        if let Some(modal) = &mut editor.modal
            && modal.capture.take().is_some()
        {
            modal.message = "Capture cancelled because keyboard focus left this window.".into();
            modal.dirty = true;
        }
    }
    let capturing = editor
        .modal
        .as_ref()
        .is_some_and(|modal| modal.capture.is_some());
    if capturing {
        editor.swallowed.extend(editor.held.iter().copied());
    }
    let events = crate::native::keyboard::take(world);
    let (press, release) = world
        .get_resource::<ButtonInput<MouseButton>>()
        .map(|buttons| {
            (
                buttons.just_pressed(MouseButton::Left),
                buttons.just_released(MouseButton::Left),
            )
        })
        .unwrap_or_default();
    if press {
        editor.pressed_trigger = capture_trigger(world);
    }
    let armed = release
        && editor
            .pressed_trigger
            .take()
            .is_some_and(|trigger| capture_trigger(world) == Some(trigger));
    if armed && !events.is_empty() {
        editor.buffered.extend(events);
        if let Some(mut keys) = world.get_resource_mut::<ButtonInput<KeyCode>>() {
            keys.reset_all();
        }
        if let Some(mut events) = world.get_resource_mut::<Messages<KeyboardInput>>() {
            events.clear();
        }
        world.insert_resource(editor);
        return;
    }
    let buffered = std::mem::take(&mut editor.buffered);
    let buffered_count = buffered.len();
    let mut intercepted = false;
    let mut retained = Vec::new();
    let events = buffered.into_iter().chain(events);
    for (index, prepared) in events.enumerate() {
        let event = &prepared.input;
        let quarantined = !editor.swallowed.is_empty();
        let mut transition = None;
        if event.state.is_pressed() {
            editor.held.insert(event.key_code);
        } else {
            editor.held.remove(&event.key_code);
        }
        let owns = editor
            .modal
            .as_ref()
            .is_some_and(|modal| modal.capture.is_some());
        let escape = crate::native::actions::native_key_matches(
            event.key_code,
            &event.logical_key,
            KeyCode::Escape,
        );
        let enter = crate::native::actions::native_key_matches(
            event.key_code,
            &event.logical_key,
            KeyCode::Enter,
        );
        if !owns && event.state.is_pressed() && !event.repeat && escape {
            let composing = world
                .get_resource::<InputFocus>()
                .and_then(|focus| focus.get())
                .and_then(|entity| world.get::<EditableText>(entity))
                .is_some_and(EditableText::is_composing);
            if prepared.modifiers == Modifiers::NONE
                && !composing
                && let Some(owner) = world
                    .get_resource::<super::KeyboardModalOwner>()
                    .and_then(|owner| owner.0)
                && let Some(binding) = world.get::<KeyBindings>(owner).and_then(|bindings| {
                    bindings.0.iter().rev().find(|binding| {
                        binding.key == KeyCode::Escape && binding.modifiers == Modifiers::NONE
                    })
                })
            {
                transition = Some((owner, binding.actions.clone()));
            }
        }
        let blocked =
            owns || quarantined || index < buffered_count && !capturing || transition.is_some();
        if blocked {
            intercepted = true;
            if event.state.is_pressed() {
                editor.swallowed.insert(event.key_code);
            } else {
                editor.swallowed.remove(&event.key_code);
            }
            if let Some(mut keys) = world.get_resource_mut::<ButtonInput<KeyCode>>() {
                keys.reset(event.key_code);
            }
        } else {
            retained.push(prepared);
            continue;
        }
        if owns && event.state.is_pressed() && !event.repeat && !prepared.modifier {
            let modifiers = prepared.modifiers;
            let modal = editor.modal.as_mut().unwrap();
            if escape && modifiers == Modifiers::NONE {
                let capture = modal.capture.take().unwrap();
                restore_focus(world, Some(capture.return_to));
                modal.message = "Capture cancelled; the draft is unchanged.".into();
            } else if enter
                && modifiers == Modifiers::NONE
                && modal
                    .capture
                    .as_ref()
                    .is_some_and(|capture| capture.value.is_some())
            {
                transition = Some((modal.overlay, crate::native::actions![Change::Validate]));
            } else {
                let value = Chord::from_input(event.key_code, modifiers)
                    .map(|chord| modal.draft.captured(chord));
                captured(modal, value);
            }
            modal.dirty = true;
        }
        if let Some((target, actions)) = transition {
            retained.clear();
            world.insert_resource(editor);
            actions.run(world, target);
            editor = world.remove_resource::<Editor>().unwrap_or_default();
        }
    }
    if let Some(mut keys) = world.get_resource_mut::<ButtonInput<KeyCode>>() {
        for key in &editor.swallowed {
            keys.reset(*key);
        }
    }
    if intercepted && let Some(mut events) = world.get_resource_mut::<Messages<KeyboardInput>>() {
        events.clear();
    }
    crate::native::keyboard::replace(world, retained);
    pointer::capture(world, &mut editor);
    world.insert_resource(editor);
}

fn captured(modal: &mut Modal, result: Result<String, String>) {
    let capture = modal.capture.as_mut().unwrap();
    let result = result.and_then(|value| {
        let mut candidate = modal.draft.clone();
        candidate.stage(capture.shortcut, Some(&value))?;
        let sharing = candidate
            .conflicts()
            .into_iter()
            .find(|(_, commands)| commands.contains(&capture.shortcut))
            .map(|(_, commands)| {
                commands
                    .into_iter()
                    .filter(|command| *command != capture.shortcut)
                    .map(|command| command.label())
                    .collect::<Vec<_>>()
                    .join(", ")
            });
        Ok((value, sharing))
    });
    match result {
        Ok((value, sharing)) => {
            capture.value = Some(value);
            modal.message = sharing.map_or_else(|| "Review the combination, then Validate (Enter) or Cancel (Escape).".into(),
                |sharing| format!("Combination also used by: {sharing}. Validate to resolve these together in the draft."));
        }
        Err(error) => {
            capture.value = None;
            modal.message = error;
        }
    }
    modal.dirty = true;
}

fn restore_focus(world: &mut World, target: Option<Entity>) {
    let target = target.filter(|entity| world.get_entity(*entity).is_ok());
    if let Some(target) = target {
        world
            .resource_mut::<InputFocus>()
            .set(target, FocusCause::Navigated);
    } else {
        world.resource_mut::<InputFocus>().clear();
    }
}

fn button(world: &mut World, parent: Entity, action: Change, value: &str) -> (Entity, Entity) {
    let entity = world
        .spawn((
            crate::native::sand::button(0),
            KeyboardModalTrigger,
            Node {
                padding: UiRect::axes(px(9), px(6)),
                flex_shrink: 0.0,
                min_width: px(0),
                max_width: percent(100),
                ..default()
            },
            ChildOf(parent),
        ))
        .id();
    world
        .entity_mut(entity)
        .insert(ActionButton::new(entity, crate::native::actions![action]));
    if matches!(action, Change::Open(Some(_)) | Change::Capture(_)) {
        capture_entry(world, entity);
    }
    world
        .get_mut::<bevy::a11y::AccessibilityNode>(entity)
        .unwrap()
        .set_label(value);
    let text = label(world, entity, value, 14.0);
    world.entity_mut(text).insert((TextLayout::linebreak(bevy::text::LineBreak::WordOrCharacter), Pickable::IGNORE));
    (entity, text)
}

fn column(world: &mut World, parent: Entity) -> Entity {
    world
        .spawn((
            Node {
                width: percent(100),
                flex_direction: FlexDirection::Column,
                row_gap: px(6),
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(parent),
        ))
        .id()
}

fn row(world: &mut World, parent: Entity) -> Entity {
    world
        .spawn((
            Node {
                column_gap: px(6),
                row_gap: px(4),
                flex_wrap: FlexWrap::Wrap,
                flex_shrink: 0.0,
                align_items: AlignItems::Center,
                ..default()
            },
            ChildOf(parent),
        ))
        .id()
}

fn scroll_column(world: &mut World, parent: Entity, node: Node) -> Entity {
    world
        .spawn((
            node,
            ScrollPosition::default(),
            crate::native::edit_mode::FocusScroll,
            ChildOf(parent),
        ))
        .observe(
            |mut event: On<Pointer<Scroll>>, mut positions: Query<&mut ScrollPosition>| {
                if let Ok(mut position) = positions.get_mut(event.entity) {
                    let multiplier = if event.unit == bevy::input::mouse::MouseScrollUnit::Line {
                        24.0
                    } else {
                        1.0
                    };
                    position.0.y -= event.y * multiplier;
                    event.propagate(false);
                }
            },
        )
        .id()
}

pub(crate) fn panel(world: &mut World, parent: Entity) {
    configuration::reload::panel(world, parent);
    let title = label(world, parent, crate::i18n::Token::Shortcuts.text(), 22.0);
    world.entity_mut(title).insert(Name::new("shortcuts.title"));
    label(
        world,
        parent,
        "Click a binding and press its combination. Review conflicts and save all changes together.",
        14.0,
    );
    label(
        world,
        parent,
        "Hover a command for its current scope and availability. You can edit a binding even when its command is unavailable.",
        14.0,
    );
    let modifier = world.resource::<Settings>().modifier();
    let modifier_summary = label(
        world,
        parent,
        &format!(
            "Mod is {}. Bindings are shared across workspaces.",
            modifier.label()
        ),
        14.0,
    );
    world.entity_mut(modifier_summary).insert(ModifierSummary);
    if let Some(error) = world
        .get_resource::<configuration::Storage>()
        .and_then(|storage| storage.error.clone())
    {
        label(world, parent, &error, 14.0);
    }
    button(world, parent, Change::Open(None), "Edit all shortcuts");
    let root = crate::native::layout::desktop::root(world, parent).unwrap_or(parent);
    for shortcut in Shortcut::ALL {
        let value = world.resource::<Settings>().text(shortcut).to_string();
        let (button, label) = button(
            world,
            parent,
            Change::Open(Some(shortcut)),
            &format!("{} · {value}", shortcut.label()),
        );
        world.entity_mut(button).insert((
            Name::new(format!("shortcut.summary.{}", shortcut.id())),
            Summary {
                shortcut,
                root,
                label,
                context: None,
            },
        ));
    }
    label(world, parent, "Interaction reference", 22.0);
    for (group, shortcuts) in super::GROUPS {
        label(world, parent, group, 18.0);
        for (keys, description) in *shortcuts {
            label(world, parent, &format!("{keys}\n{description}"), 14.0);
        }
    }
}

fn open(world: &mut World, target: Entity, shortcut: Option<Shortcut>) {
    if keyboard_owned(world) {
        return;
    }
    let mut root = target;
    while let Some(parent) = world.get::<ChildOf>(root) {
        root = parent.parent();
        if world
            .get::<crate::native::workspace::Workspaces>(root)
            .is_some()
        {
            break;
        }
    }
    let previous = world.resource::<InputFocus>().get().or(Some(target));
    let overlay = world
        .spawn((
            crate::native::inspection::InspectionExcluded,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)),
            GlobalZIndex(100),
            TabGroup::modal(),
            ChildOf(root),
            KeyBindings(vec![KeyBinding::new(
                KeyCode::Escape,
                Modifiers::NONE,
                crate::native::actions![Change::Cancel],
            )]),
        ))
        .id();
    let frame = column(world, overlay);
    world.entity_mut(frame).insert((
        Node {
            width: px(760),
            max_width: percent(96),
            height: percent(90),
            padding: UiRect::all(px(16)),
            row_gap: px(10),
            flex_direction: FlexDirection::Column,
            ..default()
        },
        crate::native::token_style::background(crate::native::tokens::Token::Surface),
    ));
    let panel = scroll_column(
        world,
        frame,
        Node {
            width: percent(100),
            min_height: px(0),
            flex_direction: FlexDirection::Column,
            row_gap: px(10),
            flex_grow: 1.0,
            flex_shrink: 1.0,
            overflow: Overflow::scroll_y(),
            ..default()
        },
    );
    world.entity_mut(panel).insert(Name::new("shortcuts.body"));
    label(world, panel, "Shortcuts", 22.0);
    label(
        world,
        panel,
        "Changes stay in this draft until Save. These scopes can overlap; duplicate combinations must be resolved. Text editing, Tab and Escape remain reserved in native controls.",
        14.0,
    );
    let options = row(world, panel);
    let (_, modifier_text) = button(world, options, Change::Modifier, "Mod: Super");
    button(world, options, Change::Conflicts, "All / affected commands");
    button(
        world,
        options,
        Change::ResetAll,
        "Restore all defaults in draft",
    );
    label(world, panel, "Search commands or key combinations", 14.0);
    let filter = world
        .spawn((
            crate::native::sand::single_line_editor(
                "",
                world.resource::<crate::native::theme::Typography>(),
                0,
                120,
            ),
            ChildOf(panel),
            bevy::a11y::AccessibilityNode::default(),
        ))
        .id();
    world
        .get_mut::<bevy::a11y::AccessibilityNode>(filter)
        .unwrap()
        .set_label("Search command names, scopes or key combinations");
    world.get_mut::<Node>(filter).unwrap().width = percent(100);
    world
        .get_mut::<EditableText>(filter)
        .unwrap()
        .max_characters = Some(120);
    let capture_panel = column(world, panel);
    let capture_text = label(world, capture_panel, "", 16.0);
    let capture_controls = row(world, capture_panel);
    let (validate, _) = button(
        world,
        capture_controls,
        Change::Validate,
        "Validate combination",
    );
    button(
        world,
        capture_controls,
        Change::CancelCapture,
        "Cancel capture",
    );
    let status = label(world, panel, "", 14.0);
    let mut node = world.get_mut::<Node>(status).unwrap();
    node.width = percent(100);
    node.min_width = px(0);
    world.entity_mut(status).insert(TextLayout::linebreak(bevy::text::LineBreak::WordOrCharacter));
    let list = scroll_column(
        world,
        panel,
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: px(8),
            flex_grow: 1.0,
            min_height: px(96),
            overflow: Overflow::scroll_y(),
            ..default()
        },
    );
    let mut capture = None;
    for command in Shortcut::ALL {
        let entry = column(world, list);
        world.entity_mut(entry).insert(Name::new(command.id()));
        label(
            world,
            entry,
            &format!("{} · {}", command.label(), command.scope().label()),
            16.0,
        );
        let controls = row(world, entry);
        let (binding, text) = button(
            world,
            controls,
            Change::Capture(command),
            "Press combination",
        );
        world
            .entity_mut(binding)
            .insert(Name::new("shortcut.capture"));
        button(world, controls, Change::Unbind(command), "Unbind");
        button(world, controls, Change::Reset(command), "Restore default");
        let status = label(world, entry, "", 14.0);
        world.entity_mut(entry).insert(Row {
            shortcut: command,
            binding,
            text,
            status,
        });
        if shortcut == Some(command) {
            capture = Some(Capture {
                shortcut: command,
                value: None,
                return_to: binding,
            });
        }
    }
    let footer = row(world, frame);
    world
        .entity_mut(footer)
        .insert(Name::new("shortcuts.footer"));
    let (save, _) = button(world, footer, Change::Save, "Save all changes");
    button(world, footer, Change::Reload, "Reload active bindings");
    button(world, footer, Change::Rebase, "Review draft against file");
    button(world, footer, Change::Cancel, "Cancel draft");
    let origin = world.resource::<Settings>().clone();
    let focus = capture.as_ref().map_or(filter, |capture| capture.return_to);
    let mut editor = world.resource_mut::<Editor>();
    if capture.is_some() {
        let held = editor.held.clone();
        editor.swallowed.extend(held);
    }
    editor.modal = Some(Modal {
        overlay,
        previous,
        draft: origin.clone(),
        origin,
        capture,
        filter,
        observed: "\0".into(),
        observed_focus: None,
        conflicts_only: false,
        review: BTreeSet::new(),
        capture_panel,
        capture_text,
        validate,
        status,
        modifier_text,
        save,
        dirty: true,
        message: "".into(),
    });
    drop(editor);
    super::own_keyboard(world, overlay);
    restore_focus(world, Some(focus));
    refresh(world);
}

impl Action for Change {
    fn apply(&self, world: &mut World, target: Entity) {
        if let Self::Open(shortcut) = self {
            open(world, target, *shortcut);
            return;
        }
        if let Some(owner) = world.get_resource::<super::KeyboardModalOwner>().and_then(|owner| owner.0)
            && world.get_resource::<Editor>().and_then(|editor| editor.modal.as_ref()).is_some_and(|modal| modal.overlay != owner) { return; }
        let Some(mut modal) = world.resource_mut::<Editor>().modal.take() else {
            return;
        };
        let mut close = false;
        let mut rebase = false;
        match *self {
            Self::Capture(shortcut) => {
                modal.capture = Some(Capture {
                    shortcut,
                    value: None,
                    return_to: target,
                });
                modal.message = "Press the combination. Escape cancels capture.".into();
                let mut editor = world.resource_mut::<Editor>();
                let held = editor.held.clone();
                editor.swallowed.extend(held);
            }
            Self::Validate => {
                if let Some(capture) = modal.capture.take() {
                    if let Some(value) = &capture.value {
                        match modal.draft.stage(capture.shortcut, Some(value)) {
                            Ok(()) => {
                                modal.conflicts_only |= !modal.draft.conflicts().is_empty();
                                modal.message = "Combination added to draft. Review all affected commands before Save.".into();
                                restore_focus(world, Some(capture.return_to));
                            }
                            Err(error) => {
                                modal.message = error;
                                modal.capture = Some(capture);
                            }
                        }
                    } else {
                        modal.capture = Some(capture);
                    }
                }
            }
            Self::CancelCapture => {
                if let Some(capture) = modal.capture.take() {
                    restore_focus(world, Some(capture.return_to));
                }
                modal.message = "Capture cancelled; the draft is unchanged.".into();
            }
            Self::Unbind(shortcut) => {
                modal.draft.stage(shortcut, None).unwrap();
                modal.message = "Binding removed in draft.".into();
            }
            Self::Reset(shortcut) => {
                modal.draft.stage_reset(shortcut);
                modal.message = "Default restored in draft.".into();
            }
            Self::ResetAll => {
                let modifier = modal.draft.modifier();
                modal.draft = Settings::default();
                modal.draft.stage_modifier(modifier).unwrap();
                modal.message =
                    "All command defaults restored in draft. Review changes before Save.".into();
            }
            Self::Modifier => {
                let modifier = if modal.draft.modifier() == ModKey::Super {
                    ModKey::Alt
                } else {
                    ModKey::Super
                };
                match modal.draft.stage_modifier(modifier) {
                    Ok(()) => {
                        modal.message =
                            "Mod changed in draft. Review any overlapping combinations.".into()
                    }
                    Err(error) => modal.message = error,
                }
            }
            Self::Conflicts => modal.conflicts_only = !modal.conflicts_only,
            Self::Reload => {
                if modal.capture.is_some() { modal.message = "Validate or cancel the captured combination before reloading.".into(); }
                else {
                    modal.origin = world.resource::<Settings>().clone();
                    modal.draft = modal.origin.clone();
                    modal.review.clear();
                    modal.message = "Active bindings loaded. Unsaved binding changes were discarded.".into();
                }
            }
            Self::Rebase => {
                if modal.capture.is_some() { modal.message = "Validate or cancel the captured combination before reviewing this draft.".into(); }
                else { rebase = true; }
            }
            Self::Save => {
                if modal.capture.is_some() {
                    modal.message = "Validate or cancel the captured combination first.".into();
                } else if world.resource::<Settings>() != &modal.origin {
                    modal.message =
                        "Active bindings changed. Reload active bindings or Review the draft against the file before saving."
                            .into();
                } else {
                    let layout = world
                        .get_resource::<configuration::LayoutSettings>()
                        .cloned()
                        .unwrap_or_default();
                    match configuration::commit(world, modal.draft.clone(), layout) {
                        Ok(message) => {
                            crate::native::notifications::report(world, "Shortcuts", &message);
                            close = true;
                        }
                        Err(error) => modal.message = error,
                    }
                }
            }
            Self::Cancel => close = true,
            Self::Open(_) => {}
        }
        modal.dirty = true;
        if close {
            super::release_keyboard(world, modal.overlay);
            world.despawn(modal.overlay);
            restore_focus(world, modal.previous);
        } else {
            let overlay = modal.overlay;
            world.resource_mut::<Editor>().modal = Some(modal);
            if rebase && let Err(error) = configuration::review::open(world, overlay, "command bindings", review_snapshot, review_apply) {
                world.resource_mut::<Editor>().modal.as_mut().unwrap().message = error;
            }
        }
        refresh(world);
    }
}

fn review_snapshot(world: &World, owner: Entity) -> Result<configuration::review::Snapshot, String> {
    let modal = world.get_resource::<Editor>().and_then(|editor| editor.modal.as_ref()).filter(|modal| modal.overlay == owner).ok_or("The binding editor is unavailable.")?;
    if modal.capture.is_some() { return Err("Validate or cancel the captured combination before reviewing this draft.".into()); }
    configuration::review::Snapshot::new(&modal.origin, &modal.draft, world.resource::<Settings>())
}

fn review_apply(world: &mut World, owner: Entity, active: serde_json::Value, draft: serde_json::Value) -> Result<(), String> {
    let origin: Settings = serde_json::from_value(active).map_err(|error| error.to_string())?;
    let draft: Settings = serde_json::from_value(draft).map_err(|error| error.to_string())?;
    let mut editor = world.resource_mut::<Editor>();
    let modal = editor.modal.as_mut().filter(|modal| modal.overlay == owner).ok_or("The binding editor is unavailable.")?;
    if modal.capture.is_some() { return Err("The binding capture changed. Cancel or validate it before reviewing.".into()); }
    modal.origin = origin;
    modal.draft = draft;
    modal.review.clear();
    modal.dirty = true;
    modal.message = "Review applied to the draft. Resolve any conflicting combinations, then Save all changes.".into();
    Ok(())
}

fn set_text(world: &mut World, entity: Entity, value: String) {
    if let Some(mut text) = world.get_mut::<Text>(entity)
        && text.0 != value
    {
        text.0 = value.clone();
    }
    if let Some(parent) = world.get::<ChildOf>(entity).map(ChildOf::parent)
        && world.get::<ActionButton>(parent).is_some()
        && let Some(mut node) = world.get_mut::<bevy::a11y::AccessibilityNode>(parent)
    {
        node.set_label(value);
    }
}

fn disabled(world: &mut World, entity: Entity, disabled: bool) {
    if disabled {
        world.entity_mut(entity).insert(InteractionDisabled);
    } else {
        world.entity_mut(entity).remove::<InteractionDisabled>();
    }
}

fn refresh(world: &mut World) {
    let settings_changed = world.is_resource_changed::<Settings>();
    let keyboard_changed = world.is_resource_changed::<crate::host::input::KeyboardSnapshot>();
    if settings_changed {
        let modifier = world.resource::<Settings>().modifier();
        let summaries: Vec<_> = world
            .query_filtered::<Entity, With<ModifierSummary>>()
            .iter(world)
            .collect();
        for summary in summaries {
            set_text(
                world,
                summary,
                format!(
                    "Mod is {}. Bindings are shared across workspaces.",
                    modifier.label()
                ),
            );
        }
    }
    let summaries: Vec<_> = world
        .query::<(Entity, &Summary)>()
        .iter(world)
        .map(|(entity, summary)| {
            (
                entity,
                summary.shortcut,
                summary.root,
                summary.label,
                summary.context,
            )
        })
        .collect();
    let mut contexts = std::collections::HashMap::new();
    for (entity, shortcut, root, label, previous) in summaries {
        let context = *contexts
            .entry(root)
            .or_insert_with(|| crate::native::layout::desktop::CommandContext::new(world, root));
        if !settings_changed && !keyboard_changed && previous == Some(context) {
            continue;
        }
        let settings = world.resource::<Settings>();
        let keyboard = world.get_resource::<crate::host::input::KeyboardSnapshot>();
        let value = shortcut.caption(settings, keyboard);
        let help = shortcut.help(settings, context, keyboard);
        set_text(world, label, value);
        if world
            .get::<crate::native::icons::Tooltip>(entity)
            .is_none_or(|tooltip| tooltip.0 != help)
        {
            world
                .entity_mut(entity)
                .insert(crate::native::icons::Tooltip(help.clone()));
        }
        if let Some(mut node) = world.get_mut::<bevy::a11y::AccessibilityNode>(entity) {
            node.set_label(help.as_str());
        }
        world.get_mut::<Summary>(entity).unwrap().context = Some(context);
    }
    let Some(mut modal) = world.resource_mut::<Editor>().modal.take() else {
        return;
    };
    if world.get_entity(modal.overlay).is_err() {
        super::release_keyboard(world, modal.overlay);
        return;
    }
    let filter = world
        .get::<EditableText>(modal.filter)
        .map(|text| text.value().to_string())
        .unwrap_or_default()
        .to_lowercase();
    let focus = world.resource::<InputFocus>().get();
    if !modal.dirty
        && !keyboard_changed
        && !settings_changed
        && filter == modal.observed
        && focus == modal.observed_focus
    {
        world.resource_mut::<Editor>().modal = Some(modal);
        return;
    }
    modal.observed = filter.clone();
    modal.observed_focus = focus;
    modal.dirty = false;
    let conflicts = modal.draft.conflicts();
    modal.review.extend(
        conflicts
            .iter()
            .flat_map(|(_, commands)| commands.iter().copied()),
    );
    modal
        .review
        .extend(Shortcut::ALL.into_iter().filter(|shortcut| {
            modal.draft.text(*shortcut) != modal.origin.text(*shortcut)
                || modal.draft.chord(*shortcut) != modal.origin.chord(*shortcut)
        }));
    let changed = Shortcut::ALL
        .into_iter()
        .filter(|shortcut| {
            modal.draft.text(*shortcut) != modal.origin.text(*shortcut)
                || modal.draft.chord(*shortcut) != modal.origin.chord(*shortcut)
        })
        .count();
    let count: usize = conflicts.iter().map(|(_, commands)| commands.len()).sum();
    let stale = world.resource::<Settings>() != &modal.origin;
    let validation = modal.draft.validate().err();
    let warning = if stale { "Active bindings changed. Reload active bindings or Review the draft against the file.\n".to_owned() }
        else { validation.as_ref().map(|error| format!("{error}\n")).unwrap_or_default() };
    set_text(
        world,
        modal.status,
        format!(
            "{warning}{}\n{changed} changed commands · {count} commands with conflicting bindings{}",
            modal.message,
            if modal.conflicts_only {
                " · showing affected commands"
            } else {
                ""
            }
        ),
    );
    set_text(
        world,
        modal.modifier_text,
        format!("Mod: {}", modal.draft.modifier().label()),
    );
    world.get_mut::<Node>(modal.capture_panel).unwrap().display = if modal.capture.is_some() {
        Display::Flex
    } else {
        Display::None
    };
    disabled(
        world,
        modal.validate,
        modal
            .capture
            .as_ref()
            .is_none_or(|capture| capture.value.is_none()),
    );
    disabled(
        world,
        modal.save,
        stale || validation.is_some() || !conflicts.is_empty() || modal.capture.is_some(),
    );
    if let Some(capture) = &modal.capture {
        set_text(
            world,
            modal.capture_text,
            format!(
                "{}: {}",
                capture.shortcut.label(),
                capture.value.as_deref().unwrap_or(
                    if capture.shortcut.pointer_action().is_some() {
                        "hold a modifier and press a mouse button…"
                    } else {
                        "press a key combination…"
                    }
                )
            ),
        );
    }
    let rows: Vec<_> = world
        .query::<(Entity, &Row)>()
        .iter(world)
        .map(|(entity, row)| (entity, row.shortcut, row.binding, row.text, row.status))
        .collect();
    for (entity, shortcut, binding, text, status) in rows {
        let chord = modal.draft.text(shortcut);
        let display = modal.draft.display_text(
            shortcut,
            world.get_resource::<crate::host::input::KeyboardSnapshot>(),
        );
        let sharing = conflicts
            .iter()
            .find(|(_, commands)| commands.contains(&shortcut));
        let mut focused_row = focus;
        let mut keep_focus = false;
        for _ in 0..crate::native::layout::MAX_DEPTH {
            let Some(focused) = focused_row else {
                break;
            };
            if focused == entity {
                keep_focus = true;
                break;
            }
            focused_row = world.get::<ChildOf>(focused).map(ChildOf::parent);
        }
        let visible = keep_focus
            || (!modal.conflicts_only || sharing.is_some() || modal.review.contains(&shortcut))
                && format!(
                    "{} {} {} {}",
                    shortcut.label(),
                    shortcut.id(),
                    shortcut.scope().label(),
                    display
                )
                .to_lowercase()
                .contains(&filter);
        world.get_mut::<Node>(entity).unwrap().display = if visible {
            Display::Flex
        } else {
            Display::None
        };
        set_text(world, text, display.clone());
        world
            .get_mut::<bevy::a11y::AccessibilityNode>(binding)
            .unwrap()
            .set_label(format!("Change {}: {display}", shortcut.label()));
        let message = if let Some((_, commands)) = sharing {
            format!(
                "Conflicts with: {}",
                commands
                    .iter()
                    .filter(|command| **command != shortcut)
                    .map(|command| command.label())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        } else if modal.origin.text(shortcut) != chord
            || modal.origin.trigger(shortcut) != modal.draft.trigger(shortcut)
        {
            format!(
                "Was {} · changed in draft",
                modal
                    .origin
                    .trigger(shortcut)
                    .map_or_else(|| "Unbound".into(), Trigger::text)
            )
        } else {
            String::new()
        };
        set_text(world, status, message);
    }
    world.resource_mut::<Editor>().modal = Some(modal);
}

#[cfg(test)]
mod tests;
