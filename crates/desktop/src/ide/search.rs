use super::*;
use lince_editor::{Edit, search::Pattern};
use std::sync::atomic::{AtomicBool, Ordering};

pub(super) fn run(world: &mut World, owner: Entity, backwards: bool, replace_all: bool) {
    let view = world.get::<View>(owner).unwrap();
    let Some(path) = view.path.clone() else {
        return;
    };
    let Ok(needle) = crate::sand_panel::value(world, view.find) else {
        return;
    };
    let Ok(replacement) = crate::sand_panel::value(world, view.replace) else {
        return;
    };
    let options = world.get::<Ide>(owner).unwrap().settings.search;
    let query = needle.clone();
    let Some(doc) = world.resource::<Documents>().0.get(&path) else {
        return;
    };
    let rope = doc.buffer.snapshot();
    let stamp = (doc.buffer.identity(), doc.buffer.revision());
    let selection = view.selection;
    let start = if backwards {
        selection[0].min(selection[1])
    } else {
        selection[0].max(selection[1])
    };
    let mut view = world.get_mut::<View>(owner).unwrap();
    view.search_cancel.store(true, Ordering::Relaxed);
    let cancel = Arc::new(AtomicBool::new(false));
    view.search_cancel = cancel.clone();
    view.search_generation += 1;
    let generation = view.search_generation;
    let replacement_bytes = replacement.len();
    let result = crate::file_explorer::worker::run(
        world,
        move || {
            let pattern = Pattern::new(&query, options)?;
            if replace_all {
                let ranges = pattern.all(&rope, &cancel)?;
                let removed: usize = ranges
                    .iter()
                    .map(|range| rope.char_to_byte(range.end) - rope.char_to_byte(range.start))
                    .sum();
                if ranges
                    .len()
                    .checked_mul(replacement_bytes)
                    .and_then(|added| added.checked_add(rope.len_bytes() - removed))
                    .is_none_or(|size| size > lince_editor::MAX_FILE_BYTES)
                {
                    return Err("The replacements exceed the 16 MiB editing limit".into());
                }
                Ok(ranges)
            } else {
                pattern
                    .find(&rope, start, backwards, &cancel)
                    .map(|range| range.into_iter().collect())
            }
        },
        move |world, result| {
            let Some(view) = world.get::<View>(owner).filter(|view| {
                view.search_generation == generation && view.path.as_ref() == Some(&path)
            }) else {
                return;
            };
            if world
                .get::<Ide>(owner)
                .is_none_or(|ide| ide.settings.search != options)
                || crate::sand_panel::value(world, view.find).ok().as_ref() != Some(&needle)
                || replace_all
                    && crate::sand_panel::value(world, view.replace).ok().as_ref()
                        != Some(&replacement)
            {
                status(world, owner, "Search changed; run it again");
                return;
            }
            if view.selection != selection
                || world
                    .resource::<Documents>()
                    .0
                    .get(&path)
                    .is_none_or(|doc| (doc.buffer.identity(), doc.buffer.revision()) != stamp)
                || view.draft
                || world
                    .get::<EditableText>(view.editor)
                    .is_some_and(|input| input.is_composing())
            {
                status(
                    world,
                    owner,
                    "Text or selection changed; run the search again",
                );
                return;
            }
            match result {
                Ok(ranges) if replace_all => {
                    let count = ranges.len();
                    let edits: Vec<_> = ranges
                        .into_iter()
                        .map(|range| Edit {
                            range,
                            text: replacement.clone(),
                        })
                        .collect();
                    let result = world
                        .resource_mut::<Documents>()
                        .0
                        .get_mut(&path)
                        .unwrap()
                        .buffer
                        .edit_batch(&edits);
                    match result {
                        Ok(()) => {
                            world.get_mut::<View>(owner).unwrap().window = None;
                            status(world, owner, format!("Replaced {count} matches"));
                        }
                        Err(e) => status(world, owner, e),
                    }
                }
                Ok(ranges) => {
                    if let Some(range) = ranges.first() {
                        let line = world.resource::<Documents>().0[&path]
                            .buffer
                            .snapshot()
                            .char_to_line(range.start);
                        editing::select(world, owner, [range.start, range.end], line);
                        status(world, owner, "");
                    } else {
                        status(world, owner, "No match");
                    }
                }
                Err(e) => status(world, owner, e),
            }
        },
    );
    status(
        world,
        owner,
        result.map_or_else(|e| e, |()| "Searching…".into()),
    );
}
