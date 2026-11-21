use std::collections::BTreeSet;

use serde_json::Value;

use crate::json::encoded_len;

use super::{Contribution, DocumentFormat, FORMAT, Field, FieldType, Manifest};

const MAX_ID_BYTES: usize = 96;
const MAX_NAME_BYTES: usize = 160;
const MAX_MANIFEST_TEXT_BYTES: usize = 4_096;
const MAX_LICENSE_BYTES: usize = 65_536;
const MAX_FIELD_TEXT_BYTES: usize = 16_384;
const MAX_VALUES_BYTES: usize = 65_536;
const INVALID_DOCUMENT: &str =
    "Document Sync needs an outgoing direction, extension and directory/filename text fields";

/// Checks the plugin identifier syntax. Identifiers are ASCII and start with a letter or digit.
pub fn valid_id(value: &str) -> bool {
    let [first, rest @ ..] = value.as_bytes() else {
        return false;
    };
    value.len() <= MAX_ID_BYTES
        && first.is_ascii_alphanumeric()
        && rest
            .iter()
            .all(|&byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn valid_text(value: &str, max_bytes: usize) -> bool {
    value.len() <= max_bytes && !value.trim().is_empty() && !value.chars().any(char::is_control)
}

impl Manifest {
    /// Validates the manifest envelope, compatibility metadata, and declared contributions.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid declarations, duplicate identifiers or namespaces, or
    /// invalid field defaults.
    pub fn validate(&self) -> Result<(), String> {
        let compatibility = &self.compatibility;
        if compatibility.publisher.len() > MAX_NAME_BYTES
            || compatibility.origin.len() > MAX_MANIFEST_TEXT_BYTES
            || compatibility.operating_systems.len() > 16
            || compatibility.interfaces.len() > 16
            || compatibility.dependencies.len() > 32
            || compatibility
                .operating_systems
                .iter()
                .chain(&compatibility.interfaces)
                .any(|item| !valid_text(item, MAX_NAME_BYTES))
            || compatibility
                .dependencies
                .iter()
                .any(|(id, version)| !valid_id(id) || *version == 0 || id == &self.id)
        {
            return Err("Invalid plugin compatibility information".into());
        }
        if self.format != FORMAT
            || !valid_id(&self.id)
            || !valid_text(&self.name, MAX_NAME_BYTES)
            || self.version == 0
            || !valid_text(&self.executable, MAX_MANIFEST_TEXT_BYTES)
            || self.arguments.len() > 32
            || self
                .arguments
                .iter()
                .any(|arg| arg.len() > MAX_MANIFEST_TEXT_BYTES || arg.contains('\0'))
            || self.contributions.is_empty()
            || self.contributions.len() > 32
            || self.licenses.is_empty()
            || self.licenses.len() > 32
            || self.licenses.iter().any(|license| {
                !valid_text(&license.name, MAX_NAME_BYTES)
                    || license.text.is_empty()
                    || license.text.len() > MAX_LICENSE_BYTES
            })
            || self.credits.len() > 32
            || self
                .credits
                .iter()
                .any(|credit| !valid_text(credit, MAX_MANIFEST_TEXT_BYTES))
        {
            return Err("Invalid plugin manifest".into());
        }

        let mut ids = BTreeSet::new();
        let mut namespaces = BTreeSet::new();
        for contribution in &self.contributions {
            let id = contribution.id();
            if !valid_id(id) || !ids.insert(id) || !valid_text(contribution.name(), MAX_NAME_BYTES)
            {
                return Err("Plugin contributions need unique identifiers and names".into());
            }
            validate_fields(contribution.fields())?;
            match contribution {
                Contribution::Sync {
                    incoming,
                    outgoing,
                    schema,
                    document: Some(format),
                    settings,
                    ..
                } if *incoming
                    || !*outgoing
                    || !schema.is_empty()
                    || !valid_document_settings(format, settings) =>
                {
                    return Err(INVALID_DOCUMENT.into());
                }
                Contribution::Sync {
                    incoming,
                    outgoing,
                    schema,
                    document: None,
                    ..
                } if !(*incoming || *outgoing)
                    || !self.contributions.iter().any(
                        |item| matches!(item, Contribution::Schema { id, .. } if id == schema),
                    ) =>
                {
                    return Err("A Sync provider needs a direction and its declared schema".into());
                }
                Contribution::Schema { namespace, .. }
                    if !valid_id(namespace)
                        || !namespace
                            .strip_prefix("plugin.")
                            .and_then(|suffix| suffix.strip_prefix(self.id.as_str()))
                            .is_some_and(|suffix| suffix.starts_with('.'))
                        || !namespaces.insert(namespace) =>
                {
                    return Err(
                        "A plugin schema needs a unique namespace owned by its plugin".into(),
                    );
                }
                _ => {}
            }
        }
        Ok(())
    }
}

fn valid_document_settings(format: &DocumentFormat, settings: &[Field]) -> bool {
    let extension = &format.extension;
    !extension.is_empty()
        && extension.len() <= 16
        && extension.bytes().all(|byte| byte.is_ascii_alphanumeric())
        && ["directory", "filename"].iter().all(|id| {
            settings
                .iter()
                .any(|field| field.id == *id && field.required && field.kind == FieldType::Text)
        })
}

fn validate_fields(fields: &[Field]) -> Result<(), &'static str> {
    if fields.len() > 32 {
        return Err("Too many plugin fields");
    }
    let mut ids = BTreeSet::new();
    for field in fields {
        if !valid_id(&field.id)
            || !valid_text(&field.name, MAX_NAME_BYTES)
            || !ids.insert(&field.id)
        {
            return Err("Plugin fields need unique identifiers and names");
        }
        if !field.default.is_null() && !field.accepts(&field.default) {
            return Err("Plugin field default has the wrong type");
        }
    }
    Ok(())
}

impl Field {
    /// Checks a JSON value against this field's declared type and text length limit.
    pub fn accepts(&self, value: &Value) -> bool {
        match self.kind {
            FieldType::Text => value
                .as_str()
                .is_some_and(|value| value.len() <= MAX_FIELD_TEXT_BYTES),
            FieldType::Integer => value.as_i64().is_some(),
            FieldType::Boolean => value.is_boolean(),
            FieldType::Json => true,
        }
    }
}

/// Validates a JSON object of plugin settings or action arguments.
///
/// # Errors
///
/// Returns an error for oversized, unknown, missing required, or wrongly typed values.
pub fn validate_values(fields: &[Field], value: &Value) -> Result<(), String> {
    let object = value.as_object().ok_or("Plugin fields must be an object")?;
    if encoded_len(value).map_err(|error| error.to_string())? > MAX_VALUES_BYTES
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        super::{Compatibility, SnapshotSource},
        *,
    };
    use crate::sand_package::License;

    fn text_field(id: &str) -> Field {
        Field {
            id: id.into(),
            name: id.into(),
            kind: FieldType::Text,
            required: true,
            default: Value::Null,
        }
    }

    fn manifest(contributions: Vec<Contribution>) -> Manifest {
        Manifest {
            format: FORMAT.into(),
            id: "lince.calendar".into(),
            name: "Calendar".into(),
            version: 1,
            compatibility: Compatibility::native(),
            executable: "calendar".into(),
            arguments: Vec::new(),
            contributions,
            licenses: vec![License {
                name: "Test".into(),
                text: "Test license".into(),
            }],
            credits: Vec::new(),
        }
    }

    #[test]
    fn schema_namespaces_belong_to_the_complete_plugin_id() {
        let plugin = |namespace: &str| {
            manifest(vec![Contribution::Schema {
                id: "calendar".into(),
                name: "Calendar".into(),
                namespace: namespace.into(),
                fields: Vec::new(),
            }])
        };
        assert!(plugin("plugin.lince.calendar.event").validate().is_ok());
        assert_eq!(
            plugin("plugin.lince.calendars.event")
                .validate()
                .as_ref()
                .map_err(String::as_str),
            Err("A plugin schema needs a unique namespace owned by its plugin"),
        );
    }

    #[test]
    fn document_settings_and_json_objects_preserve_validation_precedence() {
        let settings = vec![text_field("directory"), text_field("filename")];
        let plugin = |incoming, settings| {
            manifest(vec![Contribution::Sync {
                id: "calendar".into(),
                name: "Calendar".into(),
                settings,
                incoming,
                outgoing: true,
                replay_safe: true,
                schema: String::new(),
                document: Some(DocumentFormat {
                    source: SnapshotSource::Schedule,
                    extension: "ics".into(),
                }),
            }])
        };
        assert!(plugin(false, settings.clone()).validate().is_ok());
        assert_eq!(
            plugin(true, settings.clone())
                .validate()
                .as_ref()
                .map_err(String::as_str),
            Err(INVALID_DOCUMENT)
        );
        assert_eq!(
            plugin(true, vec![text_field("directory"), text_field("directory")])
                .validate()
                .as_ref()
                .map_err(String::as_str),
            Err("Plugin fields need unique identifiers and names"),
        );
        assert_eq!(
            validate_values(&settings, &json!({"directory": "/calendar", "extra": "x"}))
                .as_ref()
                .map_err(String::as_str),
            Err("Unknown or oversized plugin fields"),
        );
        assert_eq!(
            validate_values(&settings, &json!({"directory": "/calendar"}))
                .as_ref()
                .map_err(String::as_str),
            Err("Invalid value for filename"),
        );
        assert!(
            validate_values(
                &settings,
                &json!({"directory": "/calendar", "filename": "main.ics"})
            )
            .is_ok()
        );
    }
}
