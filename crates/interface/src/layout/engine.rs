use super::{LayoutBox, MAX_DEPTH, Overflow, Rules, Sizing, solver};
use crate::{canvas::CanvasItem, sand_text::SandText, workspace::WorkspaceMember};
use bevy::{
    picking::events::Scroll,
    prelude::*,
    text::{LineBreak, TextLayoutInfo},
    ui::widget::TextScroll,
};
use std::collections::{HashMap, HashSet};

#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct LayoutRuntime {
    pub parent: Option<Entity>,
    pub size: Vec2,
    pub content: Vec2,
    pub intrinsic: Vec2,
    pub scroll: Vec2,
    pub visual_offset: Vec2,
}

pub(super) fn rules(world: &World, entity: Entity) -> Option<Rules> {
    if let Some(text) = world.get::<SandText>(entity) {
        return Some(text_rules(text));
    }
    world
        .get::<LayoutBox>(entity)
        .map(|layout| layout.rules)
        .or_else(|| {
            world
                .get::<CanvasItem>(entity)
                .map(|item| Rules::fixed(item.size))
        })
}

fn text_rules(text: &SandText) -> Rules {
    text.layout.unwrap_or_else(|| {
        let mut rules = Rules::fixed(Vec2::from_array(text.size));
        if text.overflow == crate::sand_text::TextOverflow::Grow && text.editable {
            rules.axes[1].sizing = Sizing::Fit;
            rules.axes[1].min = text.size[1];
        } else if text.editable {
            rules.axes[1].overflow = Overflow::Scroll;
        }
        rules
    })
}

fn text_extent(layout: &TextLayoutInfo) -> Vec2 {
    layout
        .size
        .max(layout.cursor.map_or(Vec2::ZERO, |(_, cursor)| cursor.max))
}

pub fn configure(world: &mut World, entity: Entity, mut rules: Rules) -> Result<(), &'static str> {
    if !rules.valid() {
        return Err("Use valid sizes and limits.");
    }
    if regular_area(world, entity)
        && rules.axes[0].min.max(rules.axes[1].min) > rules.axes[0].max.min(rules.axes[1].max)
    {
        return Err("Square and circle areas need overlapping width and height limits.");
    }
    if world.get::<SandText>(entity).is_some() {
        if rules.axes[0].sizing == Sizing::Fit {
            rules.wrap = false;
        }
        let parent = world
            .get::<ChildOf>(entity)
            .ok_or("Text has no Sand.")?
            .parent();
        ensure(world, parent)?;
        world.get_mut::<SandText>(entity).unwrap().layout = Some(rules);
    } else {
        ensure(world, entity)?;
        world.get_mut::<LayoutBox>(entity).unwrap().rules = rules;
    }
    super::records::remember(world, entity);
    Ok(())
}

fn ensure(world: &mut World, entity: Entity) -> Result<(), &'static str> {
    if world.get::<LayoutBox>(entity).is_none() {
        let size = world
            .get::<CanvasItem>(entity)
            .ok_or("Select a Sand or an area.")?
            .size;
        world.entity_mut(entity).insert(LayoutBox::new(size));
        if world.get::<crate::area::InfluenceArea>(entity).is_some() {
            world.entity_mut(entity).insert(Pickable::default());
        }
    }
    Ok(())
}

fn regular_area(world: &World, entity: Entity) -> bool {
    world
        .get::<crate::area::InfluenceArea>(entity)
        .is_some_and(|area| {
            matches!(
                area.shape,
                crate::area::AreaShape::Square | crate::area::AreaShape::Circle
            )
        })
}

fn scope(world: &World, entity: Entity) -> Option<(Entity, u64)> {
    Some((
        world.get::<ChildOf>(entity)?.parent(),
        world.get::<WorkspaceMember>(entity)?.0,
    ))
}

pub fn attach(world: &mut World, child: Entity, parent: Entity) -> Result<(), &'static str> {
    if child == parent
        || scope(world, child).is_none()
        || scope(world, child) != scope(world, parent)
    {
        return Err("Choose a different container in this workspace.");
    }
    if world.get::<crate::sand_placement::Pinned>(child).is_some()
        || world.get::<crate::sand_placement::Pinned>(parent).is_some()
    {
        return Err("Unpin both items before nesting them.");
    }
    if world.get::<CanvasItem>(child).is_none() || world.get::<CanvasItem>(parent).is_none() {
        return Err("Choose a Sand or an area.");
    }
    let child_id = world.get::<LayoutBox>(child).map(|layout| layout.id);
    let mut ancestor = Some(parent);
    let mut seen = HashSet::new();
    while let Some(entity) = ancestor {
        if !seen.insert(entity) || seen.len() >= MAX_DEPTH || entity == child {
            return Err("This nesting would make a loop or exceed 32 levels.");
        }
        let next = world
            .get::<LayoutBox>(entity)
            .and_then(|layout| layout.parent);
        if next.is_some() && next == child_id {
            return Err("A Sand cannot contain itself through another container.");
        }
        ancestor = next.and_then(|id| {
            world
                .query::<(Entity, &LayoutBox)>()
                .iter(world)
                .find(|(candidate, layout)| {
                    layout.id == id && scope(world, *candidate) == scope(world, parent)
                })
                .map(|(entity, _)| entity)
        });
    }
    ensure(world, child)?;
    ensure(world, parent)?;
    let parent_box = *world.get::<LayoutBox>(parent).unwrap();
    let order = if world.get::<LayoutBox>(child).unwrap().parent == Some(parent_box.id) {
        world.get::<LayoutBox>(child).unwrap().order
    } else {
        world
            .query::<(Entity, &LayoutBox)>()
            .iter(world)
            .filter(|(entity, layout)| {
                layout.parent == Some(parent_box.id)
                    && scope(world, *entity) == scope(world, parent)
            })
            .map(|(_, layout)| layout.order)
            .max()
            .unwrap_or(-1)
            .saturating_add(1)
    };
    let parent_item = *world.get::<CanvasItem>(parent).unwrap();
    let item = *world.get::<CanvasItem>(child).unwrap();
    let offset = (item.position - item.size.as_dvec2() * 0.5 - parent_item.position
        + parent_item.size.as_dvec2() * 0.5)
        .as_vec2()
        .clamp(
            Vec2::splat(parent_box.rules.padding),
            Vec2::splat(super::LIMIT),
        );
    let mut layout = world.get_mut::<LayoutBox>(child).unwrap();
    layout.parent = Some(parent_box.id);
    layout.offset = offset.to_array();
    layout.order = order;
    super::records::remember(world, child);
    Ok(())
}

pub fn detach(world: &mut World, entity: Entity) {
    let size = world.get::<CanvasItem>(entity).map(|item| item.size);
    if let Some(mut layout) = world.get_mut::<LayoutBox>(entity) {
        if layout.parent.is_some()
            && let Some(size) = size
        {
            for (index, axis) in layout.rules.axes.iter_mut().enumerate() {
                if axis.sizing == Sizing::Fill {
                    axis.sizing = Sizing::Fixed;
                    axis.size = size[index].clamp(axis.min, axis.max);
                }
            }
        }
        layout.parent = None;
    }
    super::records::remember(world, entity);
}

pub(crate) fn edited(world: &mut World, entity: Entity, before: CanvasItem, after: CanvasItem) {
    let Some(mut layout) = world.get_mut::<LayoutBox>(entity) else {
        return;
    };
    if layout.parent.is_some() {
        let delta = (after.position - before.position).as_vec2() - (after.size - before.size) * 0.5;
        layout.offset = (Vec2::from_array(layout.offset) + delta)
            .clamp(Vec2::ZERO, Vec2::splat(super::LIMIT))
            .to_array();
    }
    for axis in 0..2 {
        if after.size[axis] != before.size[axis] {
            let rule = &mut layout.rules.axes[axis];
            rule.sizing = Sizing::Fixed;
            rule.size = after.size[axis].clamp(1.0, super::LIMIT);
            rule.min = rule.min.min(rule.size);
            rule.max = rule.max.max(rule.size);
        }
    }
    super::records::remember(world, entity);
}

pub(super) fn resolve(world: &mut World) {
    super::records::restore(world);
    let mut entries: Vec<_> = world
        .query::<(Entity, &LayoutBox, &CanvasItem)>()
        .iter(world)
        .filter(|(_, layout, _)| layout.valid())
        .map(|(entity, layout, item)| (entity, *layout, *item))
        .collect();
    entries.sort_by_key(|(entity, layout, _)| (layout.order, entity.to_bits()));
    let mut ids = HashMap::new();
    let mut duplicates = HashSet::new();
    for (index, (entity, layout, _)) in entries.iter().enumerate() {
        let key = (scope(world, *entity), layout.id);
        if ids.insert(key, index).is_some() {
            duplicates.insert(key);
        }
    }
    let mut entities: Vec<_> = entries.iter().map(|(entity, _, _)| *entity).collect();
    let mut items: Vec<_> = entries
        .iter()
        .map(|(entity, layout, item)| solver::Item {
            rules: layout.rules,
            equal_axes: regular_area(world, *entity),
            parent: layout.parent.and_then(|id| {
                let key = (scope(world, *entity), id);
                (!duplicates.contains(&key))
                    .then(|| ids.get(&key).copied())
                    .flatten()
            }),
            offset: Vec2::from_array(layout.offset),
            intrinsic: if world
                .get::<crate::sand_store::StoredSand>(*entity)
                .is_none()
                && world.get::<crate::area::InfluenceArea>(*entity).is_none()
            {
                world
                    .get::<ComputedNode>(*entity)
                    .map_or(Vec2::ZERO, |node| {
                        node.content_size * node.inverse_scale_factor()
                    })
            } else {
                Vec2::ZERO
            },
            size: item.size,
            available: scope(world, *entity)
                .and_then(|(root, _)| {
                    let node = world.get::<ComputedNode>(root)?;
                    let view = world.get::<crate::canvas::CanvasView>(root)?;
                    let size = node.size() * node.inverse_scale_factor() / view.zoom as f32;
                    (size.is_finite() && size.min_element() > 0.0).then_some(size)
                })
                .unwrap_or(item.size),
            content: Vec2::ZERO,
            children: Vec::new(),
        })
        .collect();
    for index in 0..items.len() {
        let mut next = Some(index);
        let mut seen = HashSet::new();
        while let Some(current) = next {
            if !seen.insert(current) || seen.len() > MAX_DEPTH {
                items[index].parent = None;
                break;
            }
            next = items[current].parent;
        }
    }
    for (parent, (entity, _, _)) in entries.iter().enumerate() {
        let mut children: Vec<_> = world
            .get::<Children>(*entity)
            .map(|children| children.iter().collect())
            .unwrap_or_default();
        children.sort_by_key(|entity| world.get::<SandText>(*entity).map_or(0, |text| text.order));
        for child in children {
            let Some(text) = world.get::<SandText>(child) else {
                continue;
            };
            let rules = text_rules(text);
            if !rules.valid() {
                continue;
            }
            let measured = world
                .get::<TextLayoutInfo>(child)
                .map_or(Vec2::ZERO, text_extent)
                * world
                    .get::<ComputedNode>(child)
                    .map_or(1.0, ComputedNode::inverse_scale_factor);
            items.push(solver::Item {
                rules,
                equal_axes: false,
                parent: Some(parent),
                offset: Vec2::from_array(text.offset),
                intrinsic: if measured.is_finite() {
                    measured.ceil() + Vec2::splat(rules.padding * 2.0)
                } else {
                    Vec2::ZERO
                },
                size: Vec2::from_array(text.size),
                available: Vec2::ZERO,
                content: Vec2::ZERO,
                children: Vec::new(),
            });
            entities.push(child);
        }
    }
    for index in 0..items.len() {
        if let Some(parent) = items[index].parent {
            items[parent].children.push(index);
        }
    }
    for item in &mut items {
        item.children.sort_by_key(|index| {
            let entity = entities[*index];
            let order = world
                .get::<LayoutBox>(entity)
                .map(|layout| layout.order)
                .or_else(|| world.get::<SandText>(entity).map(|text| text.order))
                .unwrap_or(0);
            order
        });
    }
    let mut order = Vec::with_capacity(items.len());
    let mut pending: Vec<_> = (0..items.len())
        .filter(|index| items[*index].parent.is_none())
        .collect();
    while let Some(index) = pending.pop() {
        order.push(index);
        pending.extend(items[index].children.iter().rev().copied());
    }
    solver::solve(&mut items, &order);
    let mut changed = false;
    for &index in &order {
        let entity = entities[index];
        let solved = &items[index];
        let old = world
            .get::<LayoutRuntime>(entity)
            .copied()
            .unwrap_or_default();
        let physical = world
            .get::<ComputedNode>(entity)
            .map_or(1.0, ComputedNode::inverse_scale_factor)
            .max(f32::EPSILON);
        let mut scroll = world
            .get::<TextScroll>(entity)
            .map_or(old.scroll, |scroll| scroll.0 * physical);
        for axis in 0..2 {
            scroll[axis] = if solved.rules.axes[axis].overflow == Overflow::Scroll {
                scroll[axis].clamp(0.0, (solved.content[axis] - solved.size[axis]).max(0.0))
            } else {
                0.0
            };
        }
        let parent = solved.parent.map(|parent| entities[parent]);
        let parent_state = parent
            .and_then(|parent| world.get::<LayoutRuntime>(parent))
            .copied()
            .unwrap_or_default();
        if let Some(parent) = parent.filter(|_| world.get::<CanvasItem>(entity).is_some()) {
            let parent_order = world.get::<ZIndex>(parent).map_or(0, |order| order.0);
            let child_order = world.get::<ZIndex>(entity).map_or(0, |order| order.0);
            if child_order <= parent_order {
                world
                    .entity_mut(entity)
                    .insert(ZIndex(parent_order.saturating_add(1)));
            }
        }
        let runtime = LayoutRuntime {
            parent,
            size: solved.size,
            content: solved.content,
            intrinsic: solved.intrinsic,
            scroll,
            visual_offset: parent_state.visual_offset + parent_state.scroll,
        };
        if old != runtime || world.get::<LayoutRuntime>(entity).is_none() {
            world.entity_mut(entity).insert(runtime);
            changed = true;
        }
        if let Some(mut text_scroll) = world.get_mut::<TextScroll>(entity) {
            text_scroll.set_if_neq(TextScroll(scroll / physical));
        }
        if world.get::<SandText>(entity).is_some() {
            let linebreak = if solved.rules.wrap && solved.rules.axes[0].sizing != Sizing::Fit {
                LineBreak::WordOrCharacter
            } else {
                LineBreak::NoWrap
            };
            if let Some(mut layout) = world.get_mut::<TextLayout>(entity) {
                if layout.linebreak != linebreak {
                    layout.linebreak = linebreak;
                    changed = true;
                }
            }
            let mut node = world.get_mut::<Node>(entity).unwrap();
            let padding = UiRect::all(px(solved.rules.padding));
            if node.padding != padding {
                node.padding = padding;
                changed = true;
            }
            let offset = solved.offset - parent_state.scroll;
            if node.width != px(solved.size.x)
                || node.height != px(solved.size.y)
                || node.left != px(offset.x)
                || node.top != px(offset.y)
            {
                node.width = px(solved.size.x);
                node.height = px(solved.size.y);
                node.left = px(offset.x);
                node.top = px(offset.y);
                changed = true;
            }
        } else {
            changed |= super::content::scroll(world, entity, runtime.scroll);
            let previous = *world.get::<CanvasItem>(entity).unwrap();
            let mut position = previous.position + (solved.size - previous.size).as_dvec2() * 0.5;
            if let Some(parent) = parent {
                let parent_item = world.get::<CanvasItem>(parent).unwrap();
                let placement = crate::topology::spatial(world, parent);
                let offset = -parent_item.size.as_dvec2() * 0.5
                    + solved.offset.as_dvec2()
                    + solved.size.as_dvec2() * 0.5;
                let point = placement.position(parent_item.position)
                    + placement.rotation() * bevy::math::DVec3::new(offset.x, 0.0, offset.y);
                position = bevy::math::DVec2::new(point.x, point.z);
                let mut child = crate::topology::spatial(world, entity);
                child.elevation = point.y;
                child.rotation = placement.rotation;
                if world.get::<crate::topology::Spatial>(entity) != Some(&child) {
                    world.entity_mut(entity).insert(child);
                }
            }
            if position != previous.position || solved.size != previous.size {
                let mut item = world.get_mut::<CanvasItem>(entity).unwrap();
                item.position = position;
                item.size = solved.size;
                changed = true;
                if let Some(mut area) = world.get_mut::<crate::area::InfluenceArea>(entity) {
                    area.center = position.to_array();
                    area.size = solved.size.as_dvec2().to_array();
                }
                if world.get::<crate::area::InfluenceArea>(entity).is_some() {
                    crate::area_mutation::disarm(
                        world,
                        entity,
                        "Property changes will resume with the updated layout.",
                    );
                }
            }
        }
    }
    if changed && let Some(wake) = world.get_resource::<crate::wake::WakeSignal>() {
        wake.ring();
    }
}

pub(super) fn consume_scroll(rules: Rules, runtime: &mut LayoutRuntime, delta: Vec2) -> Vec2 {
    let old = runtime.scroll;
    for axis in 0..2 {
        if rules.axes[axis].overflow == Overflow::Scroll {
            runtime.scroll[axis] = (old[axis] + delta[axis])
                .clamp(0.0, (runtime.content[axis] - runtime.size[axis]).max(0.0));
        }
    }
    delta - (runtime.scroll - old)
}

pub(super) fn finish(
    mut texts: Query<(
        &SandText,
        &TextLayoutInfo,
        &ComputedNode,
        &mut TextScroll,
        &mut LayoutRuntime,
    )>,
    wake: Option<Res<crate::wake::WakeSignal>>,
) {
    let mut changed = false;
    for (text, measured, node, mut scroll, mut runtime) in &mut texts {
        let rules = text_rules(text);
        let scale = node.inverse_scale_factor().max(f32::EPSILON);
        let content = text_extent(measured) * scale;
        if content.is_finite()
            && content.ceil() + Vec2::splat(rules.padding * 2.0) != runtime.content
        {
            changed = true;
        }
        let mut next_scroll = scroll.0;
        for axis in 0..2 {
            next_scroll[axis] = if rules.axes[axis].overflow == Overflow::Scroll {
                let maximum =
                    (text_extent(measured)[axis] - node.content_box().size()[axis]).max(0.0);
                scroll.0[axis].clamp(0.0, maximum)
            } else {
                0.0
            };
        }
        if next_scroll != scroll.0 {
            scroll.0 = next_scroll;
            changed = true;
        }
        if runtime.scroll != scroll.0 * scale {
            runtime.scroll = scroll.0 * scale;
        }
    }
    if changed && let Some(wake) = wake {
        wake.ring();
    }
}

pub(super) fn finish_content(
    contents: Query<
        (&ComputedNode, &LayoutRuntime),
        (
            With<CanvasItem>,
            Without<crate::sand_store::StoredSand>,
            Without<crate::area::InfluenceArea>,
        ),
    >,
    wake: Option<Res<crate::wake::WakeSignal>>,
) {
    if contents.iter().any(|(node, runtime)| {
        let measured = node.content_size * node.inverse_scale_factor();
        measured.is_finite() && measured != runtime.intrinsic
    }) && let Some(wake) = wake
    {
        wake.ring();
    }
}

pub(super) fn scroll(
    mut event: On<Pointer<Scroll>>,
    mut layouts: Query<(
        &mut LayoutRuntime,
        Option<&LayoutBox>,
        Option<&SandText>,
        Option<&ComputedNode>,
        Option<&mut TextScroll>,
    )>,
    parents: Query<&ChildOf>,
    mut scrolls: Query<(&mut ScrollPosition, &Node, &ComputedNode)>,
    keys: Res<ButtonInput<KeyCode>>,
    wake: Option<Res<crate::wake::WakeSignal>>,
) {
    if event.entity != event.original_event_target() {
        return;
    }
    let scale = if event.unit == bevy::input::mouse::MouseScrollUnit::Line {
        24.0
    } else {
        1.0
    };
    let mut delta = -Vec2::new(event.x, event.y) * scale;
    if keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]) && delta.x == 0.0 {
        delta = Vec2::new(delta.y, 0.0);
    }
    let mut next = Some(event.entity);
    let mut seen = HashSet::new();
    let mut consumed = false;
    while let Some(entity) = next {
        if !seen.insert(entity) || seen.len() > MAX_DEPTH * 2 {
            break;
        }
        next = parents.get(entity).ok().map(ChildOf::parent);
        if let Ok((mut position, node, computed)) = scrolls.get_mut(entity) {
            let maximum = ((computed.content_size - computed.size())
                * computed.inverse_scale_factor())
            .max(Vec2::ZERO);
            let before = position.0;
            for (axis, overflow) in [node.overflow.x, node.overflow.y].into_iter().enumerate() {
                if overflow == OverflowAxis::Scroll {
                    position.0[axis] = (before[axis] + delta[axis]).clamp(0.0, maximum[axis]);
                }
            }
            let moved = position.0 - before;
            consumed |= moved != Vec2::ZERO;
            delta -= moved;
            if delta.length_squared() < 0.01 {
                break;
            }
        }
        let Ok((mut runtime, layout, text, computed, text_scroll)) = layouts.get_mut(entity) else {
            continue;
        };
        let Some(rules) = text
            .map(text_rules)
            .or_else(|| layout.map(|layout| layout.rules))
        else {
            continue;
        };
        next = runtime.parent.or(next);
        let remaining = consume_scroll(rules, &mut runtime, delta);
        consumed |= remaining != delta;
        delta = remaining;
        if let Some(mut text_scroll) = text_scroll {
            text_scroll.0 = runtime.scroll
                / computed
                    .map_or(1.0, ComputedNode::inverse_scale_factor)
                    .max(f32::EPSILON);
        }
        if delta.length_squared() < 0.01 {
            break;
        }
    }
    if consumed {
        event.propagate(false);
        if let Some(wake) = wake {
            wake.ring();
        }
    }
}
