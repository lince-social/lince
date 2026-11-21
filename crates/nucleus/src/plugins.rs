// non-lean-yet
// Scope: const FORMAT, const PROTOCOL, const MAX_BYTES, const MAX_ITEMS.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::indexing_slicing] Verify bounds or use checked access on fallible inputs.
//   Sites 138.
// - [Clippy: clippy::collapsible_match] this `if` can be collapsed into the outer `match`. Sites
//   240.
// - [Clippy: clippy::exhaustive_enums] Review whether public API evolution requires non_exhaustive.
//   Sites 13, 27, 48, 313, 391.
// - [Clippy: clippy::missing_errors_doc] State the observable failure cases for exported functions.
//   Sites 149, 283.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 5, 6,
//   7, 8, 9, 13, 14, 15, 20, 21, 22, 27, 28, 29, 30, 31, 36, 37, 38, 39, 41, 43, 48, 49, 50, 51,
//   52, 53, 54, 55, 56, 58, 60, 61, 62, 63, 65, 66, 67, 68, 69, 74, 80, 86, 97, 98, 99, 100, 101,
//   103, 104, 106, 107, 108, 110, 115, 116, 117, 118, 119, 120, 124, 135, 149, 273, 283, 303, 304,
//   305, 306, 313, 315, 316, 317, 321, 324, 327, 345, 346, 347, 348, 349, 350, 352, 354, 356, 358,
//   360, 364, 365, 366, 367, 368, 372, 373, 374, 375, 376, 377, 381, 382, 383, 384, 385, 386, 391,
//   392, 393, 394, 395, 397, 398, 399, 401, 402, 403, 404, 406, 407, 408, 410, 411, 413, 414, 415,
//   417, 418, 420, 421, 423, 424, 426, 427, 429, 430, 432, 433, 435, 439, 449, 450, 451, 452, 453,
//   458, 459, 460, 465, 466, 467, 468, 469, 474, 475, 476, 481, 482, 483, 484, 485, 490, 491, 496,
//   497, 498, 499, 500, 501, 506, 507, 508, 509, 510.
// - [review: §5 Errors] If exported, use a typed error with preserved causes. Sites validate:
//   String (149), validate_values: String (283).

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const FORMAT: &str = "lince.plugin.v1";
pub const PROTOCOL: &str = "lince.plugin-rpc.v1";
pub const MAX_BYTES: usize = 1024 * 1024;
pub const MAX_ITEMS: usize = 100;
pub const MAX_SNAPSHOT_ITEMS: usize = 1000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotSource {
    Record,
    Schedule,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DocumentFormat {
    pub source: SnapshotSource,
    pub extension: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FieldType {
    Text,
    Integer,
    Boolean,
    Json,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Field {
    pub id: String,
    pub name: String,
    pub kind: FieldType,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub default: Value,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "feature", rename_all = "snake_case", deny_unknown_fields)]
pub enum Contribution {
    Sync {
        id: String,
        name: String,
        settings: Vec<Field>,
        incoming: bool,
        outgoing: bool,
        replay_safe: bool,
        schema: String,
        #[serde(default)]
        document: Option<DocumentFormat>,
    },
    Action {
        id: String,
        name: String,
        arguments: Vec<Field>,
    },
    Schema {
        id: String,
        name: String,
        namespace: String,
        fields: Vec<Field>,
    },
}

impl Contribution {
    pub fn id(&self) -> &str {
        match self {
            Self::Sync { id, .. } | Self::Action { id, .. } | Self::Schema { id, .. } => id,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::Sync { name, .. } | Self::Action { name, .. } | Self::Schema { name, .. } => name,
        }
    }

    pub fn fields(&self) -> &[Field] {
        match self {
            Self::Sync { settings, .. } => settings,
            Self::Action { arguments, .. } => arguments,
            Self::Schema { fields, .. } => fields,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub format: String,
    pub id: String,
    pub name: String,
    pub version: u32,
    #[serde(default)]
    pub compatibility: Compatibility,
    pub executable: String,
    #[serde(default)]
    pub arguments: Vec<String>,
    pub contributions: Vec<Contribution>,
    pub licenses: Vec<crate::sand_package::License>,
    #[serde(default)]
    pub credits: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Compatibility {
    pub publisher: String,
    pub origin: String,
    pub operating_systems: Vec<String>,
    pub interfaces: Vec<String>,
    pub dependencies: std::collections::BTreeMap<String, u32>,
}

impl Compatibility {
    pub fn native() -> Self {
        Self {
            publisher: "Lince".into(),
            origin: "https://github.com/lince-social/lince".into(),
            operating_systems: vec![std::env::consts::OS.into()],
            interfaces: vec![PROTOCOL.into()],
            dependencies: Default::default(),
        }
    }
}

pub fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 96
        && value.as_bytes()[0].is_ascii_alphanumeric()
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'-'))
}

fn text(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}

impl Manifest {
    pub fn validate(&self) -> Result<(), String> {
        let compatibility = &self.compatibility;
        if compatibility.publisher.len() > 160
            || compatibility.origin.len() > 4096
            || compatibility.operating_systems.len() > 16
            || compatibility.interfaces.len() > 16
            || compatibility.dependencies.len() > 32
            || compatibility
                .operating_systems
                .iter()
                .chain(&compatibility.interfaces)
                .any(|item| !text(item, 160))
            || compatibility
                .dependencies
                .iter()
                .any(|(id, version)| !valid_id(id) || *version == 0 || id == &self.id)
        {
            return Err("Invalid plugin compatibility information".into());
        }
        if self.format != FORMAT
            || !valid_id(&self.id)
            || !text(&self.name, 160)
            || self.version == 0
            || !text(&self.executable, 4096)
            || self.arguments.len() > 32
            || self
                .arguments
                .iter()
                .any(|arg| arg.len() > 4096 || arg.contains('\0'))
            || self.contributions.is_empty()
            || self.contributions.len() > 32
            || self.licenses.is_empty()
            || self.licenses.len() > 32
            || self.licenses.iter().any(|license| {
                !text(&license.name, 160) || license.text.is_empty() || license.text.len() > 65536
            })
            || self.credits.len() > 32
            || self.credits.iter().any(|credit| !text(credit, 4096))
        {
            return Err("Invalid plugin manifest".into());
        }
        let mut ids = BTreeSet::new();
        let mut namespaces = BTreeSet::new();
        for contribution in &self.contributions {
            if !valid_id(contribution.id())
                || !ids.insert(contribution.id())
                || !text(contribution.name(), 160)
            {
                return Err("Plugin contributions need unique identifiers and names".into());
            }
            validate_fields(contribution.fields())?;
            match contribution {
                Contribution::Sync {
                    incoming,
                    outgoing,
                    schema,
                    document,
                    ..
                } => {
                    if let Some(format) = document {
                        if *incoming
                            || !*outgoing
                            || !schema.is_empty()
                            || format.extension.is_empty()
                            || format.extension.len() > 16
                            || !format
                                .extension
                                .bytes()
                                .all(|byte| byte.is_ascii_alphanumeric())
                            || !["directory", "filename"].iter().all(|id| {
                                contribution.fields().iter().any(|field| {
                                    field.id == *id
                                        && field.required
                                        && matches!(field.kind, FieldType::Text)
                                })
                            })
                        {
                            return Err("Document Sync needs an outgoing direction, extension and directory/filename text fields".into());
                        }
                    } else if !(*incoming || *outgoing)
                        || !self
                            .contributions
                            .iter()
                            .any(|c| matches!(c, Contribution::Schema { id, .. } if id == schema))
                    {
                        return Err(
                            "A Sync provider needs a direction and its declared schema".into()
                        );
                    }
                }
                Contribution::Schema { namespace, .. } => {
                    if !valid_id(namespace)
                        || !namespace.starts_with(&format!("plugin.{}.", self.id))
                        || !namespaces.insert(namespace)
                    {
                        return Err(
                            "A plugin schema needs a unique namespace owned by its plugin".into(),
                        );
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
}

fn validate_fields(fields: &[Field]) -> Result<(), String> {
    let mut ids = BTreeSet::new();
    if fields.len() > 32 {
        return Err("Too many plugin fields".into());
    }
    for field in fields {
        if !valid_id(&field.id) || !text(&field.name, 160) || !ids.insert(&field.id) {
            return Err("Plugin fields need unique identifiers and names".into());
        }
        if !field.default.is_null() && !field.accepts(&field.default) {
            return Err("Plugin field default has the wrong type".into());
        }
    }
    Ok(())
}

impl Field {
    pub fn accepts(&self, value: &Value) -> bool {
        match self.kind {
            FieldType::Text => value.as_str().is_some_and(|value| value.len() <= 16384),
            FieldType::Integer => value.as_i64().is_some(),
            FieldType::Boolean => value.is_boolean(),
            FieldType::Json => true,
        }
    }
}

pub fn validate_values(fields: &[Field], value: &Value) -> Result<(), String> {
    let object = value.as_object().ok_or("Plugin fields must be an object")?;
    if crate::json::encoded_len(value).map_err(|error| error.to_string())? > 65536
        || object
            .keys()
            .any(|key| !fields.iter().any(|field| &field.id == key))
    {
        return Err("Unknown or oversized plugin fields".into());
    }
    for field in fields {
        match object.get(&field.id) {
            Some(value) if field.accepts(value) => {}
            None if !field.required => {}
            _ => return Err(format!("Invalid value for {}", field.name)),
        }
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Package {
    pub manifest: Manifest,
    pub digest: String,
    pub enabled: bool,
}

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    #[default]
    Incoming,
    Outgoing,
    Both,
}

impl Direction {
    pub fn incoming(&self) -> bool {
        matches!(self, Self::Incoming | Self::Both)
    }
    pub fn outgoing(&self) -> bool {
        matches!(self, Self::Outgoing | Self::Both)
    }
    pub fn name(&self) -> &'static str {
        match self {
            Self::Incoming => "incoming",
            Self::Outgoing => "outgoing",
            Self::Both => "both",
        }
    }
}

fn default_filter() -> Value {
    serde_json::json!([])
}
fn default_interval() -> u64 {
    60
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub uid: String,
    pub provider: String,
    pub name: String,
    pub settings: Value,
    pub protein_uid: Option<String>,
    #[serde(default = "default_filter")]
    pub incoming_filter: Value,
    #[serde(default)]
    pub direction: Direction,
    #[serde(default = "default_interval")]
    pub interval_seconds: u64,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub revision: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BindingStatus {
    pub binding: Binding,
    pub status: String,
    pub pending: u64,
    pub failed: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Catalog {
    pub packages: Vec<Package>,
    pub diagnostics: Vec<String>,
    pub bindings: Vec<BindingStatus>,
    pub proteins: Vec<(String, String)>,
    pub deliveries: Vec<Delivery>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Delivery {
    pub uid: String,
    pub binding: String,
    pub record: String,
    pub state: String,
    pub result: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Inspect,
    Reload,
    Install {
        directory: String,
    },
    Remove {
        plugin: String,
        digest: String,
    },
    Enable {
        plugin: String,
        digest: String,
        enabled: bool,
    },
    Invoke {
        contribution: String,
        arguments: Value,
    },
    Preview {
        binding: Binding,
    },
    SaveBinding {
        binding: Binding,
        preview: Option<String>,
    },
    Run {
        binding: String,
    },
    Stop {
        binding: String,
    },
    Start {
        binding: String,
    },
    RemoveBinding {
        binding: String,
    },
    Retry {
        delivery: String,
    },
    ResolveDelivery {
        delivery: String,
    },
    ClearHistory,
}

impl Command {
    pub fn permission(&self) -> &'static str {
        match self {
            Self::Inspect | Self::Preview { .. } => "record:read",
            _ => "record:update",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportedRecord {
    pub external_id: String,
    pub head: String,
    pub body: String,
    pub metadata: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PullResult {
    pub records: Vec<ImportedRecord>,
    pub cursor: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublishRecord {
    pub uid: String,
    pub head: String,
    pub body: String,
    pub created_at: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreparedPublication {
    pub record: ImportedRecord,
    pub payload: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportSnapshot {
    pub scope: String,
    pub generated_at_ms: i64,
    pub window: Option<crate::projection::Window>,
    pub rows: Vec<Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportDocument {
    pub content: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RpcRequest {
    pub protocol: String,
    pub id: u64,
    pub contribution: String,
    pub operation: String,
    pub input: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RpcResponse {
    pub protocol: String,
    pub id: u64,
    pub result: Option<Value>,
    pub error: Option<String>,
}
