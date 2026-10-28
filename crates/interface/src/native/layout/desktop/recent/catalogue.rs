use super::*;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct Target {
    pub entity: Entity,
    pub placement: PlacementId,
}

pub(super) struct Entry {
    pub target: Target,
    pub title: String,
    pub context: String,
}

pub(super) fn workspace(world: &World, root: Entity, target: Target) -> Option<u64> {
    let workspace = world.get::<WorkspaceMember>(target.entity)?.0;
    if crate::native::laboratory::temporary(world, root, workspace)
        || world.get::<LayoutBox>(target.entity)?.id != target.placement.0
        || world.get::<TransientParent>(target.entity).is_some()
        || !focus::eligible(world, root, workspace, target.entity)
    {
        return None;
    }
    Some(workspace)
}

pub(super) fn entry(world: &World, root: Entity, target: Target) -> Option<Entry> {
    let workspace = workspace(world, root, target)?;
    let space = world
        .get::<Workspaces>(root)?
        .entries
        .iter()
        .find(|space| space.id == workspace)?;
    let name: String = space
        .name
        .chars()
        .filter(|value| !value.is_control())
        .take(160)
        .collect();
    let mut context = if name.is_empty() {
        workspace.to_string()
    } else {
        name
    };
    if let Some(output) = world
        .get_resource::<crate::native::output_views::OutputViews>()
        .filter(|views| views.applies_to(root))
        .and_then(|views| {
            views
                .outputs
                .iter()
                .find(|output| output.workspace == workspace)
        })
    {
        context.push_str(&format!(
            " · {} {}",
            crate::i18n::Token::OverviewMonitor.text(),
            output.id.0
        ));
    }
    Some(Entry {
        target,
        title: overview::item_name(world, target.entity),
        context,
    })
}

pub(super) fn entries(world: &World, root: Entity) -> Vec<Entry> {
    world
        .get::<Children>(root)
        .into_iter()
        .flatten()
        .filter_map(|entity| {
            let target = Target {
                entity: *entity,
                placement: PlacementId(world.get::<LayoutBox>(*entity)?.id),
            };
            entry(world, root, target)
        })
        .collect()
}

pub(super) fn current(world: &World, root: Entity) -> Option<Target> {
    let spaces = world.get::<Workspaces>(root)?;
    let id = spaces
        .entries
        .iter()
        .find(|space| space.id == spaces.active)?
        .desktop
        .focused()?;
    let mut entity = world.get::<Children>(root)?.iter().find(|entity| {
        world
            .get::<WorkspaceMember>(*entity)
            .is_some_and(|member| member.0 == spaces.active)
            && world
                .get::<LayoutBox>(*entity)
                .is_some_and(|layout| layout.id == id.0)
    })?;
    for _ in 0..32 {
        if let Some(parent) = world.get::<TransientParent>(entity) {
            entity = parent.0;
        } else {
            let target = Target {
                entity,
                placement: PlacementId(world.get::<LayoutBox>(entity)?.id),
            };
            return workspace(world, root, target).map(|_| target);
        }
    }
    None
}

pub(super) fn output(world: &World, root: Entity) -> Option<OutputId> {
    world
        .get_resource::<crate::native::output_views::OutputViews>()
        .filter(|views| views.applies_to(root))
        .map(|views| views.focused)
}
