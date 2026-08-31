use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fmt::{Display, Formatter},
};

pub const INPUT_SCHEMA_VERSION: u32 = 1;

const MAX_IDENTIFIER_BYTES: usize = 256;
const MAX_KEY_BYTES: usize = 256;
const MAX_TEXT_BYTES: usize = 16 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputEnvelope {
    pub schema_version: u32,
    pub sequence: u64,
    pub source: InputSource,
    pub surface: InputSurface,
    pub target: InputTarget,
    pub event: NormalizedInput,
}

impl InputEnvelope {
    pub fn new(
        sequence: u64,
        source: InputSource,
        surface: InputSurface,
        target: InputTarget,
        event: NormalizedInput,
    ) -> Self {
        Self {
            schema_version: INPUT_SCHEMA_VERSION,
            sequence,
            source,
            surface,
            target,
            event,
        }
    }

    pub fn validate(&self) -> Result<(), InputValidationError> {
        if self.schema_version != INPUT_SCHEMA_VERSION {
            return Err(InputValidationError::new(format!(
                "unsupported input schema version {}",
                self.schema_version
            )));
        }
        if self.sequence == 0 {
            return Err(InputValidationError::new(
                "input sequence must be greater than zero",
            ));
        }
        self.surface.validate()?;
        self.target.validate()?;
        self.event.validate()?;
        Ok(())
    }

    pub fn target_local_position(&self) -> Option<Point> {
        self.event
            .surface_position()
            .map(|point| self.target.local_from_surface.transform(point))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputSource {
    LinceWinit,
    Replay,
}

impl InputSource {
    pub fn name(self) -> &'static str {
        match self {
            Self::LinceWinit => "lince_winit",
            Self::Replay => "replay",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputSurface {
    pub semantic_id: String,
    pub physical_width: u32,
    pub physical_height: u32,
    pub scale_factor: f64,
}

impl InputSurface {
    pub fn new(
        semantic_id: impl Into<String>,
        physical_width: u32,
        physical_height: u32,
        scale_factor: f64,
    ) -> Self {
        Self {
            semantic_id: semantic_id.into(),
            physical_width,
            physical_height,
            scale_factor,
        }
    }

    fn validate(&self) -> Result<(), InputValidationError> {
        validate_identifier("surface semantic id", &self.semantic_id)?;
        if self.physical_width == 0 || self.physical_height == 0 {
            return Err(InputValidationError::new(
                "input surface dimensions must be nonzero",
            ));
        }
        if !self.scale_factor.is_finite() || self.scale_factor <= 0.0 {
            return Err(InputValidationError::new(
                "input surface scale factor must be finite and positive",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputTarget {
    pub semantic_id: String,
    pub adapter: String,
    pub local_from_surface: AffineTransform,
    pub local_clip: Rect,
}

impl InputTarget {
    pub fn full_surface(
        semantic_id: impl Into<String>,
        adapter: impl Into<String>,
        surface: &InputSurface,
    ) -> Self {
        let inverse_scale = 1.0 / surface.scale_factor;
        Self {
            semantic_id: semantic_id.into(),
            adapter: adapter.into(),
            local_from_surface: AffineTransform {
                xx: inverse_scale,
                xy: 0.0,
                yx: 0.0,
                yy: inverse_scale,
                tx: 0.0,
                ty: 0.0,
            },
            local_clip: Rect {
                origin: Point::new(0.0, 0.0),
                extent: Point::new(
                    f64::from(surface.physical_width) * inverse_scale,
                    f64::from(surface.physical_height) * inverse_scale,
                ),
            },
        }
    }

    fn validate(&self) -> Result<(), InputValidationError> {
        validate_identifier("target semantic id", &self.semantic_id)?;
        validate_identifier("target adapter", &self.adapter)?;
        self.local_from_surface.validate()?;
        self.local_clip.validate()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AffineTransform {
    pub xx: f64,
    pub xy: f64,
    pub yx: f64,
    pub yy: f64,
    pub tx: f64,
    pub ty: f64,
}

impl AffineTransform {
    pub fn transform(self, point: Point) -> Point {
        Point::new(
            self.xx * point.x + self.yx * point.y + self.tx,
            self.xy * point.x + self.yy * point.y + self.ty,
        )
    }

    pub fn transform_vector(self, vector: Point) -> Point {
        Point::new(
            self.xx * vector.x + self.yx * vector.y,
            self.xy * vector.x + self.yy * vector.y,
        )
    }

    fn validate(self) -> Result<(), InputValidationError> {
        let values = [self.xx, self.xy, self.yx, self.yy, self.tx, self.ty];
        if values.iter().any(|value| !value.is_finite()) {
            return Err(InputValidationError::new(
                "input target transform must be finite",
            ));
        }
        let determinant = self.xx * self.yy - self.xy * self.yx;
        if !determinant.is_finite() || determinant.abs() <= f64::EPSILON {
            return Err(InputValidationError::new(
                "input target transform must be invertible",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rect {
    pub origin: Point,
    pub extent: Point,
}

impl Rect {
    fn validate(self) -> Result<(), InputValidationError> {
        self.origin.validate("input target clip origin")?;
        self.extent.validate("input target clip extent")?;
        if self.extent.x <= 0.0 || self.extent.y <= 0.0 {
            return Err(InputValidationError::new(
                "input target clip extent must be positive",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    fn validate(self, label: &str) -> Result<(), InputValidationError> {
        if !self.x.is_finite() || !self.y.is_finite() {
            return Err(InputValidationError::new(format!("{label} must be finite")));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputModifiers {
    pub control: bool,
    pub alt: bool,
    pub shift: bool,
    pub platform: bool,
    pub function: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PointerButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
    Other(u16),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ButtonState {
    Pressed,
    Released,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScrollUnit {
    PhysicalPixels,
    Lines,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TouchPhase {
    Started,
    Moved,
    Ended,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum NormalizedInput {
    PointerMoved {
        surface_physical: Point,
        buttons: Vec<PointerButton>,
        modifiers: InputModifiers,
    },
    PointerButton {
        surface_physical: Point,
        button: PointerButton,
        state: ButtonState,
        modifiers: InputModifiers,
    },
    Scroll {
        surface_physical: Point,
        delta: Point,
        unit: ScrollUnit,
        modifiers: InputModifiers,
    },
    Touch {
        touch_id: u64,
        surface_physical: Point,
        phase: TouchPhase,
    },
    Key {
        physical_key: Option<String>,
        logical_key: String,
        text: Option<String>,
        state: ButtonState,
        repeat: bool,
        modifiers: InputModifiers,
    },
    ModifiersChanged {
        modifiers: InputModifiers,
    },
    ImePreedit {
        text: String,
        cursor: Option<[usize; 2]>,
    },
    ImeCommit {
        text: String,
    },
    ImeEnabled,
    ImeDisabled,
    Focus {
        focused: bool,
    },
}

impl NormalizedInput {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::PointerMoved { .. } => "pointer_moved",
            Self::PointerButton { .. } => "pointer_button",
            Self::Scroll { .. } => "scroll",
            Self::Touch { .. } => "touch",
            Self::Key { .. } => "key",
            Self::ModifiersChanged { .. } => "modifiers_changed",
            Self::ImePreedit { .. } => "ime_preedit",
            Self::ImeCommit { .. } => "ime_commit",
            Self::ImeEnabled => "ime_enabled",
            Self::ImeDisabled => "ime_disabled",
            Self::Focus { .. } => "focus",
        }
    }

    pub fn surface_position(&self) -> Option<Point> {
        match self {
            Self::PointerMoved {
                surface_physical, ..
            }
            | Self::PointerButton {
                surface_physical, ..
            }
            | Self::Scroll {
                surface_physical, ..
            }
            | Self::Touch {
                surface_physical, ..
            } => Some(*surface_physical),
            _ => None,
        }
    }

    fn validate(&self) -> Result<(), InputValidationError> {
        if let Some(position) = self.surface_position() {
            position.validate("input surface position")?;
        }
        match self {
            Self::PointerMoved { buttons, .. } if buttons.len() > 32 => Err(
                InputValidationError::new("input pointer button set exceeds 32 entries"),
            ),
            Self::Scroll { delta, .. } => delta.validate("input scroll delta"),
            Self::Key {
                physical_key,
                logical_key,
                text,
                ..
            } => {
                if let Some(physical_key) = physical_key {
                    validate_text("physical key", physical_key, MAX_KEY_BYTES, false)?;
                }
                validate_text("logical key", logical_key, MAX_KEY_BYTES, false)?;
                if let Some(text) = text {
                    validate_text("key text", text, MAX_TEXT_BYTES, true)?;
                }
                Ok(())
            }
            Self::ImePreedit { text, cursor } => {
                validate_text("IME preedit", text, MAX_TEXT_BYTES, true)?;
                if let Some([start, end]) = cursor
                    && (start > end
                        || *end > text.len()
                        || !text.is_char_boundary(*start)
                        || !text.is_char_boundary(*end))
                {
                    return Err(InputValidationError::new(
                        "IME preedit cursor is not a valid byte range",
                    ));
                }
                Ok(())
            }
            Self::ImeCommit { text } => validate_text("IME commit", text, MAX_TEXT_BYTES, true),
            _ => Ok(()),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct InputEvidence {
    accepted: u64,
    rejected: u64,
    pointer: u64,
    keyboard: u64,
    focus: u64,
    ime: u64,
    last: Option<InputEnvelope>,
    last_rejection: Option<String>,
    last_sequence_by_source: BTreeMap<InputSource, u64>,
}

impl InputEvidence {
    pub fn record(&mut self, envelope: InputEnvelope) -> Result<(), InputValidationError> {
        if let Err(error) = envelope.validate() {
            self.rejected += 1;
            self.last_rejection = Some(error.to_string());
            return Err(error);
        }
        if self
            .last_sequence_by_source
            .get(&envelope.source)
            .is_some_and(|sequence| envelope.sequence <= *sequence)
        {
            let error = InputValidationError::new(format!(
                "input sequence {} is not newer for source {}",
                envelope.sequence,
                envelope.source.name()
            ));
            self.rejected += 1;
            self.last_rejection = Some(error.to_string());
            return Err(error);
        }
        self.last_sequence_by_source
            .insert(envelope.source, envelope.sequence);
        self.accepted += 1;
        match envelope.event {
            NormalizedInput::PointerMoved { .. }
            | NormalizedInput::PointerButton { .. }
            | NormalizedInput::Scroll { .. }
            | NormalizedInput::Touch { .. } => self.pointer += 1,
            NormalizedInput::Key { .. } | NormalizedInput::ModifiersChanged { .. } => {
                self.keyboard += 1;
            }
            NormalizedInput::ImePreedit { .. }
            | NormalizedInput::ImeCommit { .. }
            | NormalizedInput::ImeEnabled
            | NormalizedInput::ImeDisabled => self.ime += 1,
            NormalizedInput::Focus { .. } => self.focus += 1,
        }
        self.last = Some(envelope);
        Ok(())
    }

    pub fn accepted(&self) -> u64 {
        self.accepted
    }

    pub fn rejected(&self) -> u64 {
        self.rejected
    }

    pub fn pointer(&self) -> u64 {
        self.pointer
    }

    pub fn keyboard(&self) -> u64 {
        self.keyboard
    }

    pub fn focus(&self) -> u64 {
        self.focus
    }

    pub fn ime(&self) -> u64 {
        self.ime
    }

    pub fn last(&self) -> Option<&InputEnvelope> {
        self.last.as_ref()
    }

    pub fn last_rejection(&self) -> Option<&str> {
        self.last_rejection.as_deref()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputValidationError {
    message: String,
}

impl InputValidationError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl Display for InputValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for InputValidationError {}

fn validate_identifier(label: &str, value: &str) -> Result<(), InputValidationError> {
    validate_text(label, value, MAX_IDENTIFIER_BYTES, false)
}

fn validate_text(
    label: &str,
    value: &str,
    maximum_bytes: usize,
    empty_allowed: bool,
) -> Result<(), InputValidationError> {
    if !empty_allowed && value.is_empty() {
        return Err(InputValidationError::new(format!(
            "{label} must not be empty"
        )));
    }
    if value.len() > maximum_bytes {
        return Err(InputValidationError::new(format!(
            "{label} exceeds {maximum_bytes} bytes"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn envelope(event: NormalizedInput) -> InputEnvelope {
        let surface = InputSurface::new("box-main", 1600, 1000, 2.0);
        let target = InputTarget::full_surface("sand-fixture", "lince-winit", &surface);
        InputEnvelope::new(1, InputSource::LinceWinit, surface, target, event)
    }

    #[test]
    fn full_surface_target_maps_physical_to_local_logical_coordinates() {
        let envelope = envelope(NormalizedInput::PointerMoved {
            surface_physical: Point::new(320.0, 180.0),
            buttons: Vec::new(),
            modifiers: InputModifiers::default(),
        });

        assert_eq!(
            envelope.target_local_position(),
            Some(Point::new(160.0, 90.0))
        );
        assert_eq!(envelope.target.local_clip.extent, Point::new(800.0, 500.0));
        assert!(envelope.validate().is_ok());
    }

    #[test]
    fn full_surface_target_is_scale_invariant() {
        for scale_factor in [1.0, 1.25, 1.5, 2.0] {
            let surface = InputSurface::new(
                "box-main",
                (800.0 * scale_factor) as u32,
                (500.0 * scale_factor) as u32,
                scale_factor,
            );
            let target = InputTarget::full_surface("sand-fixture", "replay", &surface);
            let input = InputEnvelope::new(
                1,
                InputSource::Replay,
                surface,
                target,
                NormalizedInput::PointerMoved {
                    surface_physical: Point::new(160.0 * scale_factor, 90.0 * scale_factor),
                    buttons: Vec::new(),
                    modifiers: InputModifiers::default(),
                },
            );

            assert_eq!(input.target_local_position(), Some(Point::new(160.0, 90.0)));
            assert_eq!(input.target.local_clip.extent, Point::new(800.0, 500.0));
            assert!(input.validate().is_ok());
        }
    }

    #[test]
    fn unknown_schema_version_fails_closed() {
        let mut envelope = envelope(NormalizedInput::Focus { focused: true });
        envelope.schema_version += 1;

        assert_eq!(
            envelope
                .validate()
                .expect_err("unknown version must fail")
                .to_string(),
            "unsupported input schema version 2"
        );
    }

    #[test]
    fn unknown_event_kind_fails_deserialization() {
        let json = serde_json::to_value(envelope(NormalizedInput::Focus { focused: true }))
            .expect("serialize input envelope");
        let mut json = json.as_object().expect("envelope object").clone();
        json.get_mut("event")
            .and_then(serde_json::Value::as_object_mut)
            .expect("event object")
            .insert("kind".into(), "future_input_kind".into());

        assert!(serde_json::from_value::<InputEnvelope>(json.into()).is_err());
    }

    #[test]
    fn evidence_rejects_invalid_payload_without_counting_it_as_input() {
        let mut evidence = InputEvidence::default();
        let input = envelope(NormalizedInput::ImeCommit {
            text: "x".repeat(MAX_TEXT_BYTES + 1),
        });

        assert!(evidence.record(input).is_err());
        assert_eq!(evidence.accepted(), 0);
        assert_eq!(evidence.rejected(), 1);
        assert_eq!(evidence.ime(), 0);
        assert!(evidence.last().is_none());
    }

    #[test]
    fn evidence_refuses_replayed_or_out_of_order_sequences_per_source() {
        let mut evidence = InputEvidence::default();
        let input = envelope(NormalizedInput::Focus { focused: true });

        evidence.record(input.clone()).expect("first input");
        assert_eq!(
            evidence
                .record(input)
                .expect_err("replayed sequence must fail")
                .to_string(),
            "input sequence 1 is not newer for source lince_winit"
        );
        assert_eq!(evidence.accepted(), 1);
        assert_eq!(evidence.rejected(), 1);
    }

    #[test]
    fn unknown_fields_fail_deserialization() {
        let json = serde_json::json!({
            "schema_version": INPUT_SCHEMA_VERSION,
            "sequence": 1,
            "source": "lince_winit",
            "surface": {
                "semantic_id": "box-main",
                "physical_width": 100,
                "physical_height": 100,
                "scale_factor": 1.0,
                "future_field": true
            },
            "target": {
                "semantic_id": "sand-fixture",
                "adapter": "lince-winit",
                "local_from_surface": {
                    "xx": 1.0,
                    "xy": 0.0,
                    "yx": 0.0,
                    "yy": 1.0,
                    "tx": 0.0,
                    "ty": 0.0
                },
                "local_clip": {
                    "origin": {"x": 0.0, "y": 0.0},
                    "extent": {"x": 100.0, "y": 100.0}
                }
            },
            "event": {"kind": "focus", "focused": true}
        });

        assert!(serde_json::from_value::<InputEnvelope>(json).is_err());
    }
}
