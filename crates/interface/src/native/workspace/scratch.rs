use super::*;
use crate::{layout::desktop::{ItemKind, TransferKind}, native::{layout::LayoutBox, sand_placement::scratch::{Home, Homes, normalize_all}}};

pub(super) fn normalize(world: &World, root: Entity, document: &mut Document) -> Result<(), String> {
    let associations = document.workspaces.iter().flat_map(|space|
        space.desktop.scratch_items().map(move |(id, scratch)| (space.id, id, scratch.clone()))).collect::<Vec<_>>();
    if associations.is_empty() { return Ok(()); }
    let mut unique = HashSet::new();
    let mut homes = Homes::new();
    for (source, id, scratch) in associations {
        if !unique.insert(id) { return Err("A scratch placement belongs to multiple workspaces; saving was stopped.".into()); }
        let source_index = document.workspaces.iter().position(|space| space.id == source).ok_or("The scratch source is unavailable.")?;
        let kind = document.workspaces[source_index].desktop.items().iter().find(|item| item.id == id)
            .ok_or("The scratch item has no placement.")?.kind;
        if kind == ItemKind::Application {
            document.workspaces[source_index].desktop.remove(id);
            continue;
        }
        let home_index = document.workspaces.iter().position(|space| space.id == scratch.home_workspace() && !document.deleted_workspaces.contains_key(&space.id));
        let original = home_index.is_some();
        let destination = home_index.or_else(|| document.workspaces.iter().position(|space| space.id == document.active))
            .ok_or("No valid workspace can retain the scratch item's home.")?;
        if source_index != destination {
            let (before, after) = document.workspaces.split_at_mut(source_index.max(destination));
            let (source, target) = if source_index < destination { (&mut before[source_index], &mut after[0]) }
                else { (&mut after[0], &mut before[destination]) };
            source.desktop.transfer_to(&mut target.desktop, id, TransferKind::Item)
                .map_err(|_| "The scratch home could not be prepared; saving was stopped.")?;
        }
        let workspace = &mut document.workspaces[destination];
        workspace.desktop.normalize_scratch(id, original).map_err(|_| "The scratch home is invalid; saving was stopped.")?;
        let item = workspace.desktop.items().iter().find(|item| item.id == id).ok_or("The scratch item was lost while preparing its home.")?;
        homes.insert(id.0, Home { workspace: workspace.id, geometry: item.free, rules: item.rules });
    }
    normalize_children(world, root, document, &mut homes)?;
    for space in &mut document.workspaces {
        if space.desktop.focused().is_some_and(|id| homes.contains_key(&id.0)) { space.desktop.clear_focus(); }
        if let Some(id) = space.desktop.fullscreen().filter(|id| homes.contains_key(&id.0)) {
            space.desktop.request_state(id, crate::host::WindowRequest { fullscreen: Some(false), ..default() });
        }
    }
    macro_rules! normalize {
        ($($field:ident),* $(,)?) => { $(normalize_all(&mut document.$field, &homes);)* };
    }
    normalize!(sands, records, proteins, karma_castles, simulations, frequency_castles, transfer_castles,
        recorders, documents, source_views, demeters, media, drawings, extension_sands, editors, explorers,
        calendars, maps, kanbans, instincts, shader_editors, assertions, shaders, layouts);
    for saved in &mut document.workspace_focus { saved.normalize_scratch(&homes); }
    if let Some(state) = &mut document.canvas_state {
        let identities = world.get::<Children>(root).into_iter().flat_map(|children| children.iter())
            .filter_map(|entity| {
                let id = world.get::<LayoutBox>(entity)?.id;
                let identity = &world.get::<crate::native::canvas_host::Identity>(entity)?.0;
                Some((identity.clone(), id))
            }).collect::<HashMap<_, _>>();
        for placement in &mut state.snapshot.placements {
            if let Some(home) = identities.get(&placement.id).and_then(|id| homes.get(id)) {
                placement.workspace = home.workspace;
                placement.geometry.position = std::array::from_fn(|axis| home.geometry.origin[axis] + home.geometry.size[axis] * 0.5);
                placement.geometry.size = home.geometry.size;
                placement.selected = false;
            }
        }
    }
    if document.workspaces.iter().any(|space| !space.desktop.valid()) { return Err("Scratch restoration produced invalid workspace data; saving was stopped.".into()); }
    Ok(())
}

fn normalize_children(world: &World, root: Entity, document: &mut Document, homes: &mut Homes) -> Result<(), String> {
    use crate::{layout::desktop::{Geometry, Item, PlacementId}, native::layout::desktop::{ApplicationWindow, TransientParent}};
    let children = world.get::<Children>(root).into_iter().flat_map(|children| children.iter()).filter_map(|entity| {
        if world.get::<ApplicationWindow>(entity).is_some() { return None; }
        let parent = world.get::<TransientParent>(entity)?.0;
        if world.get::<ApplicationWindow>(parent).is_some() { return None; }
        Some((
            *world.get::<LayoutBox>(entity)?, world.get::<LayoutBox>(parent)?.id,
            world.get::<WorkspaceMember>(entity)?.0, *world.get::<CanvasItem>(entity)?, *world.get::<CanvasItem>(parent)?,
        ))
    }).collect::<Vec<_>>();
    for _ in 0..crate::native::layout::MAX_DEPTH {
        let mut changed = false;
        for (layout, parent, source, canvas, owner) in &children {
            if homes.contains_key(&layout.id) { continue; }
            let Some(home) = homes.get(parent) else { continue; };
            let target = home.workspace;
            let center = bevy::math::DVec2::from_array(std::array::from_fn(|axis| home.geometry.origin[axis] + home.geometry.size[axis] * 0.5));
            let center = center + canvas.position - owner.position;
            let source_index = document.workspaces.iter().position(|space| space.id == *source).ok_or("An owned scratch item's source is unavailable.")?;
            let destination = document.workspaces.iter().position(|space| space.id == target).ok_or("An owned scratch item's home is unavailable.")?;
            let id = PlacementId(layout.id);
            let size = document.workspaces[source_index].desktop.items().iter().find(|item| item.id == id)
                .map_or(canvas.size.as_dvec2().to_array(), |item| item.free.size);
            let geometry = Geometry { origin: (center - bevy::math::DVec2::from_array(size) * 0.5).to_array(), size };
            if geometry.rectangle().is_none() { return Err("An owned scratch item has invalid home geometry; saving was stopped.".into()); }
            if !document.workspaces[source_index].desktop.items().iter().any(|item| item.id == id) {
                document.workspaces[source_index].desktop.insert_inactive(Item { id, kind: ItemKind::Native, rules: layout.rules, free: geometry, free_width: None, floating: true })
                    .map_err(|_| "The owned scratch item could not be retained; saving was stopped.")?;
            }
            if source_index != destination {
                let (before, after) = document.workspaces.split_at_mut(source_index.max(destination));
                let (source, target) = if source_index < destination { (&mut before[source_index], &mut after[0]) }
                    else { (&mut after[0], &mut before[destination]) };
                source.desktop.transfer_to(&mut target.desktop, id, TransferKind::Item)
                    .map_err(|_| "The owned scratch item could not return home; saving was stopped.")?;
            }
            document.workspaces[destination].desktop.set_free_geometry(id, geometry)
                .map_err(|_| "The owned scratch item's home is invalid; saving was stopped.")?;
            homes.insert(layout.id, Home { workspace: target, geometry, rules: layout.rules });
            changed = true;
        }
        if !changed { break; }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
