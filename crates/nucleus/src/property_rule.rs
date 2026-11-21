use std::borrow::Cow::{Borrowed, Owned};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::DecimalValue;

const MAX_PROPERTY_BYTES: usize = 4096;

/// Property available to Area and window matching rules.
#[expect(
    clippy::exhaustive_enums,
    reason = "callers match every supported property"
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Property {
    /// Record or window title.
    Title,
    /// Record slug.
    Slug,
    /// Record kind.
    Kind,
    /// Record quantity as an exact decimal.
    Quantity,
    /// Record identifier.
    Identity,
    /// Native application identifier.
    ApplicationId,
}

impl Property {
    /// Properties in display order.
    pub const ALL: [Self; 6] = [
        Self::Title,
        Self::Slug,
        Self::Kind,
        Self::Quantity,
        Self::Identity,
        Self::ApplicationId,
    ];

    /// Returns the display name for the property.
    pub fn name(self) -> &'static str {
        match self {
            Self::Title => "Title",
            Self::Slug => "Slug",
            Self::Kind => "Kind",
            Self::Quantity => "Quantity",
            Self::Identity => "Record identity",
            Self::ApplicationId => "Application ID",
        }
    }

    fn key(self) -> &'static str {
        match self {
            Self::Title => "head",
            Self::Slug => "slug",
            Self::Kind => "kind",
            Self::Quantity => "quantity",
            Self::Identity => "uid",
            Self::ApplicationId => "app_id",
        }
    }

    /// Checks the text limit and decimal syntax for a quantity.
    pub fn valid_value(self, value: &str) -> bool {
        valid_text(value) && (self != Self::Quantity || DecimalValue::parse_inferred(value).is_ok())
    }
}

fn valid_text(value: &str) -> bool {
    value.len() <= MAX_PROPERTY_BYTES && !value.trim().is_empty()
}

/// Matches one named property against an expected value.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PropertyRule {
    /// Property to match.
    pub property: Property,
    /// Expected text or decimal spelling.
    pub value: String,
}

impl PropertyRule {
    /// Returns whether the expected value is valid for the selected property.
    pub fn validate(&self) -> bool {
        self.property.valid_value(&self.value)
    }

    /// Matches quantities by exact numeric value and all other properties by exact text.
    pub fn matches(&self, properties: &Value) -> bool {
        if !valid_text(&self.value) {
            return false;
        }
        let Some(value) = properties.get(self.property.key()) else {
            return false;
        };
        if self.property != Property::Quantity {
            return value.as_str() == Some(self.value.as_str());
        }
        let Ok(expected) = DecimalValue::parse_inferred(&self.value) else {
            return false;
        };
        let actual = match value {
            Value::String(text) => Borrowed(text.as_str()),
            Value::Number(_) => Owned(value.to_string()),
            _ => return false,
        };
        DecimalValue::parse_inferred(&actual)
            .is_ok_and(|actual| actual.exact_numeric_cmp(expected).is_eq())
    }
}
