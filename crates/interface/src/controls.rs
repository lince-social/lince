// non-lean-yet

//! Shared UI controls and helpers for updating live editors.

use crate::{
    i18n::native_text,
    style::{CursorToken, border, text as foreground},
    theme::{PURPLE, Typography},
    tokens::Token::{Accent, Ink},
};
use bevy::{
    input_focus::tab_navigation::TabIndex,
    prelude::*,
    text::{EditableText, LineBreak, LineHeight, TextCursorStyle, TextEdit},
    ui_widgets::Button as WidgetButton,
};
use nucleus::performance::add;
use std::{iter::repeat_n, time::Duration};

pub mod editing;

/// The default stroke width in UI pixels.
pub const BUTTON_BORDER_WIDTH: f32 = 1.0;

/// Updates a password mask without touching Bevy's change tick when it already fits.
pub fn password_mask(mut text: Mut<'_, Text>, count: usize) {
    if text.0.chars().eq(repeat_n('•', count)) {
        return;
    }
    let value = &mut text.0;
    value.clear();
    value.extend(repeat_n('•', count));
    add("password-mask-text-updates", 1);
}

/// Removes pending edits that would export a password to the clipboard.
pub fn protect_password(mut field: Mut<'_, EditableText>) {
    let exports = |edit: &TextEdit| matches!(edit, TextEdit::Copy | TextEdit::Cut);
    if !field.pending_edits.iter().any(exports) {
        return;
    }
    let before = field.pending_edits.len();
    field.pending_edits.retain(|edit| !exports(edit));
    add(
        "password-export-edits-rejected",
        (before - field.pending_edits.len()) as u64,
    );
}

/// Presentation metadata for a UI editor.
#[derive(Component)]
pub struct Editor {
    /// Caption shown for the field.
    pub title: String,
    /// Whether the field displays a mask and suppresses clipboard export.
    pub secret: bool,
}

/// Layout and input limits applied to a control after creation.
#[derive(Clone, Copy)]
pub struct EditorOptions {
    pub multiline: bool,
    pub visible_lines: f32,
    pub max_characters: usize,
    pub minimum_height: f32,
}

impl EditorOptions {
    /// Updates the editor's input settings and its node's minimum height.
    pub fn apply(self, text: &mut EditableText, node: &mut Node) {
        text.allow_newlines = self.multiline;
        text.visible_lines = Some(self.visible_lines.max(1.0));
        text.max_characters = Some(self.max_characters);
        node.min_height = px(self.minimum_height.max(0.0));
    }
}

/// Creates an editor with the shared multiline defaults.
pub fn editable(value: &str) -> EditableText {
    EditableText {
        allow_newlines: true,
        visible_lines: Some(3.0),
        max_characters: Some(256),
        cursor_blink_period: Duration::MAX,
        ..EditableText::new(value)
    }
}

fn editor_style(tab_index: i32) -> impl Bundle {
    (
        border(Accent),
        foreground(Ink),
        CursorToken(Accent),
        TabIndex(tab_index),
    )
}

/// Creates a multiline editor with the shared tab and cursor styling.
pub fn text_editor(value: &str, typography: &Typography, tab_index: i32) -> impl Bundle + use<> {
    (
        editable(value),
        Node {
            width: percent(100),
            border: UiRect::all(px(BUTTON_BORDER_WIDTH)),
            padding: UiRect::all(px(6)),
            ..default()
        },
        typography.text(22.0),
        editor_style(tab_index),
        TextCursorStyle {
            color: PURPLE,
            ..default()
        },
    )
}

/// Creates a single-line editor with a scalar count limit.
pub fn single_line_editor(
    value: &str,
    typography: &Typography,
    tab_index: i32,
    max_characters: usize,
) -> impl Bundle + use<> {
    let mut text = editable(value);
    text.allow_newlines = false;
    text.visible_lines = Some(1.0);
    text.max_characters = Some(max_characters);
    (
        text,
        Node {
            width: percent(100),
            min_width: px(0),
            flex_shrink: 0.0,
            border: UiRect::all(px(BUTTON_BORDER_WIDTH)),
            padding: UiRect::all(px(4)),
            ..default()
        },
        typography.text(16.0),
        LineHeight::Px(20.0),
        TextLayout::linebreak(LineBreak::NoWrap),
        editor_style(tab_index),
    )
}

/// Creates an activatable button at the requested tab index.
pub fn button(tab_index: i32) -> impl Bundle {
    (WidgetButton, TabIndex(tab_index))
}

/// Spawns a column within its parent.
pub fn column(world: &mut World, parent: Entity) -> Entity {
    world
        .spawn((
            ChildOf(parent),
            Node {
                width: percent(100),
                min_height: px(0),
                flex_direction: FlexDirection::Column,
                row_gap: px(8),
                flex_shrink: 0.0,
                ..default()
            },
        ))
        .id()
}

/// Spawns a wrapping row within its parent.
pub fn row(world: &mut World, parent: Entity) -> Entity {
    world
        .spawn((
            ChildOf(parent),
            Node {
                width: percent(100),
                flex_wrap: FlexWrap::Wrap,
                column_gap: px(8),
                row_gap: px(6),
                align_items: AlignItems::Center,
                flex_shrink: 0.0,
                ..default()
            },
        ))
        .id()
}

/// Copies the committed editor value into a reusable string.
pub fn copy_value(input: &EditableText, target: &mut String) {
    let text = input.value();
    target.clear();
    target.reserve(text.into_iter().map(str::len).sum());
    for part in text {
        target.push_str(part);
    }
}

/// Reads the committed value for saving.
///
/// # Errors
///
/// Returns the UI message when the editor is missing or an IME composition is active.
pub fn value(world: &World, entity: Entity) -> Result<String, String> {
    let input = world
        .get::<EditableText>(entity)
        .ok_or("Field is unavailable")?;
    if input.is_composing() {
        return Err("Finish typing before saving".into());
    }
    Ok(input.value().to_string())
}

/// Removes the parent's current child entities.
pub fn clear(world: &mut World, parent: Entity) {
    let children: Vec<_> = world
        .get::<Children>(parent)
        .map(|children| children.iter().collect())
        .unwrap_or_default();
    for child in children {
        world.despawn(child);
    }
}

/// Updates the status label when the logical text has changed.
pub fn status(world: &mut World, entity: Entity, message: impl AsRef<str>) {
    let message = message.as_ref();
    if let Some(mut text) = world.get_mut::<Text>(entity)
        && text.0 != message
        && !native_text(message).is_some_and(|script| text.0 == script.encoding())
    {
        text.0.clear();
        text.0.push_str(message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unchanged_status_preserves_storage_and_does_not_mark_text_changed() {
        let mut world = World::new();
        let entity = world.spawn(Text::new("Saved.")).id();
        world.clear_trackers();
        let original = world.get::<Text>(entity).unwrap().0.as_ptr();
        for _ in 0..1000 {
            status(&mut world, entity, "Saved.");
        }
        let text = world.entity(entity).get_ref::<Text>().unwrap();
        assert_eq!(text.0.as_ptr(), original);
        assert!(!text.is_changed());
        status(&mut world, entity, "Ready");
        let text = world.entity(entity).get_ref::<Text>().unwrap();
        assert_eq!(text.0, "Ready");
        assert_eq!(text.0.as_ptr(), original);
        assert!(text.is_changed());
    }
}
