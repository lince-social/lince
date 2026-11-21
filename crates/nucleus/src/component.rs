//! Presentation states and composition models shared by the Canvas and Karma.

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub mod composition;
mod references;

pub use composition::{Composition, Document, EventBinding, Immunity, Part};

const MAX_REFERENCE_BYTES: usize = 256;
const MAX_SEARCH_BYTES: usize = 256;
const MAX_TEXT_CHARACTERS: usize = 4096;
const MAX_LABEL_CHARACTERS: usize = 80;

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
        action: Value,
    },
    Composition {
        composition: Composition,
    },
}

impl ComponentState {
    /// Returns the stable wire discriminator.
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

    /// Checks the limits and required fields of this presentation.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid reference, unsupported setting, or invalid composition.
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Record { record, .. } if !valid_record_reference(record) => {
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
                if [&start.thread, &start.person]
                    .into_iter()
                    .any(|reference| !valid_record_reference(reference))
                {
                    return Err("Choose a thread and Person for the automatic call".into());
                }
                Ok(())
            }
            Self::Text { text } if text.chars().nth(MAX_TEXT_CHARACTERS).is_some() => {
                Err("Component text exceeds 4096 characters".into())
            }
            Self::Karma { search } | Self::Frequency { search } | Self::Transfer { search }
                if search.len() > MAX_SEARCH_BYTES || search.chars().any(char::is_control) =>
            {
                Err("Component search exceeds 256 bytes or contains control characters".into())
            }
            Self::Button { label, action } => {
                if label.trim().is_empty()
                    || label.chars().enumerate().any(|(index, character)| {
                        index >= MAX_LABEL_CHARACTERS || character.is_control()
                    })
                    || !action.get("action").is_some_and(Value::is_string)
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

    /// Visits this state and then its nested parts in depth-first order.
    ///
    /// # Errors
    ///
    /// Stops at the first error returned by the visitor and propagates it unchanged.
    pub fn visit<'a>(
        &'a self,
        visitor: &mut impl FnMut(&'a Self) -> Result<(), String>,
    ) -> Result<(), String> {
        visitor(self)?;
        if let Self::Composition { composition } = self {
            for part in &composition.parts {
                part.component.visit(visitor)?;
            }
        }
        Ok(())
    }
}

fn valid_record_reference(reference: &str) -> bool {
    reference.len() <= MAX_REFERENCE_BYTES
        && !reference.trim().is_empty()
        && !reference.chars().any(char::is_control)
}

/// Identifies the slot and state of a presented component.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Presentation {
    pub slot: String,
    pub component: ComponentState,
}
