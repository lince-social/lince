use crate::theme::{PURPLE, Typography};
use bevy::{
    input_focus::tab_navigation::TabIndex,
    prelude::*,
    text::{EditableText, TextCursorStyle},
    ui_widgets::Button as WidgetButton,
};
use std::time::Duration;

pub const BUTTON_BORDER_WIDTH: f32 = 1.0;

#[derive(Clone, Copy)]
pub struct EditorOptions {
    pub multiline: bool,
    pub visible_lines: f32,
    pub max_characters: usize,
    pub minimum_height: f32,
}

impl EditorOptions {
    pub fn apply(self, text: &mut EditableText, node: &mut Node) {
        text.allow_newlines = self.multiline;
        text.visible_lines = Some(self.visible_lines.max(1.0));
        text.max_characters = Some(self.max_characters);
        node.min_height = px(self.minimum_height.max(0.0));
    }
}

pub fn editable(value: &str) -> EditableText {
    EditableText {
        allow_newlines: true,
        visible_lines: Some(3.0),
        max_characters: Some(256),
        cursor_blink_period: Duration::MAX,
        ..EditableText::new(value)
    }
}

pub fn text_editor(value: &str, typography: &Typography, tab_index: i32) -> impl Bundle + use<> {
    (
        editable(value),
        Node {
            width: percent(100),
            border: UiRect::all(px(BUTTON_BORDER_WIDTH)),
            padding: UiRect::all(px(6)),
            ..default()
        },
        crate::style::border(crate::tokens::Token::Accent),
        typography.text(22.0),
        crate::style::text(crate::tokens::Token::Ink),
        crate::style::CursorToken(crate::tokens::Token::Accent),
        TextCursorStyle {
            color: PURPLE,
            ..default()
        },
        TabIndex(tab_index),
    )
}

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
        bevy::text::LineHeight::Px(20.0),
        TextLayout::linebreak(bevy::text::LineBreak::NoWrap),
        crate::style::border(crate::tokens::Token::Accent),
        crate::style::text(crate::tokens::Token::Ink),
        crate::style::CursorToken(crate::tokens::Token::Accent),
        TabIndex(tab_index),
    )
}

pub fn button(tab_index: i32) -> impl Bundle {
    (WidgetButton, TabIndex(tab_index))
}

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

pub fn value(world: &World, entity: Entity) -> Result<String, String> {
    let input = world
        .get::<EditableText>(entity)
        .ok_or("Field is unavailable")?;
    if input.is_composing() {
        return Err("Finish typing before saving".into());
    }
    Ok(input.value().to_string())
}

pub fn clear(world: &mut World, parent: Entity) {
    let children: Vec<_> = world
        .get::<Children>(parent)
        .map(|children| children.iter().collect())
        .unwrap_or_default();
    for child in children {
        world.despawn(child);
    }
}

pub fn status(world: &mut World, entity: Entity, message: impl Into<String>) {
    if let Some(mut text) = world.get_mut::<Text>(entity) {
        text.set_if_neq(Text::new(message.into()));
    }
}
