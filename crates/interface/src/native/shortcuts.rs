use bevy::prelude::*;

mod model;
pub(crate) mod tests;
pub(crate) use model::{Settings, Shortcut};

#[derive(Component)]
pub(crate) struct ConfiguredBindings(pub Vec<crate::native::actions::KeyBinding>);

#[derive(Clone, Copy, Debug)]
pub struct DesktopBinding {
    pub key: KeyCode,
    pub modifiers: crate::native::actions::Modifiers,
    pub action: crate::host::DesktopAction,
}

pub fn desktop_bindings(world: &World) -> Vec<DesktopBinding> {
    use crate::host::{DesktopAction as A, Direction as D};
    let defaults = Settings::default();
    let settings = world.get_resource::<Settings>().unwrap_or(&defaults);
    [
        (Shortcut::FocusLeft, A::Focus(D::Left)),
        (Shortcut::FocusDown, A::Focus(D::Down)),
        (Shortcut::FocusUp, A::Focus(D::Up)),
        (Shortcut::FocusRight, A::Focus(D::Right)),
        (Shortcut::MoveLeft, A::Move(D::Left)),
        (Shortcut::MoveDown, A::Move(D::Down)),
        (Shortcut::MoveUp, A::Move(D::Up)),
        (Shortcut::MoveRight, A::Move(D::Right)),
        (Shortcut::StackLeft, A::Stack(D::Left)),
        (Shortcut::StackRight, A::Stack(D::Right)),
        (Shortcut::Terminal, A::OpenTerminal),
        (Shortcut::Launcher, A::OpenLauncher),
        (Shortcut::Lock, A::Lock),
        (Shortcut::Logout, A::Logout),
        (Shortcut::Float, A::ToggleFloating),
        (Shortcut::Fullscreen, A::ToggleFullscreen),
        (Shortcut::Maximize, A::ToggleMaximized),
        (Shortcut::DesktopOperation, A::OpenOperation),
        (Shortcut::FocusLeftArrow, A::Focus(D::Left)),
        (Shortcut::FocusDownArrow, A::Focus(D::Down)),
        (Shortcut::FocusUpArrow, A::Focus(D::Up)),
        (Shortcut::FocusRightArrow, A::Focus(D::Right)),
        (Shortcut::Close, A::Close),
        (Shortcut::Workspace1, A::Workspace(1)),
        (Shortcut::Workspace2, A::Workspace(2)),
        (Shortcut::Workspace3, A::Workspace(3)),
        (Shortcut::Workspace4, A::Workspace(4)),
        (Shortcut::Workspace5, A::Workspace(5)),
        (Shortcut::Workspace6, A::Workspace(6)),
        (Shortcut::Workspace7, A::Workspace(7)),
        (Shortcut::Workspace8, A::Workspace(8)),
        (Shortcut::Workspace9, A::Workspace(9)),
        (Shortcut::WindowRecord, A::InspectWindowRecord),
        (Shortcut::FocusMonitorLeft, A::FocusOutput(D::Left)),
        (Shortcut::FocusMonitorDown, A::FocusOutput(D::Down)),
        (Shortcut::FocusMonitorUp, A::FocusOutput(D::Up)),
        (Shortcut::FocusMonitorRight, A::FocusOutput(D::Right)),
        (Shortcut::MoveMonitorLeft, A::MoveToOutput(D::Left)),
        (Shortcut::MoveMonitorDown, A::MoveToOutput(D::Down)),
        (Shortcut::MoveMonitorUp, A::MoveToOutput(D::Up)),
        (Shortcut::MoveMonitorRight, A::MoveToOutput(D::Right)),
        (
            Shortcut::WorkspaceMonitorLeft,
            A::MoveWorkspaceToOutput(D::Left),
        ),
        (
            Shortcut::WorkspaceMonitorDown,
            A::MoveWorkspaceToOutput(D::Down),
        ),
        (
            Shortcut::WorkspaceMonitorUp,
            A::MoveWorkspaceToOutput(D::Up),
        ),
        (
            Shortcut::WorkspaceMonitorRight,
            A::MoveWorkspaceToOutput(D::Right),
        ),
    ]
    .into_iter()
    .filter_map(|(shortcut, action)| {
        settings.chord(shortcut).map(|chord| DesktopBinding {
            key: chord.key,
            modifiers: chord.modifiers,
            action,
        })
    })
    .collect()
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
        bindings.push(KeyBinding::new(
            binding.key,
            binding.modifiers,
            crate::native::actions![binding.action],
        ));
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
        .is_some_and(|chord| chord.pressed(keys))
}

pub(crate) fn editing_text(world: &World) -> bool {
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

#[derive(Component)]
struct BindingRow {
    shortcut: Shortcut,
    editor: Entity,
    active: Entity,
    status: Entity,
}

#[derive(Clone, Copy)]
enum Change {
    Save,
    Reset,
    Unbind,
    ResetAll,
}

impl crate::native::actions::Action for Change {
    fn apply(&self, world: &mut World, target: Entity) {
        if matches!(self, Self::ResetAll) {
            *world.resource_mut::<Settings>() = Settings::default();
            let rows: Vec<_> = world
                .query_filtered::<Entity, With<BindingRow>>()
                .iter(world)
                .collect();
            for row in rows {
                refresh_row(world, row, "Restored default binding.");
            }
        } else {
            let Some(row) = world.get::<BindingRow>(target) else {
                return;
            };
            let shortcut = row.shortcut;
            let editor = row.editor;
            let status = row.status;
            let result = match self {
                Self::Save => {
                    let Some(text) = world.get::<bevy::text::EditableText>(editor) else {
                        return;
                    };
                    let value = text.value().to_string();
                    world.resource_mut::<Settings>().set(shortcut, &value)
                }
                Self::Reset => world.resource_mut::<Settings>().reset(shortcut),
                Self::Unbind => world.resource_mut::<Settings>().set(shortcut, "Unbound"),
                Self::ResetAll => unreachable!(),
            };
            match result {
                Ok(()) => refresh_row(world, target, "Saved."),
                Err(error) => {
                    if let Some(mut text) = world.get_mut::<Text>(status) {
                        text.0 = error;
                    }
                }
            }
        }
        synchronize(world);
    }
}

fn refresh_row(world: &mut World, row: Entity, message: &str) {
    let Some(row) = world.get::<BindingRow>(row) else {
        return;
    };
    let (shortcut, editor, active, status) = (row.shortcut, row.editor, row.active, row.status);
    let value = world.resource::<Settings>().text(shortcut).to_string();
    if let Some(mut text) = world.get_mut::<bevy::text::EditableText>(editor) {
        text.editor.set_text(&value);
    }
    if let Some(mut text) = world.get_mut::<Text>(active) {
        text.0 = format!("Active: {value}");
    }
    if let Some(mut text) = world.get_mut::<Text>(status) {
        text.0 = message.into();
    }
}

fn control(world: &mut World, parent: Entity, target: Entity, action: Change, value: &str) {
    let button = world
        .spawn((
            crate::native::sand::button(0),
            crate::native::actions::ActionButton::new(target, crate::native::actions![action]),
            Node {
                padding: UiRect::axes(px(10), px(7)),
                ..default()
            },
            ChildOf(parent),
        ))
        .id();
    crate::native::edit_mode::label(world, button, value, 14.0);
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

pub(crate) fn panel(world: &mut World, parent: Entity) {
    crate::native::edit_mode::label(world, parent, "Shortcuts", 22.0);
    crate::native::edit_mode::label(
        world,
        parent,
        "Enter a key such as Delete, F2 or Ctrl+Shift+D, then Save. Use Unbind to disable an action. Text editing, focus navigation and Escape keep their standard keys. Deletion and 3D movement only run outside text fields.",
        14.0,
    );
    control(
        world,
        parent,
        parent,
        Change::ResetAll,
        "Reset all shortcuts",
    );
    for shortcut in Shortcut::ALL {
        let row = world
            .spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: px(4),
                    flex_shrink: 0.0,
                    ..default()
                },
                ChildOf(parent),
            ))
            .id();
        crate::native::edit_mode::label(world, row, shortcut.label(), 16.0);
        let value = world.resource::<Settings>().text(shortcut).to_string();
        let active = crate::native::edit_mode::label(world, row, &format!("Active: {value}"), 14.0);
        let controls = world
            .spawn((
                Node {
                    column_gap: px(6),
                    row_gap: px(4),
                    flex_wrap: FlexWrap::Wrap,
                    flex_shrink: 0.0,
                    ..default()
                },
                ChildOf(row),
            ))
            .id();
        let bundle = crate::native::sand::text_editor(
            &value,
            world.resource::<crate::native::theme::Typography>(),
            0,
        );
        let editor = world
            .spawn((
                bundle,
                bevy::a11y::AccessibilityNode::default(),
                ChildOf(controls),
            ))
            .id();
        world.get_mut::<Node>(editor).unwrap().width = px(180);
        let mut text = world.get_mut::<bevy::text::EditableText>(editor).unwrap();
        text.allow_newlines = false;
        text.visible_lines = Some(1.0);
        text.max_characters = Some(80);
        world
            .get_mut::<bevy::a11y::AccessibilityNode>(editor)
            .unwrap()
            .set_label(format!("{} shortcut", shortcut.label()));
        control(world, controls, row, Change::Save, "Save");
        control(world, controls, row, Change::Reset, "Reset");
        control(world, controls, row, Change::Unbind, "Unbind");
        let status = crate::native::edit_mode::label(world, row, "", 14.0);
        world.entity_mut(row).insert(BindingRow {
            shortcut,
            editor,
            active,
            status,
        });
    }
    crate::native::edit_mode::label(world, parent, "Cheat sheet", 22.0);
    for (group, shortcuts) in GROUPS {
        crate::native::edit_mode::label(world, parent, group, 18.0);
        for (keys, description) in *shortcuts {
            crate::native::edit_mode::label(world, parent, &format!("{keys}\n{description}"), 14.0);
        }
    }
}
