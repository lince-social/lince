//! Plugin manifests, bindings, commands, and the shared RPC wire format.
//!
//! [Manifest::validate] checks contribution declarations and ownership of schema namespaces.
//! [validate_values] checks an argument or settings object against its declared fields.

use std::{collections::BTreeMap, env::consts::OS};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{projection::Window, sand_package::License};

mod validation;

pub use validation::{valid_id, validate_values};

/// The plugin manifest format identifier.
pub const FORMAT: &str = "lince.plugin.v1";
/// The plugin RPC protocol identifier.
pub const PROTOCOL: &str = "lince.plugin-rpc.v1";
/// The maximum encoded RPC message size in bytes.
pub const MAX_BYTES: usize = 1024 * 1024;
/// The maximum item count for an ordinary Sync batch.
pub const MAX_ITEMS: usize = 100;
/// The maximum row count for a document snapshot.
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
    /// Borrows the contribution identifier within the plugin.
    pub fn id(&self) -> &str {
        match self {
            Self::Sync { id, .. } | Self::Action { id, .. } | Self::Schema { id, .. } => id,
        }
    }

    /// Borrows the display name of the contribution.
    pub fn name(&self) -> &str {
        match self {
            Self::Sync { name, .. } | Self::Action { name, .. } | Self::Schema { name, .. } => name,
        }
    }

    /// Borrows the fields declared for this contribution.
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
    pub licenses: Vec<License>,
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
    pub dependencies: BTreeMap<String, u32>,
}

impl Compatibility {
    /// Creates compatibility metadata for the current operating system and protocol.
    pub fn native() -> Self {
        Self {
            publisher: "Lince".into(),
            origin: "https://github.com/lince-social/lince".into(),
            operating_systems: vec![OS.into()],
            interfaces: vec![PROTOCOL.into()],
            dependencies: Default::default(),
        }
    }
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
    /// Reports whether this binding accepts incoming records.
    pub fn incoming(&self) -> bool {
        matches!(self, Self::Incoming | Self::Both)
    }

    /// Reports whether this binding publishes outgoing records.
    pub fn outgoing(&self) -> bool {
        matches!(self, Self::Outgoing | Self::Both)
    }

    /// Returns the stable wire spelling of this direction.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Incoming => "incoming",
            Self::Outgoing => "outgoing",
            Self::Both => "both",
        }
    }
}

fn default_filter() -> Value {
    Value::Array(Vec::new())
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
    /// Returns the host Record permission required to run the command.
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
    pub window: Option<Window>,
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
