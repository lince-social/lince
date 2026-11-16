use crate::{MessageCursor, ProteinError, ThreadsInclude};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use store::sqlx::Row;
use store::{Store, records::RecordRow};

#[derive(Default)]
pub(crate) struct Selection {
    threads: HashMap<String, Vec<RecordRow>>,
    message_in: Option<String>,
    reply_to: Option<String>,
    references: Option<String>,
    call: Option<String>,
}

impl Selection {
    pub(crate) async fn load(store: &Store, records: &[&str]) -> Result<Self, ProteinError> {
        if records.is_empty() {
            return Ok(Self::default());
        }
        let _selection = nucleus::performance::Span::new("protein-thread-selection-await");
        nucleus::performance::add("protein-thread-selections", 1);
        let Some(thread_of) = store::vocabulary::feature_read(&store.pool, "thread").await? else {
            return Ok(Self::default());
        };
        let message_in = store::vocabulary::feature_read(&store.pool, "message").await?;
        let reply_to = store::vocabulary::feature_read(&store.pool, "reply").await?;
        let references = store::vocabulary::feature_read(&store.pool, "reference").await?;
        let call = store::vocabulary::feature_read(&store.pool, "call").await?;
        let threads =
            store::assertions::subjects_pointing_to_many(&store.pool, &thread_of, records).await?;
        Ok(Self {
            threads,
            message_in,
            reply_to,
            references,
            call,
        })
    }

    pub(crate) async fn for_record(
        &self,
        store: &Store,
        record: &str,
        settings: &ThreadsInclude,
    ) -> Result<Vec<Value>, ProteinError> {
        let Some(threads) = self.threads.get(record) else {
            return Ok(Vec::new());
        };
        let _projection = nucleus::performance::Span::new("protein-thread-projection-await");
        let Self {
            message_in,
            reply_to,
            references,
            ..
        } = self;
        let mut out = Vec::new();
        let mut organ_names: HashMap<Option<String>, Option<String>> = HashMap::new();
        let thread_ids: Vec<_> = threads
            .iter()
            .filter(|thread| thread.kind == "thread" && thread.quantity.is_positive())
            .map(|thread| thread.uid.as_str())
            .collect();
        let mut thread_metadata = metadata(store, &thread_ids).await?;
        for thread in threads {
            if thread.kind != "thread" || !thread.quantity.is_positive() {
                continue;
            }
            let thread_data = thread_metadata.remove(&thread.uid).unwrap_or_default();
            let social_root = thread_data.replica_root;
            let social_participant = if let Some(root) = &social_root {
                store::records::get_extension(
                    &store.pool,
                    root,
                    nucleus::social::requests::PARTICIPANTS_NAMESPACE,
                )
                .await?
            } else {
                None
            };
            let social_reveal = if let Some(root) = &social_root
                && social_participant.is_some()
            {
                store::records::get_extension(
                    &store.pool,
                    root,
                    nucleus::social::requests::REVEAL_NAMESPACE,
                )
                .await?
            } else {
                None
            };
            let messages_limit = settings
                .message_limits
                .get(&thread.uid)
                .copied()
                .unwrap_or(settings.messages_limit)
                .clamp(1, 2000);
            let before = settings.message_before.get(&thread.uid);
            let mut messages = Vec::new();
            let mut linked_messages = match message_in {
                Some(message_in) => {
                    store::assertions::messages_before(
                        &store.pool,
                        message_in,
                        &thread.uid,
                        messages_limit + 1,
                        before.map(|cursor| (cursor.created_at.as_str(), cursor.uid.as_str())),
                    )
                    .await?
                }
                None => Vec::new(),
            };
            let has_more = linked_messages.len() > messages_limit;
            linked_messages.truncate(messages_limit);
            let next_cursor = linked_messages.last().map(|message| MessageCursor {
                created_at: message.created_at.clone(),
                uid: message.uid.clone(),
            });
            linked_messages.reverse();
            let ids: Vec<_> = linked_messages
                .iter()
                .map(|message| message.uid.as_str())
                .collect();
            let mut message_metadata = metadata(store, &ids).await?;
            let mut extensions = message_extensions(store, &ids).await?;
            let mut replies = match reply_to {
                Some(predicate) => {
                    store::assertions::objects_from_subjects::<Option<String>>(
                        &store.pool,
                        &ids,
                        predicate,
                        false,
                    )
                    .await?
                }
                None => HashMap::new(),
            };
            let (mut references_by_message, mut reference_ids) = match references {
                Some(predicate) => (
                    store::assertions::objects_from_subjects::<String>(
                        &store.pool,
                        &ids,
                        predicate,
                        true,
                    )
                    .await?,
                    store::assertions::object_uids_from_subjects(&store.pool, &ids, predicate)
                        .await?,
                ),
                None => (HashMap::new(), HashMap::new()),
            };
            for message in linked_messages {
                if message.kind != "message" || !message.quantity.is_positive() {
                    continue;
                }
                let parent_message_uid = replies
                    .remove(&message.uid)
                    .and_then(|records| records.into_iter().next())
                    .map(|record| record.uid);
                let record_references = references_by_message
                    .remove(&message.uid)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|record| {
                        json!({
                            "uid": record.uid,
                            "slug": record.slug,
                            "kind": record.kind,
                            "head": record.head,
                            "body": record.body,
                        })
                    })
                    .collect::<Vec<_>>();
                let held: HashSet<_> = record_references
                    .iter()
                    .filter_map(|record| record["uid"].as_str())
                    .collect();
                let live_references: Vec<_> = reference_ids
                    .remove(&message.uid)
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|uid| !held.contains(uid.as_str()))
                    .collect();
                let Metadata {
                    created_at,
                    created_by,
                    sender,
                    replica_root,
                } = message_metadata.remove(&message.uid).unwrap_or_default();
                let organ_name =
                    organ_name_for(store, &mut organ_names, message.organ_uid.as_deref()).await?;
                let mut properties = extensions.remove(&message.uid).unwrap_or_default();
                let lifecycle = properties.remove("lince.message");
                let social = properties.remove(nucleus::social::requests::MESSAGE_NAMESPACE);
                let author = social
                    .as_ref()
                    .and_then(|s| s["content"]["author_owner"].as_str())
                    .map(str::to_owned)
                    .or_else(|| {
                        lifecycle
                            .as_ref()
                            .and_then(|value| value.get("author"))
                            .and_then(Value::as_str)
                            .map(str::to_string)
                            .or_else(|| created_by.clone())
                            .or_else(|| message.organ_uid.clone())
                    })
                    .unwrap_or_else(|| "local".into());
                let author_name = if social.is_some() {
                    if social_participant
                        .as_ref()
                        .is_some_and(|p| p["local_owner"] == author)
                    {
                        Some("You (private identity)".to_owned())
                    } else {
                        social_reveal
                            .as_ref()
                            .and_then(|r| r["peer"]["profile"]["fields"]["name"].as_str())
                            .filter(|s| !s.is_empty())
                            .map(str::to_owned)
                            .or_else(|| {
                                social_participant.as_ref().and_then(|p| {
                                    p["alias"]
                                        .as_str()
                                        .filter(|s| !s.is_empty())
                                        .map(str::to_owned)
                                })
                            })
                            .or_else(|| Some("Anonymous participant".to_owned()))
                    }
                } else {
                    organ_name_for(store, &mut organ_names, Some(&author)).await?
                };
                let operator = lifecycle
                    .as_ref()
                    .and_then(|value| value.get("operator"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| author.clone());
                let message_state = lifecycle
                    .as_ref()
                    .and_then(|value| value.get("state"))
                    .and_then(Value::as_str)
                    .filter(|value| matches!(*value, "writing" | "finished" | "interrupted"))
                    .unwrap_or("finished");
                let tool_call = properties.remove("lince.tool-call");
                messages.push(json!({
                "uid": message.uid,
                "head": message.head,
                "body": message.body,
                "quantity": message.quantity_f64(),
                "parent_message_uid": parent_message_uid,
                "created_at": created_at,
                "created_by": created_by,
                "sender": sender,
                "author": author,
                "author_name": author_name,
                "operator": operator,
                "message_state": message_state,
                "social": social,
                "social_delivery": properties.remove(nucleus::social::requests::DELIVERY_NAMESPACE),
                "tool_call": tool_call,
                "progress": properties.remove("lince.message-progress"),
                "content": properties.remove("lince.message-content"),
                "organ_name": organ_name,
                "references": record_references,
                "live_references": live_references,
                "organ_uid": message.organ_uid,
                "replica_root": replica_root,
            }));
                if messages.len() >= messages_limit {
                    break;
                }
            }
            let mut calls = Vec::new();
            if let Some(predicate) = &self.call {
                for record in
                    store::assertions::subjects_pointing_to(&store.pool, predicate, &thread.uid)
                        .await?
                        .into_iter()
                        .filter(|record| record.kind == "call_session")
                        .take(50)
                {
                    if let Ok(summary) = serde_json::from_str::<Value>(&record.body) {
                        calls.push(summary);
                    }
                }
            }
            let thread_organ_name =
                organ_name_for(store, &mut organ_names, thread.organ_uid.as_deref()).await?;
            let social_conversation = social_participant.is_some();
            out.push(json!({
                "uid": thread.uid,
                "head": thread.head,
                "body": thread.body,
                "quantity": thread.quantity_f64(),
                "created_at": thread_data.created_at,
                "created_by": thread_data.created_by,
                "sender": thread_data.sender,
                "organ_name": thread_organ_name,
                "messages": messages,
                "calls": calls,
                "messages_has_more": has_more,
                "messages_before": next_cursor,
                "messages_limit": messages_limit,
                "social_conversation": social_conversation,
            }));
        }
        Ok(out)
    }
}

pub(crate) async fn for_record(
    store: &Store,
    record: &str,
    settings: &ThreadsInclude,
) -> Result<Vec<Value>, ProteinError> {
    Selection::load(store, &[record])
        .await?
        .for_record(store, record, settings)
        .await
}

#[derive(Default)]
struct Metadata {
    created_at: Option<String>,
    created_by: Option<String>,
    sender: Option<String>,
    replica_root: Option<String>,
}

async fn metadata(
    store: &Store,
    records: &[&str],
) -> Result<HashMap<String, Metadata>, ProteinError> {
    let mut metadata = HashMap::new();
    if records.is_empty() {
        return Ok(metadata);
    }
    let ids = serde_json::to_string(records)
        .map_err(|error| store::StoreError::Protocol(error.to_string()))?;
    let rows = store::sqlx::query(
        "SELECT r.uid, CASE WHEN r.deleted_at IS NULL THEN r.created_at END AS created_at,
                r.replica_root, person.uid AS created_by, person.head AS sender_name, c.username
           FROM record r
           LEFT JOIN fact creator ON creator.rowid =
                (SELECT rowid FROM fact WHERE record_uid = r.uid ORDER BY rowid ASC LIMIT 1)
           LEFT JOIN person_credential c ON c.person_uid = creator.actor_uid
           LEFT JOIN record person ON person.uid = c.person_uid
                AND person.kind = 'person' AND person.deleted_at IS NULL
          WHERE r.uid IN (SELECT value FROM json_each(?))",
    )
    .bind(ids)
    .fetch_all(&store.pool)
    .await?;
    for row in rows {
        let created_by: Option<String> = row.get("created_by");
        let sender = row.get::<Option<String>, _>("sender_name").map(|name| {
            if name.trim().is_empty() {
                row.get("username")
            } else {
                name
            }
        });
        metadata.insert(
            row.get("uid"),
            Metadata {
                created_at: row.get("created_at"),
                created_by,
                sender,
                replica_root: row.get("replica_root"),
            },
        );
    }
    Ok(metadata)
}

async fn message_extensions(
    store: &Store,
    records: &[&str],
) -> Result<HashMap<String, HashMap<String, Value>>, ProteinError> {
    let mut extensions = HashMap::<String, HashMap<String, Value>>::new();
    if records.is_empty() {
        return Ok(extensions);
    }
    let ids = serde_json::to_string(records)
        .map_err(|error| store::StoreError::Protocol(error.to_string()))?;
    let rows = store::sqlx::query(
        "SELECT record_uid, namespace, fds FROM record_extension
          WHERE record_uid IN (SELECT value FROM json_each(?))
            AND namespace IN (?, ?, ?, ?, ?, ?)",
    )
    .bind(ids)
    .bind("lince.message")
    .bind(nucleus::social::requests::MESSAGE_NAMESPACE)
    .bind(nucleus::social::requests::DELIVERY_NAMESPACE)
    .bind("lince.tool-call")
    .bind("lince.message-progress")
    .bind("lince.message-content")
    .fetch_all(&store.pool)
    .await?;
    for row in rows {
        if let Ok(value) = serde_json::from_str(row.get::<&str, _>("fds")) {
            extensions
                .entry(row.get("record_uid"))
                .or_default()
                .insert(row.get("namespace"), value);
        }
    }
    Ok(extensions)
}

async fn organ_name_for(
    store: &Store,
    cache: &mut HashMap<Option<String>, Option<String>>,
    organ_uid: Option<&str>,
) -> Result<Option<String>, ProteinError> {
    let key = organ_uid.map(str::to_string);
    if let Some(cached) = cache.get(&key) {
        return Ok(cached.clone());
    }
    let name = match organ_uid {
        Some(uid) => store::records::live_heads_for(&store.pool, &[uid])
            .await?
            .remove(uid),
        None => store::organs::local(&store.pool)
            .await?
            .map(|organ| organ.head),
    };
    cache.insert(key, name.clone());
    Ok(name)
}
