// non-lean-yet
// Scope: struct OrderState, fn needs_metadata, fn priority, fn has_order.
// BeautifulRust: No site flagged by this pass. Check contracts and feature coverage.

use super::{CanvasToolbar, ToolbarOrder, ToolbarScroll};
use crate::native::icons::{AnchoredTooltip, InlineTooltip, TooltipIcon};
use bevy::{ecs::query::QueryState, prelude::*};

pub(super) struct OrderState {
    bars: QueryState<Entity, With<CanvasToolbar>>,
    repairs: Vec<Entity>,
    metadata: Vec<Entity>,
    children: Vec<Entity>,
    ordered: Vec<Entity>,
    pinned: Vec<Entity>,
}

impl FromWorld for OrderState {
    fn from_world(world: &mut World) -> Self {
        Self {
            bars: world.query_filtered(),
            repairs: Vec::new(),
            metadata: Vec::new(),
            children: Vec::new(),
            ordered: Vec::new(),
            pinned: Vec::new(),
        }
    }
}

fn needs_metadata(world: &World, control: Entity) -> bool {
    world.get::<InlineTooltip>(control).is_none()
        || world.get::<AnchoredTooltip>(control).is_none()
        || world
            .get::<TooltipIcon>(control)
            .is_none_or(|tooltip| tooltip.source != control)
}

fn priority(world: &World, control: Entity) -> u8 {
    world
        .get::<ToolbarOrder>(control)
        .map_or(u8::MAX, |order| order.0)
}

fn has_order(world: &World, parent: Entity, ordered: &[Entity]) -> bool {
    world
        .get::<Children>(parent)
        .map_or(ordered.is_empty(), |current| {
            current.iter().eq(ordered.iter().copied())
        })
}

fn inspect(world: &World, bar: Entity, metadata: &mut Vec<Entity>) -> bool {
    let Some(children) = world.get::<Children>(bar) else {
        return false;
    };
    let scroll = children
        .iter()
        .find(|child| world.get::<ToolbarScroll>(*child).is_some());
    let mut canonical = scroll.is_some() && children.last().copied() == scroll;
    let mut checked = 0;
    for child in children.iter().filter(|child| Some(*child) != scroll) {
        checked += 1;
        if needs_metadata(world, child) {
            metadata.push(child);
        }
        canonical &= priority(world, child) == 0;
    }
    if let Some(scroll) = scroll
        && let Some(children) = world.get::<Children>(scroll)
    {
        let mut previous = 0;
        for child in children.iter() {
            checked += 1;
            if needs_metadata(world, child) {
                metadata.push(child);
            }
            let current = priority(world, child);
            canonical &= current >= previous
                && world
                    .get::<ChildOf>(child)
                    .is_some_and(|parent| parent.parent() == scroll);
            previous = current;
        }
    }
    nucleus::performance::add("native-toolbar-controls-checked", checked);
    canonical
}

fn normalize(
    world: &mut World,
    bar: Entity,
    children: &mut Vec<Entity>,
    ordered: &mut Vec<Entity>,
    pinned: &mut Vec<Entity>,
) {
    children.clear();
    if let Some(current) = world.get::<Children>(bar) {
        children.extend(current.iter());
    }
    let scroll = children
        .iter()
        .copied()
        .find(|child| world.get::<ToolbarScroll>(*child).is_some())
        .unwrap_or_else(|| {
            world
                .spawn((
                    ToolbarScroll,
                    Node {
                        min_width: px(0),
                        flex_shrink: 1.0,
                        align_items: AlignItems::Center,
                        column_gap: px(6),
                        overflow: Overflow::scroll_x(),
                        ..default()
                    },
                    ScrollPosition::default(),
                    ChildOf(bar),
                ))
                .id()
        });
    ordered.clear();
    if let Some(current) = world.get::<Children>(scroll) {
        ordered.extend(current.iter());
    }
    pinned.clear();
    for child in children.iter().copied().filter(|child| *child != scroll) {
        if priority(world, child) == 0 {
            pinned.push(child);
        } else {
            ordered.push(child);
        }
    }
    ordered.sort_by_key(|child| priority(world, *child));
    for child in ordered.iter().copied() {
        if world
            .get::<ChildOf>(child)
            .is_none_or(|parent| parent.parent() != scroll)
        {
            world.entity_mut(child).insert(ChildOf(scroll));
        }
    }
    if !has_order(world, scroll, ordered) {
        world.entity_mut(scroll).replace_children(ordered);
    }
    pinned.push(scroll);
    if !has_order(world, bar, pinned) {
        world.entity_mut(bar).replace_children(pinned);
    }
    nucleus::performance::add("native-toolbar-order-repairs", 1);
}

pub(super) fn order_controls(world: &mut World, mut state: Local<OrderState>) {
    let _maintenance = engine::performance::Span::new("native-toolbar-maintenance");
    let OrderState {
        bars,
        repairs,
        metadata,
        children,
        ordered,
        pinned,
    } = &mut *state;
    repairs.clear();
    metadata.clear();
    for bar in bars.iter(world) {
        if !inspect(world, bar, metadata) {
            repairs.push(bar);
        }
    }
    for control in metadata.drain(..) {
        if needs_metadata(world, control) {
            world.entity_mut(control).insert((
                InlineTooltip,
                AnchoredTooltip,
                TooltipIcon { source: control },
            ));
            nucleus::performance::add("native-toolbar-metadata-updates", 1);
        }
    }
    for bar in repairs.drain(..) {
        normalize(world, bar, children, ordered, pinned);
    }
}
