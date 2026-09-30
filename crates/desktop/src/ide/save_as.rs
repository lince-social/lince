use super::*;
use lince_editor::files::SaveDestination;

pub(super) struct Pending {
    source: PathBuf,
    destination: SaveDestination,
}

pub(super) fn inspect(world: &mut World, owner: Entity, source: PathBuf, destination: PathBuf) {
    if destination == source {
        actions::save(world, owner, source);
        return;
    }
    if destination.to_str().is_none() {
        status(world, owner, "Choose a UTF-8 path");
        return;
    }
    confirm(world, owner, false);
    let result = crate::file_explorer::worker::run(
        world,
        move || SaveDestination::inspect(&destination),
        move |world, result| {
            if world.get::<View>(owner).is_none()
                || world
                    .get::<Ide>(owner)
                    .is_none_or(|ide| !ide.paths.contains(&source))
            {
                return;
            }
            match result {
                Ok(destination) => {
                    if world
                        .resource::<Documents>()
                        .0
                        .contains_key(&destination.path)
                    {
                        status(
                            world,
                            owner,
                            "Close the destination file before replacing it",
                        );
                    } else if destination.exists() {
                        status(
                            world,
                            owner,
                            format!(
                                "Replace {} with {}?",
                                destination.path.display(),
                                source.display()
                            ),
                        );
                        let mut view = world.get_mut::<View>(owner).unwrap();
                        view.replacement = Some(Pending {
                            source,
                            destination,
                        });
                        let panel = view.replace_panel;
                        world.get_mut::<Node>(panel).unwrap().display = Display::Flex;
                    } else {
                        save(world, owner, source, destination);
                    }
                }
                Err(error) => status(world, owner, error),
            }
        },
    );
    if let Err(error) = result {
        status(world, owner, error);
    }
}

pub(super) fn confirm(world: &mut World, owner: Entity, accepted: bool) {
    let Some(mut view) = world.get_mut::<View>(owner) else {
        return;
    };
    let pending = view.replacement.take();
    let panel = view.replace_panel;
    world.get_mut::<Node>(panel).unwrap().display = Display::None;
    if accepted {
        if let Some(pending) = pending {
            save(world, owner, pending.source, pending.destination);
        }
    }
}

fn save(world: &mut World, owner: Entity, path: PathBuf, destination: SaveDestination) {
    editing::capture_one(world, owner);
    let Some(view) = world.get::<View>(owner) else {
        return;
    };
    if view.draft
        || world
            .get::<EditableText>(view.editor)
            .is_some_and(|input| input.is_composing())
    {
        status(
            world,
            owner,
            "Finish composing or resolve the visible draft before saving",
        );
        return;
    }
    let Some(document) = world.resource::<Documents>().0.get(&path) else {
        return;
    };
    if document.preview.is_some() {
        return;
    }
    if document.moving
        || world.resource::<Documents>().1.contains(&destination.path)
        || document.reading
        || document.saving.is_some()
        || world
            .resource::<Documents>()
            .0
            .contains_key(&destination.path)
    {
        status(
            world,
            owner,
            "Wait for file access and choose a path that is not already open",
        );
        return;
    }
    let point = match document.buffer.prepare_save() {
        Ok(point) => point,
        Err(e) => {
            status(world, owner, e);
            return;
        }
    };
    let revision = point.revision;
    let bom = document.disk.bom;
    let reply_path = path.clone();
    let reserved = destination.path.clone();
    let finished = reserved.clone();
    let result = crate::file_explorer::worker::run(
        world,
        move || {
            let point = point.encode();
            let (file, disk) = destination.save(&point.text.to_string(), bom)?;
            Ok::<_, String>((point, file, disk))
        },
        move |world, result| {
            let mut documents = world.resource_mut::<Documents>();
            documents.1.remove(&finished);
            let Some(document) = documents.0.get_mut(&reply_path) else {
                return;
            };
            let Some(_) = document.saving.take() else {
                return;
            };
            match result {
                Ok((point, file, disk)) => {
                    let destination = file.path.clone();
                    let mut document = documents.0.remove(&reply_path).unwrap();
                    document.file = Some(file);
                    let text = disk.text.clone();
                    document.disk = disk;
                    document.error = document.buffer.saved_with_text(point, text).err();
                    documents.0.insert(destination.clone(), document);
                    tabs::relocate(world, &reply_path, &destination);
                    for mut ide in world.query::<&mut Ide>().iter_mut(world) {
                        for path in &mut ide.paths {
                            if *path == reply_path {
                                *path = destination.clone();
                            }
                        }
                        if ide.active.as_ref() == Some(&reply_path) {
                            ide.active = Some(destination.clone());
                        }
                    }
                    status(world, owner, "Saved to the new path");
                }
                Err(e) => status(world, owner, e),
            }
        },
    );
    match result {
        Ok(()) => {
            world.resource_mut::<Documents>().1.insert(reserved);
            world
                .resource_mut::<Documents>()
                .0
                .get_mut(&path)
                .unwrap()
                .saving = Some(revision)
        }
        Err(e) => status(world, owner, e),
    }
}
