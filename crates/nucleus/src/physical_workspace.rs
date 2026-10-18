use crate::canvas::{Pose, room};
use glam::{DMat3, DVec2, DVec3};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub mod replay;

pub const MAX_DEVICES: usize = 16;
pub const MAX_SURFACES: usize = 32;
pub const MAX_OBSERVATIONS: usize = 64;
pub const MAX_REQUESTS: usize = 32;
pub const MAX_RUNNING: usize = 4;
pub const MAX_AGE_MS: u64 = 5_000;

pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 96
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_.:".contains(&c))
}

pub fn revision(value: &serde_json::Value) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    let bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    if bytes.len() > 65_536 {
        return Err("The source snapshot exceeds 64 KiB.".into());
    }
    Ok(Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn text(value: &str, limit: usize) -> bool {
    !value.trim().is_empty()
        && value.len() <= limit
        && !value.chars().any(|c| c.is_control() && c != '\n')
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DeviceKind {
    Camera,
    Projector,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub kind: DeviceKind,
    pub resolution: [u32; 2],
}

impl Device {
    pub fn valid(&self) -> bool {
        valid_id(&self.id)
            && text(&self.name, 128)
            && self.resolution.iter().all(|v| (1..=8192).contains(v))
            && u64::from(self.resolution[0]) * u64::from(self.resolution[1]) <= 16_777_216
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Quad(pub [[f64; 2]; 4]);

impl Default for Quad {
    fn default() -> Self {
        Self([[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]])
    }
}

impl Quad {
    pub fn matrix(&self) -> Result<DMat3, String> {
        let points = self.0.map(DVec2::from_array);
        if points
            .iter()
            .any(|p| !p.is_finite() || p.min_element() < 0.0 || p.max_element() > 1.0)
        {
            return Err("Calibration corners must be finite normalized coordinates.".into());
        }
        let crosses = std::array::from_fn::<_, 4, _>(|i| {
            (points[(i + 1) % 4] - points[i]).perp_dot(points[(i + 2) % 4] - points[(i + 1) % 4])
        });
        if crosses.iter().any(|v| *v <= 1e-6) {
            return Err("Use four distinct convex corners in top-left, top-right, bottom-right, bottom-left order.".into());
        }
        let [a, b, c, d] = points;
        let delta = a - b + c - d;
        let u = b - c;
        let v = d - c;
        let denominator = u.perp_dot(v);
        if denominator.abs() < 1e-10 {
            return Err("The calibration is degenerate.".into());
        }
        let g = delta.perp_dot(v) / denominator;
        let h = u.perp_dot(delta) / denominator;
        let matrix = DMat3::from_cols(
            (b - a + g * b).extend(g),
            (d - a + h * d).extend(h),
            a.extend(1.0),
        );
        if !matrix.is_finite()
            || matrix.determinant().abs() < 1e-10
            || [1.0, 1.0 + g, 1.0 + h, 1.0 + g + h]
                .iter()
                .any(|w| *w < 1e-6)
        {
            return Err("The calibration crosses a projective singularity.".into());
        }
        Ok(matrix)
    }

    pub fn map(&self, point: [f64; 2], inverse: bool) -> Result<[f64; 2], String> {
        if point
            .iter()
            .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
        {
            return Err("The point must be finite.".into());
        }
        let matrix = self.matrix()?;
        let value =
            if inverse { matrix.inverse() } else { matrix } * DVec3::new(point[0], point[1], 1.0);
        if !value.is_finite() || value.z <= 1e-8 {
            return Err("The point is outside the calibrated plane.".into());
        }
        let output = (value.truncate() / value.z).to_array();
        if output.iter().any(|v| *v < -1e-8 || *v > 1.0 + 1e-8) {
            return Err("The point is outside the calibrated surface.".into());
        }
        Ok(output.map(|v| v.clamp(0.0, 1.0)))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Surface {
    pub id: String,
    pub name: String,
    pub workspace: u64,
    pub projector: String,
    pub camera: Option<String>,
    pub physical_pose_meters: Pose,
    pub size_meters: [f64; 2],
    pub calibration_revision: u64,
    pub projector_quad: Quad,
    pub camera_quad: Quad,
}

impl Surface {
    pub fn valid(&self) -> bool {
        valid_id(&self.id)
            && text(&self.name, 128)
            && self.workspace > 0
            && valid_id(&self.projector)
            && self.camera.as_ref().is_none_or(|id| valid_id(id))
            && room::valid_pose(&self.physical_pose_meters)
            && self
                .size_meters
                .iter()
                .all(|v| v.is_finite() && *v >= 0.01 && *v <= 100.0)
            && self.calibration_revision > 0
            && self.projector_quad.matrix().is_ok()
            && self.camera_quad.matrix().is_ok()
    }

    pub fn physical_point(&self, local: [f64; 2]) -> Result<[f64; 3], String> {
        if !self.valid()
            || local
                .iter()
                .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
        {
            return Err("Invalid surface or local coordinate.".into());
        }
        let point = room::compose(
            &self.physical_pose_meters,
            &Pose {
                position: [
                    local[0] * self.size_meters[0],
                    local[1] * self.size_meters[1],
                    0.0,
                ],
                rotation: [0.0, 0.0, 0.0, 1.0],
            },
        );
        if !room::valid_pose(&point) {
            return Err("The physical point exceeds finite bounds.".into());
        }
        Ok(point.position)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Configuration {
    pub devices: Vec<Device>,
    pub surfaces: Vec<Surface>,
}

impl Configuration {
    pub fn validate(&self) -> Result<(), String> {
        if self.devices.len() > MAX_DEVICES || self.surfaces.len() > MAX_SURFACES {
            return Err("An installation supports sixteen devices and thirty-two surfaces.".into());
        }
        let mut devices = BTreeMap::new();
        for device in &self.devices {
            if !device.valid() || devices.insert(&device.id, device).is_some() {
                return Err("Invalid or duplicate device.".into());
            }
        }
        let mut ids = BTreeSet::new();
        for surface in &self.surfaces {
            if !surface.valid()
                || !ids.insert(&surface.id)
                || !devices
                    .get(&surface.projector)
                    .is_some_and(|d| d.kind == DeviceKind::Projector)
                || surface.camera.as_ref().is_some_and(|id| {
                    !devices
                        .get(id)
                        .is_some_and(|d| d.kind == DeviceKind::Camera)
                })
            {
                return Err("Invalid surface, duplicate ID or mismatched device.".into());
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Connection {
    pub session: String,
    pub capture_enabled: bool,
    sequence: u64,
    time_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Evidence {
    pub id: String,
    pub device: String,
    pub session: String,
    pub sequence: u64,
    pub captured_ms: u64,
    pub surface: String,
    pub calibration_revision: u64,
    pub confidence: f64,
    pub media: Option<String>,
    pub projected_content: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Phase {
    Begin,
    Update,
    End,
    Cancel,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Interaction {
    pub evidence: Evidence,
    pub pointer: String,
    pub phase: Phase,
    pub camera_normalized: [f64; 2],
}

#[derive(Clone, Debug, PartialEq)]
pub struct Pointer {
    pub surface: String,
    pub device: String,
    pub session: String,
    pub calibration_revision: u64,
    pub local: [f64; 2],
    pub updated_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub organ: String,
    pub record: String,
    pub revision: String,
}

impl Target {
    pub fn valid(&self) -> bool {
        valid_id(&self.organ) && valid_id(&self.record) && valid_id(&self.revision)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    Queued,
    Running,
    Waiting,
    Completed,
    Canceled,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub id: String,
    pub surface: String,
    pub user: String,
    pub target: Target,
    pub observations: Vec<String>,
    pub prompt: String,
    pub status: Status,
    pub calibration_revision: u64,
    pub result: Option<String>,
    pub created_ms: u64,
    pub updated_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Commit {
    pub pointer: String,
    pub surface: String,
    pub local: [f64; 2],
}

#[derive(Clone, Debug)]
pub struct Runtime {
    pub configuration: Configuration,
    pub connections: BTreeMap<String, Connection>,
    pub observations: BTreeMap<String, Evidence>,
    pub pointers: BTreeMap<String, Pointer>,
    pub requests: BTreeMap<String, Request>,
    pub blanked: bool,
    clock_ms: u64,
}

impl Default for Runtime {
    fn default() -> Self {
        Self {
            configuration: Configuration::default(),
            connections: BTreeMap::new(),
            observations: BTreeMap::new(),
            pointers: BTreeMap::new(),
            requests: BTreeMap::new(),
            blanked: true,
            clock_ms: 0,
        }
    }
}

impl Runtime {
    pub fn new(configuration: Configuration) -> Result<Self, String> {
        configuration.validate()?;
        Ok(Self {
            configuration,
            blanked: true,
            ..Self::default()
        })
    }

    pub fn configure(&mut self, configuration: Configuration) -> Result<(), String> {
        configuration.validate()?;
        for surface in &configuration.surfaces {
            if let Some(old) = self
                .configuration
                .surfaces
                .iter()
                .find(|s| s.id == surface.id)
                && surface != old
                && surface.calibration_revision <= old.calibration_revision
            {
                return Err("Changed surfaces require a newer calibration revision.".into());
            }
        }
        if configuration != self.configuration {
            let changed: BTreeSet<_> = self
                .configuration
                .surfaces
                .iter()
                .filter(|old| !configuration.surfaces.contains(old))
                .map(|s| s.id.clone())
                .collect();
            let unchanged_devices: BTreeSet<_> = self
                .configuration
                .devices
                .iter()
                .filter(|old| configuration.devices.contains(old))
                .map(|d| d.id.clone())
                .collect();
            let preserved_devices: BTreeSet<_> = self
                .configuration
                .surfaces
                .iter()
                .filter(|s| !changed.contains(&s.id))
                .flat_map(|s| std::iter::once(s.projector.clone()).chain(s.camera.clone()))
                .collect();
            let removed_devices: Vec<_> = self
                .connections
                .keys()
                .filter(|id| !unchanged_devices.contains(*id) || !preserved_devices.contains(*id))
                .cloned()
                .collect();
            for device in removed_devices {
                self.disconnect(&device);
            }
            self.pointers.retain(|_, p| !changed.contains(&p.surface));
            self.observations
                .retain(|_, e| !changed.contains(&e.surface));
            for request in self.requests.values_mut() {
                if changed.contains(&request.surface) && !request.terminal() {
                    request.status = Status::Canceled;
                }
            }
            if self
                .configuration
                .surfaces
                .iter()
                .all(|s| changed.contains(&s.id))
            {
                self.blanked = true;
            }
        }
        self.configuration = configuration;
        Ok(())
    }

    pub fn connect(&mut self, device: &str, session: &str) -> Result<(), String> {
        if !valid_id(session) || !self.configuration.devices.iter().any(|d| d.id == device) {
            return Err("Unknown device or invalid session.".into());
        }
        if self
            .connections
            .get(device)
            .is_some_and(|c| c.session == session)
        {
            return Err("A reconnect requires a new session identity.".into());
        }
        self.disconnect(device);
        self.connections.insert(
            device.into(),
            Connection {
                session: session.into(),
                capture_enabled: false,
                sequence: 0,
                time_ms: 0,
            },
        );
        Ok(())
    }

    pub fn capture(&mut self, device: &str, enabled: bool) -> Result<(), String> {
        if !self
            .configuration
            .devices
            .iter()
            .any(|d| d.id == device && d.kind == DeviceKind::Camera)
        {
            return Err("Select a camera.".into());
        }
        self.connections
            .get_mut(device)
            .ok_or("Camera is disconnected.")?
            .capture_enabled = enabled;
        if !enabled {
            self.release_device(device);
        }
        Ok(())
    }

    fn release_device(&mut self, device: &str) {
        let removed: BTreeSet<_> = self
            .observations
            .values()
            .filter(|e| e.device == device)
            .map(|e| e.id.clone())
            .collect();
        self.observations.retain(|_, e| e.device != device);
        self.pointers.retain(|_, p| p.device != device);
        for request in self.requests.values_mut() {
            if request.observations.iter().any(|id| removed.contains(id)) && !request.terminal() {
                request.status = Status::Canceled;
            }
        }
    }

    pub fn disconnect(&mut self, device: &str) {
        self.connections.remove(device);
        self.release_device(device);
        let affected: BTreeSet<_> = self
            .configuration
            .surfaces
            .iter()
            .filter(|s| s.projector == device || s.camera.as_deref() == Some(device))
            .map(|s| s.id.clone())
            .collect();
        self.pointers.retain(|_, p| !affected.contains(&p.surface));
        for request in self.requests.values_mut() {
            if affected.contains(&request.surface) && !request.terminal() {
                request.status = Status::Canceled;
            }
        }
    }

    pub fn surface(&self, id: &str, workspace: u64) -> Result<&Surface, String> {
        self.configuration
            .surfaces
            .iter()
            .find(|s| s.id == id && s.workspace == workspace)
            .ok_or("The surface is unavailable in this workspace.".into())
    }

    pub fn advance(&mut self, now_ms: u64) -> Result<(), String> {
        if now_ms < self.clock_ms {
            return Err("The adapter clock moved backwards.".into());
        }
        self.clock_ms = now_ms;
        self.pointers
            .retain(|_, p| now_ms.saturating_sub(p.updated_ms) <= 500);
        self.observations
            .retain(|_, e| now_ms.saturating_sub(e.captured_ms) <= MAX_AGE_MS);
        for request in self.requests.values_mut() {
            if !request.terminal()
                && (now_ms.saturating_sub(request.created_ms) > 120_000
                    || request.status == Status::Queued
                        && request
                            .observations
                            .iter()
                            .any(|id| !self.observations.contains_key(id)))
            {
                request.status = Status::Canceled;
            }
        }
        Ok(())
    }

    fn validate_evidence(
        &self,
        workspace: u64,
        evidence: &Evidence,
        now_ms: u64,
    ) -> Result<(), String> {
        let surface = self.surface(&evidence.surface, workspace)?;
        let connection = self
            .connections
            .get(&evidence.device)
            .ok_or("Camera is disconnected.")?;
        if !valid_id(&evidence.id)
            || !evidence.confidence.is_finite()
            || !(0.0..=1.0).contains(&evidence.confidence)
            || evidence.media.as_ref().is_some_and(|id| !valid_id(id))
            || surface.camera.as_deref() != Some(&evidence.device)
            || surface.calibration_revision != evidence.calibration_revision
            || connection.session != evidence.session
            || !connection.capture_enabled
            || evidence.sequence <= connection.sequence
            || evidence.captured_ms < connection.time_ms
            || evidence.captured_ms > now_ms
            || now_ms.saturating_sub(evidence.captured_ms) > MAX_AGE_MS
            || self.observations.contains_key(&evidence.id)
        {
            return Err("Invalid, stale, duplicate or unapproved camera evidence.".into());
        }
        Ok(())
    }

    pub fn observe(
        &mut self,
        workspace: u64,
        evidence: Evidence,
        now_ms: u64,
    ) -> Result<(), String> {
        self.advance(now_ms)?;
        self.validate_evidence(workspace, &evidence, now_ms)?;
        if self.observations.contains_key(&evidence.id)
            || self
                .requests
                .values()
                .any(|request| request.observations.contains(&evidence.id))
        {
            return Err("Observation identity is already in use.".into());
        }
        if self.observations.len() == MAX_OBSERVATIONS {
            return Err(
                "Observation capacity reached; wait for old observations to expire.".into(),
            );
        }
        let connection = self.connections.get_mut(&evidence.device).unwrap();
        connection.sequence = evidence.sequence;
        connection.time_ms = evidence.captured_ms;
        self.observations.insert(evidence.id.clone(), evidence);
        Ok(())
    }

    pub fn interact(
        &mut self,
        workspace: u64,
        interaction: Interaction,
        now_ms: u64,
    ) -> Result<Option<Commit>, String> {
        self.advance(now_ms)?;
        if !valid_id(&interaction.pointer) {
            return Err("Invalid pointer identity.".into());
        }
        let evidence = &interaction.evidence;
        let existing = self.pointers.get(&interaction.pointer).cloned();
        let result = (|| {
            self.validate_evidence(workspace, evidence, now_ms)?;
            if self.blanked
                || !self
                    .connections
                    .contains_key(&self.surface(&evidence.surface, workspace)?.projector)
            {
                return Err("Projection is blanked or disconnected.".into());
            }
            if let Some(pointer) = &existing {
                if pointer.surface != evidence.surface
                    || pointer.device != evidence.device
                    || pointer.session != evidence.session
                {
                    return Err("The pointer belongs to another device or surface.".into());
                }
            } else if interaction.phase != Phase::Begin {
                return Err("Begin a deliberate selection first.".into());
            }
            if interaction.phase == Phase::Begin
                && (existing.is_some() || self.pointers.len() >= 16)
            {
                return Err("Pointer already selected or capacity reached.".into());
            }
            if evidence.confidence < 0.8 || evidence.projected_content {
                return Err(
                    "Selection requires confident input independent of projected content.".into(),
                );
            }
            self.surface(&evidence.surface, workspace)?
                .camera_quad
                .map(interaction.camera_normalized, true)
        })();
        let local = match result {
            Ok(local) => local,
            Err(error) => {
                if existing
                    .as_ref()
                    .is_some_and(|p| p.device == evidence.device && p.session == evidence.session)
                {
                    self.pointers.remove(&interaction.pointer);
                }
                return Err(error);
            }
        };
        let connection = self.connections.get_mut(&evidence.device).unwrap();
        connection.sequence = evidence.sequence;
        connection.time_ms = evidence.captured_ms;
        self.pointers.remove(&interaction.pointer);
        match interaction.phase {
            Phase::Begin | Phase::Update => {
                self.pointers.insert(
                    interaction.pointer,
                    Pointer {
                        surface: evidence.surface.clone(),
                        device: evidence.device.clone(),
                        session: evidence.session.clone(),
                        calibration_revision: evidence.calibration_revision,
                        local,
                        updated_ms: now_ms,
                    },
                );
                Ok(None)
            }
            Phase::End => Ok(Some(Commit {
                pointer: interaction.pointer,
                surface: evidence.surface.clone(),
                local,
            })),
            Phase::Cancel => Ok(None),
        }
    }

    pub fn blank(&mut self, blanked: bool) {
        self.blanked = blanked;
        if blanked {
            self.pointers.clear();
        }
    }

    pub fn enqueue(&mut self, request: Request, workspace: u64, now_ms: u64) -> Result<(), String> {
        self.advance(now_ms)?;
        let surface = self.surface(&request.surface, workspace)?;
        if !valid_id(&request.id)
            || !valid_id(&request.user)
            || !request.target.valid()
            || !text(&request.prompt, 4096)
            || request.status != Status::Queued
            || request.created_ms != now_ms
            || request.updated_ms != now_ms
            || request.result.is_some()
            || request.calibration_revision != surface.calibration_revision
            || request.observations.len() > 8
            || self.requests.contains_key(&request.id)
            || request.observations.iter().collect::<BTreeSet<_>>().len()
                != request.observations.len()
            || request.observations.iter().any(|id| {
                !self.observations.get(id).is_some_and(|e| {
                    e.surface == request.surface && !e.projected_content && e.confidence >= 0.8
                })
            })
        {
            return Err("Invalid request, target or observation scope.".into());
        }
        if self.requests.len() >= MAX_REQUESTS {
            return Err("Request capacity reached; dismiss finished work first.".into());
        }
        self.requests.insert(request.id.clone(), request);
        Ok(())
    }

    pub fn dispatch(&mut self, workspace: u64, now_ms: u64) -> Result<Vec<Request>, String> {
        self.advance(now_ms)?;
        let mut available = MAX_RUNNING.saturating_sub(
            self.requests
                .values()
                .filter(|r| matches!(r.status, Status::Running | Status::Waiting))
                .count(),
        );
        let mut targets: BTreeSet<_> = self
            .requests
            .values()
            .filter(|r| matches!(r.status, Status::Running | Status::Waiting))
            .map(|r| (r.target.organ.clone(), r.target.record.clone()))
            .collect();
        let mut queued: Vec<_> = self
            .requests
            .values()
            .filter(|r| r.status == Status::Queued)
            .map(|r| (r.created_ms, r.id.clone()))
            .collect();
        queued.sort();
        let surfaces: BTreeSet<_> = self
            .configuration
            .surfaces
            .iter()
            .filter(|s| s.workspace == workspace)
            .map(|s| s.id.clone())
            .collect();
        let mut output = Vec::new();
        for (_, id) in queued {
            let request = self.requests.get_mut(&id).unwrap();
            if !surfaces.contains(&request.surface)
                || available == 0
                || targets.contains(&(request.target.organ.clone(), request.target.record.clone()))
            {
                continue;
            }
            if request
                .observations
                .iter()
                .any(|id| !self.observations.contains_key(id))
            {
                request.status = Status::Canceled;
                continue;
            }
            request.status = Status::Running;
            request.updated_ms = now_ms;
            targets.insert((request.target.organ.clone(), request.target.record.clone()));
            available -= 1;
            output.push(request.clone());
        }
        Ok(output)
    }

    pub fn finish(
        &mut self,
        id: &str,
        target: &Target,
        user: &str,
        result: String,
        now_ms: u64,
    ) -> Result<(), String> {
        self.advance(now_ms)?;
        let request = self.requests.get(id).ok_or("Unknown request.")?;
        let valid = matches!(request.status, Status::Running | Status::Waiting)
            && &request.target == target
            && request.user == user
            && text(&result, 8192)
            && self.configuration.surfaces.iter().any(|s| {
                s.id == request.surface && s.calibration_revision == request.calibration_revision
            });
        let request = self.requests.get_mut(id).unwrap();
        if !valid {
            if !request.terminal() {
                request.status = Status::Failed;
            }
            return Err("Result was canceled, stale, unauthorized or invalid.".into());
        }
        request.status = Status::Completed;
        request.updated_ms = now_ms;
        request.result = Some(result);
        Ok(())
    }

    pub fn cancel(&mut self, id: &str) -> Result<(), String> {
        let request = self.requests.get_mut(id).ok_or("Unknown request.")?;
        if !request.terminal() {
            request.status = Status::Canceled;
        }
        Ok(())
    }

    pub fn cancel_all(&mut self) {
        for request in self.requests.values_mut() {
            if !request.terminal() {
                request.status = Status::Canceled;
            }
        }
    }

    pub fn dismiss(&mut self, id: &str) -> Result<(), String> {
        if !self.requests.get(id).is_some_and(Request::terminal) {
            return Err("Only finished work can be dismissed.".into());
        }
        self.requests.remove(id);
        Ok(())
    }
}

impl Request {
    pub fn terminal(&self) -> bool {
        matches!(
            self.status,
            Status::Completed | Status::Canceled | Status::Failed
        )
    }
}

#[cfg(test)]
mod tests;
