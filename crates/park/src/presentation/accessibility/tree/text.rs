use super::*;
use accesskit::{TextDirection, TextPosition, TextSelection};

pub(super) fn project(
    world: &World,
    view: &crate::native_input::projection::View,
    geometry: Option<&lince_interface::host::GeometrySnapshot>,
    entity: Entity,
    field: &mut Node,
    source: Vec<(NodeId, Node)>,
    nodes: &mut Vec<(NodeId, Node)>,
    spans: &mut BTreeMap<NodeId, Span>,
    identities: &mut BTreeMap<(NodeId, usize), NodeId>,
    alive: &mut BTreeSet<(NodeId, usize)>,
) -> Result<(), ()> {
    if source.iter().any(|(_, node)| {
        let value = node.value().unwrap_or_default();
        let mut offset = 0_usize;
        for length in node.character_lengths() {
            let Some(end) = offset.checked_add(usize::from(*length)) else { return true; };
            if value.get(offset..end).is_none() { return true; }
            offset = end;
        }
        offset != value.len()
    }) {
        field.clear_children(); field.clear_text_selection();
        return Ok(());
    }
    let origin = world.get::<ComputedNode>(entity).map(|node| node.content_box().min)
        .unwrap_or(Vec2::ZERO) - world.get::<bevy::ui::widget::TextScroll>(entity).map_or(Vec2::ZERO, |scroll| scroll.0);
    let mut groups = BTreeMap::<NodeId, Vec<NodeId>>::new();
    let mut links = BTreeMap::new();
    let selection = field.text_selection().copied();
    let start = nodes.len();
    for (source_id, original) in source {
        let count = original.character_lengths().len();
        if nodes.len().saturating_add(count.max(1)) >= MAX_NODES { return Err(()); }
        let mut group = Vec::with_capacity(count.max(1));
        let mut offset = 0;
        let mut style = original.clone();
        style.clear_children(); style.clear_transform(); style.clear_next_on_line(); style.clear_previous_on_line();
        style.clear_value(); style.clear_character_lengths(); style.clear_character_positions(); style.clear_character_widths(); style.clear_word_starts();
        for index in 0..count.max(1) {
            let length = original.character_lengths().get(index).copied().unwrap_or(0);
            let end = offset + usize::from(length);
            let mut node = style.clone();
            node.set_value(original.value().unwrap_or_default()[offset..end].to_owned());
            node.set_character_lengths(if count == 0 { Vec::new() } else { vec![length] });
            node.set_word_starts(if original.word_starts().contains(&(index as u8)) { vec![0] } else { Vec::new() });
            let direction = original.text_direction().unwrap_or(TextDirection::LeftToRight);
            let local = cluster_bounds(&original, index);
            let projected = local.and_then(|local| {
                let bounds = Rect::from_corners(
                    Vec2::new(local.x0 as f32, local.y0 as f32) + origin,
                    Vec2::new(local.x1 as f32, local.y1 as f32) + origin,
                );
                projected_bounds(world, view, geometry, entity, bounds, direction)
            });
            if let Some((bounds, direction)) = projected {
                node.set_bounds(bounds);
                node.set_text_direction(direction);
                let width = if original.character_widths().and_then(|widths| widths.get(index)).is_some_and(|width| *width == 0.0) { 0.0 }
                    else if matches!(direction, TextDirection::LeftToRight | TextDirection::RightToLeft) { bounds.x1 - bounds.x0 }
                    else { bounds.y1 - bounds.y0 };
                node.set_character_positions(if count == 0 { Vec::new() } else { vec![0.0] });
                node.set_character_widths(if count == 0 { Vec::new() } else { vec![width as f32] });
                if let Some(size) = original.font_size() && let Some(local) = local {
                    let height = if matches!(direction, TextDirection::LeftToRight | TextDirection::RightToLeft) { bounds.y1 - bounds.y0 } else { bounds.x1 - bounds.x0 };
                    let scale = height / (local.y1 - local.y0);
                    let size = f64::from(size) * scale;
                    if size.is_finite() && size > 0.0 && size <= f64::from(f32::MAX) { node.set_font_size(size as f32); }
                }
            } else {
                node.clear_bounds(); node.clear_character_positions(); node.clear_character_widths();
            }
            let key = (source_id, index);
            let id = *identities.entry(key).or_insert_with(node_id);
            alive.insert(key);
            group.push(id);
            spans.insert(id, Span { entity, source: TextPosition { node: source_id, character_index: index }, length: usize::from(count != 0) });
            nodes.push((id, node));
            offset = end;
        }
        links.insert(source_id, (original.previous_on_line(), original.next_on_line()));
        groups.insert(source_id, group);
    }
    let mut neighbours = BTreeMap::new();
    for (source, group) in &groups {
        let (previous, next) = links[source];
        for (index, id) in group.iter().enumerate() {
            let previous = index.checked_sub(1).and_then(|index| group.get(index)).copied()
                .or_else(|| previous.and_then(|previous| groups.get(&previous)?.last().copied()));
            let next = group.get(index + 1).copied().or_else(|| next.and_then(|next| groups.get(&next)?.first().copied()));
            neighbours.insert(*id, (previous, next));
        }
    }
    for (id, node) in &mut nodes[start..] {
        let (previous, next) = neighbours[&*id];
        if let Some(previous) = previous { node.set_previous_on_line(previous); }
        if let Some(next) = next { node.set_next_on_line(next); }
    }
    let children = field.children().iter().flat_map(|id| groups.get(id).into_iter().flatten().copied()).collect::<Vec<_>>();
    field.set_children(children);
    field.clear_text_selection();
    if let Some(selection) = selection.and_then(|selection| exported_selection(selection, &groups, spans)) { field.set_text_selection(selection); }
    Ok(())
}

fn cluster_bounds(node: &Node, index: usize) -> Option<accesskit::Rect> {
    let mut bounds = node.bounds()?;
    if node.character_lengths().is_empty() { return Some(bounds); }
    let position = f64::from(*node.character_positions()?.get(index)?);
    let width = f64::from(*node.character_widths()?.get(index)?);
    if !position.is_finite() || !width.is_finite() || width < 0.0 { return None; }
    match node.text_direction()? {
        TextDirection::LeftToRight => { bounds.x0 += position; bounds.x1 = bounds.x0 + width; }
        TextDirection::RightToLeft => { bounds.x1 -= position; bounds.x0 = bounds.x1 - width; }
        TextDirection::TopToBottom => { bounds.y0 += position; bounds.y1 = bounds.y0 + width; }
        TextDirection::BottomToTop => { bounds.y1 -= position; bounds.y0 = bounds.y1 - width; }
    }
    Some(bounds)
}

fn exported_selection(selection: TextSelection, groups: &BTreeMap<NodeId, Vec<NodeId>>, spans: &BTreeMap<NodeId, Span>) -> Option<TextSelection> {
    let position = |position: TextPosition| {
        let group = groups.get(&position.node)?;
        let first = spans.get(group.first()?)?;
        let count = if first.length == 0 { 0 } else { group.len() };
        if position.character_index > count { return None; }
        let index = position.character_index.min(group.len().saturating_sub(1));
        Some(TextPosition { node: *group.get(index)?, character_index: usize::from(position.character_index == count && count != 0) })
    };
    Some(TextSelection { anchor: position(selection.anchor)?, focus: position(selection.focus)? })
}

fn projected_bounds(world: &World, view: &crate::native_input::projection::View, geometry: Option<&lince_interface::host::GeometrySnapshot>, entity: Entity, bounds: Rect, direction: TextDirection) -> Option<(accesskit::Rect, TextDirection)> {
    let points = crate::native_input::projection::accessibility_points(world, view, geometry, entity, Some(bounds))?;
    let mapping = world.get_resource::<lince_interface::host::mapping::Snapshot>().filter(|snapshot| !snapshot.outputs().is_empty());
    let mapping = match mapping { Some(snapshot) => Some(snapshot.get(view.output)?), None => None };
    displayed_points(points, mapping, direction)
}

fn displayed_points(mut points: [[f64; 2]; 4], mapping: Option<&lince_interface::host::mapping::Mapping>, direction: TextDirection) -> Option<(accesskit::Rect, TextDirection)> {
    if let Some(mapping) = mapping {
        for point in &mut points { *point = mapping.logical_to_displayed(*point)?; }
    }
    let (start, end) = match direction {
        TextDirection::LeftToRight => (points[0], points[1]),
        TextDirection::RightToLeft => (points[1], points[0]),
        TextDirection::TopToBottom => (points[0], points[3]),
        TextDirection::BottomToTop => (points[3], points[0]),
    };
    let delta = [end[0] - start[0], end[1] - start[1]];
    let direction = if delta[0].abs() >= delta[1].abs() {
        if delta[0] < 0.0 { TextDirection::RightToLeft } else { TextDirection::LeftToRight }
    } else if delta[1] < 0.0 { TextDirection::BottomToTop } else { TextDirection::TopToBottom };
    let mut low: [f64; 2] = std::array::from_fn(|axis| points.iter().map(|point| point[axis]).fold(f64::INFINITY, f64::min));
    let mut high: [f64; 2] = std::array::from_fn(|axis| points.iter().map(|point| point[axis]).fold(f64::NEG_INFINITY, f64::max));
    if let Some(mapping) = mapping {
        let start = mapping.raster().logical_point([0.0; 2]);
        let end = mapping.raster().logical_point(mapping.source_size().map(f64::from));
        for axis in 0..2 { low[axis] = low[axis].max(start[axis]); high[axis] = high[axis].min(end[axis]); }
    }
    if (0..2).any(|axis| !low[axis].is_finite() || !high[axis].is_finite() || high[axis] < low[axis]) { return None; }
    Some((accesskit::Rect::new(low[0], low[1], high[0], high[1]), direction))
}

#[cfg(test)]
mod tests;
