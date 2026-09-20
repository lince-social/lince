use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use engine::{
    actions::Action,
    record_change::{Mutation, Request},
};
use loro::LoroDoc;
use serde::Deserialize;
use serde_json::{Value, json};

use super::{State, error, response};
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

impl State {
    pub(super) async fn edit(&mut self, arguments: Value) -> Result<Value, String> {
        let args: Arguments = serde_json::from_value(arguments.clone()).map_err(error)?;
        if let Some(previous) = self.previous(&args.request_id, &arguments)? {
            return Ok(previous);
        }
        let read = self
            .reads
            .get(&args.read_id)
            .ok_or("Unknown read_id. Read the Record in this turn first.")?;
        if args.edits.is_empty() || args.edits.len() > 32 {
            return Err("Supply between 1 and 32 text replacements.".into());
        }
        let doc = LoroDoc::new();
        doc.import(&read.snapshot).map_err(error)?;
        let before_version = doc.oplog_vv();
        let mut replacements = Vec::new();
        for edit in args.edits {
            if !matches!(edit.field.as_str(), "head" | "body")
                || !read.writable.contains(&edit.field)
            {
                return Err("This text property is not writable.".into());
            }
            let text = doc.get_text(edit.field.as_str()).to_string();
            let offset = if edit.before.is_empty() {
                if !text.is_empty() {
                    return Err("An empty before passage can only fill an empty field. Include nearby text to insert into a nonempty field.".into());
                }
                0
            } else {
                let offset = text.find(&edit.before).ok_or("The before passage does not appear in this read. Read again or copy the exact passage.")?;
                let next = offset
                    + text[offset..]
                        .chars()
                        .next()
                        .ok_or("Missing passage.")?
                        .len_utf8();
                if text[next..].contains(&edit.before) {
                    return Err(
                        "The before passage is ambiguous. Include more surrounding text.".into(),
                    );
                }
                text[..offset].chars().count()
            };
            let end = offset + edit.before.chars().count();
            replacements.push((edit.field, offset, end, edit.after));
        }
        replacements.sort_by(|a, b| (&a.0, a.1).cmp(&(&b.0, b.1)));
        for pair in replacements.windows(2) {
            if pair[0].0 == pair[1].0 && (pair[0].2 > pair[1].1 || pair[0].1 == pair[1].1) {
                return Err("Text replacements must not overlap.".into());
            }
        }
        for (field, start, end, after) in replacements.into_iter().rev() {
            let text = doc.get_text(field.as_str());
            if end != start {
                text.delete(start, end - start).map_err(error)?;
            }
            if !after.is_empty() {
                text.insert(start, &after).map_err(error)?;
            }
        }
        doc.commit();
        let delta =
            doc.export_json_updates_without_peer_compression(&before_version, &doc.oplog_vv());
        let bytes = serde_json::to_vec(&delta).map_err(error)?;
        if bytes.len() > engine::collab::limits().delta_bytes {
            return Err("Text edit exceeds the collaborative operation limit.".into());
        }
        let uid = read.record["uid"]
            .as_str()
            .ok_or("Missing Record UID.")?
            .to_string();
        let result = response(self.session.handle(ClientMessage::Act {
            id: args.request_id.clone(),
            action: Action::ChangeRecord { request: Request {
                id: nucleus::new_uid("op"), record_uid: uid.clone(),
                mutation: Mutation::Text { update_base64: B64.encode(bytes) },
            } },
        }).await).map(|result| json!({"record_uid":uid,"state":"saved","result":result,"next":"Read the Record again to see the current merged text."}));
        self.remember(args.request_id, &arguments, result.clone());
        result
    }
}
