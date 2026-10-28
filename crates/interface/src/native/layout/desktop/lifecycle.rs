use super::*;

pub fn before_unmap(world: &mut World, entity: Entity) -> bool {
    scratch::before_unmap(world, entity);
    let Some(root) = root(world, entity) else {
        return false;
    };
    let Some(parent) = world.get::<TransientParent>(entity).map(|parent| parent.0) else {
        return false;
    };
    let Some(layout) = world.get::<LayoutBox>(entity) else {
        return false;
    };
    let Some(workspace) = world.get::<WorkspaceMember>(entity).map(|member| member.0) else {
        return false;
    };
    if !world.get::<Workspaces>(root).is_some_and(|spaces| {
        spaces.active == workspace
            && spaces
                .entries
                .iter()
                .find(|space| space.id == workspace)
                .is_some_and(|space| space.desktop.focused() == Some(PlacementId(layout.id)))
    }) || !focus::eligible(world, root, workspace, parent)
        || parent == entity
        || transients::descendant(world, parent, entity)
    {
        return false;
    }
    let follow = focus::following(world, root);
    focus::select(world, root, workspace, parent, follow);
    true
}

#[cfg(test)]
mod tests;
