use super::*;
use crate::host::performance::{self, Count};

#[derive(Resource, Default)]
pub(super) struct Watching {
    watch: Option<Arc<std::sync::Mutex<lince_editor::watch::Watch>>>,
    pending: Option<Arc<std::sync::Mutex<lince_editor::watch::Changes>>>,
    wanted: BTreeSet<PathBuf>,
    updating: bool,
    fallback: bool,
    attempted: bool,
    refresh: bool,
}

pub(super) fn focus(
    mut events: MessageReader<bevy::window::WindowFocused>,
    mut watching: ResMut<Watching>,
) {
    if events.read().any(|event| event.focused) {
        watching.refresh = true;
    }
}

pub(super) fn update(world: &mut World) {
    worker::poll(world);
    let owners: Vec<_> = world
        .query_filtered::<Entity, With<View>>()
        .iter(world)
        .collect();
    watches(world, &owners);
    for owner in owners.iter().copied() {
        if crate::native::laboratory::blocked(world, owner) {
            continue;
        }
        if world.get::<View>(owner).unwrap().restore {
            world.get_mut::<View>(owner).unwrap().restore = false;
            let roots = world.get::<FileExplorer>(owner).unwrap().roots.clone();
            for root in roots {
                actions::add(world, owner, root, false);
            }
        }
        request(world, owner);
        super::presentation::render(world, owner);
    }
}

fn request(world: &mut World, owner: Entity) {
    let mut view = world.get_mut::<View>(owner).unwrap();
    let abandoned: Vec<_> = view
        .listings
        .keys()
        .filter(|path| {
            view.search_results || !view.pending.contains(*path) || !view.expanded.contains(*path)
        })
        .cloned()
        .collect();
    for path in abandoned {
        if let Some(cancel) = view.listings.remove(&path) {
            cancel.store(true, std::sync::atomic::Ordering::Relaxed);
            performance::count(Count::ExplorerDirectoryCancellations);
        }
        view.pending.remove(&path);
        view.cache.remove(&path);
        view.pending_sorts.remove(&path);
    }
    let view = world.get::<View>(owner).unwrap();
    if view.search_results {
        return;
    }
    let ignored = world.get::<FileExplorer>(owner).unwrap().ignored;
    let generation = view.generation;
    let requests: Vec<_> = view
        .expanded
        .iter()
        .filter(|path| !view.cache.contains_key(*path) && !view.pending.contains(*path))
        .filter_map(|path| {
            view.scopes
                .values()
                .filter(|scope| path.starts_with(&scope.path))
                .max_by_key(|scope| scope.path.components().count())
                .map(|scope| (path.clone(), scope.clone()))
        })
        .take(4)
        .collect();
    for (path, scope) in requests {
        let input = path.clone();
        let reply_path = path.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let cancellation = cancel.clone();
        let request = cancel.clone();
        let result = worker::stream(
            world,
            move |emit| {
                if let Err(error) = lince_editor::explorer::list_batches(
                    &scope,
                    &input,
                    ignored,
                    &cancellation,
                    |batch| emit(Ok(batch)),
                ) {
                    emit(Err(error));
                }
            },
            move |world, result| {
                if world.get::<View>(owner).is_none_or(|view| {
                    view.generation != generation
                        || !view.pending.contains(&reply_path)
                        || !view.expanded.contains(&reply_path)
                        || view
                            .listings
                            .get(&reply_path)
                            .is_none_or(|active| !Arc::ptr_eq(active, &request))
                }) {
                    return;
                }
                let mut view = world.get_mut::<View>(owner).unwrap();
                match result {
                    Ok(batch) => {
                        let limited = batch.truncated;
                        if batch.finished {
                            view.pending.remove(&reply_path);
                            view.listings.remove(&reply_path);
                        }
                        let listing =
                            view.cache
                                .entry(reply_path.clone())
                                .or_insert_with(|| Listing {
                                    entries: Vec::new(),
                                    truncated: false,
                                });
                        let needs_sort = !listing.entries.is_empty() && !batch.entries.is_empty();
                        listing.entries.extend(batch.entries);
                        listing.truncated = limited;
                        if needs_sort {
                            view.pending_sorts.insert(reply_path.clone());
                        }
                        view.dirty = true;
                        if limited {
                            status(
                                world,
                                owner,
                                "Directory limited to 20,000 entries; use path search",
                            );
                        }
                    }
                    Err(e) => {
                        view.pending.remove(&reply_path);
                        view.listings.remove(&reply_path);
                        view.expanded.remove(&reply_path);
                        view.cache.remove(&reply_path);
                        view.pending_sorts.remove(&reply_path);
                        view.dirty = true;
                        status(world, owner, e);
                    }
                }
            },
        );
        if result.is_ok() {
            performance::count(Count::ExplorerDirectoryRequests);
            let mut view = world.get_mut::<View>(owner).unwrap();
            view.pending.insert(path.clone());
            view.listings.insert(path, cancel);
        }
    }
}

fn watches(world: &mut World, owners: &[Entity]) {
    let mut wanted = crate::native::ide::watched(world);
    for owner in owners {
        if crate::native::laboratory::blocked(world, *owner) {
            continue;
        }
        if let Some(view) = world.get::<View>(*owner) {
            wanted.extend(view.scopes.keys().cloned());
            wanted.extend(
                view.expanded
                    .iter()
                    .filter(|path| view.scopes.keys().any(|root| path.starts_with(root)))
                    .cloned(),
            );
        }
    }
    let wake = world
        .get_resource::<crate::native::wake::WakeSignal>()
        .cloned();
    let mut watching = world.resource_mut::<Watching>();
    let refresh = std::mem::take(&mut watching.refresh);
    if !watching.attempted && !wanted.is_empty() {
        watching.attempted = true;
        match lince_editor::watch::Watch::new(move || {
            if let Some(wake) = &wake {
                wake.ring();
            }
        }) {
            Ok(watch) => {
                watching.pending = Some(watch.pending());
                watching.watch = Some(Arc::new(std::sync::Mutex::new(watch)));
            }
            Err(e) => {
                for owner in owners {
                    status(
                        world,
                        *owner,
                        format!("File watching unavailable: {e}. Use Refresh."),
                    );
                }
                return;
            }
        }
    }
    let mut changes = watching
        .pending
        .as_ref()
        .map(|pending| std::mem::take(&mut *pending.lock().expect("watch events")))
        .unwrap_or_default();
    changes.rescan |= refresh;
    watching.fallback |= changes.error.take().is_some();
    if !watching.updating && (watching.wanted != wanted || watching.fallback) {
        let fallback = watching.fallback;
        let watcher = watching.watch.clone();
        drop(watching);
        if let Some(watcher) = watcher {
            let requested = wanted.clone();
            let owners = owners.to_vec();
            let result = worker::run(
                world,
                move || {
                    let mut watcher = watcher.lock().expect("file watcher");
                    if fallback {
                        watcher.use_polling();
                    }
                    watcher.set(requested)
                },
                move |world, result| {
                    world.resource_mut::<Watching>().updating = false;
                    if let Err(error) = result {
                        for owner in owners {
                            status(
                                world,
                                owner,
                                format!("Some paths cannot be watched: {error}. Use Refresh."),
                            );
                        }
                    }
                },
            );
            if result.is_ok() {
                let mut watching = world.resource_mut::<Watching>();
                watching.wanted = wanted;
                watching.updating = true;
                watching.fallback = false;
            }
        }
    }
    if !changes.rescan && changes.paths.is_empty() {
        return;
    }
    for owner in owners {
        let Some(mut view) = world.get_mut::<View>(*owner) else {
            continue;
        };
        view.cache.retain(|path, _| {
            !changes.rescan
                && !changes
                    .paths
                    .iter()
                    .any(|changed| changed == path || changed.parent() == Some(path.as_path()))
        });
    }
    crate::native::ide::disk_changed(world, changes);
}
