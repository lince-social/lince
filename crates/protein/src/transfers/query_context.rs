//! Context row describing a viewer’s Transfer creation options.

use super::{projection::RecordIndex, transfer_query_error, viewer::TransferViewer};
use crate::ProteinError;
use serde_json::{Value, json, to_writer};
use std::collections::HashSet;
use store::{Store, sqlx::query_scalar};

const MAX_POLICY_BYTES: usize = 65_536;

pub(super) struct QueryContext<'a> {
    pub row: Value,
    pub create_person: Option<&'a str>,
    pub need_outcomes: Value,
}

pub(super) async fn read<'a>(
    store: &Store,
    viewer: &'a TransferViewer,
    installed_signer_actor: Option<&'a str>,
    visible: Option<&HashSet<String>>,
    records_by_uid: &RecordIndex<'_>,
) -> Result<QueryContext<'a>, ProteinError> {
    let mut create_blockers = viewer.create_blockers();
    let create_person = if viewer.local {
        installed_signer_actor
    } else {
        viewer.person.as_deref()
    };
    if create_person.is_none() || installed_signer_actor != create_person {
        create_blockers.push("missing_person_signer");
    }
    let create = create_blockers.is_empty();
    let need_outcomes = if let Some(person) = create_person {
        store::transfer_evidence::need_options(&store.pool, person, visible).await?
    } else {
        json!({"options":[],"more":false})
    };
    let supplier_policies = match create_person {
        Some(person) => supplier_policies(store, person, visible).await?,
        None => Vec::new(),
    };
    let person_record = viewer
        .person
        .as_deref()
        .and_then(|uid| records_by_uid.get(uid));
    let row = json!({
        "kind": "transfer_context",
        "supplier_reader_policies":supplier_policies,
        "need_outcome_options":need_outcomes["options"],
        "need_outcome_options_more":need_outcomes["more"],
        "acting_person": create_person.filter(|person| records_by_uid.get(*person).is_some_and(|record| record.kind == "person")),
        "organ": store::organs::local(&store.pool).await?.map(|organ| organ.uid),
        "viewer": {
            "local": viewer.local,
            "recognized": viewer.recognized,
            "app_user": viewer.subject,
            "person": viewer.person,
            "person_head": person_record.map(|record| record.head.as_str()),
            "person_slug": person_record.and_then(|record| record.slug.as_deref()),
        },
        "capabilities": { "create": create },
        "blocking_reasons": { "create": create_blockers },
    });
    Ok(QueryContext {
        row,
        create_person,
        need_outcomes,
    })
}

async fn supplier_policies(
    store: &Store,
    person: &str,
    visible: Option<&HashSet<String>>,
) -> Result<Vec<Value>, ProteinError> {
    let records = query_scalar::<_, String>("SELECT q.record_uid FROM trust_query_record q JOIN record r ON r.uid=q.record_uid WHERE q.owner_person_uid=? AND r.deleted_at IS NULL ORDER BY q.record_uid DESC LIMIT 16").bind(person).fetch_all(&store.pool).await?;
    let mut used = 0;
    let mut buffer = Vec::new();
    let mut supplier_policies = Vec::new();
    let mut connection = store.pool.acquire().await?;
    for record in records {
        if visible.is_some_and(|visible| !visible.contains(&record)) {
            continue;
        }
        let saved = store::trust_queries::get_on(&mut connection, &record).await?;
        buffer.clear();
        to_writer(&mut buffer, &saved.query)
            .map_err(|error| transfer_query_error("supplier_policy_invalid", error))?;
        if buffer.len() > MAX_POLICY_BYTES - used {
            continue;
        }
        used += buffer.len();
        supplier_policies.push(json!({"label":saved.name,"query":saved.query}));
    }
    Ok(supplier_policies)
}
