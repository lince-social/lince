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
    let ordered = numbers(world, root);
    let current = ordered
        .iter()
        .position(|id| *id == active)
        .ok_or("The current authored workspace is unavailable.")?;
    current
        .checked_add_signed(offset)
        .and_then(|index| ordered.get(index))
        .copied()
        .ok_or(if offset < 0 {
            "This is the first workspace."
        } else {
            "This is the last workspace."
        })
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
