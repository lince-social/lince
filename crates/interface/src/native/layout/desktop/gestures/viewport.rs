use super::super::{
    CanvasView, DesktopAction, Entity, HostViewport, Rectangle, WorkspaceLayout, Workspaces, focus,
};
use crate::{
    host::input::{GestureContext, GestureDelivery, GestureResult},
    native::{layout::desktop::overview, topology::view::View},
};
use bevy::{math::DVec3, prelude::*};

#[derive(Clone, Copy, PartialEq)]
struct Context {
    bounds: Rectangle,
    usable: Rectangle,
    mode: WorkspaceLayout,
    normalized: bool,
    raster: crate::host::raster::RasterMapping,
    physical_size: [u32; 2],
}

struct Pan {
    saved: focus::ReturnState,
    canvas: CanvasView,
    spatial: Option<View>,
    scroll: [f64; 2],
    world_per_unit: Option<DVec3>,
}

struct Session {
    gesture: GestureContext,
    root: Entity,
    context: Context,
    overview: bool,
    pan: Option<Pan>,
}

#[derive(Resource, Default)]
struct State {
    last_sequence: u64,
    session: Option<Session>,
}

pub fn active(world: &World) -> bool {
    world.get_resource::<State>().is_some_and(|state| state.session.is_some())
}

fn context(world: &World, root: Entity, gesture: GestureContext) -> Option<Context> {
    if root.to_bits() != gesture.owner {
        return None;
    }
    let spaces = world.get::<Workspaces>(root)?;
    if spaces.active != gesture.workspace {
        return None;
    }
    let model = &spaces
        .entries
        .iter()
        .find(|space| space.id == spaces.active)?
        .desktop;
    let viewport = world.get::<HostViewport>(root)?;
    if viewport.0.id != gesture.output
        || viewport.0.workspace != gesture.workspace
        || !viewport.0.valid_raster()
    {
        return None;
    }
    if let Some(views) = world.get_resource::<crate::native::output_views::OutputViews>()
        && (!views.applies_to(root) || views.focused != gesture.output)
    {
        return None;
    }
    Some(Context {
        bounds: viewport.0.bounds,
        usable: viewport.0.usable?,
        mode: model.mode(),
        normalized: model.fullscreen().is_some()
            || model
                .items()
                .iter()
                .any(|item| model.maximized(item.id) && !model.minimized(item.id)),
        raster: viewport.0.raster,
        physical_size: viewport.0.physical_size,
    })
}

fn eligible(world: &World, fingers: u32) -> bool {
    matches!(fingers, 3 | 4)
        && !crate::native::shortcuts::keyboard_capturing(world)
        && !crate::native::layout::desktop::manipulation::active(world)
        && (!crate::native::shortcuts::keyboard_owned(world)
            || fingers == 4 && overview::active(world))
}

fn valid(world: &World, session: &Session) -> bool {
    context(world, session.root, session.gesture) == Some(session.context)
        && overview::active(world) == session.overview
        && eligible(world, session.gesture.fingers)
}

fn scale(world: &World, context: Context, view: View) -> Option<DVec3> {
    let point = bevy::math::DVec2::from_array(context.bounds.origin())
        + bevy::math::DVec2::from_array(context.bounds.size()) * 0.5;
    let first = crate::native::topology::input::ray(world, point.as_vec2())?;
    let next = crate::native::topology::input::ray(world, point.as_vec2() + Vec2::X)?;
    let normal = first.direction.as_vec3();
    let plane = first.origin + normal * view.selection_depth as f32;
    let denominator = next.direction.dot(normal);
    if denominator <= 1e-6 {
        return None;
    }
    let distance = (plane - next.origin).dot(normal) / denominator;
    let delta = (next.origin + next.direction * distance - plane).as_dvec3();
    (distance.is_finite() && distance > 0.0 && delta.is_finite()).then_some(delta)
}

fn prepare(world: &mut World, session: &Session) -> Option<Pan> {
    let canvas = *world.get::<CanvasView>(session.root)?;
    if !canvas.center.is_finite() || !canvas.zoom.is_finite() || canvas.zoom <= 0.0 {
        return None;
    }
    let spatial = world.get::<View>(session.root).copied();
    let world_per_unit = if session.context.mode == WorkspaceLayout::Free
        && spatial.is_some_and(|view| view.spatial)
    {
        Some(scale(world, session.context, spatial?)?)
    } else {
        None
    };
    let scroll = world
        .get::<Workspaces>(session.root)?
        .entries
        .iter()
        .find(|space| space.id == session.gesture.workspace)?
        .desktop
        .scroll();
    Some(Pan {
        saved: focus::suspend(world, session.root)?,
        canvas,
        spatial,
        scroll,
        world_per_unit,
    })
}

fn pan(world: &mut World, session: &mut Session, travel: f64) -> bool {
    if session.gesture.fingers != 3
        || session.context.normalized
        || !travel.is_finite()
        || travel.abs() > 1_000_000.0
    {
        return false;
    }
    if session.pan.is_none() {
        session.pan = prepare(world, session);
    }
    let Some(pan) = session.pan.as_ref() else {
        return false;
    };
    if session.context.mode == WorkspaceLayout::Tiled {
        let mut spaces = world.get_mut::<Workspaces>(session.root).unwrap();
        return spaces
            .entries
            .iter_mut()
            .find(|space| space.id == session.gesture.workspace)
            .unwrap()
            .desktop
            .scroll_to(
                [pan.scroll[0] - travel, pan.scroll[1]],
                session.context.usable,
            )
            .is_ok();
    }
    let mut canvas = pan.canvas;
    if let Some(unit) = pan.world_per_unit {
        let delta = -unit * travel;
        let mut view = pan.spatial.unwrap();
        view.position = (DVec3::from_array(view.position) + delta).to_array();
        if !view.valid() {
            return false;
        }
        world.entity_mut(session.root).insert(view);
        canvas.center += bevy::math::DVec2::new(delta.x, delta.z);
    } else {
        canvas.center.x -= travel / canvas.zoom;
    }
    *world.get_mut::<CanvasView>(session.root).unwrap() = canvas;
    true
}

fn restore(world: &mut World, session: Session) {
    if !valid(world, &session) {
        return;
    }
    if let Some(pan) = session.pan {
        focus::resume(world, session.root, pan.saved);
        if session.context.mode == WorkspaceLayout::Tiled {
            let _ = world
                .get_mut::<Workspaces>(session.root)
                .unwrap()
                .entries
                .iter_mut()
                .find(|space| space.id == session.gesture.workspace)
                .unwrap()
                .desktop
                .scroll_to(pan.scroll, session.context.usable);
        }
    }
}

pub fn cancel(world: &mut World) {
    if let Some(session) = world
        .get_resource_mut::<State>()
        .and_then(|mut state| state.session.take())
    {
        restore(world, session);
    }
}

pub fn deliver(
    world: &mut World,
    root: Entity,
    delivery: GestureDelivery,
) -> Option<DesktopAction> {
    world.init_resource::<State>();
    if let GestureDelivery::Begin(gesture) = delivery {
        if gesture.sequence == 0
            || gesture.sequence <= world.resource::<State>().last_sequence
            || world.resource::<State>().session.is_some()
        {
            return None;
        }
        world.resource_mut::<State>().last_sequence = gesture.sequence;
        if !eligible(world, gesture.fingers) {
            return None;
        }
        let context = context(world, root, gesture)?;
        let opened = overview::active(world);
        world.resource_mut::<State>().session = Some(Session {
            gesture,
            root,
            context,
            overview: opened,
            pan: None,
        });
        return None;
    }
    let mut session = world.resource_mut::<State>().session.take()?;
    if !valid(world, &session) {
        return None;
    }
    let (sequence, output) = match delivery {
        GestureDelivery::Begin(_) => unreachable!(),
        GestureDelivery::Pan(progress) => (progress.sequence, progress.output),
        GestureDelivery::Finish(completion) => (completion.sequence, completion.output),
    };
    if session.root != root
        || session.gesture.sequence != sequence
        || session.gesture.output != output
    {
        world.resource_mut::<State>().session = Some(session);
        return None;
    }
    match delivery {
        GestureDelivery::Pan(progress) => {
            if pan(world, &mut session, progress.horizontal_travel) {
                world.resource_mut::<State>().session = Some(session);
            } else {
                restore(world, session);
            }
        }
        GestureDelivery::Finish(completion) => match completion.result {
            GestureResult::Pan(travel) => {
                if !pan(world, &mut session, travel) {
                    restore(world, session);
                }
            }
            GestureResult::Workspace(offset)
                if session.gesture.fingers == 3
                    && session.pan.is_none()
                    && matches!(offset, -1 | 1) =>
            {
                return Some(if offset > 0 {
                    DesktopAction::NextWorkspace
                } else {
                    DesktopAction::PreviousWorkspace
                });
            }
            GestureResult::Overview(open)
                if session.gesture.fingers == 4 && open != session.overview =>
            {
                return Some(DesktopAction::ToggleOverview);
            }
            _ => restore(world, session),
        },
        GestureDelivery::Begin(_) => unreachable!(),
    }
    None
}

#[cfg(test)]
mod tests;
