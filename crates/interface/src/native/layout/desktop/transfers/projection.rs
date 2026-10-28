use super::*;

pub(super) fn refresh(world: &mut World, root: Entity, workspace: u64) {
    let Some(spaces) = world.get::<Workspaces>(root) else {
        return;
    };
    if spaces.active == workspace
        || world
            .get_resource::<crate::native::output_views::OutputViews>()
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
    let mut model = space.desktop.clone();
    let Some(bounds) = super::super::viewport(world, root) else {
        return;
    };
    let Some(usable) = super::super::usable(world, root) else {
        return;
    };
    let Ok(mut solution) = model.solve(usable, bounds) else {
        return;
    };
    let items: BTreeMap<_, _> = world
        .get::<Children>(root)
        .map(|children| {
            children
                .iter()
                .filter_map(|entity| {
                    if world.get::<WorkspaceMember>(entity).map(|member| member.0)
                        != Some(workspace)
                        || !focus::eligible(world, root, workspace, entity)
                    {
                        return None;
                    }
                    Some((PlacementId(world.get::<LayoutBox>(entity)?.id), entity))
                })
                .collect()
        })
        .unwrap_or_default();
    super::super::transients::place(world, &items, &mut model, &mut solution, usable);
    world.init_resource::<Runtime>();
    for item in &solution.items {
        let Some(entity) = items.get(&item.id).copied() else {
            continue;
        };
        if item.floating && world.get::<TransientParent>(entity).is_none() {
            continue;
        }
        let size = bevy::math::DVec2::from_array(item.rectangle.size()).as_vec2();
        let position = bevy::math::DVec2::from_array(std::array::from_fn(|axis| {
            item.rectangle.origin()[axis]
                + solution.scroll[axis]
                + item.rectangle.size()[axis] * 0.5
        }));
        let Some(mut canvas) = world.get_mut::<CanvasItem>(entity) else {
            continue;
        };
        if canvas.position != position || canvas.size != size {
            canvas.position = position;
            canvas.size = size;
        }
        world.entity_mut(entity).insert(Managed {
            id: item.id,
            floating: item.floating,
        });
        if !item.floating {
            world
                .entity_mut(entity)
                .remove::<crate::native::area_effects::AreaScale>();
        }
        world.resource_mut::<Runtime>().geometry.insert(
            entity,
            Geometry {
                origin: (position - size.as_dvec2() * 0.5).to_array(),
                size: size.as_dvec2().to_array(),
            },
        );
    }
}
