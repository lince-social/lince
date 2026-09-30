use std::collections::{HashSet, VecDeque};

use cell::{ClientMessage, ServerMessage};
use lince_interface::karma::transfers;

use super::*;

#[cfg(test)]
#[path = "karma_tests.rs"]
mod tests;

#[derive(Component, Default)]
struct Automation {
    transfer: String,
    person: String,
    source: Value,
    data: Option<Value>,
    request: Option<String>,
    pause_request: Option<String>,
    pauses: VecDeque<(String, i64)>,
    selected: HashSet<String>,
    retreat: bool,
    notice: String,
}

#[derive(Clone)]
enum Command {
    Select(String),
    Pause(bool),
    Edit(String),
    Refresh,
    Dismiss,
}

impl Action for Command {
    fn apply(&self, world: &mut World, owner: Entity) {
        if crate::laboratory::suspended(world, owner) {
            return;
        }
        match self {
            Self::Select(uid) => {
                let mut state = world.get_mut::<Automation>(owner).unwrap();
                if !state.selected.remove(uid) {
                    state.selected.insert(uid.clone());
                }
            }
            Self::Pause(all) => {
                let state = world.get::<Automation>(owner).unwrap();
                if state.pause_request.is_some() {
                    return;
                }
                let rules = state
                    .data
                    .as_ref()
                    .map(|data| array(data, "rules"))
                    .unwrap_or_default();
                let pauses = rules
                    .iter()
                    .filter(|rule| {
                        rule["paused"] == false
                            && (*all || state.selected.contains(&text(rule, "uid")))
                    })
                    .filter_map(|rule| Some((text(rule, "uid"), rule["revision"].as_i64()?)))
                    .collect();
                world.get_mut::<Automation>(owner).unwrap().pauses = pauses;
                pause_next(world, owner);
            }
            Self::Edit(uid) => {
                crate::karma_castle::open_rule(world, owner, uid);
            }
            Self::Refresh => {
                world.get_mut::<Automation>(owner).unwrap().source = Value::Null;
                maintain(world, owner);
            }
            Self::Dismiss => world.get_mut::<Automation>(owner).unwrap().retreat = false,
        }
        ui::render(world, owner);
    }
}

fn send(world: &World, id: &str, action: engine::actions::Action) -> Result<(), String> {
    if crate::laboratory::active(world) {
        return Err("Automation is read-only in the Laboratory".into());
    }
    world
        .get_non_send::<CellBridge>()
        .ok_or("No Cell connection")?
        .outgoing
        .try_send(ClientMessage::Act {
            id: id.into(),
            action,
        })
        .map_err(|error| error.to_string())
}

pub(super) fn maintain(world: &mut World, owner: Entity) {
    let Some(castle) = world.get::<TransferCastle>(owner) else {
        return;
    };
    let transfer = castle.selected.clone();
    if transfer.is_empty() || crate::laboratory::suspended(world, owner) {
        return;
    }
    let view = world.get::<View>(owner).unwrap();
    let person = view.context["acting_person"]
        .as_str()
        .map(str::to_owned)
        .unwrap_or_default();
    let source = view
        .rows
        .iter()
        .find(|row| text(row, "uid") == transfer)
        .cloned()
        .unwrap_or(Value::Null);
    if source.is_null() || person.is_empty() {
        return;
    }
    if world.get::<Automation>(owner).is_none() {
        world.entity_mut(owner).insert(Automation::default());
    }
    let state = world.get::<Automation>(owner).unwrap();
    if state.request.is_some()
        || state.pause_request.is_some()
        || (state.transfer == transfer && state.person == person && state.source == source)
    {
        return;
    }
    let id = nucleus::new_uid("transfer-automation");
    match send(
        world,
        &id,
        engine::actions::Action::InspectTransferKarma {
            transfer: transfer.clone(),
            person: Some(person.clone()),
        },
    ) {
        Ok(()) => {
            let mut state = world.get_mut::<Automation>(owner).unwrap();
            if state.transfer != transfer || state.person != person {
                state.data = None;
                state.retreat = false;
                state.selected.clear();
            }
            state.transfer = transfer;
            state.person = person;
            state.source = source;
            state.request = Some(id);
        }
        Err(error) => {
            world.get_mut::<Automation>(owner).unwrap().notice = error;
        }
    }
}

fn pause_next(world: &mut World, owner: Entity) {
    let Some((uid, revision)) = world
        .get_mut::<Automation>(owner)
        .unwrap()
        .pauses
        .pop_front()
    else {
        let mut state = world.get_mut::<Automation>(owner).unwrap();
        state.pause_request = None;
        state.source = Value::Null;
        state.notice = "Selected Rules paused".into();
        maintain(world, owner);
        return;
    };
    let id = nucleus::new_uid("pause-transfer-rule");
    let action = engine::actions::Action::SetRecurrencePaused {
        recurrence: uid,
        expected_revision: revision,
        request_id: id.clone(),
        paused: true,
    };
    match send(world, &id, action) {
        Ok(()) => world.get_mut::<Automation>(owner).unwrap().pause_request = Some(id),
        Err(error) => {
            let mut state = world.get_mut::<Automation>(owner).unwrap();
            state.pauses.clear();
            state.notice = error;
        }
    }
}

pub(super) fn receive(world: &mut World, message: &ServerMessage) -> bool {
    let (id, data, error) = match message {
        ServerMessage::ActionOk { id, data, .. } => (id, data.as_ref(), None),
        ServerMessage::Error { id, message, .. } => (id, None, Some(message.as_str())),
        _ => return false,
    };
    let owner = world
        .query::<(Entity, &Automation)>()
        .iter(world)
        .find(|(_, state)| {
            state.request.as_ref() == Some(id) || state.pause_request.as_ref() == Some(id)
        })
        .map(|(owner, _)| owner);
    let Some(owner) = owner else {
        return false;
    };
    let pause = world
        .get::<Automation>(owner)
        .unwrap()
        .pause_request
        .as_ref()
        == Some(id);
    if let Some(error) = error {
        let mut state = world.get_mut::<Automation>(owner).unwrap();
        state.request = None;
        state.pause_request = None;
        state.pauses.clear();
        state.notice = error.into();
    } else if pause {
        world.get_mut::<Automation>(owner).unwrap().pause_request = None;
        pause_next(world, owner);
    } else {
        let mut state = world.get_mut::<Automation>(owner).unwrap();
        state.request = None;
        if let Some(data) =
            data.filter(|data| data["transfer"] == state.transfer && data["person"] == state.person)
        {
            if let Some(before) = state.data.as_ref().and_then(|data| {
                serde_json::from_value::<nucleus::transfer::karma::Snapshot>(data["state"].clone())
                    .ok()
            }) && let Ok(after) =
                serde_json::from_value::<nucleus::transfer::karma::Snapshot>(data["state"].clone())
            {
                state.retreat |= transfers::retreated(&before, &after, &state.person);
            }
            state.data = Some(data.clone());
            state.notice.clear();
        }
    }
    ui::render(world, owner);
    true
}

pub(super) fn render(world: &mut World, owner: Entity, parent: Entity, transfer: &Value) {
    let Some(state) = world.get::<Automation>(owner) else {
        return;
    };
    if state.transfer != text(transfer, "uid") {
        return;
    }
    let Some(data) = state.data.clone() else {
        return;
    };
    let selected = state.selected.clone();
    let retreat = state.retreat;
    let busy = state.pause_request.is_some();
    let notice = state.notice.clone();
    let block = ui::stack(world, parent);
    crate::edit_mode::label(world, block, "Agreement automation", 16.0);
    if retreat {
        crate::edit_mode::label(
            world,
            block,
            "Your agreement decreased. Enabled Rules can raise it again when their Condition and Threshold pass. You can pause them below.",
            13.0,
        );
        crate::castle_feed::button(world, block, owner, "Keep Rules enabled", Command::Dismiss);
    }
    if !notice.is_empty() {
        crate::edit_mode::label(world, block, &notice, 12.0);
    }
    if array(&data, "rules")
        .iter()
        .flat_map(|rule| array(rule, "commands"))
        .any(|command| {
            !command["dispatched_at"].is_null()
                && matches!(command["status"].as_str(), Some("queued" | "sent"))
        })
    {
        crate::edit_mode::label(
            world,
            block,
            "Pausing stops future firings and unsent commands. Issued commands may still finish.",
            12.0,
        );
    }
    let rules: Vec<_> = array(&data, "rules").iter().cloned().collect();
    for rule in &rules {
        let uid = text(rule, "uid");
        let line = ui::row(world, block);
        crate::castle_feed::button(
            world,
            line,
            owner,
            &format!(
                "{} {}{}",
                if selected.contains(&uid) {
                    "☑"
                } else {
                    "☐"
                },
                text(rule, "name"),
                if rule["paused"] == true {
                    " · paused"
                } else {
                    ""
                }
            ),
            Command::Select(uid.clone()),
        );
        crate::castle_feed::button(world, line, owner, "Edit Rule", Command::Edit(uid));
        for effect in array(rule, "effects") {
            let target = effect["target"]
                .as_u64()
                .map(|level| level.to_string())
                .unwrap_or_else(|| "unavailable".into());
            let maximum = if effect["can_raise_to_maximum"] == true {
                " · can reach 2"
            } else {
                ""
            };
            crate::edit_mode::label(
                world,
                block,
                &format!(
                    "{} target {target}{maximum}",
                    if effect["fixed"] == true {
                        "Fixed"
                    } else {
                        "Current calculated"
                    }
                ),
                12.0,
            );
        }
        for pending in array(rule, "pending") {
            if let Some(at) = pending["at_ms"]
                .as_i64()
                .and_then(chrono::DateTime::from_timestamp_millis)
            {
                crate::edit_mode::label(
                    world,
                    block,
                    &format!("Pending {} · {}", at.to_rfc3339(), text(pending, "status")),
                    12.0,
                );
            }
        }
        for command in array(rule, "commands") {
            crate::edit_mode::label(
                world,
                block,
                &format!(
                    "Remote {}{}",
                    text(command, "status"),
                    if command["cancelled"] == true {
                        " · cancelled before dispatch"
                    } else if !command["dispatched_at"].is_null() {
                        " · issued"
                    } else {
                        " · awaiting dispatch"
                    }
                ),
                12.0,
            );
        }
    }
    if !rules.is_empty() && !busy {
        let line = ui::row(world, block);
        crate::castle_feed::button(world, line, owner, "Pause selected", Command::Pause(false));
        crate::castle_feed::button(
            world,
            line,
            owner,
            "Pause enabled Rules",
            Command::Pause(true),
        );
    }
    crate::castle_feed::button(world, block, owner, "Refresh automation", Command::Refresh);
}

pub(super) fn target_action(
    world: &World,
    owner: Entity,
    transfer: &Value,
    person: &str,
    level: u8,
) -> Option<Value> {
    let state = world.get::<Automation>(owner)?;
    if state.transfer != text(transfer, "uid") || state.person != person {
        return None;
    }
    let snapshot: nucleus::transfer::karma::Snapshot =
        serde_json::from_value(state.data.as_ref()?["state"].clone()).ok()?;
    if transfer["revision"].as_u64() != Some(snapshot.revision) {
        return None;
    }
    let participant = snapshot.participants.get(person)?;
    let mut guard = nucleus::transfer::karma::Guard::base(&snapshot);
    guard
        .participants
        .insert(person.into(), participant.clone());
    Some(
        json!({"action":"assign-transfer-agreement-level","transfer":snapshot.transfer,"person":person,"expected_revision":snapshot.revision,"request_id":nucleus::new_uid("agreement-target"),"level":level,"expected":participant.guard,"expected_state":guard}),
    )
}
