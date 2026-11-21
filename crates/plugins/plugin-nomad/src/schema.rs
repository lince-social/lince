// non-lean-yet
// Scope: struct Resource, struct Tier, struct Category, enum Categories.
// BeautifulRust: No site flagged by this pass. Check contracts and feature coverage.

//! NOMAD HTTP response shapes. Unknown upstream fields are deliberately tolerated.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Resource {
    pub id: String,
    pub version: String,
    pub title: String,
    pub description: String,
    pub url: String,
    pub size_mb: f64,
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
    #[serde(default)]
    pub auth: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Tier {
    pub name: String,
    pub slug: String,
    pub description: String,
    #[serde(default, rename = "includesTier")]
    pub includes_tier: Option<String>,
    pub resources: Vec<Resource>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Category {
    pub name: String,
    pub slug: String,
    pub description: String,
    pub language: String,
    pub tiers: Vec<Tier>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum Categories {
    List(Vec<Category>),
    Manifest { categories: Vec<Category> },
}

impl Categories {
    pub fn into_vec(self) -> Vec<Category> {
        match self {
            Self::List(v) | Self::Manifest { categories: v } => v,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Collection {
    pub name: String,
    pub slug: String,
    pub description: String,
    pub language: String,
    pub resources: Vec<Resource>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum Collections {
    List(Vec<Collection>),
    Manifest { collections: Vec<Collection> },
}

impl Collections {
    pub fn into_vec(self) -> Vec<Collection> {
        match self {
            Self::List(v) | Self::Manifest { collections: v } => v,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FileEntry {
    #[serde(rename = "type")]
    pub kind: String,
    pub name: String,
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub size_bytes: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
pub struct Files {
    pub files: Vec<FileEntry>,
    #[serde(default)]
    pub next: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    pub source: String,
    pub file_name: String,
    #[serde(default)]
    pub collection: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub uploaded_at: Option<String>,
    #[serde(default)]
    pub is_user_upload: bool,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub chunks_embedded: u64,
    #[serde(default)]
    pub active: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
pub struct Documents {
    pub files: Vec<Document>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct WikipediaOption {
    pub id: String,
    pub name: String,
    pub description: String,
    pub size_mb: f64,
    pub url: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct Wikipedia {
    pub options: Vec<WikipediaOption>,
}

#[derive(Debug, Default)]
pub struct Snapshot {
    pub categories: Vec<Category>,
    pub archives: Vec<FileEntry>,
    pub map_collections: Vec<Collection>,
    pub maps: Vec<FileEntry>,
    pub documents: Vec<Document>,
    pub wikipedia: Vec<WikipediaOption>,
}
