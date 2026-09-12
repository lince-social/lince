use crate::{
    canvas_background::{self, CanvasColors},
    edit_mode::{EditAction, EditMode, control, label},
    sand::text_editor,
    theme::Typography,
    workspace::Workspaces,
};
use bevy::{a11y::AccessibilityNode, prelude::*, text::EditableText};

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorField {
    Background,
    Grid,
}

impl ColorField {
    fn index(self) -> usize {
        match self {
            Self::Background => 0,
            Self::Grid => 1,
        }
    }
}

#[derive(Component)]
pub(crate) struct Swatch(Entity);

#[derive(Component)]
struct Editor {
    workspace: u64,
    fields: [Entity; 2],
    error: Entity,
    observed: [String; 2],
}

fn hex(rgb: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2])
}

fn parse(value: &str) -> Option<[u8; 3]> {
    let value = value.trim().strip_prefix('#').unwrap_or(value.trim());
    if !matches!(value.len(), 3 | 6) || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let mut rgb = [0; 3];
    for (index, channel) in rgb.iter_mut().enumerate() {
        *channel = if value.len() == 3 {
            u8::from_str_radix(&value[index..index + 1], 16).ok()? * 17
        } else {
            u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).ok()?
        };
    }
    Some(rgb)
}

pub(crate) fn render(world: &mut World, root: Entity, panel: Entity) {
    let colors = canvas_background::current(world, root);
    let workspace = world.get::<Workspaces>(root).unwrap().active;
    let heading = world
        .spawn((
            Node {
                width: percent(100),
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::Center,
                ..default()
            },
            ChildOf(panel),
        ))
        .id();
    label(world, heading, "Customization", 22.0);
    control(world, root, heading, EditAction::ResetCanvasColors, "Reset");
    let mut fields = Vec::new();
    for (field, name, rgb) in [
        (ColorField::Background, "Background", colors.background),
        (ColorField::Grid, "Thin grid", colors.grid),
    ] {
        let group = world
            .spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: px(6),
                    flex_shrink: 0.0,
                    ..default()
                },
                ChildOf(panel),
            ))
            .id();
        label(world, group, name, 16.0);
        let row = world
            .spawn((
                Node {
                    column_gap: px(8),
                    align_items: AlignItems::Center,
                    flex_shrink: 0.0,
                    ..default()
                },
                ChildOf(group),
            ))
            .id();
        let swatch = world
            .spawn((
                Node {
                    width: px(28),
                    height: px(28),
                    flex_shrink: 0.0,
                    ..default()
                },
                BackgroundColor(canvas_background::color(rgb)),
                ChildOf(row),
            ))
            .id();
        let value = hex(rgb);
        let bundle = text_editor(&value, world.resource::<Typography>(), 0);
        let editor = world
            .spawn((
                bundle,
                field,
                Swatch(swatch),
                crate::token_style::background(crate::tokens::Token::Surface),
                ChildOf(row),
            ))
            .id();
        world.entity_mut(editor).insert(EditableText {
            allow_newlines: false,
            visible_lines: Some(1.0),
            max_characters: Some(16),
            ..crate::sand::editable(&value)
        });
        if let Some(mut node) = world.get_mut::<AccessibilityNode>(editor) {
            node.set_label(format!("{name} color, hex value"));
        }
        fields.push(editor);
        let palette = world
            .spawn((
                Node {
                    column_gap: px(6),
                    row_gap: px(6),
                    flex_wrap: FlexWrap::Wrap,
                    flex_shrink: 0.0,
                    ..default()
                },
                ChildOf(group),
            ))
            .id();
        for (name, rgb) in [
            ("Charcoal", [18, 18, 20]),
            ("Slate", [71, 85, 105]),
            ("Paper", [248, 250, 252]),
            ("Indigo", [99, 102, 241]),
        ] {
            let button = control(
                world,
                root,
                palette,
                EditAction::CanvasColor(field, rgb),
                name,
            );
            if let Some(mut node) = world.get_mut::<AccessibilityNode>(button) {
                node.set_label(format!(
                    "{} color: {name}",
                    if field == ColorField::Background {
                        "Background"
                    } else {
                        "Grid"
                    }
                ));
            }
            world
                .get_mut::<crate::icons::IconButton>(button)
                .unwrap()
                .label = format!(
                "{} color: {name}",
                if field == ColorField::Background {
                    "Background"
                } else {
                    "Grid"
                }
            );
            world.entity_mut(button).insert(crate::icons::IconStyle {
                background: canvas_background::color(rgb),
                color: if rgb.iter().map(|channel| *channel as u16).sum::<u16>() >= 384 {
                    Color::BLACK
                } else {
                    crate::theme::INK
                },
                ..default()
            });
            world
                .get_mut::<BorderColor>(button)
                .unwrap()
                .set_if_neq(BorderColor::all(canvas_background::color(rgb)));
        }
    }
    let error = label(world, panel, "", 14.0);
    world.get_mut::<Node>(error).unwrap().display = Display::None;
    world.entity_mut(panel).insert(Editor {
        workspace,
        fields: [fields[0], fields[1]],
        error,
        observed: [hex(colors.background), hex(colors.grid)],
    });
}

pub(crate) fn autosave(world: &mut World) {
    let roots: Vec<_> = world
        .query::<(Entity, &EditMode)>()
        .iter(world)
        .filter(|(_, mode)| mode.enabled)
        .map(|(root, mode)| (root, mode.panel))
        .collect();
    for (root, panel) in roots {
        let Some(editor) = world.get::<Editor>(panel) else {
            continue;
        };
        let mut values = Vec::new();
        for entity in editor.fields {
            if let Some(text) = world.get::<EditableText>(entity)
                && !text.is_composing()
                && text.pending_paste.is_none()
            {
                values.push(text.value().to_string());
            }
        }
        if values.len() != 2 || values.as_slice() == editor.observed {
            continue;
        }
        world.get_mut::<Editor>(panel).unwrap().observed = [values[0].clone(), values[1].clone()];
        save(world, root, false);
    }
}

pub(crate) fn preset(world: &mut World, root: Entity, field: ColorField, rgb: [u8; 3]) {
    let panel = world.get::<EditMode>(root).unwrap().panel;
    if let Some(editor) = world.get::<Editor>(panel) {
        let entity = editor.fields[field.index()];
        if let Some(mut text) = world.get_mut::<EditableText>(entity) {
            text.editor.set_text(&hex(rgb));
        }
        if let Some(wake) = world.get_resource::<crate::wake::WakeSignal>() {
            wake.ring();
        }
    }
}

pub(crate) fn save(world: &mut World, root: Entity, reset: bool) -> bool {
    let panel = world.get::<EditMode>(root).unwrap().panel;
    let Some(editor) = world.get::<Editor>(panel) else {
        return false;
    };
    let workspace = editor.workspace;
    let error = editor.error;
    let mut values = Vec::new();
    if !reset {
        for entity in editor.fields {
            let Some(text) = world.get::<EditableText>(entity) else {
                return false;
            };
            if text.is_composing() || text.pending_paste.is_some() {
                return false;
            }
            values.push(parse(&text.value().to_string()));
        }
    }
    let inherited = canvas_background::current(world, root);
    let inherited = [inherited.background, inherited.grid];
    let mut spaces = world.get_mut::<Workspaces>(root).unwrap();
    if spaces.active != workspace {
        return false;
    }
    let entry = spaces
        .entries
        .iter_mut()
        .find(|space| space.id == workspace)
        .unwrap();
    for index in 0..2 {
        if reset {
            entry.color_overrides[index] = false;
        } else if values[index].is_some() && values[index] != Some(inherited[index]) {
            entry.color_overrides[index] = true;
        }
    }
    let colors = &mut entry.colors;
    *colors = if reset {
        CanvasColors::default()
    } else {
        CanvasColors {
            background: values[0].unwrap_or(colors.background),
            grid: values[1].unwrap_or(colors.grid),
        }
    };
    let invalid = values.iter().any(Option::is_none);
    world.get_mut::<Node>(error).unwrap().display = if invalid {
        Display::Flex
    } else {
        Display::None
    };
    if invalid {
        world.get_mut::<Text>(error).unwrap().0 =
            "Use a color such as #121214 or #ABC. Incomplete colors keep their previous value."
                .into();
    }
    if let Some(wake) = world.get_resource::<crate::wake::WakeSignal>() {
        wake.ring();
    }
    true
}

pub(crate) fn preview(
    fields: Query<(&EditableText, &Swatch)>,
    mut swatches: Query<&mut BackgroundColor>,
) {
    for (text, swatch) in &fields {
        if let Some(rgb) = parse(&text.value().to_string())
            && let Ok(mut color) = swatches.get_mut(swatch.0)
        {
            color.set_if_neq(BackgroundColor(canvas_background::color(rgb)));
        }
    }
}

pub(crate) mod tests {
    use super::*;

    #[cfg_attr(test, test)]
    fn color_input_accepts_short_and_full_hex_without_panicking_on_unicode() {
        assert_eq!(parse(" #AbC "), Some([170, 187, 204]));
        assert_eq!(parse("12abEF"), Some([18, 171, 239]));
        for value in ["", "#abcd", "#12345678", "red", "💜ab", "ééé", "#12ggff"] {
            assert_eq!(parse(value), None);
        }
    }

    crate::laboratory_cases! {
        color_input_accepts_short_and_full_hex_without_panicking_on_unicode,
    }
}
