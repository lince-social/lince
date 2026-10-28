use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct AbsolutePreference {
    pub output: Option<String>,
    pub calibration: Option<[f32; 6]>,
    pub buttons: BTreeMap<u32, u32>,
}

impl AbsolutePreference {
    pub fn validate(&self) -> Result<(), String> {
        if self.output.as_ref().is_some_and(|name| name.is_empty() || name.len() > 256 || name.chars().any(char::is_control)) {
            return Err("Choose an output name of at most 256 characters.".into());
        }
        if let Some([a, b, c, d, e, f]) = self.calibration {
            let determinant = f64::from(a) * f64::from(e) - f64::from(b) * f64::from(d);
            if [a, b, c, d, e, f].iter().any(|value| !value.is_finite() || value.abs() > 16.0)
                || !determinant.is_finite() || determinant.abs() < 1e-8 {
                return Err("Calibration needs six finite values between -16 and 16 with an invertible linear transform.".into());
            }
        }
        if self.buttons.len() > 16 || self.buttons.iter().any(|(source, target)| !(0x100..=0x15f).contains(source) || !(0x100..=0x15f).contains(target)) {
            return Err("Map at most 16 supported tool buttons using Linux button codes 256–351.".into());
        }
        Ok(())
    }
}

pub type AbsolutePreferences = BTreeMap<String, AbsolutePreference>;

pub fn validate_absolute(settings: &AbsolutePreferences) -> Result<(), String> {
    if settings.len() > 128 || settings.keys().any(|key| key.is_empty() || key.len() > 1024 || key.chars().any(char::is_control)) {
        return Err("Absolute input settings accept at most 128 stable device identities.".into());
    }
    for preference in settings.values() { preference.validate()?; }
    Ok(())
}

#[derive(Clone, Debug, PartialEq)]
pub struct AbsoluteDevice {
    pub id: String,
    pub key: Option<String>,
    pub generation: u64,
    pub name: String,
    pub touch: bool,
    pub tablet: bool,
    pub calibration: bool,
    pub default_calibration: Option<[f32; 6]>,
    pub effective_calibration: Option<[f32; 6]>,
    pub associated_output: Option<String>,
    pub buttons: Vec<u32>,
    pub error: Option<String>,
}

pub type AbsoluteRequest = super::InputRequest<AbsolutePreferences>;
pub type AbsoluteReload = super::InputRequest<()>;

#[derive(Clone, Debug, PartialEq)]
pub enum AbsoluteCommand { Save(AbsoluteRequest), Reload(AbsoluteReload) }

impl AbsoluteCommand {
    pub fn id(&self) -> u64 { match self { Self::Save(request) => request.id, Self::Reload(request) => request.id } }
    pub fn revision(&self) -> u64 { match self { Self::Save(request) => request.revision, Self::Reload(request) => request.revision } }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TabletAxes {
    pub pressure: Option<f64>,
    pub distance: Option<f64>,
    pub tilt: Option<[f64; 2]>,
    pub rotation: Option<f64>,
    pub slider: Option<f64>,
    pub wheel: Option<(f64, i32)>,
}

impl TabletAxes {
    pub fn valid(self) -> bool {
        let bounded = |value: Option<f64>, low: f64, high: f64| value.is_none_or(|value| value.is_finite() && (low..=high).contains(&value));
        bounded(self.pressure, 0.0, 1.0) && bounded(self.distance, 0.0, 1.0)
            && self.tilt.is_none_or(|tilt| tilt.into_iter().all(|value| bounded(Some(value), -90.0, 90.0)))
            && bounded(self.rotation, 0.0, 360.0) && bounded(self.slider, -1.0, 1.0)
            && self.wheel.is_none_or(|(degrees, clicks)| degrees.is_finite() && degrees.abs() <= 36000.0 && clicks.unsigned_abs() <= 100000)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TabletPhase {
    Enter,
    Motion,
    Tip(bool),
    Button { code: u32, pressed: bool },
    Leave,
    Cancel,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TabletKind { Pen, Eraser, Brush, Pencil, Airbrush, Mouse, Lens, Totem, Unknown }

#[cfg_attr(feature = "ui", derive(bevy::prelude::Message, bevy::prelude::Component))]
#[derive(Clone, Debug, PartialEq)]
pub struct TabletInput {
    pub tool: u64,
    pub output: super::super::OutputId,
    pub point: [f64; 2],
    pub axes: TabletAxes,
    pub kind: TabletKind,
    pub phase: TabletPhase,
}

#[cfg_attr(feature = "ui", derive(bevy::prelude::Resource))]
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AbsoluteSnapshot {
    pub available: bool,
    pub revision: u64,
    pub settings: AbsolutePreferences,
    pub devices: Vec<AbsoluteDevice>,
    pub response: Option<super::InputResponse>,
    pub last_request: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calibration_and_button_limits_reject_nonfinite_singular_and_key_injection() {
        let mut value = AbsolutePreference { calibration: Some([1.0, 0.0, 0.0, 0.0, 1.0, 0.0]), ..Default::default() };
        assert!(value.validate().is_ok());
        for matrix in [[0.0; 6], [f32::NAN; 6], [f32::INFINITY; 6], [17.0, 0.0, 0.0, 0.0, 1.0, 0.0]] {
            value.calibration = Some(matrix);
            assert!(value.validate().is_err());
        }
        value.calibration = None;
        value.buttons.insert(331, 30);
        assert!(value.validate().is_err());
        value.buttons.insert(331, 332);
        assert!(value.validate().is_ok());
        value.output = Some("DP-1\n".into());
        assert!(value.validate().is_err());
    }

    #[test]
    fn reported_axes_distinguish_unavailable_values_and_reject_nonfinite_or_out_of_range_data() {
        assert!(TabletAxes::default().valid());
        assert!(TabletAxes { pressure: Some(0.5), distance: Some(0.25), tilt: Some([-45.0, 12.0]), rotation: Some(90.0), slider: Some(-0.5), wheel: Some((15.0, 1)) }.valid());
        for value in [f64::NAN, f64::INFINITY, -0.01, 1.01] {
            assert!(!TabletAxes { pressure: Some(value), ..Default::default() }.valid());
        }
        assert!(!TabletAxes { tilt: Some([91.0, 0.0]), ..Default::default() }.valid());
        assert!(!TabletAxes { wheel: Some((f64::NAN, 1)), ..Default::default() }.valid());
        assert!(!TabletAxes { wheel: Some((15.0, i32::MIN)), ..Default::default() }.valid());
    }
}
