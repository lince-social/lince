use crate::actions::Action;
use bevy::{prelude::*, text::EditableText};
use std::{
    path::PathBuf,
    sync::{Mutex, mpsc},
};

#[derive(Component)]
struct Picking(Mutex<mpsc::Receiver<Option<PathBuf>>>);

pub(crate) fn create(world: &mut World, parent: Entity, paths: &[PathBuf]) -> Entity {
    crate::edit_mode::label(
        world,
        parent,
        "Additional directories · agent machine",
        14.0,
    );
    let text = paths
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join("\n");
    let input = world
        .spawn((
            crate::sand::text_editor(&text, world.resource::<crate::theme::Typography>(), 0),
            ChildOf(parent),
        ))
        .insert(Node {
            min_height: px(42),
            width: percent(100),
            ..default()
        })
        .id();
    world.get_mut::<EditableText>(input).unwrap().allow_newlines = true;
    crate::description::button(world, parent, input, "Browse and add directory", Browse);
    crate::edit_mode::label(
        world,
        parent,
        "One full path per line. Delete a line to remove it. These are context roots, not uploads or a sandbox.",
        12.0,
    );
    input
}

pub(crate) fn paths(world: &World, input: Entity) -> Vec<PathBuf> {
    world
        .get::<EditableText>(input)
        .map(|text| {
            text.value()
                .to_string()
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(PathBuf::from)
                .collect()
        })
        .unwrap_or_default()
}

#[derive(Clone)]
struct Browse;
impl Action for Browse {
    fn apply(&self, world: &mut World, input: Entity) {
        if world.get::<Picking>(input).is_some() {
            return;
        }
        let (send, receive) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = send.send(rfd::FileDialog::new().pick_folder());
        });
        world.entity_mut(input).insert(Picking(Mutex::new(receive)));
    }
}

pub(crate) fn poll(world: &mut World) {
    let ready: Vec<_> = world
        .query::<(Entity, &Picking)>()
        .iter(world)
        .filter_map(|(entity, pending)| {
            pending
                .0
                .lock()
                .ok()?
                .try_recv()
                .ok()
                .map(|path| (entity, path))
        })
        .collect();
    for (entity, path) in ready {
        world.entity_mut(entity).remove::<Picking>();
        if let (Some(path), Some(mut text)) = (path, world.get_mut::<EditableText>(entity)) {
            let old = text.value().to_string();
            let path = path.display().to_string();
            if !old.lines().any(|line| line == path) {
                text.editor.set_text(&format!(
                    "{}{path}",
                    if old.is_empty() {
                        String::new()
                    } else {
                        format!("{old}\n")
                    }
                ));
            }
        }
    }
}
