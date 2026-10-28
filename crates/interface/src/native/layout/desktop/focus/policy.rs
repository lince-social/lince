use super::*;
use crate::host::input::{CursorWarp, FocusPolicy, FocusTarget};

#[derive(Resource, Default)]
pub struct CursorWarpSupport(pub bool);

#[derive(Resource, Default)]
struct PointerEpoch(u64);

#[derive(Resource, Default)]
struct Warping {
    sequence: u64,
    pending: Option<CursorWarp>,
}

pub fn policy(world: &World) -> FocusPolicy {
    world.get_resource::<crate::native::shortcuts::configuration::LayoutSettings>()
        .map_or_else(FocusPolicy::default, |settings| settings.focus)
}

pub fn pointer_epoch(world: &mut World, epoch: u64) {
    let changed = world.get_resource::<PointerEpoch>().is_none_or(|previous| previous.0 != epoch);
    if changed {
        world.insert_resource(PointerEpoch(epoch));
        if let Some(mut warp) = world.get_resource_mut::<Warping>() {
            warp.pending = None;
        }
    }
}

pub fn matches_pointer(world: &World, epoch: u64) -> bool {
    world.get_resource::<PointerEpoch>().map_or(0, |current| current.0) == epoch
}

pub fn pointer_generation(world: &World) -> u64 {
    world.get_resource::<PointerEpoch>().map_or(0, |current| current.0)
}

pub fn cancel_warp(world: &mut World) {
    if let Some(mut state) = world.get_resource_mut::<Warping>() {
        state.pending = None;
    }
}

pub(in crate::native::layout::desktop) fn resume_pointer(world: &mut World, root: Entity) {
    if let Some(workspace) = active(world, root) {
        world.init_resource::<FocusState>();
        world.resource_mut::<FocusState>().0.entry((root, workspace)).or_default().pointer_suspended = false;
    }
}

pub fn pending_warp(world: &World) -> Option<CursorWarp> {
    if !policy(world).warp || !world.get_resource::<CursorWarpSupport>().is_some_and(|support| support.0) {
        return None;
    }
    let request = world.get_resource::<Warping>()?.pending?;
    let root = Entity::from_bits(request.target.owner);
    if active(world, root) != Some(request.target.workspace) || crate::native::shortcuts::keyboard_owned(world) {
        return None;
    }
    Some(request)
}

pub fn request_warp(world: &mut World, root: Entity, target: Entity) {
    if !policy(world).warp
        || !world.get_resource::<CursorWarpSupport>().is_some_and(|support| support.0)
        || crate::native::shortcuts::keyboard_owned(world)
    {
        return;
    }
    let Some(workspace) = active(world, root) else { return; };
    if !eligible(world, root, workspace, target) { return; }
    let Some(output) = world.get::<HostViewport>(root).map(|view| view.0.id) else { return; };
    let Some(id) = world.get::<LayoutBox>(target).map(|layout| layout.id) else { return; };
    let item = world.get::<ApplicationWindow>(target)
        .map_or(crate::host::ItemId::Native(id), |window| crate::host::ItemId::Application(window.surface));
    let pointer_epoch = world.get_resource::<PointerEpoch>().map_or(0, |epoch| epoch.0);
    world.init_resource::<Warping>();
    let mut state = world.resource_mut::<Warping>();
    let Some(sequence) = state.sequence.checked_add(1) else { return; };
    state.sequence = sequence;
    state.pending = Some(CursorWarp {
        sequence,
        pointer_epoch,
        target: FocusTarget { owner: root.to_bits(), workspace, output, item },
    });
}

pub fn can_hover(world: &World, root: Entity, workspace: u64) -> bool {
    policy(world).pointer_follow
        && !crate::native::shortcuts::keyboard_owned(world)
        && !crate::native::layout::desktop::overview::active(world)
        && !crate::native::layout::desktop::recent::active(world)
        && !crate::native::layout::desktop::manipulation::active(world)
        && !crate::native::layout::desktop::gestures::active(world)
        && !world.get::<crate::native::edit_mode::EditMode>(root).is_some_and(|mode| mode.enabled)
        && !world.get_resource::<ButtonInput<MouseButton>>().is_some_and(|buttons| buttons.get_pressed().next().is_some())
        && !world.get_resource::<ButtonInput<KeyCode>>().is_some_and(|keys| keys.get_pressed().next().is_some())
        && !world.get_resource::<FocusState>().and_then(|states| states.0.get(&(root, workspace))).is_some_and(|state| state.pointer_suspended)
        && !world.get_resource::<bevy::input_focus::InputFocus>().and_then(|focus| focus.get())
            .and_then(|input| world.get::<bevy::text::EditableText>(input)).is_some_and(|input| input.is_composing())
}

pub fn hover_eligible(world: &World, root: Entity, workspace: u64, target: Entity) -> bool {
    can_hover(world, root, workspace) && eligible(world, root, workspace, target)
        && world.get::<TransientParent>(target).is_none()
}

pub fn hover(world: &mut World, root: Entity, workspace: u64, target: Entity) -> bool {
    if !hover_eligible(world, root, workspace, target) || active(world, root) != Some(workspace)
    {
        return false;
    }
    ensure(world, root);
    let Some(id) = world.get::<Managed>(target).map(|managed| managed.id) else { return false; };
    if focused(world, root, workspace) == Some(id) { return false; }
    let follow = status(world, root).0 || world.get::<Workspaces>(root).unwrap().entries.iter()
        .find(|space| space.id == workspace).is_some_and(|space| space.desktop.mode() == WorkspaceLayout::Tiled);
    select_inner(world, root, workspace, target, follow, false);
    true
}

pub(in crate::native::layout) fn moved(event: On<Pointer<bevy::picking::events::Move>>, mut commands: Commands) {
    if event.entity != event.original_event_target() || event.delta == Vec2::ZERO { return; }
    let target = event.entity;
    commands.queue(move |world: &mut World| {
        if world.contains_resource::<crate::native::layout::desktop::HostRequests>() { return; }
        let mut entity = target;
        for _ in 0..crate::native::layout::MAX_DEPTH {
            if world.get::<CanvasItem>(entity).is_some() {
                if let Some(root) = super::super::root(world, entity)
                    && let Some(workspace) = active(world, root)
                {
                    hover(world, root, workspace, entity);
                }
                return;
            }
            let Some(parent) = world.get::<ChildOf>(entity) else { return; };
            entity = parent.parent();
        }
    });
}
