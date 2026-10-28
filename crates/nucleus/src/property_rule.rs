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
    pub const ALL: [Self; 6] = [Self::Title, Self::Slug, Self::Kind, Self::Quantity, Self::Identity, Self::ApplicationId];

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
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PropertyRule {
    pub property: Property,
    pub value: String,
}

impl PropertyRule {
    pub fn validate(&self) -> bool {
        self.value.len() <= 4096 && !self.value.trim().is_empty()
            && (self.property != Property::Quantity || crate::DecimalValue::parse_inferred(&self.value).is_ok())
    }

    pub fn matches(&self, properties: &Value) -> bool {
        if !self.validate() { return false; }
        let Some(value) = properties.get(self.property.key()) else { return false; };
        if self.property == Property::Quantity {
            let actual = value.as_str().map(str::to_owned).unwrap_or_else(|| value.to_string());
            crate::DecimalValue::parse_inferred(&actual).ok()
                .zip(crate::DecimalValue::parse_inferred(&self.value).ok())
                .is_some_and(|(actual, expected)| actual.exact_numeric_cmp(expected).is_eq())
        } else {
            value.as_str() == Some(self.value.as_str())
        }
    }
}
