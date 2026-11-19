use super::*;

pub(super) enum Message {
    State {
        snapshot: String,
        writable: Vec<String>,
    },
    Change(String),
    Cursors(Vec<cell::CollabCursor>),
    Acknowledged(String),
    Error {
        connection: bool,
        message: String,
    },
}

enum Route<'a> {
    Authenticated(&'a str),
    Disconnected,
    Record { uid: &'a str, join: Option<&'a str> },
    Reply { id: &'a str, acknowledged: bool },
}

fn route(message: &ServerMessage) -> Option<Route<'_>> {
    Some(match message {
        ServerMessage::SessionAuthenticated { person, .. } => Route::Authenticated(person),
        ServerMessage::Error { id, .. } if id == "connection" => Route::Disconnected,
        ServerMessage::CollabState { id, record_uid, .. } => Route::Record {
            uid: record_uid,
            join: Some(id),
        },
        ServerMessage::CollabChange { record_uid, .. }
        | ServerMessage::CollabCursors { record_uid, .. } => Route::Record {
            uid: record_uid,
            join: None,
        },
        ServerMessage::ActionOk { id, .. } => Route::Reply {
            id,
            acknowledged: true,
        },
        ServerMessage::Error { id, .. } => Route::Reply {
            id,
            acknowledged: false,
        },
        _ => return None,
    })
}

pub(super) fn relevant(message: &ServerMessage) -> bool {
    route(message).is_some()
}

fn deliver(document: &mut Document, message: &ServerMessage) {
    let message = match message {
        ServerMessage::CollabState {
            snapshot_base64,
            writable,
            ..
        } => Message::State {
            snapshot: snapshot_base64.clone(),
            writable: writable.clone(),
        },
        ServerMessage::CollabChange { update_base64, .. } => Message::Change(update_base64.clone()),
        ServerMessage::CollabCursors { cursors, .. } => Message::Cursors(cursors.clone()),
        ServerMessage::ActionOk { id, .. } => Message::Acknowledged(id.clone()),
        ServerMessage::Error { id, message, .. } => Message::Error {
            connection: id == crate::native::cell_bridge::CONNECTION,
            message: message.clone(),
        },
        _ => return,
    };
    document.incoming.push_back(message);
    performance::count(Count::RecordBindingMessagesRouted);
}

pub(super) fn receive(world: &mut World, source: Source, message: &ServerMessage) {
    let Some(route) = route(message) else {
        return;
    };
    let _incoming = performance::Span::new(Stage::RecordBindingIncoming);
    let Some(mut bindings) = world.get_resource_mut::<Bindings>() else {
        return;
    };
    match route {
        Route::Authenticated(person) => {
            bindings
                .principals
                .insert(source.clone(), person.to_owned());
            for ((origin, _), document) in &mut bindings.documents {
                if origin == &source && document.principal.as_deref() != Some(person) {
                    fail(
                        document,
                        "The login changed. Pending edits belong to the previous login",
                    );
                    document.sender = None;
                    document.joined = false;
                }
            }
        }
        Route::Disconnected => {
            for ((origin, _), document) in &mut bindings.documents {
                if origin == &source {
                    document.sender = None;
                    document.joined = false;
                    document.joining = None;
                    document.reset_dispatch();
                    document.status = "Saved locally · waiting for connection…".into();
                }
            }
        }
        Route::Record { uid, join } => {
            let Some(document) = bindings
                .documents
                .get_mut(&identity::key_parts(&source, uid))
            else {
                return;
            };
            if join.is_none_or(|id| {
                document
                    .recovery_join
                    .as_ref()
                    .is_none_or(|expected| expected == id)
            }) {
                deliver(document, message);
            }
        }
        Route::Reply { id, acknowledged } => {
            for ((origin, uid), document) in &mut bindings.documents {
                if origin != &source {
                    continue;
                }
                performance::count(Count::RecordBindingReplyDocuments);
                let applies = document
                    .pending
                    .iter()
                    .take(document.inflight)
                    .any(|pending| pending.id == id)
                    || document.recovery_join.as_ref().map_or_else(
                        || id.strip_prefix("binding-join-") == Some(uid.as_str()),
                        |expected| id == expected,
                    )
                    || id == crate::native::cell_bridge::CONNECTION;
                if applies {
                    if acknowledged {
                        nucleus::performance::Operation::new(id).mark("record-edit-ack-routed");
                    }
                    deliver(document, message);
                }
            }
        }
    }
}
