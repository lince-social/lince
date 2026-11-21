// non-lean-yet
// Scope: struct History, struct Scope, struct Pending, struct Replay.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 97, 108, 112,
//   208, 209.
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   64, 125.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (129, 172, 215, 235).

use super::*;
use crate::host::performance::{self, Count, Stage};
use bevy::text::EditableText;
use engine::record_change::Mutation;

#[derive(Component, Default)]
struct History {
    past: Vec<Mutation>,
    future: Vec<Mutation>,
}

#[derive(Component)]
pub(super) struct Scope(pub Entity);

struct Pending {
    owner: Entity,
    inverse: Option<Mutation>,
    added: bool,
    replay: Option<Replay>,
}

struct Replay {
    redo: bool,
    submitted: Mutation,
}

#[derive(Resource, Default)]
struct Changes {
    pending: HashMap<String, Pending>,
    replay: Option<bool>,
}

pub(super) fn capture(
    world: &mut World,
    binding: &RecordBinding,
    editor: Entity,
    action: &engine::actions::Action,
) {
    let _capture = performance::Span::new(Stage::RecordHistoryCapture);
    let engine::actions::Action::ChangeRecord { request } = action else {
        return;
    };
    if !matches!(
        request.mutation,
        Mutation::Assertion { .. } | Mutation::RetractAssertion { .. } | Mutation::WorkLog { .. }
    ) {
        return;
    }
    let mut owner = editor;
    while world.get::<rows::PropertyContainer>(owner).is_none() {
        let Some(parent) = world.get::<ChildOf>(owner) else {
            return;
        };
        owner = parent.parent();
    }
    let inverse = match &request.mutation {
        Mutation::Assertion { .. } => None,
        Mutation::RetractAssertion { assertion } => {
            let row = current_record(world, binding);
            let value = row["assertions"]
                .as_array()
                .and_then(|values| values.iter().find(|value| value["uid"] == *assertion));
            if let Some(value) = value {
                let optional = |key: &str| value[key].as_str().map(str::to_owned);
                Some(Mutation::Assertion {
                    predicate: optional("predicate_uid")
                        .or_else(|| optional("predicate"))
                        .unwrap_or_default(),
                    object: optional("object"),
                    quantity: optional("quantity"),
                    unit: optional("unit"),
                })
            } else {
                let Some(value) = row["assignees"].as_array().and_then(|values| {
                    values.iter().find(|value| value["assertion"] == *assertion)
                }) else {
                    return;
                };
                Some(Mutation::Assertion {
                    predicate: value["predicate_uid"]
                        .as_str()
                        .unwrap_or("feature:assignment")
                        .into(),
                    object: value["uid"].as_str().map(str::to_owned),
                    quantity: None,
                    unit: None,
                })
            }
        }
        Mutation::WorkLog { log_id, .. } => {
            let row = current_record(world, binding);
            let value = row["work_logs"]
                .as_array()
                .and_then(|values| values.iter().find(|value| value["id"] == *log_id))
                .map(|value| serde_json::json!({"start":value["start"], "end":value["end"]}));
            Some(Mutation::WorkLog {
                log_id: log_id.clone(),
                value,
            })
        }
        _ => return,
    };
    world.init_resource::<Changes>();
    let replay = world.resource::<Changes>().replay.map(|redo| {
        performance::count(Count::RecordHistoryReplayCopies);
        Replay {
            redo,
            submitted: request.mutation.clone(),
        }
    });
    world.resource_mut::<Changes>().pending.insert(
        request.id.clone(),
        Pending {
            owner,
            inverse,
            added: matches!(request.mutation, Mutation::Assertion { .. }),
            replay,
        },
    );
    performance::count(Count::RecordHistoryCaptures);
}

fn current_record<'a>(world: &'a World, binding: &RecordBinding) -> &'a Value {
    performance::count(Count::RecordHistorySourceReads);
    world.resource::<Runtime>().areas[&binding.area]
        .data
        .iter()
        .find(|row| row["uid"].as_str() == Some(&binding.uid))
        .unwrap()
}

pub(super) fn finished(
    world: &mut World,
    id: &str,
    created: Option<String>,
    changed: bool,
    success: bool,
) {
    let Some(pending) = world
        .get_resource_mut::<Changes>()
        .and_then(|mut changes| changes.pending.remove(id))
    else {
        return;
    };
    if world.get_entity(pending.owner).is_err() {
        return;
    }
    if !success {
        if let Some(replay) = pending.replay
            && let Some(mut history) = world.get_mut::<History>(pending.owner)
        {
            if replay.redo {
                history.future.push(replay.submitted);
            } else {
                history.past.push(replay.submitted);
            }
        }
        return;
    }
    if !changed {
        return;
    }
    let inverse = if pending.added {
        created.map(|assertion| Mutation::RetractAssertion { assertion })
    } else {
        pending.inverse
    };
    let Some(inverse) = inverse else { return };
    if world.get::<History>(pending.owner).is_none() {
        world.entity_mut(pending.owner).insert(History::default());
    }
    let mut history = world.get_mut::<History>(pending.owner).unwrap();
    if pending.replay.as_ref().is_some_and(|replay| !replay.redo) {
        history.future.push(inverse);
    } else {
        history.past.push(inverse);
        if pending.replay.is_none() {
            history.future.clear();
        }
    }
}

pub(super) fn undo(world: &mut World, owner: Entity, redo: bool) {
    if world.get_resource::<Changes>().is_some_and(|changes| {
        changes
            .pending
            .values()
            .any(|pending| pending.owner == owner)
    }) {
        return;
    }
    let Some(binding) = world.get::<RecordBinding>(owner).cloned() else {
        return;
    };
    let Some(mut history) = world.get_mut::<History>(owner) else {
        return;
    };
    let mutation = if redo {
        history.future.pop()
    } else {
        history.past.pop()
    };
    let Some(mutation) = mutation else { return };
    world.resource_mut::<Changes>().replay = Some(redo);
    let action = engine::actions::Action::ChangeRecord {
        request: engine::record_change::Request {
            id: nucleus::new_uid("op"),
            record_uid: binding.uid.clone(),
            mutation: mutation.clone(),
        },
    };
    let result = execute(world, &binding, owner, action);
    world.resource_mut::<Changes>().replay = None;
    if let Err(error) = result {
        let mut history = world.get_mut::<History>(owner).unwrap();
        if redo {
            history.future.push(mutation);
        } else {
            history.past.push(mutation);
        }
        status(world, binding.area, &error);
    }
}

#[derive(Component)]
pub(super) struct TextHistory {
    current: String,
    past: VecDeque<String>,
    future: Vec<String>,
}

pub(super) fn attach_text(world: &mut World, entity: Entity) {
    let current = world
        .get::<EditableText>(entity)
        .unwrap()
        .value()
        .to_string();
    world.entity_mut(entity).insert(TextHistory {
        current,
        past: VecDeque::new(),
        future: Vec::new(),
    });
}

pub(super) fn synced_text(world: &mut World, entity: Entity, value: &str) {
    if let Some(mut history) = world.get_mut::<TextHistory>(entity)
        && history.current != value
    {
        history.current = value.into();
    }
}

pub(super) fn update(
    world: &mut World,
    changed: &mut QueryState<
        (&EditableText, &mut TextHistory),
        Or<(Changed<EditableText>, Changed<TextHistory>)>,
    >,
    editors: &mut QueryState<(&mut EditableText, &mut TextHistory)>,
) {
    let _update = performance::Span::new(Stage::RecordTextHistory);
    for (text, mut history) in changed.iter_mut(world) {
        performance::count(Count::RecordTextHistoryCandidates);
        if text.is_composing() || text.pending_paste.is_some() {
            continue;
        }
        let value = text.value();
        if value != history.current.as_str() {
            let previous = std::mem::replace(&mut history.current, value.to_string());
            history.past.push_back(previous);
            if history.past.len() > 256 {
                history.past.pop_front();
            }
            history.future.clear();
            performance::count(Count::RecordTextHistoryCaptures);
        }
    }
    let Some(keys) = world.get_resource::<ButtonInput<KeyCode>>() else {
        return;
    };
    if !keys.any_pressed([
        KeyCode::ControlLeft,
        KeyCode::ControlRight,
        KeyCode::SuperLeft,
        KeyCode::SuperRight,
    ]) || !keys.just_pressed(KeyCode::KeyZ)
    {
        return;
    }
    let redo = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    let Some(mut owner) = world
        .get_resource::<bevy::input_focus::InputFocus>()
        .and_then(|focus| focus.get())
    else {
        return;
    };
    if crate::native::record_binding::active(world, owner) {
        return;
    }
    if let Some(text) = world.get::<EditableText>(owner) {
        if text.is_composing() || text.pending_paste.is_some() {
            return;
        }
        if let Ok((mut text, mut history)) = editors.get_mut(world, owner) {
            let value = if redo {
                history.future.pop()
            } else {
                history.past.pop_back()
            };
            if let Some(value) = value {
                let previous = std::mem::replace(&mut history.current, value);
                if redo {
                    history.past.push_back(previous);
                } else {
                    history.future.push(previous);
                }
                text.editor.set_text(&history.current);
                performance::count(Count::RecordTextHistoryRestores);
                return;
            }
        } else {
            return;
        }
    }
    loop {
        if let Some(scope) = world.get::<Scope>(owner) {
            owner = scope.0;
        }
        if world.get::<History>(owner).is_some() {
            undo(world, owner, redo);
            return;
        }
        let Some(parent) = world.get::<ChildOf>(owner) else {
            return;
        };
        owner = parent.parent();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejected_undo_retains_the_change_for_retry() {
        let mut world = World::new();
        let owner = world.spawn(History::default()).id();
        world.insert_resource(Changes::default());
        world.resource_mut::<Changes>().pending.insert(
            "request".into(),
            Pending {
                owner,
                inverse: None,
                added: false,
                replay: Some(Replay {
                    redo: false,
                    submitted: Mutation::RetractAssertion {
                        assertion: "assignment".into(),
                    },
                }),
            },
        );
        finished(&mut world, "request", None, false, false);
        let history = world.get::<History>(owner).unwrap();
        assert!(
            matches!(history.past.as_slice(), [Mutation::RetractAssertion { assertion }] if assertion == "assignment")
        );
        assert!(history.future.is_empty());
        assert!(world.resource::<Changes>().pending.is_empty());
    }
}
