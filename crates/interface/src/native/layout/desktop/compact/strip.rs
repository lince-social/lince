use super::*;
use crate::native::{actions::ActionButton, topology::presentation::VisualOwner};
use bevy::picking::events::Scroll;

const HEIGHT: f32 = crate::layout::desktop::COMPACT_HEADER_HEIGHT as f32;

#[derive(Component)]
struct Strip {
    target: Target,
    members: Vec<Target>,
    list: Entity,
    buttons: Vec<(Entity, Entity)>,
    add: Entity,
}

fn key(target: &Target, members: &[Target]) -> (Entity, u64, PlacementId) {
    (target.root, target.workspace, members.iter().map(|member| member.placement).min().unwrap())
}

fn open(world: &mut World, target: Target, members: Vec<Target>, scroll: f32) -> Entity {
    let panel = world.spawn((Node {
        position_type: PositionType::Absolute, min_width: px(0), height: px(HEIGHT),
        flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(4),
        padding: UiRect::all(px(2)), overflow: Overflow::clip(), ..default()
    }, crate::native::inspection::InspectionExcluded, Visibility::Hidden,
        Name::new("desktop.compact-members"), GlobalZIndex(24), ChildOf(target.root),
        VisualOwner(target.entity), crate::native::token_style::background(crate::native::tokens::Token::Surface))).id();
    let active = members.iter().position(|member| member.placement == target.placement).unwrap();
    let width = bounds(world, &target).map_or(320.0, |bounds| bounds.width()).max(112.0) - 40.0;
    let low = active as f32 * 168.0;
    let scroll = if low < scroll { low } else { scroll.max(low + 164.0 - width) }.max(0.0);
    let list = world.spawn((Node {
        min_width: px(0), height: percent(100), flex_grow: 1.0, flex_shrink: 1.0,
        flex_direction: FlexDirection::Row, column_gap: px(4), overflow: Overflow::scroll_x(), ..default()
    }, ScrollPosition(Vec2::new(scroll, 0.0)), crate::native::edit_mode::FocusScroll, ChildOf(panel))).id();
    world.entity_mut(list).observe(|mut event: On<Pointer<Scroll>>| event.propagate(false));
    let mut buttons = Vec::new();
    for (index, member) in members.iter().enumerate() {
        let caption = title(world, member, index);
        let (button, label) = input_settings::button(world, list, panel, &caption, Change::Select(member.clone()));
        world.entity_mut(button).insert((Node { width: px(164), min_width: px(72), max_width: px(164), height: percent(100), flex_shrink: 0.0,
            align_items: AlignItems::Center, justify_content: JustifyContent::Center, padding: UiRect::axes(px(6), px(2)), ..default() },
            Name::new(format!("compact.member.{}", member.entity.to_bits())), VisualOwner(member.entity)));
        world.entity_mut(label).insert(TextLayout::linebreak(bevy::text::LineBreak::NoWrap));
        buttons.push((button, label));
    }
    let add = world.spawn((crate::native::sand::button(0), Node { height: percent(100), width: px(32), flex_shrink: 0.0,
        align_items: AlignItems::Center, justify_content: JustifyContent::Center, ..default() },
        ActionButton::new(panel, crate::native::actions![Change::Action(target.clone(), DesktopAction::AddCompactMembers)]), ChildOf(panel))).id();
    crate::native::edit_mode::label(world, add, "+", 16.0);
    world.get_mut::<bevy::a11y::AccessibilityNode>(add).unwrap().set_label("Add compact members");
    world.entity_mut(panel).insert(Strip { target, members, list, buttons, add });
    panel
}

fn title(world: &World, target: &Target, index: usize) -> String {
    let title = overview::item_name(world, target.entity);
    let short: String = title.chars().take(16).collect();
    format!("{} · {}{}", index + 1, short, if title.chars().count() > 16 { "…" } else { "" })
}

fn bounds(world: &World, target: &Target) -> Option<Rect> {
    if let Some(bounds) = crate::native::topology::presentation::bounds(world, target.entity) { return Some(bounds); }
    let bounds = crate::native::canvas_selection::screen_bounds(world, target.root, target.entity)?;
    let item = crate::native::canvas::presented(world, target.entity)?;
    let size = compact_extent(world, target.entity).map_or(item.size, |clip| clip.min(item.size));
    Some(Rect::from_corners(bounds.min, bounds.min + bounds.size() * size / item.size))
}

pub(super) fn refresh(world: &mut World) {
    let roots: Vec<_> = world.query::<(Entity, &Workspaces)>().iter(world).filter_map(|(root, spaces)| {
        let workspace = spaces.entries.iter().find(|workspace| workspace.id == spaces.active)?;
        (workspace.desktop.mode() == WorkspaceLayout::Tiled && workspace.desktop.fullscreen().is_none()).then(|| (root, workspace.desktop.compact_groups().to_vec()))
    }).collect();
    let mut desired = Vec::new();
    for (root, groups) in roots {
        for group in groups {
            let Some(children) = world.get::<Children>(root) else { continue; };
            let members = group.members.iter().map(|id| {
                let entity = children.iter().find(|entity| world.get::<Managed>(*entity).is_some_and(|managed| managed.id == *id))?;
                capture(world, root, entity, true).ok()
            }).collect::<Option<Vec<_>>>();
            let Some(members) = members else { continue; };
            let Some(target) = members.iter().find(|member| member.placement == group.active).cloned() else { continue; };
            if model(world, &target, false).is_ok() { desired.push((target, members)); }
        }
    }
    let mut existing: BTreeMap<_, _> = world.query::<(Entity, &Strip)>().iter(world).map(|(entity, strip)| (key(&strip.target, &strip.members), entity)).collect();
    let mut visible = Vec::new();
    for (target, members) in desired {
        let previous = existing.remove(&key(&target, &members));
        let same = previous.is_some_and(|entity| world.get::<Strip>(entity).is_some_and(|strip| strip.target == target && strip.members == members));
        let entity = if same { previous.unwrap() } else {
            let scroll = previous.and_then(|entity| world.get::<Strip>(entity)).and_then(|strip| world.get::<ScrollPosition>(strip.list)).map_or(0.0, |position| position.x);
            if let Some(previous) = previous { world.despawn(previous); }
            open(world, target, members, scroll)
        };
        visible.push(entity);
    }
    for entity in existing.into_values() { world.despawn(entity); }
    for entity in visible { position(world, entity); }
}

fn position(world: &mut World, entity: Entity) {
    let strip = world.get::<Strip>(entity).unwrap();
    let target = strip.target.clone();
    let Some(viewport) = crate::native::inspection::bounds(world, target.root) else { return; };
    let Some(item) = bounds(world, &target) else { return; };
    let display = item.intersect(viewport).size().min_element() > 0.0;
    let width = item.width().max(112.0).min(viewport.width());
    let left = item.min.x.clamp(viewport.min.x, (viewport.max.x - width).max(viewport.min.x)) - viewport.min.x;
    let top = (item.min.y - HEIGHT).max(viewport.min.y) - viewport.min.y;
    let add = strip.add;
    let rows = strip.members.iter().zip(&strip.buttons).enumerate().map(|(index, (member, buttons))|
        (*buttons, title(world, member, index), overview::item_name(world, member.entity), member.placement == target.placement)).collect::<Vec<_>>();
    if let Some(mut node) = world.get_mut::<Node>(entity) {
        if node.left != px(left) { node.left = px(left); }
        if node.top != px(top) { node.top = px(top); }
        if node.width != px(width) { node.width = px(width); }
    }
    if let Some(mut visibility) = world.get_mut::<Visibility>(entity) {
        let value = if display { Visibility::Inherited } else { Visibility::Hidden };
        if *visibility != value { *visibility = value; }
    }
    let disabled = unavailable(world, &target, DesktopAction::AddCompactMembers).is_some();
    input_settings::disable(world, add, disabled);
    for ((button, label), title, accessible, selected) in rows {
        let disabled = busy(world);
        input_settings::disable(world, button, disabled);
        input_settings::text(world, label, title);
        if let Some(mut node) = world.get_mut::<bevy::a11y::AccessibilityNode>(button) {
            let toggled = if selected { accesskit::Toggled::True } else { accesskit::Toggled::False };
            if node.label() != Some(accessible.as_str()) { node.set_label(accessible); }
            if node.toggled() != Some(toggled) { node.set_toggled(toggled); }
        }
    }
}
