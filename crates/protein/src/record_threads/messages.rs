use std::collections::{HashMap, HashSet};

use nucleus::performance::{Span, add};
use serde_json::Value;
use store::{
    Store,
    assertions::{
        active_object_uids_from_subjects, messages_before, object_uids_from_subjects,
        objects_from_subjects,
    },
    records::RecordRow,
};

use crate::{ProteinError, ThreadsInclude};

use super::metadata::{
    MessageExtensions, Metadata, OrganNames, encode_ids, message_extensions, metadata,
    organ_name_for,
};

#[derive(Default)]
pub(super) struct Meanings {
    pub message_in: Option<String>,
    pub reply_to: Option<String>,
    pub references: Option<String>,
}

pub(super) struct Page {
    pub messages: Vec<Value>,
    pub next_cursor: Value,
    pub has_more: bool,
    pub limit: usize,
}

pub(super) struct Social<'a> {
    pub participant: Option<&'a Value>,
    pub reveal: Option<&'a Value>,
}

pub(super) async fn project(
    store: &Store,
    thread: &str,
    settings: &ThreadsInclude,
    meanings: &Meanings,
    social: Social<'_>,
    organ_names: &mut OrganNames,
) -> Result<Page, ProteinError> {
    let Social {
        participant: social_participant,
        reveal: social_reveal,
    } = social;
    let mut social_names = None;
    let messages_limit = settings
        .message_limits
        .get(thread)
        .copied()
        .unwrap_or(settings.messages_limit)
        .clamp(1, 2000);
    let before = settings.message_before.get(thread);
    let mut linked_messages = match meanings.message_in.as_deref() {
        Some(message_in) => {
            messages_before(
                &store.pool,
                message_in,
                thread,
                messages_limit + 1,
                before.map(|cursor| (cursor.created_at.as_str(), cursor.uid.as_str())),
            )
            .await?
        }
        None => Vec::new(),
    };
    let has_more = linked_messages.len() > messages_limit;
    linked_messages.truncate(messages_limit);
    let next_cursor = linked_messages.last().map_or(Value::Null, |message| {
        Value::from_iter([
            ("created_at", Value::from(message.created_at.as_str())),
            ("uid", Value::from(message.uid.as_str())),
        ])
    });
    linked_messages.reverse();
    let ids: Vec<_> = linked_messages
        .iter()
        .map(|message| message.uid.as_str())
        .collect();
    let encoded_ids = encode_ids(&ids)?;
    let mut message_metadata = metadata(store, encoded_ids.as_deref()).await?;
    let mut extensions = message_extensions(store, encoded_ids.as_deref()).await?;
    let mut replies = match meanings.reply_to.as_deref() {
        Some(predicate) => {
            let _parents = Span::new("protein-thread-parents-await");
            active_object_uids_from_subjects(&store.pool, &ids, predicate).await?
        }
        None => HashMap::new(),
    };
    let (mut references_by_message, mut reference_ids) = match meanings.references.as_deref() {
        Some(predicate) => (
            objects_from_subjects::<String>(&store.pool, &ids, predicate, true).await?,
            object_uids_from_subjects(&store.pool, &ids, predicate).await?,
        ),
        None => (HashMap::new(), HashMap::new()),
    };
    let mut messages = Vec::with_capacity(linked_messages.len());
    for message in linked_messages {
        if message.kind != "message" || !message.quantity.is_positive() {
            continue;
        }
        let parent_message_uid = replies
            .remove(&message.uid)
            .and_then(|records| records.into_iter().next());
        let (record_references, live_references) = project_references(
            references_by_message
                .remove(&message.uid)
                .unwrap_or_default(),
            reference_ids.remove(&message.uid).unwrap_or_default(),
        );
        let Metadata {
            created_at,
            created_by,
            sender,
            replica_root,
        } = message_metadata.remove(&message.uid).unwrap_or_default();
        let organ_name = organ_name_for(store, organ_names, message.organ_uid.as_deref()).await?;
        let MessageExtensions {
            lifecycle,
            social,
            delivery,
            tool_call,
            progress,
            content,
        } = extensions.remove(&message.uid).unwrap_or_default();
        let author = social
            .as_ref()
            .and_then(|social| social["content"]["author_owner"].as_str())
            .or_else(|| text_field(&lifecycle, "author"))
            .or(created_by.as_deref())
            .or(message.organ_uid.as_deref())
            .unwrap_or("local");
        let author_name = if social.is_some() {
            let (local_owner, peer_name) = *social_names.get_or_insert_with(|| {
                add("protein-thread-social-label-preparations", 1);
                let owner =
                    social_participant.and_then(|participant| participant["local_owner"].as_str());
                let peer = social_reveal
                    .and_then(|reveal| reveal["peer"]["profile"]["fields"]["name"].as_str())
                    .filter(|name| !name.is_empty())
                    .or_else(|| {
                        social_participant
                            .and_then(|participant| participant["alias"].as_str())
                            .filter(|name| !name.is_empty())
                    })
                    .unwrap_or("Anonymous participant");
                (owner, peer)
            });
            Some(
                if local_owner == Some(author) {
                    "You (private identity)"
                } else {
                    peer_name
                }
                .to_owned(),
            )
        } else {
            organ_name_for(store, organ_names, Some(author)).await?
        };
        let operator = Value::from(text_field(&lifecycle, "operator").unwrap_or(author));
        let author = Value::from(author);
        let message_state = text_field(&lifecycle, "state")
            .filter(|value| matches!(*value, "writing" | "finished" | "interrupted"))
            .unwrap_or("finished");
        {
            let _message = Span::new("protein-thread-message-publish");
            let quantity = message.quantity_f64();
            messages.push(Value::from_iter([
                ("uid", Value::String(message.uid)),
                ("head", Value::String(message.head)),
                ("body", Value::String(message.body)),
                ("quantity", quantity.into()),
                ("parent_message_uid", parent_message_uid.into()),
                ("created_at", created_at.into()),
                ("created_by", created_by.into()),
                ("sender", sender.into()),
                ("author", author),
                ("author_name", author_name.into()),
                ("operator", operator),
                ("message_state", message_state.into()),
                ("social", social.into()),
                ("social_delivery", delivery.into()),
                ("tool_call", tool_call.into()),
                ("progress", progress.into()),
                ("content", content.into()),
                ("organ_name", organ_name.into()),
                ("references", Value::Array(record_references)),
                ("live_references", live_references.into()),
                ("organ_uid", message.organ_uid.into()),
                ("replica_root", replica_root.into()),
            ]));
        }
    }
    Ok(Page {
        messages,
        next_cursor,
        has_more,
        limit: messages_limit,
    })
}

fn text_field<'a>(document: &'a Option<Value>, key: &str) -> Option<&'a str> {
    document.as_ref()?.get(key)?.as_str()
}

fn project_references(records: Vec<RecordRow>, ids: Vec<String>) -> (Vec<Value>, Vec<String>) {
    let _references = Span::new("protein-thread-reference-publish");
    let held = records
        .iter()
        .map(|record| record.uid.as_str())
        .collect::<HashSet<_>>();
    let live = ids
        .into_iter()
        .filter(|uid| !held.contains(uid.as_str()))
        .collect();
    let snapshots = records
        .into_iter()
        .map(|record| {
            Value::from_iter([
                ("uid", Value::String(record.uid)),
                ("slug", record.slug.into()),
                ("kind", Value::String(record.kind)),
                ("head", Value::String(record.head)),
                ("body", Value::String(record.body)),
            ])
        })
        .collect();
    (snapshots, live)
}
