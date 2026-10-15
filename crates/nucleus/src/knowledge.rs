use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const ITEM_LIMIT: usize = 4 * 1024 * 1024;
pub const CACHE_LIMIT: usize = 32 * 1024 * 1024;
pub const RESULT_LIMIT: usize = 20;
pub const DEPENDENCY_LIMIT: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct License {
    pub name: String,
    pub uri: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Credit {
    pub name: String,
    pub uri: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub id: String,
    pub name: String,
    pub uri: String,
    pub online: bool,
    pub license: Option<License>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Reference {
    pub source: String,
    pub id: String,
    pub title: String,
    pub uri: String,
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Claim {
    pub id: String,
    pub property: String,
    pub value: Value,
    pub conditions: Value,
    pub references: Value,
    pub source: String,
    pub revision: String,
    pub raw: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Meaning {
    pub id: String,
    pub revision: String,
    pub title: String,
    pub definition: String,
    pub raw: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Asset {
    pub id: String,
    pub uri: String,
    pub media_type: String,
    pub license: Option<License>,
    pub credits: Vec<Credit>,
    pub sha384: Option<String>,
    pub bytes: Option<String>,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Item {
    pub source: String,
    pub id: String,
    pub revision: String,
    pub title: String,
    pub content: String,
    pub content_type: String,
    pub uri: String,
    pub license: Option<License>,
    pub credits: Vec<Credit>,
    pub meanings: Vec<Meaning>,
    pub claims: Vec<Claim>,
    pub dependencies: Vec<Reference>,
    pub assets: Vec<Asset>,
    pub raw: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SearchResult {
    pub source: String,
    pub items: Vec<Reference>,
    pub complete: bool,
    pub next_offset: Option<u32>,
    pub offline: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Archive {
    pub format: u32,
    pub source: Source,
    pub items: Vec<Item>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MeaningKind {
    Predicate,
    Unit,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Mapping {
    pub source: String,
    pub external: String,
    pub external_revision: String,
    pub external_hash: String,
    pub kind: MeaningKind,
    pub concept_uid: String,
    pub definition_revision: i64,
}

pub fn mapping_matches(mapping: &Mapping, item: &Item) -> bool {
    use sha2::{Digest, Sha384};
    mapping.source == item.source
        && item.meanings.iter().any(|meaning| {
            meaning.id == mapping.external
                && meaning.revision == mapping.external_revision
                && serde_json::to_vec(meaning).is_ok_and(|raw| {
                    Sha384::digest(raw)
                        .iter()
                        .map(|byte| format!("{byte:02x}"))
                        .collect::<String>()
                        == mapping.external_hash
                })
        })
}

pub fn validate_assets(item: &Item) -> Result<(), &'static str> {
    use base64::Engine;
    use sha2::{Digest, Sha384};
    for asset in &item.assets {
        if let Some(encoded) = &asset.bytes {
            if encoded.len() > ITEM_LIMIT {
                return Err("An asset exceeds 4 MiB");
            }
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .map_err(|_| "An asset has invalid base64 content")?;
            if asset.sha384.as_deref()
                != Some(
                    Sha384::digest(&bytes)
                        .iter()
                        .map(|byte| format!("{byte:02x}"))
                        .collect::<String>()
                        .as_str(),
                )
            {
                return Err("An asset needs a matching SHA-384 digest");
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    #[serde(default)]
    pub optional_dependencies: Vec<String>,
    #[serde(default)]
    pub optional_assets: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum UpdateField {
    Title,
    Content,
    Claims,
    Attribution,
    Assets,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Sources,
    InstallArchive {
        package: String,
    },
    ImportDump {
        format: String,
        text: String,
        language: String,
    },
    ReplaceArchive {
        source: String,
        package: String,
    },
    RemoveSource {
        source: String,
    },
    Search {
        request_id: String,
        sources: Vec<String>,
        text: String,
        #[serde(default)]
        offset: u32,
    },
    Read {
        request_id: String,
        source: String,
        item: String,
    },
    ReadCached {
        source: String,
        item: String,
    },
    Poll {
        request_id: String,
    },
    Cancel {
        request_id: String,
    },
    ClearCache {
        source: Option<String>,
    },
    Reference {
        source: String,
        item: String,
        organ: String,
    },
    InspectSaved {
        record: String,
    },
    MapMeaning {
        source: String,
        external: String,
        kind: MeaningKind,
        concept: String,
    },
    PreviewSave {
        source: String,
        item: String,
        organ: String,
        #[serde(default)]
        selection: Selection,
    },
    Save {
        preview: String,
    },
    Export {
        record: String,
    },
    Import {
        package: String,
        organ: String,
    },
    ReviewUpdate {
        record: String,
    },
    ApplyUpdate {
        review: String,
        fields: Vec<UpdateField>,
        #[serde(default)]
        fork: bool,
    },
}
