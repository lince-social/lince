//! Record selection, aggregation, and optional included data.

mod aggregate;
mod selection;

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, TimeDelta, Utc};
use nucleus::{
    parse_duration,
    performance::{Span, add},
};
use serde_json::{Map, Value, from_str};
use store::{Store, records::RecordBody, transfer_balances::read_many as read_balances};

use crate::{
    Protein, ProteinError, record_includes, record_links, record_schema, record_threads,
    relation_context, shader_inputs, transfers::protected_transfer_history,
};

use self::{aggregate::aggregate_records, selection::select_records_for};

pub(crate) use selection::matching_records_for;
pub use selection::{matching_among, matching_records, select_records};

pub(crate) async fn execute_records(
    store: &Store,
    protein: &Protein,
    visible: Option<&HashSet<String>>,
    subject: Option<&str>,
) -> Result<Vec<Value>, ProteinError> {
    let selection = Span::new("protein-record-selection-await");
    let rows = select_records_for::<Option<String>>(
        store,
        protein,
        visible,
        subject,
        record_schema::requires_body(protein),
    )
    .await?;
    drop(selection);
    let _materialization = Span::new("protein-record-materialization-await");

    if let Some(agg) = &protein.aggregate {
        return Ok(aggregate_records(&rows, agg));
    }

    let include = record_schema::selected_include(protein);
    let ids = rows
        .iter()
        .map(|record| record.uid.as_str())
        .collect::<Vec<_>>();
    let links = if let Some(settings) = &include.links
        && !rows.is_empty()
    {
        Some((
            settings,
            record_links::Graph::load(store, settings, subject).await?,
        ))
    } else {
        None
    };
    let threads = if let Some(settings) = &include.threads
        && !rows.is_empty()
    {
        Some((
            settings,
            record_threads::Selection::load(
                store,
                &ids,
                settings.fiote_sessions && subject.is_none() && visible.is_none(),
            )
            .await?,
        ))
    } else {
        None
    };
    let mut out = Vec::with_capacity(rows.len());
    let requested = protein
        .fields
        .as_ref()
        .map(|fields| fields.iter().map(String::as_str).collect::<HashSet<_>>());
    let wants = |key: &str| requested.as_ref().is_none_or(|fields| fields.contains(key));
    let concept_names = if wants("concept_name")
        || wants("assertions")
        || include.links.is_some()
        || include.relation_context.is_some()
    {
        let _names = Span::new("protein-record-names-await");
        store::vocabulary::display_names(&store.pool, subject).await?
    } else {
        HashMap::new()
    };
    let mut work = if [
        "start_date",
        "due_date",
        "estimate_min",
        "work_logs",
        "spent_seconds",
        "running_since",
    ]
    .iter()
    .any(|key| wants(key))
    {
        let _work = Span::new("protein-record-work-await");
        let selected: Vec<_> = rows.iter().map(|record| record.uid.clone()).collect();
        store::records::extensions_for_records(&store.pool, "work", &selected).await?
    } else {
        HashMap::new()
    };
    let mut projected =
        record_schema::attach(store, protein, &rows, visible, &mut work, &concept_names).await?;
    let mut shader_effects = HashMap::new();
    let mut numeric_values = shader_inputs::attach(store, &rows, &include.numeric_fields).await?;
    if include.shader_effect && !rows.is_empty() {
        let mut query = store::sqlx::QueryBuilder::new(
            "SELECT record_uid, fds FROM record_extension WHERE namespace = ",
        );
        query
            .push_bind(nucleus::shader_effect::NAMESPACE)
            .push(" AND record_uid IN (");
        let mut ids = query.separated(",");
        for record in rows.iter().take(256) {
            ids.push_bind(&record.uid);
        }
        ids.push_unseparated(")");
        for (uid, raw) in query
            .build_query_as::<(String, String)>()
            .fetch_all(&store.pool)
            .await?
        {
            if raw.len() <= 196_608
                && let Ok(value) = from_str::<Value>(&raw)
            {
                shader_effects.insert(uid, value);
            }
        }
    }
    let mut relation_context = if let Some(include) = &include.relation_context {
        relation_context::attach(store, &rows, visible, include, &concept_names, subject).await?
    } else {
        HashMap::new()
    };
    let protected_transfers = if include.facts.is_some() || include.promises.is_some() {
        protected_transfer_history(store, subject).await?
    } else {
        HashSet::new()
    };
    let mut availability = if include.availability {
        record_availability(store, &ids).await?
    } else {
        HashMap::new()
    };
    let mut prepared_includes = record_includes::Prepared::load(store, &ids, &include).await?;
    for r in rows {
        let fields = Span::new("protein-record-field-publish");
        let concept_name = wants("concept_name").then(|| {
            r.identity_predicate_uid
                .as_ref()
                .and_then(|uid| concept_names.get(uid))
                .map(String::as_str)
        });
        let mut row = Value::from_iter([("uid", r.uid.as_str())]);
        if wants("slug") {
            row["slug"] = r.slug.into();
        }
        row["kind"] = Value::String(r.kind);
        if wants("head") {
            row["head"] = Value::String(r.head);
        }
        if wants("body") {
            row["body"] = Value::String(String::from_loaded(r.body)?);
        }
        if wants("quantity") {
            row["quantity"] = Value::String(r.quantity.to_string());
        }
        if wants("concept") {
            row["concept"] = r.identity_predicate_uid.into();
        }
        if wants("unit") {
            row["unit"] = r.unit_uid.into();
        }
        if wants("organ") {
            row["organ"] = r.organ_uid.into();
        }
        if let Some(name) = concept_name {
            row["concept_name"] = name.into();
        }
        if wants("created_at") {
            row["created_at"] = Value::String(r.created_at);
        }
        if wants("updated_at") {
            row["updated_at"] = Value::String(r.updated_at);
        }
        if wants("created_hlc") {
            row["created_hlc"] = r.created_hlc.into();
        }
        for (field, metadata) in [("start_date", "start"), ("due_date", "due")] {
            if wants(field) {
                row[field] = work
                    .get_mut(&r.uid)
                    .and_then(|value| value.get_mut(metadata))
                    .filter(|value| value.is_string())
                    .map_or(Value::Null, Value::take);
            }
        }
        drop(fields);
        add("protein-record-owned-field-rows", 1);
        if let Some((settings, graph)) = &links {
            row["links"] = Value::Array(graph.for_record(&r.uid, settings, &concept_names));
        }
        if let Some((settings, threads)) = &threads {
            row["threads"] = Value::Array(threads.for_record(store, &r.uid, settings).await?);
        }
        extend_fields(&mut row, availability.remove(&r.uid));
        prepared_includes.attach(&mut row, &r.uid, &include, &protected_transfers);
        extend_fields(&mut row, projected.remove(&r.uid));
        if let Some(context) = relation_context.remove(&r.uid) {
            row["relation_context"] = context;
        }
        if include.shader_effect {
            row["shader_effect"] = shader_effects.remove(&r.uid).unwrap_or(Value::Null);
        }
        if !include.numeric_fields.is_empty() {
            row["numeric_values"] = numeric_values
                .remove(&r.uid)
                .unwrap_or_else(|| Map::new().into());
        }
        if requested.is_some()
            && let Some(row) = row.as_object_mut()
        {
            row.retain(|key, _| matches!(key.as_str(), "uid" | "kind") || wants(key));
        }
        out.push(row);
    }
    Ok(out)
}

fn extend_fields(row: &mut Value, fields: Option<Value>) {
    if let (Some(row), Some(Value::Object(fields))) = (row.as_object_mut(), fields) {
        row.extend(fields);
    }
}

pub(crate) async fn record_availability(
    store: &Store,
    record_uids: &[&str],
) -> Result<HashMap<String, Value>, ProteinError> {
    Ok(read_balances(&store.pool, record_uids)
        .await?
        .into_iter()
        .map(|(uid, balance)| {
            let surplus = balance.surplus.to_f64();
            (
                uid,
                Value::from_iter([
                    ("actual", Value::from(balance.actual.to_f64())),
                    ("reserved", balance.reserved.to_f64().into()),
                    ("available", surplus.into()),
                    ("surplus", surplus.into()),
                    ("can_offer", balance.can_offer.to_f64().into()),
                    ("actual_exact", balance.actual.to_string().into()),
                    ("reserved_exact", balance.reserved.to_string().into()),
                    ("surplus_exact", balance.surplus.to_string().into()),
                    ("can_offer_exact", balance.can_offer.to_string().into()),
                    (
                        "availability_complete",
                        balance.incomplete.is_empty().into(),
                    ),
                    (
                        "availability_incomplete_count",
                        balance.incomplete.len().into(),
                    ),
                    ("scenario_required", true.into()),
                ]),
            )
        })
        .collect())
}

pub(crate) fn resolve_at(value: &str) -> Option<String> {
    if let Some(duration) = value.strip_prefix('+') {
        let duration = duration.trim();
        // A supported suffix is one byte, as required by parse_duration.
        if !duration.ends_with(['s', 'm', 'h', 'd']) {
            return None;
        }
        let delta = TimeDelta::try_seconds(parse_duration(duration)?)?;
        return Some(Utc::now().checked_add_signed(delta)?.to_rfc3339());
    }
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|instant| instant.to_rfc3339())
}
