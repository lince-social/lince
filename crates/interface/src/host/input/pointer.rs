use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct PointerSettings {
    pub tap: Option<bool>,
    pub natural_scroll: Option<bool>,
    pub left_handed: Option<bool>,
    pub acceleration: Option<f64>,
}

impl PointerSettings {
    pub fn validate(&self) -> Result<(), String> {
        if self
            .acceleration
            .is_some_and(|speed| !speed.is_finite() || !(-1.0..=1.0).contains(&speed))
        {
            return Err(
                "Pointer acceleration must be a finite number from -1 to 1, or the device default."
                    .into(),
            );
        }
        Ok(())
    }

    pub fn resolve(&self, capabilities: &PointerCapabilities, defaults: &Self) -> Self {
        Self {
            tap: capabilities
                .tap
                .then(|| self.tap.or(defaults.tap))
                .flatten(),
            natural_scroll: capabilities
                .natural_scroll
                .then(|| self.natural_scroll.or(defaults.natural_scroll))
                .flatten(),
            left_handed: capabilities
                .left_handed
                .then(|| self.left_handed.or(defaults.left_handed))
                .flatten(),
            acceleration: capabilities
                .acceleration
                .then(|| self.acceleration.or(defaults.acceleration))
                .flatten(),
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PointerCapabilities {
    pub tap: bool,
    pub natural_scroll: bool,
    pub left_handed: bool,
    pub acceleration: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PointerDevice {
    pub id: String,
    pub name: String,
    pub capabilities: PointerCapabilities,
    pub defaults: PointerSettings,
    pub effective: PointerSettings,
    pub error: Option<String>,
}

pub type PointerRequest = super::InputRequest<PointerSettings>;

#[cfg_attr(feature = "ui", derive(bevy::prelude::Resource))]
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PointerSnapshot {
    pub available: bool,
    pub revision: u64,
    pub settings: PointerSettings,
    pub devices: Vec<PointerDevice>,
    pub response: Option<super::InputResponse>,
    pub last_request: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointer_preferences_resolve_defaults_and_omit_unsupported_controls() {
        let settings = PointerSettings {
            tap: Some(true),
            acceleration: Some(0.5),
            ..Default::default()
        };
        let mouse = PointerCapabilities {
            left_handed: true,
            acceleration: true,
            ..Default::default()
        };
        let defaults = PointerSettings {
            left_handed: Some(false),
            acceleration: Some(0.0),
            ..Default::default()
        };
        let effective = settings.resolve(&mouse, &defaults);
        assert_eq!(effective.tap, None);
        assert_eq!(effective.left_handed, Some(false));
        assert_eq!(effective.acceleration, Some(0.5));
        assert_eq!(
            PointerSettings::default().resolve(&mouse, &defaults),
            defaults
        );
        for acceleration in [f64::NAN, f64::INFINITY, -1.01, 1.01] {
            assert!(
                PointerSettings {
                    acceleration: Some(acceleration),
                    ..Default::default()
                }
                .validate()
                .is_err()
            );
        }
    }
}
