//! Read local Promises and reconcile delivered revisions in projection order.

use std::{collections::HashMap, mem::take, sync::Arc};

use serde_json::{Value, from_str};
use sqlx::{Row, SqliteConnection};

use super::{invalid, occurrence::Occurrence};
use crate::StoreError;

pub(super) struct Promise {
    pub(super) transfer: String,
    pub(super) title: String,
    pub(super) revision: u64,
    pub(super) uid: String,
    pub(super) exchange: String,
    pub(super) owner: String,
    pub(super) record: Option<String>,
    pub(super) quantity: f64,
    pub(super) unit: Option<String>,
    pub(super) state: String,
    pub(super) reserve_from: String,
    pub(super) giver: Option<String>,
    pub(super) receiver: Option<String>,
    pub(super) due: Option<String>,
    pub(super) alternative: Option<String>,
    pub(super) remote_occurrences: Option<Arc<[Occurrence]>>,
    pub(super) unavailable: bool,
}

impl Promise {
    pub(super) fn includes(&self, person: &str) -> bool {
        self.owner == person
            || [self.giver.as_deref(), self.receiver.as_deref()].contains(&Some(person))
    }

    pub(super) fn has_commitment(&self) -> bool {
        !matches!(
            self.state.as_str(),
            "open" | "withdrawn" | "broken" | "expired"
        )
    }

    pub(super) fn outgoing(&self, person: &str) -> bool {
        self.giver
            .as_deref()
            .map_or(self.quantity < 0.0, |giver| giver == person)
    }

    pub(super) fn reserves(&self) -> bool {
        match self.reserve_from.as_str() {
            "proposed" => matches!(
                self.state.as_str(),
                "proposed" | "agreed" | "active" | "kept"
            ),
            "agreed" => matches!(self.state.as_str(), "agreed" | "active" | "kept"),
            "active" => matches!(self.state.as_str(), "active" | "kept"),
            _ => false,
        }
    }
}

fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key)?.as_str()
}

fn owned(value: &Value, key: &str) -> Option<String> {
    text(value, key).map(str::to_owned)
}

pub(super) async fn load(connection: &mut SqliteConnection) -> Result<Vec<Promise>, StoreError> {
    let rows = sqlx::query(
        "SELECT p.uid, p.item_json, p.transfer_uid, p.party_uid, p.record_uid, p.delta, p.unit_uid,
                p.state, p.reserve_from, p.window_end, COALESCE(t.revision, 0) AS transfer_revision,
                t.source_uid AS alternative_source, t.satiation, tr.head AS transfer_head
         FROM promise p LEFT JOIN transfer t ON t.record_uid = p.transfer_uid
         LEFT JOIN record tr ON tr.uid = t.record_uid
         WHERE NOT EXISTS (
             SELECT 1 FROM transfer_source_group_result done
             WHERE done.source_uid = t.source_uid AND done.policy = t.satiation
               AND done.transfer_uid != t.record_uid AND p.state NOT IN ('active', 'kept'))",
    )
    .fetch_all(&mut *connection)
    .await?;
    let promises: Vec<_> = rows
        .into_iter()
        .map(|row| {
            let item: Value = row
                .get::<Option<&str>, _>("item_json")
                .and_then(|item| from_str(item).ok())
                .unwrap_or_default();
            let exchange = item.get("exchange").unwrap_or(&Value::Null);
            let uid: String = row.get("uid");
            Promise {
                transfer: row
                    .get::<Option<String>, _>("transfer_uid")
                    .unwrap_or_default(),
                title: text(&item, "title")
                    .or_else(|| row.get("transfer_head"))
                    .unwrap_or("Commitment")
                    .into(),
                revision: row.get::<i64, _>("transfer_revision").cast_unsigned(),
                exchange: text(exchange, "uid").unwrap_or(&uid).into(),
                uid,
                owner: row
                    .get::<Option<String>, _>("party_uid")
                    .unwrap_or_default(),
                record: row.get("record_uid"),
                quantity: row.get("delta"),
                unit: row.get("unit_uid"),
                state: row.get("state"),
                reserve_from: row.get("reserve_from"),
                giver: owned(exchange, "giver"),
                receiver: owned(exchange, "receiver"),
                due: row.get("window_end"),
                alternative: (row.get::<Option<&str>, _>("satiation") == Some("first_completes"))
                    .then(|| row.get("alternative_source"))
                    .flatten(),
                remote_occurrences: None,
                unavailable: false,
            }
        })
        .collect();
    let references = sqlx::query(
        "SELECT projection, state FROM transfer_remote_reference
         WHERE projection IS NOT NULL AND recipient_organ_uid =
               (SELECT uid FROM record WHERE slug = 'local-organ' AND kind = 'organ')
         ORDER BY last_transfer_revision",
    )
    .fetch_all(connection)
    .await?;
    if references.is_empty() {
        return Ok(promises);
    }

    let mut indices = HashMap::<String, HashMap<String, usize>>::new();
    let mut promises: Vec<_> = promises
        .into_iter()
        .enumerate()
        .map(|(index, promise)| {
            indices
                .entry(promise.transfer.clone())
                .or_default()
                .insert(promise.uid.clone(), index);
            Some(promise)
        })
        .collect();
    for reference in references {
        let mut projection: Value =
            from_str(reference.get::<&str, _>("projection")).map_err(invalid)?;
        let occurrences: Arc<[_]> = projection
            .get_mut("occurrences")
            .and_then(Value::as_array_mut)
            .map(|rows| {
                take(rows)
                    .into_iter()
                    .filter_map(Occurrence::from_remote)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
            .into();
        let transfer = text(&projection, "uid").unwrap_or_default();
        let revision = projection["revision"].as_u64().unwrap_or(0);
        let alternative = (projection["satiation"] == "first_completes")
            .then(|| text(&projection, "source"))
            .flatten();
        let unavailable = reference.get::<&str, _>("state") != "active";
        let satiated = projection["status"] == "satiated";
        let Some(rows) = projection.get("promises").and_then(Value::as_array) else {
            continue;
        };
        let indices = indices.entry(transfer.into()).or_default();
        for row in rows {
            let state = text(row, "state").unwrap_or_default();
            if satiated && !matches!(state, "active" | "kept") {
                continue;
            }
            let Some(uid) = text(row, "uid") else {
                continue;
            };
            if let Some(previous) = indices.get(uid).and_then(|&index| promises.get_mut(index)) {
                if previous
                    .as_ref()
                    .is_some_and(|current| current.revision >= revision)
                {
                    continue;
                }
                *previous = None;
            }
            indices.insert(uid.into(), promises.len());
            promises.push(Some(Promise {
                transfer: transfer.into(),
                title: text(row, "title")
                    .or_else(|| text(&row["item"], "title"))
                    .or_else(|| text(&projection, "head"))
                    .unwrap_or("Commitment")
                    .into(),
                revision,
                exchange: text(row, "exchange").unwrap_or(uid).into(),
                uid: uid.into(),
                owner: text(row, "party").unwrap_or_default().into(),
                record: owned(row, "record"),
                quantity: row["delta"].as_f64().unwrap_or(0.0),
                unit: owned(row, "unit"),
                state: state.into(),
                reserve_from: text(row, "reserve_from").unwrap_or("none").into(),
                giver: owned(row, "giver"),
                receiver: owned(row, "receiver"),
                due: owned(row, "window_end"),
                alternative: alternative.map(str::to_owned),
                remote_occurrences: Some(Arc::clone(&occurrences)),
                unavailable,
            }));
        }
    }
    Ok(promises.into_iter().flatten().collect())
}
