use super::*;

#[derive(Resource)]
struct Restoration {
    roots: QueryState<(Entity, &'static Workspaces, Option<&'static Owner>)>,
    requests: Vec<Request>,
}

impl FromWorld for Restoration {
    fn from_world(world: &mut World) -> Self {
        Self {
            roots: world.query(),
            requests: Vec::new(),
        }
    }
}

enum Request {
    Owner {
        root: Entity,
        canvas: String,
        reset: bool,
    },
    Focus {
        root: Entity,
        saved: Saved,
    },
}

impl Restoration {
    fn prepare(&mut self, world: &World) {
        self.requests.clear();
        let states = world.get_resource::<FocusState>();
        let mut roots = 0;
        let mut examined = 0;
        let mut pending = 0;
        // Capture before applying, as before. Restoration only clears states
        // for its own root (before its entries), then inserts states. Therefore
        // an already-present state on an unchanged owner cannot become missing
        // in the application phase. Missing entries still need live checks.
        for (root, spaces, owner) in self.roots.iter(world) {
            roots += 1;
            let changed = owner.is_some_and(|owner| owner.0 != spaces.canvas_id);
            if changed || owner.is_none() {
                self.requests.push(Request::Owner {
                    root,
                    canvas: spaces.canvas_id.clone(),
                    reset: changed,
                });
            }
            for saved in &spaces.saved_focus {
                examined += 1;
                if changed
                    || states.is_none_or(|states| !states.0.contains_key(&(root, saved.workspace)))
                {
                    self.requests.push(Request::Focus {
                        root,
                        saved: *saved,
                    });
                    pending += 1;
                }
            }
        }
        engine::performance::add("desktop-focus-restore-roots", roots);
        engine::performance::add("desktop-focus-saved-entries-examined", examined);
        engine::performance::add("desktop-focus-saved-entries-prepared", pending);
    }
}

pub(super) fn restore_missing(world: &mut World) {
    let _restore = engine::performance::Span::new("desktop-focus-restoration");
    world.init_resource::<Restoration>();
    world.resource_scope(|world, mut restoration: Mut<Restoration>| {
        restoration.prepare(world);
        // The queue keeps root and saved-entry order, including duplicate saved
        // workspace IDs. A successful earlier restore still wins the live check.
        for request in restoration.requests.drain(..) {
            match request {
                Request::Owner {
                    root,
                    canvas,
                    reset,
                } => {
                    if reset && let Some(mut states) = world.get_resource_mut::<FocusState>() {
                        states.0.retain(|(owner, _), _| *owner != root);
                    }
                    world.entity_mut(root).insert(Owner(canvas));
                    engine::performance::add("desktop-focus-owner-updates", 1);
                }
                Request::Focus { root, saved } => restore(world, root, saved),
            }
        }
    });
}

fn input_owner(world: &World) -> Option<Entity> {
    world
        .get_resource::<bevy::input_focus::InputFocus>()
        .and_then(|focus| focus.get())
        .filter(|input| world.get::<bevy::window::PrimaryWindow>(*input).is_none())
}

fn restore(world: &mut World, root: Entity, saved: Saved) {
    if world
        .get_resource::<FocusState>()
        .is_some_and(|states| states.0.contains_key(&(root, saved.workspace)))
    {
        return;
    }
    let (last_target, active_target) =
        native_targets(world, root, saved.workspace, saved.last, saved.active);
    let last = saved.last.filter(|_| last_target.is_some());
    if active_target.is_none() && saved.active.is_some()
        || last.is_none() && saved.last.is_some() && saved.active.is_none()
    {
        return;
    }
    if saved.active.is_some()
        && super::super::active(world, root) == Some(saved.workspace)
        && (crate::native::shortcuts::keyboard_owned(world)
            || input_owner(world).is_some_and(|input| {
                active_target.is_none_or(|target| !contains_input(world, input, target))
            }))
    {
        return;
    }
    let active = saved.active;
    if let Some(id) = active.filter(|id| focused(world, root, saved.workspace) != Some(*id)) {
        let Some(bounds) = viewport_for(world, root, saved.workspace)
            .map(|(_, usable)| usable)
            .or_else(|| usable(world, root))
        else {
            return;
        };
        let restored = world
            .get_mut::<Workspaces>(root)
            .and_then(|spaces| {
                spaces
                    .into_inner()
                    .entries
                    .iter_mut()
                    .find(|space| space.id == saved.workspace)
            })
            .is_some_and(|space| space.desktop.focus(id, bounds).is_ok());
        if !restored {
            return;
        }
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
    world
        .resource_mut::<FocusState>()
        .0
        .insert((root, saved.workspace), state);
    engine::performance::add("desktop-focus-states-restored", 1);
    if active.is_some()
        && super::super::active(world, root) == Some(saved.workspace)
        && input_owner(world).is_none()
        && let Some(target) = active_target
    {
        keyboard_focus(world, target);
    }
}
