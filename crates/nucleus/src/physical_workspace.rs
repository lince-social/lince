use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::{Value, to_vec};
use sha2::{Digest, Sha256};

use crate::{canvas::Pose, json::lowercase_hex};

mod devices;
mod geometry;
mod observations;
/// Deterministic execution of recorded workspace events.
pub mod replay;
mod requests;

/// Maximum devices in an installation.
pub const MAX_DEVICES: usize = 16;
/// Maximum calibrated surfaces in an installation.
pub const MAX_SURFACES: usize = 32;
/// Maximum retained camera observations.
pub const MAX_OBSERVATIONS: usize = 64;
/// Maximum retained requests, including finished requests.
pub const MAX_REQUESTS: usize = 32;
/// Maximum concurrent requests, including ones waiting for a result.
pub const MAX_RUNNING: usize = 4;
/// Maximum age of camera evidence in milliseconds.
pub const MAX_AGE_MS: u64 = 5_000;
const MAX_SNAPSHOT_BYTES: usize = 64 * 1024;
const MIN_SELECTION_CONFIDENCE: f64 = 0.8;

/// Checks the length and character set for installation and runtime identifiers.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 96
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_.:".contains(&c))
}

/// Returns the lowercase SHA-256 revision of the encoded source snapshot.
///
/// # Errors
///
/// Returns a diagnostic if the value cannot be encoded or exceeds 64 KiB.
pub fn revision(value: &Value) -> Result<String, String> {
    let bytes = to_vec(value).map_err(|error| error.to_string())?;
    if bytes.len() > MAX_SNAPSHOT_BYTES {
        return Err("The source snapshot exceeds 64 KiB.".into());
    }
    Ok(lowercase_hex(&Sha256::digest(bytes)))
}

fn text(value: &str, limit: usize) -> bool {
    !value.trim().is_empty()
        && value.len() <= limit
        && !value.chars().any(|c| c.is_control() && c != '\n')
}

/// Hardware role in a calibrated installation.
#[expect(
    clippy::exhaustive_enums,
    reason = "callers match the published variants"
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DeviceKind {
    /// Captures evidence.
    Camera,
    /// Presents workspace output.
    Projector,
}

/// Camera or projector registered in an installation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Device {
    /// Installation-local identifier.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Role of the hardware.
    pub kind: DeviceKind,
    /// Pixel width and height.
    pub resolution: [u32; 2],
}

impl Device {
    /// Checks hardware identity, display name, and resolution bounds.
    pub fn valid(&self) -> bool {
        let [width, height] = self.resolution;
        valid_id(&self.id)
            && text(&self.name, 128)
            && self.resolution.iter().all(|v| (1..=8192).contains(v))
            && u64::from(width) * u64::from(height) <= 16_777_216
    }
}

/// Projective calibration in normalized coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Quad(
    /// Corners in top-left, top-right, bottom-right, bottom-left order.
    pub [[f64; 2]; 4],
);

/// One calibrated physical surface associated with a workspace.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Surface {
    /// Installation-local surface identifier.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Workspace that owns the surface.
    pub workspace: u64,
    /// Identifier of the output device.
    pub projector: String,
    /// Identifier of the optional capture device.
    pub camera: Option<String>,
    /// Physical pose, with position expressed in meters.
    pub physical_pose_meters: Pose,
    /// Physical width and height in meters.
    pub size_meters: [f64; 2],
    /// Positive revision of the calibration.
    pub calibration_revision: u64,
    /// Projector-space calibration.
    pub projector_quad: Quad,
    /// Camera-space calibration.
    pub camera_quad: Quad,
}

/// Devices and calibrated surfaces in one installation.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Configuration {
    /// Registered cameras and projectors.
    pub devices: Vec<Device>,
    /// Surfaces using the registered devices.
    pub surfaces: Vec<Surface>,
}

impl Configuration {
    /// Checks installation limits and required links between devices and surfaces.
    ///
    /// # Errors
    ///
    /// Returns the first device, surface, or relationship diagnostic.
    pub fn validate(&self) -> Result<(), String> {
        if self.devices.len() > MAX_DEVICES || self.surfaces.len() > MAX_SURFACES {
            return Err("An installation supports sixteen devices and thirty-two surfaces.".into());
        }
        let mut devices = BTreeMap::new();
        for device in &self.devices {
            if !device.valid() || devices.insert(device.id.as_str(), device.kind).is_some() {
                return Err("Invalid or duplicate device.".into());
            }
        }
        let mut ids = BTreeSet::new();
        for surface in &self.surfaces {
            if !surface.valid()
                || !ids.insert(&surface.id)
                || devices.get(surface.projector.as_str()) != Some(&DeviceKind::Projector)
                || surface
                    .camera
                    .as_deref()
                    .is_some_and(|id| devices.get(id) != Some(&DeviceKind::Camera))
            {
                return Err("Invalid surface, duplicate ID or mismatched device.".into());
            }
        }
        Ok(())
    }
}

/// The current session and capture status of a device.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Connection {
    /// Session identity.
    pub session: String,
    /// Whether capture is currently permitted.
    pub capture_enabled: bool,
    sequence: u64,
    time_ms: u64,
}

/// One timestamped capture from a calibrated camera.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Evidence {
    /// Observation identifier.
    pub id: String,
    /// Camera identifier.
    pub device: String,
    /// Session that captured the evidence.
    pub session: String,
    /// Increasing sequence in that session.
    pub sequence: u64,
    /// Logical capture time in milliseconds.
    pub captured_ms: u64,
    /// Calibrated surface identifier.
    pub surface: String,
    /// Revision used for the capture.
    pub calibration_revision: u64,
    /// Confidence on the normalized zero-to-one scale.
    pub confidence: f64,
    /// Optional media identifier.
    pub media: Option<String>,
    /// Whether the capture includes projected content.
    pub projected_content: bool,
}

/// Phase of a deliberate pointer interaction.
#[expect(
    clippy::exhaustive_enums,
    reason = "callers match the published variants"
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Phase {
    /// Starts selection.
    Begin,
    /// Moves an existing selection.
    Update,
    /// Commits an existing selection.
    End,
    /// Drops selection without a commit.
    Cancel,
}

/// A pointer action supported by camera evidence.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Interaction {
    /// Capture supporting the pointer action.
    pub evidence: Evidence,
    /// Pointer identifier.
    pub pointer: String,
    /// Action in the selection lifecycle.
    pub phase: Phase,
    /// Point in normalized camera coordinates.
    pub camera_normalized: [f64; 2],
}

/// Active selection state projected into local surface coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct Pointer {
    /// Surface being selected.
    pub surface: String,
    /// Camera supplying the selection.
    pub device: String,
    /// Active capture session.
    pub session: String,
    /// Revision used to locate the pointer.
    pub calibration_revision: u64,
    /// Normalized surface-local position.
    pub local: [f64; 2],
    /// Time of the last update in milliseconds.
    pub updated_ms: u64,
}

/// Exact source selected for a workspace request.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    /// Organ holding the source Record.
    pub organ: String,
    /// Selected Record identifier.
    pub record: String,
    /// Source snapshot revision.
    pub revision: String,
}

impl Target {
    /// Checks the Organ, Record and source revision identifiers.
    pub fn valid(&self) -> bool {
        valid_id(&self.organ) && valid_id(&self.record) && valid_id(&self.revision)
    }
}

/// State of a workspace request.
#[expect(
    clippy::exhaustive_enums,
    reason = "callers match the published variants"
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    /// Eligible to be considered for dispatch.
    Queued,
    /// Dispatched and active.
    Running,
    /// Active with a result pending.
    Waiting,
    /// Finished with a stored result.
    Completed,
    /// Canceled before completion.
    Canceled,
    /// Terminated after an invalid result.
    Failed,
}

/// One scoped request and its lifecycle state.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    /// Request identifier.
    pub id: String,
    /// Source surface.
    pub surface: String,
    /// User authorized to complete the request.
    pub user: String,
    /// Record and revision selected as the source.
    pub target: Target,
    /// Evidence identifiers needed by the request.
    pub observations: Vec<String>,
    /// Request text.
    pub prompt: String,
    /// Current lifecycle state.
    pub status: Status,
    /// Calibration revision used to create the request.
    pub calibration_revision: u64,
    /// Result text when completed.
    pub result: Option<String>,
    /// Creation time in milliseconds.
    pub created_ms: u64,
    /// Last update time in milliseconds.
    pub updated_ms: u64,
}

/// Selection committed on a physical surface.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Commit {
    /// Pointer that committed.
    pub pointer: String,
    /// Surface selected.
    pub surface: String,
    /// Normalized location on the surface.
    pub local: [f64; 2],
}

/// Live installation state on a monotonic adapter clock.
#[derive(Clone, Debug)]
pub struct Runtime {
    /// Current installation configuration.
    pub configuration: Configuration,
    /// Active device connections by identifier.
    pub connections: BTreeMap<String, Connection>,
    /// Retained evidence by observation identifier.
    pub observations: BTreeMap<String, Evidence>,
    /// Active selections by pointer identifier.
    pub pointers: BTreeMap<String, Pointer>,
    /// Retained requests keyed by their corresponding `Request::id`.
    pub requests: BTreeMap<String, Request>,
    /// Whether projection is blanked.
    pub blanked: bool,
    clock_ms: u64,
}

#[cfg(test)]
mod tests;
