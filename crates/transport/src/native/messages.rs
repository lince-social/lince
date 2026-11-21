//! Native message streams keep the agent's generated text separate from human edits.

use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use engine::{
    actions::Action,
    collab::limits,
    record_change::{Mutation, Request},
};
use loro::{LoroDoc, UpdateOptions, VersionVector};
use nucleus::{MessageState, new_uid};
use serde::Deserialize;
use serde_json::{Value, json, to_vec};

use super::{Context, State, error, receipts::fingerprint, response};
use crate::{ClientMessage, ServerMessage};

const MAX_OPEN_STREAMS: usize = 8;
const MAX_TEXT_BYTES: usize = 64 * 1024;
const MESSAGE_EXTENSION: &str = "lince.message";
const WRITING: &str = "writing";

pub(super) struct Stream {
    doc: LoroDoc,
    accepted: VersionVector,
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

impl State {
    pub(super) async fn attach_message(
        &mut self,
        uid: &str,
        context: &Context,
    ) -> Result<(), String> {
        if self.messages.contains_key(uid) {
            return Ok(());
        }
        if self.messages.len() >= MAX_OPEN_STREAMS {
            return Err("Finish an open message before starting another.".into());
        }
        let metadata = self.extension(uid, MESSAGE_EXTENSION).await?;
        if metadata.get("author").and_then(Value::as_str) != Some(context.agent.as_str())
            || !is_writing(&metadata)
        {
            return Err("Only this agent's writing messages may be attached.".into());
        }
        let rows = self.query(json!({"protein":{"source":"record","where":[{"uid_eq":uid},{"relation":{"kind":"message-in","direction":"out","other":context.thread}}],"fields":["uid"],"limit":1}})).await?;
        if rows
            .get("rows")
            .and_then(Value::as_array)
            .is_none_or(Vec::is_empty)
        {
            return Err("The message belongs to another thread.".into());
        }
        let stream = self
            .join_stream(uid)
            .await?
            .ok_or("The message's collaborative stream could not open.")?;
        self.messages.insert(uid.into(), stream);
        Ok(())
    }

    async fn join_stream(&mut self, uid: &str) -> Result<Option<Stream>, String> {
        let joined = self
            .session
            .handle(ClientMessage::CollabJoin {
                id: new_uid("stream"),
                record_uid: uid.into(),
            })
            .await;
        self.session
            .handle(ClientMessage::CollabLeave {
                record_uid: uid.into(),
            })
            .await;
        let Some(ServerMessage::CollabState {
            snapshot_base64, ..
        }) = joined.first()
        else {
            return Ok(None);
        };
        let doc = LoroDoc::new();
        doc.import(&B64.decode(snapshot_base64).map_err(error)?)
            .map_err(error)?;
        Ok(Some(Stream {
            accepted: doc.oplog_vv(),
            doc,
        }))
    }

    pub(super) async fn message(
        &mut self,
        arguments: &Value,
        context: &Context,
    ) -> Result<Value, String> {
        let Arguments {
            request_id,
            operation,
            message_uid,
            text,
        } = Arguments::deserialize(arguments).map_err(error)?;
        if let Some(previous) = self.receipts.previous(&request_id, arguments)? {
            return Ok(previous);
        }
        if text.len() > MAX_TEXT_BYTES {
            return Err("A streamed message may contain at most 64 KiB of generated text.".into());
        }
        let mark = fingerprint(arguments)?;
        let result = match (operation, message_uid.as_deref()) {
            (Operation::Start, Some(_)) => Err("Start does not accept a message UID.".into()),
            (Operation::Start, None) => self.start_message(&request_id, text, context).await,
            (_, None) => Err("Supply the message_uid returned by start.".into()),
            (operation, Some(uid)) => self.write_message(&request_id, uid, &text, operation).await,
        };
        self.receipts.remember(request_id, mark, result.clone());
        result
    }

    async fn start_message(
        &mut self,
        request_id: &str,
        text: String,
        context: &Context,
    ) -> Result<Value, String> {
        if self.messages.len() >= MAX_OPEN_STREAMS {
            return Err("Finish an open message before starting another.".into());
        }
        let result = response(
            self.session
                .handle(ClientMessage::Act {
                    id: request_id.into(),
                    action: Action::CreateMessage {
                        content: Vec::new(),
                        thread: context.thread.clone(),
                        body: text,
                        author: Some(context.agent.clone()),
                        state: MessageState::Writing,
                        parent: None,
                        references: Vec::new(),
                    },
                })
                .await,
        )?;
        let uid = result
            .get("created")
            .and_then(Value::as_str)
            .ok_or("Lince did not return the new message UID.")?;
        let Some(stream) = self.join_stream(uid).await? else {
            self.finish_message(uid, MessageState::Interrupted).await?;
            return Err(
                "The message was created but its collaborative stream could not open.".into(),
            );
        };
        self.messages.insert(uid.into(), stream);
        Ok(json!({"message_uid":uid,"state":WRITING}))
    }

    async fn write_message(
        &mut self,
        request_id: &str,
        uid: &str,
        text: &str,
        operation: Operation,
    ) -> Result<Value, String> {
        if !self.messages.contains_key(uid) {
            return Err("This connection does not own an open stream for this message.".into());
        }
        if !is_writing(&self.extension(uid, MESSAGE_EXTENSION).await?) {
            return Err("This message is no longer writing.".into());
        }
        let stream = self
            .messages
            .get_mut(uid)
            .ok_or("This connection does not own an open stream for this message.")?;
        let doc = &stream.doc;
        doc.get_text("body")
            .update(text, UpdateOptions::default())
            .map_err(error)?;
        doc.commit();
        let next = doc.oplog_vv();
        if stream.accepted != next {
            let delta = doc
                .export_json_updates_without_peer_compression(&stream.accepted, &next)
                .map_err(error)?;
            let bytes = to_vec(&delta).map_err(error)?;
            if bytes.len() > limits().delta_bytes {
                return Err("Message edit exceeded the collaborative operation limit.".into());
            }
            response(
                self.session
                    .handle(ClientMessage::Act {
                        id: request_id.into(),
                        action: Action::ChangeRecord {
                            request: Request {
                                id: new_uid("op"),
                                record_uid: uid.into(),
                                mutation: Mutation::Text {
                                    update_base64: B64.encode(bytes),
                                },
                            },
                        },
                    })
                    .await,
            )?;
            stream.accepted = next;
        }
        let state = match operation {
            Operation::Finish => MessageState::Finished,
            Operation::Interrupt => MessageState::Interrupted,
            Operation::Start | Operation::Update => MessageState::Writing,
        };
        if state != MessageState::Writing {
            self.finish_message(uid, state).await?;
            self.messages.remove(uid);
        }
        Ok(json!({"message_uid":uid,"state":state.as_str()}))
    }

    async fn finish_message(&mut self, uid: &str, state: MessageState) -> Result<(), String> {
        let mut record = self.record(uid).await?;
        let id = new_uid("message-state");
        let Some(Value::String(body)) = record
            .as_object_mut()
            .and_then(|record| record.remove("body"))
        else {
            return Err("Message body is unavailable.".into());
        };
        response(
            self.session
                .handle(ClientMessage::Act {
                    id,
                    action: Action::ReviseMessage {
                        message: uid.into(),
                        body,
                        state,
                    },
                })
                .await,
        )?;
        Ok(())
    }

    pub(super) async fn interrupt_messages(&mut self) -> Result<(), String> {
        let mut errors = Vec::new();
        // Keep each stream registered until the asynchronous close returns.
        while let Some((uid, _)) = self.messages.first_key_value() {
            let uid = uid.clone();
            if let Err(error) = self.finish_message(&uid, MessageState::Interrupted).await {
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

fn is_writing(metadata: &Value) -> bool {
    metadata.get("state").and_then(Value::as_str) == Some(WRITING)
}
