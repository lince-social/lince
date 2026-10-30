use super::*;
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
    pub(crate) fn normalize_scratch(&mut self, homes: &crate::native::sand_placement::scratch::Homes) {
        self.last = self.last.filter(|id| !homes.contains_key(&id.0));
        self.active = self.active.filter(|id| !homes.contains_key(&id.0));
        if self.active.is_none() { self.following = false; }
    }

    pub(crate) fn valid(&self) -> bool {
        self.workspace != 0 && (!self.following || self.active.is_some())
            && (!self.unfocused || self.active.is_none() && !self.following)
    }
}

#[derive(Component)]
struct Owner(String);

fn native_target(world: &World, root: Entity, workspace: u64, id: PlacementId) -> Option<Entity> {
    entity(world, root, workspace, id)
        .filter(|target| world.get::<ApplicationWindow>(*target).is_none())
}

pub(crate) fn snapshot(world: &World, root: Entity) -> Vec<Saved> {
    let Some(spaces) = world.get::<Workspaces>(root) else {
        return Vec::new();
    };
    let states = world.get_resource::<FocusState>().filter(|_| {
        world.get::<Owner>(root).is_none_or(|owner| owner.0 == spaces.canvas_id)
    });
    let mut saved = Vec::new();
    for space in spaces.entries.iter().chain(spaces.trash.iter().map(|entry| &entry.workspace)) {
        let Some(state) = states.and_then(|states| states.0.get(&(root, space.id))) else {
            if let Some(pending) = spaces.saved_focus.iter().find(|saved| saved.workspace == space.id) {
                saved.push(*pending);
            }
            continue;
        };
        let native = |id| native_target(world, root, space.id, id).is_some();
        let last = state.last.or(space.desktop.focused()).filter(|id| native(*id));
        let active = state.active.filter(|id| space.desktop.focused() == Some(*id) && native(*id));
        saved.push(Saved {
            workspace: space.id,
            last,
            active,
            following: active.is_some_and(|id| state.follow.as_ref().is_some_and(|follow| follow.target == id)),
            unfocused: state.pointer_suspended,
        });
    }
    saved
}

pub(crate) fn remember(world: &mut World, root: Entity) {
    let saved = snapshot(world, root);
    if let Some(mut spaces) = world.get_mut::<Workspaces>(root)
        && spaces.saved_focus != saved
    {
        spaces.saved_focus = saved;
    }
}

fn input_owner(world: &World) -> Option<Entity> {
    world.get_resource::<bevy::input_focus::InputFocus>()
        .and_then(|focus| focus.get())
        .filter(|input| world.get::<bevy::window::PrimaryWindow>(*input).is_none())
}

pub(super) fn restore_missing(world: &mut World) {
    let roots: Vec<_> = world.query::<(Entity, &Workspaces)>().iter(world)
        .map(|(root, spaces)| (root, spaces.canvas_id.clone(), spaces.saved_focus.clone()))
        .collect();
    for (root, canvas, saved) in roots {
        let changed = world.get::<Owner>(root).is_some_and(|owner| owner.0 != canvas);
        if changed && let Some(mut states) = world.get_resource_mut::<FocusState>() {
            states.0.retain(|(owner, _), _| *owner != root);
        }
        if world.get::<Owner>(root).is_none_or(|owner| owner.0 != canvas) {
            world.entity_mut(root).insert(Owner(canvas));
        }
        for saved in saved {
            if world.get_resource::<FocusState>().is_some_and(|states| states.0.contains_key(&(root, saved.workspace))) {
                continue;
            }
            let native = |id| native_target(world, root, saved.workspace, id);
            let last = saved.last.filter(|id| native(*id).is_some());
            let active_target = saved.active.and_then(native);
            if active_target.is_none() && saved.active.is_some()
                || last.is_none() && saved.last.is_some() && saved.active.is_none()
            {
                continue;
            }
            if saved.active.is_some() && super::active(world, root) == Some(saved.workspace)
                && (crate::native::shortcuts::keyboard_owned(world)
                    || input_owner(world)
                        .is_some_and(|input| active_target.is_none_or(|target| !contains_input(world, input, target))))
            {
                continue;
            }
            let active = saved.active;
            if let Some(id) = active.filter(|id| focused(world, root, saved.workspace) != Some(*id)) {
                let Some(bounds) = viewport_for(world, root, saved.workspace).map(|(_, usable)| usable)
                    .or_else(|| usable(world, root)) else { continue; };
                let restored = world.get_mut::<Workspaces>(root)
                    .and_then(|spaces| spaces.into_inner().entries.iter_mut().find(|space| space.id == saved.workspace))
                    .is_some_and(|space| space.desktop.focus(id, bounds).is_ok());
                if !restored { continue; }
            }
            let state = State {
                last: last.or(active),
                active,
                input: None,
                pointer_suspended: saved.unfocused,
                follow: active.filter(|_| saved.following).map(|target| Following {
                    target,
                    stamp: None,
                    frame: None,
                    offset: DVec3::ZERO,
                }),
            };
            world.init_resource::<FocusState>();
            world.resource_mut::<FocusState>().0.insert((root, saved.workspace), state);
            if active.is_some()
                && super::active(world, root) == Some(saved.workspace)
                && input_owner(world).is_none()
                && let Some(target) = active_target
            {
                keyboard_focus(world, target);
            }
        }
    }
}
