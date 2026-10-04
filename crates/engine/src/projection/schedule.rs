use crate::EngineError;
use nucleus::{
    karma::{ReferenceKind, TypedUid},
    projection::{Context, MAX_SPANS, Scheduled},
    simulation::{Cause, Quantity, RuleOccurrence},
};
use std::collections::{HashMap, HashSet};
use store::sqlx::{Row, SqlitePool};

fn entry(
    record: &store::records::RecordRow,
    work: &serde_json::Value,
    cause: Cause,
    occurrence: Option<i64>,
    preview: bool,
) -> Result<Option<Scheduled>, EngineError> {
    let Some(time) = nucleus::schedule::range(
        work["start"].as_str(),
        work["due"].as_str(),
        work["estimate_min"].as_f64(),
        occurrence,
    )
    .map_err(EngineError::Consequence)?
    else {
        return Ok(None);
    };
    let id = match &cause {
        Cause::Rule { occurrence, .. } => format!(
            "occurrence:{}:{}:{}:{}",
            occurrence.rule_uid, occurrence.revision, occurrence.event_id, record.uid
        ),
        _ => format!("manual:{}", record.uid),
    };
    Ok(Some(Scheduled {
        id,
        record: TypedUid::new(ReferenceKind::Record, &record.uid).map_err(super::boundary)?,
        time,
        quantity: Quantity {
            value: record.quantity,
            unit: record
                .unit_uid
                .as_ref()
                .map(|uid| TypedUid::new(ReferenceKind::Unit, uid))
                .transpose()
                .map_err(super::boundary)?,
        },
        cause,
        head: record.head.chars().take(4096).collect(),
        slug: record.slug.clone(),
        record_kind: record.kind.clone(),
        preview,
    }))
}

pub(super) async fn manual(
    pool: &SqlitePool,
    context: &Context,
    existing: &HashSet<String>,
    entries: &mut Vec<Scheduled>,
    origins: &HashMap<String, Cause>,
) -> Result<(), EngineError> {
    let work = store::records::all_extensions(pool, "work").await?;
    let mut ids: HashSet<_> = entries.iter().map(|entry| entry.id.clone()).collect();
    for record in store::records::list_all(pool).await? {
        if entries.len() >= MAX_SPANS {
            break;
        }
        let Some(work) = work.get(&record.uid) else {
            continue;
        };
        if let Some(entry) = entry(
            &record,
            work,
            origins.get(&record.uid).cloned().unwrap_or(Cause::Seed {}),
            None,
            !existing.contains(&record.uid),
        )? && entry
            .time
            .overlaps(context.window.from_ms, context.window.until_ms)
            && ids.insert(entry.id.clone())
        {
            entries.push(entry);
        }
    }
    Ok(())
}

pub(super) async fn occurrences(
    pool: &SqlitePool,
    context: &Context,
    position: i64,
    existing: &HashSet<String>,
    entries: &mut Vec<Scheduled>,
) -> Result<i64, EngineError> {
    let rows = store::sqlx::query("SELECT a.rowid AS position, a.event_id, a.rule_uid, a.rule_revision, a.intended_at, a.frequency_uid, r.record_uid FROM karma_rule_application a JOIN recurrence r ON r.uid = a.rule_uid WHERE a.rowid > ? AND a.status = 'applied' ORDER BY a.rowid LIMIT ?")
        .bind(position).bind(MAX_SPANS as i64).fetch_all(pool).await?;
    let mut position = position;
    for row in rows {
        position = row.get("position");
        if entries.len() >= MAX_SPANS {
            break;
        }
        let record_uid: String = row.get("record_uid");
        let Some(record) = store::records::get(pool, &record_uid).await? else {
            continue;
        };
        let intended: String = row.get("intended_at");
        let at = chrono::DateTime::parse_from_rfc3339(&intended)
            .map_err(|error| EngineError::Consequence(error.to_string()))?
            .timestamp_millis();
        let frequency: Option<String> = row.get("frequency_uid");
        let occurrence = RuleOccurrence {
            rule_uid: row.get("rule_uid"),
            revision: u64::try_from(row.get::<i64, _>("rule_revision"))
                .map_err(|_| EngineError::Consequence("Negative rule revision".into()))?,
            event_id: row.get("event_id"),
            frequency: frequency
                .map(|uid| TypedUid::new(ReferenceKind::Frequency, uid))
                .transpose()
                .map_err(super::boundary)?,
            intended_at_ms: Some(at),
        };
        let work = store::records::get_extension(pool, &record_uid, "work")
            .await?
            .unwrap_or_else(|| serde_json::json!({}));
        if let Some(entry) = entry(
            &record,
            &work,
            Cause::Rule {
                occurrence,
                consequence: 0,
            },
            Some(at),
            !existing.contains(&record_uid),
        )? && entry
            .time
            .overlaps(context.window.from_ms, context.window.until_ms)
        {
            entries.push(entry);
        }
    }
    Ok(position)
}
