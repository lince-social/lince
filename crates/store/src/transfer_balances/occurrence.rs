//! Translate occurrence rows and delivered terms into the same typed projection.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use nucleus::DecimalValue;
use serde_json::{Value, to_string};
use sqlx::{Row, SqliteConnection};

use super::{invalid, promise::Promise};
use crate::{StoreError, exact, transfer_accounting as accounting, transfer_cancellations};

pub(super) struct Occurrence {
    pub(super) uid: String,
    pub(super) quantity: f64,
    pub(super) due: Option<String>,
    promise: Option<String>,
    exchange: Option<String>,
    path: Option<String>,
    progress: Value,
}

impl Occurrence {
    pub(super) fn from_remote(value: Value) -> Option<Self> {
        let Value::Object(mut fields) = value else {
            return None;
        };
        let uid = take_string(fields.remove("uid"))?;
        Some(Self {
            uid,
            quantity: fields
                .get("quantity")
                .and_then(Value::as_f64)
                .unwrap_or(0.0),
            due: take_string(fields.remove("window_end")),
            promise: take_string(fields.remove("promise")),
            exchange: take_string(fields.remove("exchange")),
            path: take_string(fields.remove("exchange_path")),
            progress: fields.remove("settlement_progress").unwrap_or_default(),
        })
    }

    fn cancelled(&self) -> Result<DecimalValue, StoreError> {
        if let Some(value) = self.progress["cancelled_quantity_exact"].as_str() {
            DecimalValue::parse_inferred(value).map_err(invalid)
        } else {
            accounting::amount(self.progress["cancelled_quantity"].as_f64().unwrap_or(0.0))
        }
    }
}

fn take_string(value: Option<Value>) -> Option<String> {
    match value {
        Some(Value::String(value)) => Some(value),
        _ => None,
    }
}

#[derive(Default)]
pub(super) struct Occurrences(HashMap<String, HashMap<String, Vec<Occurrence>>>);

impl Occurrences {
    pub(super) async fn load(
        connection: &mut SqliteConnection,
        selected: BTreeSet<(&str, &str)>,
    ) -> Result<Self, StoreError> {
        let mut occurrences = Self::default();
        if selected.is_empty() {
            return Ok(occurrences);
        }
        let ids = to_string(&selected).map_err(invalid)?;
        let rows = sqlx::query(
            "SELECT o.transfer_uid, o.uid, o.promise_uid, o.quantity, o.window_end, o.exchange_path_uid,
                    COALESCE(path.public_exchange_uid, o.promise_uid) AS exchange_uid
             FROM json_each(?) selected
             JOIN transfer_occurrence o ON o.transfer_uid = json_extract(selected.value, '$[0]')
             JOIN transfer_exchange_path path ON path.uid = o.exchange_path_uid
                AND COALESCE(path.public_exchange_uid, o.promise_uid) = json_extract(selected.value, '$[1]')
             ORDER BY o.created_at, o.uid",
        )
        .bind(ids)
        .fetch_all(connection)
        .await?;
        for row in rows {
            let exchange: String = row.get("exchange_uid");
            let quantity: f64 = row.get("quantity");
            let occurrence = Occurrence {
                uid: row.get("uid"),
                // The former JSON projection represented non-finite quantities as null.
                quantity: if quantity.is_finite() { quantity } else { 0.0 },
                due: row.get("window_end"),
                promise: row.get("promise_uid"),
                exchange: Some(exchange.clone()),
                path: row.get("exchange_path_uid"),
                progress: Value::Null,
            };
            occurrences
                .0
                .entry(row.get("transfer_uid"))
                .or_default()
                .entry(exchange)
                .or_default()
                .push(occurrence);
        }
        Ok(occurrences)
    }

    pub(super) fn paths<'a>(&'a self, promise: &'a Promise) -> BTreeMap<&'a str, &'a Occurrence> {
        let occurrences = promise.remote_occurrences.as_deref().unwrap_or_else(|| {
            self.0
                .get(&promise.transfer)
                .and_then(|exchanges| exchanges.get(&promise.exchange))
                .map_or(&[], Vec::as_slice)
        });
        let mut paths = BTreeMap::new();
        for occurrence in occurrences {
            let own_promise = occurrence.promise.as_deref() == Some(promise.uid.as_str());
            if own_promise || occurrence.exchange.as_deref() == Some(promise.exchange.as_str()) {
                let path = occurrence.path.as_deref().unwrap_or(&occurrence.uid);
                if own_promise {
                    paths.insert(path, occurrence);
                } else {
                    paths.entry(path).or_insert(occurrence);
                }
            }
        }
        paths
    }
}

pub(super) async fn cancelled_on(
    connection: &mut SqliteConnection,
    promise: &Promise,
    occurrence: Option<&str>,
) -> Result<DecimalValue, StoreError> {
    let Some(uid) = occurrence else {
        return Ok(exact::zero());
    };
    if let Some(occurrences) = &promise.remote_occurrences {
        return occurrences
            .iter()
            .find(|occurrence| occurrence.uid == uid)
            .map_or_else(|| Ok(exact::zero()), Occurrence::cancelled);
    }
    transfer_cancellations::cancelled_on(connection, uid).await
}
