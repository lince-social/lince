use super::*;
use crate::actions::Action;
use bevy::text::EditableText;
use std::sync::atomic::Ordering;

#[derive(Clone)]
pub(super) enum Control {
    Create,
    Add,
    NewFile,
    Choose,
    Refresh,
    Back,
    Up,
    OpenFolder,
    SelectFolder,
    Search,
    Tree,
    Grid,
    Ignored,
    Extract,
    Cancel,
    Entry(Row),
    RemoveRoot(PathBuf),
}

impl Action for Control {
    fn apply(&self, world: &mut World, owner: Entity) {
        if crate::laboratory::suspended(world, owner) {
            return;
        }
        if matches!(self, Self::Create) {
            if let Some((root, workspace, position)) = location(world, owner) {
                spawn(world, root, workspace, position, FileExplorer::default());
            }
            return;
        }
        let Some(view) = world.get::<View>(owner) else {
            return;
        };
        let (input, search) = (view.input, view.search);
        match self {
            Self::Back | Self::Up | Self::OpenFolder => {
                navigation::navigate(world, owner, self);
            }
            Self::SelectFolder => {
                let path = view
                    .selected
                    .as_ref()
                    .filter(|row| row.entry.directory)
                    .map(|row| row.entry.path.clone())
                    .or_else(|| view.directory.clone());
                let target = view.target.clone();
                if let Some(path) = path {
                    select(world, owner, &path, target);
                } else {
                    status(world, owner, "Choose a folder first");
                }
            }
            Self::Add => {
                if let Ok(path) = crate::sand_panel::value(world, input) {
                    add(world, owner, PathBuf::from(path), true);
                }
            }
            Self::NewFile => {
                let Ok(path) = crate::sand_panel::value(world, input) else {
                    return;
                };
                let path = PathBuf::from(path);
                let Some(scope) = view
                    .scopes
                    .values()
                    .filter(|scope| path.starts_with(&scope.path))
                    .max_by_key(|scope| scope.path.components().count())
                    .cloned()
                else {
                    status(
                        world,
                        owner,
                        "Enter a new file's full path inside a selected root",
                    );
                    return;
                };
                let result = worker::run(
                    world,
                    move || scope.create(&path).map(|file| file.path),
                    move |world, result| {
                        let Some(target) = world.get::<View>(owner).map(|view| view.target.clone())
                        else {
                            return;
                        };
                        match result {
                            Ok(path) => {
                                reset(world, owner);
                                select(world, owner, &path, target);
                            }
                            Err(e) => status(world, owner, e),
                        }
                    },
                );
                if let Err(e) = result {
                    status(world, owner, e);
                }
            }
            Self::Choose => {
                if view.picking {
                    return;
                }
                let result = worker::run(
                    world,
                    || rfd::FileDialog::new().pick_folders(),
                    move |world, paths| {
                        if let Some(mut view) = world.get_mut::<View>(owner) {
                            view.picking = false;
                        } else {
                            return;
                        }
                        for path in paths.unwrap_or_default().into_iter().take(16) {
                            add(world, owner, path, true);
                        }
                    },
                );
                match result {
                    Ok(()) => world.get_mut::<View>(owner).unwrap().picking = true,
                    Err(e) => status(world, owner, e),
                }
            }
            Self::Refresh | Self::Ignored => {
                if matches!(self, Self::Ignored) {
                    let mut config = world.get_mut::<FileExplorer>(owner).unwrap();
                    config.ignored = !config.ignored;
                }
                reset(world, owner);
                status(
                    world,
                    owner,
                    if world.get::<FileExplorer>(owner).unwrap().ignored {
                        "Showing ignored paths"
                    } else {
                        "Respecting .gitignore"
                    },
                );
            }
            Self::Tree | Self::Grid => {
                world.get_mut::<FileExplorer>(owner).unwrap().grid = matches!(self, Self::Grid);
                let mut view = world.get_mut::<View>(owner).unwrap();
                if matches!(self, Self::Tree) {
                    view.cancellation.store(true, Ordering::Relaxed);
                    view.generation += 1;
                    view.pending.clear();
                    view.search_rows = None;
                }
                view.dirty = true;
            }
            Self::Search => {
                if let Ok(query) = crate::sand_panel::value(world, search) {
                    search_paths(world, owner, query);
                }
            }
            Self::Entry(row) => {
                let target = view.target.clone();
                let viewport = view.viewport;
                let label = view.selected_label;
                let mut view = world.get_mut::<View>(owner).unwrap();
                if view
                    .selected
                    .as_ref()
                    .is_none_or(|selected| selected.entry.path != row.entry.path)
                {
                    view.confirmation = None;
                }
                view.selected = Some(row.clone());
                view.shown = None;
                crate::sand_panel::status(world, label, row.entry.path.display().to_string());
                if let Some(mut focus) = world.get_resource_mut::<bevy::input_focus::InputFocus>() {
                    focus.set(viewport, bevy::input_focus::FocusCause::Navigated);
                }
                if row.entry.directory {
                    if world.get::<FileExplorer>(owner).unwrap().grid {
                        navigation::enter(world, owner, Some(row.entry.path.clone()), true);
                    } else {
                        let mut view = world.get_mut::<View>(owner).unwrap();
                        if !view.expanded.remove(&row.entry.path) {
                            view.cache.remove(&row.entry.path);
                            view.expanded.insert(row.entry.path.clone());
                        } else {
                            view.expanded
                                .retain(|path| !path.starts_with(&row.entry.path));
                        }
                        view.dirty = true;
                    }
                } else {
                    if matches!(
                        target,
                        Target::Input {
                            directories: true,
                            ..
                        }
                    ) {
                        status(world, owner, "Choose a folder, then press Select folder");
                    } else {
                        select(world, owner, &row.entry.path, target);
                    }
                }
            }
            Self::RemoveRoot(path) => {
                world
                    .get_mut::<FileExplorer>(owner)
                    .unwrap()
                    .roots
                    .retain(|root| root != path);
                let mut view = world.get_mut::<View>(owner).unwrap();
                view.scopes.remove(path);
                view.cancellation.store(true, Ordering::Relaxed);
                view.generation += 1;
                view.pending.clear();
                view.cache.retain(|p, _| !p.starts_with(path));
                view.expanded.retain(|p| !p.starts_with(path));
                view.search_rows = None;
                if view
                    .directory
                    .as_ref()
                    .is_some_and(|directory| directory.starts_with(path))
                {
                    view.directory = None;
                }
                view.history.clear();
                view.dirty = true;
            }
            Self::Extract => {
                let config = world.get::<FileExplorer>(owner).unwrap().clone();
                if let Some((root, workspace, position)) = location(world, owner) {
                    spawn(world, root, workspace, position, config);
                }
            }
            Self::Cancel => {
                world.despawn(owner);
            }
            Self::Create => {}
        }
    }
}

pub(super) fn add(world: &mut World, owner: Entity, path: PathBuf, announce: bool) {
    if world
        .get::<FileExplorer>(owner)
        .is_none_or(|config| config.roots.len() >= 16 && !config.roots.contains(&path))
    {
        status(world, owner, "At most 16 roots can be open");
        return;
    }
    let generation = world.get::<View>(owner).unwrap().generation;
    let result = worker::run(
        world,
        move || Scope::open(&path),
        move |world, result| {
            if world
                .get::<View>(owner)
                .is_none_or(|view| view.generation != generation)
            {
                return;
            }
            match result {
                Ok(scope) => {
                    if scope.path.to_str().is_none() {
                        status(world, owner, "Workspace roots require UTF-8 paths");
                        return;
                    }
                    let mut config = world.get_mut::<FileExplorer>(owner).unwrap();
                    if !config.roots.contains(&scope.path) {
                        if config.roots.len() >= 16 {
                            return;
                        }
                        config.roots.push(scope.path.clone());
                    }
                    let mut view = world.get_mut::<View>(owner).unwrap();
                    view.expanded.insert(scope.path.clone());
                    view.scopes.insert(scope.path.clone(), scope);
                    view.dirty = true;
                    let label = view.status;
                    if announce
                        || world
                            .get::<Text>(label)
                            .is_some_and(|text| text.0 == "Opening directories…")
                    {
                        status(world, owner, "Ready");
                    }
                }
                Err(e) => status(world, owner, e),
            }
        },
    );
    if let Err(e) = result {
        status(world, owner, e);
    }
}

pub(super) fn reset(world: &mut World, owner: Entity) {
    let mut view = world.get_mut::<View>(owner).unwrap();
    view.cancellation.store(true, Ordering::Relaxed);
    view.generation += 1;
    view.cache.clear();
    view.pending.clear();
    view.search_rows = None;
    view.dirty = true;
}

fn search_paths(world: &mut World, owner: Entity, query: String) {
    reset(world, owner);
    if query.trim().is_empty() {
        return;
    }
    let ignored = world.get::<FileExplorer>(owner).unwrap().ignored;
    let mut view = world.get_mut::<View>(owner).unwrap();
    let generation = view.generation;
    let scopes: Vec<_> = view.scopes.values().cloned().collect();
    let cancel = Arc::new(AtomicBool::new(false));
    view.cancellation = cancel.clone();
    let result = worker::run(
        world,
        move || {
            let mut rows = Vec::new();
            let mut truncated = false;
            for scope in scopes {
                let listing = lince_editor::explorer::search(&scope, &query, ignored, &cancel)?;
                truncated |= listing.truncated;
                rows.extend(listing.entries.into_iter().map(|entry| Row {
                    entry,
                    root: scope.path.clone(),
                    depth: 0,
                }));
            }
            Ok::<_, String>((rows, truncated))
        },
        move |world, result| {
            if world
                .get::<View>(owner)
                .is_none_or(|view| view.generation != generation)
            {
                return;
            }
            match result {
                Ok((rows, truncated)) => {
                    status(
                        world,
                        owner,
                        format!(
                            "{} paths{}",
                            rows.len(),
                            if truncated {
                                " · results limited; narrow the search"
                            } else {
                                ""
                            }
                        ),
                    );
                    let mut view = world.get_mut::<View>(owner).unwrap();
                    view.search_rows = Some(rows);
                    view.dirty = true;
                }
                Err(e) => status(world, owner, e),
            }
        },
    );
    match result {
        Ok(()) => status(world, owner, "Searching…"),
        Err(e) => status(world, owner, e),
    }
}

fn select(world: &mut World, owner: Entity, path: &PathBuf, target: Target) {
    match target {
        Target::Editor(editor) => {
            let Some(scope) = world
                .get::<View>(owner)
                .and_then(|v| {
                    v.scopes
                        .values()
                        .filter(|s| path.starts_with(&s.path))
                        .max_by_key(|s| s.path.components().count())
                })
                .cloned()
            else {
                return;
            };
            crate::ide::open(world, editor, scope, path.clone());
        }
        Target::Standalone => {
            let config = world.get::<FileExplorer>(owner).unwrap().clone();
            let Some(scope) = world
                .get::<View>(owner)
                .and_then(|v| {
                    v.scopes
                        .values()
                        .filter(|s| path.starts_with(&s.path))
                        .max_by_key(|s| s.path.components().count())
                })
                .cloned()
            else {
                return;
            };
            if let Some((root, workspace, position)) = location(world, owner) {
                let editor = crate::ide::spawn(
                    world,
                    root,
                    workspace,
                    position,
                    crate::ide::Ide {
                        explorer: config,
                        ..default()
                    },
                );
                crate::ide::open(world, editor, scope, path.clone());
            }
        }
        Target::Input {
            entity,
            original,
            extensions,
            directories,
        } => {
            if !directories
                && !extensions.is_empty()
                && !path
                    .extension()
                    .and_then(|s| s.to_str())
                    .is_some_and(|ext| {
                        extensions
                            .iter()
                            .any(|allowed| allowed.eq_ignore_ascii_case(ext))
                    })
            {
                status(
                    world,
                    owner,
                    format!("Choose a {} file", extensions.join(" / ")),
                );
                return;
            }
            let Some(value) = path.to_str() else {
                status(
                    world,
                    owner,
                    "This input cannot represent a path with invalid UTF-8",
                );
                return;
            };
            match world.get_mut::<EditableText>(entity) {
                Some(mut input)
                    if !input.is_composing() && input.value().to_string() == original =>
                {
                    input.editor.set_text(value);
                    world.despawn(owner);
                }
                _ => status(
                    world,
                    owner,
                    "The path input changed or closed; cancel and reopen the picker",
                ),
            }
        }
    }
}

#[derive(Clone)]
pub struct BrowseFor {
    pub input: Entity,
    pub extensions: Vec<String>,
    pub directories: bool,
}

impl Action for BrowseFor {
    fn apply(&self, world: &mut World, owner: Entity) {
        if crate::laboratory::suspended(world, owner) {
            return;
        }
        let Ok(original) = crate::sand_panel::value(world, self.input) else {
            return;
        };
        let Some((root, workspace, position)) = location(world, owner) else {
            return;
        };
        let picker = shell(
            world,
            root,
            workspace,
            position,
            "Choose a path",
            Vec2::new(620.0, 740.0),
        );
        let path = PathBuf::from(&original);
        let initial = if path.is_absolute() {
            path.parent().map(PathBuf::from)
        } else {
            std::env::current_dir().ok()
        };
        populate(
            world,
            picker,
            FileExplorer {
                roots: initial.into_iter().collect(),
                ..default()
            },
            Target::Input {
                entity: self.input,
                original,
                extensions: self.extensions.clone(),
                directories: self.directories,
            },
        );
    }
}
