use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub const FORMAT: &str = "lince.sand-package.v1";
pub const DECLARATIVE: &str = "lince.declarative.v1";
pub const DESKTOP_LAYOUT: &str = "lince.desktop-layout.v1";
pub const MAX_BYTES: usize = 512 * 1024;
pub const PAGE_SIZE: u32 = 32;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Sand,
    Castle,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct License {
    pub name: String,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    pub origin: String,
    pub id: String,
    pub version: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub identity: Identity,
    pub name: String,
    pub kind: Kind,
    pub author: String,
    pub execution: String,
    pub permissions: Vec<String>,
    pub licenses: Vec<License>,
    pub credits: Vec<String>,
    pub key_id: String,
    pub public_key: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Package {
    pub format: String,
    pub manifest: Manifest,
    pub payload: String,
    pub digest: String,
    pub signature: String,
}

impl Identity {
    pub fn validate(&self) -> Result<(), String> {
        if !crate::valid_uid(&self.origin, "r")
            || !crate::valid_uid(&self.id, "r")
            || self.version == 0
        {
            return Err(
                "A package needs stable Organ and Record UIDs and a positive version.".into(),
            );
        }
        Ok(())
    }
}

fn text(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}

impl Manifest {
    pub fn validate(&self) -> Result<(), String> {
        self.identity.validate()?;
        if !text(&self.name, 320)
            || !crate::valid_uid(&self.author, "r")
            || !text(&self.execution, 128)
            || !text(&self.key_id, 128)
            || !text(&self.public_key, 128)
            || self.permissions.len() > 64
            || self
                .permissions
                .iter()
                .any(|permission| !text(permission, 128))
            || self.licenses.is_empty()
            || self.licenses.len() > 32
            || self.licenses.iter().any(|license| {
                !text(&license.name, 256)
                    || license.text.trim().is_empty()
                    || license.text.len() > 64 * 1024
            })
            || self.credits.len() > 64
            || self.credits.iter().any(|credit| !text(credit, 4096))
            || serde_json::to_vec(self)
                .map_err(|error| error.to_string())?
                .len()
                > 128 * 1024
        {
            return Err("Invalid package metadata, permissions, licenses or credits.".into());
        }
        Ok(())
    }
}

impl Package {
    pub fn signing_bytes(&self) -> Result<Vec<u8>, String> {
        serde_json::to_vec(&(&self.format, &self.manifest, &self.payload))
            .map_err(|error| error.to_string())
    }

    pub fn content_digest(&self) -> Result<String, String> {
        Ok(Sha256::digest(self.signing_bytes()?)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect())
    }

    pub fn validate_execution(&self) -> Result<(), String> {
        self.validate()?;
        if !matches!(
            self.manifest.execution.as_str(),
            DECLARATIVE | DESKTOP_LAYOUT
        ) {
            return Err(format!(
                "This device cannot run {}. The package and its metadata remain available.",
                self.manifest.execution
            ));
        }
        let value: serde_json::Value = serde_json::from_str(&self.payload)
            .map_err(|_| "Invalid component document.".to_string())?;
        let format = value["format"].as_str().unwrap_or_default();
        let compatible = match self.manifest.execution.as_str() {
            DECLARATIVE => {
                format == crate::canvas::Document::FORMAT
                    || (format == crate::component::composition::FORMAT
                        && value.get("composition").is_some())
            }
            DESKTOP_LAYOUT => {
                format == crate::component::composition::FORMAT && value.get("castle").is_some()
            }
            _ => {
                return Err(format!(
                    "This device cannot run {}. The package and its metadata remain available.",
                    self.manifest.execution
                ));
            }
        };
        if !compatible {
            return Err("The package contents do not match its execution model.".into());
        }
        if required_permissions(&self.payload)?
            .iter()
            .any(|permission| !self.manifest.permissions.contains(permission))
        {
            return Err("Package metadata omits permissions requested by its contents.".into());
        }
        Ok(())
    }

    pub fn validate(&self) -> Result<(), String> {
        self.manifest.validate()?;
        if self.format != FORMAT
            || self.payload.is_empty()
            || self.payload.len() > crate::component::composition::MAX_BYTES
            || self.digest != self.content_digest()?
            || !text(&self.signature, 128)
            || serde_json::to_vec(self)
                .map_err(|error| error.to_string())?
                .len()
                > MAX_BYTES
        {
            return Err("Invalid package format, size or content digest.".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entry {
    pub identity: Identity,
    pub name: String,
    pub kind: Kind,
    pub author: String,
    pub execution: String,
    pub public: bool,
    pub origin_verified: bool,
}

impl Entry {
    pub fn validate(&self) -> Result<(), String> {
        self.identity.validate()?;
        if !text(&self.name, 320)
            || !crate::valid_uid(&self.author, "r")
            || !text(&self.execution, 128)
        {
            return Err("Invalid package catalogue metadata.".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    Catalogue {
        entries: Vec<Entry>,
        next: Option<u32>,
    },
    Package {
        package: Package,
        public: bool,
        origin_verified: bool,
    },
    Saved {
        identity: Identity,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "operation", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Command {
    List {
        organ: Option<String>,
        offset: u32,
    },
    Inspect {
        organ: Option<String>,
        identity: Identity,
    },
    Save {
        record: String,
        kind: Kind,
        licenses: Vec<License>,
        credits: Vec<String>,
    },
    SetPublic {
        identity: Identity,
        public: bool,
    },
    Receive {
        organ: String,
        identity: Identity,
    },
    Enable {
        identity: Identity,
    },
}

impl Command {
    pub fn permission(&self) -> &'static str {
        match self {
            Self::List { .. } | Self::Inspect { .. } => "record:read",
            Self::Save { .. } | Self::Receive { .. } => "record:create",
            Self::SetPublic { .. } => "organ:update",
            Self::Enable { .. } => "record:update",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Query {
    List { offset: u32 },
    Inspect { identity: Identity },
}
fn collect_permissions(value: &serde_json::Value, permissions: &mut BTreeSet<String>) {
    match value {
        serde_json::Value::Object(fields) => {
            if let Some(action) = fields.get("action").and_then(serde_json::Value::as_str) {
                permissions.insert(format!("action:{action}"));
            }
            if fields.contains_key("record")
                || fields.contains_key("bindings")
                || fields.contains_key("protein")
                || fields.contains_key("Protein")
            {
                permissions.insert("record:read".into());
            }
            if let Some(kind) = fields.get("kind").and_then(serde_json::Value::as_str) {
                let kind = kind.to_ascii_lowercase();
                if fields.get("source").and_then(serde_json::Value::as_str) == Some("native") {
                    permissions.insert(format!("native:{kind}"));
                }
                match kind.as_str() {
                    "terminal" => {
                        permissions.insert("terminal:execute".into());
                    }
                    "sync" => {
                        permissions.insert("filesystem:write".into());
                        permissions.insert("record:read".into());
                    }
                    "karma" | "frequency" | "transfer" | "calendar" | "todo" | "protein"
                    | "ontology" | "organ" => {
                        permissions.insert("record:read".into());
                    }
                    "operation" => {
                        permissions.insert("record:update".into());
                    }
                    _ => {}
                }
            }
            if fields
                .get("start_call")
                .is_some_and(|value| !value.is_null())
            {
                permissions.insert("microphone".into());
                if fields["start_call"]["media"] == "video" {
                    permissions.insert("camera".into());
                }
            }
            for value in fields.values() {
                collect_permissions(value, permissions);
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                collect_permissions(value, permissions);
            }
        }
        _ => {}
    }
}

pub fn required_permissions(payload: &str) -> Result<Vec<String>, String> {
    if payload.len() > crate::component::composition::MAX_BYTES {
        return Err("Component contents exceed 256 KiB.".into());
    }
    let value: serde_json::Value =
        serde_json::from_str(payload).map_err(|_| "Invalid component document.".to_string())?;
    let mut permissions = BTreeSet::from(["canvas:place".to_string()]);
    collect_permissions(&value, &mut permissions);
    Ok(permissions.into_iter().collect())
}
