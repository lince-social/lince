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
