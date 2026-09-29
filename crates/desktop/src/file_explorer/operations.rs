use super::*;
use crate::actions::Action;
use std::path::{Component, Path};

#[derive(Clone, Copy)]
pub(super) enum Control {
    Tools,
    NewFolder,
    Rename,
    Move,
    Delete,
    UndoDelete,
}

enum Changed {
    Folder(PathBuf),
    Moved(PathBuf, PathBuf),
    Deleted(Scope, lince_editor::operations::TrashTicket),
    Restored(PathBuf),
}

fn scope_for(world: &World, owner: Entity, path: &Path) -> Result<Scope, String> {
    world
        .get::<View>(owner)
        .and_then(|view| {
            view.scopes
                .values()
                .filter(|scope| path.starts_with(&scope.path))
                .max_by_key(|scope| scope.path.components().count())
        })
        .cloned()
        .ok_or_else(|| "Choose a path inside a selected root".into())
}

impl Action for Control {
    fn apply(&self, world: &mut World, owner: Entity) {
        if crate::laboratory::suspended(world, owner) {
            return;
        }
        if let Err(e) = apply(world, owner, *self) {
            status(world, owner, e);
        }
    }
}

fn apply(world: &mut World, owner: Entity, action: Control) -> Result<(), String> {
    let Some(view) = world.get::<View>(owner) else {
        return Ok(());
    };
    if view.busy {
        return Err("Wait for the current file operation".into());
    }
    if matches!(view.target, Target::Input { .. }) {
        return Err("Open a File Explorer Castle to change files".into());
    }
    if matches!(action, Control::Tools) {
        let panel = view.operations;
        let mut node = world.get_mut::<Node>(panel).unwrap();
        node.display = if node.display == Display::None {
            Display::Flex
        } else {
            Display::None
        };
        return Ok(());
    }
    let selected = view.selected.clone();
    let destination = crate::sand_panel::value(world, view.destination)?;
    if matches!(action, Control::NewFolder) {
        let path = PathBuf::from(destination.trim());
        let path = if path.is_absolute() {
            path
        } else {
            let selected = selected
                .as_ref()
                .ok_or("Select a parent folder or enter a full path")?;
            if !selected.entry.directory {
                return Err("Select a parent folder".into());
            }
            selected.entry.path.join(path)
        };
        let scope = scope_for(world, owner, &path)?;
        return run(world, owner, None, move || {
            scope.create_directory(&path)?;
            Ok((Changed::Folder(path), BTreeMap::new()))
        });
    }
    if matches!(action, Control::UndoDelete) {
        let (scope, ticket) = view
            .trash
            .clone()
            .ok_or("There is no deletion to undo in this Explorer")?;
        let affected = crate::ide::reserve_change(world, &ticket.original, false)?;
        let source = ticket.original.clone();
        return run(world, owner, Some(source), move || {
            let restored = scope.restore_trash(&ticket)?;
            let files = bindings(&affected, &restored, &restored);
            Ok((Changed::Restored(restored), files))
        });
    }
    let selected = selected.ok_or("Select a file or folder first")?;
    let source = selected.entry.path;
    if source == selected.root {
        return Err("Selected roots cannot be renamed, moved, or deleted".into());
    }
    let scope = scope_for(world, owner, &source)?;
    if matches!(action, Control::Delete) {
        if let Some(entry) = view
            .confirmation
            .clone()
            .filter(|entry| entry.path == source)
        {
            crate::ide::reserve_change(world, &source, true)?;
            return run(world, owner, Some(source), move || {
                let ticket = scope.trash(&entry)?;
                Ok((Changed::Deleted(scope, ticket), BTreeMap::new()))
            });
        }
        let path = source.clone();
        let result = worker::run(
            world,
            move || scope.entry(&path).map(Arc::new),
            move |world, result| {
                let Some(mut view) = world.get_mut::<View>(owner) else {
                    return;
                };
                view.busy = false;
                if view
                    .selected
                    .as_ref()
                    .is_none_or(|selected| selected.entry.path != source)
                {
                    return;
                }
                match result {
                    Ok(entry) => {
                        let directory = entry.directory;
                        view.confirmation = Some(entry);
                        status(
                            world,
                            owner,
                            format!(
                                "Delete {}{}? Press Delete… again to move it into .lince-trash. Undo delete restores it.",
                                source.display(),
                                if directory { " and its contents" } else { "" }
                            ),
                        );
                    }
                    Err(e) => status(world, owner, e),
                }
            },
        );
        if result.is_ok() {
            world.get_mut::<View>(owner).unwrap().busy = true;
        }
        return result;
    }
    let target = if matches!(action, Control::Rename) {
        let name = Path::new(destination.trim());
        if name.components().count() != 1
            || !matches!(name.components().next(), Some(Component::Normal(_)))
        {
            return Err("Rename needs a single file or folder name".into());
        }
        source.parent().unwrap().join(name)
    } else {
        let path = PathBuf::from(destination.trim());
        if !path.is_absolute() {
            return Err("Move needs the full destination path, including the new name".into());
        }
        path
    };
    let target_scope = scope_for(world, owner, &target)?;
    let affected = crate::ide::reserve_change(world, &source, false)?;
    run(world, owner, Some(source.clone()), move || {
        let entry = scope.entry(&source)?;
        let target = target_scope.move_entry(&entry, &target)?;
        let files = bindings(&affected, &source, &target);
        Ok((Changed::Moved(source, target), files))
    })
}

type Bindings = BTreeMap<PathBuf, Option<Arc<lince_editor::files::FileBinding>>>;

fn bindings(paths: &[PathBuf], from: &Path, to: &Path) -> Bindings {
    paths
        .iter()
        .map(|path| {
            let destination = lince_editor::operations::relocated(path, from, to).unwrap();
            let file = destination
                .parent()
                .and_then(|parent| Scope::open(parent).ok())
                .and_then(|scope| scope.bind(&destination).ok())
                .map(Arc::new);
            (path.clone(), file)
        })
        .collect()
}

fn run(
    world: &mut World,
    owner: Entity,
    source: Option<PathBuf>,
    work: impl FnOnce() -> Result<(Changed, Bindings), String> + Send + 'static,
) -> Result<(), String> {
    let failed = source.clone();
    let result = worker::run(world, work, move |world, result| {
        if let Some(mut view) = world.get_mut::<View>(owner) {
            view.busy = false;
            view.confirmation = None;
        }
        match result {
            Ok((change, files)) => match change {
                Changed::Folder(path) => {
                    refresh(world, None, None);
                    status(world, owner, format!("Created {}", path.display()));
                }
                Changed::Moved(from, to) => {
                    crate::ide::finish_change(world, &from, Some(&to), files, true);
                    refresh(world, Some(&from), Some(&to));
                    status(world, owner, format!("Moved to {}", to.display()));
                }
                Changed::Deleted(scope, ticket) => {
                    crate::ide::finish_change(world, &ticket.original, None, files, true);
                    refresh(world, Some(&ticket.original), None);
                    status(
                        world,
                        owner,
                        format!(
                            "Moved to {}. Undo delete restores it.",
                            ticket.stored.display()
                        ),
                    );
                    if let Some(mut view) = world.get_mut::<View>(owner) {
                        view.trash = Some((scope, ticket));
                    }
                }
                Changed::Restored(path) => {
                    crate::ide::finish_change(world, &path, Some(&path), files, true);
                    refresh(world, None, None);
                    if let Some(mut view) = world.get_mut::<View>(owner) {
                        view.trash = None;
                    }
                    status(world, owner, format!("Restored {}", path.display()));
                }
            },
            Err(e) => {
                if let Some(path) = source {
                    crate::ide::finish_change(world, &path, None, BTreeMap::new(), false);
                }
                status(world, owner, e);
            }
        }
    });
    if result.is_ok() {
        world.get_mut::<View>(owner).unwrap().busy = true;
    } else if let Some(path) = failed {
        crate::ide::finish_change(world, &path, None, BTreeMap::new(), false);
    }
    result
}

fn refresh(world: &mut World, from: Option<&Path>, to: Option<&Path>) {
    let owners: Vec<_> = world
        .query_filtered::<Entity, With<View>>()
        .iter(world)
        .collect();
    for owner in owners {
        if let Some(from) = from {
            let mut config = world.get_mut::<FileExplorer>(owner).unwrap();
            config.roots = config
                .roots
                .iter()
                .filter_map(|root| {
                    if root.starts_with(from) {
                        to.and_then(|to| lince_editor::operations::relocated(root, from, to))
                    } else {
                        Some(root.clone())
                    }
                })
                .collect();
        }
        actions::reset(world, owner);
        let mut view = world.get_mut::<View>(owner).unwrap();
        view.scopes.clear();
        view.expanded.clear();
        view.restore = true;
        view.selected = None;
        view.confirmation = None;
        let label = view.selected_label;
        crate::sand_panel::status(world, label, "Select a file or folder");
    }
}
