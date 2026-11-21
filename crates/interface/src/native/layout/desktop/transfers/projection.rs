use super::{
    super::{transients, usable, viewport},
    CanvasItem, Geometry, LayoutBox, Managed, PlacementId, Runtime, TransientParent,
    WorkspaceLayout, WorkspaceMember, Workspaces, focus,
};
use crate::native::{area_effects::AreaScale, output_views::OutputViews};
use bevy::{math::DVec2, prelude::*};
use std::collections::BTreeMap;

pub(super) fn refresh(world: &mut World, root: Entity, workspace: u64) {
    let Some(spaces) = world.get::<Workspaces>(root) else {
        return;
    };
    if spaces.active == workspace
        || world
            .get_resource::<OutputViews>()
            .filter(|views| views.applies_to(root))
            .is_some_and(|views| {
                views
                    .outputs
                    .iter()
                    .any(|output| output.workspace == workspace)
            })
    {
        return;
    }
    let Some(space) = spaces.entries.iter().find(|space| space.id == workspace) else {
        return;
    };
    if space.desktop.mode() != WorkspaceLayout::Tiled || space.desktop.fullscreen().is_some() {
        return;
    }
    let Some(bounds) = viewport(world, root) else {
        return;
    };
    let Some(usable) = usable(world, root) else {
        return;
    };
    let mut model = space.desktop.clone();
    let Ok(mut solution) = model.solve(usable, bounds) else {
        return;
    };
    let items: BTreeMap<_, _> = world
        .get::<Children>(root)
        .into_iter()
        .flat_map(|children| children.iter())
        .filter_map(|entity| {
            if world.get::<WorkspaceMember>(entity) != Some(&WorkspaceMember(workspace))
                || !focus::eligible(world, root, workspace, entity)
            {
                return None;
            }
            Some((PlacementId(world.get::<LayoutBox>(entity)?.id), entity))
        })
        .collect();
    transients::place(world, &items, &mut model, &mut solution, usable);
    world.init_resource::<Runtime>();
    let scroll = DVec2::from_array(solution.scroll);
    for item in &solution.items {
        let Some(&entity) = items.get(&item.id) else {
            continue;
        };
        let Ok(mut target) = world.get_entity_mut(entity) else {
            continue;
        };
        if item.floating && !target.contains::<TransientParent>() {
            continue;
        }
        let Some(mut canvas) = target.get_mut::<CanvasItem>() else {
            continue;
        };
        let size = DVec2::from_array(item.rectangle.size());
        let position = DVec2::from_array(item.rectangle.origin()) + scroll + size * 0.5;
        let size = size.as_vec2();
        if canvas.position != position || canvas.size != size {
            canvas.position = position;
            canvas.size = size;
        }
        target.insert(Managed {
            id: item.id,
            floating: item.floating,
        });
        if !item.floating {
            target.remove::<AreaScale>();
        }
        let size = size.as_dvec2();
        let geometry = Geometry {
            origin: (position - size * 0.5).to_array(),
            size: size.to_array(),
        };
        world
            .resource_mut::<Runtime>()
            .geometry
            .insert(entity, geometry);
    }
}
