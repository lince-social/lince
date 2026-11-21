use std::collections::HashMap;

use nucleus::{
    performance::{Span, add},
    social::requests::{DELIVERY_NAMESPACE, MESSAGE_NAMESPACE},
};
use serde_json::{Value, from_str, to_string};
use store::{
    Store,
    sqlx::{Error::Protocol, Row, query},
};

use crate::ProteinError;

#[derive(Default)]
pub(super) struct Metadata {
    pub created_at: Option<String>,
    pub created_by: Option<String>,
    pub sender: Option<String>,
    pub replica_root: Option<String>,
}

pub(super) async fn metadata(
    store: &Store,
    ids: Option<&str>,
) -> Result<HashMap<String, Metadata>, ProteinError> {
    let mut metadata = HashMap::new();
    let Some(ids) = ids else {
        return Ok(metadata);
    };
    let rows = query(
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

pub(super) fn encode_ids(records: &[&str]) -> Result<Option<String>, ProteinError> {
    if records.is_empty() {
        Ok(None)
    } else {
        to_string(records)
            .map(Some)
            .map_err(|error| Protocol(error.to_string()))
    }
}

#[derive(Default)]
pub(super) struct MessageExtensions {
    pub lifecycle: Option<Value>,
    pub social: Option<Value>,
    pub delivery: Option<Value>,
    pub tool_call: Option<Value>,
    pub progress: Option<Value>,
    pub content: Option<Value>,
}

pub(super) async fn message_extensions(
    store: &Store,
    ids: Option<&str>,
) -> Result<HashMap<String, MessageExtensions>, ProteinError> {
    let mut extensions = HashMap::<String, MessageExtensions>::new();
    let Some(ids) = ids else {
        return Ok(extensions);
    };
    let rows = query(
        "SELECT record_uid, namespace, fds FROM record_extension
          WHERE record_uid IN (SELECT value FROM json_each(?))
            AND namespace IN (?, ?, ?, ?, ?, ?)",
    )
    .bind(ids)
    .bind("lince.message")
    .bind(MESSAGE_NAMESPACE)
    .bind(DELIVERY_NAMESPACE)
    .bind("lince.tool-call")
    .bind("lince.message-progress")
    .bind("lince.message-content")
    .fetch_all(&store.pool)
    .await?;
    let _decode = Span::new("protein-thread-extension-decode");
    let mut decoded = 0;
    for row in rows {
        if let Ok(value) = from_str(row.get::<&str, _>("fds")) {
            let fields = extensions.entry(row.get("record_uid")).or_default();
            let slot = match row.get::<&str, _>("namespace") {
                "lince.message" => &mut fields.lifecycle,
                MESSAGE_NAMESPACE => &mut fields.social,
                DELIVERY_NAMESPACE => &mut fields.delivery,
                "lince.tool-call" => &mut fields.tool_call,
                "lince.message-progress" => &mut fields.progress,
                "lince.message-content" => &mut fields.content,
                _ => continue,
            };
            *slot = Some(value);
            decoded += 1;
        }
    }
    add("protein-thread-extensions-decoded", decoded);
    Ok(extensions)
}

#[derive(Default)]
pub(super) struct OrganNames {
    organs: HashMap<String, Option<String>>,
    local: Option<Option<String>>,
}

pub(super) async fn organ_name_for(
    store: &Store,
    cache: &mut OrganNames,
    organ_uid: Option<&str>,
) -> Result<Option<String>, ProteinError> {
    let _name = Span::new("protein-thread-organ-name-await");
    let cached = match organ_uid {
        Some(uid) => cache.organs.get(uid),
        None => cache.local.as_ref(),
    };
    if let Some(cached) = cached {
        return Ok(cached.clone());
    }
    add("protein-thread-organ-name-reads", 1);
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
