use super::*;
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
    submitted: Mutation,
    added: bool,
    replay: Option<bool>,
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
    let engine::actions::Action::ChangeRecord { request } = action else {
        return;
    };
    let mut owner = editor;
    while world.get::<rows::PropertyContainer>(owner).is_none() {
        let Some(parent) = world.get::<ChildOf>(owner) else {
            return;
        };
        owner = parent.parent();
    }
    let row = world.resource::<Runtime>().areas[&binding.area]
        .data
        .iter()
        .find(|row| row["uid"].as_str() == Some(&binding.uid))
        .unwrap();
    let inverse = match &request.mutation {
        Mutation::Assertion { .. } => None,
        Mutation::RetractAssertion { assertion } => {
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
                    predicate: "assigned-to".into(),
                    object: value["uid"].as_str().map(str::to_owned),
                    quantity: None,
                    unit: None,
                })
            }
        }
        Mutation::WorkLog { log_id, .. } => {
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
    let replay = world.resource::<Changes>().replay;
    world.resource_mut::<Changes>().pending.insert(
        request.id.clone(),
        Pending {
            owner,
            inverse,
            submitted: request.mutation.clone(),
            added: matches!(request.mutation, Mutation::Assertion { .. }),
            replay,
        },
    );
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
        if let Some(redo) = pending.replay
            && let Some(mut history) = world.get_mut::<History>(pending.owner)
        {
            if redo {
                history.future.push(pending.submitted);
            } else {
                history.past.push(pending.submitted);
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
    if pending.replay == Some(false) {
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
struct TextHistory {
    current: String,
    past: Vec<String>,
    future: Vec<String>,
}

pub(super) fn attach_text(world: &mut World, entity: Entity) {
    let current = world
        .get::<bevy::text::EditableText>(entity)
        .unwrap()
        .value()
        .to_string();
    world.entity_mut(entity).insert(TextHistory {
        current,
        past: Vec::new(),
        future: Vec::new(),
    });
}

pub(super) fn synced_text(world: &mut World, entity: Entity, value: &str) {
    if let Some(mut history) = world.get_mut::<TextHistory>(entity) {
        history.current = value.into();
    }
}

pub(super) fn update(world: &mut World) {
    for (text, mut history) in world
        .query::<(&bevy::text::EditableText, &mut TextHistory)>()
        .iter_mut(world)
    {
        if text.is_composing() || text.pending_paste.is_some() {
            continue;
        }
        let value = text.value().to_string();
        if value != history.current {
            let previous = std::mem::replace(&mut history.current, value);
            history.past.push(previous);
            if history.past.len() > 256 {
                history.past.remove(0);
            }
            history.future.clear();
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
    if crate::record_binding::active(world, owner) {
        return;
    }
    if let Some(text) = world.get::<bevy::text::EditableText>(owner) {
        if text.is_composing() || text.pending_paste.is_some() {
            return;
        }
        if let Some(mut history) = world.get_mut::<TextHistory>(owner) {
            let value = if redo {
                history.future.pop()
            } else {
                history.past.pop()
            };
            if let Some(value) = value {
                let previous = std::mem::replace(&mut history.current, value.clone());
                if redo {
                    history.past.push(previous);
                } else {
                    history.future.push(previous);
                }
                world
                    .get_mut::<bevy::text::EditableText>(owner)
                    .unwrap()
                    .editor
                    .set_text(&value);
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
                submitted: Mutation::RetractAssertion {
                    assertion: "assignment".into(),
                },
                added: false,
                replay: Some(false),
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
