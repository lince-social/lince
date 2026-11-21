// non-lean-yet
// Scope: mod reference, const DEFAULT_VOCABULARY_UID, const TOPOS_UID, const
//   SAVED_LOCATION_ROLE_UID.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::exhaustive_enums] Review whether public API evolution requires non_exhaustive.
//   Sites 38, 275.
// - [Clippy: clippy::missing_errors_doc] State the observable failure cases for exported functions.
//   Sites 17.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 3, 5,
//   6, 7, 9, 17, 38, 39, 40, 41, 43, 44, 45, 46, 48, 49, 50, 52, 53, 55, 56, 58, 59, 61, 62, 63,
//   65, 66, 67, 69, 70, 72, 74, 75, 77, 78, 79, 81, 82, 84, 86, 87, 89, 91, 92, 94, 95, 97, 98,
//   100, 101, 103, 104, 105, 107, 108, 109, 111, 112, 113, 115, 116, 118, 119, 121, 122, 124, 125,
//   126, 127, 129, 131, 132, 133, 134, 136, 137, 139, 140, 141, 142, 143, 144, 146, 147, 148, 149,
//   151, 153, 155, 156, 157, 158, 159, 160, 162, 163, 164, 166, 167, 168, 170, 171, 176, 196, 197,
//   198, 199, 200, 201, 202, 203, 204, 209, 210, 211, 212, 213, 214, 215, 220, 221, 222, 224, 229,
//   230, 231, 232, 233, 238, 239, 240, 241, 242, 243, 244, 245, 250, 251, 252, 253, 258, 259, 260,
//   261, 262, 267, 268, 269, 270, 275, 276, 277, 279, 280, 281, 282, 284, 285, 287, 288, 289, 291,
//   292, 294, 295, 296, 298, 299, 300, 301, 306, 307, 308, 309, 310, 311, 312, 314, 318, 319, 320,
//   321, 322, 323, 324.

use serde::{Deserialize, Serialize};

pub mod reference;

pub const DEFAULT_VOCABULARY_UID: &str = "g_01JA0000000000000000000000";
pub const TOPOS_UID: &str = "c_01JA0000000000000000000001";
pub const SAVED_LOCATION_ROLE_UID: &str = "vr_01JA0000000000000000000002";

pub fn meaning_set(concepts: &[String]) -> String {
    let unique: std::collections::BTreeSet<_> = concepts.iter().map(String::as_str).collect();
    format!(
        "concepts:{}",
        unique.into_iter().collect::<Vec<_>>().join("|")
    )
}

pub fn parse_meaning_set(token: &str) -> Result<Option<Vec<String>>, &'static str> {
    let Some(value) = token.strip_prefix("concepts:") else {
        return Ok(None);
    };
    if value.len() > 128 * 29 {
        return Err("Choose at most 128 Concept identities");
    }
    let mut concepts = std::collections::BTreeSet::new();
    for uid in value.split('|') {
        if !crate::valid_uid(uid, "c") || !concepts.insert(uid.to_owned()) {
            return Err("Each selected meaning must have a distinct Concept identity");
        }
    }
    if concepts.is_empty() || concepts.len() > 128 {
        return Err("Choose 1–128 Concept identities");
    }
    Ok(Some(concepts.into_iter().collect()))
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    CreateVocabulary {
        name: String,
        organ: String,
    },
    ReviseDefinition {
        concept: String,
        expected_revision: i64,
        definition: String,
    },
    Publish {
        vocabulary: String,
        expected_revision: i64,
    },
    Withdraw {
        publication: String,
    },
    InspectPublication {
        publication: String,
    },
    PreviewPackage {
        package: String,
    },
    Adopt {
        publication: String,
        organ: String,
    },
    AdoptPackage {
        package: String,
        organ: String,
    },
    RecordHistory {
        record: String,
        #[serde(default)]
        offset: u32,
    },
    SetLanguage {
        language: String,
    },
    InspectTerminology,
    BindQuery {
        query: String,
    },
    InspectMeaning {
        concept: String,
        #[serde(default)]
        offset: u32,
    },
    InspectMeasurement {
        concept: String,
        #[serde(default)]
        unit: Option<String>,
    },
    ExportTranslations {
        vocabulary: String,
    },
    PreviewTranslations {
        package: String,
    },
    ImportTranslations {
        package: String,
    },
    SetTerminologyContext {
        context: String,
    },
    PreferTerm {
        concept: String,
        term: String,
    },
    UseWording {
        concept: String,
        wording: String,
    },
    HideTerm {
        term: String,
        hidden: bool,
    },
    RestoreOriginal {
        concept: String,
    },
    ResetTerminology {
        concept: String,
    },
    InspectFeature {
        feature: String,
    },
    PreviewFeature {
        feature: String,
        concept: String,
        enabled: bool,
        #[serde(default)]
        write: bool,
    },
    BindFeature {
        feature: String,
        concept: String,
        enabled: bool,
        #[serde(default)]
        write: bool,
        expected_revision: i64,
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

impl Command {
    pub fn is_mutation(&self) -> bool {
        !matches!(
            self,
            Self::InspectPublication { .. }
                | Self::PreviewPackage { .. }
                | Self::RecordHistory { .. }
                | Self::InspectTerminology
                | Self::BindQuery { .. }
                | Self::InspectMeaning { .. }
                | Self::InspectMeasurement { .. }
                | Self::ExportTranslations { .. }
                | Self::PreviewTranslations { .. }
                | Self::InspectFeature { .. }
                | Self::PreviewFeature { .. }
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub concept_uid: String,
    pub vocabulary_uid: String,
    pub original_name: String,
    pub original_language: String,
    pub definition: String,
    pub derived_from: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Dictionary {
    pub source: Source,
    pub terms: Vec<Term>,
    #[serde(default)]
    pub definitions: Vec<DefinitionVersion>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DefinitionVersion {
    pub concept_uid: String,
    pub revision: i64,
    pub definition: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Publication {
    pub format: u32,
    pub uid: String,
    pub vocabulary_uid: String,
    pub name: String,
    pub owner_organ: String,
    pub revision: i64,
    pub dictionaries: Vec<Dictionary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocationBinding {
    pub write_concept_uid: String,
    pub concept_uids: Vec<String>,
    pub revision: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FeatureBinding {
    pub feature: String,
    pub write_concept_uid: String,
    pub concept_uids: Vec<String>,
    pub revision: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TranslationPackage {
    pub format: u32,
    pub vocabulary_uid: String,
    pub dictionaries: Vec<Dictionary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub enum SavedLocationCommand {
    Page {
        query: Option<serde_json::Value>,
        #[serde(default)]
        records: Vec<String>,
        after: Option<String>,
        generation: String,
        bounds: [f64; 4],
    },
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
