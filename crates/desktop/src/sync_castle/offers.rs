use crate::{actions::Action, sand_panel as panel};
use bevy::prelude::*;
use engine::actions::Action as Backend;
use serde_json::Value;

#[derive(Component)]
struct View {
    record: Entity,
    peer: Entity,
    status: Entity,
    preview: Entity,
    list: Entity,
    prepared: Option<(String, String, String)>,
}
#[derive(Component)]
struct Job(tokio::sync::oneshot::Receiver<Result<(Backend, Option<Value>), String>>);
#[derive(Component)]
struct Watch {
    receiver: tokio::sync::watch::Receiver<Option<Result<Value, String>>>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Watch {
    fn drop(&mut self) {
        self.task.abort();
    }
}
#[derive(Clone)]
enum Command {
    Preview,
    Send,
    Run(Backend),
    Blob(String, bool),
}

pub(super) fn populate(world: &mut World, parent: Entity) {
    let owner = panel::column(world, parent);
    label(world, owner, "Move data and pending offers");
    label(
        world,
        owner,
        "Preview every Record linked by Assertions and every Karma dependency. Record identities, definitions and origin evidence travel together. The destination must have room for these identities; existing Records are preserved and conflicting moves stop. Ontology definitions stay available at the source. Account, executable, Place, Transfer and replica data use their existing workflows.",
    );
    label(
        world,
        owner,
        "Moves use a direct connection and are limited to 128 Records / 8 MiB, with at most 2,048 previewed dependencies. Definitions, their saved revisions and Record history travel together. The source stays until explicit acceptance and durable receipt. Cancellation closes when accepted delivery starts. Karma arrives paused; execution history, running work and local grants stay here.",
    );
    let record = panel::field(world, owner, "Record slug or UID to move", "");
    let peer = panel::field(world, owner, "Destination contact slug or UID", "");
    let controls = panel::row(world, owner);
    panel::button(
        world,
        controls,
        owner,
        "Preview complete move",
        Command::Preview,
    );
    panel::button(
        world,
        controls,
        owner,
        "Offer previewed move",
        Command::Send,
    );
    let status = label(world, owner, "Loading pending offers…");
    let preview = panel::column(world, owner);
    let list = panel::column(world, owner);
    world.entity_mut(owner).insert(View {
        record,
        peer,
        status,
        preview,
        list,
        prepared: None,
    });
}
fn label(world: &mut World, parent: Entity, text: &str) -> Entity {
    crate::edit_mode::label(world, parent, text, 13.0)
}

impl Action for Command {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(view) = world.get::<View>(owner) else {
            return;
        };
        let status = view.status;
        if crate::laboratory::active(world) {
            panel::status(world, status, "Offers are unavailable in the Laboratory");
            return;
        }
        if world.get::<Job>(owner).is_some() {
            return;
        }
        let action = match self {
            Self::Run(action) => Ok(action.clone()),
            Self::Blob(_, _) => Ok(Backend::PendingOffers),
            Self::Preview => panel::value(world, view.record).and_then(|record| {
                panel::value(world, view.peer)
                    .map(|target| Backend::PreviewRecordMove { record, target })
            }),
            Self::Send => view
                .prepared
                .as_ref()
                .ok_or_else(|| "Preview the move first".to_owned())
                .and_then(|(record, target, hash)| {
                    if panel::value(world, view.record)?.trim() != record
                        || panel::value(world, view.peer)?.trim() != target
                    {
                        return Err("The selection changed; preview it again".into());
                    }
                    Ok(Backend::MoveRecordTo {
                        record: record.clone(),
                        target: target.clone(),
                        expected_preview: Some(hash.clone()),
                    })
                }),
        };
        let action = match action {
            Ok(action) => action,
            Err(error) => {
                panel::status(world, status, error);
                return;
            }
        };
        let Some(runtime) = crate::practice_cells::runtime(world, owner) else {
            panel::status(world, status, "Local Cell unavailable");
            return;
        };
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            panel::status(world, status, "Runtime unavailable");
            return;
        };
        let wake = world.get_resource::<crate::wake::WakeSignal>().cloned();
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let command = self.clone();
        handle.spawn(async move {
            let result = match command {
                Command::Blob(id, true) => {
                    match tokio::task::spawn_blocking(|| {
                        rfd::FileDialog::new()
                            .set_title("Choose where to receive the copy")
                            .pick_folder()
                    })
                    .await
                    {
                        Ok(Some(folder)) => runtime
                            .engine
                            .accept_blob_copy(&id, &folder)
                            .await
                            .map(|_| (action, None))
                            .map_err(|e| e.to_string()),
                        _ => Err("Choose a destination folder to accept the copy".into()),
                    }
                }
                Command::Blob(id, false) => runtime
                    .engine
                    .stop_blob_copy(&id)
                    .await
                    .map(|_| (action, None))
                    .map_err(|e| e.to_string()),
                _ => runtime
                    .engine
                    .act(action.clone(), None)
                    .await
                    .map(|o| (action, o.data))
                    .map_err(|e| e.to_string()),
            };
            let _ = sender.send(result);
            if let Some(wake) = wake {
                wake.ring();
            }
        });
        world.entity_mut(owner).insert(Job(receiver));
        panel::status(world, status, "Working…");
    }
}

fn start(world: &mut World, owner: Entity) {
    if crate::laboratory::active(world) {
        return;
    }
    let Some(runtime) = crate::practice_cells::runtime(world, owner) else {
        return;
    };
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        return;
    };
    let wake = world.get_resource::<crate::wake::WakeSignal>().cloned();
    let (sender, receiver) = tokio::sync::watch::channel(None);
    let task=handle.spawn(async move {
        let mut changed=runtime.engine.watch_query_changes();
        let mut timer=tokio::time::interval(std::time::Duration::from_secs(5));
        loop {
            tokio::select! {_=timer.tick()=>{},result=changed.changed()=>{if result.is_err(){break;}}}
            let result=runtime.engine.act(Backend::PendingOffers,None).await.map(|o|o.data.unwrap_or(Value::Null)).map_err(|e|e.to_string());
            if sender.send(Some(result)).is_err(){break;}
            if let Some(wake)=&wake {wake.ring();}
        }
    });
    world.entity_mut(owner).insert(Watch { receiver, task });
}

pub(super) fn update(world: &mut World) {
    let owners: Vec<_> = world
        .query_filtered::<Entity, With<View>>()
        .iter(world)
        .collect();
    for owner in owners {
        if world.get::<Watch>(owner).is_none() {
            start(world, owner);
        }
        let result = world
            .get_mut::<Job>(owner)
            .and_then(|mut j| match j.0.try_recv() {
                Ok(v) => Some(v),
                Err(tokio::sync::oneshot::error::TryRecvError::Empty) => None,
                Err(_) => Some(Err(
                    "Offer request interrupted; inspect the local outcome before retrying".into(),
                )),
            });
        if let Some(result) = result {
            world.entity_mut(owner).remove::<Job>();
            let view = world.get::<View>(owner).unwrap();
            let status = view.status;
            let parent = view.preview;
            match result {
                Ok((Backend::PreviewRecordMove { record, target }, Some(preview))) => {
                    let hash = preview["hash"].as_str().unwrap_or("").to_owned();
                    world.get_mut::<View>(owner).unwrap().prepared =
                        Some((record.trim().into(), target.trim().into(), hash));
                    panel::clear(world, parent);
                    label(
                        world,
                        parent,
                        &format!(
                            "{} Record(s) · {} Assertion(s) · {} Karma rule(s) · {} bytes",
                            preview["records"].as_array().map_or(0, Vec::len),
                            preview["assertions"],
                            preview["karma_rules"],
                            preview["bytes"]
                        ),
                    );
                    for item in preview["records"].as_array().into_iter().flatten() {
                        label(
                            world,
                            parent,
                            &format!(
                                "{} · {} · {}",
                                item["title"].as_str().unwrap_or("Record"),
                                item["kind"].as_str().unwrap_or(""),
                                item["uid"].as_str().unwrap_or("")
                            ),
                        );
                    }
                    show_dependencies(world, parent, &preview);
                    panel::status(
                        world,
                        status,
                        "Preview ready. Review the complete set, then offer it.",
                    );
                }
                Ok((Backend::MoveRecordTo { .. }, _)) => {
                    world.get_mut::<View>(owner).unwrap().prepared = None;
                    panel::status(
                        world,
                        status,
                        "Offer saved locally; awaiting explicit acceptance",
                    );
                }
                Ok(_) => panel::status(world, status, "Local outcome saved"),
                Err(error) => panel::status(world, status, error),
            }
        }
        let snapshot = world.get_mut::<Watch>(owner).and_then(|mut w| {
            if w.receiver.has_changed().unwrap_or(false) {
                w.receiver.borrow_and_update().clone()
            } else {
                None
            }
        });
        if let Some(result) = snapshot {
            match result {
                Ok(snapshot) => render(world, owner, &snapshot),
                Err(error) => {
                    let status = world.get::<View>(owner).unwrap().status;
                    panel::status(world, status, error);
                }
            }
        }
    }
}

fn run(world: &mut World, parent: Entity, owner: Entity, caption: &str, action: Backend) {
    panel::button(world, parent, owner, caption, Command::Run(action));
}
fn strval(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or("").into()
}
fn render(world: &mut World, owner: Entity, snapshot: &Value) {
    let parent = world.get::<View>(owner).unwrap().list;
    panel::clear(world, parent);
    label(
        world,
        parent,
        "Pending offers · declined invitations remain private where required",
    );
    for offer in snapshot["pending"].as_array().into_iter().flatten() {
        let uid = strval(offer, "subject_uid");
        let peer = strval(offer, "other_party");
        let kind = strval(offer, "kind");
        label(
            world,
            parent,
            &format!(
                "{} · {} · {} · {}",
                strval(offer, "title"),
                kind,
                strval(offer, "direction"),
                peer
            ),
        );
        match kind.as_str() {
            "thread-invite" => {
                run(
                    world,
                    parent,
                    owner,
                    "Accept invitation",
                    Backend::AcceptThreadInvite {
                        invite: uid.clone(),
                    },
                );
                run(
                    world,
                    parent,
                    owner,
                    "Decline privately",
                    Backend::DeclineThreadInvite { invite: uid },
                );
            }
            "replica-grant" => run(
                world,
                parent,
                owner,
                "Cancel replica offer",
                Backend::CancelReplicaOffer {
                    root: uid,
                    target: peer,
                },
            ),
            "record-move" => {
                if let Some(m) = snapshot["moves"]
                    .as_array()
                    .and_then(|ms| ms.iter().find(|m| m["uid"] == uid))
                {
                    show_move(world, parent, m);
                    if m["state"] == "offered" && m["direction"] == "incoming" {
                        run(
                            world,
                            parent,
                            owner,
                            "Accept move and complete set",
                            Backend::AnswerRecordMove {
                                offer: uid.clone(),
                                accept: true,
                            },
                        );
                        run(
                            world,
                            parent,
                            owner,
                            "Decline move privately",
                            Backend::AnswerRecordMove {
                                offer: uid,
                                accept: false,
                            },
                        );
                    } else if m["direction"] == "outgoing"
                        && matches!(m["state"].as_str(), Some("offered" | "changed"))
                    {
                        run(
                            world,
                            parent,
                            owner,
                            "Cancel move",
                            Backend::CancelRecordMove { record: uid },
                        );
                    }
                }
            }
            "transfer" => {
                if let Some(invite) = snapshot["transfers"]
                    .as_array()
                    .and_then(|ts| ts.iter().find(|t| t["uid"] == uid))
                {
                    run(
                        world,
                        parent,
                        owner,
                        "Accept Transfer invitation",
                        Backend::AcceptTransferInvitation {
                            invitation: uid.clone(),
                            expected_revision: invite["revision"].as_u64().unwrap_or(0),
                            request_id: nucleus::new_uid("request"),
                            transfer: Some(strval(invite, "transfer")),
                            person: Some(strval(invite, "person")),
                        },
                    );
                    run(
                        world,
                        parent,
                        owner,
                        "Reject Transfer invitation",
                        Backend::RejectTransferInvitation {
                            invitation: uid,
                            request_id: nucleus::new_uid("request"),
                            transfer: Some(strval(invite, "transfer")),
                            person: Some(strval(invite, "person")),
                        },
                    );
                }
            }
            "blob-sync" => {
                panel::button(
                    world,
                    parent,
                    owner,
                    "Accept file copy",
                    Command::Blob(uid.clone(), true),
                );
                panel::button(
                    world,
                    parent,
                    owner,
                    "Cancel file copy",
                    Command::Blob(uid, false),
                );
            }
            _ => {}
        }
    }
    label(world, parent, "Local outcomes");
    for m in snapshot["moves"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|m| {
            matches!(
                m["state"].as_str(),
                Some("received" | "complete" | "cancelled" | "declined")
            )
        })
    {
        show_move(world, parent, m);
    }
    for outcome in snapshot["outcomes"].as_array().into_iter().flatten() {
        label(
            world,
            parent,
            &format!(
                "{} · {} · {}",
                strval(outcome, "kind"),
                strval(outcome, "subject_uid"),
                strval(outcome, "outcome")
            ),
        );
    }
    for refusal in snapshot["refusals"].as_array().into_iter().flatten() {
        label(
            world,
            parent,
            &format!(
                "Declined here · {} · {} · {}",
                strval(refusal, "kind"),
                strval(refusal, "subject_uid"),
                strval(refusal, "at")
            ),
        );
    }
    for invite in snapshot["transfers"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|i| i["status"] != "pending")
    {
        label(
            world,
            parent,
            &format!(
                "Transfer invitation {} · {}",
                strval(invite, "uid"),
                strval(invite, "status")
            ),
        );
    }
    for grant in snapshot["replicas"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|g| g["state"] != "offered")
    {
        label(
            world,
            parent,
            &format!(
                "Replica {} · {}",
                strval(grant, "root"),
                strval(grant, "state")
            ),
        );
    }
}
fn show_move(world: &mut World, parent: Entity, m: &Value) {
    let status = match m["state"].as_str() {
        Some("offered") => "Offer saved locally; waiting for acceptance",
        Some("accepted") => "Accepted locally; waiting for source delivery",
        Some("transferring") => {
            "Accepted delivery started; cancellation closed; source retained until durable receipt"
        }
        Some("received") => "Durably received here; source completion may still be pending",
        Some("complete") => "Durable receipt confirmed; source removed locally",
        Some("cancelled") => "Cancelled here; source retained",
        Some("declined") => "Declined here; refusal private from sender",
        Some("changed") => "Source changed; retained locally; preview again",
        _ => "Waiting",
    };
    label(world, parent, status);
    if let Some(error) = m["error"].as_str() {
        label(world, parent, error);
    }
    let p = &m["preview"];
    label(
        world,
        parent,
        &format!(
            "{} Record(s) · {} Assertions · {} Karma rules · {} bytes",
            p["records"].as_array().map_or(0, Vec::len),
            p["assertions"],
            p["karma_rules"],
            p["bytes"]
        ),
    );
    for r in p["records"].as_array().into_iter().flatten() {
        label(
            world,
            parent,
            &format!("{} · {}", strval(r, "title"), strval(r, "uid")),
        );
    }
    show_dependencies(world, parent, p);
}

fn show_dependencies(world: &mut World, parent: Entity, preview: &Value) {
    for item in preview["dependencies"].as_array().into_iter().flatten() {
        label(
            world,
            parent,
            &format!(
                "{} · {} · {}",
                strval(item, "kind"),
                strval(item, "title"),
                strval(item, "uid")
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn text(world: &mut World) -> String {
        world
            .query::<&Text>()
            .iter(world)
            .map(|t| t.0.clone())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn one_list_shows_every_offer_action_and_private_local_outcomes() {
        let mut app = crate::sand_panel::tests::app();
        let root = app.world_mut().spawn_empty().id();
        populate(app.world_mut(), root);
        let owner = app
            .world_mut()
            .query_filtered::<Entity, With<View>>()
            .single(app.world())
            .unwrap();
        let snapshot = json!({
            "pending":[
                {"kind":"thread-invite","subject_uid":"invite","other_party":"peer","direction":"incoming","title":"Conversation"},
                {"kind":"replica-grant","subject_uid":"replica","other_party":"peer","direction":"outgoing","title":"Replica"},
                {"kind":"record-move","subject_uid":"move","other_party":"peer","direction":"incoming","title":"Move"},
                {"kind":"transfer","subject_uid":"transfer-invite","other_party":"person","direction":"incoming","title":"Transfer"},
                {"kind":"blob-sync","subject_uid":"copy","other_party":"peer","direction":"incoming","title":"File"}
            ],
            "moves":[{"uid":"move","state":"offered","direction":"incoming","preview":{"records":[{"uid":"record","title":"Record"}],"dependencies":[{"uid":"field","title":"@record","kind":"Karma field"}],"assertions":1,"karma_rules":1,"bytes":80}}],
            "transfers":[{"uid":"transfer-invite","status":"pending","person":"person","transfer":"transfer","revision":1}],
            "outcomes":[{"kind":"thread-invite","subject_uid":"earlier","outcome":"declined"}],
            "refusals":[{"kind":"record-move","subject_uid":"private","at":"today"}]
        });
        render(app.world_mut(), owner, &snapshot);
        let labels = text(app.world_mut());
        for label in [
            "Accept invitation",
            "Decline privately",
            "Cancel replica offer",
            "Accept move and complete set",
            "Decline move privately",
            "Accept Transfer invitation",
            "Reject Transfer invitation",
            "Accept file copy",
            "Cancel file copy",
            "Local outcomes",
            "Declined here",
            "Karma field · @record · field",
        ] {
            assert!(labels.contains(label), "Missing {label}");
        }
    }

    #[test]
    fn changing_the_destination_invalidates_the_prepared_send() {
        let mut app = crate::sand_panel::tests::app();
        let root = app.world_mut().spawn_empty().id();
        populate(app.world_mut(), root);
        let owner = app
            .world_mut()
            .query_filtered::<Entity, With<View>>()
            .single(app.world())
            .unwrap();
        let view = app.world().get::<View>(owner).unwrap();
        let record = view.record;
        let peer = view.peer;
        let status = view.status;
        app.world_mut()
            .get_mut::<bevy::text::EditableText>(record)
            .unwrap()
            .editor
            .set_text("record");
        app.world_mut()
            .get_mut::<bevy::text::EditableText>(peer)
            .unwrap()
            .editor
            .set_text("different-peer");
        app.world_mut().get_mut::<View>(owner).unwrap().prepared = Some((
            "record".into(),
            "original-peer".into(),
            "preview-hash".into(),
        ));
        Command::Send.apply(app.world_mut(), owner);
        assert!(
            app.world()
                .get::<Text>(status)
                .unwrap()
                .0
                .contains("selection changed")
        );
        assert!(app.world().get::<Job>(owner).is_none());
    }
}
