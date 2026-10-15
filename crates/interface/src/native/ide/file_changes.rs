use super::*;
use std::path::Path;

pub(crate) fn reserve_change(
    world: &mut World,
    path: &Path,
    deleting: bool,
) -> Result<Vec<PathBuf>, String> {
    editing::capture(world);
    if world.query::<&View>().iter(world).any(|view| {
        view.draft
            && view
                .path
                .as_ref()
                .is_some_and(|open| open.starts_with(path))
    }) {
        return Err("Resolve the retained visible draft before changing this path".into());
    }
    let Some(mut documents) = world.get_resource_mut::<Documents>() else {
        return Ok(Vec::new());
    };
    let affected: Vec<_> = documents
        .0
        .keys()
        .filter(|open| open.starts_with(path))
        .cloned()
        .collect();
    for open in &affected {
        let doc = &documents.0[open];
        if doc.reading || doc.saving.is_some() || doc.moving {
            return Err("Wait for file access before changing this path".into());
        }
        if deleting
            && (doc.buffer.is_dirty() || doc.buffer.conflict().is_some() || doc.file.is_none())
        {
            return Err("Save or discard open drafts before deleting this item".into());
        }
    }
    for open in &affected {
        documents.0.get_mut(open).unwrap().moving = true;
    }
    Ok(affected)
}

pub(crate) fn finish_change(
    world: &mut World,
    from: &Path,
    to: Option<&Path>,
    bindings: BTreeMap<PathBuf, Option<Arc<FileBinding>>>,
    succeeded: bool,
) {
    let Some(mut documents) = world.get_resource_mut::<Documents>() else {
        return;
    };
    let affected: Vec<_> = documents
        .0
        .keys()
        .filter(|path| path.starts_with(from))
        .cloned()
        .collect();
    let mut removed = BTreeSet::new();
    for old in affected {
        let mut doc = documents.0.remove(&old).unwrap();
        doc.moving = false;
        if !succeeded {
            documents.0.insert(old, doc);
            continue;
        }
        if let Some(to) = to {
            let path = lince_editor::operations::relocated(&old, from, to).unwrap();
            doc.file = bindings.get(&old).cloned().flatten();
            if doc.file.is_none() {
                doc.error = Some("The path moved but could not be reopened; use Save as…".into());
            }
            documents.0.insert(path, doc);
        } else if doc.buffer.is_dirty() || doc.buffer.conflict().is_some() {
            doc.file = None;
            doc.error = Some(
                "The file was deleted while typing; this draft is retained. Use Save as…".into(),
            );
            documents.0.insert(old, doc);
        } else {
            removed.insert(old);
        }
    }
    if !succeeded {
        return;
    }
    if let Some(to) = to {
        tabs::relocate(world, from, to);
    }
    for mut ide in world.query::<&mut Ide>().iter_mut(world) {
        ide.paths.retain(|path| !removed.contains(path));
        if let Some(to) = to {
            for path in &mut ide.paths {
                if let Some(next) = lince_editor::operations::relocated(path, from, to) {
                    *path = next;
                }
            }
            if let Some(active) = &ide.active {
                if let Some(next) = lince_editor::operations::relocated(active, from, to) {
                    ide.active = Some(next);
                }
            }
        }
        if ide
            .active
            .as_ref()
            .is_some_and(|active| removed.contains(active))
        {
            ide.active = ide.paths.first().cloned();
        }
    }
}
