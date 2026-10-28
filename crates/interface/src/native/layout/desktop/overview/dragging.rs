use super::*;
use crate::layout::desktop::{DropSlot, OverviewSettings, edge_scroll};
use bevy::picking::pointer::{PointerButton, PointerId};
use std::time::{Duration, Instant};

#[derive(Clone, Copy)]
struct Destination {
    workspace: u64,
    slot: Option<DropSlot>,
    feedback: Entity,
}

pub(super) struct WindowDrag {
    pointer: PointerId,
    source: u64,
    id: [u8; 16],
    entity: Entity,
    owner: Entity,
    button: Entity,
    active: u64,
    viewport: Option<OutputViewport>,
    bounds: Rectangle,
    settings: OverviewSettings,
    point: Vec2,
    previous: Instant,
    wake: Option<Instant>,
    ghost: Entity,
    caption: Entity,
    destination: Option<Destination>,
}

pub(super) fn install(
    world: &mut World,
    button: Entity,
    root: Entity,
    workspace: u64,
    id: [u8; 16],
) {
    world.entity_mut(button).observe(
        move |mut event: On<Pointer<DragStart>>, mut commands: Commands| {
            if event.button == PointerButton::Primary {
                let pointer = event.pointer_id;
                let point = event.pointer_location.position;
                commands.queue(move |world: &mut World| {
                    begin(world, button, root, workspace, id, pointer, point)
                });
                event.propagate(false);
            }
        },
    );
    world
        .entity_mut(button)
        .observe(|mut event: On<Pointer<Drag>>, mut commands: Commands| {
            if event.button == PointerButton::Primary {
                let pointer = event.pointer_id;
                let point = event.pointer_location.position;
                commands.queue(move |world: &mut World| motion(world, pointer, point));
                event.propagate(false);
            }
        });
    world
        .entity_mut(button)
        .observe(|mut event: On<Pointer<DragEnd>>, mut commands: Commands| {
            if event.button == PointerButton::Primary {
                let pointer = event.pointer_id;
                let point = event.pointer_location.position;
                commands.queue(move |world: &mut World| finish(world, pointer, point));
                event.propagate(false);
            }
        });
    world
        .entity_mut(button)
        .observe(|mut event: On<Pointer<Cancel>>, mut commands: Commands| {
            let pointer = event.pointer_id;
            commands.queue(move |world: &mut World| cancel_pointer(world, pointer));
            event.propagate(false);
        });
}

pub(super) fn rectangle(world: &World, entity: Entity) -> Option<Rect> {
    if world
        .get::<InheritedVisibility>(entity)
        .is_some_and(|visibility| !visibility.get())
    {
        return None;
    }
    let mut bounds = crate::native::inspection::bounds(world, entity)?;
    if let Some(clip) = world.get::<bevy::ui::CalculatedClip>(entity) {
        let scale = world.get::<ComputedNode>(entity)?.inverse_scale_factor();
        bounds = bounds.intersect(Rect::from_corners(
            clip.clip.min * scale,
            clip.clip.max * scale,
        ));
    }
    (bounds.size().min_element() > 0.0).then_some(bounds)
}

pub(super) fn context(world: &World, session: &Session) -> bool {
    world.get_entity(session.overlay).is_ok()
        && output(world, session.root) == session.output
        && world
            .get_resource::<crate::native::shortcuts::KeyboardModalOwner>()
            .is_some_and(|owner| owner.0 == Some(session.overlay))
}

fn valid(world: &World, session: &Session, drag: &WindowDrag) -> bool {
    context(world, session)
        && world
            .get::<Workspaces>(session.root)
            .is_some_and(|spaces| spaces.active == drag.active)
        && super::super::viewport(world, session.root) == Some(drag.bounds)
        && world
            .get::<HostViewport>(session.root)
            .map(|viewport| &viewport.0)
            == drag.viewport.as_ref()
        && world.get_entity(drag.button).is_ok()
        && world
            .get::<LayoutBox>(drag.entity)
            .is_some_and(|layout| layout.id == drag.id)
        && super::super::transfers::owning_item(world, session.root, drag.source, drag.entity).ok()
            == Some(drag.owner)
        && drag.destination.is_none_or(|destination| {
            world.get_entity(destination.feedback).is_ok()
                && world.get::<Workspaces>(session.root).is_some_and(|spaces| {
                    spaces.entries.iter().any(|space| {
                        space.id == destination.workspace
                            && destination.slot.is_none_or(|slot| {
                                space.desktop.mode() == WorkspaceLayout::Tiled
                                    && space.desktop.fullscreen().is_none()
                                    && !space.desktop.minimized(slot.anchor)
                                    && space
                                        .desktop
                                        .items()
                                        .iter()
                                        .any(|item| item.id == slot.anchor && !item.floating)
                                    && valid_selection(
                                        world,
                                        session.root,
                                        destination.workspace,
                                        Some(slot.anchor.0),
                                    )
                            })
                    })
                })
        })
}

fn begin(
    world: &mut World,
    button: Entity,
    root: Entity,
    source: u64,
    id: [u8; 16],
    pointer: PointerId,
    point: Vec2,
) {
    let Some(mut overview) = world.remove_resource::<Overview>() else {
        return;
    };
    if let Some(session) = overview.0.as_mut()
        && session.root == root
        && session.drag.is_none()
        && context(world, session)
        && point.is_finite()
        && valid_selection(world, root, source, Some(id))
    {
        let entity = world
            .get::<Children>(root)
            .unwrap()
            .iter()
            .find(|entity| {
                world
                    .get::<LayoutBox>(*entity)
                    .is_some_and(|layout| layout.id == id)
            })
            .unwrap();
        let owner = super::super::transfers::owning_item(world, root, source, entity).ok();
        if let Some(owner) = owner
            && let Some(bounds) = super::super::viewport(world, root)
        {
            let ghost = world
                .spawn((
                    Name::new("overview.drag"),
                    Node {
                        position_type: PositionType::Absolute,
                        width: px(220),
                        max_width: percent(80),
                        max_height: percent(80),
                        padding: UiRect::all(px(8)),
                        border: UiRect::all(px(2)),
                        overflow: Overflow::clip(),
                        ..default()
                    },
                    Pickable::IGNORE,
                    GlobalZIndex(95),
                    crate::native::token_style::background(crate::native::tokens::Token::Surface),
                    crate::native::token_style::border(crate::native::tokens::Token::Accent),
                    ChildOf(session.overlay),
                ))
                .id();
            let name = item_name(world, entity);
            let caption = label(world, ghost, &name, 14.0);
            world.entity_mut(caption).insert((
                Pickable::IGNORE,
                TextLayout::linebreak(bevy::text::LineBreak::WordOrCharacter),
            ));
            {
                let mut node = world.get_mut::<Node>(caption).unwrap();
                node.width = percent(100);
                node.min_width = px(0);
                node.flex_shrink = 1.0;
            }
            world.entity_mut(button).remove::<bevy::ui::Pressed>();
            session.drag = Some(WindowDrag {
                pointer,
                source,
                id,
                entity,
                owner,
                button,
                active: world.get::<Workspaces>(root).unwrap().active,
                viewport: world
                    .get::<HostViewport>(root)
                    .map(|viewport| viewport.0.clone()),
                bounds,
                settings: settings::current(world),
                point,
                previous: Instant::now(),
                wake: None,
                ghost,
                caption,
                destination: None,
            });
            update(world, session, Instant::now());
        }
    }
    world.insert_resource(overview);
}

fn motion(world: &mut World, pointer: PointerId, point: Vec2) {
    let Some(mut overview) = world.remove_resource::<Overview>() else {
        return;
    };
    if let Some(session) = overview.0.as_mut()
        && let Some(drag) = session.drag.as_mut().filter(|drag| drag.pointer == pointer)
    {
        drag.point = point;
        update(world, session, Instant::now());
    }
    world.insert_resource(overview);
}

fn destination(
    world: &World,
    session: &Session,
    drag: &WindowDrag,
    point: Vec2,
) -> Option<Destination> {
    for card in session.cards.iter().rev() {
        if !rectangle(world, card.entity).is_some_and(|bounds| bounds.contains(point)) {
            continue;
        }
        let model = &world
            .get::<Workspaces>(session.root)?
            .entries
            .iter()
            .find(|entry| entry.id == card.workspace)?
            .desktop;
        let source = &world
            .get::<Workspaces>(session.root)?
            .entries
            .iter()
            .find(|entry| entry.id == drag.source)?
            .desktop;
        let tiled = world.get::<LayoutBox>(drag.owner).is_some_and(|layout| {
            source
                .items()
                .iter()
                .any(|item| item.id.0 == layout.id && !item.floating)
        });
        if model.mode() == WorkspaceLayout::Tiled
            && tiled
            && source.fullscreen().is_none()
            && model.fullscreen().is_none()
        {
            for id in card.order.iter().rev() {
                let (button, _) = card.items.get(id)?;
                let Some(bounds) =
                    rectangle(world, *button).filter(|bounds| bounds.contains(point))
                else {
                    continue;
                };
                let anchor = PlacementId(*id);
                if !model.minimized(anchor)
                    && model
                        .items()
                        .iter()
                        .any(|item| item.id == anchor && !item.floating)
                {
                    let uv = (point - bounds.min) / bounds.size();
                    let side = if uv.y < 0.2 {
                        Direction::Up
                    } else if uv.y > 0.8 {
                        Direction::Down
                    } else if uv.x < 0.5 {
                        Direction::Left
                    } else {
                        Direction::Right
                    };
                    return Some(Destination {
                        workspace: card.workspace,
                        slot: Some(DropSlot { anchor, side }),
                        feedback: *button,
                    });
                }
            }
        }
        return Some(Destination {
            workspace: card.workspace,
            slot: None,
            feedback: card.preview,
        });
    }
    None
}

fn clear(world: &mut World, drag: WindowDrag) {
    if let Some(destination) = drag.destination
        && let Some(mut outline) = world.get_mut::<Outline>(destination.feedback)
    {
        outline.width = px(0);
    }
    if let Ok(entity) = world.get_entity_mut(drag.ghost) {
        entity.despawn();
    }
}

pub(super) fn update(world: &mut World, session: &mut Session, now: Instant) {
    let Some(mut drag) = session.drag.take() else {
        return;
    };
    if !valid(world, session, &drag) || !drag.point.is_finite() {
        clear(world, drag);
        return;
    }
    let elapsed = now.saturating_duration_since(drag.previous);
    drag.previous = now;
    if scroll(world, session.list, drag.point, elapsed, drag.settings)
        && drag.wake.is_none_or(|deadline| now >= deadline)
        && let Some(wake) = world.get_resource::<crate::host::WakeSignal>()
    {
        let delay = Duration::from_millis(16);
        drag.wake = now.checked_add(delay);
        wake.after(delay);
    }
    if let Some(previous) = drag.destination
        && let Some(mut outline) = world.get_mut::<Outline>(previous.feedback)
    {
        outline.width = px(0);
    }
    drag.destination = destination(world, session, &drag, drag.point);
    if let Some(destination) = drag.destination {
        world.entity_mut(destination.feedback).insert((
            Outline {
                width: px(3),
                offset: px(2),
                color: crate::native::theme::PURPLE,
            },
            crate::native::token_style::OutlineToken(crate::native::tokens::Token::Accent),
        ));
    }
    let help = match drag.destination {
        Some(destination) => {
            let workspace = world
                .get::<Workspaces>(session.root)
                .unwrap()
                .entries
                .iter()
                .find(|space| space.id == destination.workspace)
                .unwrap();
            let relation = destination
                .slot
                .and_then(|slot| {
                    let anchor = world.get::<Children>(session.root)?.iter().find(|entity| {
                        world
                            .get::<LayoutBox>(*entity)
                            .is_some_and(|layout| layout.id == slot.anchor.0)
                    })?;
                    let direction = match slot.side {
                        Direction::Left => "to the left of",
                        Direction::Right => "to the right of",
                        Direction::Up => "above",
                        Direction::Down => "below",
                    };
                    Some(format!(
                        "{direction} {} · {}",
                        item_name(world, anchor),
                        workspace.name
                    ))
                })
                .unwrap_or_else(|| format!("to {}", workspace.name));
            format!(
                "Move {} {relation} · Escape cancels",
                item_name(world, drag.entity)
            )
        }
        None => "Drag to an overview workspace · Escape cancels".into(),
    };
    if world
        .get::<Text>(drag.caption)
        .is_some_and(|text| text.0 != help)
    {
        world.get_mut::<Text>(drag.caption).unwrap().0 = help;
    }
    if let Some(bounds) = rectangle(world, session.overlay) {
        let extent = world
            .get::<ComputedNode>(drag.ghost)
            .map(|node| node.size() * node.inverse_scale_factor())
            .filter(|size| size.min_element() > 0.0)
            .unwrap_or(Vec2::new((bounds.width() * 0.8).min(220.0), 40.0));
        if let Some(mut node) = world.get_mut::<Node>(drag.ghost) {
            node.left = px((drag.point.x - bounds.min.x + 12.0)
                .clamp(0.0, (bounds.width() - extent.x).max(0.0)));
            node.top = px((drag.point.y - bounds.min.y + 12.0)
                .clamp(0.0, (bounds.height() - extent.y).max(0.0)));
        }
    }
    session.drag = Some(drag);
}

fn finish(world: &mut World, pointer: PointerId, point: Vec2) {
    let Some(mut overview) = world.remove_resource::<Overview>() else {
        return;
    };
    if let Some(session) = overview.0.as_mut()
        && session
            .drag
            .as_ref()
            .is_some_and(|drag| drag.pointer == pointer)
    {
        let mut drag = session.drag.take().unwrap();
        drag.point = point;
        if valid(world, session, &drag)
            && point.is_finite()
            && let Some(destination) = destination(world, session, &drag, point)
        {
            let result = super::super::transfers::prepare_captured(
                world,
                session.root,
                drag.source,
                PlacementId(drag.id),
                destination.workspace,
                destination.slot,
            )
            .and_then(|prepared| prepared.commit(world));
            match result {
                Ok(_) => {
                    session.selected = (destination.workspace, Some(drag.id));
                    session.focus_selection = true;
                }
                Err(error) => crate::native::notifications::report(world, "Overview", &error),
            }
        }
        clear(world, drag);
    }
    world.insert_resource(overview);
}

pub(super) fn scroll(
    world: &mut World,
    list: Entity,
    point: Vec2,
    elapsed: Duration,
    settings: OverviewSettings,
) -> bool {
    let Some(bounds) = rectangle(world, list).and_then(|bounds| {
        Rectangle::new(
            bounds.min.as_dvec2().to_array(),
            bounds.size().as_dvec2().to_array(),
        )
    }) else {
        return false;
    };
    let Some(node) = world.get::<ComputedNode>(list) else {
        return false;
    };
    let limit = (node.content_size().y - node.size().y).max(0.0) * node.inverse_scale_factor();
    let amount = edge_scroll(point.as_dvec2().to_array(), bounds, elapsed, settings) as f32;
    if !limit.is_finite() || amount == 0.0 {
        return false;
    }
    let Some(mut position) = world.get_mut::<ScrollPosition>(list) else {
        return false;
    };
    if !position.0.y.is_finite() {
        return false;
    }
    let next = (position.0.y + amount).clamp(0.0, limit);
    if next == position.0.y {
        return false;
    }
    position.0.y = next;
    true
}

fn cancel_pointer(world: &mut World, pointer: PointerId) {
    let Some(mut overview) = world.remove_resource::<Overview>() else {
        return;
    };
    if let Some(session) = overview.0.as_mut()
        && session
            .drag
            .as_ref()
            .is_some_and(|drag| drag.pointer == pointer)
    {
        clear(world, session.drag.take().unwrap());
    }
    world.insert_resource(overview);
}

#[cfg(test)]
mod tests;
