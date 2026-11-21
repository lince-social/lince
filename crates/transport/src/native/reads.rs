//! Fiote Record queries and connection-local collaborative read snapshots.

use std::{collections::BTreeMap, mem::take, sync::Arc};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use nucleus::{new_uid, valid_uid};
use protein::Protein;
use serde::Deserialize;
use serde_json::{Map, Value, from_value, json};

use super::{Context, State, bounded, error, json_argument, response};
use crate::ClientMessage;

pub(super) const MAX_READS: usize = 32;
const MAX_SNAPSHOT_BYTES: usize = 16 * 1024 * 1024;

pub(super) struct Read {
    pub(super) record: Arc<Value>,
    pub(super) snapshot: Vec<u8>,
    pub(super) writable: Vec<String>,
}

#[derive(Default)]
pub(super) struct Reads {
    entries: BTreeMap<String, Read>,
    snapshot_bytes: usize,
}

impl Reads {
    pub(super) fn get(&self, id: &str) -> Option<&Read> {
        self.entries.get(id)
    }

    fn insert(&mut self, id: String, read: Read) {
        self.snapshot_bytes += read.snapshot.len();
        if let Some(previous) = self.entries.insert(id, read) {
            self.snapshot_bytes -= previous.snapshot.len();
        }
    }

    fn release(&mut self, ids: Vec<String>) -> usize {
        let mut released = 0;
        for id in ids {
            if let Some(read) = self.entries.remove(&id) {
                self.snapshot_bytes -= read.snapshot.len();
                released += 1;
            }
        }
        released
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct QueryArguments {
    #[serde(default, deserialize_with = "json_argument")]
    protein: Option<Protein>,
    saved: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadArguments {
    record_uid: String,
    #[serde(default)]
    extensions: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReleaseReads {
    read_ids: Vec<String>,
}

impl State {
    pub(super) async fn instructions(
        &mut self,
        context: &Context,
        pinned: Option<&str>,
    ) -> Result<Value, String> {
        let mut value = self
            .extension(&context.thread, "lince.fiote-session")
            .await?;
        let missing = "No instructions are pinned to this Fiote session.";
        let object = value.as_object_mut().ok_or(missing)?;
        if object.get("fiote").and_then(Value::as_str) != Some(context.record.as_str()) {
            return Err(missing.into());
        }
        for source in object
            .get("sources")
            .and_then(Value::as_array)
            .ok_or("Missing instruction sources.")?
        {
            self.record(
                source
                    .get("record")
                    .and_then(Value::as_str)
                    .ok_or("Missing instruction Record.")?,
            )
            .await?;
        }
        let system = pinned.ok_or("Reconnect this session to restore its pinned instructions.")?;
        object.insert("system".into(), system.into());
        Ok(value)
    }

    pub(super) async fn query(&mut self, arguments: Value) -> Result<Value, String> {
        let args: QueryArguments = from_value(arguments).map_err(error)?;
        let protein = match (args.protein, args.saved) {
            (Some(protein), None) => protein,
            (None, Some(name)) => {
                let selector = if valid_uid(&name, "r") {
                    json!({"uid_eq":name})
                } else {
                    json!({"slug_eq":name})
                };
                let lookup = from_value(json!({
                    "source":"record","where":[selector,{"kind_eq":"protein"}],
                    "fields":["uid","extension"],"include":{"extension":{"namespace":"lince.protein"}},"limit":1
                }))
                .map_err(error)?;
                let result = self.subscribe(lookup).await?;
                let ast = first_row(result)
                    .map(take_extension)
                    .ok_or("The saved Protein is missing or is not visible to this session.")?;
                from_value(ast).map_err(error)?
            }
            _ => return Err("Supply either protein or saved.".into()),
        };
        self.subscribe(protein).await
    }

    async fn subscribe(&mut self, mut protein: Protein) -> Result<Value, String> {
        let id = new_uid("query");
        let limit = protein.limit.unwrap_or(50).min(100);
        protein.limit = Some(limit);
        let result = response(
            self.session
                .handle(ClientMessage::Subscribe {
                    id: id.clone(),
                    protein,
                })
                .await,
        );
        self.session.handle(ClientMessage::Unsubscribe { id }).await;
        let mut result = result?;
        let output = object_or_create(&mut result)?;
        let has_more = output
            .get("rows")
            .and_then(Value::as_array)
            .is_some_and(|rows| rows.len() == limit);
        output.insert("limit".into(), json!(limit));
        output.insert("may_have_more".into(), has_more.into());
        bounded(result)
    }

    pub(super) async fn record(&mut self, uid: &str) -> Result<Value, String> {
        if !valid_uid(uid, "r") {
            return Err("Use a stable Record UID from a Protein result.".into());
        }
        let result = self
            .query(json!({"protein":{"source":"record","where":[{"uid_eq":uid}],"limit":1}}))
            .await?;
        first_row(result)
            .ok_or_else(|| "The Record is missing or is not visible to this session.".into())
    }

    pub(super) async fn read(&mut self, arguments: Value) -> Result<Value, String> {
        let args: ReadArguments = from_value(arguments).map_err(error)?;
        if self.reads.entries.len() >= MAX_READS {
            return Err(
                "This turn has reached its 32 Record snapshots. Continue in another turn.".into(),
            );
        }
        if args.extensions.len() > 8
            || args
                .extensions
                .iter()
                .any(|name| name.is_empty() || name.len() > 128)
        {
            return Err("Read at most eight extension namespaces of 1–128 bytes.".into());
        }
        let mut record = self.record(&args.record_uid).await?;
        for namespace in args.extensions {
            let value = self.extension(&args.record_uid, &namespace).await?;
            insert_extension(&mut record, namespace, value)?;
        }
        let state = response(
            self.session
                .handle(ClientMessage::CollabJoin {
                    id: new_uid("read"),
                    record_uid: args.record_uid.clone(),
                })
                .await,
        );
        self.session
            .handle(ClientMessage::CollabLeave {
                record_uid: args.record_uid,
            })
            .await;
        let mut state = state?;
        let snapshot = STANDARD
            .decode(
                state
                    .get("snapshot_base64")
                    .and_then(Value::as_str)
                    .ok_or("Missing collaborative snapshot.")?,
            )
            .map_err(error)?;
        if snapshot.len() > MAX_SNAPSHOT_BYTES - self.reads.snapshot_bytes {
            return Err("Collaborative snapshots exceed this turn's memory limit.".into());
        }
        let writable =
            from_value(state.get_mut("writable").map_or(Value::Null, take)).map_err(error)?;
        let read_id = new_uid("read");
        let result = bounded(
            json!({"record":record,"read_id":read_id,"change_id":new_uid("op"),"text_version":state.get("version"),"writable_text":writable}),
        )?;
        self.reads.insert(
            read_id,
            Read {
                record: Arc::new(record),
                snapshot,
                writable,
            },
        );
        Ok(result)
    }

    pub(super) fn release(&mut self, arguments: Value) -> Result<Value, String> {
        let ReleaseReads { read_ids } = from_value(arguments).map_err(error)?;
        if read_ids.len() > MAX_READS {
            return Err("Release at most 32 read snapshots at a time.".into());
        }
        let released = self.reads.release(read_ids);
        Ok(json!({ "released": released }))
    }

    pub(super) async fn extension(&mut self, uid: &str, namespace: &str) -> Result<Value, String> {
        let result = self
            .query(json!({"protein":{
            "source":"record","where":[{"uid_eq":uid}],"fields":["uid","extension"],
                "include":{"extension":{"namespace":namespace}},"limit":1
            }}))
            .await?;
        first_row(result)
            .map(take_extension)
            .ok_or_else(|| "The Record is no longer visible.".into())
    }
}

fn first_row(mut result: Value) -> Option<Value> {
    result
        .get_mut("rows")?
        .as_array_mut()?
        .first_mut()
        .map(take)
}

fn take_extension(mut row: Value) -> Value {
    row.get_mut("extension").map_or(Value::Null, take)
}

pub(super) fn insert_extension(
    record: &mut Value,
    namespace: String,
    value: Value,
) -> Result<(), String> {
    let slot = object_or_create(record)?
        .entry("extensions")
        .or_insert(Value::Null);
    object_or_create(slot)?.insert(namespace, value);
    Ok(())
}

fn object_or_create(value: &mut Value) -> Result<&mut Map<String, Value>, String> {
    if value.is_null() {
        *value = Value::Object(Map::new());
    }
    value
        .as_object_mut()
        .ok_or_else(|| "Expected an object in the native tool response.".into())
}
