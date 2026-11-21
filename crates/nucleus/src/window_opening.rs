use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{json::encoded_len, property_rule::PropertyRule};

/// Record extension containing a rule for matching windows.
pub const RULE_NAMESPACE: &str = "lince.park.opening-rule";
/// Record extension containing preferences for one window.
pub const WINDOW_NAMESPACE: &str = "lince.park.window-opening";
/// Maximum encoded size of either opening extension.
pub const MAX_BYTES: usize = 64 * 1024;
/// Maximum number of opening rules combined for an owner.
pub const MAX_RULES: usize = 256;

const MAX_PIXELS: u32 = 100_000;
const MAX_OUTPUT_BYTES: usize = 512;
const MAX_CONDITIONS: usize = 32;

/// Requested window width or height.
#[expect(
    clippy::exhaustive_enums,
    reason = "callers distinguish both supported dimension units"
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "kebab-case")]
pub enum Dimension {
    /// Fraction of available space as a numerator and denominator.
    Fraction([u16; 2]),
    /// Absolute size in pixels.
    Pixels(u32),
}

impl Dimension {
    /// Checks the supported positive range for this size.
    pub fn valid(self) -> bool {
        match self {
            Self::Fraction([numerator, denominator]) => numerator > 0 && numerator <= denominator,
            Self::Pixels(size) => (1..=MAX_PIXELS).contains(&size),
        }
    }

    /// Resolves a validated size against the available space and clamps it to the pixel limits.
    pub fn resolve(self, available: f64) -> f64 {
        match self {
            Self::Fraction([numerator, denominator]) => {
                available * f64::from(numerator) / f64::from(denominator)
            }
            Self::Pixels(size) => f64::from(size),
        }
        .clamp(1.0, f64::from(MAX_PIXELS))
    }
}

/// Sparse overrides applied when a window opens.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Properties {
    /// Destination workspace identifier.
    pub workspace: Option<u64>,
    /// Destination output name.
    pub output: Option<String>,
    /// Requested width.
    pub width: Option<Dimension>,
    /// Requested height.
    pub height: Option<Dimension>,
    /// Whether the window should float.
    pub floating: Option<bool>,
    /// Whether the window should maximize.
    pub maximized: Option<bool>,
    /// Whether the window should receive focus.
    pub focus: Option<bool>,
}

impl Properties {
    /// Checks the identifier, name, and dimension bounds for supplied overrides.
    ///
    /// # Errors
    ///
    /// Returns a diagnostic when an override is out of range or malformed.
    pub fn validate(&self) -> Result<(), String> {
        if self.workspace == Some(0)
            || self.output.as_deref().is_some_and(|output| {
                output.is_empty()
                    || output.len() > MAX_OUTPUT_BYTES
                    || output.chars().any(char::is_control)
            })
            || self.width.is_some_and(|width| !width.valid())
            || self.height.is_some_and(|height| !height.valid())
        {
            return Err("Choose an existing workspace/output and positive sizes up to 100000 pixels or fractions no greater than one.".into());
        }
        Ok(())
    }

    /// Applies this override over lower-priority properties, retaining explicit false values.
    pub fn over(&self, lower: &Self) -> Self {
        Self {
            workspace: self.workspace.or(lower.workspace),
            output: self.output.as_ref().or(lower.output.as_ref()).cloned(),
            width: self.width.or(lower.width),
            height: self.height.or(lower.height),
            floating: self.floating.or(lower.floating),
            maximized: self.maximized.or(lower.maximized),
            focus: self.focus.or(lower.focus),
        }
    }
}

/// A revisioned set of matching conditions and opening overrides.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    /// Positive revision of the stored rule.
    pub revision: u64,
    /// Whether the rule may match a window.
    pub enabled: bool,
    /// Whether all conditions must match. Otherwise at least one must match.
    pub match_all: bool,
    /// Conditions in evaluation order.
    pub conditions: Vec<PropertyRule>,
    /// Overrides supplied by a matching rule.
    pub properties: Properties,
}

impl Rule {
    /// Checks rule metadata and size before checking the supplied overrides.
    ///
    /// # Errors
    ///
    /// Returns the first rule or property validation diagnostic.
    pub fn validate(&self) -> Result<(), String> {
        if self.revision == 0
            || self.conditions.is_empty()
            || self.conditions.len() > MAX_CONDITIONS
            || self
                .conditions
                .iter()
                .any(|condition| !condition.validate())
            || encoded_len(self).map_err(|error| error.to_string())? > MAX_BYTES
        {
            return Err("An opening rule needs a revision and 1–32 valid matching conditions within 64 KiB.".into());
        }
        self.properties.validate()
    }

    /// Tests the conditions of an enabled, valid rule against the window metadata.
    pub fn matches(&self, metadata: &Value) -> bool {
        self.enabled
            && self.validate().is_ok()
            && if self.match_all {
                self.conditions
                    .iter()
                    .all(|condition| condition.matches(metadata))
            } else {
                self.conditions
                    .iter()
                    .any(|condition| condition.matches(metadata))
            }
    }
}

/// Revisioned opening overrides for a single window.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Window {
    /// Positive revision of the stored preferences.
    pub revision: u64,
    /// Overrides for the window.
    pub properties: Properties,
}

impl Window {
    /// Checks the revision and supplied opening overrides.
    ///
    /// # Errors
    ///
    /// Returns a revision diagnostic before any invalid-property diagnostic.
    pub fn validate(&self) -> Result<(), String> {
        if self.revision == 0 {
            return Err("Window opening preferences need a revision.".into());
        }
        self.properties.validate()
    }
}

/// Reports the revision of a valid opening extension, or `None` for other namespaces.
///
/// # Errors
///
/// Returns the first encoded size, structure, or validation diagnostic.
pub fn revision(namespace: &str, value: &Value) -> Result<Option<u64>, String> {
    if namespace != RULE_NAMESPACE && namespace != WINDOW_NAMESPACE {
        return Ok(None);
    }
    if encoded_len(value).map_err(|error| error.to_string())? > MAX_BYTES {
        return Err("Opening preferences exceed 64 KiB.".into());
    }
    let revision = if namespace == RULE_NAMESPACE {
        let rule = Rule::deserialize(value).map_err(|error| error.to_string())?;
        rule.validate()?;
        rule.revision
    } else {
        let window = Window::deserialize(value).map_err(|error| error.to_string())?;
        window.validate()?;
        window.revision
    };
    Ok(Some(revision))
}

#[cfg(test)]
mod tests;
