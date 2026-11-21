use crate::DecimalValue;

use super::{AuthorMode, PostDraft};

impl PostDraft {
    /// Validates limits, public labels, destinations, and the announcement quantity.
    ///
    /// # Errors
    ///
    /// Returns the first invalid field or transport configuration as an editor message.
    pub fn validate(&self) -> Result<(), String> {
        if let Some(transport) = &self.transport {
            transport.validate()?;
        }
        if self
            .lifetime_days
            .is_some_and(|days| !(1..=7).contains(&days))
        {
            return Err("Choose an announcement lifetime from one to seven days".into());
        }
        text(&self.title, 160, false)?;
        text(&self.text, 1200, true)?;
        text(&self.alias, 80, true)?;
        text(&self.language, 32, true)?;
        text(&self.area, 160, true)?;
        text(&self.availability, 160, true)?;
        if self.destinations.len() > 8
            || self
                .destinations
                .iter()
                .any(|s| s.is_empty() || s.len() > 128)
        {
            return Err("Choose at most eight valid publication services".into());
        }
        for value in [&self.unit, &self.concept].into_iter().flatten() {
            text(value, 160, false)?;
            if ["r_", "c_", "p_", "pl_"]
                .iter()
                .any(|prefix| value.starts_with(prefix))
            {
                return Err(
                    "Use a public concept/unit label rather than a private Record UID".into(),
                );
            }
        }
        if let Some(amount) = &self.quantity {
            let number = DecimalValue::parse_inferred(amount)
                .map_err(|_| "Enter an exact decimal quantity")?;
            if number.is_zero()
                || number.is_negative()
                || self.unit.as_deref().is_none_or(str::is_empty)
            {
                return Err("An optional quantity must be positive and have a unit; direction is selected separately".into());
            }
        }
        if self.mode == AuthorMode::Identified && !self.alias.is_empty() {
            return Err("An anonymous alias belongs to anonymous publication".into());
        }
        Ok(())
    }
}

/// Checks a Unicode character limit and permits tabs and newlines.
///
/// An input that trims to empty is accepted when `empty` is true.
///
/// # Errors
///
/// Returns an editor message for invalid characters, excessive length, or missing required text.
pub fn text(value: &str, max: usize, empty: bool) -> Result<(), String> {
    if (!empty && value.trim().is_empty())
        || value.chars().enumerate().any(|(index, character)| {
            index >= max || (character.is_control() && !matches!(character, '\n' | '\t'))
        })
    {
        Err(format!(
            "Use at most {max} characters and remove unsupported control characters"
        ))
    } else {
        Ok(())
    }
}
