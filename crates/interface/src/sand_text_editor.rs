use crate::{
    edit_mode::{EditAction, EditMode, control, label},
    sand::text_editor,
    sand_store::StoredSand,
    sand_text::{self, SandText, SavedText, TextOverflow},
    theme::Typography,
    workspace::{WorkspaceMember, Workspaces},
};
use bevy::{a11y::AccessibilityNode, prelude::*, text::EditableText};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextAction {
    Select(Entity),
    Add(bool),
    Remove,
    Overflow(TextOverflow),
}

#[derive(Component)]
pub(crate) struct TextPanel {
    pub sand: Entity,
    selected: Option<Entity>,
    pub(crate) fields: Vec<Entity>,
    error: Option<Entity>,
    original: String,
    observed: Vec<String>,
}

impl TextPanel {
    pub fn new(sand: Entity) -> Self {
        Self {
            sand,
            selected: None,
            fields: Vec::new(),
            error: None,
            original: String::new(),
            observed: Vec::new(),
        }
    }
}

fn available(world: &World, root: Entity, sand: Entity) -> bool {
    world.get::<StoredSand>(sand).is_some()
        && world
            .get::<ChildOf>(sand)
            .is_some_and(|parent| parent.parent() == root)
        && world
            .get::<WorkspaceMember>(sand)
            .zip(world.get::<Workspaces>(root))
            .is_some_and(|(member, spaces)| member.0 == spaces.active)
}

pub(crate) fn open(world: &mut World, root: Entity, sand: Entity) {
    if available(world, root, sand) {
        let panel = world.get::<EditMode>(root).unwrap().panel;
        let mut state = TextPanel::new(sand);
        state.selected = blocks(world, sand).first().copied();
        world.entity_mut(panel).insert(state);
    }
}

fn blocks(world: &World, sand: Entity) -> Vec<Entity> {
    world
        .get::<Children>(sand)
        .into_iter()
        .flatten()
        .copied()
        .filter(|entity| world.get::<SandText>(*entity).is_some())
        .collect()
}

pub(crate) fn apply(world: &mut World, root: Entity, action: TextAction) -> bool {
    let panel = world.get::<EditMode>(root).unwrap().panel;
    let Some(state) = world.get::<TextPanel>(panel) else {
        return false;
    };
    let sand = state.sand;
    if !available(world, root, sand) {
        return true;
    }
    let selected = state
        .selected
        .filter(|entity| blocks(world, sand).contains(entity));
    match action {
        TextAction::Select(entity) => {
            if blocks(world, sand).contains(&entity) {
                world.get_mut::<TextPanel>(panel).unwrap().selected = Some(entity);
            }
        }
        TextAction::Add(editable) => {
            let count = blocks(world, sand).len();
            if count >= 128 {
                error(world, panel, "This Sand already has 128 text areas.");
                return false;
            }
            let mut area = SandText::new(editable);
            area.offset[1] += count as f32 * 32.0;
            let entity = sand_text::spawn(
                world,
                sand,
                SavedText {
                    area,
                    text: String::new(),
                },
            );
            world.get_mut::<TextPanel>(panel).unwrap().selected = Some(entity);
            let mut stored = world.get_mut::<StoredSand>(sand).unwrap();
            stored.content.get_or_insert(entity);
        }
        TextAction::Remove => {
            if let Some(selected) = selected {
                world.despawn(selected);
                let first = blocks(world, sand).first().copied();
                world.get_mut::<TextPanel>(panel).unwrap().selected = first;
                let mut stored = world.get_mut::<StoredSand>(sand).unwrap();
                if stored.content == Some(selected) {
                    stored.content = first;
                }
            }
        }
        TextAction::Overflow(overflow) => {
            let Some(selected) = selected else {
                return false;
            };
            let fields = state.fields.clone();
            let text_field = fields.first().copied();
            let original = state.original.clone();
            let mut values = Vec::new();
            for entity in fields {
                let Some(text) = world.get::<EditableText>(entity) else {
                    return false;
                };
                if text.is_composing() || text.pending_paste.is_some() {
                    error(
                        world,
                        panel,
                        "Finish typing or pasting to apply this change.",
                    );
                    return false;
                }
                values.push(text.value().to_string());
            }
            if values.len() != 5 {
                return false;
            }
            let current = sand_text::value(world, selected);
            if current != original && values[0] != original && values[0] != current {
                error(
                    world,
                    panel,
                    "The text also changed in the Sand. Select its text area again to load that edit.",
                );
                return false;
            }
            if values[0] == original {
                values[0] = current;
                if let Some(mut text) =
                    text_field.and_then(|field| world.get_mut::<EditableText>(field))
                    && text.value().to_string() != values[0]
                {
                    text.editor.set_text(&values[0]);
                }
            }
            if let Some(mut text) = world.get_mut::<EditableText>(selected) {
                if text.is_composing() || text.pending_paste.is_some() {
                    error(
                        world,
                        panel,
                        "Finish the edit in the Sand to apply this change.",
                    );
                    return false;
                }
                if text.value().to_string() != values[0] {
                    text.editor.set_text(&values[0]);
                }
            } else if let Some(mut text) = world.get_mut::<Text>(selected) {
                text.0.clone_from(&values[0]);
            }
            world.get_mut::<TextPanel>(panel).unwrap().original = values[0].clone();
            let numbers: Result<Vec<f32>, _> = values[1..]
                .iter()
                .map(|value| value.trim().parse())
                .collect();
            let Ok(numbers) = numbers else {
                error(world, panel, "Use numbers for position and size.");
                return false;
            };
            let mut area = world.get::<SandText>(selected).unwrap().clone();
            area.offset = [numbers[0], numbers[1]];
            area.size = [numbers[2], numbers[3]];
            area.overflow = overflow;
            if !area.validate() {
                error(
                    world,
                    panel,
                    "Position must be 0–100000. Width and height must be 24–100000.",
                );
                return false;
            }
            let mut node = world.get_mut::<Node>(selected).unwrap();
            node.left = px(area.offset[0]);
            node.top = px(area.offset[1]);
            node.width = px(area.size[0]);
            node.height = px(area.size[1]);
            world.entity_mut(selected).insert(area);
            sand_text::fit_sand(world, sand);
            error(world, panel, "");
        }
    }
    true
}

pub(crate) fn autosave(world: &mut World) {
    let panels: Vec<_> = world
        .query::<(Entity, &EditMode)>()
        .iter(world)
        .filter(|(_, mode)| mode.enabled)
        .map(|(root, mode)| (root, mode.panel))
        .collect();
    for (root, panel) in panels {
        let Some(state) = world.get::<TextPanel>(panel) else {
            continue;
        };
        let Some(selected) = state.selected else {
            continue;
        };
        let Some(area) = world.get::<SandText>(selected) else {
            continue;
        };
        let overflow = area.overflow;
        let values: Option<Vec<String>> = state
            .fields
            .iter()
            .map(|entity| {
                let text = world.get::<EditableText>(*entity)?;
                (!text.is_composing() && text.pending_paste.is_none())
                    .then(|| text.value().to_string())
            })
            .collect();
        let Some(values) = values else { continue };
        if values.is_empty() || values == state.observed {
            continue;
        }
        world.get_mut::<TextPanel>(panel).unwrap().observed = values;
        apply(world, root, TextAction::Overflow(overflow));
    }
}

fn error(world: &mut World, panel: Entity, message: &str) {
    if let Some(entity) = world.get::<TextPanel>(panel).and_then(|state| state.error)
        && let Some(mut text) = world.get_mut::<Text>(entity)
    {
        text.0 = message.into();
    }
}

pub(crate) fn render(world: &mut World, root: Entity, panel: Entity) -> bool {
    let Some(state) = world.get::<TextPanel>(panel) else {
        return false;
    };
    let sand = state.sand;
    let selected = state.selected;
    if !available(world, root, sand) {
        world.entity_mut(panel).remove::<TextPanel>();
        return false;
    }
    label(world, panel, "Text inside this Sand", 22.0);
    label(
        world,
        panel,
        "Place text from the Sand’s top left corner. The text area has no visible border during use.",
        14.0,
    );
    control(
        world,
        root,
        panel,
        EditAction::Text(TextAction::Add(false)),
        "Add static text",
    );
    control(
        world,
        root,
        panel,
        EditAction::Text(TextAction::Add(true)),
        "Add editable text",
    );
    for (index, entity) in blocks(world, sand).into_iter().enumerate() {
        let area = world.get::<SandText>(entity).unwrap();
        let preview: String = sand_text::value(world, entity).chars().take(32).collect();
        let name = format!(
            "{}{} {}: {}",
            if Some(entity) == selected {
                "Selected · "
            } else {
                ""
            },
            if area.editable {
                "Editable text"
            } else {
                "Static text"
            },
            index + 1,
            preview.replace('\n', " ")
        );
        control(
            world,
            root,
            panel,
            EditAction::Text(TextAction::Select(entity)),
            &name,
        );
    }
    let Some(selected) = selected else {
        return true;
    };
    let Some(area) = world.get::<SandText>(selected).cloned() else {
        return true;
    };
    let entries = [
        ("Text", sand_text::value(world, selected)),
        ("Distance from left", area.offset[0].to_string()),
        ("Distance from top", area.offset[1].to_string()),
        ("Width", area.size[0].to_string()),
        ("Height", area.size[1].to_string()),
    ];
    let original = entries[0].1.clone();
    let mut fields = Vec::new();
    for (index, (name, value)) in entries.into_iter().enumerate() {
        label(world, panel, name, 14.0);
        let bundle = text_editor(&value, world.resource::<Typography>(), 0);
        let entity = world
            .spawn((
                bundle,
                ChildOf(panel),
                crate::token_style::background(crate::tokens::Token::Surface),
            ))
            .id();
        let mut editor = world.get_mut::<EditableText>(entity).unwrap();
        editor.max_characters = Some(if index == 0 { 4096 } else { 16 });
        editor.allow_newlines = index == 0;
        editor.visible_lines = Some(if index == 0 { 3.0 } else { 1.0 });
        if let Some(mut node) = world.get_mut::<AccessibilityNode>(entity) {
            node.set_label(name);
        }
        fields.push(entity);
    }
    let notice = label(world, panel, "", 14.0);
    let mut state = world.get_mut::<TextPanel>(panel).unwrap();
    state.fields = fields;
    state.error = Some(notice);
    state.original = original;
    if area.editable {
        label(
            world,
            panel,
            match area.overflow {
                TextOverflow::Scroll => {
                    "Currently: scroll inside the area. Click the text to write. Scroll up or down to read it."
                }
                TextOverflow::Grow => {
                    "Currently: grow downward. Click the text to write. Height is the minimum size; the Sand grows to fit."
                }
            },
            14.0,
        );
        control(
            world,
            root,
            panel,
            EditAction::Text(TextAction::Overflow(TextOverflow::Scroll)),
            "Scroll inside area",
        );
        control(
            world,
            root,
            panel,
            EditAction::Text(TextAction::Overflow(TextOverflow::Grow)),
            "Grow downward",
        );
    } else {
        label(
            world,
            panel,
            "Static text stays within this area. Change it here in edit mode.",
            14.0,
        );
    }
    control(
        world,
        root,
        panel,
        EditAction::Text(TextAction::Remove),
        "Remove text area",
    );
    true
}
