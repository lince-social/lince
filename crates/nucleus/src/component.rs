use serde::{Deserialize, Serialize};

pub mod composition;
pub use composition::{Composition, Document, EventBinding, Immunity, Part};

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum RecordMode {
    #[default]
    Full,
    Description,
    Call,
}

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum CallMedia {
    #[default]
    Audio,
    Video,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CallStart {
    pub thread: String,
    pub person: String,
    #[serde(default)]
    pub media: CallMedia,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum ComponentState {
    Record {
        #[serde(default)]
        record: String,
        #[serde(default)]
        mode: RecordMode,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        start_call: Option<CallStart>,
    },
    Text {
        text: String,
    },
    Karma {
        #[serde(default)]
        search: String,
    },
    Frequency {
        #[serde(default)]
        search: String,
    },
    Transfer {
        #[serde(default)]
        search: String,
    },
    Calendar,
    Area {
        #[serde(default)]
        immunity: Immunity,
        #[serde(default)]
        strength: i32,
    },
    Button {
        label: String,
        action: serde_json::Value,
    },
    Composition {
        composition: Composition,
    },
}

impl ComponentState {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Record { .. } => "record",
            Self::Text { .. } => "text",
            Self::Karma { .. } => "karma",
            Self::Frequency { .. } => "frequency",
            Self::Transfer { .. } => "transfer",
            Self::Calendar => "calendar",
            Self::Area { .. } => "area",
            Self::Button { .. } => "button",
            Self::Composition { .. } => "composition",
        }
    }

    pub fn record(&self) -> Option<&str> {
        match self {
            Self::Record { record, .. } => Some(record),
            _ => None,
        }
    }

    pub fn record_mut(&mut self) -> Option<&mut String> {
        match self {
            Self::Record { record, .. } => Some(record),
            _ => None,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Record { record, .. }
                if record.trim().is_empty()
                    || record.len() > 256
                    || record.chars().any(char::is_control) =>
            {
                Err("Choose a Record for the component".into())
            }
            Self::Record {
                mode,
                start_call: Some(start),
                ..
            } => {
                if *mode != RecordMode::Call {
                    return Err("Automatic calling requires Record mode call".into());
                }
                if [&start.thread, &start.person].into_iter().any(|reference| {
                    reference.trim().is_empty()
                        || reference.len() > 256
                        || reference.chars().any(char::is_control)
                }) {
                    return Err("Choose a thread and Person for the automatic call".into());
                }
                Ok(())
            }
            Self::Text { text } if text.chars().count() > 4096 => {
                Err("Component text exceeds 4096 characters".into())
            }
            Self::Karma { search } | Self::Frequency { search } | Self::Transfer { search }
                if search.len() > 256 || search.chars().any(char::is_control) =>
            {
                Err("Component search exceeds 256 bytes or contains control characters".into())
            }
            Self::Button { label, action } => {
                if label.trim().is_empty()
                    || label.chars().count() > 80
                    || label.chars().any(char::is_control)
                    || !action.is_object()
                    || action
                        .get("action")
                        .and_then(serde_json::Value::as_str)
                        .is_none()
                {
                    return Err("A button needs a label and an ordinary Action".into());
                }
                Ok(())
            }
            Self::Area { strength, .. } if !(0..=100_000).contains(strength) => {
                Err("Area strength must be between 0 and 100000".into())
            }
            Self::Composition { composition } => composition.validate(),
            _ => Ok(()),
        }
    }

    pub fn visit(
        &self,
        visitor: &mut impl FnMut(&Self) -> Result<(), String>,
    ) -> Result<(), String> {
        visitor(self)?;
        if let Self::Composition { composition } = self {
            for part in &composition.parts {
                part.component.visit(visitor)?;
            }
        }
        Ok(())
    }

    pub fn records_mut(&mut self) -> Vec<&mut String> {
        match self {
            Self::Record {
                record, start_call, ..
            } => {
                let mut records = vec![record];
                if let Some(start) = start_call {
                    records.extend([&mut start.thread, &mut start.person]);
                }
                records
            }
            Self::Composition { composition } => composition
                .parts
                .iter_mut()
                .flat_map(|part| part.component.records_mut())
                .collect(),
            _ => Vec::new(),
        }
    }

    pub fn records(&self) -> Vec<&str> {
        match self {
            Self::Record {
                record, start_call, ..
            } => {
                let mut records = vec![record.as_str()];
                if let Some(start) = start_call {
                    records.extend([start.thread.as_str(), start.person.as_str()]);
                }
                records
            }
            Self::Composition { composition } => composition
                .parts
                .iter()
                .flat_map(|part| part.component.records())
                .collect(),
            _ => Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Presentation {
    pub slot: String,
    pub component: ComponentState,
}
