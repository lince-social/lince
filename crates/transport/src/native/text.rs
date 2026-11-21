//! Builds collaborative text replacements against the snapshot read in this turn.

use std::ops::Range;

use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use engine::{
    actions::Action,
    collab::limits,
    record_change::{Mutation, Request},
};
use loro::LoroDoc;
use nucleus::new_uid;
use serde::Deserialize;
use serde_json::{Value, json, to_vec};

use super::{Read, State, error, receipts::fingerprint, response};
use crate::ClientMessage;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Arguments {
    read_id: String,
    request_id: String,
    edits: Vec<Edit>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Edit {
    field: String,
    before: String,
    after: String,
}

struct Replacement {
    field: String,
    range: Range<usize>,
    after: String,
}

impl State {
    pub(super) async fn edit(&mut self, arguments: &Value) -> Result<Value, String> {
        let args = Arguments::deserialize(arguments).map_err(error)?;
        if let Some(previous) = self.receipts.previous(&args.request_id, arguments)? {
            return Ok(previous);
        }
        let read = self
            .reads
            .get(&args.read_id)
            .ok_or("Unknown read_id. Read the Record in this turn first.")?;
        if !(1..=32).contains(&args.edits.len()) {
            return Err("Supply between 1 and 32 text replacements.".into());
        }
        let bytes = prepare_update(read, args.edits)?;
        if bytes.len() > limits().delta_bytes {
            return Err("Text edit exceeds the collaborative operation limit.".into());
        }
        let uid = read
            .record
            .get("uid")
            .and_then(Value::as_str)
            .ok_or("Missing Record UID.")?;
        let mark = fingerprint(arguments)?;
        let message = ClientMessage::Act {
            id: args.request_id.clone(),
            action: Action::ChangeRecord {
                request: Request {
                    id: new_uid("op"),
                    record_uid: uid.into(),
                    mutation: Mutation::Text {
                        update_base64: B64.encode(bytes),
                    },
                },
            },
        };
        let result = response(self.session.handle(message).await).map(|result| {
            json!({
                "record_uid": uid, "state": "saved", "result": result,
                "next": "Read the Record again to see the current merged text."
            })
        });
        self.receipts
            .remember(args.request_id, mark, result.clone());
        result
    }
}

fn prepare_update(read: &Read, edits: Vec<Edit>) -> Result<Vec<u8>, String> {
    let doc = LoroDoc::new();
    doc.import(&read.snapshot).map_err(error)?;
    let before_version = doc.oplog_vv();
    let mut replacements = Vec::with_capacity(edits.len());
    {
        let (mut head, mut body) = (None, None);
        for Edit {
            field,
            before,
            after,
        } in edits
        {
            let source = match field.as_str() {
                "head" => &mut head,
                "body" => &mut body,
                _ => return Err("This text property is not writable.".into()),
            };
            if !read.writable.contains(&field) {
                return Err("This text property is not writable.".into());
            }
            let text = source.get_or_insert_with(|| doc.get_text(field.as_str()).to_string());
            replacements.push(Replacement {
                range: passage_range(text, &before)?,
                field,
                after,
            });
        }
    }
    replacements.sort_by(|a, b| (&a.field, a.range.start).cmp(&(&b.field, b.range.start)));
    if replacements.windows(2).any(|pair| {
        matches!(pair, [a, b] if a.field == b.field && (a.range.end > b.range.start || a.range.start == b.range.start))
    }) {
        return Err("Text replacements must not overlap.".into());
    }
    // Later offsets must be applied first so earlier offsets still name the same passage.
    for Replacement {
        field,
        range,
        after,
    } in replacements.into_iter().rev()
    {
        let text = doc.get_text(field.as_str());
        if !range.is_empty() {
            text.delete(range.start, range.len()).map_err(error)?;
        }
        if !after.is_empty() {
            text.insert(range.start, &after).map_err(error)?;
        }
    }
    doc.commit();
    let delta = doc
        .export_json_updates_without_peer_compression(&before_version, &doc.oplog_vv())
        .map_err(error)?;
    to_vec(&delta).map_err(error)
}

fn passage_range(text: &str, before: &str) -> Result<Range<usize>, String> {
    if before.is_empty() {
        return if text.is_empty() {
            Ok(0..0)
        } else {
            Err("An empty before passage can only fill an empty field. Include nearby text to insert into a nonempty field.".into())
        };
    }
    let offset = text.find(before).ok_or(
        "The before passage does not appear in this read. Read again or copy the exact passage.",
    )?;
    let (prefix, found) = text.split_at_checked(offset).ok_or("Missing passage.")?;
    let mut later = found.chars();
    later.next().ok_or("Missing passage.")?;
    if later.as_str().contains(before) {
        return Err("The before passage is ambiguous. Include more surrounding text.".into());
    }
    let start = prefix.chars().count();
    Ok(start..start + before.chars().count())
}
