use super::*;

pub(super) fn update(world: &mut World) {
    recovery::update(world);
    autosave::update(world, std::time::Instant::now());
    tools::update(world);
    let owners: Vec<_> = world
        .query_filtered::<Entity, With<View>>()
        .iter(world)
        .collect();
    for owner in &owners {
        if crate::laboratory::suspended(world, *owner) {
            continue;
        }
        restore(world, *owner);
        closing::update(world, *owner);
        render(world, *owner);
        project::render(world, *owner);
    }
    let composing: BTreeSet<_> = owners
        .iter()
        .filter_map(|owner| world.get::<View>(*owner))
        .filter(|view| {
            world
                .get::<EditableText>(view.editor)
                .is_some_and(|input| input.is_composing())
                || view.paste.is_some()
        })
        .filter_map(|view| view.path.clone())
        .collect();
    let requests: Vec<_> = world
        .resource::<Documents>()
        .0
        .iter()
        .filter(|(path, doc)| {
            doc.refresh
                && !doc.reading
                && !doc.moving
                && doc.saving.is_none()
                && !composing.contains(*path)
        })
        .filter_map(|(path, doc)| {
            doc.file.as_ref().map(|file| {
                (
                    path.clone(),
                    file.clone(),
                    doc.buffer.reconciliation(),
                    doc.preview.is_some(),
                )
            })
        })
        .collect();
    for (path, file, comparison, preview) in requests {
        let reply_path = path.clone();
        let result = crate::file_explorer::worker::run(
            world,
            move || {
                let snapshot = if preview {
                    file.preview()?
                } else {
                    file.read()?
                };
                let plan = comparison.compute(snapshot.text.to_string())?;
                Ok::<_, String>((snapshot, plan))
            },
            move |world, result| {
                let composing = world.query::<&View>().iter(world).any(|view| {
                    view.path.as_ref() == Some(&reply_path)
                        && world
                            .get::<EditableText>(view.editor)
                            .is_some_and(|input| input.is_composing())
                });
                let mut documents = world.resource_mut::<Documents>();
                let Some(document) = documents.0.get_mut(&reply_path) else {
                    return;
                };
                document.reading = false;
                if composing {
                    document.refresh = true;
                    return;
                }
                match result {
                    Ok((snapshot, plan)) => match document.buffer.accept(plan) {
                        Ok(true) => {
                            document.disk = snapshot;
                            document.error = None;
                        }
                        Ok(false) => document.refresh = true,
                        Err(e) => document.error = Some(e),
                    },
                    Err(e) => {
                        document.error = Some(format!("Disk unavailable; edits retained: {e}"))
                    }
                }
            },
        );
        if result.is_ok() {
            let mut docs = world.resource_mut::<Documents>();
            let doc = docs.0.get_mut(&path).unwrap();
            doc.refresh = false;
            doc.reading = true;
        }
    }
    let in_use: BTreeSet<_> = world
        .query::<&Ide>()
        .iter(world)
        .flat_map(|ide| ide.paths.iter().cloned())
        .collect();
    world.resource_mut::<Documents>().0.retain(|path, doc| {
        in_use.contains(path)
            || doc.buffer.is_dirty()
            || doc.buffer.conflict().is_some()
            || doc.file.is_none()
            || doc.saving.is_some()
            || doc.moving
    });
}

fn restore(world: &mut World, owner: Entity) {
    if !recovery::ready(world) {
        return;
    }
    if !world.get::<View>(owner).unwrap().restore {
        return;
    }
    world.get_mut::<View>(owner).unwrap().restore = false;
    let config = world.get::<Ide>(owner).unwrap().clone();
    for path in config.paths {
        let root = config
            .explorer
            .roots
            .iter()
            .filter(|root| path.starts_with(root))
            .max_by_key(|root| root.components().count())
            .cloned()
            .or_else(|| path.parent().map(PathBuf::from));
        if let Some(root) = root {
            let result = crate::file_explorer::worker::run(
                world,
                move || Scope::open(&root),
                move |world, result| match result {
                    Ok(scope) => open_with_activation(world, owner, scope, path, false),
                    Err(e) => status(world, owner, e),
                },
            );
            if let Err(e) = result {
                status(world, owner, e);
            }
        }
    }
}

pub(super) fn render(world: &mut World, owner: Entity) {
    settings::render(world, owner);
    if let Some(view) = world.get::<View>(owner).filter(|view| view.draft) {
        let (label, message) = (view.status, view.notice.clone());
        crate::sand_panel::status(world, label, message);
        return;
    }
    let (explorer, tabs) = {
        let view = world.get::<View>(owner).unwrap();
        (view.explorer, view.tabs)
    };
    if let Some(config) = world
        .get::<crate::file_explorer::FileExplorer>(explorer)
        .cloned()
    {
        if world.get::<Ide>(owner).unwrap().explorer != config {
            world.get_mut::<Ide>(owner).unwrap().explorer = config;
        }
    }
    let ide = world.get::<Ide>(owner).unwrap().clone();
    let labels: Vec<_> = ide
        .paths
        .iter()
        .map(|path| {
            (
                path.clone(),
                world
                    .resource::<Documents>()
                    .0
                    .get(path)
                    .is_some_and(|doc| doc.buffer.is_dirty()),
            )
        })
        .collect();
    if world.get::<View>(owner).unwrap().tab_labels != labels
        || world.get::<View>(owner).unwrap().path != ide.active
    {
        crate::sand_panel::clear(world, tabs);
        for (path, dirty) in &labels {
            let active = ide.active.as_ref() == Some(path);
            let caption = format!(
                "{}{}{}",
                if active { "› " } else { "" },
                path.file_name().unwrap_or_default().to_string_lossy(),
                if *dirty { " *" } else { "" }
            );
            let entity = crate::sand_panel::button(
                world,
                tabs,
                owner,
                &caption,
                actions::Control::Tab(path.clone()),
            );
            world
                .entity_mut(entity)
                .insert(crate::icons::Tooltip(path.display().to_string()));
        }
        world.get_mut::<View>(owner).unwrap().tab_labels = labels;
    }
    let view = world.get::<View>(owner).unwrap();
    let (viewport, content, editor, gutter, status_label) = (
        view.viewport,
        view.content,
        view.editor,
        view.gutter,
        view.status,
    );
    if view.path != ide.active {
        tabs::switch(world, owner, ide.active.clone());
    }
    let Some(path) = ide.active else {
        if let Some(mut syntax) = world.get_mut::<highlight::Syntax>(editor) {
            syntax.language = None;
        }
        let mut editor = world.get_mut::<EditableText>(editor).unwrap();
        if !editor.value().to_string().is_empty() {
            editor.editor.set_text("");
        }
        let view = world.get::<View>(owner).unwrap();
        let message = if view.notice.is_empty() {
            "Open a UTF-8 file from the Explorer".into()
        } else {
            view.notice.clone()
        };
        let controls = [view.conflicts, view.disk_preview];
        for entity in controls {
            if world.get::<Node>(entity).unwrap().display != Display::None {
                world.get_mut::<Node>(entity).unwrap().display = Display::None;
            }
        }
        crate::sand_panel::status(world, status_label, message);
        return;
    };
    let language = lince_editor::language::detect(&path);
    if let Some(mut syntax) = world.get_mut::<highlight::Syntax>(editor) {
        if syntax.language != language {
            syntax.language = language;
        }
    }
    let Some(doc) = world.resource::<Documents>().0.get(&path) else {
        let mut input = world.get_mut::<EditableText>(editor).unwrap();
        if !input.value().to_string().is_empty() {
            input.editor.set_text("");
        }
        let message = world.get::<View>(owner).unwrap().notice.clone();
        crate::sand_panel::status(
            world,
            status_label,
            if message.is_empty() {
                format!("Opening {}…", path.display())
            } else {
                message
            },
        );
        return;
    };
    let conflict = doc.buffer.conflict().is_some();
    let view = world.get::<View>(owner).unwrap();
    let mode = if doc.buffer.conflict().is_some() {
        "CONFLICT: overlapping edits. Review disk, then choose a version"
    } else if doc.saving.is_some() {
        "Saving…"
    } else if doc.buffer.is_dirty() {
        "Unsaved"
    } else {
        "Saved"
    };
    let mut message = format!(
        "{} · {} · line {} · {}",
        path.display(),
        mode,
        doc.buffer
            .snapshot()
            .char_to_line(view.selection[1].min(doc.buffer.snapshot().len_chars()))
            + 1,
        doc.error.as_deref().unwrap_or(&view.notice)
    );
    if let Some(reason) = &doc.preview {
        message = format!(
            "{} · Read-only preview, first 64 KiB · {reason}",
            path.display()
        );
    }
    if let Some(error) = recovery::error(world) {
        message.push_str(&format!(" · Draft recovery: {error}"));
    }
    let offset = world.get::<ScrollPosition>(viewport).map_or(0.0, |p| p.y);
    let first = (offset.max(0.0) / LINE_HEIGHT) as usize;
    let first = first
        .saturating_sub(4)
        .min(doc.buffer.snapshot().len_lines().saturating_sub(1));
    let viewport_size = world
        .get::<ComputedNode>(viewport)
        .map_or(Vec2::new(500.0, 500.0), |node| {
            node.size() * node.inverse_scale_factor()
        });
    let lines = ((viewport_size.y / LINE_HEIGHT).ceil() as usize + 8).clamp(8, WINDOW_LINES);
    let geometry = (lines, viewport_size.x);
    let preview_key = (doc.buffer.observed().as_ptr() as usize, first);
    let review = conflict && view.review;
    let preview = if review && view.preview_key != Some(preview_key) {
        let mut text = format!("Disk version from line {}\n", first + 1);
        for line in doc.buffer.observed().lines().skip(first).take(WINDOW_LINES) {
            for ch in line.chars() {
                if text.len() + ch.len_utf8() > lince_editor::MAX_WINDOW_BYTES {
                    break;
                }
                text.push(ch);
            }
            if text.len() >= lince_editor::MAX_WINDOW_BYTES {
                break;
            }
            text.push('\n');
        }
        Some(text)
    } else {
        None
    };
    let composing = world
        .get::<EditableText>(editor)
        .is_some_and(|input| input.is_composing() || input.pending_paste.is_some());
    let needed = !composing
        && view.window.as_ref().is_none_or(|window| {
            window.identity != doc.buffer.identity()
                || view.window_geometry != geometry
                || window.revision != doc.buffer.revision()
                || window.first_line != first
                || window.total_lines != doc.buffer.snapshot().len_lines()
        });
    let update = if needed {
        let selection = std::array::from_fn(|index| {
            view.anchors[index]
                .as_ref()
                .and_then(|anchor| doc.buffer.position(anchor))
                .unwrap_or(view.selection[index])
                .min(doc.buffer.snapshot().len_chars())
        });
        Some((doc.buffer.window(first, lines), selection))
    } else {
        None
    };
    if update.as_ref().map_or_else(
        || view.window.as_ref().is_some_and(|window| window.clipped),
        |(window, _)| window.clipped,
    ) {
        message.push_str(" · Long-line preview: editing disabled in this window");
    }
    let resize = update.is_some() || view.follow_caret;
    let (conflicts, disk_preview, disk_text) = (view.conflicts, view.disk_preview, view.disk_text);
    let display = if conflict {
        Display::Flex
    } else {
        Display::None
    };
    if world.get::<Node>(conflicts).unwrap().display != display {
        world.get_mut::<Node>(conflicts).unwrap().display = display;
    }
    let display = if review { Display::Flex } else { Display::None };
    if world.get::<Node>(disk_preview).unwrap().display != display {
        world.get_mut::<Node>(disk_preview).unwrap().display = display;
    }
    if let Some(preview) = preview {
        crate::sand_panel::status(world, disk_text, preview);
        world.get_mut::<View>(owner).unwrap().preview_key = Some(preview_key);
    }
    crate::sand_panel::status(world, status_label, message);
    if let Some((window, selection)) = update {
        let bytes = selection.map(|position| {
            window
                .text
                .chars()
                .take(position.saturating_sub(window.start))
                .map(char::len_utf8)
                .sum::<usize>()
                .min(window.text.len())
        });
        world
            .get_mut::<EditableText>(editor)
            .unwrap()
            .editor
            .set_text(&window.text);
        editing::widget_selection(world, editor, bytes);
        let mut view = world.get_mut::<View>(owner).unwrap();
        view.window = Some(window);
        view.window_geometry = geometry;
        view.selection = selection;
    }
    if resize {
        let view = world.get::<View>(owner).unwrap();
        let window = view.window.as_ref().unwrap();
        let lines = window.text.split('\n').count();
        let numbers = (window.first_line + 1..=window.first_line + lines)
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        let width = window
            .text
            .lines()
            .map(|line| {
                line.chars().fold(0_usize, |column, ch| {
                    if ch == '\t' {
                        (column / 8 + 1) * 8
                    } else {
                        column + 1
                    }
                })
            })
            .max()
            .unwrap_or(0) as f32
            * 16.0
            + 80.0;
        let width = ((width / 256.0).ceil() * 256.0).max(viewport_size.x);
        let top = px(window.first_line as f32 * LINE_HEIGHT);
        let height = px(window.total_lines as f32 * LINE_HEIGHT + LINE_HEIGHT);
        for (entity, width, height, top) in [
            (content, px(width), height, px(0)),
            (
                editor,
                px(width - 64.0),
                px(lines as f32 * LINE_HEIGHT),
                top,
            ),
            (gutter, px(58), Val::Auto, top),
        ] {
            let node = world.get::<Node>(entity).unwrap();
            if (node.width, node.height, node.top) != (width, height, top) {
                let mut node = world.get_mut::<Node>(entity).unwrap();
                node.width = width;
                node.height = height;
                node.top = top;
            }
        }
        crate::sand_panel::status(world, gutter, numbers);
    }
    editing::follow_horizontal(world, owner);
}
