// non-lean-yet
// Scope: enum Property, struct PropertyRule.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::map_unwrap_or] called `map(<f>).unwrap_or_else(<g>)` on an `Option` value.
//   Sites 75.
// - [Clippy: clippy::exhaustive_enums] Review whether public API evolution requires non_exhaustive.
//   Sites 5.
// - [Clippy: clippy::manual_is_variant_and] called `.ok().is_some_and(..)` on a `Result` value.
//   Sites 79.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 5, 6,
//   7, 8, 9, 10, 11, 15, 24, 46, 54, 55, 56, 60, 64.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Property {
    Title,
    Slug,
    Kind,
    Quantity,
    Identity,
    ApplicationId,
}

impl Property {
    pub const ALL: [Self; 6] = [
        Self::Title,
        Self::Slug,
        Self::Kind,
        Self::Quantity,
        Self::Identity,
        Self::ApplicationId,
    ];

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

    pub fn valid_value(self, value: &str) -> bool {
        value.len() <= 4096
            && !value.trim().is_empty()
            && (self != Self::Quantity || crate::DecimalValue::parse_inferred(value).is_ok())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PropertyRule {
    pub property: Property,
    pub value: String,
}

impl PropertyRule {
    pub fn validate(&self) -> bool {
        self.property.valid_value(&self.value)
    }

    pub fn matches(&self, properties: &Value) -> bool {
        if self.property == Property::Quantity {
            if self.value.len() > 4096 || self.value.trim().is_empty() {
                return false;
            }
            let Ok(expected) = crate::DecimalValue::parse_inferred(&self.value) else {
                return false;
            };
            let Some(value) = properties.get(self.property.key()) else {
                return false;
            };
            let actual = value
                .as_str()
                .map(std::borrow::Cow::Borrowed)
                .unwrap_or_else(|| std::borrow::Cow::Owned(value.to_string()));
            crate::DecimalValue::parse_inferred(&actual)
                .ok()
                .is_some_and(|actual| actual.exact_numeric_cmp(expected).is_eq())
        } else {
            self.validate()
                && properties
                    .get(self.property.key())
                    .is_some_and(|value| value.as_str() == Some(self.value.as_str()))
        }
    }
}
