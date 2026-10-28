use crate::property_rule::PropertyRule;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const RULE_NAMESPACE: &str = "lince.park.opening-rule";
pub const WINDOW_NAMESPACE: &str = "lince.park.window-opening";
pub const MAX_BYTES: usize = 64 * 1024;
pub const MAX_RULES: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "kebab-case")]
pub enum Dimension {
    Fraction([u16; 2]),
    Pixels(u32),
}

impl Dimension {
    pub fn valid(self) -> bool {
        match self {
            Self::Fraction([numerator, denominator]) => numerator > 0 && numerator <= denominator,
            Self::Pixels(size) => (1..=100_000).contains(&size),
        }
    }

    pub fn resolve(self, available: f64) -> f64 {
        match self {
            Self::Fraction([numerator, denominator]) => available * f64::from(numerator) / f64::from(denominator),
            Self::Pixels(size) => f64::from(size),
        }.clamp(1.0, 100_000.0)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Properties {
    pub workspace: Option<u64>,
    pub output: Option<String>,
    pub width: Option<Dimension>,
    pub height: Option<Dimension>,
    pub floating: Option<bool>,
    pub maximized: Option<bool>,
    pub focus: Option<bool>,
}

impl Properties {
    pub fn validate(&self) -> Result<(), String> {
        if self.workspace == Some(0)
            || self.output.as_ref().is_some_and(|output| output.is_empty() || output.len() > 512 || output.chars().any(char::is_control))
            || self.width.is_some_and(|width| !width.valid())
            || self.height.is_some_and(|height| !height.valid())
        {
            return Err("Choose an existing workspace/output and positive sizes up to 100000 pixels or fractions no greater than one.".into());
        }
        Ok(())
    }

    pub fn over(&self, lower: &Self) -> Self {
        Self {
            workspace: self.workspace.or(lower.workspace),
            output: self.output.clone().or_else(|| lower.output.clone()),
            width: self.width.or(lower.width),
            height: self.height.or(lower.height),
            floating: self.floating.or(lower.floating),
            maximized: self.maximized.or(lower.maximized),
            focus: self.focus.or(lower.focus),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub revision: u64,
    pub enabled: bool,
    pub match_all: bool,
    pub conditions: Vec<PropertyRule>,
    pub properties: Properties,
}

impl Rule {
    pub fn validate(&self) -> Result<(), String> {
        if self.revision == 0 || self.conditions.is_empty() || self.conditions.len() > 32
            || self.conditions.iter().any(|condition| !condition.validate())
            || serde_json::to_vec(self).map_err(|error| error.to_string())?.len() > MAX_BYTES
        {
            return Err("An opening rule needs a revision and 1–32 valid matching conditions within 64 KiB.".into());
        }
        self.properties.validate()
    }

    pub fn matches(&self, metadata: &Value) -> bool {
        self.enabled && self.validate().is_ok() && if self.match_all {
            self.conditions.iter().all(|condition| condition.matches(metadata))
        } else {
            self.conditions.iter().any(|condition| condition.matches(metadata))
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Window {
    pub revision: u64,
    pub properties: Properties,
}

impl Window {
    pub fn validate(&self) -> Result<(), String> {
        if self.revision == 0 { return Err("Window opening preferences need a revision.".into()); }
        self.properties.validate()
    }
}

pub fn revision(namespace: &str, value: &Value) -> Result<Option<u64>, String> {
    if namespace != RULE_NAMESPACE && namespace != WINDOW_NAMESPACE { return Ok(None); }
    if serde_json::to_vec(value).map_err(|error| error.to_string())?.len() > MAX_BYTES {
        return Err("Opening preferences exceed 64 KiB.".into());
    }
    let revision = if namespace == RULE_NAMESPACE {
        let rule: Rule = serde_json::from_value(value.clone()).map_err(|error| error.to_string())?;
        rule.validate()?;
        rule.revision
    } else {
        let window: Window = serde_json::from_value(value.clone()).map_err(|error| error.to_string())?;
        window.validate()?;
        window.revision
    };
    Ok(Some(revision))
}

#[cfg(test)]
mod tests;
