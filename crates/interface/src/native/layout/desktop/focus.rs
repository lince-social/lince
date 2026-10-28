use super::*;
use crate::host::Direction;
use bevy::math::{DVec2, DVec3};

mod memory;
pub(crate) use memory::{Saved, remember, snapshot};
mod policy;
pub mod settings;
pub use policy::{CursorWarpSupport, can_hover, cancel_warp, hover, hover_eligible, matches_pointer, pending_warp, pointer_epoch, pointer_generation, policy, request_warp};
pub(super) use policy::resume_pointer;
pub(in crate::native::layout) use policy::moved;

#[derive(Resource, Default)]
struct FocusState(BTreeMap<(Entity, u64), State>);

#[derive(Clone, Default)]
struct State {
    last: Option<PlacementId>,
    active: Option<PlacementId>,
    input: Option<Entity>,
    follow: Option<Following>,
    pointer_suspended: bool,
}

#[derive(Clone)]
struct Following {
    target: PlacementId,
    stamp: Option<Stamp>,
    frame: Option<([f64; 6], [f64; 3])>,
    offset: DVec3,
}

#[derive(Clone, Copy, PartialEq)]
struct Stamp {
    center: DVec2,
    zoom: f64,
    spatial: Option<crate::native::topology::view::View>,
}

pub(super) struct ReturnState {
    workspace: u64,
    canvas: CanvasView,
    spatial: Option<crate::native::topology::view::View>,
    state: State,
    focused: Option<PlacementId>,
    input: Option<Entity>,
}

pub(super) fn suspend(world: &mut World, root: Entity) -> Option<ReturnState> {
    let workspace = active(world, root)?;
    let canvas = *world.get::<CanvasView>(root)?;
    let spatial = world
        .get::<crate::native::topology::view::View>(root)
        .copied();
    let focused = focused(world, root, workspace);
    let input = world
        .get_resource::<bevy::input_focus::InputFocus>()
        .and_then(|focus| focus.get());
    world.init_resource::<FocusState>();
    let mut states = world.resource_mut::<FocusState>();
    let state = states.0.entry((root, workspace)).or_default();
    let saved = state.clone();
    state.follow = None;
    Some(ReturnState {
        workspace,
        canvas,
        spatial,
        state: saved,
        focused,
        input,
    })
}

pub(super) fn resume(world: &mut World, root: Entity, mut saved: ReturnState) {
    if !world.get::<Workspaces>(root).is_some_and(|spaces| {
        spaces
            .entries
            .iter()
            .any(|space| space.id == saved.workspace)
    }) {
        return;
    }
    let target = saved
        .focused
        .and_then(|id| entity(world, root, saved.workspace, id));
    if target.is_none() {
        saved.state.active = None;
        saved.state.follow = None;
    }
    if let Some(follow) = &mut saved.state.follow {
        follow.stamp = None;
        follow.frame = None;
    }
    world.init_resource::<FocusState>();
    world
        .resource_mut::<FocusState>()
        .0
        .insert((root, saved.workspace), saved.state);
    if active(world, root) == Some(saved.workspace) {
        *world.get_mut::<CanvasView>(root).unwrap() = saved.canvas;
        if let Some(spatial) = saved.spatial {
            world.entity_mut(root).insert(spatial);
        }
        let bounds = usable(world, root);
        let mut spaces = world.get_mut::<Workspaces>(root).unwrap();
        let model = &mut spaces
            .entries
            .iter_mut()
            .find(|space| space.id == saved.workspace)
            .unwrap()
            .desktop;
        if let Some(id) = saved.focused.filter(|_| target.is_some()) {
            if let Some(bounds) = bounds {
                let _ = model.focus(id, bounds);
            }
        } else {
            model.clear_focus();
        }
        let input = saved
            .input
            .filter(|input| {
                return_input(
                    world,
                    root,
                    saved.workspace,
                    *input,
                    target,
                    saved.focused.is_some(),
                )
            })
            .or(target);
        if let Some(mut focus) = world.get_resource_mut::<bevy::input_focus::InputFocus>() {
            if let Some(input) = input {
                focus.set(input, bevy::input_focus::FocusCause::Navigated);
            } else {
                focus.clear();
            }
        }
    }
}

pub(super) fn return_input(
    world: &World,
    root: Entity,
    workspace: u64,
    mut input: Entity,
    target: Option<Entity>,
    had_focus: bool,
) -> bool {
    for _ in 0..crate::native::layout::MAX_DEPTH {
        if world.get_entity(input).is_err()
            || world
                .get::<bevy::ecs::entity_disabling::Disabled>(input)
                .is_some()
        {
            return false;
        }
        if input == root {
            return true;
        }
        if world.get::<CanvasItem>(input).is_some() {
            return target == Some(input) || !had_focus && eligible(world, root, workspace, input);
        }
        if world
            .get::<Node>(input)
            .is_some_and(|node| node.display == Display::None)
        {
            return false;
        }
        let Some(parent) = world.get::<ChildOf>(input) else {
            return false;
        };
        input = parent.parent();
    }
    false
}

fn active(world: &World, root: Entity) -> Option<u64> {
    Some(world.get::<Workspaces>(root)?.active)
}

fn focused(world: &World, root: Entity, workspace: u64) -> Option<PlacementId> {
    world
        .get::<Workspaces>(root)?
        .entries
        .iter()
        .find(|space| space.id == workspace)?
        .desktop
        .focused()
}

pub(super) fn eligible(world: &World, root: Entity, workspace: u64, entity: Entity) -> bool {
    candidate(world, entity)
        && !crate::native::inspection::excluded(world, entity)
        && world
            .get::<bevy::ecs::entity_disabling::Disabled>(entity)
            .is_none()
        && world
            .get::<crate::native::castle::Pending>(entity)
            .is_none()
        && world
            .get::<WorkspaceMember>(entity)
            .is_some_and(|member| member.0 == workspace)
        && world
            .get::<ChildOf>(entity)
            .is_some_and(|parent| parent.parent() == root)
        && world.get::<CanvasItem>(entity).is_some_and(|item| {
            item.position.is_finite() && item.size.is_finite() && item.size.min_element() > 0.0
        })
        && world.get::<Workspaces>(root).is_some_and(|spaces| {
            spaces
                .entries
                .iter()
                .find(|space| space.id == workspace)
                .is_some_and(|space| !transients::minimized(world, entity, &space.desktop))
        })
}

fn candidates(world: &World, root: Entity, workspace: u64) -> Vec<Entity> {
    world
        .get::<Children>(root)
        .map_or_else(Vec::new, |children| {
            children
                .iter()
                .filter(|entity| eligible(world, root, workspace, *entity))
                .collect()
        })
}

fn entity(world: &World, root: Entity, workspace: u64, id: PlacementId) -> Option<Entity> {
    candidates(world, root, workspace)
        .into_iter()
        .find(|entity| {
            world
                .get::<Managed>(*entity)
                .map(|managed| managed.id)
                .or_else(|| {
                    world
                        .get::<LayoutBox>(*entity)
                        .map(|layout| PlacementId(layout.id))
                })
                == Some(id)
        })
}

pub(super) fn contains_input(world: &World, mut input: Entity, target: Entity) -> bool {
    for _ in 0..32 {
        if world.get_entity(input).is_err() {
            return false;
        }
        if input == target {
            return true;
        }
        let Some(parent) = world.get::<ChildOf>(input) else {
            return false;
        };
        input = parent.parent();
    }
    false
}

pub(crate) fn remember_input(world: &mut World, root: Entity) {
    let Some(workspace) = active(world, root) else {
        return;
    };
    if world
        .get_resource::<crate::native::shortcuts::KeyboardModalOwner>()
        .and_then(|owner| owner.0)
        .is_some_and(|owner| {
            world
                .get_resource::<bevy::input_focus::InputFocus>()
                .and_then(|focus| focus.get())
                .is_some_and(|input| contains_input(world, input, owner))
        })
    {
        return;
    }
    let target = focused(world, root, workspace).and_then(|id| entity(world, root, workspace, id));
    let input = world
        .get_resource::<bevy::input_focus::InputFocus>()
        .and_then(|focus| focus.get())
        .filter(|input| target.is_some_and(|target| contains_input(world, *input, target)));
    world.init_resource::<FocusState>();
    world
        .resource_mut::<FocusState>()
        .0
        .entry((root, workspace))
        .or_default()
        .input = input;
}

pub(crate) fn restore_input(world: &mut World, root: Entity) {
    memory::restore_missing(world);
    let Some(workspace) = active(world, root) else {
        return;
    };
    let Some(target) =
        focused(world, root, workspace).and_then(|id| entity(world, root, workspace, id))
    else {
        return;
    };
    let input = world
        .get_resource::<FocusState>()
        .and_then(|states| states.0.get(&(root, workspace)))
        .and_then(|state| state.input)
        .filter(|input| contains_input(world, *input, target))
        .unwrap_or(target);
    if let Some(mut focus) = world.get_resource_mut::<bevy::input_focus::InputFocus>() {
        focus.set(input, bevy::input_focus::FocusCause::Navigated);
    }
}

pub(super) fn ensure(world: &mut World, root: Entity) {
    let Some(workspace) = active(world, root) else {
        return;
    };
    world.init_resource::<Runtime>();
    let empty = world
        .get::<Workspaces>(root)
        .unwrap()
        .entries
        .iter()
        .find(|space| space.id == workspace)
        .is_some_and(|space| space.desktop.items().is_empty());
    let unmanaged = candidates(world, root, workspace)
        .iter()
        .any(|entity| world.get::<Managed>(*entity).is_none());
    if !empty && !unmanaged {
        return;
    }
    world
        .resource_mut::<Runtime>()
        .enabled
        .insert((root, workspace));
    super::resolve(world);
    if empty {
        if let Some(space) = world
            .get_mut::<Workspaces>(root)
            .unwrap()
            .entries
            .iter_mut()
            .find(|space| space.id == workspace)
        {
            space.desktop.clear_focus();
        }
    }
}

fn pick(world: &World, root: Entity, workspace: u64) -> Option<Entity> {
    let choices = candidates(world, root, workspace);
    if let Some(selected) = crate::native::canvas_selection::selected(world, root)
        .into_iter()
        .find(|entity| choices.contains(entity))
    {
        return Some(selected);
    }
    if let Some(id) = world
        .get_resource::<FocusState>()
        .and_then(|state| state.0.get(&(root, workspace)))
        .and_then(|state| state.last)
        .or_else(|| focused(world, root, workspace))
        && let Some(entity) = entity(world, root, workspace, id)
    {
        return Some(entity);
    }
    let center = crate::native::output_views::canvas(world, root, workspace).center;
    choices.into_iter().min_by(|first, second| {
        let distance = |entity| {
            world
                .get::<CanvasItem>(entity)
                .unwrap()
                .position
                .distance_squared(center)
        };
        distance(*first)
            .total_cmp(&distance(*second))
            .then_with(|| {
                world
                    .get::<Managed>(*first)
                    .map(|managed| managed.id)
                    .cmp(&world.get::<Managed>(*second).map(|managed| managed.id))
            })
    })
}

pub(super) fn status(world: &World, root: Entity) -> (bool, bool) {
    let Some(workspace) = active(world, root) else {
        return (false, false);
    };
    let id =
        focused(world, root, workspace).filter(|id| entity(world, root, workspace, *id).is_some());
    let free = world
        .get::<Workspaces>(root)
        .and_then(|spaces| spaces.entries.iter().find(|space| space.id == workspace))
        .is_some_and(|space| {
            space.desktop.mode() == WorkspaceLayout::Free
                && space.desktop.fullscreen().is_none()
                && id.is_none_or(|id| !space.desktop.maximized(id))
        });
    let selected = id.is_some()
        && (!free
            || world
                .get_resource::<FocusState>()
                .and_then(|states| states.0.get(&(root, workspace)))
                .is_some_and(|state| state.active == id));
    (
        selected,
        selected || !candidates(world, root, workspace).is_empty(),
    )
}

pub fn following(world: &World, root: Entity) -> bool {
    active(world, root).is_some_and(|workspace| {
        world
            .get_resource::<FocusState>()
            .and_then(|state| state.0.get(&(root, workspace)))
            .is_some_and(|state| state.follow.is_some())
    })
}

pub(crate) fn release(world: &mut World, root: Entity) {
    cancel_warp(world);
    let Some(workspace) = active(world, root) else {
        return;
    };
    if let Some(mut state) = world.get_resource_mut::<FocusState>()
        && let Some(state) = state.0.get_mut(&(root, workspace))
    {
        state.follow = None;
    }
}

pub(super) fn clear_mode(world: &mut World, root: Entity) {
    release(world, root);
    if let Some(workspace) = active(world, root)
        && let Some(mut states) = world.get_resource_mut::<FocusState>()
        && let Some(state) = states.0.get_mut(&(root, workspace))
    {
        state.active = None;
    }
}

pub(crate) fn observe_camera(world: &mut World, root: Entity) {
    let Some(workspace) = active(world, root) else {
        return;
    };
    let current = stamp(world, root, workspace);
    if let Some(mut states) = world.get_resource_mut::<FocusState>()
        && let Some(state) = states.0.get_mut(&(root, workspace))
        && let Some(last) = state.follow.as_ref().and_then(|follow| follow.stamp)
        && last.spatial.is_some() == current.spatial.is_some()
        && last != current
    {
        state.follow = None;
    }
}

pub(super) fn remember_selection(world: &mut World, root: Entity) {
    let Some(workspace) = active(world, root) else {
        return;
    };
    let selected = crate::native::canvas_selection::selected(world, root)
        .into_iter()
        .find(|entity| eligible(world, root, workspace, *entity));
    let Some(id) = selected
        .and_then(|entity| world.get::<LayoutBox>(entity))
        .map(|layout| PlacementId(layout.id))
    else {
        return;
    };
    world.init_resource::<FocusState>();
    world
        .resource_mut::<FocusState>()
        .0
        .entry((root, workspace))
        .or_default()
        .last = Some(id);
}

pub(super) fn select(
    world: &mut World,
    root: Entity,
    workspace: u64,
    target: Entity,
    follow: bool,
) {
    select_inner(world, root, workspace, target, follow, true);
}

fn select_inner(world: &mut World, root: Entity, workspace: u64, target: Entity, follow: bool, warp: bool) {
    focus_item(world, target);
    let Some(id) = world.get::<Managed>(target).map(|managed| managed.id) else {
        return;
    };
    let free = world
        .get::<Workspaces>(root)
        .unwrap()
        .entries
        .iter()
        .find(|space| space.id == workspace)
        .is_some_and(|space| space.desktop.mode() == WorkspaceLayout::Free);
    world.init_resource::<FocusState>();
    {
        let mut states = world.resource_mut::<FocusState>();
        let state = states.0.entry((root, workspace)).or_default();
        state.last = Some(id);
        state.pointer_suspended = false;
        state.active = follow.then_some(id);
        if follow && free {
            state.follow = Some(Following {
                target: id,
                stamp: None,
                frame: None,
                offset: DVec3::ZERO,
            });
        } else {
            state.follow = None;
        }
    }
    keyboard_focus(world, target);
    if warp {
        request_warp(world, root, target);
    }
    if !free && let Some(mut runtime) = world.get_resource_mut::<Runtime>() {
        runtime.views.remove(&(root, workspace));
    }
}

pub(super) fn keyboard_focus(world: &mut World, target: Entity) {
    if crate::native::shortcuts::keyboard_owned(world) {
        return;
    }
    world.init_resource::<bevy::input_focus::InputFocus>();
    world
        .resource_mut::<bevy::input_focus::InputFocus>()
        .set(target, bevy::input_focus::FocusCause::Navigated);
}

pub(super) fn apply(world: &mut World, root: Entity, action: DesktopAction) -> bool {
    if !matches!(
        action,
        DesktopAction::ToggleFocus
            | DesktopAction::Unfocus
            | DesktopAction::Focus(_)
            | DesktopAction::Move(_)
    ) {
        return false;
    }
    ensure(world, root);
    let Some(workspace) = active(world, root) else {
        return true;
    };
    let current =
        focused(world, root, workspace).filter(|id| entity(world, root, workspace, *id).is_some());
    if action == DesktopAction::Unfocus
        || action == DesktopAction::ToggleFocus && status(world, root).0
    {
        cancel_warp(world);
        world.init_resource::<FocusState>();
        {
            let mut states = world.resource_mut::<FocusState>();
            let state = states.0.entry((root, workspace)).or_default();
            state.last = current.or(state.last);
            state.active = None;
            state.follow = None;
            state.pointer_suspended = true;
        }
        world
            .get_mut::<Workspaces>(root)
            .unwrap()
            .entries
            .iter_mut()
            .find(|space| space.id == workspace)
            .unwrap()
            .desktop
            .clear_focus();
        if let Some(mut focus) = world.get_resource_mut::<bevy::input_focus::InputFocus>() {
            focus.clear();
        }
    } else if action == DesktopAction::ToggleFocus {
        if let Some(target) = pick(world, root, workspace) {
            select(world, root, workspace, target, true);
        }
    } else {
        let mode = world
            .get::<Workspaces>(root)
            .unwrap()
            .entries
            .iter()
            .find(|space| space.id == workspace)
            .unwrap()
            .desktop
            .mode();
        let floating = current
            .and_then(|id| entity(world, root, workspace, id))
            .and_then(|entity| world.get::<Managed>(entity))
            .is_some_and(|managed| managed.floating);
        match action {
            DesktopAction::Focus(direction) if mode == WorkspaceLayout::Free => {
                let origin = current
                    .and_then(|id| entity(world, root, workspace, id))
                    .and_then(|entity| world.get::<CanvasItem>(entity))
                    .map_or_else(
                        || crate::native::output_views::canvas(world, root, workspace).center,
                        |item| item.position,
                    );
                let axis = usize::from(matches!(direction, Direction::Up | Direction::Down));
                let sign = if matches!(direction, Direction::Left | Direction::Up) {
                    -1.0
                } else {
                    1.0
                };
                let target = candidates(world, root, workspace)
                    .into_iter()
                    .filter_map(|target| {
                        let delta = world.get::<CanvasItem>(target)?.position - origin;
                        let primary = delta[axis] * sign;
                        if primary <= 0.001 {
                            return None;
                        }
                        let cross = delta[1 - axis].abs();
                        Some((target, primary.hypot(cross) * (1.0 + cross / primary)))
                    })
                    .min_by(|first, second| {
                        first.1.total_cmp(&second.1).then_with(|| {
                            world
                                .get::<Managed>(first.0)
                                .map(|managed| managed.id)
                                .cmp(&world.get::<Managed>(second.0).map(|managed| managed.id))
                        })
                    })
                    .map(|(target, _)| target);
                if let Some(target) = target {
                    select(world, root, workspace, target, true);
                }
            }
            DesktopAction::Move(direction) if mode == WorkspaceLayout::Free || floating => {
                let Some(target) = current.and_then(|id| entity(world, root, workspace, id)) else {
                    return true;
                };
                let step = f64::from(
                    world
                        .get_resource::<crate::native::shortcuts::configuration::LayoutSettings>()
                        .cloned()
                        .unwrap_or_default()
                        .move_step,
                );
                let delta = match direction {
                    Direction::Left => DVec2::NEG_X,
                    Direction::Right => DVec2::X,
                    Direction::Up => DVec2::NEG_Y,
                    Direction::Down => DVec2::Y,
                } * step;
                let mut item = *world.get::<CanvasItem>(target).unwrap();
                item.position = (item.position + delta)
                    .clamp(DVec2::splat(-999_000_000.0), DVec2::splat(999_000_000.0));
                *world.get_mut::<CanvasItem>(target).unwrap() = item;
            }
            _ => {
                return false;
            }
        }
    }
    if let Some(wake) = world.get_resource::<crate::host::WakeSignal>() {
        wake.ring();
    }
    true
}

fn stamp(world: &World, root: Entity, workspace: u64) -> Stamp {
    let canvas = crate::native::output_views::canvas(world, root, workspace);
    let spatial = if active(world, root) == Some(workspace) {
        crate::native::topology::view::spatial(world, root)
            .then(|| world.get::<crate::native::topology::view::View>(root).copied()).flatten()
    } else {
        world.get::<Workspaces>(root).and_then(|spaces| spaces.entries.iter().find(|space| space.id == workspace))
            .filter(|space| space.topology.spatial && space.desktop.fullscreen().is_none())
            .map(|space| space.topology)
    };
    Stamp {
        center: canvas.center,
        zoom: canvas.zoom,
        spatial,
    }
}

fn viewport_for(world: &World, root: Entity, workspace: u64) -> Option<(Rectangle, Rectangle)> {
    if let Some(views) = world
        .get_resource::<crate::native::output_views::OutputViews>()
        .filter(|views| views.applies_to(root))
    {
        let output = views
            .outputs
            .iter()
            .find(|output| output.workspace == workspace)?;
        return Some((output.bounds, output.usable?));
    }
    (active(world, root) == Some(workspace))
        .then(|| viewport(world, root).zip(usable(world, root)))
        .flatten()
}

pub(crate) fn resolve(world: &mut World) {
    memory::restore_missing(world);
    let Some(mut states) = world.remove_resource::<FocusState>() else {
        return;
    };
    states.0.retain(|(root, workspace), _| {
        world
            .get::<Workspaces>(*root)
            .is_some_and(|spaces| spaces.entries.iter().any(|space| space.id == *workspace))
    });
    for ((root, workspace), state) in &mut states.0 {
        let (root, workspace) = (*root, *workspace);
        if let Some(last) = state.last
            && entity(world, root, workspace, last).is_none()
        {
            state.last = None;
        }
        if state.active.is_some_and(|id| {
            focused(world, root, workspace) != Some(id)
                || entity(world, root, workspace, id).is_none()
        }) {
            state.active = None;
            state.follow = None;
        }
        let Some(mut follow) = state.follow.take() else {
            continue;
        };
        let Some((bounds, usable)) = viewport_for(world, root, workspace) else {
            state.follow = Some(follow);
            continue;
        };
        let Some(target) = entity(world, root, workspace, follow.target) else {
            continue;
        };
        let model = &world
            .get::<Workspaces>(root)
            .unwrap()
            .entries
            .iter()
            .find(|space| space.id == workspace)
            .unwrap()
            .desktop;
        if model.mode() != WorkspaceLayout::Free
            || model.focused() != Some(follow.target)
            || model.fullscreen().is_some()
            || model.maximized(follow.target)
        {
            continue;
        }
        let current = stamp(world, root, workspace);
        let same_projection = follow
            .stamp
            .is_some_and(|last| last.spatial.is_some() == current.spatial.is_some());
        if same_projection && follow.stamp != Some(current) {
            continue;
        }
        let item = *world.get::<CanvasItem>(target).unwrap();
        let scale = f64::from(
            world
                .get::<crate::native::area_effects::AreaScale>(target)
                .map_or(1.0, |scale| scale.0),
        );
        let (offset, _) = crate::native::layout::viewport::clip(world, target);
        let position = item.position - offset.as_dvec2();
        let size = item.size.as_dvec2() * scale;
        let placement = crate::native::topology::spatial(world, target);
        let framing = (
            [
                usable.origin()[0] - bounds.origin()[0],
                usable.origin()[1] - bounds.origin()[1],
                usable.size()[0],
                usable.size()[1],
                bounds.size()[0],
                bounds.size()[1],
            ],
            [size.x, size.y, placement.depth(item.size) * scale],
        );
        let reframe = !same_projection || follow.frame != Some(framing);
        let screen_offset = DVec2::from_array(std::array::from_fn(|axis| {
            usable.origin()[axis] - bounds.origin()[axis]
                + (usable.size()[axis] - bounds.size()[axis]) * 0.5
        }));
        let mut canvas = crate::native::output_views::canvas(world, root, workspace);
        if let Some(mut spatial) = current.spatial {
            let anchor = placement.position(item.position)
                - placement.rotation() * DVec3::new(f64::from(offset.x), 0.0, f64::from(offset.y))
                - DVec3::Y
                    * crate::native::topology::holes::depth(world, root, workspace) as f64
                    * 0.2;
            if reframe {
                let half = std::f64::consts::FRAC_PI_8;
                let fov = (half.tan() * usable.size()[1] / bounds.size()[1])
                    .atan()
                    .min((half.tan() * usable.size()[0] / bounds.size()[1]).atan());
                let radius = (size.length_squared() + (placement.depth(item.size) * scale).powi(2))
                    .sqrt()
                    * 0.5;
                let distance = (radius / fov.sin() * 1.08).max(32.0);
                let rotation = bevy::math::DQuat::from_euler(
                    EulerRot::YXZ,
                    f64::from(spatial.yaw),
                    f64::from(spatial.pitch),
                    0.0,
                );
                let pixel = 2.0 * distance * half.tan() / bounds.size()[1];
                follow.offset = rotation
                    * DVec3::new(-screen_offset.x * pixel, screen_offset.y * pixel, distance);
            }
            spatial.position = (anchor + follow.offset).to_array();
            if active(world, root) == Some(workspace) {
                world.get_mut::<crate::native::topology::view::View>(root)
                    .unwrap().set_if_neq(spatial);
            } else if let Some(space) = world.get_mut::<Workspaces>(root)
                .and_then(|spaces| spaces.into_inner().entries.iter_mut().find(|space| space.id == workspace))
            {
                space.topology = spatial;
            }
            canvas.center = position;
        } else {
            if reframe {
                let fit = ((usable.size()[0] - 16.0).max(1.0) / size.x)
                    .min((usable.size()[1] - 16.0).max(1.0) / size.y);
                canvas.set_zoom(canvas.zoom.min(fit));
            }
            canvas.center = position - screen_offset / canvas.zoom;
        }
        if active(world, root) == Some(workspace) {
            world
                .get_mut::<CanvasView>(root)
                .unwrap()
                .set_if_neq(canvas);
        } else if let Some(space) = world
            .get_mut::<Workspaces>(root)
            .unwrap()
            .entries
            .iter_mut()
            .find(|space| space.id == workspace)
        {
            space.center = canvas.center.to_array();
            space.zoom = canvas.zoom;
        }
        follow.stamp = Some(stamp(world, root, workspace));
        follow.frame = Some(framing);
        state.follow = Some(follow);
    }
    world.insert_resource(states);
    let dead = world
        .get_resource::<bevy::input_focus::InputFocus>()
        .and_then(|focus| focus.get())
        .is_some_and(|entity| world.get_entity(entity).is_err());
    if dead {
        world
            .resource_mut::<bevy::input_focus::InputFocus>()
            .clear();
    }
}

#[cfg(test)]
mod tests;
