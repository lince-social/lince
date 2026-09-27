use bevy::prelude::*;

pub const GROUPS: &[(&str, &[(&str, &str)])] = &[
    (
        "Everywhere",
        &[
            ("Alt+E", "Toggle edit mode"),
            ("Ctrl+K", "Open Operation"),
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
            ("Ctrl+right-drag", "Select Sands in a rectangle"),
            ("Delete, outside text fields", "Delete selected Sands or Castles"),
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
        &[
            ("W / S", "Move forward / backward"),
            ("A / D", "Move left / right"),
            ("Q / E", "Move down / up"),
            ("Arrow keys", "Turn the camera"),
            ("Right-drag the scene", "Turn the camera with the pointer"),
        ],
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
    crate::edit_mode::label(world, parent, "Cheat sheet", 22.0);
    for (group, shortcuts) in GROUPS {
        crate::edit_mode::label(world, parent, group, 18.0);
        for (keys, description) in *shortcuts {
            crate::edit_mode::label(world, parent, &format!("{keys}\n{description}"), 14.0);
        }
    }
}
