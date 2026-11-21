use super::*;
use crate::native::sand_placement::scratch::Homes;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Saved {
    pub(crate) workspace: u64,
    last: Option<PlacementId>,
    active: Option<PlacementId>,
    following: bool,
    unfocused: bool,
}

impl Saved {
    pub(crate) fn normalize_scratch(&mut self, homes: &Homes) {
        self.last = self.last.filter(|id| !homes.contains_key(&id.0));
        self.active = self.active.filter(|id| !homes.contains_key(&id.0));
        self.following &= self.active.is_some();
    }

    pub(crate) fn valid(&self) -> bool {
        self.workspace != 0
            && match self.active {
                Some(_) => !self.unfocused,
                None => !self.following,
            }
    }
}

#[derive(Component)]
struct Owner(String);

fn native_target(world: &World, root: Entity, workspace: u64, id: PlacementId) -> Option<Entity> {
    entity(world, root, workspace, id)
        .filter(|target| world.get::<ApplicationWindow>(*target).is_none())
}

fn native_targets(
    world: &World,
    root: Entity,
    workspace: u64,
    last: Option<PlacementId>,
    active: Option<PlacementId>,
) -> (Option<Entity>, Option<Entity>) {
    let previous = last.and_then(|id| native_target(world, root, workspace, id));
    let current = if active == last {
        previous
    } else {
        active.and_then(|id| native_target(world, root, workspace, id))
    };
    (previous, current)
}

pub(crate) fn snapshot(world: &World, root: Entity) -> Vec<Saved> {
    let Some(spaces) = world.get::<Workspaces>(root) else {
        return Vec::new();
    };
    let states = world.get_resource::<FocusState>().filter(|_| {
        world
            .get::<Owner>(root)
            .is_none_or(|owner| owner.0 == spaces.canvas_id)
    });
    spaces
        .entries
        .iter()
        .chain(spaces.trash.iter().map(|entry| &entry.workspace))
        .filter_map(|space| {
            let Some(state) = states.and_then(|states| states.0.get(&(root, space.id))) else {
                return spaces
                    .saved_focus
                    .iter()
                    .find(|saved| saved.workspace == space.id)
                    .copied();
            };
            let focused = space.desktop.focused();
            let last = state.last.or(focused);
            let active = state.active.filter(|id| focused == Some(*id));
            let (last_target, active_target) = native_targets(world, root, space.id, last, active);
            let active = active.filter(|_| active_target.is_some());
            Some(Saved {
                workspace: space.id,
                last: last.filter(|_| last_target.is_some()),
                active,
                following: state
                    .follow
                    .as_ref()
                    .is_some_and(|follow| active == Some(follow.target)),
                unfocused: state.pointer_suspended,
            })
        })
        .collect()
}

pub(crate) fn remember(world: &mut World, root: Entity) {
    let saved = snapshot(world, root);
    if let Some(mut spaces) = world.get_mut::<Workspaces>(root)
        && spaces.saved_focus != saved
    {
        spaces.saved_focus = saved;
    }
}

mod restoration;

pub(super) fn restore_missing(world: &mut World) {
    restoration::restore_missing(world);
}
