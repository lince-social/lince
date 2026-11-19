use super::*;

mod history;
pub use history::{Selection, capture, numeric_destination};
pub(crate) use history::forget;

pub fn destination(world: &World, root: Entity, offset: isize) -> Result<u64, &'static str> {
    if !matches!(offset, -1 | 1) {
        return Err("Choose the previous or next workspace.");
    }
    let spaces = world
        .get::<Workspaces>(root)
        .ok_or("The workspace is unavailable.")?;
    let active = crate::native::laboratory::scope(world)
        .filter(|(owner, _, temporary, _)| *owner == root && spaces.active == *temporary)
        .map_or(spaces.active, |(_, _, _, previous)| previous);
    let mut ordered = ordered_ids(world, root);
    let mut previous = None;
    while let Some(id) = ordered.next() {
        if id == active {
            return if offset < 0 {
                previous.ok_or("This is the first workspace.")
            } else {
                ordered.next().ok_or("This is the last workspace.")
            };
        }
        previous = Some(id);
    }
    Err("The current authored workspace is unavailable.")
}

pub fn reorder_destination(
    world: &World,
    root: Entity,
    offset: isize,
) -> Result<u64, &'static str> {
    let spaces = world
        .get::<Workspaces>(root)
        .ok_or("The workspace is unavailable.")?;
    if crate::native::laboratory::temporary(world, root, spaces.active) {
        return Err("Switch to an authored workspace before reordering it.");
    }
    destination(world, root, offset)
}

pub fn reorder(world: &mut World, root: Entity, offset: isize) -> Result<(), &'static str> {
    let destination = reorder_destination(world, root, offset)?;
    {
        let mut spaces = world.get_mut::<Workspaces>(root).unwrap();
        let current = spaces
            .entries
            .iter()
            .position(|entry| entry.id == spaces.active)
            .unwrap();
        let destination = spaces
            .entries
            .iter()
            .position(|entry| entry.id == destination)
            .unwrap();
        spaces.entries.swap(current, destination);
    }
    crate::native::edit_mode::reorder_workspaces(world, root);
    if let Some(wake) = world.get_resource::<crate::host::WakeSignal>() {
        wake.ring();
    }
    Ok(())
}

#[cfg(test)]
mod tests;
