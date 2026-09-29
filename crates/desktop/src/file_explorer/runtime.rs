use super::*;

#[derive(Resource, Default)]
pub(super) struct Watching {
    watch: Option<lince_editor::watch::Watch>,
    attempted: bool,
}

pub(super) fn update(world: &mut World) {
    worker::poll(world);
    let owners: Vec<_> = world
        .query_filtered::<Entity, With<View>>()
        .iter(world)
        .collect();
    watches(world, &owners);
    for owner in owners.iter().copied() {
        if crate::laboratory::suspended(world, owner) {
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
        render(world, owner);
    }
}

fn request(world: &mut World, owner: Entity) {
    let mut view = world.get_mut::<View>(owner).unwrap();
    let abandoned: Vec<_> = view
        .listings
        .keys()
        .filter(|path| !view.pending.contains(*path) || !view.expanded.contains(*path))
        .cloned()
        .collect();
    for path in abandoned {
        if let Some(cancel) = view.listings.remove(&path) {
            cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        view.pending.remove(&path);
        view.cache.remove(&path);
    }
    let view = world.get::<View>(owner).unwrap();
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
                        listing.entries.extend(batch.entries);
                        lince_editor::explorer::sort(&mut listing.entries);
                        listing.truncated = limited;
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
                        view.dirty = true;
                        status(world, owner, e);
                    }
                }
            },
        );
        if result.is_ok() {
            let mut view = world.get_mut::<View>(owner).unwrap();
            view.pending.insert(path.clone());
            view.listings.insert(path, cancel);
        }
    }
}

fn render(world: &mut World, owner: Entity) {
    if world.get::<View>(owner).unwrap().dirty {
        let config = world.get::<FileExplorer>(owner).unwrap();
        let view = world.get::<View>(owner).unwrap();
        let rows = if let Some(rows) = &view.search_rows {
            rows.clone()
        } else {
            let mut rows = Vec::new();
            for root in &config.roots {
                rows.push(Row {
                    entry: Entry {
                        path: root.clone(),
                        directory: true,
                        link: false,
                    },
                    root: root.clone(),
                    depth: 0,
                });
                flatten(view, root, root, 1, &mut rows);
            }
            rows
        };
        let mut view = world.get_mut::<View>(owner).unwrap();
        view.rows = rows;
        view.shown = None;
        view.dirty = false;
    }
    let view = world.get::<View>(owner).unwrap();
    let grid = world.get::<FileExplorer>(owner).unwrap().grid;
    let columns = if grid {
        world.get::<ComputedNode>(view.viewport).map_or(2, |node| {
            ((node.size().x * node.inverse_scale_factor()) / 150.0)
                .floor()
                .max(1.0) as usize
        })
    } else {
        1
    };
    let height = if grid { 76.0 } else { 30.0 };
    let offset = world
        .get::<ScrollPosition>(view.viewport)
        .map_or(0.0, |scroll| scroll.y);
    let first = (offset.max(0.0) / height).floor() as usize * columns;
    let count = world.get::<ComputedNode>(view.viewport).map_or(30, |node| {
        ((node.size().y * node.inverse_scale_factor()) / height).ceil() as usize + 2
    }) * columns;
    let first = first.min(view.rows.len());
    let key = (first, columns, count, grid);
    if view.shown == Some(key) {
        return;
    }
    let content = view.content;
    let total = view.rows.len().div_ceil(columns);
    let rows: Vec<_> = view
        .rows
        .iter()
        .skip(first)
        .take(count.min(160))
        .cloned()
        .collect();
    let expanded = view.expanded.clone();
    let search = view.search_rows.is_some();
    crate::sand_panel::clear(world, content);
    world.get_mut::<Node>(content).unwrap().height = px(total as f32 * height);
    for (index, row) in rows.into_iter().enumerate() {
        let n = first + index;
        let prefix = if row.entry.directory {
            if expanded.contains(&row.entry.path) {
                "▾ "
            } else {
                "▸ "
            }
        } else {
            ""
        };
        let name = if search {
            row.entry
                .path
                .strip_prefix(&row.root)
                .unwrap_or(&row.entry.path)
                .to_string_lossy()
                .into_owned()
        } else if row.depth == 0 {
            row.root.display().to_string()
        } else {
            row.entry
                .path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        };
        let caption = if grid {
            format!(
                "{}\n{name}{}",
                if row.entry.directory { "▣" } else { "▤" },
                if row.entry.link { " ↗" } else { "" }
            )
        } else {
            format!("{prefix}{name}{}", if row.entry.link { " ↗" } else { "" })
        };
        let button = crate::sand_panel::button(
            world,
            content,
            owner,
            &caption,
            actions::Control::Entry(row.clone()),
        );
        world.entity_mut(button).insert(Node {
            position_type: PositionType::Absolute,
            left: if grid {
                percent((n % columns) as f32 * 100.0 / columns as f32)
            } else {
                px(0)
            },
            top: px((n / columns) as f32 * height),
            width: percent(100.0 / columns as f32),
            height: px(height),
            padding: UiRect {
                left: px(if grid {
                    6.0
                } else {
                    row.depth as f32 * 14.0 + 4.0
                }),
                right: px(4),
                top: px(4),
                bottom: px(4),
            },
            overflow: Overflow::clip(),
            ..default()
        });
        if !grid && row.depth == 0 {
            let remove = crate::sand_panel::button(
                world,
                content,
                owner,
                "×",
                actions::Control::RemoveRoot(row.root),
            );
            world.entity_mut(remove).insert(Node {
                position_type: PositionType::Absolute,
                right: px(0),
                top: px(n as f32 * height),
                width: px(24),
                height: px(height),
                ..default()
            });
        }
    }
    world.get_mut::<View>(owner).unwrap().shown = Some(key);
}

fn flatten(view: &View, root: &PathBuf, path: &PathBuf, depth: usize, rows: &mut Vec<Row>) {
    if depth > 64 || rows.len() >= 100_000 || !view.expanded.contains(path) {
        return;
    }
    if let Some(listing) = view.cache.get(path) {
        for entry in &listing.entries {
            if rows.len() >= 100_000 {
                break;
            }
            rows.push(Row {
                entry: entry.clone(),
                root: root.clone(),
                depth,
            });
            if entry.directory && &entry.path != path {
                flatten(view, root, &entry.path, depth + 1, rows);
            }
        }
    }
}

fn watches(world: &mut World, owners: &[Entity]) {
    let mut wanted = crate::ide::watched(world);
    for owner in owners {
        if crate::laboratory::suspended(world, *owner) {
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
    let wake = world.get_resource::<crate::wake::WakeSignal>().cloned();
    let mut watching = world.resource_mut::<Watching>();
    if !watching.attempted && !wanted.is_empty() {
        watching.attempted = true;
        match lince_editor::watch::Watch::new(move || {
            if let Some(wake) = &wake {
                wake.ring();
            }
        }) {
            Ok(watch) => watching.watch = Some(watch),
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
    let Some(watch) = &mut watching.watch else {
        return;
    };
    let result = watch.set(wanted);
    let changes = watch.drain();
    if let Err(e) = result {
        for owner in owners {
            status(
                world,
                *owner,
                format!("Some paths cannot be watched: {e}. Use Refresh."),
            );
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
    crate::ide::disk_changed(world, changes);
}
