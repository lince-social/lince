use serde::{Deserialize, Serialize};

pub const DEFAULT_VOCABULARY_UID: &str = "g_01JA0000000000000000000000";
pub const TOPOS_UID: &str = "c_01JA0000000000000000000001";
pub const SAVED_LOCATION_ROLE_UID: &str = "vr_01JA0000000000000000000002";

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    SetLanguage {
        language: String,
    },
    CreateConcept {
        vocabulary: String,
        name: String,
        language: String,
        definition: String,
        derived_from: Option<String>,
    },
    AddTerm {
        concept: String,
        name: String,
        language: String,
        #[serde(default)]
        preferred: bool,
        #[serde(default)]
        personal: bool,
    },
    EditTerm {
        term: String,
        expected_revision: i64,
        name: String,
        language: String,
        preferred: bool,
    },
    RetireTerm {
        term: String,
        expected_revision: i64,
    },
    BindLocationConcept {
        concept: String,
        enabled: bool,
        #[serde(default)]
        write: bool,
        expected_revision: i64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Term {
    pub uid: String,
    pub concept_uid: String,
    pub language: String,
    pub name: String,
    pub owner_uid: Option<String>,
    pub preferred: bool,
    pub retired: bool,
    pub revision: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    pub concept_uid: String,
    pub vocabulary_uid: String,
    pub original_name: String,
    pub original_language: String,
    pub definition: String,
    pub derived_from: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dictionary {
    pub source: Source,
    pub terms: Vec<Term>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocationBinding {
    pub write_concept_uid: String,
    pub concept_uids: Vec<String>,
    pub revision: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub enum SavedLocationCommand {
    Inspect {
        record: String,
    },
    Observe {
        record: String,
        node_id: String,
    },
    Search {
        text: String,
    },
    Bind {
        record: String,
        address: String,
    },
    Select {
        record: String,
        assertion: Option<String>,
        expected_revision: i64,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocationCandidate {
    pub assertion_uid: Option<String>,
    pub record_uid: String,
    pub head: String,
    pub slug: Option<String>,
    pub latitude: f64,
    pub longitude: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub radius_metres: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedLocation {
    pub record_uid: String,
    pub candidates: Vec<LocationCandidate>,
    pub selected: Option<LocationCandidate>,
    pub explicit: bool,
    pub unavailable: bool,
    pub revision: i64,
}
