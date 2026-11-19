use crate::{
    CalculatedClip, ComputedNode, Display, FixedNode, Node, OverrideClip, UiClippingBoundary,
    experimental::{UiChildren, UiRootNodes},
    ui_transform::UiGlobalTransform,
};
use bevy_ecs::{
    entity::{Entity, EntityHashSet},
    entity_disabling::Disabled,
    hierarchy::{ChildOf, Children},
    lifecycle::RemovedComponents,
    query::{Added, Allow, Changed, Has, Or},
    system::{Commands, Local, ParamSet, Query},
};

/// Updates clipping for all nodes
pub fn update_clipping_system(
    mut commands: Commands,
    root_nodes: UiRootNodes,
    mut queries: ParamSet<(
        Query<(
            &Node,
            &ComputedNode,
            &UiGlobalTransform,
            Option<&mut CalculatedClip>,
            Has<OverrideClip>,
            Has<FixedNode>,
            Option<&mut UiClippingBoundary>,
        )>,
        Query<
            Entity,
            Or<(
                Changed<Node>,
                Changed<ComputedNode>,
                Changed<UiGlobalTransform>,
                Changed<Children>,
                Changed<ChildOf>,
                Changed<OverrideClip>,
                Changed<FixedNode>,
                Changed<CalculatedClip>,
                Added<UiClippingBoundary>,
            )>,
        >,
    )>,
    ui_children: UiChildren,
    disabled: Query<(), (Added<Disabled>, Allow<Disabled>)>,
    mut removed: (
        RemovedComponents<Node>,
        RemovedComponents<Children>,
        RemovedComponents<ChildOf>,
        RemovedComponents<OverrideClip>,
        RemovedComponents<FixedNode>,
        RemovedComponents<CalculatedClip>,
        RemovedComponents<UiClippingBoundary>,
        RemovedComponents<Disabled>,
    ),
    #[cfg(feature = "ghost_nodes")] ghosts: Query<Entity, Added<crate::experimental::GhostNode>>,
    #[cfg(feature = "ghost_nodes")] mut removed_ghosts: RemovedComponents<
        crate::experimental::GhostNode,
    >,
    mut dirty: Local<EntityHashSet>,
) {
    dirty.clear();
    let reset = removed.7.read().count() != 0 || !disabled.is_empty();
    for entity in queries
        .p1()
        .iter()
        .chain(removed.0.read())
        .chain(removed.1.read())
        .chain(removed.2.read())
        .chain(removed.3.read())
        .chain(removed.4.read())
        .chain(removed.5.read())
        .chain(removed.6.read())
    {
        ui_children.affected_roots(entity, &mut dirty);
    }
    #[cfg(feature = "ghost_nodes")]
    for entity in ghosts.iter().chain(removed_ghosts.read()) {
        ui_children.affected_roots(entity, &mut dirty);
    }
    let mut node_query = queries.p0();
    for root_node in root_nodes.iter() {
        if reset || dirty.contains(&root_node) {
            update_clipping(
                &mut commands,
                &ui_children,
                &mut node_query,
                root_node,
                None,
            );
        }
    }
}

fn update_clipping(
    commands: &mut Commands,
    ui_children: &UiChildren,
    node_query: &mut Query<(
        &Node,
        &ComputedNode,
        &UiGlobalTransform,
        Option<&mut CalculatedClip>,
        Has<OverrideClip>,
        Has<FixedNode>,
        Option<&mut UiClippingBoundary>,
    )>,
    entity: Entity,
    mut maybe_inherited_clip: Option<CalculatedClip>,
) {
    let Ok((
        node,
        computed_node,
        transform,
        maybe_calculated_clip,
        has_override_clip,
        has_fixed_node,
        boundary,
    )) = node_query.get_mut(entity)
    else {
        return;
    };

    if let Some(mut boundary) = boundary {
        if boundary.inherited_clip != maybe_inherited_clip {
            boundary.inherited_clip = maybe_inherited_clip;
        }
        return;
    }

    // If the UI node entity has an `OverrideClip` or `FixedNode` component, discard any inherited clip rect
    if has_override_clip || has_fixed_node {
        maybe_inherited_clip = None;
    }

    // If `display` is None, clip the entire node and all its descendants.
    if node.display == Display::None {
        maybe_inherited_clip = Some(CalculatedClip::FullyClipped);
    }

    // Update this node's CalculatedClip component
    if let Some(mut calculated_clip) = maybe_calculated_clip {
        if let Some(inherited_clip) = maybe_inherited_clip.as_ref() {
            // Replace the previous calculated clip with the inherited clipping rect
            if *calculated_clip != *inherited_clip {
                *calculated_clip = inherited_clip.clone();
            }
        } else {
            // No inherited clipping rect, remove the component
            commands.entity(entity).remove::<CalculatedClip>();
        }
    } else if let Some(inherited_clip) = maybe_inherited_clip.as_ref() {
        // No previous calculated clip, add a new CalculatedClip component with the inherited clipping rect
        commands.entity(entity).try_insert(inherited_clip.clone());
    }

    // Calculate new clip rectangle for children nodes
    let children_clip = if maybe_inherited_clip
        .as_ref()
        .is_some_and(CalculatedClip::is_fully_clipped)
        || node.overflow.is_visible()
    {
        // The current node doesn't clip, propagate the optional inherited clipping rect to any children
        maybe_inherited_clip
    } else if let Some(clip_from_world) = transform.try_inverse() {
        let mut clip = maybe_inherited_clip.unwrap_or_default();
        clip.push_rect(
            computed_node.resolve_clip_rect(node.overflow, node.overflow_clip_margin),
            clip_from_world,
        );
        Some(clip)
    } else {
        Some(CalculatedClip::FullyClipped)
    };

    for child in ui_children.iter_ui_children(entity) {
        update_clipping(
            commands,
            ui_children,
            node_query,
            child,
            children_clip.clone(),
        );
    }
}
