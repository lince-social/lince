// non-lean-yet
// Scope: struct State, struct Observation, fn refresh.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 65.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (43, 49).

use super::*;
use bevy::ecs::query::QueryState;
use std::collections::HashMap;

#[derive(Resource)]
struct State {
    overlays: QueryState<Entity, With<Session>>,
    fields: QueryState<(Entity, &'static FieldStatus)>,
    pending: Vec<Entity>,
    labels: Vec<(Entity, &'static str)>,
    observed: HashMap<Entity, Observation>,
}

struct Observation {
    baseline: Snapshot,
    value: Snapshot,
    equal: bool,
}

impl FromWorld for State {
    fn from_world(world: &mut World) -> Self {
        Self {
            overlays: world.query_filtered(),
            fields: world.query(),
            pending: Vec::new(),
            labels: Vec::new(),
            observed: HashMap::new(),
        }
    }
}

pub(super) fn refresh(world: &mut World) {
    let _presentation = engine::performance::Span::new("configuration-review-presentation");
    let mut state = world
        .remove_resource::<State>()
        .unwrap_or_else(|| State::from_world(world));
    state.pending.clear();
    state.pending.extend(state.overlays.iter(world));
    state
        .observed
        .retain(|overlay, _| world.get::<Session>(*overlay).is_some());
    for overlay in state.pending.drain(..) {
        let session = world.get::<Session>(overlay).unwrap();
        let owns = world
            .get_resource::<shortcuts::KeyboardModalOwner>()
            .is_some_and(|owner| owner.0 == Some(overlay));
        if !owns || !owner_eligible(world, session) {
            state.observed.remove(&overlay);
            let session = world.entity_mut(overlay).take::<Session>().unwrap();
            close(world, overlay, session);
            continue;
        }
        let previous = state.observed.get(&overlay);
        let source = previous.map_or(&session.snapshot, |previous| &previous.value);
        let error = match observe(world, session, source) {
            Ok(snapshot) => {
                let unchanged = previous
                    .filter(|previous| {
                        previous.value.same(&snapshot) && previous.baseline.same(&session.snapshot)
                    })
                    .map_or_else(|| snapshot == session.snapshot, |previous| previous.equal);
                state.observed.insert(
                    overlay,
                    Observation {
                        baseline: session.snapshot.clone(),
                        value: snapshot,
                        equal: unchanged,
                    },
                );
                (!unchanged).then(|| "The editor or active values changed. Your draft is kept; Update comparison before continuing.".to_owned())
            }
            Err(error) => Some(error),
        };
        let unresolved = session
            .proposal
            .conflicts
            .iter()
            .filter(|conflict| conflict.choice.is_none())
            .count();
        let available = error.is_none() && session.proposal.resolved();
        let message = error.unwrap_or_else(|| {
            format!(
                "{}\n{unresolved} unresolved fields. Active settings are unchanged by this review.",
                session.message
            )
        });
        let (status, confirm, sequence) = (session.status, session.confirm, session.sequence);
        state.labels.clear();
        state.labels.extend(
            state
                .fields
                .iter(world)
                .filter(|(_, field)| field.overlay == overlay && field.sequence == sequence)
                .map(|(entity, field)| {
                    let choice = session
                        .proposal
                        .conflicts
                        .get(field.index)
                        .and_then(|conflict| conflict.choice);
                    let label = match choice {
                        None => "Choose a value",
                        Some(Choice::Local) => "Selected: my draft value",
                        Some(Choice::File) => "Selected: accepted file value",
                    };
                    (entity, label)
                }),
        );
        text(world, status, message);
        disable(world, confirm, !available);
        for (entity, label) in state.labels.drain(..) {
            text(world, entity, label);
        }
    }
    world.insert_resource(state);
}
