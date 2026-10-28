use super::*;
use crate::native::canvas::ItemPreview;
use crate::native::{canvas::SpatialPreview, topology::Spatial};
use crate::{
    host::Manipulation,
    layout::desktop::{DropSlot, Gesture, Solution, Workspace},
};

#[derive(Component)]
pub(crate) struct Outline;

pub mod pointer;

#[derive(Resource, Default)]
struct State(Option<Session>);

struct Session {
    root: Entity,
    target: Entity,
    workspace: u64,
    output: Option<OutputViewport>,
    bounds: Rectangle,
    parent: Option<TransientParent>,
    gesture: Gesture,
    operation: Manipulation,
    saved: Option<focus::ReturnState>,
    delta: [f64; 2],
    slot: Option<DropSlot>,
    previews: Vec<Entity>,
    outline: Option<Entity>,
    basis: Workspace,
    proposed: Workspace,
    spatial: Spatial,
    displayed: CanvasItem,
    uv: [f64; 2],
    scale: f64,
    maximized: bool,
}

struct Proposal {
    model: Workspace,
    solution: Solution,
    placement: Option<transients::Placement>,
    spatial: Spatial,
}

pub fn active(world: &World) -> bool {
    world
        .get_resource::<State>()
        .is_some_and(|state| state.0.is_some())
}

pub fn window_state(world: &World, entity: Entity) -> Option<crate::host::WindowState> {
    world.get::<ItemPreview>(entity)?;
    let session = world.get_resource::<State>()?.0.as_ref()?;
    let id = world.get::<Managed>(entity)?.id;
    session.proposed.items().iter().find(|item| item.id == id)?;
    Some(crate::host::WindowState {
        minimized: session.proposed.minimized(id),
        fullscreen: session.proposed.protocol_fullscreen(id),
        windowed_fullscreen: session.proposed.windowed_fullscreen(id),
        maximized: session.proposed.maximized(id),
        resizing: resizing(world, entity),
    })
}

pub fn resizing(world: &World, entity: Entity) -> bool {
    world
        .get_resource::<State>()
        .and_then(|state| state.0.as_ref())
        .is_some_and(|session| {
            session.operation == Manipulation::Resize
                && (session.target == entity
                    || session.previews.contains(&entity)
                        && world
                            .get::<ItemPreview>(entity)
                            .zip(world.get::<CanvasItem>(entity))
                            .is_some_and(|(preview, committed)| preview.0.size != committed.size))
        })
}

pub(crate) fn held(world: &World, entity: Entity) -> bool {
    world
        .get_resource::<State>()
        .and_then(|state| state.0.as_ref())
        .is_some_and(|session| {
            session.target == entity
                || session.previews.contains(&entity)
                || transients::descendant(world, session.target, entity)
        })
}

pub fn begin(
    world: &mut World,
    root: Entity,
    target: Entity,
    operation: Manipulation,
    uv: [f64; 2],
) -> bool {
    begin_with_edges(world, root, target, operation, uv, None)
}

pub fn begin_with_edges(
    world: &mut World,
    root: Entity,
    target: Entity,
    operation: Manipulation,
    uv: [f64; 2],
    edges: Option<[i8; 2]>,
) -> bool {
    if active(world) || crate::native::shortcuts::keyboard_owned(world) {
        return false;
    }
    let Some(workspace) = world.get::<Workspaces>(root).map(|spaces| spaces.active) else {
        return false;
    };
    if !focus::eligible(world, root, workspace, target) {
        return false;
    }
    focus::ensure(world, root);
    let Some(bounds) = usable(world, root) else {
        return false;
    };
    let Some(id) = world.get::<Managed>(target).map(|managed| managed.id) else {
        return false;
    };
    let Some(model) = model(world, root, workspace) else {
        return false;
    };
    let Ok(gesture) = Gesture::begin_with_edges(model, id, operation, uv, bounds, edges) else {
        return false;
    };
    let spatial = crate::native::topology::spatial(world, target);
    if !spatial.valid() {
        return false;
    }
    let maximized = model.maximized(id);
    let basis = model.clone();
    let saved = focus::suspend(world, root);
    let session = Session {
        root,
        target,
        workspace,
        bounds,
        gesture,
        operation,
        saved,
        output: world
            .get::<HostViewport>(root)
            .map(|viewport| viewport.0.clone()),
        parent: world.get::<TransientParent>(target).copied(),
        delta: [0.0; 2],
        slot: None,
        previews: Vec::new(),
        outline: None,
        proposed: basis.clone(),
        basis,
        spatial,
        displayed: *world.get::<CanvasItem>(target).unwrap(),
        uv,
        scale: world
            .get::<crate::native::area_effects::AreaScale>(target)
            .map_or(1.0, |scale| f64::from(scale.0)),
        maximized,
    };
    world.init_resource::<State>();
    world.resource_mut::<State>().0 = Some(session);
    true
}

fn model(world: &World, root: Entity, workspace: u64) -> Option<&Workspace> {
    let spaces = world.get::<Workspaces>(root)?;
    (spaces.active == workspace).then_some(())?;
    Some(
        &spaces
            .entries
            .iter()
            .find(|space| space.id == workspace)?
            .desktop,
    )
}

fn valid(world: &World, session: &Session) -> bool {
    !crate::native::shortcuts::keyboard_owned(world)
        && focus::eligible(world, session.root, session.workspace, session.target)
        && world
            .get::<Managed>(session.target)
            .is_some_and(|managed| managed.id == session.gesture.id())
        && world.get::<TransientParent>(session.target).copied() == session.parent
        && crate::native::topology::spatial(world, session.target) == session.spatial
        && world
            .get::<HostViewport>(session.root)
            .map(|viewport| &viewport.0)
            == session.output.as_ref()
        && usable(world, session.root) == Some(session.bounds)
        && model(world, session.root, session.workspace).is_some()
}

fn entities(world: &World, session: &Session) -> BTreeMap<PlacementId, Entity> {
    world
        .get::<Children>(session.root)
        .map(|children| {
            children
                .iter()
                .filter_map(|entity| {
                    (world.get::<WorkspaceMember>(entity)?.0 == session.workspace).then_some(())?;
                    Some((world.get::<Managed>(entity)?.id, entity))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn proposal(world: &World, session: &Session) -> Option<Proposal> {
    if !valid(world, session) {
        return None;
    }
    let current = model(world, session.root, session.workspace)?;
    let (mut next, mut geometry) = session
        .gesture
        .propose(current, session.delta, session.slot)
        .ok()?;
    let mut spatial = session.spatial;
    if !session.gesture.tiled() && next != *current {
        let original = session.gesture.original();
        let center = |geometry: Geometry| {
            bevy::math::DVec2::from_array(std::array::from_fn(|axis| {
                geometry.origin[axis] + geometry.size[axis] * 0.5
            }))
        };
        let base = center(original);
        let local = center(geometry) - base;
        let resize = if session.operation == Manipulation::Resize {
            spatial.rotation()
                * bevy::math::DVec3::new(local.x * session.scale, 0.0, local.y * session.scale)
                - bevy::math::DVec3::new(local.x, 0.0, local.y)
        } else {
            bevy::math::DVec3::ZERO
        };
        let restore = if session.maximized {
            let offset = |size: [f64; 2]| {
                spatial.rotation()
                    * bevy::math::DVec3::new(
                        (session.uv[0] - 0.5) * size[0] * session.scale,
                        0.0,
                        (session.uv[1] - 0.5) * size[1] * session.scale,
                    )
            };
            spatial.position(session.displayed.position)
                + offset(session.displayed.size.as_dvec2().to_array())
                - spatial.position(base)
                - offset(original.size)
        } else {
            bevy::math::DVec3::ZERO
        };
        let shift = resize + restore;
        geometry.origin[0] += shift.x;
        geometry.origin[1] += shift.z;
        spatial.elevation += shift.y;
        if !spatial.valid() {
            return None;
        }
        next.set_free_geometry(session.gesture.id(), geometry)
            .ok()?;
    }
    let entities = entities(world, session);
    let offset = if next != *current {
        session.parent.and_then(|parent| {
            let parent_id = world.get::<Managed>(parent.0)?.id;
            let mut baseline = current.clone();
            let mut solution = baseline.solve(session.bounds, session.bounds).ok()?;
            transients::place(
                world,
                &entities,
                &mut baseline,
                &mut solution,
                session.bounds,
            );
            let parent_bounds = solution
                .items
                .iter()
                .find(|item| item.id == parent_id)?
                .rectangle;
            Some(transients::Placement {
                parent: parent.0,
                offset: bevy::math::DVec2::from_array(std::array::from_fn(|axis| {
                    geometry.origin[axis] + geometry.size[axis] * 0.5
                        - parent_bounds.origin()[axis]
                        - parent_bounds.size()[axis] * 0.5
                })),
            })
        })
    } else {
        None
    };
    let mut solution = next.solve(session.bounds, session.bounds).ok()?;
    transients::place_with(
        world,
        &entities,
        &mut next,
        &mut solution,
        session.bounds,
        offset.map(|offset| (session.target, offset)),
    );
    let offset = offset.and_then(|mut placement| {
        let parent_id = world.get::<Managed>(placement.parent)?.id;
        let parent = solution
            .items
            .iter()
            .find(|item| item.id == parent_id)?
            .rectangle;
        let child = solution
            .items
            .iter()
            .find(|item| item.id == session.gesture.id())?
            .rectangle;
        placement.offset = bevy::math::DVec2::from_array(std::array::from_fn(|axis| {
            child.origin()[axis] + child.size()[axis] * 0.5
                - parent.origin()[axis]
                - parent.size()[axis] * 0.5
        }));
        Some(placement)
    });
    Some(Proposal {
        model: next,
        solution,
        placement: offset,
        spatial,
    })
}

fn canvas(item: &crate::layout::desktop::SolvedItem, solution: &Solution) -> CanvasItem {
    CanvasItem {
        position: bevy::math::DVec2::from_array(std::array::from_fn(|axis| {
            item.rectangle.origin()[axis]
                + solution.scroll[axis]
                + item.rectangle.size()[axis] * 0.5
        })),
        size: bevy::math::DVec2::from_array(item.rectangle.size()).as_vec2(),
    }
}

fn changed(world: &World, session: &Session, proposed: &Workspace, solution: &Solution) -> BTreeSet<PlacementId> {
    let Some(mut baseline) = model(world, session.root, session.workspace).cloned() else {
        return BTreeSet::new();
    };
    let Ok(mut previous) = baseline.solve(session.bounds, session.bounds) else {
        return BTreeSet::new();
    };
    transients::place(
        world,
        &entities(world, session),
        &mut baseline,
        &mut previous,
        session.bounds,
    );
    solution
        .items
        .iter()
        .filter(|item| {
            previous
                .items
                .iter()
                .find(|other| other.id == item.id)
                .is_none_or(|other| other.rectangle != item.rectangle)
                || baseline.compact_viewport(item.id, session.bounds, &previous) != proposed.compact_viewport(item.id, session.bounds, solution)
        })
        .map(|item| item.id)
        .collect()
}

fn clear_outline(world: &mut World, session: &mut Session) {
    if let Some(entity) = session.outline.take()
        && let Ok(entity) = world.get_entity_mut(entity)
    {
        entity.despawn();
    }
}

fn clear(world: &mut World, session: &mut Session) {
    if let Ok(mut target) = world.get_entity_mut(session.target) {
        target.remove::<SpatialPreview>();
    }
    for entity in session.previews.drain(..) {
        if let Ok(mut entity) = world.get_entity_mut(entity) {
            entity.remove::<ItemPreview>();
            entity.remove::<CompactViewportPreview>();
        }
    }
    clear_outline(world, session);
}

fn restore(world: &mut World, session: &mut Session) {
    clear(world, session);
    let modal = crate::native::shortcuts::keyboard_owned(world);
    let input = modal
        .then(|| {
            world
                .get_resource::<bevy::input_focus::InputFocus>()
                .and_then(|focus| focus.get())
        })
        .flatten();
    if let Some(saved) = session.saved.take() {
        focus::resume(world, session.root, saved);
    }
    if let Some(input) = input.filter(|input| world.get_entity(*input).is_ok())
        && let Some(mut focus) = world.get_resource_mut::<bevy::input_focus::InputFocus>()
    {
        focus.set(input, bevy::input_focus::FocusCause::Navigated);
    }
}

pub fn cancel(world: &mut World) {
    if let Some(mut session) = world
        .get_resource_mut::<State>()
        .and_then(|mut state| state.0.take())
    {
        restore(world, &mut session);
    }
}

pub fn propose(world: &mut World, delta: [f64; 2], slot: Option<DropSlot>) -> bool {
    let Some(mut session) = world
        .get_resource_mut::<State>()
        .and_then(|mut state| state.0.take())
    else {
        return false;
    };
    session.delta = delta;
    session.slot = slot;
    let Some(Proposal {
        model: proposed,
        solution,
        spatial,
        ..
    }) = proposal(world, &session)
    else {
        restore(world, &mut session);
        return false;
    };
    session.proposed = proposed;
    session.basis = model(world, session.root, session.workspace)
        .unwrap()
        .clone();
    if spatial != session.spatial {
        if world
            .get::<SpatialPreview>(session.target)
            .is_none_or(|preview| preview.0 != spatial)
        {
            world
                .entity_mut(session.target)
                .insert(SpatialPreview(spatial));
        }
    } else {
        world.entity_mut(session.target).remove::<SpatialPreview>();
    }
    let entities = entities(world, &session);
    let mut previews = Vec::new();
    if session.operation == Manipulation::Move && session.gesture.tiled() {
        if delta != [0.0; 2]
            && slot.is_some_and(|slot| slot.anchor != session.gesture.id())
            && let Some(item) = solution
                .items
                .iter()
                .find(|item| item.id == session.gesture.id())
        {
            let desired = canvas(item, &solution);
            let outline = if let Some(outline) = session
                .outline
                .filter(|entity| world.get_entity(*entity).is_ok())
            {
                if world.get::<CanvasItem>(outline).is_some_and(|item| {
                    item.position != desired.position || item.size != desired.size
                }) {
                    world.entity_mut(outline).insert(desired);
                }
                outline
            } else {
                world
                    .spawn((
                        desired,
                        WorkspaceMember(session.workspace),
                        ChildOf(session.root),
                        Outline,
                        crate::native::canvas_item::Ephemeral,
                        crate::native::inspection::InspectionExcluded,
                        Pickable::IGNORE,
                        Node {
                            border: UiRect::all(px(2)),
                            ..default()
                        },
                        ZIndex(i32::MAX),
                        crate::native::token_style::border(crate::native::tokens::Token::Accent),
                    ))
                    .id()
            };
            let spatial = crate::native::topology::spatial(world, session.target);
            if world.get::<crate::native::topology::Spatial>(outline) != Some(&spatial) {
                world.entity_mut(outline).insert(spatial);
            }
            session.outline = Some(outline);
        } else {
            clear_outline(world, &mut session);
        }
    } else {
        clear_outline(world, &mut session);
        let changed = changed(world, &session, &session.proposed, &solution);
        for item in &solution.items {
            if !changed.contains(&item.id) {
                continue;
            }
            let Some(entity) = entities.get(&item.id).copied() else {
                continue;
            };
            let desired = canvas(item, &solution);
            let clip = CompactViewportPreview(session.proposed.compact_viewport(item.id, session.bounds, &solution).map(|size| bevy::math::DVec2::from_array(size).as_vec2()));
            if world.get::<CompactViewportPreview>(entity) != Some(&clip) { world.entity_mut(entity).insert(clip); }
            if world.get::<ItemPreview>(entity).is_none_or(|preview| {
                preview.0.position != desired.position || preview.0.size != desired.size
            }) {
                world.entity_mut(entity).insert(ItemPreview(desired));
            }
            previews.push(entity);
        }
    }
    for entity in session
        .previews
        .drain(..)
        .filter(|entity| !previews.contains(entity))
    {
        if let Ok(mut entity) = world.get_entity_mut(entity) {
            entity.remove::<ItemPreview>();
            entity.remove::<CompactViewportPreview>();
        }
    }
    session.previews = previews;
    world.resource_mut::<State>().0 = Some(session);
    true
}

pub fn commit(world: &mut World) -> bool {
    let Some(mut session) = world
        .get_resource_mut::<State>()
        .and_then(|mut state| state.0.take())
    else {
        return false;
    };
    let Some(Proposal {
        model: next,
        solution,
        placement: offset,
        spatial,
    }) = proposal(world, &session)
    else {
        restore(world, &mut session);
        return false;
    };
    if model(world, session.root, session.workspace) == Some(&next) {
        restore(world, &mut session);
        return true;
    }
    let entities = entities(world, &session);
    let changed = changed(world, &session, &next, &solution);
    if spatial != session.spatial {
        world.entity_mut(session.target).insert(spatial);
    }
    if session.maximized
        && let Some(mut runtime) = world.get_resource_mut::<Runtime>()
    {
        runtime
            .fullscreen_views
            .remove(&(session.root, session.workspace));
    }
    if let Some(offset) = offset {
        world.entity_mut(session.target).insert(offset);
    }
    for item in next.items() {
        if let Some(entity) = entities.get(&item.id)
            && let Some(mut layout) = world.get_mut::<LayoutBox>(*entity)
            && layout.rules != item.rules
        {
            layout.rules = item.rules;
        }
    }
    world
        .get_mut::<Workspaces>(session.root)
        .unwrap()
        .entries
        .iter_mut()
        .find(|space| space.id == session.workspace)
        .unwrap()
        .desktop = next;
    for item in &solution.items {
        if changed.contains(&item.id)
            && let Some(entity) = entities.get(&item.id)
        {
            *world.get_mut::<CanvasItem>(*entity).unwrap() = canvas(item, &solution);
            if !session.gesture.tiled() {
                crate::native::topology::groups::rebind(world, *entity);
            }
        }
    }
    clear(world, &mut session);
    focus::select(
        world,
        session.root,
        session.workspace,
        session.target,
        false,
    );
    super::super::records::remember(world, session.target);
    true
}

pub(in crate::native::layout) fn update(world: &mut World) {
    let pending = world
        .get_resource::<State>()
        .and_then(|state| state.0.as_ref())
        .filter(|session| {
            !valid(world, session)
                || model(world, session.root, session.workspace) != Some(&session.basis)
        })
        .map(|session| (session.delta, session.slot));
    if let Some((delta, slot)) = pending {
        propose(world, delta, slot);
    }
}

#[cfg(test)]
mod tests;
