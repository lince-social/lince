use super::{CustomCastle, storage};
use crate::{
    actions::{Action, ActionButton},
    edit_mode::label,
    icons::{Icon, IconButton},
};
use bevy::{prelude::*, text::EditableText};

#[derive(Component, Default)]
struct Status {
    name: String,
    message: String,
}

#[derive(Clone)]
enum Command {
    Save(Entity),
    Add(String),
    Refresh,
}

impl Action for Command {
    fn apply(&self, world: &mut World, root: Entity) {
        if !world
            .get::<crate::edit_mode::EditMode>(root)
            .is_some_and(|m| m.enabled)
        {
            return;
        }
        let result = match self {
            Self::Save(field) => {
                let Some(text) = world.get::<EditableText>(*field) else {
                    return;
                };
                let name = text.value().to_string();
                world.entity_mut(root).insert(Status {
                    name: name.clone(),
                    message: String::new(),
                });
                CustomCastle::capture(world, root, &name).and_then(|castle| {
                    let directory = storage::directory(world).map_err(|e| e.to_string())?;
                    storage::save(&directory, &castle).map_err(|e| e.to_string())?;
                    Ok(format!("Saved {} to Custom.", castle.name))
                })
            }
            Self::Add(filename) => storage::directory(world)
                .and_then(|directory| storage::load(&directory, filename))
                .map_err(|e| e.to_string())
                .and_then(|castle| {
                    castle.spawn(world, root)?;
                    Ok(format!("Added {} at the camera.", castle.name))
                }),
            Self::Refresh => Ok(String::new()),
        };
        if world.get::<Status>(root).is_none() {
            world.entity_mut(root).insert(Status::default());
        }
        world.get_mut::<Status>(root).unwrap().message = result.unwrap_or_else(|e| e);
        crate::edit_mode::render_panel(world, root);
    }
}

fn row(world: &mut World, parent: Entity) -> Entity {
    world
        .spawn((
            Node {
                width: percent(100),
                column_gap: px(8),
                align_items: AlignItems::Center,
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(parent),
        ))
        .id()
}

fn icon(world: &mut World, parent: Entity, root: Entity, icon: Icon, tip: &str, command: Command) {
    world.spawn((
        IconButton::new(icon, tip),
        ActionButton::new(root, crate::actions![command]),
        ChildOf(parent),
    ));
}

pub(crate) fn store_entries(world: &mut World, root: Entity, parent: Entity) {
    let heading = row(world, parent);
    label(world, heading, "Custom", 18.0);
    let directory = storage::directory(world);
    let tip = match &directory {
        Ok(directory) => format!(
            "Your custom Castles are saved as files in {}. Back up these files before switching computers or wiping this one. Copy them into the same folder on another computer, then refresh Custom. Each entry's info shows its file.",
            directory.display()
        ),
        Err(error) => error.to_string(),
    };
    world.spawn((IconButton::new(Icon::Info, tip), ChildOf(heading)));
    icon(
        world,
        heading,
        root,
        Icon::Reset,
        "Refresh custom Castles from disk",
        Command::Refresh,
    );
    label(
        world,
        parent,
        "Select Sands or a group, name it, then save it here.",
        14.0,
    );
    let name = world
        .get::<Status>(root)
        .map_or_else(String::new, |s| s.name.clone());
    let bundle = crate::sand::text_editor(&name, world.resource::<crate::theme::Typography>(), 0);
    let controls = row(world, parent);
    let field = world.spawn((bundle, ChildOf(controls))).id();
    world.entity_mut(field).insert((
        Node {
            flex_grow: 1.0,
            min_width: px(0),
            height: px(36),
            border: UiRect::all(px(1)),
            ..default()
        },
        crate::token_style::border(crate::tokens::Token::Accent),
    ));
    let mut editor = world.get_mut::<EditableText>(field).unwrap();
    editor.max_characters = Some(80);
    editor.visible_lines = Some(1.0);
    if let Some(mut node) = world.get_mut::<bevy::a11y::AccessibilityNode>(field) {
        node.set_label("Custom Castle name");
    }
    icon(
        world,
        controls,
        root,
        Icon::Save,
        "Save selected group as a custom Castle",
        Command::Save(field),
    );
    let message = world
        .get::<Status>(root)
        .map_or_else(String::new, |s| s.message.clone());
    if !message.is_empty() {
        label(world, parent, &message, 14.0);
    }
    let directory = match directory {
        Ok(directory) => directory,
        Err(error) => {
            label(world, parent, &error.to_string(), 14.0);
            return;
        }
    };
    let (entries, errors) = storage::entries(&directory);
    if entries.is_empty() {
        label(world, parent, "No custom Castles saved yet.", 14.0);
    }
    for (filename, name, count) in entries {
        let entry = row(world, parent);
        let title = label(world, entry, &format!("{name} · {count} parts"), 16.0);
        let mut node = world.get_mut::<Node>(title).unwrap();
        node.flex_grow = 1.0;
        node.flex_shrink = 1.0;
        node.flex_basis = px(0);
        node.min_width = px(0);
        world.spawn((
            IconButton::new(
                Icon::Info,
                format!(
                    "Back up this custom Castle file: {}",
                    directory.join(&filename).display()
                ),
            ),
            ChildOf(entry),
        ));
        icon(
            world,
            entry,
            root,
            Icon::Plus,
            &format!("Add {name} at the camera"),
            Command::Add(filename),
        );
    }
    for error in errors.iter().take(3) {
        label(world, parent, error, 13.0);
    }
    if errors.len() > 3 {
        label(
            world,
            parent,
            &format!("{} more files could not be loaded.", errors.len() - 3),
            13.0,
        );
    }
}
