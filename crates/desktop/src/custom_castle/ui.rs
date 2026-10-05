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

#[derive(Component)]
struct NameField;

#[derive(Component)]
struct SavedCustom(String);

#[derive(Component)]
struct AddedCustom(Vec<Entity>);

#[derive(Clone)]
enum Command {
    Save(Entity),
    Add(String),
    Refresh,
}

impl Action for Command {
    fn tutorial_operations(&self) -> &'static [lince_interface::practice::Operation] {
        use lince_interface::practice::Operation;
        match self {
            Self::Save(_) => &[Operation::SaveCustomCastle],
            Self::Add(_) => &[Operation::AddCustomCastle],
            _ => &[],
        }
    }
    fn practice_intent(&self) -> crate::actions::PracticeIntent {
        match self {
            Self::Save(_) => crate::actions::PracticeIntent::Feature(
                lince_interface::practice::Operation::SaveCustomCastle,
            ),
            Self::Add(_) => crate::actions::PracticeIntent::Feature(
                lince_interface::practice::Operation::AddCustomCastle,
            ),
            Self::Refresh => crate::actions::PracticeIntent::Navigation,
        }
    }
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
                    let directory =
                        storage::scoped_directory(world, root).map_err(|e| e.to_string())?;
                    let path = storage::save(&directory, &castle).map_err(|e| e.to_string())?;
                    world.entity_mut(root).insert(SavedCustom(
                        path.file_name().unwrap().to_string_lossy().into_owned(),
                    ));
                    Ok(format!("Saved {} to Custom.", castle.name))
                })
            }
            Self::Add(filename) => storage::scoped_directory(world, root)
                .and_then(|directory| storage::load(&directory, filename))
                .map_err(|e| e.to_string())
                .and_then(|castle| {
                    let entities = castle.spawn(world, root)?;
                    crate::instinct::practice::track_custom(world, root, &entities);
                    world.entity_mut(root).insert(AddedCustom(entities));
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
    super::library::show(world, root, parent);
    let heading = row(world, parent);
    label(world, heading, "Custom", 18.0);
    let directory = storage::scoped_directory(world, root);
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
    let field = world
        .spawn((
            bundle,
            ChildOf(controls),
            NameField,
            crate::actions::TutorialField {
                owner: root,
                operation: lince_interface::practice::Operation::SaveCustomCastle,
            },
        ))
        .id();
    world.entity_mut(field).insert((
        Node {
            flex_grow: 1.0,
            min_width: px(0),
            height: px(36),
            border: UiRect::all(px(1)),
            ..default()
        },
        crate::token_style::border(crate::tokens::Token::Accent),
        crate::icons::Tooltip("Custom Castle name".into()),
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
        let Ok(castle) = storage::load(&directory, &filename) else {
            continue;
        };
        let path = directory.join(&filename);
        let entry = crate::sand_store::castle_entry(
            world,
            root,
            parent,
            &name,
            &format!("A saved composition of {count} parts."),
            Command::Add(filename),
            |world, root| {
                let _ = castle.spawn(world, root);
                crate::sand_store::preview::compose(world, root)
            },
        );
        world
            .entity_mut(entry)
            .insert(crate::icons::Tooltip(format!(
                "Back up this custom Castle file: {}",
                path.display()
            )));
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

pub(crate) fn save_example(world: &mut World, root: Entity) {
    if world.get::<SavedCustom>(root).is_some() {
        return;
    }
    let field = world
        .query_filtered::<Entity, With<NameField>>()
        .iter(world)
        .find(|entity| crate::instinct::practice::window_root(world, *entity) == Some(root));
    if let Some(field) = field {
        world
            .get_mut::<EditableText>(field)
            .unwrap()
            .editor
            .set_text("My practice Castle");
        Command::Save(field).apply(world, root);
    }
}

pub(crate) fn add_example(world: &mut World, root: Entity) {
    if world.get::<AddedCustom>(root).is_some() {
        return;
    }
    if let Some(filename) = world.get::<SavedCustom>(root).map(|saved| saved.0.clone()) {
        Command::Add(filename).apply(world, root);
    }
}

pub(crate) fn saved_example(world: &World, root: Entity) -> bool {
    world.get::<SavedCustom>(root).is_some()
}

pub(crate) fn added_example(world: &World, root: Entity) -> bool {
    world.get::<AddedCustom>(root).is_some_and(|added| {
        !added.0.is_empty()
            && added
                .0
                .iter()
                .all(|entity| world.get_entity(*entity).is_ok())
    })
}

pub(crate) fn clear_example(world: &mut World, root: Entity) {
    world
        .entity_mut(root)
        .remove::<(SavedCustom, AddedCustom)>();
}
