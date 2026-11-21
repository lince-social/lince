//! Record-linked threads and their message pages.

mod messages;
mod metadata;

use std::collections::HashMap;

use nucleus::{
    performance::{Span, add},
    social::requests::{PARTICIPANTS_NAMESPACE, REVEAL_NAMESPACE},
};
use serde_json::{Value, from_str};
use store::{
    Store,
    assertions::{subjects_pointing_to, subjects_pointing_to_many},
    records::{RecordRow, get_extension},
    vocabulary::feature_read,
};

use crate::{ProteinError, ThreadsInclude};

use self::{
    messages::{Meanings, Page, Social},
    metadata::{OrganNames, encode_ids, metadata, organ_name_for},
};

#[derive(Default)]
pub(crate) struct Selection {
    threads: HashMap<String, Vec<RecordRow>>,
    tasks: HashMap<String, (String, String)>,
    meanings: Meanings,
    call: Option<String>,
}

impl Selection {
    pub(crate) async fn load(
        store: &Store,
        records: &[&str],
        fiote_sessions: bool,
    ) -> Result<Self, ProteinError> {
        if records.is_empty() {
            return Ok(Self::default());
        }
        let _selection = Span::new("protein-thread-selection-await");
        add("protein-thread-selections", 1);
        let Some(thread_of) = feature_read(&store.pool, "thread").await? else {
            return Ok(Self::default());
        };
        let meanings = Meanings {
            message_in: feature_read(&store.pool, "message").await?,
            reply_to: feature_read(&store.pool, "reply").await?,
            references: feature_read(&store.pool, "reference").await?,
        };
        let call = feature_read(&store.pool, "call").await?;
        let mut threads = subjects_pointing_to_many(&store.pool, &thread_of, records).await?;
        let tasks = if fiote_sessions {
            include_fiote_sessions(store, records, &mut threads).await?
        } else {
            HashMap::new()
        };
        Ok(Self {
            threads,
            tasks,
            meanings,
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
        let _projection = Span::new("protein-thread-projection-await");
        let mut out = Vec::new();
        let mut organ_names = OrganNames::default();
        let threads = threads
            .iter()
            .filter(|thread| thread.kind == "thread" && thread.quantity.is_positive());
        let ids = threads
            .clone()
            .map(|thread| thread.uid.as_str())
            .collect::<Vec<_>>();
        let mut thread_metadata = metadata(store, encode_ids(&ids)?.as_deref()).await?;
        for thread in threads {
            let thread_data = thread_metadata.remove(&thread.uid).unwrap_or_default();
            let social_root = thread_data.replica_root;
            let social_participant = if let Some(root) = &social_root {
                get_extension(&store.pool, root, PARTICIPANTS_NAMESPACE).await?
            } else {
                None
            };
            let social_reveal = if let Some(root) = &social_root
                && social_participant.is_some()
            {
                get_extension(&store.pool, root, REVEAL_NAMESPACE).await?
            } else {
                None
            };
            let Page {
                messages,
                next_cursor,
                has_more,
                limit,
            } = messages::project(
                store,
                &thread.uid,
                settings,
                &self.meanings,
                Social {
                    participant: social_participant.as_ref(),
                    reveal: social_reveal.as_ref(),
                },
                &mut organ_names,
            )
            .await?;
            let calls = if let Some(predicate) = &self.call {
                subjects_pointing_to(&store.pool, predicate, &thread.uid)
                    .await?
                    .into_iter()
                    .filter(|record| record.kind == "call_session")
                    .take(50)
                    .filter_map(|record| from_str::<Value>(&record.body).ok())
                    .collect()
            } else {
                Vec::new()
            };
            let thread_organ_name =
                organ_name_for(store, &mut organ_names, thread.organ_uid.as_deref()).await?;
            let social_conversation = social_participant.is_some();
            let _page = Span::new("protein-thread-page-publish");
            let mut row = Value::from_iter([
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
                ("messages_before", next_cursor),
                ("messages_limit", limit.into()),
                ("social_conversation", social_conversation.into()),
            ]);
            if let Some((task, title)) = self.tasks.get(&thread.uid) {
                row["task_uid"] = task.as_str().into();
                row["task_title"] = title.as_str().into();
            }
            out.push(row);
        }
        if settings.fiote_sessions {
            fn last_activity(thread: &Value) -> &str {
                thread["messages"]
                    .as_array()
                    .and_then(|messages| {
                        messages
                            .iter()
                            .filter_map(|message| message["created_at"].as_str())
                            .max()
                    })
                    .or_else(|| thread["created_at"].as_str())
                    .unwrap_or_default()
            }
            out.sort_by(|left, right| last_activity(right).cmp(last_activity(left)));
        }
        Ok(out)
    }
}

async fn include_fiote_sessions(
    store: &Store,
    records: &[&str],
    threads: &mut HashMap<String, Vec<RecordRow>>,
) -> Result<HashMap<String, (String, String)>, ProteinError> {
    let Some(ids) = encode_ids(records)? else {
        return Ok(HashMap::new());
    };
    let sessions: Vec<(String, String, Option<String>, Option<String>)> = store::sqlx::query_as(
        "SELECT owner.record_uid, session.record_uid, task.uid, task.head
         FROM record_extension session
         JOIN record_extension owner
           ON owner.namespace = 'lince.fiote' AND owner.record_uid = json_extract(session.fds, '$.fiote')
         JOIN json_each(?) requested ON requested.value = owner.record_uid
         LEFT JOIN record task ON task.uid = json_extract(session.fds, '$.task') AND task.deleted_at IS NULL
         WHERE session.namespace IN ('lince.fiote-session', 'lince.fiote-task')",
    )
    .bind(ids)
    .fetch_all(&store.pool)
    .await?;
    let ids: Vec<_> = sessions
        .iter()
        .map(|(_, thread, _, _)| thread.as_str())
        .collect();
    let rows: HashMap<_, _> = store::records::get_many(&store.pool, &ids)
        .await?
        .into_iter()
        .map(|row| (row.uid.clone(), row))
        .collect();
    let mut tasks = HashMap::new();
    for (fiote, thread, task, title) in sessions {
        if let (Some(task), Some(title)) = (task, title) {
            tasks.insert(thread.clone(), (task, title));
        }
        let related = threads.entry(fiote).or_default();
        if let Some(row) = rows.get(&thread)
            && !related.iter().any(|existing| existing.uid == thread)
        {
            related.push(row.clone());
        }
    }
    Ok(tasks)
}

pub(crate) async fn for_record(
    store: &Store,
    record: &str,
    settings: &ThreadsInclude,
) -> Result<Vec<Value>, ProteinError> {
    Selection::load(store, &[record], false)
        .await?
        .for_record(store, record, settings)
        .await
}
