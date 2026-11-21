use std::{
    collections::{HashMap, HashSet},
    sync::OnceLock,
};

use nucleus::{
    performance::{Span, add},
    projection::OccurrenceLink,
    simulation::RuleOccurrence,
};
use serde::Deserialize;
use serde_json::Value;

struct Link<'a> {
    value: &'a Value,
    parsed: OnceLock<Option<OccurrenceLink>>,
}

impl Link<'_> {
    fn get(&self) -> Option<&OccurrenceLink> {
        self.parsed
            .get_or_init(|| {
                add("protein-schedule-link-decodes", 1);
                OccurrenceLink::deserialize(self.value).ok()
            })
            .as_ref()
    }
}

pub(super) struct Work<'a> {
    rows: &'a HashMap<String, Value>,
    links: OnceLock<HashMap<&'a str, Link<'a>>>,
}

impl<'a> Work<'a> {
    pub(super) fn new(rows: &'a HashMap<String, Value>) -> Self {
        Self {
            rows,
            links: OnceLock::new(),
        }
    }

    pub(super) fn row(&self, uid: &str) -> Option<(&str, &Value)> {
        self.rows
            .get_key_value(uid)
            .map(|(uid, value)| (uid.as_str(), value))
    }

    fn links(&self) -> &HashMap<&'a str, Link<'a>> {
        self.links.get_or_init(|| {
            let _prepare = Span::new("protein-schedule-link-catalog");
            let links: HashMap<_, _> = self
                .rows
                .iter()
                .filter_map(|(uid, work)| {
                    Some((
                        uid.as_str(),
                        Link {
                            value: work.get("projection_occurrence")?,
                            parsed: OnceLock::new(),
                        },
                    ))
                })
                .collect();
            add(
                "protein-schedule-link-values",
                u64::try_from(links.len()).unwrap_or(u64::MAX),
            );
            links
        })
    }

    pub(super) fn occurrence(&self, uid: &str) -> Option<&OccurrenceLink> {
        self.rows.get(uid)?.get("projection_occurrence")?;
        self.links().get(uid)?.get()
    }
}

type Identity<'a> = (&'a str, &'a str, u64, &'a str);

fn identity<'a>(record: &'a str, occurrence: &'a RuleOccurrence) -> Identity<'a> {
    (
        record,
        &occurrence.rule_uid,
        occurrence.revision,
        &occurrence.event_id,
    )
}

pub(super) struct Materialized<'a, 'work> {
    work: &'a Work<'work>,
    visible: Option<&'a HashSet<String>>,
    identities: OnceLock<HashSet<Identity<'a>>>,
}

impl<'a, 'work> Materialized<'a, 'work> {
    pub(super) fn new(work: &'a Work<'work>, visible: Option<&'a HashSet<String>>) -> Self {
        Self {
            work,
            visible,
            identities: OnceLock::new(),
        }
    }

    pub(super) fn contains(&self, record: &str, occurrence: &RuleOccurrence) -> bool {
        let keys = self.identities.get_or_init(|| {
            let _prepare = Span::new("protein-schedule-materialized-index");
            let work: &'a Work<'work> = self.work;
            work.links()
                .iter()
                .filter(|(uid, _)| self.visible.is_none_or(|visible| visible.contains(**uid)))
                .filter_map(|(_, link)| link.get())
                .map(|link| identity(link.record.as_str(), &link.occurrence))
                .collect()
        });
        add("protein-schedule-occurrence-lookups", 1);
        keys.contains(&identity(record, occurrence))
    }
}
