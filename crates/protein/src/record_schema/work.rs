use std::collections::HashMap;

use chrono::{DateTime, Utc};
use nucleus::performance::Span;
use serde_json::{Map, Value};
use store::{
    Store,
    records::RecordRow,
    sqlx::{Error::Protocol, QueryBuilder},
};

use crate::{Protein, ProteinError};

use super::requests;

pub(super) struct WorkFields {
    estimate: bool,
    logs: bool,
    spent: bool,
    running: bool,
}

impl WorkFields {
    pub(super) fn selected(query: &Protein) -> Self {
        Self {
            estimate: requests(query, "estimate_min"),
            logs: requests(query, "work_logs"),
            spent: requests(query, "spent_seconds"),
            running: requests(query, "running_since"),
        }
    }

    pub(super) fn any(&self) -> bool {
        self.estimate || self.logs || self.spent || self.running
    }

    pub(super) async fn log_ids<Body>(
        &self,
        store: &Store,
        records: &[RecordRow<Body>],
        work: &HashMap<String, Value>,
    ) -> Result<HashMap<String, Vec<String>>, ProteinError> {
        let mut ids = HashMap::<_, Vec<_>>::new();
        if !self.logs {
            return Ok(ids);
        }
        let owners = records
            .iter()
            .filter(|record| {
                work.get(&record.uid)
                    .and_then(|metadata| metadata.get("logs"))
                    .and_then(Value::as_array)
                    .is_some_and(|logs| !logs.is_empty())
            })
            .map(|record| record.uid.as_str())
            .collect::<Vec<_>>();
        for owners in owners.chunks(512) {
            let _ids = Span::new("protein-work-log-ids-await");
            let mut query = QueryBuilder::new(
                "SELECT record_uid, property FROM record_property WHERE record_uid IN (",
            );
            let mut arguments = query.separated(",");
            for &uid in owners {
                arguments.push_bind(uid);
            }
            arguments.push_unseparated(")");
            query.push(" AND property LIKE 'work.log:%' AND json_type(value) = 'object' ORDER BY record_uid, json_extract(value, '$.start'), property");
            for (uid, property) in query
                .build_query_as::<(String, String)>()
                .fetch_all(&store.pool)
                .await?
            {
                ids.entry(uid).or_default().push(property);
            }
        }
        Ok(ids)
    }

    pub(super) fn append(
        &self,
        output: &mut Map<String, Value>,
        metadata: Option<&mut Value>,
        ids: Option<Vec<String>>,
    ) -> Result<(), ProteinError> {
        let mut metadata = metadata.and_then(Value::as_object_mut);
        if self.estimate {
            output.insert(
                "estimate_min".into(),
                metadata
                    .as_mut()
                    .and_then(|work| work.remove("estimate_min"))
                    .unwrap_or(Value::Null),
            );
        }
        if !(self.logs || self.spent || self.running) {
            return Ok(());
        }
        let mut logs = match metadata.and_then(|work| work.remove("logs")) {
            Some(Value::Array(logs)) => logs,
            _ => Vec::new(),
        };
        if self.logs {
            let mut ids = ids.into_iter().flatten();
            for (index, log) in logs.iter_mut().enumerate() {
                if log.is_null() {
                    *log = Value::Object(Map::new());
                }
                let log = log
                    .as_object_mut()
                    .ok_or_else(|| Protocol("Record work log must be an object".into()))?;
                log.insert(
                    "id".into(),
                    ids.next()
                        .unwrap_or_else(|| format!("work.log:seed-{index}"))
                        .into(),
                );
            }
        }
        let (mut spent, mut running) = (0i64, None);
        if self.spent || self.running {
            for (index, log) in logs.iter().enumerate() {
                let Some(start) = instant(&log["start"]) else {
                    continue;
                };
                let end = instant(&log["end"]);
                if self.running && end.is_none() {
                    running = Some(index);
                }
                if self.spent {
                    let end = end.map_or_else(Utc::now, |end| end.with_timezone(&Utc));
                    spent = spent
                        .saturating_add((end - start.with_timezone(&Utc)).num_seconds().max(0));
                }
            }
        }
        let running = running
            .and_then(|index| logs.get_mut(index)?.get_mut("start"))
            .map(|start| {
                if self.logs {
                    start.clone()
                } else {
                    start.take()
                }
            });
        if self.logs {
            output.insert("work_logs".into(), Value::Array(logs));
        }
        if self.spent {
            output.insert("spent_seconds".into(), spent.into());
        }
        if self.running {
            output.insert("running_since".into(), running.unwrap_or(Value::Null));
        }
        Ok(())
    }
}

fn instant(value: &Value) -> Option<DateTime<chrono::FixedOffset>> {
    DateTime::parse_from_rfc3339(value.as_str()?).ok()
}
