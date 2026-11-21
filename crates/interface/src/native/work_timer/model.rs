// non-lean-yet

pub(super) use crate::work_timer_logs::{Entry, Logs};
use bevy::prelude::Component;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Component, Clone, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct LocalTimer {
    pub(super) logs: Vec<Entry>,
}

impl LocalTimer {
    pub(crate) fn valid(&self) -> bool {
        let ids: std::collections::HashSet<_> = self.logs.iter().map(|entry| &entry.id).collect();
        ids.len() == self.logs.len()
            && self
                .logs
                .iter()
                .all(|entry| entry.id.starts_with("work.log:") && entry.id.len() <= 160)
            && self.logs.iter().filter(|entry| entry.end.is_none()).count() <= 1
            && engine::private_work::WorkMetadata::validate(
                &json!({"logs":self.logs.iter().map(Entry::value).collect::<Vec<_>>()}),
            )
            .is_ok()
    }

    pub(super) fn change(&mut self, id: &str, value: Option<Entry>) -> Result<(), String> {
        let mut next = self.clone();
        next.logs.retain(|entry| entry.id != id);
        if let Some(value) = value {
            next.logs.push(value);
        }
        if !next.valid() {
            return Err("Use timestamps with a timezone, an end after the start, and at most one running entry.".into());
        }
        next.logs.sort_by_key(Entry::start_time);
        *self = next;
        Ok(())
    }

    pub(super) fn toggle(&mut self, now: DateTime<Utc>) -> Result<(), String> {
        if let Some(entry) = self.logs.iter().find(|entry| entry.end.is_none()) {
            let mut entry = entry.clone();
            entry.end = Some(
                now.max(entry.start_time().unwrap().with_timezone(&Utc))
                    .to_rfc3339(),
            );
            self.change(&entry.id.clone(), Some(entry))
        } else {
            let id = format!("work.log:{}", nucleus::new_uid("op"));
            self.change(
                &id,
                Some(Entry {
                    id: id.clone(),
                    start: now.to_rfc3339(),
                    end: None,
                }),
            )
        }
    }
}
