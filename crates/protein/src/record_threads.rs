use crate::{MessageCursor, ProteinError, ThreadsInclude};
use serde_json::{Value, json};
use std::collections::HashMap;
use store::{Store, records::RecordRow};

#[derive(Default)]
pub(crate) struct Selection {
    threads: HashMap<String, Vec<RecordRow>>,
    message_in: Option<String>,
    reply_to: Option<String>,
    references: Option<String>,
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
        let threads =
            store::assertions::subjects_pointing_to_many(&store.pool, &thread_of, records).await?;
        Ok(Self {
            threads,
            message_in,
            reply_to,
            references,
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
        for thread in threads {
            if thread.kind != "thread" || !thread.quantity.is_positive() {
                continue;
            }
            let social_root = store::replica::root_of(&store.pool, &thread.uid).await?;
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
            for message in linked_messages {
                if message.kind != "message" || !message.quantity.is_positive() {
                    continue;
                }
                let parent_message_uid = match reply_to {
                    Some(reply_to) => {
                        store::assertions::objects_from_subject(&store.pool, &message.uid, reply_to)
                            .await?
                            .into_iter()
                            .next()
                            .map(|r| r.uid)
                    }
                    None => None,
                };
                let record_references = match references {
                    Some(references) => store::assertions::objects_from_subject(
                        &store.pool,
                        &message.uid,
                        references,
                    )
                    .await?
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
                    .collect::<Vec<_>>(),
                    None => Vec::new(),
                };
                let created_at = store::records::created_at(&store.pool, &message.uid).await?;
                let (created_by, sender) = creator_info(store, &message.uid).await?;
                let organ_name =
                    organ_name_for(store, &mut organ_names, message.organ_uid.as_deref()).await?;
                let lifecycle =
                    store::records::get_extension(&store.pool, &message.uid, "lince.message")
                        .await?;
                let social = store::records::get_extension(
                    &store.pool,
                    &message.uid,
                    nucleus::social::requests::MESSAGE_NAMESPACE,
                )
                .await?;
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
                let tool_call =
                    store::records::get_extension(&store.pool, &message.uid, "lince.tool-call")
                        .await?;
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
                "social_delivery": store::records::get_extension(&store.pool,&message.uid,nucleus::social::requests::DELIVERY_NAMESPACE).await?,
                "tool_call": tool_call,
                "progress": store::records::get_extension(&store.pool, &message.uid, "lince.message-progress").await?,
                "content": store::records::get_extension(&store.pool, &message.uid, "lince.message-content").await?,
                "organ_name": organ_name,
                "references": record_references,
                "live_references": match references {
                    Some(predicate) => {
                        let held: Vec<&str> = record_references
                            .iter()
                            .filter_map(|item| item["uid"].as_str())
                            .collect();
                        store::assertions::object_uids_from_subject(
                            &store.pool,
                            &message.uid,
                            predicate,
                        )
                        .await?
                        .into_iter()
                        .filter(|uid| !held.iter().any(|kept| kept == uid))
                        .collect::<Vec<_>>()
                    }
                    None => Vec::new(),
                },
                "organ_uid": message.organ_uid,
                "replica_root": store::replica::root_of(&store.pool, &message.uid).await?,
            }));
                if messages.len() >= messages_limit {
                    break;
                }
            }
            let thread_created_at = store::records::created_at(&store.pool, &thread.uid).await?;
            let mut calls = Vec::new();
            if let Some(predicate) = store::vocabulary::feature_read(&store.pool, "call").await? {
                for record in
                    store::assertions::subjects_pointing_to(&store.pool, &predicate, &thread.uid)
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
            let (thread_created_by, thread_sender) = creator_info(store, &thread.uid).await?;
            let thread_organ_name =
                organ_name_for(store, &mut organ_names, thread.organ_uid.as_deref()).await?;
            let social_conversation = social_participant.is_some();
            out.push(json!({
                "uid": thread.uid,
                "head": thread.head,
                "body": thread.body,
                "quantity": thread.quantity_f64(),
                "created_at": thread_created_at,
                "created_by": thread_created_by,
                "sender": thread_sender,
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

async fn creator_info(
    store: &Store,
    record_uid: &str,
) -> Result<(Option<String>, Option<String>), ProteinError> {
    let Some(actor_uid) = store::facts::creator_uid(&store.pool, record_uid).await? else {
        return Ok((None, None));
    };
    let Some(user) = store::auth::user_by_uid(&store.pool, &actor_uid).await? else {
        return Ok((None, None));
    };
    let name = if user.name.trim().is_empty() {
        user.username
    } else {
        user.name
    };
    Ok((Some(actor_uid), Some(name)))
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
        Some(uid) => store::records::get(&store.pool, uid)
            .await?
            .map(|record| record.head),
        None => store::organs::local(&store.pool)
            .await?
            .map(|organ| organ.head),
    };
    cache.insert(key, name.clone());
    Ok(name)
}
