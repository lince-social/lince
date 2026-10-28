use super::*;
use cell::{ClientMessage, ServerMessage};

fn send(world: &World, owner: Entity, message: ClientMessage) -> Result<(), String> {
    if crate::native::laboratory::blocked(world, owner) {
        return Err("Transfers are read-only in the Laboratory".into());
    }
    crate::native::practice_cells::send(world, owner, message)
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
        owner,
        ClientMessage::Subscribe {
            id: id.clone(),
            protein,
        },
    )?;
    let sender =
        crate::native::practice_cells::sender(world, owner).ok_or("The Cell is unavailable")?;
    world
        .resource_mut::<Requests>()
        .1
        .insert(id.clone(), sender);
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
        if let Some(sender) = world.resource_mut::<Requests>().1.remove(&id) {
            let _ = sender.try_send(ClientMessage::Unsubscribe { id: id.clone() });
        }
        world.resource_mut::<Requests>().0.remove(&id);
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
        if let Some(sender) = world.resource_mut::<Requests>().1.remove(&id) {
            let _ = sender.try_send(ClientMessage::Unsubscribe { id: id.clone() });
        }
        world.resource_mut::<Requests>().0.remove(&id);
    }
    let owners: Vec<_> = world
        .query_filtered::<Entity, With<TransferCastle>>()
        .iter(world)
        .collect();
    for owner in owners {
        if crate::native::laboratory::suspended(world, owner) {
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
        karma::maintain(world, owner);
        audience::maintain(world, owner);
    }
}

pub(super) fn submit(world: &mut World, owner: Entity) {
    ui::capture(world, owner);
    let view = world.get::<View>(owner).unwrap();
    if !view.ready || view.pending.is_some() || crate::native::laboratory::suspended(world, owner) {
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
        owner,
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

pub(super) fn stop_location(world: &mut World, owner: Entity, record: String, person: String) {
    if world.get::<View>(owner).unwrap().pending.is_some() {
        status(world, owner, "Wait for the current action to finish");
        return;
    }
    let action = engine::actions::Action::Location {
        request: nucleus::location::Command::Stop {
            person: person.clone(),
            record_uid: record,
        },
    };
    let id = nucleus::new_uid("transfer-action");
    let mut form = Form::action("Stop my sharing", json!({"action":"location"}));
    form.actor = Some(person);
    match send(
        world,
        owner,
        ClientMessage::Act {
            id: id.clone(),
            action,
        },
    ) {
        Ok(()) => {
            world.get_mut::<View>(owner).unwrap().pending = Some(Pending { id, form });
            status(world, owner, "Stopping my location sharing…");
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
        if audience::receive(world, &message) || karma::receive(world, &message) {
            continue;
        }
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
                        let viewer_changed = context.as_ref().map(|context| &context["viewer"])
                            != Some(&view.context["viewer"]);
                        if viewer_changed {
                            view.receipt = None;
                            view.receipt_list = Value::Null;
                            view.receipt_export = None;
                            view.receipt_file_job = None;
                            if view.pending.as_ref().is_some_and(|pending| {
                                matches!(
                                    pending.form.data["action"].as_str(),
                                    Some("transfer-receipt" | "identity")
                                )
                            }) {
                                view.pending = None;
                            }
                        }
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
                        if viewer_changed {
                            super::receipts::clear_form(world, owner);
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
                data,
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
                let action = if pending.form.step.is_some() {
                    pending.form.mode.as_str()
                } else {
                    pending.form.data["action"].as_str().unwrap_or_default()
                };

                let mut castle = world.get_mut::<TransferCastle>(owner).unwrap();
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
                if action == "transfer-receipt" {
                    let mut view = world.get_mut::<View>(owner).unwrap();
                    view.receipt_export = data
                        .as_ref()
                        .and_then(|data| data["receipt_export"].as_str())
                        .map(str::to_owned);
                    if let Some(receipt) = data.as_ref().and_then(|data| data.get("receipt")) {
                        view.receipt = Some(receipt.clone());
                        view.receipt_page = 0;
                    }
                    if let Some(listing) = data.as_ref().and_then(|data| data.get("receipt_list")) {
                        view.receipt_list = listing.clone();
                    }
                }
                if action == "identity"
                    && let Some(identity) = data.as_ref().and_then(|data| data.get("identity"))
                {
                    let mut view = world.get_mut::<View>(owner).unwrap();
                    view.receipt = Some(
                        json!({"state":"identity","document":{"kind":"identity","value":identity}}),
                    );
                    view.receipt_export = None;
                    view.receipt_page = 0;
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

                if let Some(data) = data.as_ref().map(|data| data.get("trust").unwrap_or(data))
                    && data["kind"] == "supplier_candidates"
                    && pending.form.actor.as_deref() == Some(person(world, owner).as_str())
                {
                    let view = world.get::<View>(owner).unwrap();
                    let matches = SupplierMatches {
                        person: person(world, owner),
                        transfer: world.get::<TransferCastle>(owner).unwrap().selected.clone(),
                        rows: view.rows.clone(),
                        context: view.context.clone(),
                        data: data.clone(),
                    };
                    world.entity_mut(owner).insert(matches);
                }
                forms::render(world, owner);
                ui::render(world, owner);
                receipt_files::complete(world, owner, &pending.form, data.as_ref());
            }
            ServerMessage::Error { id, message, code } => {
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
                            || (id == crate::native::cell_bridge::CONNECTION
                                && crate::native::practice_cells::source(world, *owner).is_none())
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
                    let receipt_failure = view.pending.as_ref().is_some_and(|pending| {
                        pending.id == id
                            && matches!(
                                pending.form.data["action"].as_str(),
                                Some("transfer-receipt" | "identity")
                            )
                    });
                    if receipt_failure {
                        view.receipt = None;
                        view.receipt_list = Value::Null;
                    }
                    if view
                        .pending
                        .as_ref()
                        .is_some_and(|pending| pending.id == id)
                        || id == crate::native::cell_bridge::CONNECTION
                    {
                        view.pending = None;
                    }
                    view.receipt_export = None;
                    if lane.as_deref() == Some("transfers")
                        || id == crate::native::cell_bridge::CONNECTION
                    {
                        view.ready = false;
                    }
                    if lane.as_deref() == Some("preview") {
                        view.preview = None;
                    }
                    let message = if receipt_failure {
                        receipts::verification_message(code.as_deref(), &message)
                    } else {
                        message.clone()
                    };
                    status(world, owner, &message);
                    if receipt_failure {
                        ui::render(world, owner);
                    }
                }
            }
            _ => {}
        }
    }
}
