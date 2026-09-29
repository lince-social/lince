use super::*;
use std::time::{Duration, Instant};

struct Pending {
    stamp: (u64, u64, u16),
    due: Instant,
    attempted: bool,
}

#[derive(Resource, Default)]
pub(super) struct Autosave {
    pending: BTreeMap<PathBuf, Pending>,
    wake_at: Option<Instant>,
}

pub(super) fn update(world: &mut World, now: Instant) {
    world.init_resource::<Autosave>();
    let mut enabled = BTreeMap::<PathBuf, (Entity, u16)>::new();
    for (owner, ide) in world.query::<(Entity, &Ide)>().iter(world) {
        if ide.settings.autosave_seconds == 0 || crate::laboratory::suspended(world, owner) {
            continue;
        }
        for path in &ide.paths {
            let value = enabled
                .entry(path.clone())
                .or_insert((owner, ide.settings.autosave_seconds));
            if value.1 > ide.settings.autosave_seconds {
                *value = (owner, ide.settings.autosave_seconds);
            }
        }
    }
    let blocked: BTreeSet<_> = world
        .query::<&View>()
        .iter(world)
        .filter(|view| {
            view.draft
                || view.paste.is_some()
                || world
                    .get::<EditableText>(view.editor)
                    .is_some_and(|input| input.is_composing())
        })
        .filter_map(|view| view.path.clone())
        .collect();
    let mut ready = Vec::new();
    let mut next = None;
    world.resource_scope(|world, mut state: Mut<Autosave>| {
        let documents = world.resource::<Documents>();
        state.pending.retain(|path, _| {
            enabled.contains_key(path)
                && documents
                    .0
                    .get(path)
                    .is_some_and(|doc| doc.buffer.is_dirty())
        });
        for (path, (owner, seconds)) in enabled {
            let Some(doc) = documents.0.get(&path).filter(|doc| doc.buffer.is_dirty()) else {
                continue;
            };
            let stamp = (doc.buffer.identity(), doc.buffer.revision(), seconds);
            let pending = state.pending.entry(path.clone()).or_insert(Pending {
                stamp,
                due: now + Duration::from_secs(seconds.into()),
                attempted: false,
            });
            if pending.stamp != stamp {
                *pending = Pending {
                    stamp,
                    due: now + Duration::from_secs(seconds.into()),
                    attempted: false,
                };
            }
            if pending.attempted
                || doc.file.is_none()
                || doc.error.is_some()
                || doc.buffer.conflict().is_some()
            {
                continue;
            }
            if pending.due > now {
                next = Some(next.map_or(pending.due, |earlier: Instant| earlier.min(pending.due)));
            } else if !doc.reading
                && !doc.refresh
                && !doc.moving
                && doc.saving.is_none()
                && !blocked.contains(&path)
            {
                pending.attempted = true;
                ready.push((owner, path));
            }
        }
        if state.wake_at.is_some_and(|due| due <= now) {
            state.wake_at = None;
        }
        if let Some(due) = next.filter(|due| state.wake_at.is_none_or(|scheduled| *due < scheduled))
        {
            state.wake_at = Some(due);
            if let Some(wake) = world.get_resource::<crate::wake::WakeSignal>() {
                wake.after(due.saturating_duration_since(now));
            }
        }
    });
    for (owner, path) in ready {
        actions::save(world, owner, path);
    }
}
