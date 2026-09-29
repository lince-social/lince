use super::*;
use lince_editor::recovery::{Draft, Store};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct Stamp {
    instance: u64,
    revision: u64,
    dirty: bool,
}

#[derive(Default)]
struct Progress {
    completed: Option<Stamp>,
    attempted: Option<Stamp>,
    pending: bool,
    failed: bool,
}

struct VisibleDraft {
    value: String,
    revision: u64,
    draft: Draft,
}

#[derive(Resource, Default)]
pub(super) struct Recovery {
    attempted: bool,
    ready: bool,
    store: Option<Arc<Store>>,
    progress: BTreeMap<PathBuf, Progress>,
    restored: Vec<PathBuf>,
    due: Option<Instant>,
    error: Option<String>,
    quitting: bool,
    visible: BTreeMap<PathBuf, VisibleDraft>,
    visible_revision: u64,
}

pub(super) fn ready(world: &World) -> bool {
    !world.contains_resource::<crate::workspace::WorkspaceFile>()
        || world
            .get_resource::<Recovery>()
            .is_some_and(|state| state.ready)
}

pub(super) fn error(world: &World) -> Option<&str> {
    world
        .get_resource::<Recovery>()
        .and_then(|state| state.error.as_deref())
}

pub(super) fn retry(world: &mut World) {
    if let Some(mut state) = world.get_resource_mut::<Recovery>() {
        state.error = None;
        if state.store.is_none() {
            state.attempted = false;
        }
        for progress in state.progress.values_mut() {
            progress.failed = false;
        }
    }
}

fn load(world: &mut World) {
    let Some(file) = world.get_resource::<crate::workspace::WorkspaceFile>() else {
        return;
    };
    let directory = file.directory().join("editor-drafts");
    if world.resource::<Recovery>().attempted {
        return;
    }
    let result = crate::file_explorer::worker::run(
        world,
        move || {
            let store = Arc::new(Store::open(&directory)?);
            let (drafts, mut errors) = store.load()?;
            let mut restored = Vec::new();
            for draft in drafts {
                match restore(draft.clone()) {
                    Ok(document) => restored.push((draft.path, document)),
                    Err(e) => errors.push(format!(
                        "{}: {e}; checkpoint retained",
                        draft.path.display()
                    )),
                }
            }
            Ok::<_, String>((store, restored, errors))
        },
        |world, result| {
            world.resource_mut::<Recovery>().ready = true;
            match result {
                Ok((store, restored, errors)) => {
                    let mut paths = Vec::new();
                    for (path, document) in restored {
                        paths.push(path.clone());
                        world
                            .resource_mut::<Documents>()
                            .0
                            .entry(path)
                            .or_insert(document);
                    }
                    let mut state = world.resource_mut::<Recovery>();
                    state.store = Some(store);
                    for path in &paths {
                        state.progress.entry(path.clone()).or_default();
                    }
                    state.restored.extend(paths);
                    state.error = (!errors.is_empty()).then(|| errors.join("; "));
                }
                Err(e) => world.resource_mut::<Recovery>().error = Some(e),
            }
        },
    );
    if result.is_ok() {
        world.resource_mut::<Recovery>().attempted = true;
    }
}

fn restore(draft: Draft) -> Result<Document, String> {
    let mut buffer = draft.checkpoint.restore()?;
    let file = if draft.detached {
        None
    } else {
        draft
            .path
            .parent()
            .and_then(|parent| Scope::open(parent).ok())
            .and_then(|scope| scope.bind(&draft.path).ok())
            .map(Arc::new)
    };
    let mut disk = Snapshot::recovered(buffer.observed().into(), draft.bom);
    let mut error = Some("Recovered unsaved draft".into());
    if let Some(file) = &file {
        match file.read() {
            Ok(current) => {
                let plan = buffer.reconciliation().compute(current.text.to_string())?;
                buffer.accept(plan)?;
                disk = current;
            }
            Err(e) => {
                error = Some(format!(
                    "Recovered draft; source unavailable: {e}. Use Save as…"
                ))
            }
        }
    } else {
        error = Some("Recovered draft; original location unavailable. Use Save as…".into());
    }
    Ok(Document {
        preview: None,
        buffer,
        file,
        disk,
        reading: false,
        saving: None,
        refresh: false,
        moving: false,
        error,
    })
}

fn attach(world: &mut World) {
    if world.resource::<Recovery>().restored.is_empty() {
        return;
    }
    let Some(root) = world
        .query_filtered::<Entity, With<crate::workspace::Workspaces>>()
        .iter(world)
        .next()
    else {
        return;
    };
    let paths = std::mem::take(&mut world.resource_mut::<Recovery>().restored);
    for path in paths {
        if world
            .query::<&Ide>()
            .iter(world)
            .any(|ide| ide.paths.contains(&path))
        {
            continue;
        }
        let owner = world
            .query::<(Entity, &Ide)>()
            .iter(world)
            .find(|(_, ide)| ide.paths.len() < 16)
            .map(|(entity, _)| entity);
        let owner = owner.unwrap_or_else(|| {
            let workspace = world
                .get::<crate::workspace::Workspaces>(root)
                .unwrap()
                .active;
            spawn(world, root, workspace, DVec2::ZERO, Ide::default())
        });
        let mut ide = world.get_mut::<Ide>(owner).unwrap();
        ide.paths.push(path.clone());
        if ide.active.is_none() {
            ide.active = Some(path);
        }
    }
}

fn desired(world: &World) -> BTreeMap<PathBuf, Stamp> {
    let mut desired: BTreeMap<_, _> = world
        .resource::<Documents>()
        .0
        .iter()
        .map(|(path, doc)| {
            (
                path.clone(),
                Stamp {
                    instance: doc.buffer.identity(),
                    revision: doc.buffer.revision(),
                    dirty: doc.buffer.is_dirty()
                        || doc.buffer.conflict().is_some()
                        || doc.file.is_none(),
                },
            )
        })
        .collect();
    for (path, visible) in &world.resource::<Recovery>().visible {
        desired.insert(
            path.clone(),
            Stamp {
                instance: 0,
                revision: visible.revision,
                dirty: true,
            },
        );
    }
    desired
}

fn capture_visible(world: &mut World) {
    let directory = world
        .resource::<crate::workspace::WorkspaceFile>()
        .directory()
        .join("editor-recovered-views");
    let views: Vec<_> = world
        .query::<(Entity, &View)>()
        .iter(world)
        .filter(|(_, view)| view.draft)
        .filter_map(|(entity, view)| {
            view.path.as_ref().map(|path| {
                (
                    directory
                        .join(entity.to_bits().to_string())
                        .join(path.file_name().unwrap_or_default()),
                    world
                        .get::<EditableText>(view.editor)
                        .unwrap()
                        .value()
                        .to_string(),
                )
            })
        })
        .collect();
    let mut state = world.resource_mut::<Recovery>();
    state
        .visible
        .retain(|path, _| views.iter().any(|(wanted, _)| wanted == path));
    for (path, value) in views {
        if state
            .visible
            .get(&path)
            .is_some_and(|previous| previous.value == value)
        {
            continue;
        }
        state.visible_revision += 1;
        let revision = state.visible_revision;
        match lince_editor::Checkpoint::unsaved(&value) {
            Ok(checkpoint) => {
                state.visible.insert(
                    path.clone(),
                    VisibleDraft {
                        value,
                        revision,
                        draft: Draft {
                            path,
                            checkpoint,
                            bom: false,
                            detached: true,
                        },
                    },
                );
            }
            Err(e) => state.error = Some(e),
        }
    }
}

pub(super) fn settled(world: &World) -> bool {
    if world
        .resource::<Documents>()
        .0
        .values()
        .any(|doc| doc.saving.is_some() || doc.moving)
    {
        return false;
    }
    let wanted = desired(world);
    let state = world.resource::<Recovery>();
    if !state.ready || state.store.is_none() {
        return false;
    }
    wanted.iter().all(|(path, stamp)| {
        !stamp.dirty
            || state
                .progress
                .get(path)
                .is_some_and(|progress| !progress.pending && progress.completed == Some(*stamp))
    }) && state.progress.iter().all(|(path, progress)| {
        !progress.pending
            && !progress.failed
            && (wanted.get(path).is_some_and(|stamp| stamp.dirty)
                || progress.completed.is_some_and(|stamp| !stamp.dirty))
    })
}

pub(super) fn protect_exit(world: &mut World) -> Option<bool> {
    if world
        .get_resource::<Recovery>()
        .is_none_or(|state| state.store.is_none())
    {
        return None;
    }
    capture_visible(world);
    if settled(world) {
        return Some(false);
    }
    let mut state = world.resource_mut::<Recovery>();
    state.quitting = true;
    state.due = Some(Instant::now());
    if let Some(wake) = world.get_resource::<crate::wake::WakeSignal>() {
        wake.ring();
    }
    Some(true)
}

pub(super) fn update(world: &mut World) {
    if !world.contains_resource::<crate::workspace::WorkspaceFile>() {
        return;
    }
    world.init_resource::<Recovery>();
    load(world);
    attach(world);
    capture_visible(world);
    let Some(store) = world.resource::<Recovery>().store.clone() else {
        return;
    };
    let mut wanted = desired(world);
    for path in world.resource::<Recovery>().progress.keys() {
        wanted.entry(path.clone()).or_default();
    }
    let pending: Vec<_> = wanted
        .into_iter()
        .filter(|(path, stamp)| {
            world
                .resource::<Recovery>()
                .progress
                .get(path)
                .map_or(stamp.dirty, |progress| {
                    !progress.pending
                        && progress.completed != Some(*stamp)
                        && !(progress.failed && progress.attempted == Some(*stamp))
                        && (stamp.dirty || progress.completed.is_none_or(|previous| previous.dirty))
                })
        })
        .collect();
    if !pending.is_empty() {
        let now = Instant::now();
        if world.resource::<Recovery>().due.is_none() {
            let delay = if world.resource::<Recovery>().quitting {
                Duration::ZERO
            } else {
                Duration::from_millis(500)
            };
            world.resource_mut::<Recovery>().due = Some(now + delay);
            if let Some(wake) = world.get_resource::<crate::wake::WakeSignal>() {
                wake.after(delay);
            }
        }
        if world
            .resource::<Recovery>()
            .due
            .is_some_and(|due| now >= due)
        {
            world.resource_mut::<Recovery>().due = None;
            for (path, stamp) in pending.into_iter().take(4) {
                let tracking = path.clone();
                let draft = stamp.dirty.then(|| {
                    if let Some(visible) = world.resource::<Recovery>().visible.get(&path) {
                        return visible.draft.clone();
                    }
                    let doc = &world.resource::<Documents>().0[&path];
                    Draft {
                        path: path.clone(),
                        checkpoint: doc.buffer.checkpoint(),
                        bom: doc.disk.bom,
                        detached: doc.file.is_none(),
                    }
                });
                let store = store.clone();
                let key = path.clone();
                let result = crate::file_explorer::worker::run(
                    world,
                    move || {
                        if let Some(draft) = draft {
                            store.write(&draft)
                        } else {
                            store.remove(&path)
                        }
                    },
                    move |world, result| {
                        let mut state = world.resource_mut::<Recovery>();
                        let removed = result.is_ok() && !stamp.dirty;
                        let progress = state.progress.entry(key.clone()).or_default();
                        progress.pending = false;
                        match result {
                            Ok(()) => {
                                progress.completed = Some(stamp);
                                progress.failed = false;
                            }
                            Err(e) => {
                                progress.failed = true;
                                let closing = state.quitting;
                                state.quitting = false;
                                state.error = Some(format!(
                                    "Could not save a recovery copy: {e}{}",
                                    if closing { "; close canceled" } else { "" }
                                ));
                            }
                        }
                        if removed {
                            state.progress.remove(&key);
                        }
                    },
                );
                if result.is_ok() {
                    let mut state = world.resource_mut::<Recovery>();
                    let progress = state.progress.entry(tracking).or_default();
                    progress.pending = true;
                    progress.attempted = Some(stamp);
                }
            }
        }
    }
    if world.resource::<Recovery>().quitting && settled(world) {
        world.resource_mut::<Recovery>().quitting = false;
        world.write_message(AppExit::Success);
    }
}
