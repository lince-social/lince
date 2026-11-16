use super::*;
use crate::host::performance::{self, Count, Span, Stage};
use crate::native::retained;
use std::{borrow::Cow, collections::HashMap};

#[derive(Default)]
pub(super) struct Rows(retained::Children<(Key, usize)>);

#[derive(Clone, PartialEq, Eq, Hash)]
enum Key {
    PendingHeading,
    OutcomesHeading,
    Pending(String, String, String, String),
    Move(String),
    Outcome(usize),
    Refusal(usize),
    Transfer(String),
    Replica(String, String),
}

#[derive(Clone, Copy, PartialEq)]
enum Row<'a> {
    Text(&'a str),
    Pending(&'a Value, Option<&'a Value>),
    Move(&'a Value),
    Outcome(&'a Value),
    Refusal(&'a Value),
    Transfer(&'a Value),
    Replica(&'a Value),
}

fn items(snapshot: &Value) -> Vec<((Key, usize), Row<'_>)> {
    // Pending controls also consume their move/invitation detail. Include that
    // detail in equality so changed revisions never retain stale actions.
    let mut moves = HashMap::new();
    let mut transfers = HashMap::new();
    for value in snapshot["moves"].as_array().into_iter().flatten() {
        if let Some(uid) = value["uid"].as_str() {
            moves.entry(uid).or_insert(value);
        }
    }
    for value in snapshot["transfers"].as_array().into_iter().flatten() {
        if let Some(uid) = value["uid"].as_str() {
            transfers.entry(uid).or_insert(value);
        }
    }
    let mut rows = vec![(
        Key::PendingHeading,
        Row::Text("Pending offers · declined invitations remain private where required"),
    )];
    for offer in snapshot["pending"].as_array().into_iter().flatten() {
        let uid = strval(offer, "subject_uid");
        let kind = strval(offer, "kind");
        let detail = match kind.as_str() {
            "record-move" => moves.get(uid.as_str()).copied(),
            "transfer" => transfers.get(uid.as_str()).copied(),
            _ => None,
        };
        rows.push((
            Key::Pending(
                kind,
                uid,
                strval(offer, "direction"),
                strval(offer, "other_party"),
            ),
            Row::Pending(offer, detail),
        ));
    }
    rows.push((Key::OutcomesHeading, Row::Text("Local outcomes")));
    for value in snapshot["moves"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|value| {
            matches!(
                value["state"].as_str(),
                Some("received" | "complete" | "cancelled" | "declined")
            )
        })
    {
        rows.push((Key::Move(strval(value, "uid")), Row::Move(value)));
    }
    for (index, value) in snapshot["outcomes"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        rows.push((Key::Outcome(index), Row::Outcome(value)));
    }
    for (index, value) in snapshot["refusals"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        rows.push((Key::Refusal(index), Row::Refusal(value)));
    }
    for value in snapshot["transfers"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|value| value["status"] != "pending")
    {
        rows.push((Key::Transfer(strval(value, "uid")), Row::Transfer(value)));
    }
    for value in snapshot["replicas"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|value| value["state"] != "offered")
    {
        rows.push((
            Key::Replica(strval(value, "root"), strval(value, "peer")),
            Row::Replica(value),
        ));
    }
    // Preserve repeated history/offers rather than treating a repeated display
    // identity as authorization to discard another row.
    let mut occurrences = HashMap::new();
    rows.into_iter()
        .map(|(key, row)| {
            let next = occurrences.entry(key.clone()).or_insert(0);
            let ordinal = *next;
            *next += 1;
            ((key, ordinal), row)
        })
        .collect()
}

impl Rows {
    pub(super) fn render(
        &mut self,
        world: &mut World,
        parent: Entity,
        owner: Entity,
        previous: Option<&Value>,
        snapshot: &Value,
    ) {
        let _span = Span::new(Stage::NativeSyncOffersPresentation);
        let previous: HashMap<_, _> = previous.into_iter().flat_map(items).collect();
        let rows = items(snapshot).into_iter().map(|(key, row)| {
            let unchanged = previous.get(&key) == Some(&row);
            (key, (row, unchanged))
        });
        let (mut created, mut retained, mut rebuilt) = (0, 0, 0);
        self.0.reconcile(
            world,
            parent,
            rows,
            |world, parent, &(row, _)| {
                created += 1;
                if let Some(text) = caption(row) {
                    return label(world, parent, text.as_ref());
                }
                let entity = panel::column(world, parent);
                fill(world, entity, owner, row);
                entity
            },
            |world, entity, &(row, unchanged)| {
                retained += 1;
                if unchanged {
                    return;
                }
                if let Some(text) = caption(row) {
                    panel::status(world, entity, text);
                } else {
                    rebuilt += 1;
                    panel::clear(world, entity);
                    fill(world, entity, owner, row);
                }
            },
        );
        performance::add(Count::SyncOfferRowsCreated, created);
        performance::add(Count::SyncOfferRowsRetained, retained);
        performance::add(Count::SyncOfferRowsRebuilt, rebuilt);
    }
}

fn caption(row: Row<'_>) -> Option<Cow<'_, str>> {
    Some(match row {
        Row::Text(text) => Cow::Borrowed(text),
        Row::Outcome(value) => Cow::Owned(format!(
            "{} · {} · {}",
            strval(value, "kind"),
            strval(value, "subject_uid"),
            strval(value, "outcome")
        )),
        Row::Refusal(value) => Cow::Owned(format!(
            "Declined here · {} · {} · {}",
            strval(value, "kind"),
            strval(value, "subject_uid"),
            strval(value, "at")
        )),
        Row::Transfer(value) => Cow::Owned(format!(
            "Transfer invitation {} · {}",
            strval(value, "uid"),
            strval(value, "status")
        )),
        Row::Replica(value) => Cow::Owned(format!(
            "Replica {} · {}",
            strval(value, "root"),
            strval(value, "state")
        )),
        Row::Pending(..) | Row::Move(_) => return None,
    })
}

fn fill(world: &mut World, parent: Entity, owner: Entity, row: Row<'_>) {
    match row {
        Row::Pending(offer, detail) => pending(world, parent, owner, offer, detail),
        Row::Move(value) => show_move(world, parent, value),
        other => {
            if let Some(text) = caption(other) {
                label(world, parent, text.as_ref());
            }
        }
    }
}

fn pending(
    world: &mut World,
    parent: Entity,
    owner: Entity,
    offer: &Value,
    detail: Option<&Value>,
) {
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
            if let Some(m) = detail {
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
            if let Some(invite) = detail {
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
