// non-lean-yet
// Scope: enum AutomationStage, const READINGS, fn is_reading, fn is_agreement_reading.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 74, 108, 119,
//   124, 137.
// - [Clippy: clippy::as_conversions] Use From or TryFrom when it preserves the conversion contract.
//   Sites 89.
// - [Clippy: clippy::exhaustive_enums] Review whether public API evolution requires non_exhaustive.
//   Sites 11.
// - [Clippy: clippy::missing_errors_doc] State the observable failure cases for exported functions.
//   Sites 59, 156.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 11, 13,
//   14, 15, 16, 19, 29, 33, 42, 43, 44, 49, 50, 51, 52, 53, 54, 55, 59, 96, 97, 98, 99, 100, 101,
//   102, 106, 117, 128, 144, 156.
// - [review: §5 Errors] If exported, use a typed error with preserved causes. Sites validate:
//   String (59, 156).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::AgreementGuard;

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum AutomationStage {
    #[default]
    Draft,
    Publish,
    Commit,
    Activate,
}

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
        validate_evidence(&self.transfer, &self.participants)
    }
}

fn validate_evidence(
    transfer: &str,
    participants: &BTreeMap<String, Participant>,
) -> Result<(), String> {
    use crate::karma::{ReferenceKind, TimestampMs, TypedUid};
    let _validation = crate::performance::Span::new("transfer-automation-evidence-validation");
    crate::performance::add("transfer-automation-evidence-validations", 1);
    TypedUid::new(ReferenceKind::Transfer, transfer.to_owned())
        .map_err(|error| error.to_string())?;
    for (person, state) in participants {
        TypedUid::new(ReferenceKind::Person, person.clone()).map_err(|error| error.to_string())?;
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
    crate::performance::add(
        "transfer-automation-participants-validated",
        participants.len() as u64,
    );
    Ok(())
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
        validate_evidence(&self.transfer, &self.participants)
    }
}
