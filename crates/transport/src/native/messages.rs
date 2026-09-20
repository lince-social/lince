use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use engine::{
    actions::Action,
    record_change::{Mutation, Request},
};
use loro::LoroDoc;
use serde::Deserialize;
use serde_json::{Value, json};

use super::{Context, State, error, response};
use crate::{ClientMessage, ServerMessage};

pub(super) struct Stream {
    doc: LoroDoc,
    accepted: loro::VersionVector,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Operation {
    Start,
    Update,
    Finish,
    Interrupt,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Arguments {
    request_id: String,
    operation: Operation,
    message_uid: Option<String>,
    text: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Release {
    read_ids: Vec<String>,
}

impl State {
    pub(super) async fn attach_message(
        &mut self,
        uid: &str,
        context: &Context,
    ) -> Result<(), String> {
        if self.messages.contains_key(uid) {
            return Ok(());
        }
        if self.messages.len() >= 8 {
            return Err("Finish an open message before starting another.".into());
        }
        let metadata = self.extension(uid, "lince.message").await?;
        if metadata["author"] != context.agent || metadata["state"] != "writing" {
            return Err("Only this agent's writing messages may be attached.".into());
        }
        let rows = self.query(json!({"protein":{"source":"record","where":[{"uid_eq":uid},{"relation":{"kind":"message-in","direction":"out","other":context.thread}}],"fields":["uid"],"limit":1}})).await?;
        if rows["rows"].as_array().is_none_or(Vec::is_empty) {
            return Err("The message belongs to another thread.".into());
        }
        let joined = self
            .session
            .handle(ClientMessage::CollabJoin {
                id: nucleus::new_uid("stream"),
                record_uid: uid.into(),
            })
            .await;
        self.session
            .handle(ClientMessage::CollabLeave {
                record_uid: uid.into(),
            })
            .await;
        let doc = LoroDoc::new();
        match joined.first() {
            Some(ServerMessage::CollabState {
                snapshot_base64, ..
            }) => {
                doc.import(&B64.decode(snapshot_base64).map_err(error)?)
                    .map_err(error)?;
            }
            _ => {
                return Err("The message's collaborative stream could not open.".into());
            }
        }
        self.messages.insert(
            uid.into(),
            Stream {
                accepted: doc.oplog_vv(),
                doc,
            },
        );
        Ok(())
    }
    pub(super) fn release(&mut self, arguments: Value) -> Result<Value, String> {
        let args: Release = serde_json::from_value(arguments).map_err(error)?;
        if args.read_ids.len() > 32 {
            return Err("Release at most 32 read snapshots at a time.".into());
        }
        let mut released = 0;
        for id in args.read_ids {
            released += usize::from(self.reads.remove(&id).is_some());
        }
        Ok(json!({"released":released}))
    }

    pub(super) async fn message(
        &mut self,
        arguments: Value,
        context: &Context,
    ) -> Result<Value, String> {
        let args: Arguments = serde_json::from_value(arguments.clone()).map_err(error)?;
        if let Some(previous) = self.previous(&args.request_id, &arguments)? {
            return Ok(previous);
        }
        if args.text.len() > 65_536 {
            return Err("A streamed message may contain at most 64 KiB of generated text.".into());
        }
        let result = self.message_inner(&args, context).await;
        self.remember(args.request_id, &arguments, result.clone());
        result
    }

    async fn message_inner(
        &mut self,
        args: &Arguments,
        context: &Context,
    ) -> Result<Value, String> {
        if matches!(args.operation, Operation::Start) {
            if args.message_uid.is_some() {
                return Err("Start does not accept a message UID.".into());
            }
            if self.messages.len() >= 8 {
                return Err("Finish an open message before starting another.".into());
            }
            let result = response(
                self.session
                    .handle(ClientMessage::Act {
                        id: args.request_id.clone(),
                        action: Action::CreateMessage {
                            thread: context.thread.clone(),
                            body: args.text.clone(),
                            author: Some(context.agent.clone()),
                            state: nucleus::MessageState::Writing,
                            parent: None,
                            references: Vec::new(),
                        },
                    })
                    .await,
            )?;
            let uid = result["created"]
                .as_str()
                .ok_or("Lince did not return the new message UID.")?
                .to_string();
            let joined = self
                .session
                .handle(ClientMessage::CollabJoin {
                    id: nucleus::new_uid("stream"),
                    record_uid: uid.clone(),
                })
                .await;
            self.session
                .handle(ClientMessage::CollabLeave {
                    record_uid: uid.clone(),
                })
                .await;
            let doc = LoroDoc::new();
            match joined.first() {
                Some(ServerMessage::CollabState {
                    snapshot_base64, ..
                }) => {
                    doc.import(&B64.decode(snapshot_base64).map_err(error)?)
                        .map_err(error)?;
                }
                _ => {
                    self.finish_message(&uid, nucleus::MessageState::Interrupted)
                        .await?;
                    return Err(
                        "The message was created but its collaborative stream could not open."
                            .into(),
                    );
                }
            }
            self.messages.insert(
                uid.clone(),
                Stream {
                    accepted: doc.oplog_vv(),
                    doc,
                },
            );
            return Ok(json!({"message_uid":uid,"state":"writing"}));
        }
        let uid = args
            .message_uid
            .as_deref()
            .ok_or("Supply the message_uid returned by start.")?;
        if !self.messages.contains_key(uid) {
            return Err("This connection does not own an open stream for this message.".into());
        }
        if self.extension(uid, "lince.message").await?["state"] != "writing" {
            return Err("This message is no longer writing.".into());
        }
        let stream = self.messages.get_mut(uid).unwrap();
        let doc = &stream.doc;
        let version = stream.accepted.clone();
        doc.get_text("body")
            .update(&args.text, loro::UpdateOptions::default())
            .map_err(error)?;
        doc.commit();
        if version != doc.oplog_vv() {
            let delta = doc.export_json_updates_without_peer_compression(&version, &doc.oplog_vv());
            let bytes = serde_json::to_vec(&delta).map_err(error)?;
            if bytes.len() > engine::collab::limits().delta_bytes {
                return Err("Message edit exceeded the collaborative operation limit.".into());
            }
            response(
                self.session
                    .handle(ClientMessage::Act {
                        id: args.request_id.clone(),
                        action: Action::ChangeRecord {
                            request: Request {
                                id: nucleus::new_uid("op"),
                                record_uid: uid.into(),
                                mutation: Mutation::Text {
                                    update_base64: B64.encode(bytes),
                                },
                            },
                        },
                    })
                    .await,
            )?;
            stream.accepted = doc.oplog_vv();
        }
        let state = match args.operation {
            Operation::Finish => nucleus::MessageState::Finished,
            Operation::Interrupt => nucleus::MessageState::Interrupted,
            _ => nucleus::MessageState::Writing,
        };
        if state != nucleus::MessageState::Writing {
            self.finish_message(uid, state).await?;
            self.messages.remove(uid);
        }
        Ok(json!({"message_uid":uid,"state":state.as_str()}))
    }

    async fn finish_message(
        &mut self,
        uid: &str,
        state: nucleus::MessageState,
    ) -> Result<(), String> {
        let record = self.record(uid).await?;
        response(
            self.session
                .handle(ClientMessage::Act {
                    id: nucleus::new_uid("message-state"),
                    action: Action::ReviseMessage {
                        message: uid.into(),
                        body: record["body"]
                            .as_str()
                            .ok_or("Message body is unavailable.")?
                            .into(),
                        state,
                    },
                })
                .await,
        )?;
        Ok(())
    }

    pub(super) async fn interrupt_messages(&mut self) -> Result<(), String> {
        let messages: Vec<_> = self.messages.keys().cloned().collect();
        let mut errors = Vec::new();
        for uid in messages {
            if let Err(error) = self
                .finish_message(&uid, nucleus::MessageState::Interrupted)
                .await
            {
                errors.push(error);
            }
            self.messages.remove(&uid);
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }
}
