use bevy::prelude::*;

mod commands;
mod pointer;
pub use pointer::{PointerBinding, PointerChord};
pub(crate) mod configuration;
mod editor;
mod widths;
pub(crate) use editor::capture_entry;
pub(crate) use editor::{CaptureInput, EditorPlugin, panel};
pub use editor::{KeyboardModalTrigger, keyboard_capturing, keyboard_owned};
pub(crate) use widths::panel as width_settings;
pub(crate) use widths::ratio as size_ratio;
mod model;
pub(crate) mod tests;
pub(crate) use model::Scope;
pub use model::{Settings, Shortcut};

#[derive(Component)]
pub(crate) struct ConfiguredBindings(pub Vec<crate::native::actions::KeyBinding>);

#[derive(Resource, Default)]
struct DesktopKeys {
    consumed: std::collections::HashMap<(Entity, KeyCode), Option<crate::host::DesktopAction>>,
    forwarded: bevy::ecs::message::Messages<bevy::input::keyboard::KeyboardInput>,
    dispatcher: Option<bevy::ecs::system::SystemId>,
}

fn scoped_binding(
    world: &World,
    root: Entity,
    key: KeyCode,
    modifiers: crate::native::actions::Modifiers,
) -> bool {
    let mut cursor = world
        .get_resource::<bevy::input_focus::InputFocus>()
        .and_then(|focus| focus.get())
        .or(Some(root));
    for _ in 0..crate::native::layout::MAX_DEPTH {
        let Some(entity) = cursor else { break };
        if world
            .get::<crate::native::actions::KeyBindings>(entity)
            .is_some_and(|bindings| {
                bindings
                    .0
                    .iter()
                    .any(|binding| binding.key == key && binding.modifiers == modifiers)
            })
        {
            return true;
        }
        if entity == root {
            return false;
        }
        cursor = world
            .get::<ChildOf>(entity)
            .map(ChildOf::parent)
            .or(Some(root));
    }
    false
}

pub(crate) fn desktop_input(world: &mut World) {
    use crate::native::actions::Modifiers;
    use bevy::{
        ecs::message::Messages, input::keyboard::KeyboardInput, input_focus::dispatch_focused_input,
    };
    world.init_resource::<DesktopKeys>();
    let mut state = world.remove_resource::<DesktopKeys>().unwrap();
    let input = crate::native::keyboard::take(world);
    let Some(mut original) = world.remove_resource::<Messages<KeyboardInput>>() else {
        world.insert_resource(state);
        return;
    };
    original.clear();
    if state.dispatcher.is_none() {
        state.dispatcher = Some(world.register_system(dispatch_focused_input::<KeyboardInput>));
    }
    state.forwarded.update();
    world.insert_resource(std::mem::take(&mut state.forwarded));
    for prepared in input {
        world.insert_resource(crate::native::keyboard::Dispatch {
            window: prepared.input.window,
            modifiers: prepared.modifiers,
            held: prepared.held.clone(),
        });
        dispatch_key(world, &mut state, prepared);
        world.remove_resource::<crate::native::keyboard::Dispatch>();
    }
    if let Some(keys) = world.get_resource::<ButtonInput<KeyCode>>() {
        state
            .consumed
            .retain(|(window, key), _| world.get_entity(*window).is_ok() && keys.pressed(*key));
    }
    state.forwarded = world.remove_resource::<Messages<KeyboardInput>>().unwrap();
    world.insert_resource(original);
    crate::native::keyboard::logical(
        world,
        crate::native::keyboard::current(world).unwrap_or(Modifiers::NONE),
    );
    world.insert_resource(state);
}

fn dispatch_key(
    world: &mut World,
    state: &mut DesktopKeys,
    prepared: crate::native::keyboard::PreparedKey,
) {
    use crate::native::actions::WindowActionTarget;
    use bevy::{ecs::message::Messages, input::keyboard::KeyboardInput};
    let event = &prepared.input;
    if editor::dispatch_input(world, event) {
        if !event.state.is_pressed() {
            state.consumed.remove(&(event.window, event.key_code));
        }
        return;
    }
    let modifiers = prepared.modifiers;
    let identity = (event.window, event.key_code);
    if crate::native::layout::desktop::manipulation::pointer::keyboard(world, &prepared) {
        if event.state.is_pressed() {
            state.consumed.entry(identity).or_insert(None);
        } else {
            state.consumed.remove(&identity);
        }
        return;
    }
    let previous = state.consumed.get(&identity).copied();
    let binding = active_desktop_bindings(world)
        .into_iter()
        .find(|binding| binding.key == event.key_code && binding.modifiers == modifiers);
    let root = world
        .get::<WindowActionTarget>(event.window)
        .map(|target| target.0)
        .filter(|root| {
            world
                .get::<crate::native::canvas::CanvasView>(*root)
                .is_some()
        })
        .filter(|_| {
            world
                .get::<Window>(event.window)
                .is_none_or(|window| window.focused)
        });
    if let Some(root) = root
        && crate::native::layout::desktop::recent::key(
            world,
            root,
            &prepared,
            binding.is_some_and(|binding| binding.action.is_recent()),
        )
    {
        if event.state.is_pressed() {
            state.consumed.entry(identity).or_insert(None);
        } else {
            state.consumed.remove(&identity);
        }
        return;
    }
    if !event.state.is_pressed() && previous.is_some() {
        state.consumed.remove(&identity);
        return;
    }
    if event.state.is_pressed()
        && let Some(binding) = binding
        && let Some(root) = root
        && !scoped_binding(world, root, event.key_code, modifiers)
        && (previous.is_none() && !event.repeat
            || binding.repeat && event.repeat && previous == Some(Some(binding.action)))
    {
        state.consumed.insert(identity, Some(binding.action));
        crate::native::actions![binding.action].run(world, root);
    } else if previous.is_some() {
        return;
    } else {
        if world
            .get::<Window>(event.window)
            .is_some_and(|window| !window.focused)
        {
            return;
        }
        crate::native::keyboard::logical(world, modifiers);
        world
            .resource_mut::<Messages<KeyboardInput>>()
            .write(event.clone());
        if let Err(error) = world.run_system(state.dispatcher.unwrap()) {
            crate::native::notifications::report(world, "Keyboard", &error.to_string());
        }
        world.resource_mut::<Messages<KeyboardInput>>().clear();
    }
}

#[derive(Resource, Default)]
pub(crate) struct KeyboardModalOwner(pub Option<Entity>);

pub fn keyboard_owner(world: &World) -> Option<Entity> {
    world.get_resource::<KeyboardModalOwner>().and_then(|owner| owner.0)
}

pub(crate) fn input_eligible(world: &World, root: Entity, canvas: &str, target: Entity) -> bool {
    use crate::native::workspace::{Workspaces, WorkspaceMember};
    if world.get_resource::<crate::host::input::InputActivity>().is_some_and(|activity| !activity.active()) { return false; }
    let Some(spaces) = world.get::<Workspaces>(root).filter(|spaces| spaces.canvas_id == canvas) else { return false; };
    let mut ancestor = Some(target);
    for _ in 0..crate::native::layout::MAX_DEPTH {
        let Some(entity) = ancestor else { return false; };
        if world.get_entity(entity).is_err()
            || world.get::<WorkspaceMember>(entity).is_some_and(|member| member.0 != spaces.active)
            || world.get::<Node>(entity).is_some_and(|node| node.display == Display::None)
            || world.get::<Visibility>(entity).is_some_and(|visibility| *visibility == Visibility::Hidden)
            || world.get::<InheritedVisibility>(entity).is_some_and(|visibility| !visibility.get())
            || world.get::<bevy::ui::InteractionDisabled>(entity).is_some()
        { return false; }
        if entity == root { return true; }
        ancestor = world.get::<ChildOf>(entity).map(ChildOf::parent);
    }
    false
}

pub(crate) fn own_keyboard(world: &mut World, overlay: Entity) {
    world.init_resource::<KeyboardModalOwner>();
    world.resource_mut::<KeyboardModalOwner>().0 = Some(overlay);
}

pub(crate) fn release_keyboard(world: &mut World, overlay: Entity) {
    if let Some(mut owner) = world.get_resource_mut::<KeyboardModalOwner>()
        && owner.0 == Some(overlay)
    {
        owner.0 = None;
    }
    editor::quarantine(world);
}

#[derive(Clone, Copy, Debug)]
pub struct DesktopBinding {
    pub key: KeyCode,
    pub modifiers: crate::native::actions::Modifiers,
    pub action: crate::host::DesktopAction,
    pub repeat: bool,
}

impl Shortcut {
    pub(crate) fn desktop_action(self) -> Option<crate::host::DesktopAction> {
        use crate::host::{DesktopAction as A, Direction as D};
        Some(match self {
            Self::Overview => A::ToggleOverview,
            Self::DesktopEdit => A::ToggleEdit,
            Self::Unfocus => A::ToggleFocus,
            Self::RecentNext => A::RecentNext,
            Self::RecentPrevious => A::RecentPrevious,
            Self::FocusLeft => A::Focus(D::Left),
            Self::FocusDown => A::Focus(D::Down),
            Self::FocusUp => A::Focus(D::Up),
            Self::FocusRight => A::Focus(D::Right),
            Self::MoveLeft => A::Move(D::Left),
            Self::MoveDown => A::Move(D::Down),
            Self::MoveUp => A::Move(D::Up),
            Self::MoveRight => A::Move(D::Right),
            Self::Terminal => A::OpenTerminal,
            Self::Launcher => A::OpenLauncher,
            Self::Lock => A::Lock,
            Self::Logout => A::Logout,
            Self::External(action) => A::External(action),
            Self::Tiling => A::ToggleTiling,
            Self::CycleWidth => A::CycleItemWidth,
            Self::ShrinkWidth => A::Resize(D::Left),
            Self::GrowWidth => A::Resize(D::Right),
            Self::ShrinkHeight => A::Resize(D::Up),
            Self::GrowHeight => A::Resize(D::Down),
            Self::CycleRowHeight => A::CycleRowHeight,
            Self::Float => A::ToggleFloating,
            Self::Fullscreen => A::ToggleFullscreen,
            Self::WindowedFullscreen => A::ToggleWindowedFullscreen,
            Self::CompactCreate => A::CreateCompactGroup,
            Self::CompactAdd => A::AddCompactMembers,
            Self::CompactNext => A::NextCompactMember,
            Self::CompactPrevious => A::PreviousCompactMember,
            Self::CompactEarlier => A::MoveCompactMember(D::Left),
            Self::CompactLater => A::MoveCompactMember(D::Right),
            Self::CompactDetach => A::DetachCompactMember,
            Self::CompactDissolve => A::DissolveCompactGroup,
            Self::ScratchMark => A::MarkScratch,
            Self::ScratchShow => A::ShowScratch,
            Self::ScratchHide => A::HideScratch,
            Self::ScratchUnmark => A::UnmarkScratch,
            Self::MagnifyIncrease => A::Magnify(crate::host::magnification::Adjustment::Increase),
            Self::MagnifyDecrease => A::Magnify(crate::host::magnification::Adjustment::Decrease),
            Self::MagnifyReset => A::Magnify(crate::host::magnification::Adjustment::Reset),
            Self::Maximize => A::ToggleMaximized,
            Self::DesktopOperation => A::OpenOperation,
            Self::FocusLeftArrow => A::Focus(D::Left),
            Self::FocusDownArrow => A::Focus(D::Down),
            Self::FocusUpArrow => A::Focus(D::Up),
            Self::FocusRightArrow => A::Focus(D::Right),
            Self::Close => A::Close,
            Self::Workspace1 => A::Workspace(1),
            Self::Workspace2 => A::Workspace(2),
            Self::Workspace3 => A::Workspace(3),
            Self::Workspace4 => A::Workspace(4),
            Self::Workspace5 => A::Workspace(5),
            Self::Workspace6 => A::Workspace(6),
            Self::Workspace7 => A::Workspace(7),
            Self::Workspace8 => A::Workspace(8),
            Self::Workspace9 => A::Workspace(9),
            Self::WorkspacePrevious => A::PreviousWorkspace,
            Self::WorkspaceNext => A::NextWorkspace,
            Self::WorkspaceEarlier => A::MoveWorkspaceEarlier,
            Self::WorkspaceLater => A::MoveWorkspaceLater,
            Self::MoveWorkspace1 => A::MoveToWorkspace(1),
            Self::MoveWorkspace2 => A::MoveToWorkspace(2),
            Self::MoveWorkspace3 => A::MoveToWorkspace(3),
            Self::MoveWorkspace4 => A::MoveToWorkspace(4),
            Self::MoveWorkspace5 => A::MoveToWorkspace(5),
            Self::MoveWorkspace6 => A::MoveToWorkspace(6),
            Self::MoveWorkspace7 => A::MoveToWorkspace(7),
            Self::MoveWorkspace8 => A::MoveToWorkspace(8),
            Self::MoveWorkspace9 => A::MoveToWorkspace(9),
            Self::WindowRecord => A::InspectWindowRecord,
            Self::ReapplyOpening => A::ReapplyOpeningRules,
            Self::FocusMonitorLeft => A::FocusOutput(D::Left),
            Self::FocusMonitorDown => A::FocusOutput(D::Down),
            Self::FocusMonitorUp => A::FocusOutput(D::Up),
            Self::FocusMonitorRight => A::FocusOutput(D::Right),
            Self::MoveMonitorLeft => A::MoveToOutput(D::Left),
            Self::MoveMonitorDown => A::MoveToOutput(D::Down),
            Self::MoveMonitorUp => A::MoveToOutput(D::Up),
            Self::MoveMonitorRight => A::MoveToOutput(D::Right),
            Self::RowMonitorLeft => A::MoveRowToOutput(D::Left),
            Self::RowMonitorDown => A::MoveRowToOutput(D::Down),
            Self::RowMonitorUp => A::MoveRowToOutput(D::Up),
            Self::RowMonitorRight => A::MoveRowToOutput(D::Right),
            Self::WorkspaceMonitorLeft => A::MoveWorkspaceToOutput(D::Left),
            Self::WorkspaceMonitorDown => A::MoveWorkspaceToOutput(D::Down),
            Self::WorkspaceMonitorUp => A::MoveWorkspaceToOutput(D::Up),
            Self::WorkspaceMonitorRight => A::MoveWorkspaceToOutput(D::Right),
            _ => return None,
        })
    }
}

pub fn desktop_bindings(world: &World) -> Vec<DesktopBinding> {
    let defaults = Settings::default();
    let settings = world.get_resource::<Settings>().unwrap_or(&defaults);
    Shortcut::ALL
        .into_iter()
        .filter_map(|shortcut| {
            let action = shortcut.desktop_action()?;
            let chord = settings.chord(shortcut)?;
            Some(DesktopBinding {
                key: chord.key,
                modifiers: chord.modifiers,
                action,
                repeat: shortcut.repeats(),
            })
        })
        .collect()
}

pub fn active_desktop_bindings(world: &World) -> Vec<DesktopBinding> {
    active_bindings(world, desktop_bindings(world))
}

fn active_bindings(world: &World, bindings: Vec<DesktopBinding>) -> Vec<DesktopBinding> {
    let hosted = world.contains_resource::<crate::native::layout::desktop::HostRequests>();
    let owned = keyboard_owned(world);
    let overview = crate::native::layout::desktop::overview::active(world);
    let recent = crate::native::layout::desktop::recent::active(world);
    bindings
        .into_iter()
        .filter(|binding| {
            (!binding.action.requires_host() || hosted)
                && (!owned
                    || overview && binding.action == crate::host::DesktopAction::ToggleOverview
                    || recent && binding.action.is_recent())
        })
        .collect()
}

pub fn binding_snapshot(world: &World) -> (Vec<DesktopBinding>, Vec<DesktopBinding>) {
    let hosted = world.contains_resource::<crate::native::layout::desktop::HostRequests>();
    let all = desktop_bindings(world);
    let hardware = all.iter().copied().filter(|binding| {
        hosted && matches!(binding.action, crate::host::DesktopAction::External(action) if action.lockable())
    }).collect();
    (active_bindings(world, all), hardware)
}

pub fn pointer_bindings(world: &World) -> Vec<PointerBinding> {
    if keyboard_owned(world) {
        return Vec::new();
    }
    let defaults = Settings::default();
    let settings = world.get_resource::<Settings>().unwrap_or(&defaults);
    Shortcut::ALL
        .into_iter()
        .filter_map(|shortcut| {
            Some(PointerBinding {
                operation: shortcut.pointer_action()?,
                chord: settings.pointer(shortcut)?,
            })
        })
        .collect()
}

pub(crate) fn edit_binding(world: &mut World, root: Entity, shortcut: Shortcut) {
    editor::edit(world, root, shortcut);
}

pub(crate) fn install(world: &mut World, root: Entity) {
    use crate::native::actions::KeyBinding;
    let settings = world.resource::<Settings>();
    let mut bindings = Vec::new();
    for (shortcut, actions) in [
        (
            Shortcut::DeleteSand,
            crate::native::actions![crate::native::deletion::DeleteSelected],
        ),
        (
            Shortcut::ToggleEdit,
            crate::native::actions![crate::native::edit_mode::EditAction::Toggle],
        ),
        (
            Shortcut::OpenOperation,
            crate::native::actions![crate::native::operation::OpenOperation],
        ),
    ] {
        if let Some(chord) = settings.chord(shortcut) {
            let binding = KeyBinding::new(chord.key, chord.modifiers, actions);
            bindings.push(
                if shortcut == Shortcut::DeleteSand
                    || !chord.modifiers.control
                        && !chord.modifiers.alt
                        && !chord.modifiers.super_key
                {
                    binding.outside_text()
                } else {
                    binding
                },
            );
        }
    }
    for binding in desktop_bindings(world) {
        let mut configured = KeyBinding::new(
            binding.key,
            binding.modifiers,
            crate::native::actions![binding.action],
        );
        configured.repeat = binding.repeat;
        bindings.push(configured);
    }
    world.entity_mut(root).insert(ConfiguredBindings(bindings));
}

pub(crate) fn synchronize(world: &mut World) {
    if !world.is_resource_changed::<Settings>() {
        return;
    }
    let roots: Vec<_> = world
        .query_filtered::<Entity, With<ConfiguredBindings>>()
        .iter(world)
        .collect();
    for root in roots {
        install(world, root);
    }
}

pub(crate) fn pressed(world: &World, shortcut: Shortcut, keys: &ButtonInput<KeyCode>) -> bool {
    let defaults = Settings::default();
    world
        .get_resource::<Settings>()
        .unwrap_or(&defaults)
        .chord(shortcut)
        .is_some_and(|chord| {
            if let Some(modifiers) = crate::native::keyboard::current(world) {
                keys.pressed(chord.key) && modifiers == chord.modifiers
            } else if let Some(keyboard) =
                world.get_resource::<crate::host::input::KeyboardSnapshot>()
            {
                keys.pressed(chord.key)
                    && crate::native::actions::Modifiers::pressed_for(keys, Some(keyboard))
                        == chord.modifiers
            } else {
                chord.pressed(keys)
            }
        })
}

pub(crate) fn editing_text(world: &World) -> bool {
    if keyboard_owned(world) {
        return true;
    }
    let mut target = world
        .get_resource::<bevy::input_focus::InputFocus>()
        .and_then(|f| f.get());
    while let Some(entity) = target {
        if world.get::<bevy::text::EditableText>(entity).is_some() {
            return true;
        }
        target = world.get::<ChildOf>(entity).map(ChildOf::parent);
    }
    false
}

pub const GROUPS: &[(&str, &[(&str, &str)])] = &[
    (
        "Everywhere",
        &[
            ("Tab / Shift+Tab", "Focus the next / previous control"),
            ("Enter / Space", "Activate the focused button"),
            (
                "Escape",
                "Close the current popup, cancel a tool or clear selection",
            ),
            ("Click", "Use a control or focus text"),
            (
                "Wheel / Shift+wheel",
                "Scroll a panel vertically / horizontally",
            ),
        ],
    ),
    (
        "2D canvas",
        &[
            ("Drag the background", "Move the camera"),
            ("Right-drag a Sand", "Move the Sand"),
            ("Ctrl+left-drag a Sand", "Move the camera over a Sand"),
            (
                "Ctrl+right-click a Sand",
                "Select it or its group, in either mode",
            ),
            (
                "Ctrl+right-drag",
                "Select Sands in a rectangle, in either mode",
            ),
            (
                "Wheel on the background or on a Sand in edit mode",
                "Zoom the canvas",
            ),
        ],
    ),
    (
        "Edit mode",
        &[
            ("Click a Sand", "Select it and inspect its settings"),
            ("Left-drag a Sand", "Move it and the selected group"),
            ("Drag an edge or corner", "Resize the Sand"),
            (
                "Drag with an area drawing tool",
                "Draw an area of influence",
            ),
            (
                "Click with an area target tool",
                "Choose the target point or Sand",
            ),
        ],
    ),
    (
        "3D view, outside text fields",
        &[("Right-drag the scene", "Turn the camera with the pointer")],
    ),
    (
        "Text fields",
        &[
            ("Arrow keys / Home / End", "Move the caret"),
            ("Shift with caret movement", "Select text"),
            ("Ctrl+Left / Ctrl+Right", "Move by word"),
            ("Ctrl+A", "Select all text"),
            (
                "Ctrl+C / Ctrl+X / Ctrl+V",
                "Copy / cut (also Shift+Delete) / paste",
            ),
            (
                "Ctrl+Home / Ctrl+End, or Ctrl+Up / Ctrl+Down",
                "Move to the start / end of the text",
            ),
            (
                "Alt+Home / Alt+End (macOS: Command+Left / Right)",
                "Move to the start / end of the unwrapped line",
            ),
            ("Backspace / Delete", "Delete before / after the caret"),
            ("Ctrl+Backspace / Ctrl+Delete", "Delete a word"),
            ("Click-drag text", "Select text with the pointer"),
            ("Shift+click text", "Extend the selection"),
            (
                "Double-click / triple-click text",
                "Select a word / all text",
            ),
            ("Enter in multiline text", "Insert a new line"),
        ],
    ),
    (
        "Operation",
        &[
            (
                "@ followed by a slug",
                "Find a Record; submitting sets its quantity to zero",
            ),
            ("/ followed by a command", "Find a command"),
            ("Up / Down", "Choose a suggestion"),
            ("Tab or click a suggestion", "Complete the input"),
            ("Enter", "Run the entered operation"),
        ],
    ),
];
