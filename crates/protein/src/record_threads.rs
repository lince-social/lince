use crate::{MessageCursor, ProteinError, ThreadsInclude};
use serde_json::Value;
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
        let mut organ_names = OrganNames::default();
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
            let mut social_names = None;
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
                    let _parents = nucleus::performance::Span::new("protein-thread-parents-await");
                    store::assertions::active_object_uids_from_subjects(
                        &store.pool,
                        &ids,
                        predicate,
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
                    .and_then(|records| records.into_iter().next());
                let record_references = {
                    let _references =
                        nucleus::performance::Span::new("protein-thread-reference-publish");
                    references_by_message
                        .remove(&message.uid)
                        .unwrap_or_default()
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
                        .collect::<Vec<_>>()
                };
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
                drop(held);
                let Metadata {
                    created_at,
                    created_by,
                    sender,
                    replica_root,
                } = message_metadata.remove(&message.uid).unwrap_or_default();
                let organ_name =
                    organ_name_for(store, &mut organ_names, message.organ_uid.as_deref()).await?;
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
                    let (local_owner, peer_name) = *social_names.get_or_insert_with(|| {
                        nucleus::performance::add("protein-thread-social-label-preparations", 1);
                        let owner = social_participant
                            .as_ref()
                            .and_then(|participant| participant["local_owner"].as_str());
                        let peer = social_reveal
                            .as_ref()
                            .and_then(|reveal| reveal["peer"]["profile"]["fields"]["name"].as_str())
                            .filter(|name| !name.is_empty())
                            .or_else(|| {
                                social_participant
                                    .as_ref()
                                    .and_then(|participant| participant["alias"].as_str())
                                    .filter(|name| !name.is_empty())
                            })
                            .unwrap_or("Anonymous participant");
                        (owner, peer)
                    });
                    Some(
                        if local_owner == Some(author.as_str()) {
                            "You (private identity)"
                        } else {
                            peer_name
                        }
                        .to_owned(),
                    )
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
                {
                    let _message =
                        nucleus::performance::Span::new("protein-thread-message-publish");
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
                        ("author", Value::String(author)),
                        ("author_name", author_name.into()),
                        ("operator", Value::String(operator)),
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
            let _page = nucleus::performance::Span::new("protein-thread-page-publish");
            let next_cursor = next_cursor.map(|MessageCursor { created_at, uid }| {
                Value::from_iter([
                    ("created_at", Value::String(created_at)),
                    ("uid", Value::String(uid)),
                ])
            });
            out.push(Value::from_iter([
                ("uid", thread.uid.as_str().into()),
                ("head", thread.head.as_str().into()),
                ("body", thread.body.as_str().into()),
                ("quantity", thread.quantity_f64().into()),
                ("created_at", thread_data.created_at.into()),
                ("created_by", thread_data.created_by.into()),
                ("sender", thread_data.sender.into()),
                ("organ_name", thread_organ_name.into()),
                ("messages", Value::Array(messages)),
                ("calls", Value::Array(calls)),
                ("messages_has_more", has_more.into()),
                ("messages_before", next_cursor.into()),
                ("messages_limit", messages_limit.into()),
                ("social_conversation", social_conversation.into()),
            ]));
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

#[derive(Default)]
struct MessageExtensions {
    lifecycle: Option<Value>,
    social: Option<Value>,
    delivery: Option<Value>,
    tool_call: Option<Value>,
    progress: Option<Value>,
    content: Option<Value>,
}

async fn message_extensions(
    store: &Store,
    records: &[&str],
) -> Result<HashMap<String, MessageExtensions>, ProteinError> {
    let mut extensions = HashMap::<String, MessageExtensions>::new();
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
    let _decode = nucleus::performance::Span::new("protein-thread-extension-decode");
    let mut decoded = 0;
    for row in rows {
        if let Ok(value) = serde_json::from_str(row.get::<&str, _>("fds")) {
            let fields = extensions.entry(row.get("record_uid")).or_default();
            let slot = match row.get::<&str, _>("namespace") {
                "lince.message" => &mut fields.lifecycle,
                nucleus::social::requests::MESSAGE_NAMESPACE => &mut fields.social,
                nucleus::social::requests::DELIVERY_NAMESPACE => &mut fields.delivery,
                "lince.tool-call" => &mut fields.tool_call,
                "lince.message-progress" => &mut fields.progress,
                "lince.message-content" => &mut fields.content,
                _ => continue,
            };
            *slot = Some(value);
            decoded += 1;
        }
    }
    nucleus::performance::add("protein-thread-extensions-decoded", decoded);
    Ok(extensions)
}

#[derive(Default)]
struct OrganNames {
    organs: HashMap<String, Option<String>>,
    local: Option<Option<String>>,
}

async fn organ_name_for(
    store: &Store,
    cache: &mut OrganNames,
    organ_uid: Option<&str>,
) -> Result<Option<String>, ProteinError> {
    let _name = nucleus::performance::Span::new("protein-thread-organ-name-await");
    let cached = match organ_uid {
        Some(uid) => cache.organs.get(uid),
        None => cache.local.as_ref(),
    };
    if let Some(cached) = cached {
        return Ok(cached.clone());
    }
    nucleus::performance::add("protein-thread-organ-name-reads", 1);
    let name = match organ_uid {
        Some(uid) => store::records::live_heads_for(&store.pool, &[uid])
            .await?
            .remove(uid),
        None => store::organs::local_head(&store.pool).await?,
    };
    match organ_uid {
        Some(uid) => {
            cache.organs.insert(uid.to_owned(), name.clone());
        }
        None => cache.local = Some(name.clone()),
    }
    Ok(name)
}
