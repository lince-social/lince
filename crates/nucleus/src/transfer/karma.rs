use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::AgreementGuard;

pub const READINGS: [&str; 7] = [
    "agreement_level",
    "agreement_changed_at",
    "agreement_age",
    "transfer_revision",
    "transfer_active",
    "transfer_published",
    "transfer_ready",
];

pub fn is_reading(name: &str) -> bool {
    READINGS.contains(&name)
}

pub fn is_agreement_reading(name: &str) -> bool {
    matches!(
        name,
        "agreement_level" | "agreement_changed_at" | "agreement_age"
    )
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Participant {
    pub guard: AgreementGuard,
    pub changed_at_ms: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub transfer: String,
    pub revision: u64,
    pub active: bool,
    pub published: bool,
    pub ready: bool,
    pub participants: BTreeMap<String, Participant>,
}

impl Snapshot {
    pub fn validate(&self) -> Result<(), String> {
        use crate::karma::{ReferenceKind, TimestampMs, TypedUid};
        TypedUid::new(ReferenceKind::Transfer, self.transfer.clone())
            .map_err(|error| error.to_string())?;
        for (person, state) in &self.participants {
            TypedUid::new(ReferenceKind::Person, person.clone())
                .map_err(|error| error.to_string())?;
            if state.guard.level > 2 {
                return Err("Transfer agreement level is outside 0–2".into());
            }
            if let Some(at) = state.changed_at_ms {
                TimestampMs::from_millis(at).map_err(|error| error.to_string())?;
            }
            if state.guard.change_uid.as_ref().is_some_and(|uid| {
                uid.is_empty() || uid.len() > 200 || uid.chars().any(char::is_control)
            }) {
                return Err("Transfer agreement change identity is invalid".into());
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Guard {
    pub transfer: String,
    pub revision: u64,
    pub active: Option<bool>,
    pub published: Option<bool>,
    pub ready: Option<bool>,
    pub participants: BTreeMap<String, Participant>,
}

impl Guard {
    pub fn base(state: &Snapshot) -> Self {
        Self {
            transfer: state.transfer.clone(),
            revision: state.revision,
            active: None,
            published: None,
            ready: None,
            participants: BTreeMap::new(),
        }
    }

    pub fn exact(state: &Snapshot) -> Self {
        Self {
            transfer: state.transfer.clone(),
            revision: state.revision,
            active: Some(state.active),
            published: Some(state.published),
            ready: Some(state.ready),
            participants: state.participants.clone(),
        }
    }

    pub fn observe(&mut self, state: &Snapshot, reading: &str, person: Option<&str>) {
        match reading {
            "transfer_active" => self.active = Some(state.active),
            "transfer_published" => self.published = Some(state.published),
            "transfer_ready" => self.ready = Some(state.ready),
            name if is_agreement_reading(name) => {
                if let Some(person) = person
                    && let Some(state) = state.participants.get(person)
                {
                    self.participants.insert(person.into(), state.clone());
                }
            }
            _ => {}
        }
    }

    pub fn matches(&self, state: &Snapshot) -> bool {
        self.transfer == state.transfer
            && self.revision == state.revision
            && self.active.is_none_or(|value| state.active == value)
            && self.published.is_none_or(|value| state.published == value)
            && self.ready.is_none_or(|value| state.ready == value)
            && self
                .participants
                .iter()
                .all(|(person, original)| state.participants.get(person) == Some(original))
    }

    pub fn validate(&self) -> Result<(), String> {
        Snapshot {
            transfer: self.transfer.clone(),
            revision: self.revision,
            active: false,
            published: false,
            ready: false,
            participants: self.participants.clone(),
        }
        .validate()
    }
}
