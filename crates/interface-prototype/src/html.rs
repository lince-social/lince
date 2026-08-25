use crate::input::{ButtonState, InputEnvelope, NormalizedInput, Point, PointerButton, ScrollUnit};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeSet, VecDeque},
    fmt::{Display, Formatter},
};

pub const BRIDGE_SCHEMA_VERSION: u32 = 1;

const MAX_BRIDGE_BYTES: usize = 64 * 1024;
const MAX_PAYLOAD_BYTES: usize = 32 * 1024;
const MAX_IDENTIFIER_BYTES: usize = 256;
const MAX_ORIGIN_BYTES: usize = 2048;
const MAX_COLUMNS: usize = 64;
const MAX_MESSAGES_PER_SECOND: usize = 120;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageMode {
    Ephemeral,
    Persistent,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HtmlAuthority {
    Website,
    Installed,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaGrant {
    Camera,
    Microphone,
    DisplayCapture,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstalledGrants {
    pub protein_read: bool,
    pub events: BTreeSet<String>,
    pub actions: BTreeSet<String>,
    pub persistent_storage: bool,
    pub media: BTreeSet<MediaGrant>,
}

impl InstalledGrants {
    pub fn validate(&self) -> Result<(), BridgeError> {
        for event in &self.events {
            validate_identifier("event grant", event)?;
        }
        for action in &self.actions {
            validate_identifier("Action grant", action)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum HtmlSurfaceManifest {
    Website {
        partition_key: String,
        origin: String,
        persistent_storage: bool,
        media: BTreeSet<MediaGrant>,
    },
    Installed {
        partition_key: String,
        package_id: String,
        origin: String,
        grants: InstalledGrants,
    },
}

impl HtmlSurfaceManifest {
    pub fn validate(&self) -> Result<(), BridgeError> {
        match self {
            Self::Website {
                partition_key,
                origin,
                ..
            } => {
                validate_identifier("request-context partition", partition_key)?;
                validate_origin(origin)
            }
            Self::Installed {
                partition_key,
                package_id,
                origin,
                grants,
            } => {
                validate_identifier("request-context partition", partition_key)?;
                validate_identifier("installed package", package_id)?;
                validate_origin(origin)?;
                grants.validate()
            }
        }
    }

    pub fn authority(&self) -> HtmlAuthority {
        match self {
            Self::Website { .. } => HtmlAuthority::Website,
            Self::Installed { .. } => HtmlAuthority::Installed,
        }
    }

    pub fn origin(&self) -> &str {
        match self {
            Self::Website { origin, .. } | Self::Installed { origin, .. } => origin,
        }
    }

    pub fn request_context(&self) -> RequestContextPolicy {
        match self {
            Self::Website {
                partition_key,
                persistent_storage,
                media,
                ..
            } => RequestContextPolicy {
                partition_key: partition_key.clone(),
                storage: if *persistent_storage {
                    StorageMode::Persistent
                } else {
                    StorageMode::Ephemeral
                },
                network_access: true,
                lince_bridge: false,
                media: media.clone(),
            },
            Self::Installed {
                partition_key,
                grants,
                ..
            } => RequestContextPolicy {
                partition_key: partition_key.clone(),
                storage: if grants.persistent_storage {
                    StorageMode::Persistent
                } else {
                    StorageMode::Ephemeral
                },
                network_access: true,
                lince_bridge: true,
                media: grants.media.clone(),
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestContextPolicy {
    pub partition_key: String,
    pub storage: StorageMode,
    pub network_access: bool,
    pub lince_bridge: bool,
    pub media: BTreeSet<MediaGrant>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BridgeEnvelope {
    pub schema_version: u32,
    pub sequence: u64,
    pub request: BridgeRequest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum BridgeRequest {
    ProteinSubscribe {
        subscription_id: String,
        protein_id: String,
        columns: Vec<String>,
    },
    ProteinUnsubscribe {
        subscription_id: String,
    },
    EmitEvent {
        event: String,
        payload_json: String,
    },
    RequestAction {
        action: String,
        payload_json: String,
    },
}

impl BridgeRequest {
    fn validate(&self) -> Result<(), BridgeError> {
        match self {
            Self::ProteinSubscribe {
                subscription_id,
                protein_id,
                columns,
            } => {
                validate_identifier("subscription", subscription_id)?;
                validate_identifier("Protein", protein_id)?;
                if columns.len() > MAX_COLUMNS {
                    return Err(BridgeError::new(format!(
                        "Protein subscription exceeds {MAX_COLUMNS} columns"
                    )));
                }
                for column in columns {
                    validate_identifier("Protein column", column)?;
                }
                Ok(())
            }
            Self::ProteinUnsubscribe { subscription_id } => {
                validate_identifier("subscription", subscription_id)
            }
            Self::EmitEvent {
                event,
                payload_json,
            } => {
                validate_identifier("event", event)?;
                validate_payload(payload_json)
            }
            Self::RequestAction {
                action,
                payload_json,
            } => {
                validate_identifier("Action", action)?;
                validate_payload(payload_json)
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum BridgeDecision {
    Allowed {
        surface_id: String,
        browser_id: i32,
        sequence: u64,
        request: BridgeRequest,
    },
    Refused {
        surface_id: String,
        browser_id: i32,
        reason: String,
    },
}

#[derive(Clone, Debug)]
pub struct BridgeSession {
    surface_id: String,
    browser_id: i32,
    manifest: HtmlSurfaceManifest,
    last_sequence: u64,
    recent_messages: VecDeque<u64>,
}

impl BridgeSession {
    pub fn new(
        surface_id: impl Into<String>,
        browser_id: i32,
        manifest: HtmlSurfaceManifest,
    ) -> Result<Self, BridgeError> {
        let surface_id = surface_id.into();
        validate_identifier("HTML surface", &surface_id)?;
        if browser_id <= 0 {
            return Err(BridgeError::new("CEF browser id must be positive"));
        }
        manifest.validate()?;
        Ok(Self {
            surface_id,
            browser_id,
            manifest,
            last_sequence: 0,
            recent_messages: VecDeque::new(),
        })
    }

    pub fn handle_json(
        &mut self,
        bytes: &[u8],
        observed_origin: &str,
        now_millis: u64,
    ) -> BridgeDecision {
        match self.evaluate_json(bytes, observed_origin, now_millis) {
            Ok(envelope) => BridgeDecision::Allowed {
                surface_id: self.surface_id.clone(),
                browser_id: self.browser_id,
                sequence: envelope.sequence,
                request: envelope.request,
            },
            Err(error) => BridgeDecision::Refused {
                surface_id: self.surface_id.clone(),
                browser_id: self.browser_id,
                reason: error.to_string(),
            },
        }
    }

    fn evaluate_json(
        &mut self,
        bytes: &[u8],
        observed_origin: &str,
        now_millis: u64,
    ) -> Result<BridgeEnvelope, BridgeError> {
        if bytes.len() > MAX_BRIDGE_BYTES {
            return Err(BridgeError::new(format!(
                "bridge message exceeds {MAX_BRIDGE_BYTES} bytes"
            )));
        }
        if self.manifest.authority() == HtmlAuthority::Website {
            return Err(BridgeError::new("Website surfaces have no Lince bridge"));
        }
        while self
            .recent_messages
            .front()
            .is_some_and(|timestamp| now_millis.saturating_sub(*timestamp) >= 1000)
        {
            self.recent_messages.pop_front();
        }
        if self.recent_messages.len() >= MAX_MESSAGES_PER_SECOND {
            return Err(BridgeError::new("bridge message rate exceeded"));
        }
        self.recent_messages.push_back(now_millis);
        let envelope = serde_json::from_slice::<BridgeEnvelope>(bytes)
            .map_err(|error| BridgeError::new(format!("invalid bridge message: {error}")))?;
        if envelope.schema_version != BRIDGE_SCHEMA_VERSION {
            return Err(BridgeError::new(format!(
                "unsupported bridge schema version {}",
                envelope.schema_version
            )));
        }
        if envelope.sequence == 0 || envelope.sequence <= self.last_sequence {
            return Err(BridgeError::new("bridge sequence is not newer"));
        }
        validate_origin(observed_origin)?;
        if observed_origin != self.manifest.origin() {
            return Err(BridgeError::new(
                "bridge source origin does not match surface",
            ));
        }
        envelope.request.validate()?;
        self.authorize(&envelope.request)?;
        self.last_sequence = envelope.sequence;
        Ok(envelope)
    }

    fn authorize(&self, request: &BridgeRequest) -> Result<(), BridgeError> {
        let HtmlSurfaceManifest::Installed { grants, .. } = &self.manifest else {
            return Err(BridgeError::new("Website surfaces have no Lince bridge"));
        };
        match request {
            BridgeRequest::ProteinSubscribe { .. } if !grants.protein_read => {
                Err(BridgeError::new("installed Sand lacks Protein read grant"))
            }
            BridgeRequest::EmitEvent { event, .. } if !grants.events.contains(event) => {
                Err(BridgeError::new("installed Sand lacks event grant"))
            }
            BridgeRequest::RequestAction { action, .. } if !grants.actions.contains(action) => {
                Err(BridgeError::new("installed Sand lacks Action grant"))
            }
            _ => Ok(()),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum CefInputCommand {
    PointerMoved {
        x: i32,
        y: i32,
        modifiers: crate::input::InputModifiers,
    },
    PointerButton {
        x: i32,
        y: i32,
        button: CefPointerButton,
        state: ButtonState,
        modifiers: crate::input::InputModifiers,
    },
    Scroll {
        x: i32,
        y: i32,
        delta_x: i32,
        delta_y: i32,
        modifiers: crate::input::InputModifiers,
    },
    Key {
        logical_key: String,
        text: Option<String>,
        state: ButtonState,
        modifiers: crate::input::InputModifiers,
    },
    Focus {
        focused: bool,
    },
    ImePreedit {
        text: String,
        cursor: Option<[usize; 2]>,
    },
    ImeCommit {
        text: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CefPointerButton {
    Left,
    Right,
    Middle,
}

pub fn route_cef_input(envelope: &InputEnvelope) -> Result<CefInputCommand, BridgeError> {
    envelope
        .validate()
        .map_err(|error| BridgeError::new(error.to_string()))?;
    if envelope.target.adapter != "cef" {
        return Err(BridgeError::new("input target is not a CEF adapter"));
    }
    let local = envelope.target_local_position();
    if let Some(point) = local {
        validate_local_point(envelope, point)?;
    }
    match &envelope.event {
        NormalizedInput::PointerMoved { modifiers, .. } => {
            let (x, y) = cef_point(local)?;
            Ok(CefInputCommand::PointerMoved {
                x,
                y,
                modifiers: *modifiers,
            })
        }
        NormalizedInput::PointerButton {
            button,
            state,
            modifiers,
            ..
        } => {
            let (x, y) = cef_point(local)?;
            Ok(CefInputCommand::PointerButton {
                x,
                y,
                button: cef_button(*button)?,
                state: *state,
                modifiers: *modifiers,
            })
        }
        NormalizedInput::Scroll {
            delta,
            unit,
            modifiers,
            ..
        } => {
            let (x, y) = cef_point(local)?;
            let multiplier = if *unit == ScrollUnit::Lines {
                40.0
            } else {
                1.0
            };
            let transformed = envelope
                .target
                .local_from_surface
                .transform_vector(Point::new(delta.x * multiplier, delta.y * multiplier));
            Ok(CefInputCommand::Scroll {
                x,
                y,
                delta_x: cef_coordinate(transformed.x)?,
                delta_y: cef_coordinate(transformed.y)?,
                modifiers: *modifiers,
            })
        }
        NormalizedInput::Key {
            logical_key,
            text,
            state,
            modifiers,
            ..
        } => Ok(CefInputCommand::Key {
            logical_key: logical_key.clone(),
            text: text.clone(),
            state: *state,
            modifiers: *modifiers,
        }),
        NormalizedInput::ImePreedit { text, cursor } => Ok(CefInputCommand::ImePreedit {
            text: text.clone(),
            cursor: *cursor,
        }),
        NormalizedInput::ImeCommit { text } => {
            Ok(CefInputCommand::ImeCommit { text: text.clone() })
        }
        NormalizedInput::Focus { focused } => Ok(CefInputCommand::Focus { focused: *focused }),
        NormalizedInput::ModifiersChanged { .. }
        | NormalizedInput::Touch { .. }
        | NormalizedInput::ImeEnabled
        | NormalizedInput::ImeDisabled => Err(BridgeError::new(
            "normalized input kind has no direct CEF command",
        )),
    }
}

fn validate_local_point(envelope: &InputEnvelope, point: Point) -> Result<(), BridgeError> {
    let clip = envelope.target.local_clip;
    let maximum_x = clip.origin.x + clip.extent.x;
    let maximum_y = clip.origin.y + clip.extent.y;
    if point.x < clip.origin.x
        || point.y < clip.origin.y
        || point.x >= maximum_x
        || point.y >= maximum_y
    {
        return Err(BridgeError::new("input is outside the CEF surface clip"));
    }
    Ok(())
}

fn cef_point(point: Option<Point>) -> Result<(i32, i32), BridgeError> {
    let point = point.ok_or_else(|| BridgeError::new("CEF pointer input has no position"))?;
    Ok((cef_coordinate(point.x)?, cef_coordinate(point.y)?))
}

fn cef_coordinate(value: f64) -> Result<i32, BridgeError> {
    if !value.is_finite() || value < f64::from(i32::MIN) || value > f64::from(i32::MAX) {
        return Err(BridgeError::new("CEF input coordinate is out of range"));
    }
    Ok(value.round() as i32)
}

fn cef_button(button: PointerButton) -> Result<CefPointerButton, BridgeError> {
    match button {
        PointerButton::Left => Ok(CefPointerButton::Left),
        PointerButton::Right => Ok(CefPointerButton::Right),
        PointerButton::Middle => Ok(CefPointerButton::Middle),
        PointerButton::Back | PointerButton::Forward | PointerButton::Other(_) => Err(
            BridgeError::new("pointer button is unsupported by CEF route"),
        ),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BridgeError {
    message: String,
}

impl BridgeError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl Display for BridgeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for BridgeError {}

fn validate_identifier(label: &str, value: &str) -> Result<(), BridgeError> {
    if value.is_empty() {
        return Err(BridgeError::new(format!("{label} must not be empty")));
    }
    if value.len() > MAX_IDENTIFIER_BYTES {
        return Err(BridgeError::new(format!(
            "{label} exceeds {MAX_IDENTIFIER_BYTES} bytes"
        )));
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return Err(BridgeError::new(format!(
            "{label} contains unsupported characters"
        )));
    }
    Ok(())
}

fn validate_origin(value: &str) -> Result<(), BridgeError> {
    if value.is_empty() || value.len() > MAX_ORIGIN_BYTES {
        return Err(BridgeError::new("HTML origin length is invalid"));
    }
    let parsed = url::Url::parse(value)
        .map_err(|error| BridgeError::new(format!("HTML origin is invalid: {error}")))?;
    if !matches!(parsed.scheme(), "https" | "lince-sand") {
        return Err(BridgeError::new(
            "HTML origin must use https or the installed-Sand scheme",
        ));
    }
    if parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || !matches!(parsed.path(), "" | "/")
    {
        return Err(BridgeError::new("HTML origin must be an origin only"));
    }
    Ok(())
}

fn validate_payload(value: &str) -> Result<(), BridgeError> {
    if value.len() > MAX_PAYLOAD_BYTES {
        return Err(BridgeError::new(format!(
            "bridge payload exceeds {MAX_PAYLOAD_BYTES} bytes"
        )));
    }
    serde_json::from_str::<serde_json::Value>(value)
        .map(|_| ())
        .map_err(|error| BridgeError::new(format!("bridge payload is not JSON: {error}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{
        AffineTransform, InputModifiers, InputSource, InputSurface, InputTarget, Rect,
    };

    fn installed() -> HtmlSurfaceManifest {
        HtmlSurfaceManifest::Installed {
            partition_key: "installed-weather".into(),
            package_id: "weather.sand".into(),
            origin: "lince-sand://weather.sand".into(),
            grants: InstalledGrants {
                protein_read: true,
                events: BTreeSet::from(["record_clicked".into()]),
                actions: BTreeSet::from(["record.update".into()]),
                persistent_storage: true,
                media: BTreeSet::new(),
            },
        }
    }

    fn message(sequence: u64, request: BridgeRequest) -> Vec<u8> {
        serde_json::to_vec(&BridgeEnvelope {
            schema_version: BRIDGE_SCHEMA_VERSION,
            sequence,
            request,
        })
        .expect("serialize bridge fixture")
    }

    #[test]
    fn website_context_has_network_but_no_lince_bridge() {
        let manifest = HtmlSurfaceManifest::Website {
            partition_key: "website-example".into(),
            origin: "https://example.com".into(),
            persistent_storage: false,
            media: BTreeSet::new(),
        };
        let context = manifest.request_context();
        let mut session = BridgeSession::new("site", 1, manifest).expect("Website session");
        let decision = session.handle_json(b"not even parsed", "https://example.com", 0);

        assert!(context.network_access);
        assert!(!context.lince_bridge);
        assert_eq!(context.storage, StorageMode::Ephemeral);
        assert!(
            matches!(decision, BridgeDecision::Refused { reason, .. } if reason == "Website surfaces have no Lince bridge")
        );
    }

    #[test]
    fn installed_bridge_accepts_only_matching_origin_and_grants() {
        let mut session = BridgeSession::new("weather", 7, installed()).expect("installed session");
        let allowed = session.handle_json(
            &message(
                1,
                BridgeRequest::ProteinSubscribe {
                    subscription_id: "forecast".into(),
                    protein_id: "weather-current".into(),
                    columns: vec!["temperature".into()],
                },
            ),
            "lince-sand://weather.sand",
            10,
        );
        let refused = session.handle_json(
            &message(
                2,
                BridgeRequest::RequestAction {
                    action: "record.delete".into(),
                    payload_json: "{}".into(),
                },
            ),
            "lince-sand://weather.sand",
            11,
        );

        assert!(matches!(
            allowed,
            BridgeDecision::Allowed { browser_id: 7, .. }
        ));
        assert!(
            matches!(refused, BridgeDecision::Refused { reason, .. } if reason == "installed Sand lacks Action grant")
        );
    }

    #[test]
    fn bridge_fails_closed_for_unknown_operation_version_fields_and_replay() {
        let mut session = BridgeSession::new("weather", 7, installed()).expect("installed session");
        let unknown = br#"{"schema_version":1,"sequence":1,"request":{"op":"take_everything"}}"#;
        let wrong_version = br#"{"schema_version":2,"sequence":1,"request":{"op":"protein_unsubscribe","subscription_id":"forecast"}}"#;
        let extra = br#"{"schema_version":1,"sequence":1,"request":{"op":"protein_unsubscribe","subscription_id":"forecast","secret":true}}"#;
        let valid = message(
            1,
            BridgeRequest::ProteinUnsubscribe {
                subscription_id: "forecast".into(),
            },
        );

        assert!(matches!(
            session.handle_json(unknown, "lince-sand://weather.sand", 0),
            BridgeDecision::Refused { .. }
        ));
        assert!(matches!(
            session.handle_json(wrong_version, "lince-sand://weather.sand", 0),
            BridgeDecision::Refused { .. }
        ));
        assert!(matches!(
            session.handle_json(extra, "lince-sand://weather.sand", 0),
            BridgeDecision::Refused { .. }
        ));
        assert!(matches!(
            session.handle_json(&valid, "lince-sand://weather.sand", 0),
            BridgeDecision::Allowed { .. }
        ));
        assert!(
            matches!(session.handle_json(&valid, "lince-sand://weather.sand", 1), BridgeDecision::Refused { reason, .. } if reason == "bridge sequence is not newer")
        );
    }

    #[test]
    fn transformed_pointer_and_scroll_reach_cef_local_coordinates() {
        let surface = InputSurface::new("box", 1600, 1000, 2.0);
        let target = InputTarget {
            semantic_id: "external-weather".into(),
            adapter: "cef".into(),
            local_from_surface: AffineTransform {
                xx: 0.0,
                xy: -0.5,
                yx: 0.5,
                yy: 0.0,
                tx: 0.0,
                ty: 400.0,
            },
            local_clip: Rect {
                origin: Point::new(0.0, 0.0),
                extent: Point::new(500.0, 400.0),
            },
        };
        let pointer = InputEnvelope::new(
            1,
            InputSource::LinceWinit,
            surface.clone(),
            target.clone(),
            NormalizedInput::PointerMoved {
                surface_physical: Point::new(200.0, 600.0),
                buttons: Vec::new(),
                modifiers: InputModifiers::default(),
            },
        );
        let scroll = InputEnvelope::new(
            2,
            InputSource::LinceWinit,
            surface,
            target,
            NormalizedInput::Scroll {
                surface_physical: Point::new(200.0, 600.0),
                delta: Point::new(0.0, 3.0),
                unit: ScrollUnit::Lines,
                modifiers: InputModifiers::default(),
            },
        );

        assert_eq!(
            route_cef_input(&pointer).expect("pointer route"),
            CefInputCommand::PointerMoved {
                x: 300,
                y: 300,
                modifiers: InputModifiers::default(),
            }
        );
        assert_eq!(
            route_cef_input(&scroll).expect("scroll route"),
            CefInputCommand::Scroll {
                x: 300,
                y: 300,
                delta_x: 60,
                delta_y: 0,
                modifiers: InputModifiers::default(),
            }
        );
    }

    #[test]
    fn pointer_outside_transformed_clip_is_refused() {
        let surface = InputSurface::new("box", 800, 600, 1.0);
        let target = InputTarget {
            semantic_id: "external-weather".into(),
            adapter: "cef".into(),
            local_from_surface: AffineTransform {
                xx: 1.0,
                xy: 0.0,
                yx: 0.0,
                yy: 1.0,
                tx: -300.0,
                ty: 0.0,
            },
            local_clip: Rect {
                origin: Point::new(0.0, 0.0),
                extent: Point::new(200.0, 200.0),
            },
        };
        let input = InputEnvelope::new(
            1,
            InputSource::LinceWinit,
            surface,
            target,
            NormalizedInput::PointerMoved {
                surface_physical: Point::new(250.0, 100.0),
                buttons: Vec::new(),
                modifiers: InputModifiers::default(),
            },
        );

        assert_eq!(
            route_cef_input(&input).expect_err("outside clip"),
            BridgeError::new("input is outside the CEF surface clip")
        );
    }
}
