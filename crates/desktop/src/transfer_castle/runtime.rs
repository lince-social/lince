use super::*;
use cell::{ClientMessage, ServerMessage};

fn send(world: &World, message: ClientMessage) -> Result<(), String> {
    if crate::laboratory::active(world) {
        return Err("Transfers are read-only in the Laboratory".into());
    }
    world
        .get_non_send::<CellBridge>()
        .ok_or("No Cell connection")?
        .outgoing
        .try_send(message)
        .map_err(|error| format!("Could not reach the Cell: {error}"))
}

pub(super) fn subscribe(
    world: &mut World,
    owner: Entity,
    kind: &str,
    query: Value,
) -> Result<String, String> {
    let protein = serde_json::from_value(query)
        .map_err(|error| format!("Invalid transfer query: {error}"))?;
    let id = nucleus::new_uid("transfer-view");
    send(
        world,
        ClientMessage::Subscribe {
            id: id.clone(),
            protein,
        },
    )?;
    world
        .resource_mut::<Requests>()
        .0
        .insert(id.clone(), (owner, kind.into()));
    Ok(id)
}

pub(super) fn cancel(world: &mut World, owner: Entity, kind: &str) {
    let ids: Vec<_> = world
        .resource::<Requests>()
        .0
        .iter()
        .filter(|(_, (target, lane))| *target == owner && lane == kind)
        .map(|(id, _)| id.clone())
        .collect();
    for id in ids {
        if send(world, ClientMessage::Unsubscribe { id: id.clone() }).is_ok() {
            world.resource_mut::<Requests>().0.remove(&id);
        }
    }
}

pub(super) fn refresh(world: &mut World, owner: Entity) {
    cancel(world, owner, "transfers");
    if let Some(mut view) = world.get_mut::<View>(owner) {
        view.ready = false;
    }
    status(world, owner, "Refreshing transfers…");
}

pub(super) fn maintain(world: &mut World) {
    let stale: Vec<_> = world
        .resource::<Requests>()
        .0
        .iter()
        .filter(|(_, (owner, _))| world.get::<TransferCastle>(*owner).is_none())
        .map(|(id, _)| id.clone())
        .collect();
    for id in stale {
        if send(world, ClientMessage::Unsubscribe { id: id.clone() }).is_ok() {
            world.resource_mut::<Requests>().0.remove(&id);
        }
    }
    let owners: Vec<_> = world
        .query_filtered::<Entity, With<TransferCastle>>()
        .iter(world)
        .collect();
    for owner in owners {
        if crate::laboratory::suspended(world, owner) {
            continue;
        }
        for (kind, source) in [
            ("transfers", "transfer"),
            ("records", "record"),
            ("units", "concept"),
        ] {
            if world
                .resource::<Requests>()
                .0
                .values()
                .any(|(target, lane)| *target == owner && lane == kind)
            {
                continue;
            }
            if let Err(error) = subscribe(world, owner, kind, json!({"source":source})) {
                status(world, owner, error);
                break;
            }
        }
    }
}

pub(super) fn submit(world: &mut World, owner: Entity) {
    ui::capture(world, owner);
    let view = world.get::<View>(owner).unwrap();
    if !view.ready || view.pending.is_some() || crate::laboratory::suspended(world, owner) {
        status(
            world,
            owner,
            "Wait for a fresh Cell snapshot before submitting",
        );
        return;
    }
    let Some(mut form) = world.get::<TransferCastle>(owner).unwrap().form.clone() else {
        return;
    };
    let action = form.commit_fields().and_then(|()| form.payload()).and_then(|payload| {
        if let Some(uid) = &form.transfer {
            let current = world.get::<View>(owner).unwrap().rows.iter().find(|row| text(row, "uid") == *uid);
            if current.is_none_or(|row| row["revision"].as_u64() != form.revision) {
                return Err("These terms changed. Keep this draft open to copy your changes, or cancel and reopen the current revision".into());
            }
        }
        serde_json::from_value::<engine::actions::Action>(payload).map_err(|error| format!("Check the form: {error}"))
    });
    let action = match action {
        Ok(action) => action,
        Err(error) => {
            status(world, owner, error);
            return;
        }
    };
    let id = nucleus::new_uid("transfer-action");
    match send(
        world,
        ClientMessage::Act {
            id: id.clone(),
            action,
        },
    ) {
        Ok(()) => {
            world.get_mut::<TransferCastle>(owner).unwrap().form = Some(form.clone());
            world.get_mut::<View>(owner).unwrap().pending = Some(Pending { id, form });
            status(world, owner, "Submitting…");
        }
        Err(error) => status(world, owner, error),
    }
}

pub(super) fn receive(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<CellMessage>>,
) {
    let messages: Vec<_> = cursor
        .read(world.resource::<Messages<CellMessage>>())
        .map(|message| message.0.clone())
        .collect();
    for message in messages {
        match message {
            ServerMessage::Snapshot { id, rows } | ServerMessage::Update { id, rows } => {
                let Some((owner, kind)) = world.resource::<Requests>().0.get(&id).cloned() else {
                    continue;
                };
                if world.get::<View>(owner).is_none() {
                    continue;
                }
                ui::capture(world, owner);
                match kind.as_str() {
                    "transfers" => {
                        let context = rows
                            .iter()
                            .find(|row| row["kind"] == "transfer_context")
                            .cloned();
                        let fresh: Vec<_> = rows
                            .into_iter()
                            .filter(|row| row["kind"] != "transfer_context")
                            .collect();
                        let view = world.get::<View>(owner).unwrap();
                        if view.ready
                            && view.rows == fresh
                            && context.as_ref() == Some(&view.context)
                        {
                            continue;
                        }
                        let mut view = world.get_mut::<View>(owner).unwrap();
                        view.ready = context.is_some();
                        view.context = context.unwrap_or(Value::Null);
                        view.rows = fresh;
                        if !view.ready {
                            status(
                                world,
                                owner,
                                "The Cell did not include a transfer viewer context",
                            );
                        } else if view.pending.is_none() {
                            let message = view
                                .notice
                                .clone()
                                .unwrap_or_else(|| "Connected · live transfers".into());
                            status(world, owner, message);
                        }
                        ui::render(world, owner);
                    }
                    "records" | "units" => {
                        let mut view = world.get_mut::<View>(owner).unwrap();
                        if kind == "records" {
                            view.records
                                .retain(|row| row["transfer_picker_unit"] == true);
                            view.records.extend(rows);
                        } else {
                            view.records
                                .retain(|row| row["transfer_picker_unit"] != true);
                            view.records.extend(rows.into_iter().map(|mut row| {
                                row["transfer_picker_unit"] = json!(true);
                                row
                            }));
                        }
                    }
                    "preview" => {
                        if world.get::<View>(owner).unwrap().preview_request.as_ref() != Some(&id) {
                            continue;
                        }
                        world.get_mut::<View>(owner).unwrap().preview = rows.into_iter().next();
                        forms::render_preview(world, owner);
                    }
                    _ => {}
                }
            }
            ServerMessage::ActionOk {
                id,
                created,
                warnings,
                ..
            } => {
                let owner = world
                    .query::<(Entity, &View)>()
                    .iter(world)
                    .find(|(_, view)| {
                        view.pending
                            .as_ref()
                            .is_some_and(|pending| pending.id == id)
                    })
                    .map(|(owner, _)| owner);
                let Some(owner) = owner else { continue };
                ui::capture(world, owner);
                let pending = world
                    .get_mut::<View>(owner)
                    .unwrap()
                    .pending
                    .take()
                    .unwrap();
                let mut castle = world.get_mut::<TransferCastle>(owner).unwrap();
                let action = if pending.form.step.is_some() {
                    pending.form.mode.as_str()
                } else {
                    pending.form.data["action"].as_str().unwrap_or_default()
                };
                if castle.form.as_ref() == Some(&pending.form) {
                    castle.form = None;
                } else if let Some(form) = &mut castle.form {
                    if action == "create-transfer-draft"
                        && let Some(uid) = &created
                    {
                        form.transfer = Some(uid.clone());
                        form.revision = Some(1);
                        form.mode = "revise-transfer-draft".into();
                        form.title = "Edit transfer".into();
                    } else if action == "revise-transfer-draft" || action == "counteroffer-transfer"
                    {
                        form.revision = form.revision.map(|revision| revision + 1);
                    }
                }
                if matches!(
                    action,
                    "create-transfer-draft"
                        | "create-transfer-remainder-draft"
                        | "create-reversing-transfer-draft"
                ) && let Some(created) = created
                {
                    castle.selected = created;
                }
                cancel(world, owner, "preview");
                refresh(world, owner);
                if !warnings.is_empty() {
                    let message = format!("Saved with warnings: {}", warnings.join(" · "));
                    world.get_mut::<View>(owner).unwrap().notice = Some(message.clone());
                    status(world, owner, message);
                } else {
                    world.get_mut::<View>(owner).unwrap().notice = None;
                }
                forms::render(world, owner);
                ui::render(world, owner);
            }
            ServerMessage::Error { id, message, .. } => {
                let owners: Vec<_> = world
                    .query::<(Entity, &View)>()
                    .iter(world)
                    .filter(|(owner, view)| {
                        view.pending
                            .as_ref()
                            .is_some_and(|pending| pending.id == id)
                            || world
                                .resource::<Requests>()
                                .0
                                .get(&id)
                                .is_some_and(|(target, _)| target == owner)
                            || id == crate::cell_bridge::CONNECTION
                    })
                    .map(|(owner, _)| owner)
                    .collect();
                for owner in owners {
                    let lane = world
                        .resource::<Requests>()
                        .0
                        .get(&id)
                        .map(|(_, kind)| kind.clone());
                    let mut view = world.get_mut::<View>(owner).unwrap();
                    if view
                        .pending
                        .as_ref()
                        .is_some_and(|pending| pending.id == id)
                        || id == crate::cell_bridge::CONNECTION
                    {
                        view.pending = None;
                    }
                    if lane.as_deref() == Some("transfers") || id == crate::cell_bridge::CONNECTION
                    {
                        view.ready = false;
                    }
                    if lane.as_deref() == Some("preview") {
                        view.preview = None;
                    }
                    status(world, owner, &message);
                }
            }
            _ => {}
        }
    }
}
