use super::*;
use crate::native::{
    actions::{Action, Modifiers},
    shortcuts::Settings,
};
use bevy::input_focus::{FocusCause, InputFocus};

mod catalogue;
mod ui;
use catalogue::{Entry, Target};

const LIMIT: usize = 4096;

#[derive(Default)]
struct History {
    order: Vec<Target>,
    inputs: BTreeMap<Target, Entity>,
    observed: Option<Target>,
}

#[derive(Resource, Default)]
struct Recent {
    histories: BTreeMap<Entity, History>,
    pending: BTreeMap<u64, (Entity, Target)>,
    next_request: u64,
    session: Option<Session>,
}

struct Session {
    root: Entity,
    workspace: u64,
    output: Option<OutputId>,
    overlay: Entity,
    entries: Vec<Target>,
    selected: usize,
    rows: BTreeMap<Target, (Entity, Entity)>,
    previous: Option<Entity>,
    hold: Modifiers,
}

#[derive(Clone, Copy)]
enum Change {
    Cancel,
    Choose(Target),
}

impl Action for Change {
    fn apply(&self, world: &mut World, target: Entity) {
        if !world
            .get_resource::<Recent>()
            .and_then(|state| state.session.as_ref())
            .is_some_and(|session| session.overlay == target)
            || !active(world)
        {
            return;
        }
        match *self {
            Self::Cancel => dismiss(world),
            Self::Choose(target) => {
                let root = world.resource::<Recent>().session.as_ref().unwrap().root;
                close(world);
                enqueue(world, root, target);
            }
        }
    }
}

fn promote(order: &mut Vec<Target>, target: Target) {
    order.retain(|current| *current != target);
    order.insert(0, target);
    order.truncate(LIMIT);
}

pub fn active(world: &World) -> bool {
    world
        .get_resource::<Recent>()
        .and_then(|state| state.session.as_ref())
        .is_some_and(|session| {
            world.get_entity(session.overlay).is_ok()
                && world
                    .get_resource::<crate::native::shortcuts::KeyboardModalOwner>()
                    .is_some_and(|owner| owner.0 == Some(session.overlay))
        })
}

pub(super) fn observe(world: &mut World, root: Entity) {
    let current = catalogue::current(world, root);
    let input = world
        .get_resource::<InputFocus>()
        .and_then(InputFocus::get)
        .filter(|input| {
            current.is_some_and(|target| focus::contains_input(world, *input, target.entity))
        });
    world.init_resource::<Recent>();
    let mut state = world.resource_mut::<Recent>();
    if state.pending.values().any(|(owner, _)| *owner == root) {
        return;
    }
    let history = state.histories.entry(root).or_default();
    if history.observed != current {
        if let Some(target) = current {
            promote(&mut history.order, target);
        }
        history.observed = current;
    }
    if let Some((target, input)) = current.zip(input) {
        history.inputs.insert(target, input);
    }
}

pub(super) fn cycle(world: &mut World, root: Entity, reverse: bool) {
    if active(world) {
        if world.resource::<Recent>().session.as_ref().unwrap().root == root {
            navigate(world, reverse);
        }
        return;
    }
    if crate::native::shortcuts::keyboard_owned(world) {
        return;
    }
    if world
        .get_resource::<Recent>()
        .is_some_and(|state| state.session.is_some())
    {
        close(world);
    }
    observe(world, root);
    let mut entries = catalogue::entries(world, root);
    let state = world.resource::<Recent>();
    let mut order = state
        .histories
        .get(&root)
        .map(|history| history.order.clone())
        .unwrap_or_default();
    let mut current = catalogue::current(world, root);
    for (_, target) in state.pending.values().filter(|(owner, _)| *owner == root) {
        if catalogue::workspace(world, root, *target).is_some() {
            promote(&mut order, *target);
            current = Some(*target);
        }
    }
    if entries.is_empty() || entries.len() == 1 && current == Some(entries[0].target) {
        return;
    }
    let ranks: BTreeMap<_, _> = order
        .into_iter()
        .enumerate()
        .map(|(index, target)| (target, index))
        .collect();
    entries.sort_by_key(|entry| ranks.get(&entry.target).copied().unwrap_or(usize::MAX));
    entries.truncate(LIMIT);
    let selected = current
        .and_then(|target| entries.iter().position(|entry| entry.target == target))
        .map_or(0, |index| step(index, entries.len(), reverse));
    let hold = world
        .get_resource::<Settings>()
        .and_then(|settings| settings.recent_hold().ok().flatten());
    if let Some(hold) = hold.filter(|hold| {
        world
            .get_resource::<crate::native::keyboard::Dispatch>()
            .is_some_and(|event| event.modifiers.contains(*hold))
    }) {
        ui::open(world, root, entries, selected, hold);
    } else {
        enqueue(world, root, entries[selected].target);
    }
}

fn step(index: usize, count: usize, reverse: bool) -> usize {
    if reverse {
        (index + count - 1) % count
    } else {
        (index + 1) % count
    }
}

fn navigate(world: &mut World, reverse: bool) {
    prune(world);
    let mut state = world.remove_resource::<Recent>().unwrap();
    if let Some(session) = state.session.as_mut() {
        session.selected = step(session.selected, session.entries.len(), reverse);
        ui::refresh(world, session, true);
    }
    world.insert_resource(state);
}

pub(crate) fn key(
    world: &mut World,
    root: Entity,
    event: &crate::native::keyboard::PreparedKey,
    binding: bool,
) -> bool {
    if !active(world)
        || !world
            .resource::<Recent>()
            .session
            .as_ref()
            .is_some_and(|session| session.root == root)
    {
        return false;
    }
    let hold = world.resource::<Recent>().session.as_ref().unwrap().hold;
    if !event.input.state.is_pressed() {
        if event.roles.intersection(hold) != Modifiers::NONE && !event.after.contains(hold) {
            commit(world);
        }
        return true;
    }
    if !event.input.repeat {
        let matches = |key| {
            crate::native::actions::native_key_matches(
                event.input.key_code,
                &event.input.logical_key,
                key,
            )
        };
        if matches(KeyCode::Escape) {
            close(world);
            return true;
        }
        if binding {
            return false;
        }
        if matches(KeyCode::Enter) || matches(KeyCode::Space) {
            commit(world);
            return true;
        }
        let reverse = matches(KeyCode::ArrowUp) || matches(KeyCode::ArrowLeft);
        if reverse || matches(KeyCode::ArrowDown) || matches(KeyCode::ArrowRight) {
            navigate(world, reverse);
            return true;
        }
    }
    !binding
}

pub fn dismiss(world: &mut World) {
    close(world);
}

fn close(world: &mut World) {
    let session = world
        .get_resource_mut::<Recent>()
        .and_then(|mut state| state.session.take());
    let Some(session) = session else {
        return;
    };
    let owns = world
        .get_resource::<crate::native::shortcuts::KeyboardModalOwner>()
        .is_some_and(|owner| owner.0 == Some(session.overlay));
    if owns {
        crate::native::shortcuts::release_keyboard(world, session.overlay);
        if let Some(mut focus) = world.get_resource_mut::<InputFocus>() {
            focus.clear();
        }
        let previous = session.previous.filter(|input| {
            world
                .get::<Workspaces>(session.root)
                .is_some_and(|spaces| spaces.active == session.workspace)
                && focus::return_input(
                    world,
                    session.root,
                    session.workspace,
                    *input,
                    catalogue::current(world, session.root).map(|target| target.entity),
                    catalogue::current(world, session.root).is_some(),
                )
        });
        if let Some(previous) = previous {
            if let Some(mut focus) = world.get_resource_mut::<InputFocus>() {
                focus.set(previous, FocusCause::Navigated);
            }
        } else {
            focus::restore_input(world, session.root);
        }
    }
    if world.get_entity(session.overlay).is_ok() {
        world.despawn(session.overlay);
    }
}

fn commit(world: &mut World) {
    prune(world);
    let target = world
        .get_resource::<Recent>()
        .and_then(|state| state.session.as_ref())
        .map(|session| (session.root, session.entries[session.selected]));
    close(world);
    if let Some((root, target)) = target {
        enqueue(world, root, target);
    }
}

fn enqueue(world: &mut World, root: Entity, target: Target) {
    if catalogue::workspace(world, root, target).is_none() {
        return;
    }
    world.init_resource::<Recent>();
    let mut state = world.resource_mut::<Recent>();
    let Some(request) = state
        .next_request
        .checked_add(1)
        .filter(|_| state.pending.len() < 64)
    else {
        return;
    };
    state.next_request = request;
    state.pending.insert(request, (root, target));
    DesktopAction::CommitRecent { request }.apply(world, root);
}

pub fn selection(world: &World, root: Entity, request: u64) -> Option<(u64, [u8; 16])> {
    let (owner, target) = world.get_resource::<Recent>()?.pending.get(&request)?;
    if *owner != root {
        return None;
    }
    Some((
        catalogue::workspace(world, root, *target)?,
        target.placement.0,
    ))
}

pub fn activate(world: &mut World, root: Entity, request: u64) -> bool {
    let Some((workspace, _)) = selection(world, root, request) else {
        acknowledge(world, root, request, false);
        return false;
    };
    let target = world.resource::<Recent>().pending[&request].1;
    if world.get::<Workspaces>(root).unwrap().active != workspace {
        crate::native::workspace::switch_output(world, root, workspace);
    }
    focus::ensure(world, root);
    if world.get::<Workspaces>(root).unwrap().active != workspace
        || catalogue::workspace(world, root, target).is_none()
    {
        acknowledge(world, root, request, false);
        return false;
    }
    {
        let mut spaces = world.get_mut::<Workspaces>(root).unwrap();
        let model = &mut spaces
            .entries
            .iter_mut()
            .find(|space| space.id == workspace)
            .unwrap()
            .desktop;
        if let Some(fullscreen) = model.fullscreen().filter(|id| *id != target.placement) {
            model.request_state(
                fullscreen,
                crate::host::WindowRequest {
                    fullscreen: Some(false),
                    ..default()
                },
            );
        }
    }
    focus::select(world, root, workspace, target.entity, true);
    let input = world
        .resource::<Recent>()
        .histories
        .get(&root)
        .and_then(|history| history.inputs.get(&target))
        .copied()
        .filter(|input| {
            focus::return_input(world, root, workspace, *input, Some(target.entity), true)
        });
    if let Some(input) = input
        && let Some(mut focus) = world.get_resource_mut::<InputFocus>()
    {
        focus.set(input, FocusCause::Navigated);
    }
    acknowledge(world, root, request, true);
    true
}

pub fn acknowledge(world: &mut World, root: Entity, request: u64, accepted: bool) {
    let Some(mut state) = world.remove_resource::<Recent>() else {
        return;
    };
    if state
        .pending
        .get(&request)
        .is_some_and(|(owner, _)| *owner == root)
        && let Some((_, target)) = state.pending.remove(&request)
        && accepted
    {
        let history = state.histories.entry(root).or_default();
        promote(&mut history.order, target);
        history.observed = Some(target);
    }
    if let Some(session) = state
        .session
        .as_mut()
        .filter(|session| session.root == root)
        && world
            .get_resource::<crate::native::shortcuts::KeyboardModalOwner>()
            .is_some_and(|owner| owner.0 == Some(session.overlay))
    {
        session.workspace = world
            .get::<Workspaces>(root)
            .map_or(session.workspace, |spaces| spaces.active);
        session.output = catalogue::output(world, root);
        session.previous = world
            .get_resource::<InputFocus>()
            .and_then(InputFocus::get)
            .filter(|input| !focus::contains_input(world, *input, session.overlay));
        ui::refresh(world, session, true);
    }
    world.insert_resource(state);
    observe(world, root);
}

fn prune(world: &mut World) {
    let Some(mut state) = world.remove_resource::<Recent>() else {
        return;
    };
    let empty = state.session.as_mut().is_some_and(|session| {
        let selected = session.entries.get(session.selected).copied();
        session
            .entries
            .retain(|target| catalogue::workspace(world, session.root, *target).is_some());
        if session.entries.is_empty() {
            return true;
        }
        session.selected = selected
            .and_then(|target| session.entries.iter().position(|entry| *entry == target))
            .unwrap_or(session.selected.min(session.entries.len() - 1));
        ui::refresh(
            world,
            session,
            selected != session.entries.get(session.selected).copied(),
        );
        false
    });
    world.insert_resource(state);
    if empty {
        close(world);
    }
}

pub(crate) fn update(world: &mut World) {
    let roots: Vec<_> = world
        .query_filtered::<Entity, With<Workspaces>>()
        .iter(world)
        .collect();
    for root in roots {
        observe(world, root);
    }
    let Some(mut state) = world.remove_resource::<Recent>() else {
        return;
    };
    state.histories.retain(|root, history| {
        if world.get::<Workspaces>(*root).is_none() {
            return false;
        }
        history
            .order
            .retain(|target| catalogue::workspace(world, *root, *target).is_some());
        let targets: BTreeSet<_> = history.order.iter().copied().collect();
        history.inputs.retain(|target, input| {
            targets.contains(target) && focus::contains_input(world, *input, target.entity)
        });
        true
    });
    state
        .pending
        .retain(|_, (root, target)| catalogue::workspace(world, *root, *target).is_some());
    let unfocused = world
        .query_filtered::<&Window, With<bevy::window::PrimaryWindow>>()
        .iter(world)
        .any(|window| !window.focused);
    let invalid = state.session.as_ref().is_some_and(|session| {
        unfocused
            || world.get_entity(session.overlay).is_err()
            || !world
                .get_resource::<crate::native::shortcuts::KeyboardModalOwner>()
                .is_some_and(|owner| owner.0 == Some(session.overlay))
            || !world
                .get::<Workspaces>(session.root)
                .is_some_and(|spaces| spaces.active == session.workspace)
            || catalogue::output(world, session.root) != session.output
    });
    world.insert_resource(state);
    if invalid {
        close(world);
        return;
    }
    prune(world);
    if world
        .get_resource::<Recent>()
        .and_then(|state| state.session.as_ref())
        .is_some_and(|session| {
            crate::native::keyboard::current(world).is_some_and(|held| !held.contains(session.hold))
        })
    {
        commit(world);
    }
}

#[cfg(test)]
mod tests;
